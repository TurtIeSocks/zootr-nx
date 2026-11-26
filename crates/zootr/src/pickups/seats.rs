//! Boulders' and heavy rocks' checks. A broken boulder or a thrown rock
//! is gone for good (its key never resets), so its item can't be its
//! drop: a drop can roll off a ledge or into lava, and nothing is left to
//! break again. The item lies where the rock stood instead, until Link
//! takes it (maintainer, 2026-11-25): a seat, a copy of the rock's
//! placement under a fresh hash holding the item as a pickup lying loose
//! does, inside the rock until it breaks or is lifted away. A boulder
//! drops nothing.
//!
//! A stand-in seat is gone once taken by its own key, as a heart piece's
//! is. A seat holding a plain pickup, which may come back, stands until
//! its boulder's check is got (`picked::After::Seated`).

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
    actors: &[
        actor::OBJ_BROKEN_ROCK_A_01,
        actor::OBJ_GORON_BROKEN_ROCK_A_01,
    ],
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

/// The silver rocks, which take the Silver Gauntlets' lift grade. None
/// drops anything, so each brings a Green Rupee to the pool, as a boulder
/// does.
pub const SILVER_ROCKS: Kind = Kind {
    actors: &[actor::OBJ_LIFT_ROCK_B_01],
    item: Some(actor::OBJ_RUPEE_GREEN),
    field: GYAML,
    needs: Some("silver-gauntlets"),
    named: None,
};

/// The rocks over grotto entrances, which stay vanilla: breaking or
/// lifting one opens the hole under its seat, so Link reaching for the
/// item falls in (maintainer, 2026-11-25, rocks spec). The boulders are
/// the ones whose placement names the empty table; the silver rocks are
/// Desert Colossus's and Gerudo Valley's, in each age's map.
pub const OVER_GROTTOS: [u64; 11] = [
    5524931827254348969,
    11477097298279134715,
    8580335210618464267,
    15605814217953065995,
    12960902182455217518,
    5207698098030997501,
    16547116969322393756,
    10395622039634609957,
    12476546951712559797,
    15973093482153030029,
    8414723300021904225,
];

/// The kinds whose item lies in a seat.
const SEATED: [&Kind; 3] = [&BOULDERS, &RED_BOULDERS, &SILVER_ROCKS];

/// The seated kinds whose pack drops something, which a dealt one stops.
const DROPPING: [&Kind; 2] = [&BOULDERS, &RED_BOULDERS];

/// Where seats start among a placement's copies: well past any step's.
const SEAT: usize = 1 << 16;

/// Whether `holder` is a boulder whose item lies in a seat.
pub fn places(holder: &str) -> bool {
    SEATED.iter().any(|k| k.places(holder))
}

/// Whether `check` is a boulder's or a heavy rock's.
pub fn holds(check: &ItemCheck) -> bool {
    places(&check.holder)
}

