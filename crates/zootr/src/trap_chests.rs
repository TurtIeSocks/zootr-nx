//! The ice-trap chests, dealt as plain chests (`checks::set_drops`). The
//! plain chest never opens under a trap chest's own placement hash, with or
//! without an opened flag, so a dealt one is written under a fresh hash on
//! the same spot, with a key like the trap's and an opened flag of its own.
//! The check keeps the trap's hash everywhere else: names, logic, the
//! ledger, Archipelago.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::Byml;

use crate::checks::Drop;
use crate::{gamedata, shows, tracker};

/// The six trap chests: the Water Trial's, the Light Trial's three, the
/// Spirit Temple's and the Gerudo Training Ground's.
pub const HASHES: [u64; 6] = [
    3253191540272036329,
    5732331492537302993,
    17726220643880970879,
    11406545451127340703,
    876108895472176682,
    7881496104962067240,
];

/// A chest whose opened flag each new one copies: the Water Trial's other
/// chest, beside its trap.
const LIKE: u64 = 14404733996390762670;

/// Where a dealt trap chest `hash` is placed.
pub fn placement(hash: u64) -> u64 {
    shows::copy_hash(hash, 1)
}

/// The opened flag of trap chest `hash` once dealt, a saved Bool.
pub fn opened(hash: u64) -> String {
    format!("Randomizer_TrapChest_{hash}")
}

/// [`opened`], when `hash` is a trap chest.
pub fn opened_at(hash: u64) -> Option<String> {
    HASHES.contains(&hash).then(|| opened(hash))
}

/// Each trap chest `drops` deals.
fn dealt(drops: &HashMap<u64, Drop>) -> impl Iterator<Item = u64> + '_ {
    HASHES.into_iter().filter(|h| drops.contains_key(h))
}

/// Each dealt trap chest and its placement, for the keys
/// (`gamedata::add_keys`).
pub fn keyed(drops: &HashMap<u64, Drop>) -> impl Iterator<Item = (u64, u64)> + '_ {
    dealt(drops).map(|h| (h, placement(h)))
}

/// Give each dealt trap chest's placement its opened flag: a member of the
/// chests' Struct, named by the placement in decimal as the game looks it
/// up, holding a copy of [`LIKE`]'s flag.
pub fn game_data(root: &mut Byml, drops: &HashMap<u64, Drop>) -> Result<()> {
    for h in dealt(drops) {
        let name = placement(h).to_string();
        gamedata::copy_member(root, tracker::CHESTS, &LIKE.to_string(), &name, &opened(h))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{self, TRAP_CHEST};

    #[test]
    fn the_six_are_every_trap_chest_placed() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut traps: Vec<u64> = (checks::scan(&romfs).unwrap().iter())
            .filter(|c| c.holder == **TRAP_CHEST)
            .map(|c| c.hash)
            .collect();
        traps.sort();
        let mut hashes = HASHES.to_vec();
        hashes.sort();
        assert_eq!(traps, hashes);
    }

    #[test]
    fn a_dealt_trap_chest_moves_to_a_hash_of_its_own_with_its_own_flag() {
        let drops = HashMap::from([(
            HASHES[0],
            Drop {
                item: "Item_A".into(),
                swap: None,
            },
        )]);
        let keyed: Vec<_> = keyed(&drops).collect();
        assert_eq!(keyed, [(HASHES[0], placement(HASHES[0]))]);
        assert!(!HASHES.contains(&placement(HASHES[0])));
        assert_eq!(opened_at(HASHES[0]), Some(opened(HASHES[0])));
        assert_eq!(opened_at(LIKE), None);
    }
}
