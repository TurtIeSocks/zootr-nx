//! What Link has: the item tracker's grid, filled from a save or from the
//! watcher's live changes. See `docs/research/saves.md`.

use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::{Romfs, Save, murmur3};
use serde::Serialize;

use crate::bombchu_bags;
use crate::checks::{self, ItemCheck};
use crate::gamedata::GameData;
use crate::keys;
use crate::known::{Name, actor, field, flag, flow, item};
use crate::savefile::Change;

mod edit;
mod glyph;
mod layout;
pub use edit::{Intent, Pouch};
pub use glyph::Glyph;
pub use layout::{COLUMNS, IconSize, Placement, Section, TrackerOptions};

const POUCH: &str = "PouchData";
const STOCK: &Name = flag::STOCK_NUM;

/// One cell of the grid.
struct Slot {
    section: Section,
    label: &'static str,
    /// What fills it, lowest tier first. Held is a `StockNum` above 0, or
    /// `IsGet` for what the pouch doesn't keep.
    tiers: &'static [&'static Name],
    /// Drawn instead of the held tier.
    icon: Option<&'static Name>,
    /// Shown as a number.
    count: Option<&'static Name>,
}

const fn slot(section: Section, label: &'static str, tiers: &'static [&'static Name]) -> Slot {
    Slot {
        section,
        label,
        tiers,
        icon: None,
        count: None,
    }
}

const fn ammo(
    label: &'static str,
    tiers: &'static [&'static Name],
    icon: Option<&'static Name>,
    count: &'static Name,
) -> Slot {
    Slot {
        icon,
        count: Some(count),
        ..slot(Section::Inventory, label, tiers)
    }
}

