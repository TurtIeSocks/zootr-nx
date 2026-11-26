//! The gauntlet's grid: every item kind at every holder kind. A holder
//! gives one item a new game, so the cells run in rounds, each with at most
//! one cell a holder. Each holder is the one of its kind the sweep's nights
//! passed most (2026-11-13 to 2026-11-17), else one `probe ledger` lists.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::Serialize;

use crate::known::{actor, item};
use crate::{ice_traps, key_tints};

/// What an item kind is for, so `quick` runs one of each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Gear,
    Quest,
    Dungeon,
    Health,
    Filler,
}

/// A kind of holder and the one placement that stands for it.
#[derive(Debug)]
pub struct HolderKind {
    /// As the command line and the report spell it, like `small-chest`.
    pub name: &'static str,
    pub holder: u64,
}

const fn holder(name: &'static str, holder: u64) -> HolderKind {
    HolderKind { name, holder }
}

/// Every holder kind, as the spec lists them.
pub const HOLDERS: [HolderKind; 23] = [
    // Inside Ganon's Castle, Five Arrows: 13 passes, no fail.
    holder("small-chest", 18277713685010400796),
    // Shadow Temple, Hover Boots: 6 of 7.
    holder("big-chest", 14535080241831873137),
    // Dodongo's Cavern: 7 of 7.
    holder("pot", 4234933260265093025),
    // Hyrule Castle box: 1 of 1.
    holder("crate", 5862372604076992768),
    // Hyrule Field, adult: 1 of 1.
    holder("grass", 9816312395494594323),
    // Desert Colossus: 1 of 2.
    holder("rock", 6141646481426607938),
    // Kakariko: no trigger yet.
    holder("beehive", 313487388078450371),
    // Zora River: 6 of 6.
    holder("freestanding", 4189391771527808272),
    // A Hyrule Field Green Rupee.
    holder("loose", 578077075299673958),
    // Graveyard: no trigger yet.
    holder("skulltula", 229475726653285413),
    // Lost Woods, sticks: 8 of 9, a drill's control.
    holder("scrub", 5406759280757349546),
    // Kokiri Shop 3: 8 of 8.
    holder("shop", 12274555791381804315),
    // Medigoron: 6 of 6.
    holder("merchant", 16722769607033707959),
    // The Lake Scientist, a drill's.
    holder("npc-gift", 10888561858771484232),
    // Dampe's race, second prize: a minigame prize that opens.
    holder("minigame", 5534919361313859554),
    // Great Fairy of Wisdom: 3 of 4.
    holder("great-fairy", 7533377855264181032),
    // Zelda's song (Song of Time): 10 of 10.
    holder("song", 3701241424386675305),
    // Gerudo Valley cow: 5 of 5.
    holder("cow", 9796879244811108931),
    // Granny's Odd Mushroom trade.
    holder("trade", 3994987831222113762),
    // No trigger yet.
    holder("running-man", 11582029116926037889),
    // Forest Temple: 6 of 6.
    holder("boss-heart", 18379569660496940670),
    // Queen Gohma's reward: no trigger yet.
    holder("dungeon-reward", 7976713952680358969),
    // No trigger yet.
    holder("master-sword", 6465850391777915933),
];

/// The small chests `items` puts one item kind each in: the 20 the nights
/// passed most, none ever failing.
pub const SMALL_CHESTS: [u64; ITEMS.len()] = [
    18277713685010400796,
    14404733996390762670,
    11929552275106311978,
    13027331340815880287,
    196331300747102419,
    3949488292768750868,
    16785682308850448108,
    10696620323243142689,
    12567412825200613393,
    9634924691849556346,
    12945617941305968586,
    10521366928524045067,
    1171460703078800858,
    9911316961118170383,
    1883068192328530224,
    4320448574091221496,
    2503861086004941173,
    12042193296697281461,
    1111717451118505545,
    8838426083116998768,
];

/// A kind of item and the one item that stands for it.
#[derive(Debug)]
pub struct ItemKind {
    pub name: &'static str,
    pub group: Group,
    /// The one `quick` runs from its group.
    pub quick: bool,
    item: fn() -> String,
}

impl ItemKind {
    /// The item as the pool names it.
    pub fn item(&self) -> String {
        (self.item)()
    }
}

const fn kind(name: &'static str, group: Group, quick: bool, item: fn() -> String) -> ItemKind {
    ItemKind {
        name,
        group,
        quick,
        item,
    }
}

/// The dungeon whose keys stand for every dungeon's: the first with a boss
/// key.
const KEYS_OF: usize = 3;

