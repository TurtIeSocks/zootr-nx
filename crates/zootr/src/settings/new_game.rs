//! What a seed's settings do to a new save: [`NewGame`], read from the
//! settings by id.

use super::*;
use crate::world::Era;

/// How a new save starts: GameData default edits, the cutscenes skipped,
/// the chests and the starting items.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewGame {
    pub defaults: Vec<String>,
    pub skips: Skips,
    pub chest_size: ChestSize,
    /// Leave every check's item where it is.
    pub vanilla: bool,
    /// Leave each chest's item where it is, and shuffle the other checks.
    pub keep_chests: bool,
    /// What the Inventory panel's grid starts Link holding.
    pub start: starting::Start,
    /// Upgrade chests give their line's levels in order.
    pub progressive: bool,
    /// The Door of Time starts open.
    pub open_door: bool,
    /// Business scrubs are checks.
    pub scrubs: bool,
    /// Rupees every scrub asks for its check.
    pub scrub_price: i32,
    /// Slots per shop that are checks, 0 for none.
    pub shop_slots: usize,
    /// Freestanding heart pieces are checks.
    pub freestanding: bool,
    /// Gold Skulltulas' drops are checks.
    pub skulltulas: bool,
    /// Boss heart containers are checks.
    pub boss_hearts: bool,
    /// Pots are checks.
    pub pots: bool,
    /// Crates are checks.
    pub crates: bool,
    /// Small rocks are checks.
    pub rocks: bool,
    /// Grass shrubs are checks.
    pub grass: bool,
    /// Beehives are checks.
    pub hives: bool,
    /// Each cow gives a check for Epona's Song.
    pub cows: bool,
    /// Where every dungeon's keys are. Unless vanilla, their chests are
    /// checks.
    pub keys: DungeonKeys,
    /// Where the thieves' keys are. Unless vanilla, the keys they carry
    /// are checks.
    pub fortress_keys: FortressKeys,
    /// How much junk becomes ice traps.
    pub ice_traps: IceTraps,
    /// Which dungeon doors swap.
    pub dungeon_entrances: DungeonEntrances,
    /// When Jabu-Jabu waits in the adult fountain.
    pub adult_jabu: AdultJabu,
    /// Doors pinned by hand for a test, as `DOOR=DUNGEON`.
    pub door_pins: Vec<String>,
    /// Which dungeon is behind each door, rolled with the seed.
    pub doors: Doors,
    /// The seed, for what's rolled while writing, like shop prices.
    pub seed: u64,
    /// Link's age on a new file, random already rolled.
    pub age: Era,
    /// Whether `age` was rolled.
    pub random_age: bool,
    /// What caps Bombchus.
    pub bombchus: Bombchus,
    /// The Small Nut Bag is in the pool, and nuts need a bag.
    pub shuffle_nut_bag: bool,
    /// The Small Stick Bag is in the pool, and sticks need a bag.
    pub shuffle_stick_bag: bool,
    /// Any magic item turns the meter on, as in the game. Off, only the
    /// meter items do, and a Magic Meter is in the pool.
    pub magic_with_first_item: bool,
    /// Navi never talks on her own.
    pub quiet_navi: bool,
    /// Enemies never show, besides those that are checks or hold one.
    pub quiet_enemies: bool,
    /// Link runs faster in the Bunny Hood.
    pub bunny_hood: bool,
    /// Night-only Gold Skulltulas show by day too.
    pub day_skulltulas: bool,
    /// The rainbow bridge rises at six medallions.
    pub bridge_medallions: bool,
    /// Gossip stones name where needed items are.
    pub hints: bool,
    pub logic: Logic,
    /// Every toggle that's on, by id, for rules that read settings.
    pub on: BTreeSet<String>,
    /// Where a new game starts, as `MAP/SPOT`, instead of Link's house.
    pub spawn: Option<String>,
    /// Placement fields set by hand for a test, as `HASH.PATH=VALUE`.
    pub fields: Vec<String>,
    /// Pouch items a new game is handed, by actor, beyond `start`.
    pub give: Vec<String>,
    /// Quest steps the opening moves on, as `(challenge, step)`.
    pub progress: Vec<(String, String)>,
    /// Ints the seed adds, at 0 and kept in the save.
    pub ints: Vec<String>,
    /// The copies standing in for other players' items, in an AP seed.
    pub foreign: Vec<crate::foreign::Foreign>,
    /// Every slot's name by number, in an AP seed, for received items'
    /// notices.
    pub players: std::collections::BTreeMap<u32, String>,
}

