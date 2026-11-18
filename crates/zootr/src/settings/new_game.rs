//! What a seed's settings do to a new save: [`NewGame`], read from the
//! settings by id.

use super::*;

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
    /// Doors pinned by hand for a test, as `DOOR=DUNGEON`.
    pub door_pins: Vec<String>,
    /// Which dungeon is behind each door, rolled with the seed.
    pub doors: Doors,
    /// The seed, for what's rolled while writing, like shop prices.
    pub seed: u64,
    /// Bombchus have their own cap instead of the bomb bag's.
    pub free_bombchus: bool,
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
        let on = |id: &str| is_on(&schema, settings, id);
        let picked: Vec<String> = PRESETS
            .iter()
            .filter(|p| on(p.name))
            .map(|p| p.name.to_owned())
            .collect();
        let keys = chosen::<DungeonKeys>(settings, KEYS);
        if keys.copies() && !on(SHUFFLE_ITEMS) {
            bail!(
                "{} needs items shuffled: keys take the place of key chests' items",
                keys.label()
            );
        }
        let fortress_keys = chosen::<FortressKeys>(settings, FORTRESS_KEYS);
        if fortress_keys.moved() && picked.iter().any(|p| p == CARPENTERS_FREED) {
            bail!(
                "Fortress keys can't start with {CARPENTERS_FREED}: its thieves are already beaten"
            );
        }
        let start = starting::Start::from_settings(settings)?;
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
            dungeon_entrances: chosen(settings, DUNGEON_ENTRANCES),
            door_pins: vec![],
            doors: Doors::default(),
            shop_slots: match on(SHOPS) {
                true => chosen::<ShopSlots>(settings, SHOP_SLOTS).count(),
                false => 0,
            },
            seed: 0,
            free_bombchus: on(FREE_BOMBCHUS),
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

/// Every toggle that's on, by id, counting parents.
pub(crate) fn toggles_on(settings: &Settings) -> BTreeSet<String> {
    let schema = schema();
    schema
        .iter()
        .filter(|d| is_on(&schema, settings, &d.id))
        .map(|d| d.id.clone())
        .collect()
}

/// Every toggle's id.
pub(crate) fn toggles() -> BTreeSet<String> {
    schema()
        .into_iter()
        .filter(|d| matches!(d.kind, Kind::Toggle { .. }))
        .map(|d| d.id)
        .collect()
}

/// Whether setting `id` is on: its own value or default, or its parent's.
fn is_on(schema: &[SettingDef], settings: &Settings, id: &str) -> bool {
    schema.iter().find(|d| d.id == id).is_some_and(|d| {
        let own = match d.kind {
            Kind::Toggle { default } => {
                settings.get(id).and_then(Value::as_bool).unwrap_or(default)
            }
            Kind::Choice { .. } | Kind::Inventory => false,
        };
        own || d.parent.is_some_and(|p| is_on(schema, settings, p))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{flag, item};

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
                start: starting::Start::default(),
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
                door_pins: vec![],
                doors: Doors::default(),
                seed: 0,
                free_bombchus: true,
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
    fn a_quest_preset_hands_over_its_items_from_its_own_subgroup() {
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
        assert_eq!(def.subgroup, Some("Quest progress"));
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