/// Whether `check` is a boulder's, whose drop a dealt one stops.
pub fn drops(check: &ItemCheck) -> bool {
    DROPPING.iter().any(|k| k.holds(check))
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
    /// The BoolExp each seat holding a plain pickup stands by, until its
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
            // A built stand-in keeps a key that never resets, so it's gone
            // once taken. Gated too, it went mid-get: the Mirror Shield's
            // get holds it while its take sets the gate, and the get hung.
            if m.step.is_none() && !m.built() {
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
    /// A Death Mountain Crater boulder over a grotto, naming no drop table.
    const NO_TABLE: u64 = OVER_GROTTOS[0];
    /// A red boulder only the child's Death Mountain Crater places. Most
    /// of the crater's stand in both ages' maps under one hash.
    const CHILD_RED: u64 = 15044423826661592135;
    /// A silver rock in the child's and the adult's Goron City maze.
    const CHILD_SILVER: u64 = 14579027738236428127;
    const ADULT_SILVER: u64 = 7587980043199731531;

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
        let a = found
            .next()
            .unwrap_or_else(|| panic!("no placement {hash}"));
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
        assert_eq!(
            str_field(boulder, GYAML),
            Some(actor::OBJ_BROKEN_ROCK_A_01.get())
        );
        assert!(presence_of(boulder).is_none());
        let table = dynamic(boulder).and_then(|d| str_field(d, field::DROP_DROP_TABLE.get()));
        assert_eq!(table, Some("None"));
        // The seat lies where it stood, a stand-in standing until taken.
        assert!(
            str_field(lies, GYAML)
                .unwrap()
                .starts_with(super::super::NEW)
        );
        assert_eq!(
            lies.as_map().unwrap()[TRANSLATE],
            boulder.as_map().unwrap()[TRANSLATE]
        );
        for key in [DYNAMIC, "Scale", "Bakeable"] {
            assert!(!lies.as_map().unwrap().contains_key(key), "{key}");
        }
        assert!(
            presence_of(lies).is_none(),
            "a stand-in's key keeps it taken"
        );
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
        // A rupee comes back, so the seat stands until the check is got.
        let gate = presence_of(lies).unwrap();
        assert_eq!((gate.flag, gate.negated), (picked::got(BOULDER), true));
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
        assert_eq!(
            table.as_map().unwrap()[field::DROP_DROP_TABLE.get()],
            Byml::String("None".into())
        );
        assert!(no_drop(NO_TABLE).ends_with("Drop__DropTable=None"));
    }

    /// A boulder asks its tool, and a red boulder or silver rock only a
    /// child's map places can't be got while its tool is an adult's.
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
        // So with a silver rock: only the adult lifts one, for now.
        assert!(world.never(CHILD_SILVER));
        assert!(!world.never(ADULT_SILVER));
    }

    /// The rocks over grottos are the seated ones standing on a grotto's
    /// entrance, a boulder among them exactly when it names the empty
    /// table, and none is dealt.
    #[test]
    fn the_rocks_over_grottos_stand_on_grotto_entrances() {
        let Some(romfs) = dev_dump() else { return };
        let holes = [
            actor::LOOK_ATTENTION_ACTOR_HIDING_HOLE_ENTRANCE,
            actor::FLD_OBJ_AIR_WALL_HIDING_HOLE_COVER,
        ];
        let mut found = Vec::new();
        for kind in SEATED {
            for check in kind.checks(&romfs).unwrap() {
                let doc = checks::load_banc(&romfs, &check.file).unwrap();
                let rock = one(&doc.root, check.hash);
                let at = checks::vec3(rock, TRANSLATE).unwrap();
                // A map's grotto holes may stand in its other file.
                let files = [
                    check.file.replace("_Dynamic", "_Static"),
                    check.file.replace("_Static", "_Dynamic"),
                ];
                let over = (files.iter())
                    .map(|f| checks::load_banc(&romfs, f).unwrap())
                    .any(|other| {
                        (checks::actors(&other.root).unwrap().iter())
                            .filter(|a| holes.iter().any(|h| str_field(a, GYAML) == Some(h.get())))
                            .filter_map(|a| checks::vec3(a, TRANSLATE))
                            .any(|h| (0..3).map(|i| (h[i] - at[i]).powi(2)).sum::<f32>() < 1.0)
                    });
                if DROPPING.contains(&kind) {
                    let table =
                        dynamic(rock).and_then(|d| str_field(d, field::DROP_DROP_TABLE.get()));
                    assert_eq!(over, table == Some("None"), "{}", check.hash);
                }
                if over {
                    found.push(check.hash);
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        let mut want = OVER_GROTTOS.to_vec();
        want.sort_unstable();
        assert_eq!(found, want);
        let dealt = checks::dealt(
            &romfs,
            &NewGame {
                rocks: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(dealt.iter().all(|c| !OVER_GROTTOS.contains(&c.hash)));
    }
}