impl NewGame {
    /// A test's new game: the defaults, with key rings.
    #[cfg(test)]
    pub(crate) fn rings() -> Self {
        Self {
            keys: DungeonKeys::Rings,
            ..Self::default()
        }
    }

    /// `settings`, then `edits` (`NAME=VALUE`) on top.
    pub fn from_settings(settings: &Settings, edits: &[String]) -> Result<Self> {
        let schema = schema();
        let dungeon_entrances = chosen::<DungeonEntrances>(settings, DUNGEON_ENTRANCES);
        let on = |id: &str| is_on(&schema, settings, id);
        let picked: Vec<String> = PRESETS
            .iter()
            .filter(|p| on(p.name))
            .map(|p| p.name.to_owned())
            .collect();
        let has = |name: &str| picked.iter().any(|p| p == name);
        let keys = chosen::<DungeonKeys>(settings, KEYS);
        if keys.copies() && !on(SHUFFLE_ITEMS) {
            bail!(
                "{} needs items shuffled: keys take the place of key chests' items",
                keys.label()
            );
        }
        let fortress_keys = chosen::<FortressKeys>(settings, FORTRESS_KEYS);
        if fortress_keys.moved() && has(CARPENTERS_FREED) {
            bail!(
                "Fortress keys can't start with {CARPENTERS_FREED}: its thieves are already beaten"
            );
        }
        if has(CARPENTERS_FAST) && has(CARPENTERS_FREED) {
            bail!(
                "{CARPENTERS_FAST} can't start with {CARPENTERS_FREED}: both count the carpenters"
            );
        }
        let mut start = starting::Start::from_settings(settings)?;
        for word in super::forced(&schema, settings).1 {
            if !start.holds(word) {
                start.add(word, 1)?;
            }
        }
        let edits = [
            keys::defaults(
                keys == DungeonKeys::Start,
                fortress_keys == FortressKeys::Start,
            ),
            start.defaults(),
            edits.to_vec(),
        ]
        .concat();
        Ok(Self {
            defaults: presets::expand(&picked, &edits)?,
            skips: Skips {
                intro: on(SKIP_INTRO),
                saria: on(SKIP_SARIA),
                owl: on(SKIP_OWL),
                great_deku: on(SKIP_GREAT_DEKU),
                arrivals: on(SKIP_ARRIVALS),
                areas: AREAS
                    .iter()
                    .map(|a| a.0)
                    .filter(|a| on(&skip_id(a)))
                    .collect(),
            },
            chest_size: chosen(settings, CHEST_SIZE),
            ice_traps: chosen(settings, ICE_TRAPS),
            vanilla: !on(SHUFFLE_ITEMS),
            keep_chests: !on(CHESTS),
            start,
            progressive: on(PROGRESSIVE),
            open_door: on(OPEN_DOOR),
            scrubs: on(SCRUBS),
            scrub_price: chosen::<ScrubPrice>(settings, SCRUB_PRICE).rupees(),
            freestanding: on(FREESTANDING),
            skulltulas: on(SKULLTULAS),
            boss_hearts: on(BOSS_HEARTS),
            pots: on(POTS),
            crates: on(CRATES),
            rocks: on(ROCKS),
            grass: on(GRASS),
            hives: on(HIVES),
            cows: on(COWS),
            keys,
            fortress_keys,
            dungeon_entrances,
            adult_jabu: chosen::<AdultJabu>(settings, ADULT_JABU),
            door_pins: vec![],
            doors: Doors::default(),
            shop_slots: match on(SHOPS) {
                true => chosen::<ShopSlots>(settings, SHOP_SLOTS).count(),
                false => 0,
            },
            seed: 0,
            age: match chosen::<StartingAge>(settings, STARTING_AGE) {
                StartingAge::Adult => Era::Adult,
                StartingAge::Child | StartingAge::Random => Era::Child,
            },
            random_age: false,
            bombchus: chosen(settings, BOMBCHUS),
            shuffle_nut_bag: on(SHUFFLE_NUT_BAG),
            shuffle_stick_bag: on(SHUFFLE_STICK_BAG),
            magic_with_first_item: on(MAGIC_WITH_FIRST_ITEM),
            quiet_navi: on(QUIET_NAVI),
            quiet_enemies: on(QUIET_ENEMIES),
            bunny_hood: on(BUNNY_HOOD),
            day_skulltulas: on(DAY_SKULLTULAS),
            bridge_medallions: on(BRIDGE_MEDALLIONS),
            hints: on(HINTS),
            logic: chosen(settings, LOGIC),
            on: toggles_on(settings),
            give: presets::gives(&picked)?,
            progress: presets::progress(&picked)?,
            spawn: None,
            fields: presets::fields(&picked)?,
            ints: vec![],
            foreign: vec![],
            players: Default::default(),
        })
    }
}

