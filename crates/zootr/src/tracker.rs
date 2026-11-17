//! Which of the seed's spots Link has checked: chests by their opened flag,
//! gifts by the played flag of their event, scrubs by their sold flag. See
//! `docs/research/chests.md`.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Result;
use ounce_formats::{Save, murmur3};
use serde::Serialize;

use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::gifts::GIFTS;
use crate::history::{Entry, Mark};
use crate::savefile::Change;
use crate::scrubs;

/// Struct whose members are named by a chest's placement hash in decimal
/// and hold its opened flag.
const CHESTS: u32 = 0xce0c4dfe;

/// Each chest's opened flag, a Bool known only by its hash, by the chest's
/// placement hash.
pub(crate) fn chest_flags(gd: &GameData) -> Result<impl Fn(u64) -> Option<u32>> {
    let chests: HashMap<u32, u32> = gd.members(CHESTS)?.into_iter().collect();
    Ok(move |hash: u64| chests.get(&murmur3(hash.to_string().as_bytes())).copied())
}

#[derive(Default)]
pub struct Tracker {
    /// Spot to the Bool that's set once it's checked.
    flags: HashMap<u64, u32>,
    /// Tracked flags that are set.
    on: HashSet<u32>,
    checked: BTreeSet<u64>,
}

/// Spots that changed since the last look.
#[derive(Debug, Default, PartialEq)]
pub struct Flips {
    pub checked: Vec<u64>,
    /// After loading an older save, say.
    pub unchecked: Vec<u64>,
}

/// One row of the Checks tab.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Row {
    pub spot: Spot,
    pub checked: bool,
    /// The spot's opened flag. Without one it never ticks.
    pub flag: Option<u32>,
    /// The user's label for the flag, which wins over the spot's names.
    pub label: Option<String>,
    pub mark: Option<Mark>,
}

impl Tracker {
    /// Spots with no known flag are left out, never checked.
    pub fn new(gd: &GameData, spots: &[Spot]) -> Result<Self> {
        let chests = chest_flags(gd)?;
        let flag = |spot: &Spot| {
            let name = match (
                GIFTS.iter().find(|g| g.hash() == spot.hash),
                scrubs::at(spot.hash),
            ) {
                (Some(g), _) => g.found(),
                (_, Some(s)) => s.sold(&spot.item),
                _ => return chests(spot.hash),
            };
            name.map(|f| murmur3(f.as_bytes()))
        };
        let flags = spots
            .iter()
            .filter_map(|s| Some((s.hash, flag(s)?)))
            .collect();
        Ok(Self {
            flags,
            ..Default::default()
        })
    }

    /// Every flag it watches.
    pub fn flags(&self) -> impl Iterator<Item = u32> + '_ {
        self.flags.values().copied()
    }

    /// The Bool that's set once spot `hash` is checked, if known.
    pub fn flag(&self, hash: u64) -> Option<u32> {
        self.flags.get(&hash).copied()
    }

    /// Takes every flag from `set`, a save or values read live.
    pub fn load(&mut self, set: impl Fn(u32) -> bool) -> Flips {
        self.on = self.flags.values().copied().filter(|&f| set(f)).collect();
        self.flips()
    }

    /// Takes one change the watcher saw.
    pub fn change(&mut self, c: &Change) -> Flips {
        if c.kind != "Bool" || !self.flags.values().any(|&f| f == c.hash) {
            return Flips::default();
        }
        if c.change.ends_with("-> true") {
            self.on.insert(c.hash);
        } else {
            self.on.remove(&c.hash);
        }
        self.flips()
    }

    fn flips(&mut self) -> Flips {
        let now: BTreeSet<u64> = self
            .flags
            .iter()
            .filter(|(_, f)| self.on.contains(f))
            .map(|(&s, _)| s)
            .collect();
        let flips = Flips {
            checked: now.difference(&self.checked).copied().collect(),
            unchecked: self.checked.difference(&now).copied().collect(),
        };
        self.checked = now;
        flips
    }

    /// The tab's rows for a seed's history entry.
    /// The Checks tab's rows, with `label` giving each flag's label.
    pub fn rows(&self, entry: &Entry, label: impl Fn(u32) -> Option<String>) -> Vec<Row> {
        let row = |spot: &Spot| {
            let flag = self.flags.get(&spot.hash).copied();
            Row {
                spot: spot.clone(),
                checked: self.checked.contains(&spot.hash),
                flag,
                label: flag.and_then(&label),
                mark: entry.marks.get(&spot.hash).cloned(),
            }
        };
        entry.report.spots.iter().map(row).collect()
    }

    pub fn checked(&self) -> &BTreeSet<u64> {
        &self.checked
    }
}

