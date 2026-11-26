//! Boulders' checks. A broken boulder is gone for good (its key never
//! resets), so its item can't be its drop: a drop can roll off a ledge or
//! into lava, and nothing is left to break again. The item lies where the
//! boulder stood instead, until Link takes it (maintainer, 2026-11-25): a
//! seat, a copy of the boulder's placement under a fresh hash holding the
//! item as a pickup lying loose does, inside the boulder until it breaks.
//! The boulder drops nothing.
//!
//! A seat stands until its boulder's check is got (`picked::After::Seated`),
//! and keeps a key like the boulder's, so a vanilla pickup there stays
//! taken too.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::Byml;

use super::{GYAML, Kind, moved};
use crate::checks::{self, DYNAMIC, Drop, ItemCheck, hash_of};
use crate::known::{actor, field};
use crate::shows::{copy_hash, rehash};

/// The boulders bombs or the hammer break. Each drops a Green Rupee from
/// its pack's table, unless its placement names none.
pub const BOULDERS: Kind = Kind {
    actors: &[actor::OBJ_BROKEN_ROCK_A_01, actor::OBJ_GORON_BROKEN_ROCK_A_01],
    item: Some(actor::OBJ_RUPEE_GREEN),
    field: GYAML,
    needs: Some("break-boulder"),
    named: None,
};

/// The red boulders, where the original game's hammer-only rocks stood.
/// Their damage table takes bombs as well, but the maintainer puts them in
/// the hammer's tier. Same table as the other boulders.
pub const RED_BOULDERS: Kind = Kind {
    actors: &[actor::OBJ_BROKEN_ROCK_A_02],
    item: Some(actor::OBJ_RUPEE_GREEN),
    field: GYAML,
    needs: Some("hammer"),
    named: None,
};

/// The kinds whose item lies in a seat.
const SEATED: [&Kind; 2] = [&BOULDERS, &RED_BOULDERS];

/// Where seats start among a placement's copies: well past any step's.
const SEAT: usize = 1 << 16;

/// Whether `holder` is a boulder whose item lies in a seat.
pub fn places(holder: &str) -> bool {
    SEATED.iter().any(|k| k.places(holder))
}

/// Whether `check` is a boulder's.
pub fn holds(check: &ItemCheck) -> bool {
    places(&check.holder)
}

/// Where step `k` of the item of the boulder placed as `of` lies.
pub fn seat(of: u64, k: usize) -> u64 {
    copy_hash(of, SEAT + k)
}

/// The field that keeps the boulder placed as `of` from dropping its
/// Green Rupee: the empty table some boulders already name.
pub fn no_drop(of: u64) -> String {
    format!("{of}.{DYNAMIC}.{}=None", field::DROP_DROP_TABLE)
}

/// Every boulder's seats.
#[derive(Default)]
pub struct Seats {
    /// Each seat, by the boulder it copies.
    pub pairs: Vec<(u64, u64)>,
    /// The BoolExp each seat that shows no steps stands by, until its
    /// boulder's check is got. A step's seat stands by its step's gate
    /// (`pickups::steps`).
    gates: HashMap<u64, String>,
}

impl Seats {
    pub fn new(
        checks: &[ItemCheck],
        drops: &HashMap<u64, Drop>,
        progressive: crate::progressive::Lines,
    ) -> Self {
        let mut seats = Self::default();
        let boulders: Vec<ItemCheck> = checks.iter().filter(|c| holds(c)).cloned().collect();
        for m in moved(&boulders, drops, progressive) {
            seats.pairs.push((m.of, m.hash));
            if m.step.is_none() {
                seats.gates.insert(m.hash, crate::picked::got(m.of));
            }
        }
        seats
    }

