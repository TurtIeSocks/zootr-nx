//! A sweep that steps aside: between spots it asks the turn queue whether
//! someone waits for its game, and if so it keeps its place in
//! `resume.json` and stops. Started again, it finishes that round on the
//! same seed, skipping the spots already done.

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::turn::{Ask, TurnConfig};

/// How long a sweep keeps asking a queue that doesn't answer before it
/// sweeps anyway: past a restart of the service, which cuts the wait
/// short. A wait that ended that way doesn't mean the game is free.
const PATIENCE: Duration = Duration::from_secs(10 * 60);
/// Between those asks.
const RETRY: Duration = Duration::from_secs(10);

/// Where a yielded round stopped.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    pub round: usize,
    pub seed: u64,
    /// The spots swept before it stopped, by name.
    pub done: Vec<String>,
}

impl Resume {
    fn path(dir: &Path) -> PathBuf {
        dir.join("resume.json")
    }

    /// The round a sweep in `dir` stopped in, if one did.
    pub fn load(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::write(Self::path(dir), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// The round finished: nothing to resume.
    pub fn clear(dir: &Path) {
        let _ = std::fs::remove_file(Self::path(dir));
    }

    /// Drop each of `planned` whose `name` `done` holds, as many times as
    /// it holds it (two spots can share a name).
    pub fn skip<T>(&self, planned: &mut Vec<T>, name: impl Fn(&T) -> String) {
        let mut done = self.done.clone();
        planned.retain(|p| {
            let named = name(p);
            match done.iter().position(|d| *d == named) {
                Some(i) => {
                    done.swap_remove(i);
                    false
                }
                None => true,
            }
        });
    }
}

/// The sweep's place in the turn queue: sweep `name` on game `game`.
pub struct Yielding {
    pub turn: TurnConfig,
    pub game: String,
    pub name: String,
}

impl Yielding {
    /// Wait until no one holds or waits for the game, then sweep on it.
    /// A queue that stops answering is asked again for [`PATIENCE`], and
    /// only then doesn't stop the sweep.
    pub fn start(&self) {
        eprintln!("waiting for {} to be free", self.game);
        let ask = || {
            let game = self.game.clone();
            self.turn.ask(Ask::SweepStart {
                game,
                name: self.name.clone(),
            })
        };
        if let Err(e) = patiently(ask, PATIENCE, RETRY) {
            eprintln!("sweeping anyway: {e:#}");
        }
    }

    /// Whether someone wants the game: step aside now.
    pub fn asked(&self) -> bool {
        let ask = Ask::SweepYields {
            game: self.game.clone(),
            name: self.name.clone(),
        };
        match self.turn.ask(ask) {
            Ok(answer) => answer.yes == Some(true),
            Err(e) => {
                eprintln!("sweeping on: {e:#}");
                false
            }
        }
    }

    /// The sweep stopped: the game is free for whoever waits.
    pub fn done(&self) {
        let ask = Ask::SweepDone {
            game: self.game.clone(),
            name: self.name.clone(),
        };
        if let Err(e) = self.turn.ask(ask) {
            eprintln!("telling the queue the sweep stopped: {e:#}");
        }
    }
}

/// `ask` until it answers, every `retry`, for at most `patience`: its
/// answer, or its last error.
fn patiently<T>(
    mut ask: impl FnMut() -> Result<T>,
    patience: Duration,
    retry: Duration,
) -> Result<T> {
    let until = Instant::now() + patience;
    loop {
        match ask() {
            Err(e) if Instant::now() < until => {
                eprintln!("the turn queue didn't answer, asking again: {e:#}");
                sleep(retry);
            }
            answer => return answer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_cut_short_is_asked_again_and_a_queue_down_for_good_gives_up() {
        let ms = Duration::from_millis;
        let mut asks = 0;
        let answer = patiently(
            || {
                asks += 1;
                anyhow::ensure!(asks > 2, "restarting");
                Ok(asks)
            },
            ms(500),
            ms(1),
        );
        assert_eq!(answer.unwrap(), 3);
        let down = patiently(|| -> Result<()> { anyhow::bail!("down") }, ms(20), ms(5));
        assert_eq!(down.unwrap_err().to_string(), "down");
    }

    #[test]
    fn skip_drops_each_done_spot_once_and_keeps_the_order() {
        let resume = Resume {
            done: vec!["b".into(), "a".into()],
            ..Resume::default()
        };
        let mut planned = vec!["a", "a", "b", "c"];
        resume.skip(&mut planned, |p| p.to_string());
        assert_eq!(planned, ["a", "c"]);
    }

    #[test]
    fn a_saved_round_loads_back_and_clears() {
        let dir = std::env::temp_dir().join(format!("zootr-test-{}-resume", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Resume::load(&dir), None);
        let resume = Resume {
            round: 7,
            seed: 412851,
            done: vec!["a".into()],
        };
        resume.save(&dir).unwrap();
        assert_eq!(Resume::load(&dir), Some(resume));
        Resume::clear(&dir);
        assert_eq!(Resume::load(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