const fn bottle(label: &'static str, tiers: &'static [&'static Name]) -> Slot {
    Slot {
        icon: Some(item::BOTTLE),
        ..slot(Section::Inventory, label, tiers)
    }
}

use Section::*;

/// The pause screen's order, row by row.
#[rustfmt::skip]
const GRID: &[Slot] = &[
    ammo("Deku Sticks", &[item::STICK_BAG_SMALL, item::STICK_BAG_MEDIUM, item::STICK_BAG_LARGE], Some(item::DEKU_STICK), item::DEKU_STICK),
    ammo("Deku Nuts", &[item::NUT_BAG_SMALL, item::NUT_BAG_MEDIUM, item::NUT_BAG_LARGE], Some(item::DEKU_NUT), item::DEKU_NUT),
    ammo("Bombs", &[item::BOMB_BAG_SMALL, item::BOMB_BAG_MEDIUM, item::BOMB_BAG_LARGE], Some(item::BOMB), item::BOMB),
    ammo("Fairy Bow", &[actor::WEAPON_RANGE_BOW], None, item::ARROW_NORMAL),
    slot(Inventory, "Fire Arrow", &[item::ARROW_FIRE]),
    slot(Inventory, "Din's Fire", &[item::DIN_FIRE]),
    ammo("Slingshot", &[actor::WEAPON_RANGE_SLING_SHOT], None, item::DEKU_SEED),
    slot(Inventory, "Ocarina", &[item::OCARINA_FAIRY, item::OCARINA_TIME]),
    ammo("Bombchus", &[item::BOMBCHU], None, item::BOMBCHU),
    slot(Inventory, "Hookshot", &[item::HOOKSHOT, item::LONGSHOT]),
    slot(Inventory, "Ice Arrow", &[item::ARROW_ICE]),
    slot(Inventory, "Farore's Wind", &[item::FARORE_WIND]),
    slot(Inventory, "Boomerang", &[item::BOOMERANG]),
    slot(Inventory, "Lens of Truth", &[actor::ARMOR_GLASS_TRUTH]),
    ammo("Magic Beans", &[item::MAGIC_BEAN], None, item::MAGIC_BEAN),
    slot(Inventory, "Megaton Hammer", &[item::HAMMER]),
    slot(Inventory, "Light Arrow", &[item::ARROW_LIGHT]),
    slot(Inventory, "Nayru's Love", &[item::NAYRU_LOVE]),
    bottle("Bottle", &[field::BOTTLE_A]),
    bottle("Bottle", &[field::BOTTLE_B]),
    bottle("Bottle", &[field::BOTTLE_C]),
    bottle("Bottle", &[field::BOTTLE_D]),
    bottle("Bottle", &[field::BOTTLE_E]),
    ammo("Business Scrub Seeds", &[item::SCRUB_BUSINESS_SEED], None, item::SCRUB_BUSINESS_SEED),
    // Link can hold every trade item at once, so each has a cell.
    slot(Trade, "Weird Egg", &[item::WEIRD_EGG]),
    slot(Trade, "Cucco", &[item::ALARM_CUCCO]),
    slot(Trade, "Zelda's Letter", &[item::ZELDA_LETTER]),
    slot(Trade, "Pocket Egg", &[item::POCKET_EGG]),
    slot(Trade, "Pocket Cucco", &[item::POCKET_CUCCO]),
    slot(Trade, "Cojiro", &[item::COJIRO_CUCCO]),
    slot(Trade, "Odd Mushroom", &[item::ODD_MUSHROOM]),
    slot(Trade, "Odd Potion", &[item::ODD_POWDER]),
    slot(Trade, "Poacher's Saw", &[item::POACHER_SAW]),
    slot(Trade, "Broken Goron's Sword", &[item::BROKEN_SWORD]),
    slot(Trade, "Prescription", &[item::PRESCRIPTION]),
    slot(Trade, "Eyeball Frog", &[item::EYEBALL_FROG]),
    slot(Trade, "Eye Drops", &[item::EYE_DROP]),
    slot(Trade, "Claim Check", &[item::CLAIM_CHECK]),
    // Link can hold every mask at once (the mask slot), so each has a cell.
    slot(Masks, "Keaton Mask", &[actor::ARMOR_CHILD_MASK_KEATON]),
    slot(Masks, "Skull Mask", &[actor::ARMOR_CHILD_MASK_SKULL]),
    slot(Masks, "Spooky Mask", &[actor::ARMOR_CHILD_MASK_SPOOKY]),
    slot(Masks, "Bunny Hood", &[actor::ARMOR_CHILD_MASK_BUNNY]),
    slot(Masks, "Goron Mask", &[actor::ARMOR_CHILD_MASK_GORON]),
    slot(Masks, "Zora Mask", &[actor::ARMOR_CHILD_MASK_ZORA]),
    slot(Masks, "Gerudo Mask", &[actor::ARMOR_CHILD_MASK_GERUDO]),
    slot(Masks, "Mask of Truth", &[actor::ARMOR_CHILD_MASK_TRUTH]),
    slot(Equipment, "Kokiri Sword", &[actor::WEAPON_SWORD_KOKIRI]),
    slot(Equipment, "Master Sword", &[actor::WEAPON_SWORD_MASTER]),
    slot(Equipment, "Biggoron's Sword", &[actor::WEAPON_SWORD_GIANT, actor::WEAPON_SWORD_BIGGORON]),
    slot(Equipment, "Deku Shield", &[actor::WEAPON_SHIELD_DEKU]),
    slot(Equipment, "Hylian Shield", &[actor::WEAPON_SHIELD_HYLIAN]),
    slot(Equipment, "Mirror Shield", &[actor::WEAPON_SHIELD_MIRROR]),
    slot(Equipment, "Kokiri Tunic", &[actor::ARMOR_ADULT_NORMAL]),
    slot(Equipment, "Goron Tunic", &[actor::ARMOR_ADULT_GORON]),
    slot(Equipment, "Zora Tunic", &[actor::ARMOR_ADULT_ZORA]),
    slot(Equipment, "Iron Boots", &[actor::ARMOR_ADULT_HEAVY_BOOT]),
    slot(Equipment, "Hover Boots", &[actor::ARMOR_ADULT_HOVER_BOOT]),
    slot(Upgrades, "Strength", &[actor::ARMOR_CHILD_GAUNTLET_GORON, actor::ARMOR_ADULT_GAUNTLET_SILVER, actor::ARMOR_ADULT_GAUNTLET_GOLD]),
    slot(Upgrades, "Scale", &[item::SCALE_SILVER, item::SCALE_GOLD]),
    slot(Upgrades, "Wallet", &[item::WALLET_SMALL, item::WALLET_MEDIUM, item::WALLET_LARGE]),
    slot(Upgrades, "Bomb Bag", &[item::BOMB_BAG_SMALL, item::BOMB_BAG_MEDIUM, item::BOMB_BAG_LARGE]),
    Slot { icon: Some(item::BOMBCHU), ..slot(Upgrades, "Bombchu Bag", &[bombchu_bags::SMALL, bombchu_bags::MEDIUM, bombchu_bags::LARGE]) },
    slot(Upgrades, "Quiver", &[item::QUIVER_SMALL, item::QUIVER_MEDIUM, item::QUIVER_LARGE]),
    slot(Upgrades, "Bullet Bag", &[item::BULLET_BAG_SMALL, item::BULLET_BAG_MEDIUM, item::BULLET_BAG_LARGE]),
    slot(Upgrades, "Stick Capacity", &[item::STICK_BAG_SMALL, item::STICK_BAG_MEDIUM, item::STICK_BAG_LARGE]),
    slot(Upgrades, "Nut Capacity", &[item::NUT_BAG_SMALL, item::NUT_BAG_MEDIUM, item::NUT_BAG_LARGE]),
    Slot { count: Some(actor::OBJ_HEART_PIECE), ..slot(Quest, "Heart Pieces", &[actor::OBJ_HEART_PIECE]) },
    slot(Quest, "Magic", &[crate::magic::meter::METER, item::FAIRY_BLESS_MP_DOUBLE]),
    slot(Quest, "Improved Spin", &[item::FAIRY_BLESS_MP]),
    slot(Quest, "Double Defense", &[item::FAIRY_BLESS_DEFENSE_DOUBLE]),
    slot(Quest, "Gerudo Card", &[item::TOKEN_GERUDO]),
    slot(Quest, "Stone of Agony", &[item::AGONY_STONE]),
    Slot { count: Some(item::TOKEN_GOLD), ..slot(Quest, "Gold Skulltulas", &[item::TOKEN_GOLD]) },
    slot(Songs, "Zelda's Lullaby", &[item::MELODY_ZELDA_LULLABY]),
    slot(Songs, "Epona's Song", &[item::MELODY_EPONA_SONG]),
    slot(Songs, "Saria's Song", &[item::MELODY_SARIA_SONG]),
    slot(Songs, "Sun's Song", &[item::MELODY_SUN_SONG]),
    slot(Songs, "Song of Time", &[item::MELODY_TIME_SONG]),
    slot(Songs, "Song of Storms", &[item::MELODY_STORM_SONG]),
    slot(Songs, "Minuet of Forest", &[item::MELODY_FOREST_MINUET]),
    slot(Songs, "Bolero of Fire", &[item::MELODY_FIRE_BOLERO]),
    slot(Songs, "Serenade of Water", &[item::MELODY_WATER_SERENADE]),
    slot(Songs, "Requiem of Spirit", &[item::MELODY_SPIRIT_REQUIEM]),
    slot(Songs, "Nocturne of Shadow", &[item::MELODY_SHADOW_NOCTURNE]),
    slot(Songs, "Prelude of Light", &[item::MELODY_LIGHT_PRELUDE]),
    slot(Stones, "Kokiri Emerald", &[item::STONE_FOREST]),
    slot(Stones, "Goron Ruby", &[item::STONE_FIRE]),
    slot(Stones, "Zora Sapphire", &[item::STONE_WATER]),
    slot(Medallions, "Forest", &[item::MEDAL_FOREST]),
    slot(Medallions, "Fire", &[item::MEDAL_FIRE]),
    slot(Medallions, "Water", &[item::MEDAL_WATER]),
    slot(Medallions, "Spirit", &[item::MEDAL_SPIRIT]),
    slot(Medallions, "Shadow", &[item::MEDAL_SHADOW]),
    slot(Medallions, "Light", &[item::MEDAL_LIGHT]),
];

/// The tiers of grid cell `at`, lowest first: none past the grid.
pub fn tiers_at(at: usize) -> &'static [&'static Name] {
    GRID.get(at).map_or(&[], |s| s.tiers)
}

