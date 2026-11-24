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
use crate::presets::{self, PRESETS};
use crate::starting;

mod choices;
mod layout;
mod new_game;
mod schema;
mod upgrade;
pub use choices::{
    AdultJabu, Bombchus, Bridge, ChestSize, DoorOfTime, DungeonEntrances, DungeonKeys,
    FortressKeys, GerudoFortress, IceTraps, Keys, Logic, ScrubPrice, ShopSlots, StartingAge,
    Trials,
};
pub use layout::{GROUPS, Place, group, place};
pub use new_game::NewGame;
pub(crate) use new_game::value_word;
pub(crate) use new_game::{toggles, toggles_on};
pub use schema::schema;
pub(crate) use upgrade::{OLD_PRESETS, OLD_TOGGLES, dropped, renamed, renamed_as, upgrade};

/// Setting values by id. A missing id takes its default.
pub type Settings = BTreeMap<String, Value>;

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
    /// The toggle that must be on for this one to do anything. While it's
    /// off, the app greys this one out.
    pub requires: Option<&'static str>,
    /// The section of its panel it sits in.
    pub section: &'static str,
    /// The line under that section's title: see [`Place::about`].
    pub section_about: &'static str,
    /// Its section is a checklist: see [`Place::list`].
    pub list: bool,
    /// Start inventory words a value gives outside the start inventory, as
    /// `(value, words)`: a toggle's `on`, else an option's id. The grid
    /// shows them locked: see [`GIVES`].
    pub gives: Vec<(String, Vec<&'static str>)>,
    /// Whether the Archipelago world offers it: the app marks the rest.
    pub archipelago: Archipelago,
}

