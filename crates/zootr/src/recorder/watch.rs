//! [`Watch`]: starts a recording whenever the game runs, live reads when
//! it can attach, autosave diffs when the build can't be read.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;

use super::{Event, Hub, LiveStatus, Pick, Recorder, WATCH, why};
use crate::config::Target;
use crate::gamedata::GameData;
use crate::live::{self, Game, NotSigned, Poll, Unsupported};

/// What [`Watch`] does with an attach result.
#[derive(Debug)]
pub enum Next<G> {
    Record(Pick<G>, Option<String>),
    /// Can't record until the game starts again.
    Wait(String),
    /// Try again in a moment: the game may still be setting up.
    Retry(String),
}

/// Live reads if attaching worked. Only a build that isn't signed, or a
/// target off macOS without a bridge, can't attach at all, so that falls back to autosaves, and
/// anything else is retried.
pub fn decide<G>(attach: Result<G>, saves: Option<PathBuf>) -> Next<G> {
    match attach {
        Ok(g) => Next::Record(Pick::Live(g), None),
        Err(e) if e.is::<NotSigned>() || e.is::<Unsupported>() => match saves {
            Some(dir) => Next::Record(Pick::Saves(dir), Some(why(&e))),
            None => Next::Wait(why(&e)),
        },
        Err(e) => Next::Retry(why(&e)),
    }
}

/// `target`'s save folder, if it's there.
fn saves(target: &Target) -> Option<PathBuf> {
    target.save_dir.clone().filter(|d| d.is_dir())
}

/// What [`Watch`] is doing.
#[derive(Debug, PartialEq)]
enum Phase {
    Waiting,
    /// Can't read the game until it starts again.
    Blocked(String),
    Attaching(String),
    Recording,
}

impl Phase {
    /// One look at the game. Returns the session to start, if any.
    fn tick<G>(
        &mut self,
        running: bool,
        ended: bool,
        attach: impl FnOnce() -> Next<G>,
    ) -> Option<(Pick<G>, Option<String>)> {
        match self {
            Phase::Recording if !ended => {}
            Phase::Recording => *self = Phase::Waiting,
            _ if !running => *self = Phase::Waiting,
            Phase::Blocked(_) => {}
            Phase::Waiting | Phase::Attaching(_) => match attach() {
                Next::Record(pick, reason) => {
                    *self = Phase::Recording;
                    return Some((pick, reason));
                }
                Next::Wait(why) => *self = Phase::Blocked(why),
                Next::Retry(why) => *self = Phase::Attaching(why),
            },
        }
        None
    }

    /// A session sends its own status when it starts.
    fn status(&self) -> Option<LiveStatus> {
        match self {
            Phase::Waiting => Some(LiveStatus::Waiting { reason: None }),
            Phase::Blocked(why) => Some(LiveStatus::Waiting {
                reason: Some(why.clone()),
            }),
            Phase::Attaching(why) => Some(LiveStatus::Attaching {
                reason: why.clone(),
            }),
            Phase::Recording => None,
        }
    }
}

/// Records the target whenever it runs. Dropping it ends any session.
pub struct Watch {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Watches the target `target` names, looked up on every pass so a new
    /// pick takes effect. `gd` loads the GameData names first.
    pub fn start(
        hub: Arc<Hub>,
        gd: impl FnOnce() -> Result<Arc<GameData>> + Send + 'static,
        target: impl Fn() -> Result<Target> + Send + 'static,
        logs: PathBuf,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let waiting = |e: anyhow::Error| Event::Status {
                status: LiveStatus::Waiting {
                    reason: Some(format!("{e:#}")),
                },
            };
            let gd = match gd() {
                Ok(gd) => gd,
                Err(e) => return hub.send(waiting(e)),
            };
            let (mut phase, mut rec) = (Phase::Waiting, None::<Recorder>);
            while !flag.load(Ordering::Relaxed) {
                match target() {
                    Ok(t) => {
                        let ended = rec.as_ref().is_some_and(Recorder::finished);
                        let attach = || decide(Game::attach(&t).and_then(Poll::new), saves(&t));
                        if let Some((pick, reason)) = phase.tick(t.running(), ended, attach) {
                            let (send, asks) = (hub.sender(), hub.asks());
                            rec = Some(Recorder::run(
                                pick,
                                reason,
                                &t,
                                gd.clone(),
                                &logs,
                                send,
                                asks,
                            ));
                        }
                        if let Some(status) = phase.status() {
                            hub.send(Event::Status { status });
                        }
                    }
                    Err(e) => hub.send(waiting(e)),
                }
                let mut slept = Duration::ZERO;
                while slept < WATCH && !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(live::TICK);
                    slept += live::TICK;
                }
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::flag;

    fn unsigned() -> Result<u8> {
        Err(NotSigned { pid: 1 }.into())
    }

    #[test]
    fn only_an_unsigned_or_unsupported_build_gives_up_on_live_reads() {
        let dir = PathBuf::from("saves");
        assert!(matches!(
            decide(Ok(1), None),
            Next::Record(Pick::Live(1), None)
        ));
        let saves = decide(unsigned(), Some(dir.clone()));
        assert!(matches!(saves, Next::Record(Pick::Saves(d), Some(_)) if d == dir));
        assert!(matches!(decide(unsigned(), None), Next::Wait(_)));
        let elsewhere = decide::<u8>(Err(Unsupported.into()), Some(dir.clone()));
        assert!(
            matches!(elsewhere, Next::Record(Pick::Saves(_), Some(why)) if why == "live reads need macOS, or a target with a bridge")
        );
        let booting = decide::<u8>(
            Err(anyhow::anyhow!("no {}\nyet", flag::GAME_DATA)),
            Some(dir),
        );
        assert!(matches!(booting, Next::Retry(why) if why == format!("no {}", flag::GAME_DATA)));
    }

    #[test]
    fn the_watch_attaches_when_the_game_runs_and_waits_when_it_closes() {
        let mut phase = Phase::Waiting;
        let never = || -> Next<u8> { panic!("shouldn't attach") };
        assert!(phase.tick(false, false, never).is_none());
        let retry = || Next::<u8>::Retry("booting".into());
        assert!(phase.tick(true, false, retry).is_none());
        assert_eq!(phase, Phase::Attaching("booting".into()));
        let live = || Next::Record(Pick::Live(1), None);
        assert!(matches!(
            phase.tick(true, false, live),
            Some((Pick::Live(1), None))
        ));
        assert!(phase.tick(true, false, never).is_none() && phase.status().is_none());
        phase.tick(false, true, never);
        assert_eq!(phase.status(), Some(LiveStatus::Waiting { reason: None }));
        let attaching = Phase::Attaching("booting".into());
        let mut closed = attaching;
        closed.tick(false, false, never);
        assert_eq!(closed, Phase::Waiting);
    }

    #[test]
    fn the_watch_holds_off_an_unsigned_build_until_the_game_restarts() {
        let mut phase = Phase::Waiting;
        phase.tick(true, false, || Next::<u8>::Wait("not signed".into()));
        let reason = Some("not signed".into());
        assert_eq!(phase.status(), Some(LiveStatus::Waiting { reason }));
        phase.tick(true, false, || -> Next<u8> { panic!("no retry") });
        phase.tick(false, false, || -> Next<u8> { panic!("not running") });
        assert_eq!(phase, Phase::Waiting);
    }
}