/// One actor of the grid, as a new game can start holding it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub actor: &'static Name,
    /// A count that isn't one of its cell's tiers, like the bombs a bag
    /// holds. A count that is, like a Gold Skulltula's, is the item itself.
    pub ammo: bool,
}

/// Every actor in the grid once, in its order: each cell's tiers lowest
/// first, then its count.
pub fn held() -> Vec<Held> {
    let mut out: Vec<Held> = Vec::new();
    for s in GRID {
        let tiers = s.tiers.iter().map(|&actor| Held { actor, ammo: false });
        let count =
            (s.count.filter(|c| !s.tiers.contains(c))).map(|actor| Held { actor, ammo: true });
        for h in tiers.chain(count) {
            if !out.iter().any(|o| o.actor == h.actor) {
                out.push(h);
            }
        }
    }
    out
}

/// The section of the cell `actor` fills.
pub fn section(actor: &Name) -> Option<Section> {
    let cell = GRID.iter().find(|s| s.tiers.contains(&actor));
    cell.map(|s| s.section)
}

/// The tiers below `actor` in its cell, which holding it holds too.
pub fn below(actor: &Name) -> Vec<&'static Name> {
    let tiers = tiers_of(actor);
    tiers.iter().take_while(|&&t| t != actor).copied().collect()
}

/// Every tier of the cell `actor` fills, lowest first.
pub fn tiers_of(actor: &Name) -> &'static [&'static Name] {
    let cell = GRID.iter().find(|s| s.tiers.contains(&actor));
    cell.map_or(&[], |s| s.tiers)
}

/// The dungeons the Dungeons section has a row for, in game order: the
/// `DungeonInfo` member, which is also the dungeon's Banc folder, its name,
/// and the short name randomizer players use.
const DUNGEONS: [(&Name, &str, &str); 12] = [
    (flag::INSIDE_THE_DEKU_TREE, "Deku Tree", "DT"),
    (flag::DODONGO_CAVERN, "Dodongo's Cavern", "DC"),
    (flag::INSIDE_JABU_JABU_BELLY, "Jabu-Jabu's Belly", "JB"),
    (flag::FOREST_TEMPLE, "Forest Temple", "FoT"),
    (flag::FIRE_TEMPLE, "Fire Temple", "FiT"),
    (flag::WATER_TEMPLE, "Water Temple", "WaT"),
    (flag::SHADOW_TEMPLE, "Shadow Temple", "ShT"),
    (flag::SPIRIT_TEMPLE, "Spirit Temple", "SpT"),
    (flow::BOTTOM_OF_THE_WELL, "Bottom of the Well", "BotW"),
    (flag::ICE_CAVERN, "Ice Cavern", "IC"),
    (
        flag::GERUDO_TRAINING_GROUND,
        "Gerudo Training Ground",
        "GTG",
    ),
    (flag::INSIDE_GANON_CASTLE, "Ganon's Castle", "GC"),
];