/// Every toggle that's on, by id, counting parents, and each setting's
/// value as rules name it ([`value_word`]).
pub(crate) fn toggles_on(settings: &Settings) -> BTreeSet<String> {
    let schema = schema();
    let on = (schema.iter())
        .filter(|d| is_on(&schema, settings, &d.id))
        .map(|d| d.id.clone());
    let values = (schema.iter()).filter_map(|d| {
        let value = match &d.kind {
            Kind::Toggle { .. } => [OFF, ON][usize::from(is_on(&schema, settings, &d.id))],
            Kind::Choice { default, options } => (settings.get(&d.id))
                .and_then(Value::as_str)
                .filter(|v| options.iter().any(|o| o.id == *v))
                .unwrap_or(default),
            Kind::Inventory { .. } => return None,
        };
        Some(value_word(&d.id, value))
    });
    on.chain(values).collect()
}

/// A toggle's value words.
const ON: &str = "on";
const OFF: &str = "off";

/// How rules name setting `id` holding `value`: `id=value`, a choice's
/// option or a toggle's `on` or `off`.
pub(crate) fn value_word(id: &str, value: &str) -> String {
    format!("{id}={value}")
}

/// Every word a rule may read after `setting:`: each toggle's id, and each
/// setting's values ([`value_word`]).
pub(crate) fn toggles() -> BTreeSet<String> {
    let words = schema().into_iter().flat_map(|d| {
        let values: Vec<String> = match &d.kind {
            Kind::Toggle { .. } => {
                vec![d.id.clone(), value_word(&d.id, ON), value_word(&d.id, OFF)]
            }
            Kind::Choice { options, .. } => {
                (options.iter()).map(|o| value_word(&d.id, &o.id)).collect()
            }
            Kind::Inventory { .. } => vec![],
        };
        values
    });
    words.collect()
}