/// Every item kind, as the spec lists them.
pub const ITEMS: [ItemKind; 20] = [
    kind("progressive", Group::Gear, true, || {
        item::HOOKSHOT.to_string()
    }),
    kind("major", Group::Gear, false, || item::BOOMERANG.to_string()),
    kind("equipment", Group::Gear, false, || {
        actor::OBJ_ARMOR_ZORA.to_string()
    }),
    kind("capacity", Group::Gear, false, || {
        item::WALLET_MEDIUM.to_string()
    }),
    kind("magic", Group::Gear, false, || {
        item::FAIRY_BLESS_MP.to_string()
    }),
    kind("bottle", Group::Gear, false, || item::BOTTLE.to_string()),
    kind("mask", Group::Gear, false, || item::MASK_KEATON.to_string()),
    kind("song", Group::Quest, true, || {
        item::MELODY_ZELDA_LULLABY.to_string()
    }),
    kind("dungeon-reward", Group::Quest, false, || {
        item::MEDAL_FOREST.to_string()
    }),
    kind("trade", Group::Quest, false, || {
        item::POCKET_EGG.to_string()
    }),
    kind("token", Group::Quest, false, || {
        item::TOKEN_GOLD.to_string()
    }),
    kind("small-key", Group::Dungeon, true, || {
        key_tints::ring(KEYS_OF)
    }),
    kind("boss-key", Group::Dungeon, false, || {
        key_tints::boss(KEYS_OF)
    }),
    kind("map", Group::Dungeon, false, || {
        item::DUNGEON_MAP.to_string()
    }),
    kind("heart-piece", Group::Health, true, || {
        actor::OBJ_HEART_PIECE.to_string()
    }),
    kind("heart-container", Group::Health, false, || {
        actor::OBJ_HEART_CONTAINER.to_string()
    }),
    kind("rupees", Group::Filler, false, || {
        actor::OBJ_RUPEE_RED.to_string()
    }),
    kind("ammo", Group::Filler, false, || {
        item::BOMB_BUNDLE_SMALL.to_string()
    }),
    kind("bombchus", Group::Filler, false, || {
        item::BOMBCHU_BUNDLE_SMALL.to_string()
    }),
    kind(ICE_TRAP, Group::Filler, true, || {
        ice_traps::POOLED.to_string()
    }),
];

/// One item kind at one holder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cell {
    /// Its place in [`HOLDERS`].
    pub holder_kind: usize,
    /// Its place in [`ITEMS`].
    pub item_kind: usize,
    /// The placement that holds it: its kind's, or one of
    /// [`SMALL_CHESTS`] for `items`.
    pub holder: u64,
}

impl Cell {
    fn at(holder_kind: usize, item_kind: usize) -> Self {
        Self {
            holder_kind,
            item_kind,
            holder: HOLDERS[holder_kind].holder,
        }
    }

    /// `holder-kind × item-kind`, as rows and the report name it.
    pub fn name(&self) -> String {
        let (h, i) = (HOLDERS[self.holder_kind].name, ITEMS[self.item_kind].name);
        format!("{h} × {i}")
    }
}

/// What a gauntlet runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Run {
    /// The tour, then every cell.
    Full,
    /// The tour, then one item kind of each group at every holder kind.
    Quick,
    /// The tour alone.
    Places,
    /// Every holder kind, one item kind each.
    Holders,
    /// Every item kind, each in a small chest of its own.
    Items,
}

impl Run {
    /// Whether it loads every place first.
    pub fn tours(self) -> bool {
        matches!(self, Run::Full | Run::Quick | Run::Places)
    }

    /// Its cells, before any filter.
    pub fn cells(self) -> Vec<Cell> {
        let every = || (0..HOLDERS.len()).flat_map(|h| (0..ITEMS.len()).map(move |i| (h, i)));
        match self {
            Run::Places => vec![],
            Run::Full => every().map(|(h, i)| Cell::at(h, i)).collect(),
            Run::Quick => (every().filter(|&(_, i)| ITEMS[i].quick))
                .map(|(h, i)| Cell::at(h, i))
                .collect(),
            // The full grid's first round: holder kind h holds item kind
            // h mod 20.
            Run::Holders => (0..HOLDERS.len())
                .map(|h| Cell::at(h, h % ITEMS.len()))
                .collect(),
            Run::Items => (0..ITEMS.len())
                .map(|i| Cell {
                    holder: SMALL_CHESTS[i],
                    ..Cell::at(SMALL_CHEST, i)
                })
                .collect(),
        }
    }
}

/// The ice trap's item kind: it passes when its get names its disguise.
pub const ICE_TRAP: &str = "ice-trap";