/// The Fortress's row, which shows its keys alone.
const FORTRESS: (&Name, &str, &str) = (keys::FORTRESS.name, "Gerudo Fortress", "GF");

/// Rows in the Dungeons section.
const ROWS: usize = DUNGEONS.len() + 1;

/// Every icon name a cell can carry, for UIs that draw their own.
pub fn icons() -> impl Iterator<Item = &'static str> {
    let grid = GRID.iter().flat_map(|s| match &s.icon {
        Some(icon) => std::slice::from_ref(icon),
        None => s.tiers,
    });
    grid.copied()
        .chain(KEPT.map(|k| k.1))
        .chain(crate::save_editor::STATS.map(|s| s.1))
        .map(|icon| icon.get())
}

/// What a row keeps, one cell each: the `DungeonInfo` member that says it,
/// the item that gives it, and its name. The first is a count, the others
/// are held or not.
static KEPT: LazyLock<[(&str, &Name, &str); 4]> = LazyLock::new(|| {
    [
        (flag::SMALL_KEY_NUM, keys::SMALL, "Small Keys"),
        ("IsGetBossKey", keys::BOSS, "Boss Key"),
        ("IsGetMap", item::DUNGEON_MAP, "Map"),
        ("IsGetCompass", item::DUNGEON_COMPASS, "Compass"),
    ]
});

/// One cell of a dungeon's row.
#[derive(Clone)]
struct Kept {
    label: String,
    /// The first cell's, which starts the row.
    row: Option<DungeonRow>,
    icon: &'static Name,
    /// Counts keys, where the others are held or not.
    small: bool,
    /// The member's hash, or none where the dungeon has no such item and
    /// the cell stays empty.
    key: Option<u32>,
}

/// Every cell of every row, in order. A dungeon has an item when one of its
/// vanilla chests holds it. The Fortress has its keys alone.
fn kept(found: &[ItemCheck]) -> Vec<Kept> {
    let has = |area: &str, item: &str| found.iter().any(|c| c.area == area && c.item == item);
    let rows = DUNGEONS.into_iter().chain([FORTRESS]);
    rows.flat_map(|(member, name, short)| {
        KEPT.into_iter()
            .enumerate()
            .map(move |(i, (field, item, what))| {
                let fortress = member == FORTRESS.0;
                let here = match fortress {
                    true => i == 0,
                    false => has(member, item),
                };
                Kept {
                    label: format!("{name} {what}"),
                    row: (i == 0).then(|| DungeonRow {
                        short: short.into(),
                        name: name.into(),
                    }),
                    icon: item,
                    small: i == 0,
                    key: here.then(|| murmur3(format!("DungeonInfo.{member}.{field}").as_bytes())),
                }
            })
    })
    .collect()
}

/// One drawn cell.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Cell {
    pub section: Section,
    pub label: String,
    /// The icon's name: the held tier, else the first.
    pub icon: String,
    /// What draws in its place when the game's art is off.
    pub glyph: Glyph,
    /// 0 when missing, else which tier is held, from 1.
    pub tier: u8,
    /// 0 where a dungeon has no such item, drawn fainter than missing.
    pub tiers: u8,
    pub count: Option<i32>,
    /// Set on a row's first cell, in a section laid out in rows.
    pub row: Option<DungeonRow>,
    /// Ours, and not in this save or seed: drawn nowhere.
    pub hidden: bool,
}

/// The dungeon a row of the Dungeons section is for.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DungeonRow {
    /// What starts the row, such as "FoT".
    pub short: String,
    /// The full name, such as "Forest Temple".
    pub name: String,
}

#[derive(Clone, Default)]
pub struct Inventory {
    /// Each actor the grid reads, to the value that says it's held.
    keys: HashMap<&'static Name, u32>,
    /// The Dungeons section's cells.
    dungeons: Vec<Kept>,
    values: HashMap<u32, i32>,
    /// Each pouch count's `IsGet` flag, where the game has one. Its menus
    /// list only an item whose flag is on.
    owned: HashMap<u32, u32>,
}

impl Inventory {
    /// The grid for `romfs`, whose chests say which dungeon has what.
    pub fn new(gd: &GameData, romfs: &Romfs) -> Result<Self> {
        Self::build(gd, &checks::scan(romfs)?)
    }

