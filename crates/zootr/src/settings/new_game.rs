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
    /// A chest's look says what it holds (`chest_looks`).
    pub chest_looks: bool,
    /// Leave every check's item where it is: the CLI's `--vanilla`, and
    /// seeds from when Shuffle items was a setting.
    pub vanilla: bool,
    /// Leave each chest's item where it is, and shuffle the other checks.
    pub keep_chests: bool,
    /// Leave Saria's and Zelda's ocarinas where they are: NPC Gifts off.
    pub keep_gifts: bool,
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
    /// Rupees, hearts and other pickups lying loose are checks.
    pub loose_items: bool,
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
    /// How the fortress carpenters start.
    pub gerudo_fortress: GerudoFortress,
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
    /// Breakables come back on every map load.
    pub breakables_respawn: bool,
    /// What raises the rainbow bridge.
    pub bridge: Bridge,
    /// Whether Ganon's trials stand or start done.
    pub trials: Trials,
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
    /// The items a chest grows for when its size matches its contents:
    /// what logic needs, or with no logic what isn't junk. Set once the
    /// items are placed (`ice_traps::wanted`).
    pub major: BTreeSet<String>,
    /// The copies standing in for other players' items, in an AP seed.
    pub foreign: Vec<crate::foreign::Foreign>,
    /// Every slot's name by number, in an AP seed, for received items'
    /// notices.
    pub players: std::collections::BTreeMap<u32, String>,
    /// The stones' hints AP picked, in an AP seed that has them: the seed
    /// picks none of its own.
    pub given_hints: Option<Vec<crate::hints::Hint>>,
    /// The colors the seed paints.
    pub colors: crate::colors::Colors,
    /// The songs the seed plays.
    pub audio: crate::audio::Audio,
}

impl NewGame {
    /// Whether a seed fills with logic: items shuffle and logic is on.
    /// Otherwise items stay put or scatter at random.
    pub fn fills(&self) -> bool {
        self.logic == Logic::Glitchless && !self.vanilla
    }

    /// A test's new game: the defaults, with key rings.
    #[cfg(test)]
    pub(crate) fn rings() -> Self {
        Self {
            keys: DungeonKeys::Rings,
            ..Self::default()
        }
    }

    /// Fit the start to `pouch`, the seed's own table (`Pouch::seeded`),
    /// and mark what it then holds, as [`Start::settle`] says. Returns what
    /// came down.
    pub fn settle_start(&mut self, pouch: &crate::inventory::Pouch) -> Vec<String> {
        let notes = self.start.settle(pouch);
        for d in self.start.defaults() {
            if !self.defaults.contains(&d) {
                self.defaults.push(d);
            }
        }
        notes
    }

