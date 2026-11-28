//! How many Heart Pieces and Heart Containers Link has collected, counted
//! from the checks. The save can't say: its Heart Piece count wraps at 4,
//! adding a heart (Mac, 2026-11-27: 1, 2, 3, then 0 with max life 12 to
//! 16), and no pouch entry counts containers.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::{Romfs, murmur3};

use crate::gamedata::GameData;
use crate::generate::{Held, Spot};
use crate::known::{Name, actor};
use crate::savefile::Change;
use crate::tracker::{Flag, Tracker};

/// What the counts are of.
pub const ITEMS: [&Name; 2] = [actor::OBJ_HEART_PIECE, actor::OBJ_HEART_CONTAINER];

/// Hearts a new game has.
const BASE: i32 = 3;
/// Heart Pieces to a heart.
const PER_HEART: i32 = 4;
/// Life in quarters of a heart.
const MAX_LIFE: &str = "PlayerStatus.MaxLife";

/// Every check that holds an [`ITEMS`] item in a vanilla game, as a spot.
/// Built from the dump alone, so once per dump does for every seed.
pub fn vanilla(romfs: &Romfs) -> Result<Vec<Spot>> {
    crate::ledger::holding(romfs, &ITEMS)
}

/// The checks that give hearts in one seed, and which are done.
pub struct Hearts {
    tracker: Tracker,
    /// What each check gives, by hash.
    gives: HashMap<u64, &'static Name>,
    /// What the seed starts Link with.
    start: HashMap<&'static Name, i32>,
    /// Max life in quarters, to check the counts against.
    max_life: Option<i32>,
}

impl Hearts {
    /// The checks a seed whose spots are `dealt` deals a heart, and the
    /// `vanilla` heart checks it doesn't deal, plus what it starts Link
    /// with. A starting item's copy left the pool, so nothing counts twice.
    pub fn new(gd: &GameData, vanilla: &[Spot], dealt: &[Spot], start: &[Held]) -> Result<Self> {
        let item = |s: &Spot| ITEMS.into_iter().find(|i| s.item == i.get());
        let undealt = vanilla
            .iter()
            .filter(|v| !dealt.iter().any(|s| s.hash == v.hash));
        let spots: Vec<Spot> = (dealt.iter())
            .filter(|s| item(s).is_some())
            .chain(undealt)
            .cloned()
            .collect();
        let gives = spots
            .iter()
            .filter_map(|s| Some((s.hash, item(s)?)))
            .collect();
        let start = (start.iter())
            .filter_map(|h| {
                Some((
                    ITEMS.into_iter().find(|i| h.item == i.get())?,
                    h.count as i32,
                ))
            })
            .collect();
        Ok(Self {
            tracker: Tracker::new(gd, &spots)?,
            gives,
            start,
            max_life: None,
        })
    }

    /// Takes every flag and the max life from a save or values read live.
    pub fn load(&mut self, flag: impl Fn(Flag) -> bool, value: impl Fn(u32) -> Option<u32>) {
        self.tracker.load(flag);
        self.max_life = value(murmur3(MAX_LIFE.as_bytes())).map(|v| v as i32);
        self.check();
    }

    /// Takes one change the watcher saw. True when a count changed.
    pub fn change(&mut self, c: &Change) -> bool {
        if c.hash == murmur3(MAX_LIFE.as_bytes()) {
            self.max_life = (c.change.rsplit_once("-> ")).and_then(|(_, n)| n.parse().ok());
            self.check();
            return false;
        }
        let flips = self.tracker.change(c);
        let changed = !flips.checked.is_empty() || !flips.unchecked.is_empty();
        if changed {
            self.check();
        }
        changed
    }

    /// Each [`ITEMS`] item and how many Link has.
    pub fn counts(&self) -> [(&'static Name, i32); 2] {
        ITEMS.map(|i| (i, self.count(i)))
    }

    /// How many of `item` Link has collected, starting ones included.
    pub fn count(&self, item: &Name) -> i32 {
        let done = self.tracker.checked().iter();
        let found = done
            .filter(|h| self.gives.get(h).is_some_and(|&g| g == item))
            .count();
        self.start.get(item).copied().unwrap_or(0) + found as i32
    }

    /// Logs when the counts don't add up to the max life: a check's flag
    /// is missing or wrong. The counts still show.
    fn check(&self) {
        let Some(life) = self.max_life else { return };
        let [pieces, containers] = ITEMS.map(|i| self.count(i));
        let hearts = BASE + containers + pieces / PER_HEART;
        if life / 4 != hearts {
            log::debug!(
                "hearts: max life holds {} but the checks give {hearts} ({containers} containers, {pieces} pieces)",
                life / 4
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    #[test]
    fn every_vanilla_heart_check_has_a_flag() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let spots = vanilla(&romfs).unwrap();
        let t = Tracker::new(&gd, &spots).unwrap();
        for s in &spots {
            assert!(
                t.flag(s.hash).is_some(),
                "{} {} {}",
                s.hash,
                s.area,
                s.chest_name
            );
        }
        let of = |i: &Name| spots.iter().filter(|s| s.item == i.get()).count();
        assert_eq!(ITEMS.map(of), [36, 8]);
    }

    #[test]
    fn a_seed_counts_what_it_deals_where_and_what_it_starts_with() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let vanilla = vanilla(&romfs).unwrap();
        let first = |i: &Name| vanilla.iter().find(|s| s.item == i.get()).unwrap();
        let [piece, container] = ITEMS.map(first);
        // The seed deals the piece's check a rupee and the container's a
        // piece, and starts Link with two containers.
        let dealt = [
            Spot {
                item: actor::OBJ_RUPEE_GREEN.to_string(),
                ..piece.clone()
            },
            Spot {
                item: actor::OBJ_HEART_PIECE.to_string(),
                ..container.clone()
            },
        ];
        let start = [Held {
            item: actor::OBJ_HEART_CONTAINER.to_string(),
            name: "Heart Container".into(),
            count: 2,
            from: vec![],
            via: None,
        }];
        let mut hearts = Hearts::new(&gd, &vanilla, &dealt, &start).unwrap();
        assert_eq!(ITEMS.map(|i| hearts.count(i)), [0, 2]);
        hearts.load(|_| true, |_| None);
        assert_eq!(ITEMS.map(|i| hearts.count(i)), [36 - 1 + 1, 8 - 1 + 2]);
    }
}