    fn build(gd: &GameData, found: &[ItemCheck]) -> Result<Self> {
        let stock: HashMap<u32, u32> = stock_keys(gd)?.into_iter().collect();
        let key = |actor: &str| match stock.get(&murmur3(actor.as_bytes())) {
            Some(&k) => k,
            None => murmur3(format!("IsGet.{actor}").as_bytes()),
        };
        let actors = || GRID.iter().flat_map(|s| s.tiers.iter().chain(&s.count));
        let keys = actors().map(|&a| (a, key(a))).collect();
        let owned = actors()
            .filter_map(|a| {
                let count = *stock.get(&murmur3(a.as_bytes()))?;
                let flag = murmur3(format!("IsGet.{a}").as_bytes());
                gd.kind(flag).map(|_| (count, flag))
            })
            .collect();
        Ok(Self {
            keys,
            dungeons: kept(found),
            values: HashMap::new(),
            owned,
        })
    }

    /// Every hash the grid reads.
    fn hashes(&self) -> impl Iterator<Item = u32> + '_ {
        let kept = self.dungeons.iter().filter_map(|k| k.key);
        self.keys.values().copied().chain(kept)
    }

    /// Takes every value from `value`, a save or values read live.
    pub fn load(&mut self, value: impl Fn(u32) -> Option<u32>) {
        self.values = (self.hashes())
            .filter_map(|k| Some((k, value(k)? as i32)))
            .collect();
    }

    /// Takes one change the watcher saw. True when the grid changed.
    pub fn change(&mut self, c: &Change) -> bool {
        if !self.hashes().any(|k| k == c.hash) {
            return false;
        }
        let Some((_, after)) = c.change.rsplit_once("-> ") else {
            return false;
        };
        let value = match after {
            "true" => 1,
            "false" => 0,
            n => n.parse().unwrap_or_default(),
        };
        self.values.insert(c.hash, value) != Some(value)
    }

    /// Whether `slot` is of our own items and the loaded values hold none
    /// of them, as a save or seed without them.
    fn absent(&self, slot: &Slot) -> bool {
        let ours = |a: &&Name| crate::known::is_ours(a.get());
        let loaded = |a: &&Name| {
            self.keys
                .get(a)
                .is_some_and(|k| self.values.contains_key(k))
        };
        slot.tiers.iter().all(|a| ours(a) && !loaded(a))
    }

    fn value(&self, actor: &Name) -> i32 {
        (self.keys.get(actor))
            .and_then(|k| self.values.get(k))
            .copied()
            .unwrap_or_default()
    }

    pub fn cells(&self) -> Vec<Cell> {
        let dungeons = self.dungeons.iter().map(|k| {
            let value = k
                .key
                .map_or(0, |key| self.values.get(&key).copied().unwrap_or(0));
            Cell {
                section: Section::Dungeons,
                label: k.label.clone(),
                icon: k.icon.to_string(),
                glyph: Glyph::of(k.icon),
                tier: u8::from(value > 0),
                tiers: u8::from(k.key.is_some()),
                count: (k.small && k.key.is_some()).then_some(value),
                row: k.row.clone(),
                hidden: false,
            }
        });
        GRID.iter()
            .map(|s| {
                let tier = self.tier(s);
                let held = s.tiers[tier.saturating_sub(1)];
                let icon = s.icon.unwrap_or(held);
                Cell {
                    section: s.section,
                    label: s.label.into(),
                    icon: icon.to_string(),
                    glyph: Glyph::of(icon),
                    tier: tier as u8,
                    tiers: s.tiers.len() as u8,
                    count: s.count.map(|a| self.value(a)),
                    row: None,
                    hidden: self.absent(s),
                }
            })
            .chain(dungeons)
            .collect()
    }
}

/// A save's raw scalars.
pub fn in_save(save: &Save) -> impl Fn(u32) -> Option<u32> + '_ {
    |h| save.get(h).and_then(|e| save.scalar(e))
}

/// Each pouch actor's hash and the hash of its `StockNum` Int.
pub fn stock_keys(gd: &GameData) -> Result<Vec<(u32, u32)>> {
    let stock = murmur3(STOCK.as_bytes());
    let mut keys = Vec::new();
    for (actor, entry) in gd.members(murmur3(POUCH.as_bytes()))? {
        let found = gd.members(entry)?.into_iter().find(|&(m, _)| m == stock);
        keys.push((
            actor,
            found
                .with_context(|| format!("{actor:08x} has no {STOCK}"))?
                .1,
        ));
    }
    Ok(keys)
}

/// The hash of `actor`'s pouch count, when it has one. Songs and the like
/// are owned by their `IsGet` alone.
pub fn stock(gd: &GameData, actor: &str) -> Result<Option<u32>> {
    let hash = murmur3(actor.as_bytes());
    Ok((stock_keys(gd)?.into_iter()).find_map(|(a, s)| (a == hash).then_some(s)))
}

