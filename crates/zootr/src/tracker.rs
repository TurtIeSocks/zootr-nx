//! Which of the seed's spots Link has checked: chests by their opened flag,
//! gifts by the played flag of their event, scrubs by their sold flag, cows
//! and Gerudo thieves by their own, and Gold Skulltulas, heart pieces and
//! the like by their per-placement flag (Bool64bitKey) that never resets.
//! Pots, rocks, boxes, hives and Gold Skulltulas dealt anything but their
//! own token by the seed's Bool the hook sets once their drop is picked up
//! (`picked`). See `docs/research/chests.md` and
//! `docs/research/tracker-flags.md`.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Result;
use ounce_formats::{Save, murmur3};
use serde::Serialize;

use crate::entrances::{DUNGEONS, reveal};
use crate::gamedata::GameData;
use crate::generate::{Door, Spot};
use crate::gifts::GIFTS;
use crate::history::{Entry, Mark};
use crate::savefile::Change;
use crate::{cows, fortress, goron_pot, keys, picked, scrubs};

/// Struct whose members are named by a chest's placement hash in decimal
/// and hold its opened flag.
const CHESTS: u32 = 0xce0c4dfe;

/// Each chest's opened flag, a Bool known only by its hash, by the chest's
/// placement hash.
pub(crate) fn chest_flags(gd: &GameData) -> Result<impl Fn(u64) -> Option<u32>> {
    let chests: HashMap<u32, u32> = gd.members(CHESTS)?.into_iter().collect();
    Ok(move |hash: u64| chests.get(&murmur3(hash.to_string().as_bytes())).copied())
}

/// What's set once a spot is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flag {
    /// A Bool, by its hash.
    Bool(u32),
    /// A per-placement flag, by its key: the placement's hash.
    Key(u64),
}

impl std::fmt::Display for Flag {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Bool(hash) => write!(f, "#{hash:08x}"),
            Self::Key(hash) => write!(f, "key {hash}"),
        }
    }
}

#[derive(Default)]
pub struct Tracker {
    /// Spot to what's set once it's checked.
    flags: HashMap<u64, Flag>,
    /// Each spot's stand-ins' taken Bools, which check it too, set without
    /// the hook (`pickups::taken`).
    taken: Vec<(u64, Flag)>,
    /// Tracked flags that are set.
    on: HashSet<Flag>,
    checked: BTreeSet<u64>,
    /// Each shuffled door, by id, and the Bool set once Link has been
    /// through it.
    doors: Vec<(String, u32)>,
    /// Doors Link has been through.
    revealed: BTreeSet<String>,
}

/// Spots that changed since the last look.
#[derive(Debug, Default, PartialEq)]
pub struct Flips {
    pub checked: Vec<u64>,
    /// After loading an older save, say.
    pub unchecked: Vec<u64>,
    /// Whether a door was revealed, or hidden again.
    pub doors: bool,
}

/// One shuffled door on the Checks tab.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DoorRow {
    /// The door, by `--door`'s id: the dungeon behind it in vanilla. The
    /// doors' ids are also every dungeon a door can lead to.
    pub id: String,
    pub name: String,
    /// The dungeon behind it in vanilla, by name, labelling `id`.
    pub vanilla: String,
    /// The dungeon behind it by id, once Link has been through.
    pub found: Option<String>,
    /// The player's guess, by id, until the door is revealed.
    pub pick: Option<String>,
}

