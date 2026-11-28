//! The new game's state for each cell its mode frees: what the vanilla
//! rescue leaves behind, nothing invented.
//!
//! | State | What it does |
//! |---|---|
//! | `beaten` | his thief never spawns, and her floor key only with it unset |
//! | the key taken | her key never spawns on the floor |
//! | `opened` | the cell's logic sees the door already opened |
//! | `saved` | the cell's logic deletes him, and the valley camp shows his escaped copy |
//! | the door Bool | the lock is gone on load and the door pushes open |
//!
//! The counts follow: freed cells helped, and the rest still to free, so
//! the last rescue plays the clear event as in the game. Gerudo Fortress
//! Freed also starts with the clear event's results.

use anyhow::{Context, Result};
use ounce_formats::murmur3;

use super::{CELLS, CLEARED, Cell, jailed};
use crate::gamedata::GameData;
use crate::known::{Name, field, flag, flow};
use crate::presets::all;
use crate::settings::GerudoFortress;

/// The carpenters counted freed, and those still to free.
const HELPED: &Name = field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED;
const LEFT: &Name = field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED_02;

/// The edits a new game under `mode` makes: each freed cell's state, the
/// counts, and under Freed the clear event's results. Nothing under Normal.
pub fn edits(gd: &GameData, mode: GerudoFortress) -> Result<Vec<String>> {
    let freed: Vec<&Cell> = CELLS.iter().filter(|c| !jailed(mode, c)).collect();
    if freed.is_empty() {
        return Ok(Vec::new());
    }
    let mut edits = Vec::new();
    for cell in &freed {
        edits.extend(all([cell.beaten, cell.opened, cell.saved], "true"));
        edits.push(format!("{}=true", cell.key_taken()));
        edits.push(format!("#{:08x}=true", door(gd, cell)?));
    }
    edits.push(format!("{HELPED}={}", freed.len()));
    edits.push(format!("{LEFT}={}", CELLS.len() - freed.len()));
    if freed.len() == CELLS.len() {
        edits.extend(cleared());
    }
    Ok(edits)
}

/// The quest steps a new game under `mode` moves, as `(challenge, step)`:
/// with every cell freed, the step the clear event moves it to. A step
/// moved by the opening, not a save edit, stands the guards down with it.
pub fn progress(mode: GerudoFortress) -> Vec<(String, String)> {
    match CELLS.iter().any(|c| jailed(mode, c)) {
        true => Vec::new(),
        false => vec![(flow::SAGE_OF_SPIRIT.to_string(), CLEARED.to_owned())],
    }
}

/// What the clear event leaves beside the quest step: the first guard off
/// her post, the event played, and each spot the guards throw Link from
/// passed.
fn cleared() -> Vec<String> {
    let mut edits = vec![
        format!(
            "{}=false",
            field::GERUDO_FORTRESS_NPC_GERUDO_001_IS_IN_FIRST_PLACE
        ),
        format!("{}=true", super::valley::played()),
    ];
    edits.extend(all(
        [
            flag::GERUDO_FORTRESS_WARP_POINT01_CLEAR_EVENT,
            flag::GERUDO_FORTRESS_WARP_POINT02_CLEAR_EVENT,
            flag::GERUDO_FORTRESS_WARP_POINT03_CLEAR_EVENT,
            flag::GERUDO_FORTRESS_WARP_POINT04_CLEAR_EVENT,
        ],
        "true",
    ));
    edits
}

/// `cell`'s door Bool, which the game sets as its lock breaks: the member
/// of [`flag::IS_CHAIN_LOCK_DOOR_OPEN`] keyed by its shutter's placement.
/// The game knows it only by its hash.
fn door(gd: &GameData, cell: &Cell) -> Result<u32> {
    let doors = gd.members(murmur3(flag::IS_CHAIN_LOCK_DOOR_OPEN.get().as_bytes()))?;
    let key = murmur3(cell.shutter.to_string().as_bytes());
    let found = doors.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    found.with_context(|| format!("cell {}'s door has no open flag", cell.n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    fn load() -> Option<GameData> {
        dev_dump().map(|romfs| GameData::load(&romfs).unwrap())
    }

    #[test]
    fn normal_starts_with_nothing_freed() {
        let Some(gd) = load() else { return };
        assert!(edits(&gd, GerudoFortress::Normal).unwrap().is_empty());
        assert!(progress(GerudoFortress::Normal).is_empty());
        assert!(progress(GerudoFortress::Fast).is_empty());
    }

    #[test]
    fn fast_frees_all_but_the_one_torch_jail() {
        let Some(gd) = load() else { return };
        let edits = edits(&gd, GerudoFortress::Fast).unwrap();
        for cell in &CELLS {
            let freed = edits.contains(&format!("{}=true", cell.saved));
            assert_eq!(freed, !jailed(GerudoFortress::Fast, cell), "{}", cell.n);
        }
        assert!(edits.contains(&format!("{HELPED}=3")));
        assert!(edits.contains(&format!("{LEFT}=1")));
        assert!(!edits.contains(&format!("{}=true", super::super::valley::played())));
    }

    #[test]
    fn freed_frees_every_cell_and_clears_the_quest() {
        let Some(gd) = load() else { return };
        let edits = edits(&gd, GerudoFortress::Freed).unwrap();
        for cell in &CELLS {
            for f in [cell.beaten, cell.opened, cell.saved] {
                assert!(edits.contains(&format!("{f}=true")), "{f}");
            }
            assert!(edits.contains(&format!("{}=true", cell.key_taken())));
        }
        assert!(edits.contains(&format!("{HELPED}=4")));
        assert!(edits.contains(&format!("{LEFT}=0")));
        assert!(edits.contains(&format!("{}=true", super::super::valley::played())));
        assert_eq!(
            progress(GerudoFortress::Freed),
            [(flow::SAGE_OF_SPIRIT.to_string(), CLEARED.to_owned())]
        );
    }

    /// Each door Bool is its own, found by its shutter, and they're the
    /// ones the cells were seen opening by live (2026-11-28).
    #[test]
    fn each_door_flag_is_found_by_its_shutter() {
        let Some(gd) = load() else { return };
        let doors: Vec<u32> = CELLS.iter().map(|c| door(&gd, c).unwrap()).collect();
        assert_eq!(doors, [0xd19eea2d, 0x8521f437, 0x14c93196, 0x5048b586]);
        for d in doors {
            assert_eq!(gd.kind(d), Some("Bool"), "{d:08x}");
        }
    }

    #[test]
    fn the_start_edits_apply_to_the_game() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let mut doc =
            ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        for mode in [GerudoFortress::Fast, GerudoFortress::Freed] {
            crate::presets::apply(&gd, &mut doc.root, &edits(&gd, mode).unwrap()).unwrap();
        }
    }

    /// A freed cell's `saved` and `beaten` are what the rescue flow sets
    /// for him (it names each by an argument, so the flag map resolves
    /// them): a start never invents a state of its own.
    #[test]
    fn a_freed_cell_is_left_as_the_rescue_leaves_it() {
        let Some(g) = crate::flagmap::Graph::dev() else {
            return;
        };
        let rescue = flow::SAGE_OF_SPIRIT_RESCUE_CARPENTER.get();
        for cell in &CELLS {
            for f in [cell.saved, cell.beaten] {
                let node = g.find(&format!("state:{f}")).unwrap();
                let writers = g.writers(node);
                assert!(
                    writers.edges.iter().any(|e| e.file.contains(rescue)),
                    "the rescue never sets {f}"
                );
            }
        }
    }
}
