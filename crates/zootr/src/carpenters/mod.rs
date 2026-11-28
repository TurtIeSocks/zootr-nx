//! The carpenters quest in the Gerudo Fortress: four carpenters jailed in
//! four cells, each behind a thief who carries the key to his door. Beating
//! her and opening the door frees him, and the fourth freed plays the clear
//! event: the card, the guards standing down, the valley bridge.
//!
//! [`CELLS`] is the one table every part reads. A mode ([`GerudoFortress`])
//! decides which cells start jailed ([`jailed`]); every other cell starts as
//! the vanilla rescue leaves it ([`start`]).
//!
//! - [`rescue`]: the carpenters take part in every quest step, so a jailed
//!   one is rescued at any step (issue #550).
//! - [`start`]: the new game's state for each freed cell.
//! - [`guards`]: the card stands the guards down however Link gets it.
//! - [`valley`]: the bridge and the valley follow the clear event.
//! - [`keys`]: the thieves' keys as checks, and as copies.
//! - [`logic`]: what each cell's checks need.

use anyhow::Result;
use ounce_formats::{ModBuilder, Romfs};

use crate::checks::Drop;
use crate::flows::Rewrites;
use crate::known::{Name, actor, flag, flow};
use crate::settings::GerudoFortress;

pub(crate) mod guards;
pub(crate) mod keys;
pub(crate) mod logic;
mod rescue;
pub(crate) mod start;
pub(crate) mod valley;

pub use guards::CARD;

/// One cell: its carpenter, his thief and her key, the door and what lies
/// inside, and the flags the rescue sets.
pub struct Cell {
    /// The cell's number, 1 to 4, which its thief's call and key carry.
    pub n: usize,
    /// The carpenter jailed here, and his placement in the cell (which
    /// the table's test checks every other placement against).
    pub carpenter: &'static Name,
    #[cfg_attr(not(test), allow(dead_code))]
    pub placement: u64,
    /// The thief guarding him, by placement, and the flow entry that runs
    /// as she falls and moves her key to her.
    pub thief: u64,
    pub thief_call: &'static Name,
    /// Whether she also carries her key as a drop, which a seed moving the
    /// keys clears, so the one key she gives is the seed's.
    pub carries: bool,
    /// The key she carries, by actor.
    pub key: &'static Name,
    /// The door's shutter, whose placement keys the cell's door Bool in
    /// [`flag::IS_CHAIN_LOCK_DOOR_OPEN`].
    pub shutter: u64,
    /// The pot inside the cell.
    pub pot: u64,
    /// True once he's freed; the cell's logic then deletes him, and the
    /// valley camp shows his escaped copy.
    pub saved: &'static Name,
    /// True once his thief is beaten; she never spawns again.
    pub beaten: &'static Name,
    /// True once his door first opens.
    pub opened: &'static Name,
    /// Whether reaching the cell takes the Hookshot. The 1 Torch Jail opens
    /// off the fortress floor; the other three are up on the walls.
    pub hookshot: bool,
}

impl Cell {
    /// The Bool the cell's key sets once taken.
    pub fn key_taken(&self) -> String {
        keys::taken(self.n)
    }
}

/// The four cells, numbered as their thieves' keys are.
pub const CELLS: [Cell; 4] = [
    Cell {
        n: 1,
        carpenter: actor::NPC_HYLIA_013_ADULT,
        placement: 9866222048459639395,
        thief: 3350876166368957251,
        thief_call: flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF01,
        carries: true,
        key: actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_01,
        shutter: 3863558049542520938,
        pot: 10284629266025908616,
        saved: flag::GERUDO_FORTRESS_SAVED_NPC_HYLIA_013_ADULT,
        beaten: flag::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF_NPC_HYLIA_013_ADULT,
        opened: flag::GERUDO_FORTRESS_IS_AFTER_HELP_CARPENTER_NPC_HYLIA_013_ADULT,
        hookshot: false,
    },
    Cell {
        n: 2,
        carpenter: actor::NPC_HYLIA_014_ADULT,
        placement: 7927735019387187870,
        thief: 3233425834948791513,
        thief_call: flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF02,
        carries: true,
        key: actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_02,
        shutter: 12201947902126135747,
        pot: 2601738864413660498,
        saved: flag::GERUDO_FORTRESS_SAVED_NPC_HYLIA_014_ADULT,
        beaten: flag::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF_NPC_HYLIA_014_ADULT,
        opened: flag::GERUDO_FORTRESS_IS_AFTER_HELP_CARPENTER_NPC_HYLIA_014_ADULT,
        hookshot: true,
    },
    Cell {
        n: 3,
        carpenter: actor::NPC_HYLIA_015_ADULT,
        placement: 12805708668838722693,
        thief: 18433860338868908771,
        thief_call: flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF03,
        carries: false,
        key: actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_03,
        shutter: 13654920463841843247,
        pot: 6163979871466990965,
        saved: flag::GERUDO_FORTRESS_SAVED_NPC_HYLIA_015_ADULT,
        beaten: flag::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF_NPC_HYLIA_015_ADULT,
        opened: flag::GERUDO_FORTRESS_IS_AFTER_HELP_CARPENTER_NPC_HYLIA_015_ADULT,
        hookshot: true,
    },
    Cell {
        n: 4,
        carpenter: actor::NPC_HYLIA_016_ADULT,
        placement: 5615021988618946450,
        thief: 13730570727502202323,
        thief_call: flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF04,
        carries: true,
        key: actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_04,
        shutter: 13796324535605592497,
        pot: 14149922038181651872,
        saved: flag::GERUDO_FORTRESS_SAVED_NPC_HYLIA_016_ADULT,
        beaten: flag::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF_NPC_HYLIA_016_ADULT,
        opened: flag::GERUDO_FORTRESS_IS_AFTER_HELP_CARPENTER_NPC_HYLIA_016_ADULT,
        hookshot: true,
    },
];

