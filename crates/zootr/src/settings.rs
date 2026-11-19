//! The randomizer's options, described once so any UI can render them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use clap::ValueEnum;
use clap::builder::{PossibleValue, PossibleValuesParser};
use serde::Serialize;
use serde_json::Value;

use crate::cutscenes::{AREAS, Skips};
use crate::keys;
use crate::presets::{self, CARPENTERS_FREED, PRESETS};
use crate::starting::{self, ITEMS};

mod choices;
mod new_game;
mod upgrade;
pub use choices::{
    ChestSize, DungeonKeys, FortressKeys, IceTraps, Keys, Logic, ScrubPrice, ShopSlots,
};
pub use new_game::NewGame;
pub(crate) use new_game::{toggles, toggles_on};
pub(crate) use upgrade::upgrade;

/// Setting values by id. A missing id takes its default.
pub type Settings = BTreeMap<String, Value>;

/// The settings panels, by name.
pub mod group {
    /// Comfort and speed: nothing shuffled, nothing opened.
    pub const GENERAL: &str = "General";
    /// How items are placed, and what stands open from the start.
    pub const LOGIC: &str = "Logic & access";
    /// What a new save starts holding or having done.
    pub const INVENTORY: &str = "Inventory";
    /// Which places become checks.
    pub const SHUFFLES: &str = "Shuffles";
    /// What a dungeon or its boss gives, and its keys.
    pub const DUNGEONS: &str = "Dungeons";
    /// Which cutscenes play.
    pub const CUTSCENES: &str = "Cutscenes";
}

