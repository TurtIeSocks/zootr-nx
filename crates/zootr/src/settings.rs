//! The randomizer's options, described once so any UI can render them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use clap::ValueEnum;
use clap::builder::{PossibleValue, PossibleValuesParser};
use serde::Serialize;
use serde_json::Value;

use crate::cutscenes::{AREAS, Skips};
use crate::entrances::Doors;
use crate::keys;
use crate::presets::{self, CARPENTERS_FAST, CARPENTERS_FREED, PRESETS};
use crate::starting;

mod choices;
mod new_game;
mod upgrade;
pub use choices::{
    AdultJabu, Bombchus, ChestSize, DungeonEntrances, DungeonKeys, FortressKeys, IceTraps, Keys,
    Logic, ScrubPrice, ShopSlots, StartingAge,
};
pub use new_game::NewGame;
pub(crate) use new_game::value_word;
pub(crate) use new_game::{toggles, toggles_on};
pub(crate) use upgrade::upgrade;

/// Setting values by id. A missing id takes its default.
pub type Settings = BTreeMap<String, Value>;

/// The settings panels, by name.
pub mod group {
    /// Comfort and speed: nothing shuffled, nothing opened.
    pub const GENERAL: &str = "General";
    /// Changes to how Link plays: nothing shuffled, nothing opened.
    pub const ENHANCEMENTS: &str = "Enhancements";
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
pub const GROUPS: [&str; 7] = [
    group::GENERAL,
    group::ENHANCEMENTS,
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
    /// What a new game starts holding, the save editor's grid: a
    /// [`starting::Start`]. Unset is the game's own.
    Inventory,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Choice {
    pub id: String,
    pub label: &'static str,
    /// Toggles this pick turns on, shown locked.
    pub forces: Vec<&'static str>,
    /// Start inventory words this pick gives, shown held.
    pub starts: Vec<&'static str>,
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
            forces: Vec::new(),
            starts: Vec::new(),
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

/// `def`, a choice of `T`, with what each pick `forces` on.
fn forcing<T: ValueEnum>(mut def: SettingDef, forces: fn(T) -> choices::Forces) -> SettingDef {
    if let Kind::Choice { options, .. } = &mut def.kind {
        for o in options {
            let (toggles, starts) = forces(T::from_str(&o.id, false).expect("its own id"));
            o.forces = toggles.to_vec();
            o.starts = starts.to_vec();
        }
    }
    def
}

/// What the picks in `settings` force on: toggle ids, then start words.
pub(crate) fn forced(
    schema: &[SettingDef],
    settings: &Settings,
) -> (Vec<&'static str>, Vec<&'static str>) {
    let picks = schema.iter().filter_map(|d| match &d.kind {
        Kind::Choice { default, options } => {
            let id = settings
                .get(&d.id)
                .and_then(Value::as_str)
                .unwrap_or(default);
            options
                .iter()
                .find(|o| o.id == id)
                .or_else(|| options.iter().find(|o| &o.id == default))
        }
        _ => None,
    });
    let (mut toggles, mut starts) = (Vec::new(), Vec::new());
    for pick in picks {
        toggles.extend(&pick.forces);
        starts.extend(&pick.starts);
    }
    (toggles, starts)
}

/// `settings` with a random starting age rolled from `seed`, and whether
/// it was.
pub fn roll_age(settings: &Settings, seed: u64) -> (Settings, bool) {
    let age = chosen::<StartingAge>(settings, STARTING_AGE);
    let mut rolled = settings.clone();
    if age == StartingAge::Random {
        let id = choice_id(age.resolve(seed));
        rolled.insert(STARTING_AGE.into(), Value::String(id));
    }
    (rolled, age == StartingAge::Random)
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
pub(crate) const BOMBCHUS: &str = "bombchus";
pub(crate) const SHUFFLE_NUT_BAG: &str = "shuffle-nut-bag";
pub(crate) const SHUFFLE_STICK_BAG: &str = "shuffle-stick-bag";
const BUNNY_HOOD: &str = "bunny-hood";
const DAY_SKULLTULAS: &str = "day-skulltulas";
pub(crate) const AGELESS_WEAPONS: &str = "ageless-weapons";
const QUIET_NAVI: &str = "quiet-navi";
pub(crate) const QUIET_ENEMIES: &str = "quiet-enemies";
pub(crate) const OPEN_DOOR: &str = "open-door-of-time";
pub(crate) const STARTING_AGE: &str = "starting-age";
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
pub(crate) const DUNGEON_ENTRANCES: &str = "shuffle-dungeon-entrances";
pub(crate) const ADULT_JABU: &str = "adult-jabu-jabu";
pub(crate) const MERCHANTS: &str = "merchants";
pub(crate) const TRADE_CHAIN: &str = "trade-chain";
pub(crate) const RUNNING_RACE: &str = "running-race";
pub(crate) const MASTER_SWORD: &str = "master-sword-shuffle";

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
    let start = SettingDef {
        kind: Kind::Inventory,
        ..toggle(
            starting::SETTING,
            group::INVENTORY,
            "Starting inventory",
            "What a new game starts with. Each item leaves the shuffle, and wherever it was found holds a Blue Rupee instead",
        )
    };
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
            ADULT_JABU,
            group::LOGIC,
            "Adult Jabu-Jabu",
            "Jabu-Jabu waits in the frozen fountain for adult Link too, from the start or once the Water sage is done. Adult Link walks in when his mouth is open (the Jabu-Jabu open preset), but only child Link's Boomerang beats the tentacles and Barinade. With dungeon entrances shuffled, his door leads wherever it leads for child Link",
            AdultJabu::label,
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
        .chain([forcing(
            choice(
                STARTING_AGE,
                group::LOGIC,
                "Starting age",
                "How old Link is on a new file. Adult wakes in the Temple of Time with the Master Sword, and opens the Door of Time and the forest, so he can go back to child and walk out. Random rolls one from the seed",
                StartingAge::label,
            ),
            StartingAge::forces,
        )])
        .chain([default_on(toggle(
                OPEN_DOOR,
                group::LOGIC,
                "Open Door of Time",
                "The Door of Time stands open from the start, so Link can grow up without the stones or the Song of Time. Zelda and Impa stay until he's met them. He skips Rauru, whose gift comes at the start, and can put the sword back any time",
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
        .chain([forcing(
            choice(
                DUNGEON_ENTRANCES,
                group::DUNGEONS,
                "Dungeon entrances",
                "Each dungeon's front door leads into another dungeon, and its way out leads back to that door. On swaps the eleven dungeons. With Ganon's Castle, its door joins them, and Ganondorf waits wherever it leads. A door keeps what it needs to open, whatever is behind it. The Deku Tree lets adult Link in, and blue warps lead where they always do at Link's own age",
                DungeonEntrances::label,
            ),
            DungeonEntrances::forces,
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
        .chain([toggle(
            MASTER_SWORD,
            group::SHUFFLES,
            "Master Sword",
            "The pedestal holds an item from the pool, and the Master Sword can turn up anywhere. Pulling or returning the sword still travels through time without it. Adult Link without it has no sword",
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
        .chain([choice(
            BOMBCHUS,
            group::GENERAL,
            "Bombchus",
            "What caps the Bombchus Link holds. The bomb bag's, as in the game. Their own cap of 40, so they open the same checks bombs do without waiting on the bag. Or three Bombchu Bags in the pool, holding 20, 30 and 40, and none until the first",
            Bombchus::label,
        )])
        .chain([
            toggle(
                SHUFFLE_NUT_BAG,
                group::GENERAL,
                "Shuffle the nut bag",
                "The Small Nut Bag is an item in the pool, and Link holds no Deku Nuts until he finds it. Off, the first nut he picks up brings it, as in the game",
            ),
            toggle(
                SHUFFLE_STICK_BAG,
                group::GENERAL,
                "Shuffle the stick bag",
                "The Small Stick Bag is an item in the pool, and Link holds no Deku Sticks until he finds it. Off, the first stick he picks up brings it, as in the game",
            ),
        ])
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
            group::ENHANCEMENTS,
            "Bunny Hood speed",
            "Link runs half again as fast while he wears the Bunny Hood",
        )])
        .chain([toggle(
            DAY_SKULLTULAS,
            group::ENHANCEMENTS,
            "Gold Skulltulas by day",
            "Gold Skulltulas that only come out at night show at any hour",
        )])
        .chain([toggle(
            AGELESS_WEAPONS,
            group::ENHANCEMENTS,
            "One-handed swords and shields for both ages",
            "Child and adult Link can each wield every one-handed sword and every shield, and logic expects them to. Biggoron's Sword and the Giant's Knife stay adult Link's, and only he can aim the Mirror Shield at light",
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
        .chain([start])
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
        assert_eq!(group("claim-check"), group::LOGIC);
        assert_eq!(group(starting::SETTING), group::INVENTORY);
        assert_eq!(group(SHOP_SLOTS), group::SHUFFLES);
        assert_eq!(group(KEYS), group::DUNGEONS);
        assert_eq!(group(HINTS), group::GENERAL);
        assert_eq!(group(BUNNY_HOOD), group::ENHANCEMENTS);
        assert_eq!(group(AGELESS_WEAPONS), group::ENHANCEMENTS);
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
    fn enhancements_start_off() {
        let on = toggles_on(&Settings::new());
        let enhancements = schema()
            .into_iter()
            .filter(|d| d.group == group::ENHANCEMENTS);
        assert!(enhancements.map(|d| d.id).all(|id| !on.contains(&id)));
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