/// [`HOLDERS`]' small chest, which `items` fills.
const SMALL_CHEST: usize = 0;

/// The place in `names` of each of `picked`, or which one isn't there.
fn places_of(picked: &[String], names: &[&str], what: &str) -> Result<BTreeSet<usize>> {
    let mut out = BTreeSet::new();
    for p in picked {
        match names.iter().position(|n| n == p) {
            Some(i) => _ = out.insert(i),
            None => bail!("no {what} kind {p}: {}", names.join(", ")),
        }
    }
    Ok(out)
}

/// `cells` with only the holder kinds and item kinds named, all of either
/// when none is.
pub fn filtered(cells: Vec<Cell>, holders: &[String], items: &[String]) -> Result<Vec<Cell>> {
    let h = places_of(holders, &HOLDERS.map(|k| k.name), "holder")?;
    let i = places_of(items, &ITEMS.map(|k| k.name), "item")?;
    let keep = |c: &Cell| {
        (h.is_empty() || h.contains(&c.holder_kind)) && (i.is_empty() || i.contains(&c.item_kind))
    };
    Ok(cells.into_iter().filter(keep).collect())
}

/// `cells` dealt into rounds, at most one a holder a round: each into the
/// first round without its holder, taken in Latin square order (holder
/// kind h holds item kind `(h + r) mod 20` in round r), so the full grid
/// runs in 20 rounds and cells at holders of their own share one.
pub fn rounds(cells: &[Cell]) -> Vec<Vec<Cell>> {
    let n = ITEMS.len();
    let mut ordered = cells.to_vec();
    ordered.sort_by_key(|c| ((c.item_kind + n - c.holder_kind % n) % n, c.holder_kind));
    let mut out: Vec<Vec<Cell>> = vec![];
    for c in ordered {
        match out
            .iter_mut()
            .find(|r| r.iter().all(|o| o.holder != c.holder))
        {
            Some(round) => round.push(c),
            None => out.push(vec![c]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_grid_runs_every_cell_once_in_twenty_rounds() {
        let cells = Run::Full.cells();
        assert_eq!(cells.len(), 460);
        let rounds = rounds(&cells);
        assert_eq!(rounds.len(), 20);
        let mut seen: Vec<Cell> = rounds.iter().flatten().copied().collect();
        seen.sort();
        let mut want = cells.clone();
        want.sort();
        assert_eq!(seen, want);
        for r in &rounds {
            let holders: BTreeSet<u64> = r.iter().map(|c| c.holder).collect();
            assert_eq!(holders.len(), r.len(), "a holder twice in a round");
        }
    }

    #[test]
    fn each_run_has_its_cells() {
        assert_eq!(Run::Quick.cells().len(), 115);
        assert!(Run::Places.cells().is_empty());
        let holders = Run::Holders.cells();
        assert_eq!(holders.len(), 23);
        assert_eq!(rounds(&holders).len(), 1);
        let items = Run::Items.cells();
        assert_eq!(items.len(), 20);
        assert!(items.iter().all(|c| c.holder_kind == SMALL_CHEST));
        assert_eq!(rounds(&items).len(), 1, "a chest of its own each");
        assert!(Run::Places.tours() && !Run::Items.tours());
    }

    #[test]
    fn quick_runs_one_item_of_each_group() {
        let groups: Vec<Group> = ITEMS.iter().filter(|k| k.quick).map(|k| k.group).collect();
        assert_eq!(
            groups,
            [
                Group::Gear,
                Group::Quest,
                Group::Dungeon,
                Group::Health,
                Group::Filler
            ]
        );
    }

    #[test]
    fn filters_keep_a_row_or_a_column() {
        let pots = filtered(Run::Full.cells(), &["pot".into()], &[]).unwrap();
        assert_eq!(pots.len(), 20);
        let one = filtered(Run::Full.cells(), &["pot".into()], &["song".into()]).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].name(), "pot × song");
        let err = filtered(Run::Full.cells(), &["fake-holder".into()], &[]).unwrap_err();
        assert!(err.to_string().contains("small-chest"), "{err}");
    }

    #[test]
    fn kinds_and_holders_are_unique() {
        let names: BTreeSet<_> = HOLDERS.iter().map(|k| k.name).collect();
        assert_eq!(names.len(), HOLDERS.len());
        let items: BTreeSet<_> = ITEMS.iter().map(|k| k.name).collect();
        assert_eq!(items.len(), ITEMS.len());
        let chests: BTreeSet<_> = SMALL_CHESTS.iter().collect();
        assert_eq!(chests.len(), SMALL_CHESTS.len());
    }
}