/// Whether the Archipelago world offers a setting, or one of a choice's
/// options: the app marks the rest. See [`crate::ap::LEFT_OUT`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "ap", content = "why", rename_all = "kebab-case")]
pub enum Archipelago {
    Offered,
    /// Coming, in this stage of the queue.
    Later(&'static str),
    /// Not a setting a multiworld can use, and why.
    Never(&'static str),
    /// [`Later`](Self::Later), and on in every Archipelago seed until then.
    OnUntil(&'static str),
    /// [`Never`](Self::Never), and on in every Archipelago seed.
    AlwaysOn(&'static str),
}

impl Archipelago {
    /// The reason it's left out, empty when offered.
    pub fn why(self) -> &'static str {
        match self {
            Archipelago::Offered => "",
            Archipelago::Later(why)
            | Archipelago::Never(why)
            | Archipelago::OnUntil(why)
            | Archipelago::AlwaysOn(why) => why,
        }
    }
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
    /// [`starting::Start`].
    Inventory {
        default: starting::Start,
    },
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Choice {
    pub id: String,
    pub label: &'static str,
    /// Toggles this pick turns on, and other choices' picks it makes as
    /// `id=option`, shown locked.
    pub forces: Vec<&'static str>,
    /// Start inventory words this pick gives, shown held.
    pub starts: Vec<&'static str>,
    /// Whether the Archipelago world offers this pick.
    pub archipelago: Archipelago,
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
    place: Place,
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
            archipelago: Archipelago::Offered,
        })
        .collect();
    SettingDef {
        kind: Kind::Choice {
            default: choice_id(T::default()),
            options,
        },
        ..toggle(id, place, label, description)
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

/// `settings` with what the picks force written in: each forced choice
/// set to its option, each forced toggle on.
pub(crate) fn with_picks(schema: &[SettingDef], settings: &Settings) -> Settings {
    let mut out = settings.clone();
    for word in forced(schema, settings).0 {
        match word.split_once('=') {
            Some((id, option)) => out.insert(id.into(), option.into()),
            None => out.insert(word.into(), true.into()),
        };
    }
    out
}

/// `settings` with a random starting age rolled from `seed`, and whether
/// it was. What Random forces stays forced whichever age it rolls, as the
/// app showed it.
pub fn roll_age(settings: &Settings, seed: u64) -> (Settings, bool) {
    let age = chosen::<StartingAge>(settings, STARTING_AGE);
    if age != StartingAge::Random {
        return (settings.clone(), false);
    }
    let mut rolled = with_picks(&schema(), settings);
    let id = choice_id(age.resolve(seed));
    rolled.insert(STARTING_AGE.into(), Value::String(id));
    (rolled, true)
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
pub(crate) const SHUFFLE_MASKS: &str = "shuffle-masks";
const BUNNY_HOOD: &str = "bunny-hood";
const DAY_SKULLTULAS: &str = "day-skulltulas";
pub(crate) const AGELESS_WEAPONS: &str = "ageless-weapons";
pub(crate) const AGELESS_ITEMS: &str = "ageless-items";
pub(crate) const MAGIC_WITH_FIRST_ITEM: &str = "magic-with-first-item";
const QUIET_NAVI: &str = "quiet-navi";
pub(crate) const QUIET_ENEMIES: &str = "quiet-enemies";
pub(crate) const DOOR_OF_TIME: &str = "door-of-time";
/// The rule word for [`DOOR_OF_TIME`] open ([`value_word`]), which Starting
/// Age's Adult also forces.
pub(crate) const DOOR_OPEN: &str = "door-of-time=open";
pub(crate) const STARTING_AGE: &str = "starting-age";
pub(crate) const GERUDO_FORTRESS: &str = "gerudo-fortress";
pub(crate) const MET_ZELDA: &str = "met-zelda";
/// The rule words for [`GERUDO_FORTRESS`] Fast and Freed ([`value_word`]).
pub(crate) const FORTRESS_FAST: &str = "gerudo-fortress=fast";
pub(crate) const FORTRESS_FREED: &str = "gerudo-fortress=freed";
pub(crate) const SCRUBS: &str = "scrubs";
pub(crate) const SCRUB_PRICE: &str = "scrub-price";
pub(crate) const NPC_GIFTS: &str = "npc-gifts";
pub(crate) const HUNDRED_TOKENS: &str = "hundred-token-reward";
pub(crate) const DUNGEON_REWARDS: &str = "dungeon-rewards";
pub(crate) const SONGS: &str = "songs";
pub(crate) const BRIDGE: &str = "rainbow-bridge";
pub(crate) const TRIALS: &str = "ganons-trials";
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
    place: Place,
    label: &'static str,
    description: &'static str,
) -> SettingDef {
    SettingDef {
        id: id.into(),
        group: place.group,
        label,
        description,
        kind: Kind::Toggle { default: false },
        parent: None,
        requires: None,
        section: place.section,
        section_about: place.about,
        list: place.list,
        gives: Vec::new(),
        archipelago: Archipelago::Offered,
    }
}

/// `def`, doing nothing unless toggle `id` is on.
fn requires(id: &'static str, def: SettingDef) -> SettingDef {
    SettingDef {
        requires: Some(id),
        ..def
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

/// The items a setting's value starts Link with by its own path, not the
/// start inventory, as rule word ([`value_word`], or a toggle's id) and
/// start words: Met Zelda's letter, and Gerudo Fortress
/// Freed's membership card.
const GIVES: [(&str, &[&str]); 2] = [
    (MET_ZELDA, &["zeldas-letter"]),
    (FORTRESS_FREED, &["gerudo-card"]),
];

/// `def`'s [`SettingDef::gives`], from [`GIVES`].
fn gives(def: &SettingDef) -> Vec<(String, Vec<&'static str>)> {
    let values: Vec<String> = match &def.kind {
        Kind::Toggle { .. } => vec![new_game::ON.into()],
        Kind::Choice { options, .. } => options.iter().map(|o| o.id.clone()).collect(),
        Kind::Inventory { .. } => vec![],
    };
    let word = |value: &str| match def.kind {
        Kind::Toggle { .. } => def.id.clone(),
        _ => value_word(&def.id, value),
    };
    (values.into_iter())
        .filter_map(|v| {
            let (_, words) = GIVES.iter().find(|(w, _)| *w == word(&v))?;
            Some((v, words.to_vec()))
        })
        .collect()
}

/// The rows of the sections that mix presets and settings, or settings
/// from far apart in [`schema`], in the order they show.
const ORDER: [&str; 13] = [
    presets::OPEN_FOREST,
    presets::KAKARIKO_GATE,
    presets::KING_ZORA_MOVED,
    presets::JABU_OPEN,
    ADULT_JABU,
    presets::OPEN_WATERFALL,
    presets::ADULT_DEKU_TREE,
    DOOR_OF_TIME,
    DUNGEON_REWARDS,
    MAPS_COMPASSES,
    BOSS_HEARTS,
    KEYS,
    FORTRESS_KEYS,
];

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
    fn met_zelda_and_freed_carpenters_give_their_items_locked() {
        let gives = |id: &str| schema().into_iter().find(|d| d.id == id).unwrap().gives;
        let met = vec![("on".to_owned(), vec!["zeldas-letter"])];
        assert_eq!(gives(MET_ZELDA), met);
        let freed = vec![("freed".to_owned(), vec!["gerudo-card"])];
        assert_eq!(gives(GERUDO_FORTRESS), freed);
        let given: usize = schema().iter().map(|d| d.gives.len()).sum();
        assert_eq!(given, GIVES.len());
        for (_, words) in GIVES {
            assert!(
                words.iter().all(|w| starting::parse(w).is_ok()),
                "{words:?}"
            );
        }
    }

    #[test]
    fn each_setting_sits_in_its_section() {
        let at = |id: &str| {
            let d = schema().into_iter().find(|d| d.id == id).unwrap();
            (d.group, d.section)
        };
        let is = |p: Place| (p.group, p.section);
        assert_eq!(at(CHEST_SIZE), is(place::COMFORT));
        assert_eq!(at(starting::SETTING), is(place::START));
        assert_eq!(at(SHOP_SLOTS), is(place::CHECKS));
        assert_eq!(at(ICE_TRAPS), is(place::CHECKS));
        assert_eq!(at(PROGRESSIVE), is(place::ITEM_POOL));
        assert_eq!(at(HINTS), is(place::HINTS));
        assert_eq!(at(BUNNY_HOOD), is(place::GAMEPLAY));
        assert_eq!(at(AGELESS_ITEMS), is(place::GAMEPLAY));
        assert_eq!(at(SKIP_ALL), is(place::CUTSCENES));
        assert_eq!(at(DUNGEON_ENTRANCES), is(place::ENTRANCES));
        // The Logic & Access sections and Shuffles' Dungeons, row by row.
        let rows = |p: Place| {
            (schema().into_iter())
                .filter(|d| (d.group, d.section) == is(p))
                .map(|d| d.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(place::LOGIC),
            [LOGIC, STARTING_AGE, GERUDO_FORTRESS, MET_ZELDA]
        );
        assert_eq!(rows(place::WIN_CONDITION), [BRIDGE, TRIALS]);
        assert_eq!(rows(place::AREA_ACCESS), ORDER[..8]);
        assert_eq!(rows(place::DUNGEONS), ORDER[8..]);
        assert_eq!(rows(place::ENTRANCES), [DUNGEON_ENTRANCES]);
        assert_eq!(place::ENTRANCES.group, group::LOGIC);
        assert_eq!(place::DUNGEONS.group, group::SHUFFLES);
        assert!(!GROUPS.contains(&"Dungeons"));
    }

    #[test]
    fn every_section_is_on_a_panel_and_each_setting_in_a_section() {
        assert!(place::ALL.iter().all(|p| GROUPS.contains(&p.group)));
        for d in schema() {
            let p = Place {
                group: d.group,
                section: d.section,
                about: d.section_about,
                list: d.list,
            };
            assert!(place::ALL.contains(&p), "{}", d.id);
            // A checklist holds only toggles.
            assert!(!d.list || matches!(d.kind, Kind::Toggle { .. }), "{}", d.id);
        }
    }

    #[test]
    fn a_setting_that_needs_another_names_a_toggle() {
        let schema = schema();
        let toggle =
            |id: &str| (schema.iter()).any(|d| d.id == id && matches!(d.kind, Kind::Toggle { .. }));
        let needs: Vec<_> = (schema.iter())
            .filter_map(|d| d.requires.map(|r| (d.id.as_str(), r)))
            .collect();
        assert_eq!(
            needs,
            [
                (SCRUB_PRICE, SCRUBS),
                (HUNDRED_TOKENS, NPC_GIFTS),
                (SHOP_SLOTS, SHOPS)
            ]
        );
        assert!(needs.iter().all(|(_, r)| toggle(r)));
    }

    #[test]
    fn the_running_race_and_the_hundred_token_reward_start_off() {
        let on = toggles_on(&Settings::new());
        assert!(!on.contains(RUNNING_RACE));
        assert!(!on.contains(HUNDRED_TOKENS));
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