    /// `settings`, then `edits` (`NAME=VALUE`) on top.
    pub fn from_settings(settings: &Settings, edits: &[String]) -> Result<Self> {
        let schema = schema();
        let settings = &super::with_picks(&schema, settings);
        let dungeon_entrances = chosen::<DungeonEntrances>(settings, DUNGEON_ENTRANCES);
        let on = |id: &str| is_on(&schema, settings, id);
        let picked: Vec<String> = PRESETS
            .iter()
            .filter(|p| on(p.name))
            .map(|p| p.name.to_owned())
            .collect();
        let keys = chosen::<DungeonKeys>(settings, KEYS);
        let fortress_keys = chosen::<FortressKeys>(settings, FORTRESS_KEYS);
        let gerudo_fortress = chosen::<GerudoFortress>(settings, GERUDO_FORTRESS);
        let bridge = chosen::<Bridge>(settings, BRIDGE);
        let trials = chosen::<Trials>(settings, TRIALS);
        let mut start = starting::Start::from_settings(settings)?;
        for word in super::forced(&schema, settings).1 {
            if !start.holds(word) {
                start.add(word, 1)?;
            }
        }
        crate::gifts::egg::start_hatched(&mut start, on(super::NPC_GIFTS))?;
        crate::bottles::check_start(start.bottles(), on(presets::KING_ZORA_MOVED))?;
        let defaults = [
            choice_edits(
                keys,
                fortress_keys,
                &start,
                bridge,
                trials,
                gerudo_fortress,
                on(MET_ZELDA),
            ),
            edits.to_vec(),
        ]
        .concat();
        Ok(Self {
            defaults: presets::expand(&picked, &defaults)?,
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
            chest_looks: on(CHEST_LOOKS),
            ice_traps: chosen(settings, ICE_TRAPS),
            vanilla: false,
            keep_chests: !on(CHESTS),
            keep_gifts: !on(NPC_GIFTS),
            start,
            progressive: on(PROGRESSIVE),
            open_door: chosen::<DoorOfTime>(settings, DOOR_OF_TIME) == DoorOfTime::Open,
            scrubs: on(SCRUBS),
            scrub_price: chosen::<ScrubPrice>(settings, SCRUB_PRICE).rupees(),
            freestanding: on(FREESTANDING),
            loose_items: on(LOOSE_ITEMS),
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
            gerudo_fortress,
            dungeon_entrances,
            adult_jabu: chosen::<AdultJabu>(settings, ADULT_JABU),
            door_pins: vec![],
            doors: Doors::default(),
            shop_slots: if on(SHOPS) {
                chosen::<ShopSlots>(settings, SHOP_SLOTS).count()
            } else {
                0
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
            breakables_respawn: on(BREAKABLES_RESPAWN),
            bridge,
            trials,
            hints: on(HINTS),
            logic: chosen(settings, LOGIC),
            on: toggles_on(settings),
            give: presets::gives(&picked)?,
            progress: [
                presets::progress(&picked)?,
                match gerudo_fortress {
                    GerudoFortress::Freed => vec![crate::gerudo::freed_step()],
                    GerudoFortress::Normal | GerudoFortress::Fast => vec![],
                },
            ]
            .concat(),
            spawn: None,
            fields: presets::fields(&picked)?,
            ints: vec![],
            major: BTreeSet::new(),
            foreign: vec![],
            players: Default::default(),
            given_hints: None,
            colors: Default::default(),
            audio: Default::default(),
        })
    }
}

/// The default edits the chosen keys, starting inventory, bridge, trials,
/// fortress and Met Zelda make, before any edits of the player's own.
fn choice_edits(
    keys: DungeonKeys,
    fortress_keys: FortressKeys,
    start: &starting::Start,
    bridge: Bridge,
    trials: Trials,
    gerudo_fortress: GerudoFortress,
    met_zelda: bool,
) -> Vec<String> {
    [
        presets::softlocks(),
        keys::defaults(
            keys == DungeonKeys::Start,
            fortress_keys == FortressKeys::Start,
        ),
        start.defaults(),
        match bridge {
            Bridge::Open => crate::bridge::open(),
            Bridge::Vanilla | Bridge::SixMedallions => vec![],
        },
        match trials {
            Trials::Done => crate::bridge::trials_done(),
            Trials::AllSix => vec![],
        },
        match gerudo_fortress {
            GerudoFortress::Normal => vec![],
            GerudoFortress::Fast => crate::gerudo::fast(),
            GerudoFortress::Freed => crate::gerudo::freed(),
        },
        if met_zelda {
            presets::met_zelda()
        } else {
            vec![]
        },
    ]
    .concat()
}

/// Every toggle that's on, by id, counting parents, and each setting's
/// value as rules name it ([`value_word`]).
pub(crate) fn toggles_on(settings: &Settings) -> BTreeSet<String> {
    let schema = schema();
    let settings = &super::with_picks(&schema, settings);
    let on = (schema.iter())
        .filter(|d| is_on(&schema, settings, &d.id))
        .map(|d| d.id.clone());
    let values = (schema.iter()).filter_map(|d| {
        let value = match &d.kind {
            Kind::Toggle { .. } => [OFF, ON][usize::from(is_on(&schema, settings, &d.id))],
            Kind::Choice {
                default, options, ..
            } => (settings.get(&d.id))
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
pub(super) const ON: &str = "on";
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

/// The start words `settings` keep out, each with why ("Needs King Zora
/// Moved"): bottle 5 while King Zora hasn't moved, since five starting
/// bottles leave Ruto's Letter no slot ([`crate::bottles::check_start`]).
pub fn barred(settings: &Settings) -> Vec<(&'static str, String)> {
    let schema = schema();
    let id = presets::KING_ZORA_MOVED;
    if is_on(&schema, &super::with_picks(&schema, settings), id) {
        return Vec::new();
    }
    let label = schema.iter().find(|d| d.id == id).map_or(id, |d| d.label);
    vec![(BOTTLE_5, format!("Needs {label}"))]
}

/// `start` without the words `settings` keep out ([`barred`]). Whether
/// it changed.
pub fn fit_start(start: &mut starting::Start, settings: &Settings) -> bool {
    let held = |w: &&str| start.holds(w);
    let out: Vec<_> = barred(settings)
        .into_iter()
        .map(|(w, _)| w)
        .filter(held)
        .collect();
    out.iter().for_each(|w| _ = start.0.remove(*w));
    !out.is_empty()
}

/// [`fit_start`] on the settings' own start inventory, saying so when it
/// dropped something.
pub fn fit_bottles(settings: &mut Settings) -> Result<Option<String>> {
    let mut start = starting::Start::from_settings(settings)?;
    if !fit_start(&mut start, settings) {
        return Ok(None);
    }
    settings.insert(starting::SETTING.into(), serde_json::to_value(start)?);
    Ok(Some(
        "Dropped bottle 5 from the start inventory: five starting bottles need King Zora Moved"
            .into(),
    ))
}

/// The start word for the last bottle slot.
const BOTTLE_5: &str = "bottle-5";

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
    use crate::cutscenes::played_set;
    use crate::known::flag;
    use crate::presets::ADULT_DEKU_TREE;

    /// Every seed fills with logic unless logic is off. A default seed has
    /// no check to fill, so every item stays where it always is.
    #[test]
    fn every_seed_fills_unless_logic_is_off() {
        let of = |settings: &Settings| NewGame::from_settings(settings, &[]).unwrap().fills();
        assert!(of(&Settings::new()));
        let mut randomizer = super::super::favorites::randomizer();
        assert!(of(&randomizer));
        randomizer.insert(LOGIC.into(), choice_id(Logic::NoLogic).into());
        assert!(!of(&randomizer));
    }

    #[test]
    fn a_settled_start_marks_the_ammo_its_new_bags_hold() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut new_game = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        let nuts = format!("IsGet.{}=true", crate::known::item::DEKU_NUT);
        assert!(!new_game.defaults.contains(&nuts));
        let pouch = crate::inventory::Pouch::load(&romfs)
            .unwrap()
            .seeded(&new_game);
        new_game.settle_start(&pouch);
        assert!(new_game.start.holds("nut-bag"));
        assert!(new_game.defaults.contains(&nuts), "{:?}", new_game.defaults);
    }

    /// The softlock fixes, then `edits`: what a new game takes with every
    /// preset off.
    fn with_defaults(edits: &[&str]) -> Vec<String> {
        let edits: Vec<String> = edits.iter().map(|&e| e.into()).collect();
        presets::expand(&[], &[presets::softlocks(), edits].concat()).unwrap()
    }

    /// `with_defaults`, with `keys` and then `edits` after the presets.
    fn with_defaults_of(keys: &[String], edits: &[&str]) -> Vec<String> {
        let mut out = with_defaults(&[]);
        out.extend(keys.iter().cloned());
        out.extend(edits.iter().map(|&e| e.to_owned()));
        out
    }

    /// The Deku Tree's slingshot room starts solved, its plank down, so
    /// Link without a slingshot or bow isn't trapped there.
    #[test]
    fn every_new_game_starts_the_slingshot_room_solved() {
        let game = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        let solved = format!(
            "{}=true",
            crate::known::flag::INSIDE_THE_DEKU_TREE_ROOM_C_CLEARED
        );
        assert!(game.defaults.contains(&solved), "{:?}", game.defaults);
    }

    #[test]
    fn unset_settings_take_their_defaults() {
        let game = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        assert_eq!(
            game,
            NewGame {
                defaults: with_defaults(&[]),
                skips: Skips::default(),
                chest_size: ChestSize::Vanilla,
                chest_looks: false,
                vanilla: false,
                keep_chests: true,
                keep_gifts: true,
                start: starting::Start::of(&starting::DEFAULT),
                progressive: false,
                open_door: false,
                scrubs: false,
                scrub_price: ScrubPrice::default().rupees(),
                shop_slots: 0,
                freestanding: false,
                loose_items: false,
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
                gerudo_fortress: GerudoFortress::Normal,
                ice_traps: IceTraps::Off,
                dungeon_entrances: DungeonEntrances::Off,
                adult_jabu: AdultJabu::Off,
                door_pins: vec![],
                doors: Doors::default(),
                seed: 0,
                age: Era::Child,
                random_age: false,
                bombchus: Bombchus::BombBag,
                shuffle_nut_bag: false,
                shuffle_stick_bag: false,
                magic_with_first_item: false,
                quiet_navi: false,
                quiet_enemies: false,
                bunny_hood: false,
                day_skulltulas: false,
                breakables_respawn: false,
                bridge: Bridge::Vanilla,
                trials: Trials::AllSix,
                hints: false,
                spawn: None,
                fields: vec![],
                give: vec![],
                progress: vec![],
                ints: vec![],
                major: BTreeSet::new(),
                foreign: vec![],
                players: Default::default(),
                given_hints: None,
                colors: Default::default(),
                audio: Default::default(),
                logic: Logic::Glitchless,
                on: toggles_on(&Settings::new()),
            }
        );
    }

    /// #531: Chests alone shuffled nothing, as a hidden Shuffle items
    /// defaulted off and kept every item in place.
    #[test]
    fn chests_alone_shuffle_the_chests() {
        let chests = Settings::from([(CHESTS.into(), true.into())]);
        let game = NewGame::from_settings(&chests, &[]).unwrap();
        assert!(!game.vanilla && !game.keep_chests && game.fills());
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
            (PROGRESSIVE.into(), false.into()),
        ]);
        let game = NewGame::from_settings(&settings, &["A=1".into()]).unwrap();
        let rupees = format!("#{:08x}=50", crate::save_editor::RUPEES);
        let mut defaults = with_defaults(&[&rupees, "A=1"]);
        defaults.insert(0, format!("{}=true", flag::NPC_HYLIA_SOLDIER_003_GATE_OPEN));
        assert_eq!(game.defaults, defaults);
        assert!(game.skips.is_empty());
        assert_eq!(game.start.0.len(), 3);
        assert!(!game.vanilla && !game.progressive);
    }

    /// The defaults `settings` add over a new game's own.
    fn added(settings: &Settings) -> (NewGame, Vec<String>) {
        let base = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        let game = NewGame::from_settings(settings, &[]).unwrap();
        let added = (game.defaults.iter())
            .filter(|d| !base.defaults.contains(d))
            .cloned()
            .collect();
        (game, added)
    }

    /// Whether `edit` moves a quest step of `challenge`.
    fn steps(edit: &str, challenge: &crate::known::Name) -> bool {
        edit.starts_with(&format!("{}=", crate::steps::saved(challenge)))
    }

    #[test]
    fn an_open_bridge_writes_only_the_played_flag() {
        needs_names!();
        let open = Settings::from([(BRIDGE.into(), "open".into())]);
        let (game, added) = added(&open);
        let played = played_set(crate::known::field::GANON_CASTLE_ADULT_FIELD_RAINBOW);
        assert_eq!(added, [played]);
        assert_eq!(game.bridge, Bridge::Open);
        assert!(game.give.is_empty() && game.progress.is_empty());
        for quest in [
            crate::known::flow::ALL_SAGE,
            crate::known::flow::DEFEAT_GANON,
        ] {
            assert!(!game.defaults.iter().any(|d| steps(d, quest)), "{quest}");
        }
    }

    #[test]
    fn trials_done_write_only_the_barrier_state() {
        needs_names!();
        let done = Settings::from([(TRIALS.into(), "done".into())]);
        let (game, added) = added(&done);
        let field = crate::known::field::INSIDE_GANON_CASTLE_DUNGEON_BARRIER_COUNT;
        assert!(added.contains(&format!("{field}=0")));
        let clear = crate::known::field::INSIDE_GANON_CASTLE_IS_ALL_BARRIER_CLEAR;
        assert!(added.contains(&format!("{clear}=true")));
        for trial in ["Light", "Forest", "Water", "Shadow", "Fire", "Spirit"] {
            let removed = format!("InsideGanonCastle_Dungeon_BarrierRemove_{trial}");
            let played = played_set(&removed);
            assert!(added.contains(&played), "{trial}");
            assert!(added.contains(&format!("{removed}=true")), "{trial}");
        }
        assert_eq!(added.len(), 14, "{added:?}");
        assert_eq!(game.trials, Trials::Done);
        assert!(game.give.is_empty() && game.progress.is_empty());
        let quest = crate::known::flow::DEFEAT_GANON;
        assert!(!game.defaults.iter().any(|d| steps(d, quest)));
        let bridge = crate::known::field::GANON_CASTLE_ADULT_FIELD_RAINBOW;
        assert!(!game.defaults.iter().any(|d| d.contains(&**bridge)));
    }

    #[test]
    fn a_choice_reads_its_id_and_falls_back_to_its_default() {
        let size = |v: &str| {
            let settings = Settings::from([(CHEST_SIZE.into(), v.into())]);
            NewGame::from_settings(&settings, &[]).unwrap().chest_size
        };
        assert_eq!(size("small"), ChestSize::Small);
        assert_eq!(size("matches"), ChestSize::Matches);
        assert_eq!(size("huge"), ChestSize::Vanilla);
        let def = schema().into_iter().find(|d| d.id == CHEST_SIZE).unwrap();
        let Kind::Choice {
            default, options, ..
        } = def.kind
        else {
            panic!()
        };
        assert_eq!(default, "vanilla");
        let ids: Vec<_> = options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["vanilla", "small", "matches"]);
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
        let rings = Settings::from([(KEYS.into(), "rings".into())]);
        let settings = crate::settings::favorites::randomizer_and(rings);
        let game = NewGame::from_settings(&settings, &[]).unwrap();
        assert_eq!(game.keys, DungeonKeys::Rings);
        assert!(!game.defaults.iter().any(|d| d.contains("DungeonInfo")));
        let singles = Settings::from([(KEYS.into(), "singles".into())]);
        let game = NewGame::from_settings(&singles, &[]).unwrap();
        assert_eq!(game.keys, DungeonKeys::Singles);
        let def = schema().into_iter().find(|d| d.id == KEYS).unwrap();
        let Kind::Choice {
            default, options, ..
        } = def.kind
        else {
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
        let opened = [DOOR_OPEN, presets::OPEN_FOREST];
        assert!(opened.iter().all(|id| new_game.on.contains(*id)));
        // Only the start inventory gives it: the pedestal waits for a pull.
        assert!(!new_game.start.holds("master-sword"));
        let closed = Settings::from([
            (STARTING_AGE.into(), "adult".into()),
            (DOOR_OF_TIME.into(), "closed".into()),
        ]);
        let closed = NewGame::from_settings(&closed, &[]).unwrap();
        assert!(closed.open_door && closed.on.contains(DOOR_OPEN));
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
        // Whichever it rolls, the door and the forest open, as the app
        // showed before the roll.
        let opened = [DOOR_OPEN, presets::OPEN_FOREST];
        for seed in 0..16 {
            let (settings, _) = super::super::roll_age(&random, seed);
            let new_game = NewGame::from_settings(&settings, &[]).unwrap();
            assert!(new_game.open_door, "seed {seed}");
            assert!(
                opened.iter().all(|id| new_game.on.contains(*id)),
                "seed {seed}"
            );
        }
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
    fn fortress_keys_go_with_every_gerudo_fortress() {
        let fortress = |keys: &str, carpenters: &str| {
            let settings = Settings::from([
                (FORTRESS_KEYS.into(), keys.into()),
                (GERUDO_FORTRESS.into(), carpenters.into()),
            ]);
            NewGame::from_settings(&settings, &[])
        };
        for keys in ["vanilla", "start", "singles"] {
            for carpenters in ["normal", "fast", "freed"] {
                assert!(fortress(keys, carpenters).is_ok(), "{keys} {carpenters}");
            }
        }
    }

    #[test]
    fn the_gerudo_fortress_choice_writes_its_edits_and_moves_the_quest_only_when_freed() {
        needs_names!();
        let game = |carpenters: &str| {
            let settings = Settings::from([(GERUDO_FORTRESS.into(), carpenters.into())]);
            NewGame::from_settings(&settings, &[]).unwrap()
        };
        let normal = game("normal");
        assert_eq!(normal.gerudo_fortress, GerudoFortress::Normal);
        assert!(normal.progress.is_empty() && normal.give.is_empty());
        let fast = game("fast");
        assert!(
            crate::gerudo::fast()
                .iter()
                .all(|e| fast.defaults.contains(e))
        );
        assert!(fast.progress.is_empty() && fast.give.is_empty());
        assert!(fast.on.contains(FORTRESS_FAST));
        let freed = game("freed");
        assert!(
            crate::gerudo::freed()
                .iter()
                .all(|e| freed.defaults.contains(e))
        );
        assert_eq!(freed.progress, [crate::gerudo::freed_step()]);
        assert!(freed.on.contains(FORTRESS_FREED) && !freed.on.contains(FORTRESS_FAST));
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

    #[test]
    fn turning_king_zora_moved_off_drops_bottle_5() {
        let five: Vec<String> = (1..=5).map(|i| format!("bottle-{i}")).collect();
        let start = starting::Start::of(&five.iter().map(String::as_str).collect::<Vec<_>>());
        let mut settings = Settings::from([
            (
                starting::SETTING.into(),
                serde_json::to_value(&start).unwrap(),
            ),
            (presets::KING_ZORA_MOVED.into(), true.into()),
        ]);
        assert_eq!(fit_bottles(&mut settings).unwrap(), None);
        assert!(NewGame::from_settings(&settings, &[]).is_ok());
        settings.insert(presets::KING_ZORA_MOVED.into(), false.into());
        let err = NewGame::from_settings(&settings, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("King Zora Moved"), "{err}");
        assert_eq!(barred(&settings)[0].1, "Needs King Zora Moved");
        assert!(fit_bottles(&mut settings).unwrap().is_some());
        let kept = starting::Start::from_settings(&settings).unwrap();
        assert_eq!(kept.bottles(), 4);
        assert!(!kept.holds(BOTTLE_5));
        assert!(NewGame::from_settings(&settings, &[]).is_ok());
    }
}
