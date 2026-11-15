//! What a run watches: the lines `until` and `expect` wait on, and the
//! stall check.

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::dev::Goal;
use crate::gamedata::GameData;
use crate::live::{self, Memory};

/// How a wait for some lines ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Wait {
    Held,
    /// The deadline passed, with the lines as they stood.
    TimedOut(Vec<Goal>),
    /// A line couldn't be read.
    Failed(Goal),
    /// The watch stopped the wait, on a stall.
    Stopped,
}

/// Each `NAME=VALUE` line as the game holds it now.
pub fn read(mem: &impl Memory, gd: &GameData, global: u64, lines: &[String]) -> Vec<Goal> {
    let read = |h| live::read(mem, gd, global, h);
    lines.iter().map(|l| Goal::check(l, read)).collect()
}

/// Look at `lines` every `tick` until they all hold, one can't be read or
/// `within` passes. `watch` runs before each look and stops the wait when
/// it says so.
pub fn wait(
    mem: &impl Memory,
    gd: &GameData,
    global: u64,
    lines: &[String],
    (within, tick): (Duration, Duration),
    mut watch: impl FnMut() -> Result<bool>,
) -> Result<Wait> {
    let start = Instant::now();
    loop {
        if !watch()? {
            return Ok(Wait::Stopped);
        }
        let goals = read(mem, gd, global, lines);
        if let Some(bad) = goals.iter().find(|g| g.error.is_some()) {
            return Ok(Wait::Failed(bad.clone()));
        }
        if goals.iter().all(|g| g.met) {
            return Ok(Wait::Held);
        }
        if start.elapsed() >= within {
            return Ok(Wait::TimedOut(goals));
        }
        std::thread::sleep(tick);
    }
}

/// An event that plays while nothing changes for `limit` is stuck. A map
/// loading never is.
#[derive(Debug)]
pub struct Stall {
    limit: Duration,
    /// When the quiet began.
    since: Option<Duration>,
}

impl Stall {
    pub fn new(limit: Duration) -> Self {
        Self { limit, since: None }
    }

    /// Whether the game has stalled by `at`, given what it's doing and
    /// whether anything changed since the last tick.
    pub fn tick(&mut self, at: Duration, event: bool, loading: bool, changed: bool) -> bool {
        if !event || loading || changed {
            self.since = None;
            return false;
        }
        at - *self.since.get_or_insert(at) >= self.limit
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use anyhow::bail;

    use super::*;
    use crate::live::tests::{Fake, hash, ints};

    /// Memory that moves to the next game on every look, and stays on
    /// the last.
    struct Later(Vec<Fake>, Cell<usize>);

    impl Memory for Later {
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            // The manager pointer is the first read of a look.
            if at == 0x1000 {
                self.1.set(self.1.get() + 1);
            }
            let n = self.1.get().saturating_sub(1);
            self.0[n.min(self.0.len() - 1)].read(at, len)
        }

        fn write(&self, _: u64, _: &[u8]) -> Result<()> {
            bail!("read only")
        }
    }

    const GLOBAL: u64 = 0x1000;

    fn line(value: u32) -> Vec<String> {
        vec![format!("{:#x}={value}", hash(2, 1))]
    }

    fn quick(within: u64) -> (Duration, Duration) {
        (Duration::from_millis(within), Duration::ZERO)
    }

    #[test]
    fn until_passes_once_the_value_arrives() {
        let gd = GameData::default();
        let mem = Later(vec![ints([5, 1]), ints([5, 1]), ints([5, 7])], 0.into());
        let mut looks = 0;
        let watch = || {
            looks += 1;
            Ok(true)
        };
        let waited = wait(&mem, &gd, GLOBAL, &line(7), quick(5000), watch).unwrap();
        assert_eq!(waited, Wait::Held);
        assert_eq!(looks, 3);
    }

    #[test]
    fn expect_times_out_with_the_values_it_saw() {
        let gd = GameData::default();
        let mut lines = line(8);
        lines.push(format!("{:#x}=5", hash(2, 0)));
        let waited = wait(&ints([5, 7]), &gd, GLOBAL, &lines, quick(0), || Ok(true));
        let Wait::TimedOut(goals) = waited.unwrap() else {
            panic!("no timeout")
        };
        let seen: Vec<_> = goals.iter().map(|g| (g.now.as_deref(), g.met)).collect();
        assert_eq!(seen, [(Some("7"), false), (Some("5"), true)]);
    }

    #[test]
    fn a_line_the_game_cant_read_fails_at_once() {
        let gd = GameData::default();
        let lines = vec!["0xdead=1".to_string()];
        let waited = wait(&ints([5, 7]), &gd, GLOBAL, &lines, quick(60_000), || {
            Ok(true)
        });
        let Wait::Failed(goal) = waited.unwrap() else {
            panic!("no failure")
        };
        assert!(goal.error.unwrap().contains("isn't a value"));
        let stopped = wait(&ints([5, 7]), &gd, GLOBAL, &line(8), quick(60_000), || {
            Ok(false)
        });
        assert_eq!(stopped.unwrap(), Wait::Stopped);
    }

    /// A stream of ticks a second apart: (event, loading, changed).
    fn stalls(stream: &[(bool, bool, bool)]) -> Vec<u64> {
        let mut stall = Stall::new(Duration::from_secs(15));
        let at = |i: usize| Duration::from_secs(i as u64);
        (stream.iter().enumerate())
            .filter(|&(i, &(e, l, c))| stall.tick(at(i), e, l, c))
            .map(|(i, _)| i as u64)
            .collect()
    }

    #[test]
    fn an_event_that_stops_changing_stalls_and_a_load_never_does() {
        let mut frozen = vec![(true, false, true); 5];
        frozen.extend([(true, false, false); 20]);
        assert_eq!(stalls(&frozen)[0], 20);
        let loading = vec![(true, true, false); 40];
        assert!(stalls(&loading).is_empty());
        let idle = vec![(false, false, false); 40];
        assert!(stalls(&idle).is_empty());
        let mut busy = vec![(true, false, false); 10];
        busy.push((true, false, true));
        busy.extend([(true, false, false); 10]);
        busy.push((true, true, false));
        busy.extend([(true, false, false); 10]);
        assert!(stalls(&busy).is_empty());
    }
}