/// The Spirit quest's step once the four are free, which the clear event
/// moves it to: past it the guards stand down.
pub const CLEARED: &str = "Step02";

/// The cell Gerudo Fortress Fast leaves jailed, as Ship does: the 1 Torch
/// Jail, the one off the fortress floor.
const ONE_TORCH_JAIL: usize = 1;

/// Whether `cell` starts jailed under `mode`.
pub fn jailed(mode: GerudoFortress, cell: &Cell) -> bool {
    match mode {
        GerudoFortress::Normal => true,
        GerudoFortress::Fast => cell.n == ONE_TORCH_JAIL,
        GerudoFortress::Freed => false,
    }
}

/// How many cells start jailed under `mode`: the thief checks and keys
/// it keeps in the pool.
pub fn jailed_count(mode: GerudoFortress) -> usize {
    CELLS.iter().filter(|c| jailed(mode, c)).count()
}

/// The cell whose thief carries `key`, a Fortress key actor.
pub fn cell_of_key(key: &str) -> Option<&'static Cell> {
    CELLS.iter().find(|c| *c.key == *key)
}

/// The flow edits every seed makes: the card stands the guards down, and
/// the valley's talk follows the clear event.
pub fn flows(romfs: &Romfs, rewrites: &mut Rewrites, drops: &DropMap) -> Result<()> {
    guards::stand_down(romfs, rewrites, drops)?;
    valley::flows(romfs, rewrites)
}

/// The files every seed writes beside the flows and maps: the carpenters
/// in every quest step, and the valley's bridge, navmesh, schedule and map
/// picture.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    rescue::write(romfs, builder)?;
    valley::write(romfs, builder)
}

type DropMap = std::collections::HashMap<u64, Drop>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    #[test]
    fn each_mode_jails_its_cells() {
        let carpenters = |mode| -> Vec<&Name> {
            CELLS
                .iter()
                .filter(|c| jailed(mode, c))
                .map(|c| c.carpenter)
                .collect()
        };
        assert_eq!(carpenters(GerudoFortress::Normal).len(), 4);
        assert_eq!(
            carpenters(GerudoFortress::Fast),
            [actor::NPC_HYLIA_013_ADULT]
        );
        assert!(carpenters(GerudoFortress::Freed).is_empty());
        assert_eq!(
            [
                GerudoFortress::Normal,
                GerudoFortress::Fast,
                GerudoFortress::Freed
            ]
            .map(jailed_count),
            [4, 1, 0]
        );
    }

    #[test]
    fn the_cells_are_numbered_in_order() {
        for (i, c) in CELLS.iter().enumerate() {
            assert_eq!(c.n, i + 1);
        }
    }

    /// Each placement in `CELLS` is what it says, in the adult fortress:
    /// the carpenter, his thief, the key, the door and the pot, and the door
    /// and pot within a few metres of him.
    #[test]
    fn every_cell_names_its_own_placements() {
        let Some(romfs) = dev_dump() else { return };
        let placed =
            crate::warp::placements_in(&romfs, crate::known::map::GERUDO_FORTRESS_ADULT).unwrap();
        let at = |hash: u64| {
            let p = placed.iter().find(|p| p.hash == Some(hash));
            p.unwrap_or_else(|| panic!("no placement {hash}"))
        };
        let apart = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[2] - b[2]);
        for c in &CELLS {
            let carpenter = at(c.placement);
            assert_eq!(carpenter.name, c.carpenter.get());
            assert!(
                at(c.thief)
                    .name
                    .starts_with(actor::ENEMY_MIDDLE_BOSS_GERUDO_THIEF.get()),
                "{}",
                c.n
            );
            // Neither a door nor a pot has a known name of its own.
            assert!(at(c.shutter).name.contains("SwitchShutter"), "{}", c.n);
            assert!(at(c.pot).name.contains("Pot"), "{}", c.n);
            for near in [c.shutter, c.pot] {
                let d = apart(carpenter.pose.at, at(near).pose.at);
                assert!(d < 6.0, "cell {}: {near} is {d} m from him", c.n);
            }
        }
    }
}