/// Flags set in a save.
pub fn in_save(save: &Save) -> impl Fn(u32) -> bool + '_ {
    |h| {
        save.get(h)
            .and_then(|e| save.scalar(e))
            .is_some_and(|v| v != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    /// Mido's House, front left to far right, and each chest's flag.
    const MIDO: [(u64, u32); 4] = [
        (4555665598794293658, 0xc8643846),
        (12786193094758345086, 0x467020fd),
        (10831981390482426294, 0x316d8a20),
        (9725242744547055868, 0xf870d00b),
    ];

    fn spot(hash: u64) -> Spot {
        Spot {
            hash,
            ..Default::default()
        }
    }

    fn mido() -> Tracker {
        Tracker {
            flags: MIDO.into_iter().collect(),
            ..Default::default()
        }
    }

    fn bool_(hash: u32, change: &str) -> Change {
        Change::new(&GameData::default(), hash, "Bool", change)
    }

    #[test]
    fn a_flip_is_reported_once() {
        let mut t = mido();
        let flips = t.change(&bool_(MIDO[1].1, "false -> true"));
        assert_eq!(flips.checked, [MIDO[1].0]);
        assert!(
            t.change(&bool_(MIDO[1].1, "false -> true"))
                .checked
                .is_empty()
        );
        assert_eq!(t.checked().iter().copied().collect::<Vec<_>>(), [MIDO[1].0]);
        let flips = t.change(&bool_(MIDO[1].1, "true -> false"));
        assert_eq!(flips.unchecked, [MIDO[1].0]);
        assert!(t.checked().is_empty());
    }

    #[test]
    fn other_changes_are_ignored() {
        let mut t = mido();
        assert_eq!(t.change(&bool_(1, "false -> true")), Flips::default());
        let int = Change::new(&GameData::default(), MIDO[0].1, "Int", "0 -> 1");
        assert_eq!(t.change(&int), Flips::default());
    }

    #[test]
    fn a_save_marks_its_set_flags() {
        let (on, off) = (MIDO[3].1, MIDO[0].1);
        let save = Save::parse(Save::fixture(&[(0, 0), (on, 1), (off, 0)])).unwrap();
        let mut t = mido();
        t.change(&bool_(off, "false -> true"));
        let flips = t.load(in_save(&save));
        assert_eq!(
            (flips.checked, flips.unchecked),
            (vec![MIDO[3].0], vec![MIDO[0].0])
        );
    }

    #[test]
    fn rows_carry_checked_flags_labels_and_marks() {
        let mut t = mido();
        t.change(&bool_(MIDO[2].1, "false -> true"));
        let mut history = crate::history::History::default();
        let report = crate::generate::Report {
            seed: 0,
            events: vec![],
            way: vec![],
            playthrough: vec![],
            unprovable: vec![],
            start: Default::default(),
            spots: vec![spot(MIDO[2].0), spot(1)],
            written: crate::generate::Written {
                defaults: vec![],
                actors: 0,
                files: 0,
            },
        };
        history.add(
            Default::default(),
            Default::default(),
            Default::default(),
            report,
        );
        history.mark(1, 1, Some(Mark::Skipped)).unwrap();
        let label = |f| (f == MIDO[2].1).then(|| "by the bed".to_owned());
        let rows = t.rows(history.get(1).unwrap(), label);
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.checked, r.flag, r.label.as_deref(), r.mark.clone()))
            .collect();
        assert_eq!(
            got,
            [
                (true, Some(MIDO[2].1), Some("by the bed"), None),
                (false, None, None, Some(Mark::Skipped))
            ]
        );
    }

    #[test]
    fn chests_and_gifts_map_to_flags() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let mut spots: Vec<Spot> = MIDO.iter().map(|&(h, _)| spot(h)).collect();
        spots.extend(GIFTS.iter().map(|g| spot(g.hash())));
        let t = Tracker::new(&gd, &spots).unwrap();
        for (h, f) in MIDO {
            assert_eq!(t.flags[&h], f);
        }
        for (g, found) in GIFTS.iter().filter_map(|g| Some((g, g.found()?))) {
            let f = t.flags[&g.hash()];
            assert_eq!(gd.table(&found), Some("Bool"), "{}", g.flow);
            assert!(gd.entry(f).is_some());
        }
    }

    #[test]
    fn scrubs_map_to_their_sold_flags() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        for s in &scrubs::SCRUBS {
            for item in [s.item.to_string(), crate::foreign::actor(1)] {
                let spot = Spot {
                    item: item.clone(),
                    ..spot(s.hash())
                };
                let t = Tracker::new(&gd, &[spot]).unwrap();
                let sold = s.sold(&item).unwrap();
                assert_eq!(t.flag(s.hash()), Some(murmur3(sold.as_bytes())), "{sold}");
                if !sold.starts_with("Rando_") {
                    assert_eq!(gd.table(&sold), Some("Bool"), "{sold}");
                }
            }
        }
    }
}
