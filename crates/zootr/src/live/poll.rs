//! A running game polled for what changed, with the mute for values that
//! change too fast to read.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use anyhow::Result;

use super::{BURST, Game, Memory, Value, WINDOW, changes, keys_of, manager, values};
use crate::gamedata::GameData;
use crate::savefile::{self, Change};

/// A running game, polled for what changed since the last look.
pub struct Poll<M> {
    mem: M,
    global: u64,
    was: Vec<Value>,
    keys: BTreeSet<u64>,
    noise: Noise,
    muted: HashSet<u32>,
}

impl Poll<Game> {
    pub fn new(game: Game) -> Result<Self> {
        let global = game.global;
        Self::over(game, global)
    }
}

impl<M: Memory> Poll<M> {
    pub fn over(mem: M, global: u64) -> Result<Self> {
        let manager = manager(&mem, global)?;
        Ok(Self {
            was: values(&mem, manager)?,
            keys: keys_of(&mem, manager)?,
            mem,
            global,
            noise: Noise::default(),
            muted: HashSet::new(),
        })
    }

    /// What changed since the last call among values `wanted` keeps, and
    /// the values muted for the first time. `all` mutes nothing.
    pub fn next(
        &mut self,
        gd: &GameData,
        all: bool,
        mut wanted: impl FnMut(u32) -> bool,
    ) -> Result<(Vec<Change>, Vec<u32>)> {
        let manager = manager(&self.mem, self.global)?;
        let now = values(&self.mem, manager)?;
        let keys = keys_of(&self.mem, manager)?;
        let (at, mut muted) = (Instant::now(), Vec::new());
        let keep = |h| {
            if !wanted(h) {
                return false;
            }
            if all || self.noise.allows(h, at) {
                return true;
            }
            if self.muted.insert(h) {
                muted.push(h);
            }
            false
        };
        let mut out = changes(gd, &self.was, &now, keep);
        out.extend(savefile::keyed(&self.keys, &keys));
        (self.was, self.keys) = (now, keys);
        Ok((out, muted))
    }
}

/// Mutes values that change too often to read, like Link's position and
/// the play clock. Once muted, a value stays muted.
#[derive(Default)]
pub struct Noise(HashMap<u32, (Instant, u32)>);

impl Noise {
    /// Whether a change to `hash` at `now` should still print.
    pub fn allows(&mut self, hash: u32, now: Instant) -> bool {
        let (start, n) = self.0.entry(hash).or_insert((now, 0));
        if *n <= BURST && now - *start > WINDOW {
            (*start, *n) = (now, 0);
        }
        *n += 1;
        *n <= BURST
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ounce_formats::KEYED;

    use super::super::keys;
    use super::super::tests::{Flip, hash, ints, keyed};
    use super::*;

    #[test]
    fn a_poll_reports_a_value_muted_once() {
        let gd = GameData::default();
        let flip = Flip(vec![ints([5, 1]), ints([5, 2])], 0.into());
        let mut poll = Poll::over(flip, 0x1000).unwrap();
        let mut seen = Vec::new();
        for _ in 0..8 {
            let (changes, muted) = poll.next(&gd, false, |_| true).unwrap();
            seen.push((changes.len(), muted));
        }
        let mut expected = vec![(1, vec![]); 5];
        expected.extend([(0, vec![hash(2, 1)]), (0, vec![]), (0, vec![])]);
        assert_eq!(seen, expected);
        assert_eq!(poll.next(&gd, true, |_| true).unwrap().0.len(), 1);
    }

    #[test]
    fn a_poll_reports_each_placement_flag_that_flips() {
        let gd = GameData::default();
        let at = |a, b| keyed(&[], &[], &[(1 << 63, a), (7, b)]);
        let flip = Flip(vec![at(false, true), at(true, false)], 0.into());
        assert_eq!(keys(&flip.0[0], 0x1000).unwrap(), BTreeSet::from([7]));
        let mut poll = Poll::over(flip, 0x1000).unwrap();
        let (changes, _) = poll.next(&gd, false, |_| true).unwrap();
        // A flip lands right after the manager's read, so the first poll
        // reads the second game and the next one the first.
        let shown: Vec<_> = (changes.iter())
            .map(|c| (c.key(), c.kind.as_str(), c.change.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                (Some(7), KEYED, "false -> true"),
                (Some(1 << 63), KEYED, "true -> false"),
            ]
        );
    }

    #[test]
    fn noise_mutes_a_burst_for_good() {
        let (mut noise, start) = (Noise::default(), Instant::now());
        let at = |ms| start + Duration::from_millis(ms);
        let slow: Vec<bool> = (0..8).map(|k| noise.allows(1, at(k * 4000))).collect();
        assert!(slow.iter().all(|&q| q));
        let fast: Vec<bool> = (0..7).map(|k| noise.allows(2, at(k * 100))).collect();
        assert_eq!(fast, [true, true, true, true, true, false, false]);
        assert!(!noise.allows(2, at(60_000)));
    }
}