/// One row of the Checks tab.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Row {
    pub spot: Spot,
    pub checked: bool,
    /// The spot's Bool, which the tab can label. A spot ticked by its
    /// per-placement flag has none.
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
                cows::flag_at(spot.hash).or_else(|| goron_pot::flag_at(spot.hash)),
                fortress::got(&spot.vanilla, &spot.item),
                picked::flag_at(spot),
                keys::loose_taken(spot.hash),
            ) {
                (Some(g), ..) => g.found(),
                (_, Some(s), ..) => s.sold(&spot.item),
                (_, _, Some(cow), ..) => Some(cow),
                (.., Some(thief), _, _) => Some(thief),
                (.., Some(picked), _) => Some(picked),
                (.., Some(loose)) => Some(loose.to_owned()),
                _ => {
                    let key = gd.has_key(spot.hash).then_some(Flag::Key(spot.hash));
                    return chests(spot.hash).map(Flag::Bool).or(key);
                }
            };
            name.map(|f| Flag::Bool(murmur3(f.as_bytes())))
        };
        let flags = spots
            .iter()
            .filter_map(|s| Some((s.hash, flag(s)?)))
            .collect();
        let taken = (spots.iter())
            .flat_map(|s| {
                s.taken
                    .iter()
                    .map(|t| (s.hash, Flag::Bool(murmur3(t.as_bytes()))))
            })
            .collect();
        Ok(Self {
            flags,
            taken,
            ..Default::default()
        })
    }

    /// Watches `doors`, each revealed by its dungeon's arrival Bool.
    pub fn with_doors(mut self, doors: &[Door]) -> Self {
        self.doors = (doors.iter())
            .filter_map(|d| {
                let i = DUNGEONS.iter().position(|x| x.id == d.dungeon)?;
                Some((d.id.clone(), murmur3(reveal::flag(i).as_bytes())))
            })
            .collect();
        self
    }

    /// Every flag it watches.
    pub fn flags(&self) -> impl Iterator<Item = Flag> + '_ {
        let doors = self.doors.iter().map(|(_, f)| Flag::Bool(*f));
        let taken = self.taken.iter().map(|(_, f)| *f);
        self.flags.values().copied().chain(taken).chain(doors)
    }

    /// What's set once spot `hash` is checked, if known.
    pub fn flag(&self, hash: u64) -> Option<Flag> {
        self.flags.get(&hash).copied()
    }

    /// Takes every flag from `set`, a save or values read live.
    pub fn load(&mut self, set: impl Fn(Flag) -> bool) -> Flips {
        self.on = self.flags().filter(|&f| set(f)).collect();
        self.flips()
    }

    /// Takes one change the watcher saw.
    pub fn change(&mut self, c: &Change) -> Flips {
        let flag = match c.key() {
            Some(key) => Flag::Key(key),
            None if c.kind == "Bool" => Flag::Bool(c.hash),
            None => return Flips::default(),
        };
        if !self.flags().any(|f| f == flag) {
            return Flips::default();
        }
        if c.change.ends_with("-> true") {
            self.on.insert(flag);
        } else {
            self.on.remove(&flag);
        }
        self.flips()
    }

    fn flips(&mut self) -> Flips {
        let now: BTreeSet<u64> = (self.flags.iter().map(|(&s, &f)| (s, f)))
            .chain(self.taken.iter().copied())
            .filter(|(_, f)| self.on.contains(f))
            .map(|(s, _)| s)
            .collect();
        let revealed: BTreeSet<String> = (self.doors.iter())
            .filter(|(_, f)| self.on.contains(&Flag::Bool(*f)))
            .map(|(d, _)| d.clone())
            .collect();
        let flips = Flips {
            checked: now.difference(&self.checked).copied().collect(),
            unchecked: self.checked.difference(&now).copied().collect(),
            doors: revealed != self.revealed,
        };
        self.checked = now;
        self.revealed = revealed;
        flips
    }

    /// The Checks tab's door rows for a seed's history entry.
    pub fn door_rows(&self, entry: &Entry) -> Vec<DoorRow> {
        let row = |d: &Door| {
            let found = self.revealed.contains(&d.id).then(|| d.dungeon.clone());
            DoorRow {
                id: d.id.clone(),
                name: d.name(),
                vanilla: d.vanilla.clone(),
                // A revealed door ignores its pick.
                pick: (found.is_none())
                    .then(|| entry.picks.get(&d.id).cloned())
                    .flatten(),
                found,
            }
        };
        entry.report.entrances.iter().map(row).collect()
    }

    /// The tab's rows for a seed's history entry.
    /// The Checks tab's rows, with `label` giving each flag's label.
    pub fn rows(&self, entry: &Entry, label: impl Fn(u32) -> Option<String>) -> Vec<Row> {
        let row = |spot: &Spot| {
            // Only a Bool can be labelled.
            let flag = match self.flags.get(&spot.hash) {
                Some(Flag::Bool(f)) => Some(*f),
                _ => None,
            };
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
pub fn in_save(save: &Save) -> impl Fn(Flag) -> bool + '_ {
    let keys = save.keys();
    move |f| match f {
        Flag::Bool(h) => (save.get(h))
            .and_then(|e| save.scalar(e))
            .is_some_and(|v| v != 0),
        Flag::Key(k) => keys.contains(&k),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::gifts;
    use crate::settings::{MASTER_SWORD, MERCHANTS, NPC_GIFTS, TRADE_CHAIN};

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
            flags: MIDO.map(|(s, f)| (s, Flag::Bool(f))).into(),
            ..Default::default()
        }
    }

    fn bool_(hash: u32, change: &str) -> Change {
        Change::new(&GameData::default(), hash, "Bool", change)
    }

    /// Kokiri Forest's door holding the Fire Temple, and the Fire Temple's
    /// door holding the Deku Tree.
    fn swapped() -> Vec<Door> {
        let door = |id: &str, vanilla: &str, dungeon: &str, name: &str| Door {
            id: id.into(),
            area: "FakeArea".into(),
            vanilla: vanilla.into(),
            dungeon: dungeon.into(),
            dungeon_name: name.into(),
        };
        vec![
            door("deku-tree", "Deku Tree", "fire-temple", "Fire Temple"),
            door("fire-temple", "Fire Temple", "deku-tree", "Deku Tree"),
        ]
    }

    fn arrived(dungeon: &str) -> u32 {
        let i = DUNGEONS.iter().position(|d| d.id == dungeon).unwrap();
        murmur3(reveal::flag(i).as_bytes())
    }

    #[test]
    fn a_door_is_revealed_by_the_bool_of_the_dungeon_behind_it() {
        let mut t = Tracker::default().with_doors(&swapped());
        let mut history = crate::history::History::default();
        let report = crate::generate::Report {
            entrances: swapped(),
            ..crate::history::tests::report(7)
        };
        let none = (Default::default(), Default::default(), Default::default());
        history.add(none.0, none.1, none.2, report);
        history.pick(1, "deku-tree", Some("deku-tree")).unwrap();
        let entry = history.get(1).unwrap();
        let rows = t.door_rows(entry);
        assert_eq!(rows[0].found, None);
        assert_eq!(rows[0].pick.as_deref(), Some("deku-tree"));
        let flips = t.change(&bool_(arrived("fire-temple"), "false -> true"));
        assert!(flips.doors && flips.checked.is_empty());
        let rows = t.door_rows(entry);
        assert_eq!(rows[0].found.as_deref(), Some("fire-temple"));
        assert_eq!(rows[0].pick, None, "a revealed door ignores its pick");
        assert_eq!(rows[1].found, None);
        assert!(
            !t.change(&bool_(arrived("fire-temple"), "false -> true"))
                .doors
        );
    }

    #[test]
    fn a_save_reveals_the_doors_link_has_been_through() {
        let mut t = Tracker::default().with_doors(&swapped());
        let deku = arrived("deku-tree");
        assert!(t.load(|f| f == Flag::Bool(deku)).doors);
        assert_eq!(t.revealed, BTreeSet::from(["fire-temple".to_string()]));
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
    fn a_placement_ticks_by_its_own_key() {
        let mut t = Tracker {
            flags: [(7, Flag::Key(7))].into(),
            ..Default::default()
        };
        assert_eq!(t.change(&Change::keyed(8, true)), Flips::default());
        assert_eq!(t.change(&Change::keyed(7, true)).checked, [7]);
        assert_eq!(t.change(&Change::keyed(7, false)).unchecked, [7]);
        // A Bool that happens to share the key's low bits is another flag.
        assert_eq!(t.change(&bool_(7, "false -> true")), Flips::default());
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
            entrances: vec![],
            spots: vec![spot(MIDO[2].0), spot(1)],
            written: crate::generate::Written {
                defaults: vec![],
                actors: 0,
                files: 0,
                stand_ins: vec![],
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
            assert_eq!(t.flags[&h], Flag::Bool(f));
        }
        let on = [MERCHANTS, NPC_GIFTS, TRADE_CHAIN, MASTER_SWORD]
            .map(String::from)
            .into();
        let added = gifts::flags(&on);
        for (g, found) in GIFTS.iter().filter_map(|g| Some((g, g.found()?))) {
            let f = t.flags[&g.hash()];
            assert_eq!(f, Flag::Bool(murmur3(found.as_bytes())), "{}", g.giver);
            if !added.contains(&found) {
                assert_eq!(gd.table(&found), Some("Bool"), "{}", g.flow);
            }
        }
        for own in &added {
            let gift = GIFTS.iter().filter(|g| g.found().as_ref() == Some(own));
            assert_eq!(gift.count(), 1, "{own} is one gift's");
        }
    }

    #[test]
    fn cows_map_to_their_own_flags() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let hash = 11142894434822030727;
        let t = Tracker::new(&gd, &[spot(hash)]).unwrap();
        let flag = cows::flag_at(hash).unwrap();
        assert_eq!(t.flag(hash), Some(Flag::Bool(murmur3(flag.as_bytes()))));
    }

    #[test]
    fn a_pot_ticks_once_its_drop_is_picked_up() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        // Kokiri Forest's rock, whose own key resets with its area.
        let hash = 6429280567576879719;
        let rock = Spot {
            holder: crate::known::actor::OBJ_LIFT_ROCK_A_01.to_string(),
            ..spot(hash)
        };
        let t = Tracker::new(&gd, &[rock]).unwrap();
        let flag = picked::flag(hash);
        assert_eq!(t.flag(hash), Some(Flag::Bool(murmur3(flag.as_bytes()))));
    }

    #[test]
    fn a_pot_ticks_on_any_stand_in_taken_without_the_hook() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let taken = [
            "Rando_Taken_Obj_RandoStep_001_0",
            "Rando_Taken_Obj_RandoStep_001_1",
        ];
        let pot = Spot {
            taken: taken.map(String::from).to_vec(),
            ..spot(7)
        };
        let mut t = Tracker::new(&gd, &[pot]).unwrap();
        let second = murmur3(taken[1].as_bytes());
        assert!(t.flags().any(|f| f == Flag::Bool(second)));
        let flips = t.change(&bool_(second, "false -> true"));
        assert_eq!(flips.checked, [7]);
        let flips = t.change(&bool_(second, "true -> false"));
        assert_eq!(flips.unchecked, [7]);
    }

    #[test]
    fn a_shop_slot_ticks_once_it_sells() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let slot = &crate::shops::slots(&romfs).unwrap()[0];
        let shelf = Spot {
            holder: "Kokiri Shop 1".into(),
            ..spot(slot.hash())
        };
        let t = Tracker::new(&gd, &[shelf]).unwrap();
        let flag = crate::shops::sold::flag(slot);
        assert_eq!(
            t.flag(slot.hash()),
            Some(Flag::Bool(murmur3(flag.as_bytes())))
        );
    }

    #[test]
    fn a_skulltula_keeping_its_token_ticks_by_its_key() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let s = &crate::pickups::SKULLTULAS.checks(&romfs).unwrap()[0];
        let token = Spot {
            holder: s.holder.clone(),
            vanilla: s.item.clone(),
            item: s.item.clone(),
            ..spot(s.hash)
        };
        let t = Tracker::new(&gd, &[token]).unwrap();
        assert_eq!(t.flag(s.hash), Some(Flag::Key(s.hash)));
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
                assert_eq!(
                    t.flag(s.hash()),
                    Some(Flag::Bool(murmur3(sold.as_bytes()))),
                    "{sold}"
                );
                if !sold.starts_with("Rando_") {
                    assert_eq!(gd.table(&sold), Some("Bool"), "{sold}");
                }
            }
        }
    }
}