/// Every panel, in the order the app lists them.
pub const GROUPS: [&str; 6] = [
    group::GENERAL,
    group::LOGIC,
    group::INVENTORY,
    group::SHUFFLES,
    group::DUNGEONS,
    group::CUTSCENES,
];

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SettingDef {
    pub id: String,
    pub group: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    /// When the parent is on, this one is too, whatever its own value.
    pub parent: Option<&'static str>,
    /// Settings sharing one sit together in a list that starts closed.
    pub subgroup: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Toggle {
        default: bool,
    },
    /// One of `options`, saved by id.
    Choice {
        default: String,
        options: Vec<Choice>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Choice {
    pub id: String,
    pub label: &'static str,
}

/// The id a choice is saved as, its clap name.
pub(crate) fn choice_id(value: impl ValueEnum) -> String {
    value
        .to_possible_value()
        .map(|v| v.get_name().to_owned())
        .unwrap_or_default()
}

fn choice<T: ValueEnum + Copy + Default>(
    id: impl Into<String>,
    group: &'static str,
    label: &'static str,
    description: &'static str,
    labels: fn(T) -> &'static str,
) -> SettingDef {
    let options = T::value_variants()
        .iter()
        .map(|&v| Choice {
            id: choice_id(v),
            label: labels(v),
        })
        .collect();
    SettingDef {
        kind: Kind::Choice {
            default: choice_id(T::default()),
            options,
        },
        ..toggle(id, group, label, description)
    }
}

/// A choice's value, or its default when unset or unknown.
fn chosen<T: ValueEnum + Default>(settings: &Settings, id: &str) -> T {
    settings
        .get(id)
        .and_then(Value::as_str)
        .and_then(|v| T::from_str(v, false).ok())
        .unwrap_or_default()
}

pub(crate) const SKIP_ALL: &str = "skip-cutscenes";
pub(crate) const SKIP_INTRO: &str = "skip-intro";
const SKIP_SARIA: &str = "skip-saria";
const SKIP_OWL: &str = "skip-owl";
const SKIP_GREAT_DEKU: &str = "skip-great-deku";
const SKIP_ARRIVALS: &str = "skip-arrivals";
pub(crate) const CHEST_SIZE: &str = "chest-size";
pub(crate) const LOGIC: &str = "logic";
pub(crate) const SHUFFLE_ITEMS: &str = "shuffle-items";
pub(crate) const CHESTS: &str = "chests";
pub(crate) const PROGRESSIVE: &str = "progressive-items";
const FREE_BOMBCHUS: &str = "free-bombchus";
const BUNNY_HOOD: &str = "bunny-hood";
const QUIET_NAVI: &str = "quiet-navi";
pub(crate) const QUIET_ENEMIES: &str = "quiet-enemies";
pub(crate) const OPEN_DOOR: &str = "open-door-of-time";
pub(crate) const SCRUBS: &str = "scrubs";
const SCRUB_PRICE: &str = "scrub-price";
pub(crate) const NPC_GIFTS: &str = "npc-gifts";
pub(crate) const HUNDRED_TOKENS: &str = "hundred-token-reward";
pub(crate) const DUNGEON_REWARDS: &str = "dungeon-rewards";
pub(crate) const SONGS: &str = "songs";
const BRIDGE_MEDALLIONS: &str = "bridge-medallions";
const HINTS: &str = "hints";
pub(crate) const MAPS_COMPASSES: &str = "maps-compasses";
pub(crate) const SHOPS: &str = "shopsanity";
pub(crate) const SHOP_SLOTS: &str = "shop-slots";
pub(crate) const FREESTANDING: &str = "freestanding";
pub(crate) const SKULLTULAS: &str = "gold-skulltulas";
pub(crate) const BOSS_HEARTS: &str = "boss-hearts";
pub(crate) const POTS: &str = "pots";
pub(crate) const CRATES: &str = "crates";
pub(crate) const ROCKS: &str = "rocks";
pub(crate) const GRASS: &str = "grass";
pub(crate) const HIVES: &str = "hives";
pub(crate) const COWS: &str = "cows";
pub(crate) const KEYS: &str = "keys";
pub(crate) const FORTRESS_KEYS: &str = "fortress-keys";
pub(crate) const ICE_TRAPS: &str = "ice-traps";
pub(crate) const MERCHANTS: &str = "merchants";
pub(crate) const TRADE_CHAIN: &str = "trade-chain";
pub(crate) const RUNNING_RACE: &str = "running-race";

/// The setting `--skip <name>` turns on: `all`, a named one or an area.
pub(crate) fn skip_id(name: &str) -> String {
    match name {
        "all" => SKIP_ALL.into(),
        _ => format!("skip-{name}"),
    }
}

pub fn skip_parser() -> PossibleValuesParser {
    let named = [
        ("all", "Every cutscene below"),
        ("intro", "The opening movie and Navi's wake-up"),
        ("saria", "Saria outside Link's house"),
        ("owl", "Kaepora Gaebora, rides included"),
        (
            "great-deku",
            "The Great Deku Tree's talk and his mouth opening",
        ),
        (
            "arrivals",
            "The camera pan the first time Link reaches an area",
        ),
    ];
    PossibleValuesParser::new(
        named
            .into_iter()
            .chain(AREAS.iter().map(|a| (a.0, a.1)))
            .map(|(name, help)| PossibleValue::new(name).help(help)),
    )
}

fn default_on(def: SettingDef) -> SettingDef {
    SettingDef {
        kind: Kind::Toggle { default: true },
        ..def
    }
}

fn toggle(
    id: impl Into<String>,
    group: &'static str,
    label: &'static str,
    description: &'static str,
) -> SettingDef {
    SettingDef {
        id: id.into(),
        group,
        label,
        description,
        kind: Kind::Toggle { default: false },
        parent: None,
        subgroup: None,
    }
}

/// Settings the CLI takes but the app leaves out for now. Shuffle items
/// off undoes every other shuffle, which no one turning it off expected.
/// Quiet enemies is for the harness's seeds.
const HIDDEN: [&str; 2] = [SHUFFLE_ITEMS, QUIET_ENEMIES];

/// The settings the app shows.
pub fn shown() -> Vec<SettingDef> {
    schema()
        .into_iter()
        .filter(|d| !HIDDEN.contains(&d.id.as_str()))
        .collect()
}

pub fn schema() -> Vec<SettingDef> {
    let preset_defs = PRESETS.iter().map(|p| {
        let def = SettingDef {
            subgroup: p.subgroup,
            ..toggle(p.name, p.group, p.label, p.what)
        };
        match presets::DEFAULT_ON.contains(&p.name) {
            true => default_on(def),
            false => def,
        }
    });
    let cutscene = |id: String, label, description| SettingDef {
        parent: Some(SKIP_ALL),
        ..toggle(id, group::CUTSCENES, label, description)
    };
    let named = [
        default_on(cutscene(
            SKIP_INTRO.into(),
            "Skip the intro",
            "A new game starts with Link in control, without the opening movie and Navi's wake-up",
        )),
        cutscene(
            SKIP_SARIA.into(),
            "Skip Saria's greeting",
            "Link leaves his house without the forest pan or Saria running up to greet him",
        ),
        cutscene(
            SKIP_OWL.into(),
            "Skip Kaepora Gaebora",
            "The owl never stops Link to talk. His rides go too",
        ),
        cutscene(
            SKIP_GREAT_DEKU.into(),
            "Skip meeting the Great Deku Tree",
            "His mouth is open from the start, and the forest quest is where his talk leaves it",
        ),
        default_on(cutscene(
            SKIP_ARRIVALS.into(),
            "Skip area intros",
            "Reaching an area for the first time skips its camera pan. Any quest step the arrival sets still happens",
        )),
    ];
    let start = ITEMS.iter().map(|i| SettingDef {
        subgroup: Some(starting::SUBGROUP),
        ..toggle(
            i.id,
            group::INVENTORY,
            i.label,
            if i.equip {
                "A new game starts with it equipped. Wherever it's found holds a Blue Rupee instead"
            } else {
                "A new game starts with it. Wherever it's found holds a Blue Rupee instead"
            },
        )
    });
    let areas = AREAS.iter().map(|(id, label, _)| SettingDef {
        subgroup: Some("By area"),
        ..cutscene(
            skip_id(id),
            label,
            "Every cutscene here that only plays, changing nothing",
        )
    });
    let mut all = preset_defs
        .chain([choice(
            LOGIC,
            group::LOGIC,
            "Logic",
            "Glitchless places each item where Link can reach it without that item, so a seed can always be beaten. No logic places items anywhere, so a seed may not be",
            Logic::label,
        )])
        .chain([choice(
            ICE_TRAPS,
            group::GENERAL,
            "Ice traps",
            "Swaps junk (rupees, ammo, recovery hearts) for ice traps. Each looks like an item logic needs until Link gets it, then says FOOL! and freezes him",
            IceTraps::label,
        )])
        .chain([choice(
            CHEST_SIZE,
            group::GENERAL,
            "Chest size",
            "All small makes every chest a small one, which opens without the big chest's long animation. What a chest holds doesn't change",
            ChestSize::label,
        )])
        .chain([default_on(toggle(
            SHUFFLE_ITEMS,
            group::SHUFFLES,
            "Shuffle items",
            "Every check gives a shuffled item. Off, each gives what it always does, though a starting item's spot still holds a Blue Rupee",
        ))])
        .chain([default_on(toggle(
            CHESTS,
            group::SHUFFLES,
            "Chests",
            "Each chest is a check. Off, a chest holds what it always does and the other checks shuffle among themselves, though a starting item's chest holds a Blue Rupee",
        ))])
        .chain([default_on(toggle(
                OPEN_DOOR,
                group::LOGIC,
                "Open Door of Time",
                "The Door of Time stands open from the start, so Link can grow up without the stones or the Song of Time. Zelda and Impa stay until he's met them. He skips Rauru, starts with the Light Medallion, and can put the sword back any time",
            ))])
        .chain([default_on(toggle(
                SCRUBS,
                group::SHUFFLES,
                "Business scrubs",
                "Every Business Scrub outside the Dream World sells a check once. Scrubs that sold ammo on repeat sell their one item and leave",
            ))])
        .chain([choice(
            SCRUB_PRICE,
            group::SHUFFLES,
            "Scrub price",
            "What every Business Scrub asks for its check, with Business scrubs on",
            ScrubPrice::label,
        )])
        .chain([toggle(
                NPC_GIFTS,
                group::SHUFFLES,
                "NPC gifts",
                "What people hand over becomes a check: their gifts, minigame prizes, the House of Skulltula's rewards and the Great Fairies' upgrades",
            )])
        .chain([default_on(toggle(
                HUNDRED_TOKENS,
                group::SHUFFLES,
                "100-token reward",
                "With NPC gifts on, the House of Skulltula's reward for all 100 tokens is a check too. Off, it pays its Gold Rupee as always, and logic never sends Link for all 100",
            ))])
        .chain([toggle(
                DUNGEON_REWARDS,
                group::DUNGEONS,
                "Dungeon rewards",
                "The Spiritual Stones and five medallions become checks, given by the scene after each boss. Light stays with Rauru",
            )])
        .chain([toggle(
                SONGS,
                group::SHUFFLES,
                "Songs",
                "Ten songs become checks, given without the lesson: Zelda's Lullaby, Epona's Song, Saria's Song, the Song of Storms and Sheik's six. The Sun's Song, the Song of Time and the Scarecrow's Song stay where they are",
            )])
        .chain([toggle(
            MAPS_COMPASSES,
            group::DUNGEONS,
            "Maps and compasses",
            "Each dungeon's map and compass turn up in a random chest of that dungeon. Keys stay where they are",
        )])
        .chain([choice(
            KEYS,
            group::DUNGEONS,
            "Keys",
            "With Start, Link holds every dungeon's small keys and boss key from the start. With Key rings, each dungeon's small keys come as one ring that opens all its small-key doors. With Keysanity, each small key comes on its own. Rings, small keys and boss keys turn up in random chests and gifts, never in their own dungeon. The 43 chests that held a key become checks. With Start each brings a Red Rupee to the pool. With Key rings, one of a dungeon's rupees becomes its ring and another its boss key. The two keys lying in the open stay where they are",
            DungeonKeys::label,
        )])
        .chain([choice(
            FORTRESS_KEYS,
            group::DUNGEONS,
            "Fortress keys",
            "With Start, Link holds the four Gerudo Fortress keys from the start, so every cell opens whatever the thieves drop. With Keysanity, each key turns up at a random check, anywhere. Either way, the four keys the thieves carry become checks. Can't be used with Carpenters freed, since its thieves are already beaten",
            FortressKeys::label,
        )])
        .chain([toggle(
            SHOPS,
            group::SHUFFLES,
            "Shopsanity",
            "Shelf slots in the eight shops sell checks, each once, at a random price. Prices over 99 rupees need a bigger wallet in logic",
        )])
        .chain([choice(
            SHOP_SLOTS,
            group::SHUFFLES,
            "Shop slots",
            "How many slots of each shop sell a check with Shopsanity on. The rest sell what they always did",
            ShopSlots::label,
        )])
        .chain([toggle(
            FREESTANDING,
            group::SHUFFLES,
            "Freestanding heart pieces",
            "The fourteen heart pieces lying out in the world are checks. One that holds something else shows it, spinning where the piece was",
        )])
        .chain([toggle(
            COWS,
            group::SHUFFLES,
            "Cows",
            "Each of the eight cows gives a check once, for Epona's Song. After that it gives milk, which still needs a bottle",
        )])
        .chain([toggle(
            MERCHANTS,
            group::SHUFFLES,
            "Merchants",
            "People who sell one thing sell a check once, for the price of the thing: Medigoron's Giant's Knife, the carpet merchant's Bombchus, and the bean seller's first bean. Bombchus and beans stay on sale after that",
        )])
        .chain([toggle(
            TRADE_CHAIN,
            group::SHUFFLES,
            "Adult trade quest",
            "Each trade on the way to Biggoron's sword becomes a check, from Anju's Pocket Egg to Biggoron's Claim Check, and the ten trade items join the pool. Showing an item to the one who wants it gives that check, whatever order Link finds them in. Grog or Fado waits at the stump by which item Link holds. The Pocket Cucco still hatches from the egg",
        )])
        .chain([default_on(toggle(
            RUNNING_RACE,
            group::SHUFFLES,
            "Running man's race",
            "The running man's prize for Link's first win becomes a check. Later wins still pay a Red Rupee. Logic expects it only with Carpenters freed, since he offers the race once the valley bridge is fixed",
        ))])
        .chain([toggle(
            SKULLTULAS,
            group::SHUFFLES,
            "Gold Skulltulas",
            "Each of the 100 Gold Skulltulas drops a check when killed. A token for each goes in the pool, so the House of Skulltula still counts them. Logic wants the Boomerang or Hookshot for every one",
        )])
        .chain([toggle(
            BOSS_HEARTS,
            group::DUNGEONS,
            "Boss heart containers",
            "The heart container each of the eight bosses leaves behind is a check",
        )])
        .chain([
            toggle(
                POTS,
                group::SHUFFLES,
                "Pots",
                "Each pot is a check, flying pots included. One placed with no drop of its own brings a Recovery Heart, a Deku Stick and the like to the pool. A broken pot may come back when the area reloads, so its item may too",
            ),
            toggle(
                CRATES,
                group::SHUFFLES,
                "Crates",
                "Each wooden crate is a check. Link breaks a big one by rolling into it",
            ),
            toggle(
                ROCKS,
                group::SHUFFLES,
                "Rocks",
                "Each small rock is a check. Link lifts and throws one to break it",
            ),
            toggle(
                GRASS,
                group::SHUFFLES,
                "Grass",
                "Each grass shrub Link can cut or lift is a check, like those outside Saria's house",
            ),
            toggle(
                HIVES,
                group::SHUFFLES,
                "Beehives",
                "Each of Kakariko's beehives is a check. Anything thrown or shot but a slingshot seed knocks one down",
            ),
        ])
        .chain([default_on(toggle(
                PROGRESSIVE,
                group::GENERAL,
                "Progressive items",
                "Upgrade chests and gifts give the next level Link doesn't have: Hookshot then Longshot, the Goron Bracelet then Silver and Gold Gauntlets, Silver then Gold Scale, Fairy Ocarina then Time, and each bag, quiver and wallet up a size",
            ))])
        .chain([default_on(toggle(
                FREE_BOMBCHUS,
                group::GENERAL,
                "Bombchus without a bomb bag",
                "Bombchus hold up to 50 on their own, so they open the same checks bombs do without waiting on the bag",
            ))])
        .chain([default_on(toggle(
            QUIET_NAVI,
            group::GENERAL,
            "Quiet Navi",
            "Navi never stops Link to talk on her own: the Lost Woods warning, dungeon and quest tips, and reminders. Hints you ask her for still work",
        ))])
        .chain([toggle(
            QUIET_ENEMIES,
            group::GENERAL,
            "Quiet enemies",
            "Enemies never show, so nothing grabs Link or knocks him back. Gold Skulltulas, scrubs, Big Poes, the Skull Kid and enemies that are checks stay. For the play harness's seeds",
        )])
        .chain([toggle(
            BUNNY_HOOD,
            group::GENERAL,
            "Bunny Hood speed",
            "Link runs half again as fast while he wears the Bunny Hood",
        )])
        .chain([toggle(
            BRIDGE_MEDALLIONS,
            group::LOGIC,
            "Bridge at six medallions",
            "The rainbow bridge to Ganon's Castle rises once Link holds all six medallions, without the Light Arrows from Zelda first",
        )])
        .chain([toggle(
            HINTS,
            group::GENERAL,
            "Gossip stone hints",
            "Each gossip stone names where an item Link needs is hidden, without the Mask of Truth or any small talk",
        )])
        .chain(start)
        .chain([toggle(
            SKIP_ALL,
            group::CUTSCENES,
            "Skip all cutscenes",
            "Turns on every skip below. Cutscenes that give an item, set a flag or move Link still play",
        )])
        .chain(named)
        .chain(areas)
        .collect::<Vec<_>>();
    // Stable, so each panel keeps the order the chain gives it.
    all.sort_by_key(|d| GROUPS.iter().position(|&g| g == d.group));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_lists_each_panel_once_in_panel_order() {
        let mut groups: Vec<&str> = schema().iter().map(|d| d.group).collect();
        groups.dedup();
        assert_eq!(groups, GROUPS);
    }

    #[test]
    fn each_setting_sits_on_its_panel() {
        let group = |id: &str| schema().into_iter().find(|d| d.id == id).unwrap().group;
        assert_eq!(group(CHEST_SIZE), group::GENERAL);
        assert_eq!(group("open-forest"), group::LOGIC);
        assert_eq!(group("warp-kit"), group::INVENTORY);
        assert_eq!(group("claim-check"), group::INVENTORY);
        assert_eq!(group(ITEMS[0].id), group::INVENTORY);
        assert_eq!(group(SHOP_SLOTS), group::SHUFFLES);
        assert_eq!(group(KEYS), group::DUNGEONS);
        assert_eq!(group(HINTS), group::GENERAL);
        assert_eq!(group(SKIP_ALL), group::CUTSCENES);
    }

    #[test]
    fn untested_only_ever_leads_a_description() {
        // The app turns a leading "Untested. " into a badge, and only that.
        for d in schema() {
            let rest = d
                .description
                .strip_prefix("Untested. ")
                .unwrap_or(d.description);
            assert!(
                !rest.to_lowercase().contains("untested"),
                "{}: {}",
                d.id,
                d.description
            );
        }
    }

    #[test]
    fn every_id_is_unique() {
        let schema = schema();
        let mut ids: Vec<_> = schema.iter().map(|d| &d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), schema.len());
        for d in &schema {
            if let Some(p) = d.parent {
                assert!(
                    schema.iter().any(|s| s.id == p),
                    "{} has no parent {p}",
                    d.id
                );
            }
        }
    }
}