    /// Place each seat in `root`, a map's doc: a copy of its boulder, under
    /// its own hash and without what makes it a boulder (its drop, scale
    /// and baked collision), standing until the check is got. Run before
    /// the step gates and the fields, which give each seat its gate and its
    /// pickup. Whether any was placed.
    pub fn place(&self, root: &mut Byml) -> Result<bool> {
        let Some(list) = root.as_mut_map()?.get_mut("Actors") else {
            return Ok(false);
        };
        let list = list.as_mut_array()?;
        let mut new = Vec::new();
        for &(of, hash) in &self.pairs {
            let Some(boulder) = list.iter().find(|a| hash_of(a) == Some(of)) else {
                continue;
            };
            let mut seat = boulder.clone();
            rehash(&mut seat, hash)?;
            let map = seat.as_mut_map()?;
            for key in [DYNAMIC, "Scale", "Bakeable"] {
                map.remove(key);
            }
            if let Some(gate) = self.gates.get(&hash) {
                checks::gate(&mut seat, gate, true)?;
            }
            new.push(seat);
        }
        let placed = !new.is_empty();
        list.extend(new);
        Ok(placed)
    }
}

#[cfg(test)]
mod tests {
    use ounce_formats::murmur3;
    use zootr_hook::Spot;

    use super::*;
    use crate::checks::{TRANSLATE, str_field};
    use crate::game::dev_dump;
    use crate::gamedata::GameData;
    use crate::known::item;
    use crate::picked;
    use crate::pickups::steps::Steps;
    use crate::progressive::Lines;
    use crate::settings::NewGame;
    use crate::world::presence_of;

    /// A boulder in the adult Goron City.
    const BOULDER: u64 = 3009822989781876601;
    /// A Death Mountain Crater boulder both ages' maps place, naming no
    /// drop table.
    const NO_TABLE: u64 = 5524931827254348969;
    /// A red boulder only the child's Death Mountain Crater places. Most
    /// of the crater's stand in both ages' maps under one hash.
    const CHILD_RED: u64 = 15044423826661592135;

    /// The boulder placed as `hash`, dealt `drops`, with every edit the
    /// seed makes to its map: the map, and the hook's spots in it.
    fn placed(hash: u64, drops: &HashMap<u64, Drop>, lines: Lines) -> (Byml, Vec<Spot>, Steps) {
        let romfs = dev_dump().unwrap();
        let gd = GameData::load(&romfs).unwrap();
        let checks: Vec<ItemCheck> = (BOULDERS.checks(&romfs).unwrap().into_iter())
            .filter(|c| c.hash == hash)
            .collect();
        let holders = picked::holders(&checks, drops);
        let steps = Steps::new(&checks, drops, lines, &holders, &gd).unwrap();
        let seats = Seats::new(&checks, drops, lines);
        let mut doc = checks::load_banc(&romfs, &checks[0].file).unwrap();
        picked::place(&mut doc.root, &holders).unwrap();
        assert!(seats.place(&mut doc.root).unwrap());
        steps.place(&mut doc.root).unwrap();
        let mut fields: Vec<String> = (moved(&checks, drops, lines).iter())
            .map(|m| format!("{}.{}={}", m.hash, m.field, m.actor))
            .collect();
        fields.push(no_drop(hash));
        checks::set_fields(&mut doc.root, &fields).unwrap();
        let copies = [steps.copies.as_slice(), &seats.pairs].concat();
        let spots = picked::spots(&doc.root, &holders, &copies).unwrap();
        (doc.root, spots, steps)
    }

    /// The one placement `hash` in `root`.
    fn one(root: &Byml, hash: u64) -> &Byml {
        let mut found = (checks::actors(root).unwrap().iter()).filter(|a| hash_of(a) == Some(hash));
        let a = found.next().unwrap_or_else(|| panic!("no placement {hash}"));
        assert!(found.next().is_none(), "{hash} placed twice");
        a
    }

    fn dynamic(a: &Byml) -> Option<&Byml> {
        a.as_map().unwrap().get(DYNAMIC)
    }