/// Whether the game has an `IsGet` flag for `actor`.
pub fn has_flag(gd: &GameData, actor: &str) -> bool {
    gd.kind(murmur3(format!("IsGet.{actor}").as_bytes()))
        .is_some()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::known::{actor, field, flag, item};

    /// The masks, in the game's order.
    const MASKS: [&Name; 8] = [
        actor::ARMOR_CHILD_MASK_KEATON,
        actor::ARMOR_CHILD_MASK_SKULL,
        actor::ARMOR_CHILD_MASK_SPOOKY,
        actor::ARMOR_CHILD_MASK_BUNNY,
        actor::ARMOR_CHILD_MASK_GORON,
        actor::ARMOR_CHILD_MASK_ZORA,
        actor::ARMOR_CHILD_MASK_GERUDO,
        actor::ARMOR_CHILD_MASK_TRUTH,
    ];

    /// Every trade item, child then adult, as the Trade section lists them.
    const TRADE: [&Name; 14] = [
        item::WEIRD_EGG,
        item::ALARM_CUCCO,
        item::ZELDA_LETTER,
        item::POCKET_EGG,
        item::POCKET_CUCCO,
        item::COJIRO_CUCCO,
        item::ODD_MUSHROOM,
        item::ODD_POWDER,
        item::POACHER_SAW,
        item::BROKEN_SWORD,
        item::PRESCRIPTION,
        item::EYEBALL_FROG,
        item::EYE_DROP,
        item::CLAIM_CHECK,
    ];

    #[test]
    fn each_trade_item_has_a_cell_of_its_own_in_trade() {
        let trade: Vec<_> = GRID.iter().filter(|s| s.section == Trade).collect();
        let held: Vec<_> = trade.iter().map(|s| s.tiers.to_vec()).collect();
        assert_eq!(held, TRADE.map(|a| vec![a]));
    }

    #[test]
    fn each_mask_has_a_cell_of_its_own_in_masks() {
        let masks: Vec<_> = (GRID.iter())
            .filter(|s| s.section == Masks)
            .map(|s| s.tiers.to_vec())
            .collect();
        assert_eq!(masks, MASKS.map(|a| vec![a]));
        let elsewhere = (GRID.iter())
            .filter(|s| s.section != Masks)
            .any(|s| MASKS.iter().any(|m| s.tiers.contains(m)));
        assert!(!elsewhere);
    }

    #[test]
    fn the_grid_has_a_cell_for_each_of_the_five_bottles() {
        let bottles: Vec<_> = (GRID.iter())
            .filter(|s| s.icon == Some(item::BOTTLE))
            .map(|s| s.tiers.to_vec())
            .collect();
        let slots = [
            field::BOTTLE_A,
            field::BOTTLE_B,
            field::BOTTLE_C,
            field::BOTTLE_D,
            field::BOTTLE_E,
        ];
        assert_eq!(bottles, slots.map(|b| vec![b]));
    }

    /// Keys as if every actor were its own hash.
    pub(super) fn inventory() -> Inventory {
        inventory_of(&[])
    }

    /// `inventory`, with the dungeons that have the `(area, item)`s.
    pub(super) fn inventory_of(has: &[(&Name, &Name)]) -> Inventory {
        let keys = GRID
            .iter()
            .flat_map(|s| s.tiers.iter().chain(&s.count))
            .map(|&a| (a, murmur3(a.as_bytes())))
            .collect();
        let found: Vec<_> = (has.iter())
            .map(|&(area, item)| crate::gifts::check(String::new(), area, 1, "Chest".into(), item))
            .collect();
        Inventory {
            keys,
            dungeons: kept(&found),
            ..Default::default()
        }
    }

    fn dungeon_hash(member: &str, field: &str) -> u32 {
        murmur3(format!("DungeonInfo.{member}.{field}").as_bytes())
    }

    fn cell<'a>(cells: &'a [Cell], label: &str) -> &'a Cell {
        &cells[cell_at(cells, label)]
    }

    /// Where the cell labelled `label` sits.
    pub(super) fn cell_at(cells: &[Cell], label: &str) -> usize {
        cells.iter().position(|c| c.label == label).unwrap()
    }

    #[test]
    fn a_save_fills_tiers_and_counts() {
        let held = [
            (item::BOMB_BAG_SMALL, 1),
            (item::BOMB_BAG_MEDIUM, 1),
            (item::BOMB, 25),
            (item::LONGSHOT, 1),
            (item::MELODY_SUN_SONG, 1),
            (item::SCRUB_BUSINESS_SEED, 7),
        ];
        // A type marker first, then each value as a scalar.
        let pairs: Vec<_> = std::iter::once((0, 0))
            .chain(held.iter().map(|&(a, v)| (murmur3(a.as_bytes()), v)))
            .collect();
        let save = Save::parse(Save::fixture(&pairs)).unwrap();
        let mut inv = inventory();
        inv.load(in_save(&save));
        let cells = inv.cells();

        let bombs = cell(&cells, "Bombs");
        assert_eq!(
            (bombs.tier, bombs.icon.as_str(), bombs.count),
            (2, item::BOMB.get(), Some(25))
        );
        let bag = cell(&cells, "Bomb Bag");
        assert_eq!(
            (bag.tier, bag.tiers, bag.icon.as_str()),
            (2, 3, item::BOMB_BAG_MEDIUM.get())
        );
        let hook = cell(&cells, "Hookshot");
        assert_eq!((hook.tier, hook.icon.as_str()), (2, item::LONGSHOT.get()));
        assert_eq!(cell(&cells, "Sun's Song").tier, 1);
        let seeds = cell(&cells, "Business Scrub Seeds");
        assert_eq!(
            (seeds.section, seeds.tier, seeds.count),
            (Inventory, 1, Some(7))
        );

        let missing = cell(&cells, "Megaton Hammer");
        assert_eq!(
            (missing.tier, missing.icon.as_str()),
            (0, item::HAMMER.get())
        );
        assert_eq!(cell(&cells, "Slingshot").count, Some(0));
    }

    #[test]
    fn every_cell_draws_a_known_glyph() {
        let held = |names: &[&Name]| {
            let pairs: Vec<_> = std::iter::once((0, 0))
                .chain(names.iter().map(|a| (murmur3(a.as_bytes()), 1)))
                .collect();
            let save = Save::parse(Save::fixture(&pairs)).unwrap();
            let mut inv = inventory();
            inv.load(in_save(&save));
            inv.cells()
        };
        let truth = held(&[actor::ARMOR_CHILD_MASK_TRUTH]);
        assert_eq!(
            cell(&truth, "Mask of Truth").glyph,
            Glyph::of(actor::ARMOR_CHILD_MASK_TRUTH)
        );
        // A bottle slot draws a bottle whatever it holds.
        assert_eq!(cell(&truth, "Bottle").glyph, Glyph::of(item::BOTTLE));
        let map = cell(&truth, "Forest Temple Map");
        assert_eq!(map.glyph, Glyph::of(item::DUNGEON_MAP));
        assert!(truth.iter().all(|c| c.glyph != Glyph::Unknown));
    }

    /// Forest has all four, the Deku Tree a map and a compass.
    const HAS: [(&Name, &Name); 6] = [
        (flag::FOREST_TEMPLE, keys::SMALL),
        (flag::FOREST_TEMPLE, keys::BOSS),
        (flag::FOREST_TEMPLE, item::DUNGEON_MAP),
        (flag::FOREST_TEMPLE, item::DUNGEON_COMPASS),
        (flag::INSIDE_THE_DEKU_TREE, item::DUNGEON_MAP),
        (flag::INSIDE_THE_DEKU_TREE, item::DUNGEON_COMPASS),
    ];

    #[test]
    fn a_save_fills_each_dungeons_row_and_leaves_missing_items_empty() {
        let held = [
            (dungeon_hash(flag::FOREST_TEMPLE, flag::SMALL_KEY_NUM), 3),
            (dungeon_hash(flag::FOREST_TEMPLE, "IsGetBossKey"), 1),
            (dungeon_hash(flag::INSIDE_THE_DEKU_TREE, "IsGetMap"), 1),
        ];
        let pairs: Vec<_> = std::iter::once((0, 0)).chain(held).collect();
        let save = Save::parse(Save::fixture(&pairs)).unwrap();
        let mut inv = inventory_of(&HAS);
        inv.load(in_save(&save));
        let cells = inv.cells();
        assert_eq!(cells.len(), GRID.len() + ROWS * KEPT.len());
        let shown = |label: &str| {
            let c = cell(&cells, label);
            assert_eq!(c.section, Section::Dungeons);
            (c.tier, c.tiers, c.count)
        };
        assert_eq!(shown("Forest Temple Small Keys"), (1, 1, Some(3)));
        assert_eq!(shown("Forest Temple Boss Key"), (1, 1, None));
        assert_eq!(shown("Forest Temple Map"), (0, 1, None));
        assert_eq!(shown("Deku Tree Map"), (1, 1, None));
        assert_eq!(shown("Deku Tree Compass"), (0, 1, None));
        // A dungeon without keys or a boss key leaves those cells empty.
        assert_eq!(shown("Deku Tree Small Keys"), (0, 0, None));
        assert_eq!(shown("Deku Tree Boss Key"), (0, 0, None));
        // The Fortress shows its keys alone.
        assert_eq!(shown("Gerudo Fortress Small Keys"), (0, 1, Some(0)));
        for what in ["Boss Key", "Map", "Compass"] {
            assert_eq!(shown(&format!("Gerudo Fortress {what}")), (0, 0, None));
        }
        let first = cells.iter().find(|c| c.section == Section::Dungeons);
        assert_eq!(first.unwrap().label, "Deku Tree Small Keys");
        // Each row's first cell names its dungeon, and no other cell does.
        let rows: Vec<_> = cells.iter().filter_map(|c| c.row.as_ref()).collect();
        assert_eq!(rows.len(), ROWS);
        let forest = cell(&cells, "Forest Temple Small Keys").row.as_ref();
        assert_eq!(
            forest.map(|r| (r.short.as_str(), r.name.as_str())),
            Some(("FoT", "Forest Temple"))
        );
        assert_eq!(cell(&cells, "Forest Temple Map").row, None);
        assert_eq!(rows.last().unwrap().short, "GF");
    }

    #[test]
    fn live_changes_move_a_dungeons_count_and_flags() {
        let mut inv = inventory_of(&HAS);
        let change = |m: &str, f: &str, kind: &str, c: &str| {
            Change::new(&GameData::default(), dungeon_hash(m, f), kind, c)
        };
        let key = change(flag::FOREST_TEMPLE, flag::SMALL_KEY_NUM, "Int", "0 -> 5");
        assert!(inv.change(&key));
        assert!(!inv.change(&key));
        let key = change(flag::FOREST_TEMPLE, flag::SMALL_KEY_NUM, "Int", "5 -> 4");
        assert!(inv.change(&key));
        let boss = change(flag::FOREST_TEMPLE, "IsGetBossKey", "Bool", "false -> true");
        assert!(inv.change(&boss));
        let cells = inv.cells();
        assert_eq!(cell(&cells, "Forest Temple Small Keys").count, Some(4));
        assert_eq!(cell(&cells, "Forest Temple Boss Key").tier, 1);
        // Nothing reads a cell a dungeon leaves empty.
        let none = change(
            flag::INSIDE_THE_DEKU_TREE,
            flag::SMALL_KEY_NUM,
            "Int",
            "0 -> 1",
        );
        assert!(!inv.change(&none));
        let back = change(flag::FOREST_TEMPLE, "IsGetBossKey", "Bool", "true -> false");
        assert!(inv.change(&back));
        assert_eq!(cell(&inv.cells(), "Forest Temple Boss Key").tier, 0);
    }

    #[test]
    fn live_changes_update_the_grid_once() {
        let mut inv = inventory();
        let hammer = murmur3(item::HAMMER.as_bytes());
        let change = |c: &str| Change::new(&GameData::default(), hammer, "Int", c);
        assert!(inv.change(&change("0 -> 1")));
        assert!(!inv.change(&change("0 -> 1")));
        assert_eq!(cell(&inv.cells(), "Megaton Hammer").tier, 1);
        let other = Change::new(&GameData::default(), 7, "Int", "0 -> 1");
        assert!(!inv.change(&other));
    }

    #[test]
    fn a_pouch_count_is_found_for_items_with_one_and_not_for_songs() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        assert_eq!(stock(&gd, item::BOOMERANG).unwrap(), Some(0xa1d7_6d31));
        assert_eq!(stock(&gd, item::MELODY_SUN_SONG).unwrap(), None);
        assert!(has_flag(&gd, item::MELODY_SUN_SONG));
        assert!(!has_flag(&gd, "Item_A"));
    }

    #[test]
    fn every_grid_actor_has_a_key_in_the_game() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let inv = Inventory::new(&gd, &romfs).unwrap();
        // Ours are added with the seed.
        let missing: Vec<_> = (inv.keys.iter())
            .filter(|(a, k)| gd.entry(**k).is_none() && !crate::known::is_ours(a))
            .map(|(a, _)| *a)
            .collect();
        assert!(
            missing.is_empty(),
            "no {} entry for {missing:?}",
            flag::GAME_DATA
        );
        let lost: Vec<_> = (inv.dungeons.iter())
            .filter(|k| k.key.is_some_and(|k| gd.entry(k).is_none()))
            .map(|k| k.label.as_str())
            .collect();
        assert!(lost.is_empty(), "no {} entry for {lost:?}", flag::GAME_DATA);
    }

    #[test]
    fn the_vanilla_chests_say_which_dungeon_has_which_item() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let cells = Inventory::new(&gd, &romfs).unwrap().cells();
        let has = |row: &str| -> String {
            let found = KEPT.iter().map(|k| cell(&cells, &format!("{row} {}", k.2)));
            found.map(|c| if c.tiers > 0 { 'x' } else { '-' }).collect()
        };
        // Small keys, boss key, map, compass.
        for (row, expected) in [
            ("Deku Tree", "--xx"),
            ("Dodongo's Cavern", "--xx"),
            ("Jabu-Jabu's Belly", "--xx"),
            ("Forest Temple", "xxxx"),
            ("Fire Temple", "xxxx"),
            ("Water Temple", "xxxx"),
            ("Shadow Temple", "xxxx"),
            ("Spirit Temple", "xxxx"),
            ("Bottom of the Well", "x-xx"),
            ("Ice Cavern", "--xx"),
            ("Gerudo Training Ground", "x---"),
            ("Ganon's Castle", "xx--"),
            ("Gerudo Fortress", "x---"),
        ] {
            assert_eq!(has(row), expected, "{row}");
        }
    }

    #[test]
    fn the_dungeon_icons_are_built() {
        let icons: Vec<_> = icons().collect();
        for (_, item, _) in KEPT.iter() {
            assert!(icons.contains(&item.get()), "{item}");
        }
    }
}