/// Whether setting `id` is on: its own value or default, or its parent's.
fn is_on(schema: &[SettingDef], settings: &Settings, id: &str) -> bool {
    schema.iter().find(|d| d.id == id).is_some_and(|d| {
        let own = match d.kind {
            Kind::Toggle { default } => {
                settings.get(id).and_then(Value::as_bool).unwrap_or(default)
            }
            Kind::Choice { .. } | Kind::Inventory { .. } => false,
        };
        own || d.parent.is_some_and(|p| is_on(schema, settings, p))
            || super::forced(schema, settings).0.contains(&id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{flag, item};
    use crate::presets::ADULT_DEKU_TREE;

    /// What the default-on presets add, then `edits`.
    fn with_defaults(edits: &[&str]) -> Vec<String> {
        let on: Vec<String> = presets::DEFAULT_ON.map(String::from).into();
        let edits: Vec<String> = edits.iter().map(|&e| e.into()).collect();
        presets::expand(&on, &edits).unwrap()
    }

    /// `with_defaults`, with `keys` and then `edits` after the presets.
    fn with_defaults_of(keys: &[String], edits: &[&str]) -> Vec<String> {
        let mut out = with_defaults(&[]);
        out.extend(keys.iter().cloned());
        out.extend(edits.iter().map(|&e| e.to_owned()));
        out
    }

    #[test]
    fn unset_settings_take_their_defaults() {
        let game = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        assert_eq!(
            game,
            NewGame {
                defaults: with_defaults(&[]),
                skips: Skips {
                    intro: true,
                    arrivals: true,
                    ..Default::default()
                },
                chest_size: ChestSize::Vanilla,
                vanilla: false,
                keep_chests: false,
                start: starting::Start::of(&starting::DEFAULT),
                progressive: true,
                open_door: true,
                scrubs: true,
                scrub_price: ScrubPrice::default().rupees(),
                shop_slots: 0,
                freestanding: false,
                skulltulas: false,
                boss_hearts: false,
                pots: false,
                crates: false,
                rocks: false,
                grass: false,
                hives: false,
                cows: false,
                keys: DungeonKeys::Vanilla,
                fortress_keys: FortressKeys::Vanilla,
                ice_traps: IceTraps::Off,
                dungeon_entrances: DungeonEntrances::Off,
                adult_jabu: AdultJabu::Off,
                door_pins: vec![],
                doors: Doors::default(),
                seed: 0,
                age: Era::Child,
                random_age: false,
                bombchus: Bombchus::Free,
                shuffle_nut_bag: false,
                shuffle_stick_bag: false,
                magic_with_first_item: false,
                quiet_navi: true,
                quiet_enemies: false,
                bunny_hood: false,
                day_skulltulas: false,
                bridge_medallions: false,
                hints: false,
                spawn: None,
                fields: vec![],
                give: vec![],
                progress: vec![],
                ints: vec![],
                foreign: vec![],
                players: Default::default(),
                logic: Logic::Glitchless,
                on: toggles_on(&Settings::new()),
            }
        );
    }

    #[test]
    fn toggles_map_to_preset_edits() {
        let settings = Settings::from([
            ("kakariko-gate".into(), true.into()),
            (SKIP_INTRO.into(), false.into()),
            (SKIP_ARRIVALS.into(), false.into()),
            ("unknown".into(), true.into()),
            (
                starting::SETTING.into(),
                serde_json::json!({ "bomb-bag": 1, "kokiri-sword": 1, "rupees": 50 }),
            ),
            (SHUFFLE_ITEMS.into(), false.into()),
            (PROGRESSIVE.into(), false.into()),
        ]);
        let game = NewGame::from_settings(&settings, &["A=1".into()]).unwrap();
        let rupees = format!("#{:08x}=50", crate::save_editor::RUPEES);
        let mut defaults = with_defaults(&[&rupees, "A=1"]);
        defaults.insert(0, format!("{}=true", flag::NPC_HYLIA_SOLDIER_003_GATE_OPEN));
        assert_eq!(game.defaults, defaults);
        assert!(game.skips.is_empty());
        assert_eq!(game.start.0.len(), 3);
        assert!(game.vanilla && !game.progressive);
    }

    #[test]
    fn a_quest_preset_hands_over_its_items_from_its_own_section() {
        needs_names!();
        let settings = Settings::from([("claim-check".into(), true.into())]);
        let game = NewGame::from_settings(&settings, &[]).unwrap();
        assert_eq!(game.give, [item::CLAIM_CHECK]);
        assert!(game.defaults.contains(&crate::steps::at(
            crate::known::flow::SUB_EVENT_GET_SWORD_BIG_GORON,
            "Step11"
        )));
        let def = schema()
            .into_iter()
            .find(|d| d.id == "claim-check")
            .unwrap();
        assert_eq!(def.section, crate::settings::place::QUEST.section);
    }

    #[test]
    fn a_choice_reads_its_id_and_falls_back_to_its_default() {
        let size = |v: &str| {
            let settings = Settings::from([(CHEST_SIZE.into(), v.into())]);
            NewGame::from_settings(&settings, &[]).unwrap().chest_size
        };
        assert_eq!(size("small"), ChestSize::Small);
        assert_eq!(size("huge"), ChestSize::Vanilla);
        let def = schema().into_iter().find(|d| d.id == CHEST_SIZE).unwrap();
        let Kind::Choice { default, options } = def.kind else {
            panic!()
        };
        assert_eq!(default, "vanilla");
        let ids: Vec<_> = options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["vanilla", "small"]);
    }

    #[test]
    fn keys_start_fills_the_counters_before_any_edit() {
        let settings = Settings::from([(KEYS.into(), "start".into())]);
        let game = NewGame::from_settings(&settings, &["A=1".into()]).unwrap();
        assert_eq!(game.keys, DungeonKeys::Start);
        assert_eq!(
            game.defaults,
            with_defaults_of(&keys::defaults(true, false), &["A=1"])
        );
        let settings = Settings::from([(FORTRESS_KEYS.into(), "start".into())]);
        let game = NewGame::from_settings(&settings, &[]).unwrap();
        assert!(game.keys == DungeonKeys::Vanilla && game.fortress_keys == FortressKeys::Start);
        assert_eq!(
            game.defaults,
            with_defaults_of(&keys::defaults(false, true), &[])
        );
        let game = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        assert_eq!(game.keys, DungeonKeys::Vanilla);
        assert!(!game.defaults.iter().any(|d| d.contains("DungeonInfo")));
        let settings = Settings::from([(KEYS.into(), "rings".into())]);
        let game = NewGame::from_settings(&settings, &[]).unwrap();
        assert_eq!(game.keys, DungeonKeys::Rings);
        assert!(!game.defaults.iter().any(|d| d.contains("DungeonInfo")));
        let mut vanilla = settings.clone();
        vanilla.insert(SHUFFLE_ITEMS.into(), false.into());
        assert!(NewGame::from_settings(&vanilla, &[]).is_err());
        let def = schema().into_iter().find(|d| d.id == KEYS).unwrap();
        let Kind::Choice { default, options } = def.kind else {
            panic!()
        };
        assert_eq!(default, "vanilla");
        let ids: Vec<_> = options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["vanilla", "start", "rings", "singles"]);
    }

    #[test]
    fn adult_opens_the_door_and_the_forest_and_leaves_the_sword_to_the_pedestal() {
        let adult = Settings::from([
            (STARTING_AGE.into(), "adult".into()),
            (starting::SETTING.into(), serde_json::json!({})),
        ]);
        let new_game = NewGame::from_settings(&adult, &[]).unwrap();
        assert_eq!(new_game.age, Era::Adult);
        assert!(new_game.open_door);
        let opened = [OPEN_DOOR, presets::OPEN_FOREST];
        assert!(opened.iter().all(|id| new_game.on.contains(*id)));
        // The pedestal's gift gives it in the opening (`gifts::pedestal`).
        assert!(!new_game.start.holds("master-sword"));
        let closed = Settings::from([
            (STARTING_AGE.into(), "adult".into()),
            (OPEN_DOOR.into(), false.into()),
        ]);
        assert!(NewGame::from_settings(&closed, &[]).unwrap().open_door);
        let child = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        assert_eq!(child.age, Era::Child);
        assert!(!child.on.contains(presets::OPEN_FOREST));
    }

    #[test]
    fn a_random_age_rolls_the_same_for_a_seed_and_both_turn_up() {
        let random = Settings::from([(STARTING_AGE.into(), "random".into())]);
        let age = |seed| {
            let (settings, rolled) = super::super::roll_age(&random, seed);
            assert!(rolled);
            NewGame::from_settings(&settings, &[]).unwrap().age
        };
        let ages: BTreeSet<_> = (0..64).map(age).collect();
        assert_eq!(ages, [Era::Child, Era::Adult].into());
        assert!((0..16).all(|seed| age(seed) == age(seed)));
        let (_, rolled) = super::super::roll_age(&Settings::new(), 3);
        assert!(!rolled);
    }

    #[test]
    fn shuffled_doors_clear_the_adult_deku_trees_debris() {
        needs_names!();
        let preset = presets::fields(&[ADULT_DEKU_TREE.into()]).unwrap();
        let off = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        assert!(!off.on.contains(ADULT_DEKU_TREE));
        assert!(!preset.iter().all(|f| off.fields.contains(f)));
        for mode in ["on", "on-ganon"] {
            let settings = Settings::from([(DUNGEON_ENTRANCES.into(), mode.into())]);
            let on = NewGame::from_settings(&settings, &[]).unwrap();
            assert!(on.on.contains(ADULT_DEKU_TREE), "{mode}");
            assert!(preset.iter().all(|f| on.fields.contains(f)), "{mode}");
        }
    }

    #[test]
    fn fast_carpenters_are_refused_with_freed_ones_but_not_with_fortress_keys() {
        let mut settings = Settings::from([
            (CARPENTERS_FAST.into(), true.into()),
            (FORTRESS_KEYS.into(), "singles".into()),
        ]);
        assert!(NewGame::from_settings(&settings, &[]).is_ok());
        settings.insert(FORTRESS_KEYS.into(), "vanilla".into());
        settings.insert(CARPENTERS_FREED.into(), true.into());
        let err = NewGame::from_settings(&settings, &[]).unwrap_err();
        assert!(err.to_string().contains(CARPENTERS_FAST), "{err}");
    }

    #[test]
    fn fortress_keys_are_refused_with_carpenters_freed() {
        let mut settings = Settings::from([(FORTRESS_KEYS.into(), "start".into())]);
        assert!(NewGame::from_settings(&settings, &[]).is_ok());
        settings.insert(CARPENTERS_FREED.into(), true.into());
        let err = NewGame::from_settings(&settings, &[]).unwrap_err();
        assert!(err.to_string().contains(CARPENTERS_FREED), "{err}");
        settings.insert(FORTRESS_KEYS.into(), "singles".into());
        assert!(NewGame::from_settings(&settings, &[]).is_err());
        settings.insert(KEYS.into(), "start".into());
        settings.insert(FORTRESS_KEYS.into(), "vanilla".into());
        assert!(NewGame::from_settings(&settings, &[]).is_ok());
    }

    #[test]
    fn fortress_keysanity_deals_copies_and_starts_with_no_keys() {
        let settings = Settings::from([(FORTRESS_KEYS.into(), "singles".into())]);
        let game = NewGame::from_settings(&settings, &[]).unwrap();
        assert_eq!(game.fortress_keys, FortressKeys::Singles);
        assert!(game.fortress_keys.moved() && game.fortress_keys.copies());
        assert_eq!(
            game.defaults,
            with_defaults_of(&keys::defaults(false, false), &[])
        );
    }

    #[test]
    fn skip_all_turns_on_every_cutscene_setting() {
        let settings = Settings::from([
            (SKIP_ALL.into(), true.into()),
            (SKIP_INTRO.into(), false.into()),
        ]);
        let skips = NewGame::from_settings(&settings, &[]).unwrap().skips;
        assert!(skips.intro && skips.saria && skips.owl && skips.great_deku);
        assert_eq!(skips.areas.len(), AREAS.len());
        let one = Settings::from([(skip_id("kokiri"), true.into())]);
        let skips = NewGame::from_settings(&one, &[]).unwrap().skips;
        assert_eq!(skips.areas, ["kokiri"].into());
        assert!(!skips.owl);
    }
}