    #[test]
    fn a_boulder_holds_its_item_in_a_seat_and_drops_nothing() {
        if dev_dump().is_none() {
            return;
        }
        let drops = HashMap::from([(BOULDER, Drop::from(item::SHIELD_MIRROR.to_string()))]);
        let (root, spots, _) = placed(BOULDER, &drops, Lines::climbing());
        let (boulder, lies) = (one(&root, BOULDER), one(&root, seat(BOULDER, 0)));
        // The boulder stays itself, minus its drop.
        assert_eq!(str_field(boulder, GYAML), Some(actor::OBJ_BROKEN_ROCK_A_01.get()));
        assert!(presence_of(boulder).is_none());
        let table = dynamic(boulder).and_then(|d| str_field(d, field::DROP_DROP_TABLE.get()));
        assert_eq!(table, Some("None"));
        // The seat lies where it stood, a stand-in standing until got.
        assert!(str_field(lies, GYAML).unwrap().starts_with(super::super::NEW));
        assert_eq!(lies.as_map().unwrap()[TRANSLATE], boulder.as_map().unwrap()[TRANSLATE]);
        for key in [DYNAMIC, "Scale", "Bakeable"] {
            assert!(!lies.as_map().unwrap().contains_key(key), "{key}");
        }
        let gate = presence_of(lies).unwrap();
        assert_eq!((gate.flag, gate.negated), (picked::got(BOULDER), true));
        // The hook ticks the boulder's flag at the seat alone.
        assert_eq!(spots.len(), 1);
        assert_eq!(spots[0].placed, seat(BOULDER, 0));
        assert_eq!(spots[0].flag, murmur3(picked::flag(BOULDER).as_bytes()));
    }

    #[test]
    fn a_boulder_dealt_its_own_rupee_still_holds_it_in_a_seat() {
        if dev_dump().is_none() {
            return;
        }
        let (root, _, _) = placed(BOULDER, &HashMap::new(), Lines::climbing());
        let lies = one(&root, seat(BOULDER, 0));
        assert_eq!(str_field(lies, GYAML), Some(actor::OBJ_RUPEE_GREEN.get()));
    }

    #[test]
    fn a_boulder_dealt_a_line_shows_each_step_in_a_seat_of_its_own() {
        if dev_dump().is_none() {
            return;
        }
        let drops = HashMap::from([(BOULDER, Drop::from(item::HOOKSHOT.to_string()))]);
        let (root, spots, steps) = placed(BOULDER, &drops, Lines::climbing());
        assert!(steps.copies.is_empty(), "the seats copy the boulder");
        for k in 0..2 {
            let gate = presence_of(one(&root, seat(BOULDER, k))).unwrap();
            assert_eq!(gate.flag, crate::pickups::steps::gate(seat(BOULDER, k)));
            assert!(!gate.negated);
        }
        assert_eq!(spots.len(), 2);
    }

    /// The table some boulders already name is the string the seed writes.
    #[test]
    fn a_boulder_naming_no_table_names_it_as_the_seed_does() {
        let Some(romfs) = dev_dump() else { return };
        let checks = BOULDERS.checks(&romfs).unwrap();
        let check = checks.iter().find(|c| c.hash == NO_TABLE).unwrap();
        let doc = checks::load_banc(&romfs, &check.file).unwrap();
        let table = dynamic(one(&doc.root, NO_TABLE)).unwrap();
        assert_eq!(table.as_map().unwrap()[field::DROP_DROP_TABLE.get()], Byml::String("None".into()));
        assert!(no_drop(NO_TABLE).ends_with("Drop__DropTable=None"));
    }

    /// A boulder asks its tool, and a red boulder only a child's map
    /// places can't be broken while the hammer is an adult's.
    #[test]
    fn a_boulder_asks_its_tool_and_a_childs_red_boulder_is_never() {
        let Some(romfs) = dev_dump() else { return };
        let book = crate::rules::Book::load().unwrap();
        let new_game = NewGame {
            rocks: true,
            ..Default::default()
        };
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        assert!(checks.iter().any(|c| c.hash == CHILD_RED));
        let ctx = crate::rules::Ctx::new(&new_game);
        let world = crate::logic::World::build(&romfs, &book, &ctx, &checks).unwrap();
        assert!(world.never(CHILD_RED));
        assert!(!world.never(BOULDER));
        assert!(!world.never(NO_TABLE));
    }
}
