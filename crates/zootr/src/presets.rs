//! New-game presets: GameData defaults a fresh save starts from, or edits
//! applied to an existing save.

use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use clap::builder::{PossibleValue, PossibleValuesParser};
use ounce_formats::Byml;

use crate::gamedata::GameData;
use crate::known::{Name, actor, field, flag, flow, item};
use crate::savefile;
use crate::settings::{Place, place};
use crate::value;

/// Every flag in `names` set to `value`.
fn all(names: impl IntoIterator<Item = impl std::fmt::Display>, value: &str) -> Vec<String> {
    names.into_iter().map(|n| format!("{n}={value}")).collect()
}

/// `edits` as owned strings.
fn lit(edits: &[&str]) -> Vec<String> {
    edits.iter().map(|&e| e.into()).collect()
}

pub struct Preset {
    pub name: &'static str,
    pub label: &'static str,
    pub what: &'static str,
    /// Where the settings show it.
    pub place: Place,
    /// Its `NAME=VALUE` edits.
    edits: fn() -> Vec<String>,
    /// Pouch items a new game is handed, by actor. A save can't take these.
    pub give: &'static [&'static Name],
    /// Quest steps a new game's opening moves on, as `(challenge, step)`.
    /// A save edit can't: the challenge system keeps its own state.
    pub progress: &'static [(&'static Name, &'static str)],
    /// Placement fields the seed's maps take, as `HASH.PATH=VALUE`. A save
    /// edit can't: they live in the seed.
    fields: fn() -> Vec<String>,
}

impl Preset {
    /// Whether only a new game can take it: it hands over items, moves a
    /// quest on or edits the seed's maps.
    fn new_only(&self) -> bool {
        !self.give.is_empty() || !self.progress.is_empty() || !(self.fields)().is_empty()
    }
}

/// The preset whose four carpenters are already freed.
pub const CARPENTERS_FREED: &str = "carpenters-freed";
/// The preset whose first carpenter freed counts as all four.
pub const CARPENTERS_FAST: &str = "carpenters-fast";

/// Presets a new game takes unless turned off.
pub const DEFAULT_ON: [&str; 4] = ["menus", "clock", "no-tutorials", "no-pickup-texts"];

const STONES: [&Name; 3] = [
    flow::GET_STONE_OF_FOREST,
    flow::GET_STONE_OF_FIRE,
    flow::GET_STONE_OF_WATER,
];
const TRIALS: [&str; 6] = ["Light", "Forest", "Water", "Shadow", "Fire", "Spirit"];

/// Zelda met and her lullaby learned, with the Door of Time quest at `door`.
fn zelda(door: &str) -> Vec<String> {
    let mut edits = lit(&[
        &crate::steps::at(flow::GOTO_HYRULE_CASTLE, "Complete"),
        &format!("IsGet.{}=true", item::MELODY_ZELDA_LULLABY),
    ]);
    edits.push(crate::steps::at(flow::DOOR_OF_TIME, door));
    edits
}

/// The four carpenters freed and their guards beaten. The quest step isn't
/// here: the opening moves it, as freeing the carpenters does, so the
/// fortress guards stand down with it.
fn carpenters() -> Vec<String> {
    let mut edits = lit(&[
        &format!(
            "{}=false",
            field::GERUDO_FORTRESS_NPC_GERUDO_001_IS_IN_FIRST_PLACE
        ),
        &format!(
            "{}=4",
            field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED
        ),
    ]);
    let cells = (13..=16).flat_map(|n| {
        ["DestroyGerudoThief", "IsAfterHelpCarpenter"]
            .map(|what| format!("GerudoFortress_{what}_Npc_Hylia_0{n}_Adult"))
    });
    edits.extend(all(cells, "true"));
    edits.extend(all(
        [
            flag::GERUDO_FORTRESS_WARP_POINT01_CLEAR_EVENT,
            flag::GERUDO_FORTRESS_WARP_POINT02_CLEAR_EVENT,
            flag::GERUDO_FORTRESS_WARP_POINT03_CLEAR_EVENT,
            flag::GERUDO_FORTRESS_WARP_POINT04_CLEAR_EVENT,
        ]
        .map(|f| f.to_string()),
        "true",
    ));
    edits
}

/// Three carpenters counted as freed, so freeing any one plays the clear
/// event that frees all four (`gerudo::clear_once`). Every cell still
/// holds its carpenter, behind her thief.
fn carpenters_fast() -> Vec<String> {
    vec![
        format!(
            "{}=3",
            field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED
        ),
        format!(
            "{}=1",
            field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED_02
        ),
    ]
}

/// Every sage awake and the rainbow bridge up, with Ganon's quest at `step`.
fn ganon(step: &str) -> Vec<String> {
    let mut edits = lit(&[
        &crate::steps::at(flow::ALL_SAGE, "Complete"),
        &format!(
            "{}.{}=true",
            flag::HAVE_PLAYED_EVENT,
            field::GANON_CASTLE_ADULT_FIELD_RAINBOW
        ),
    ]);
    edits.push(crate::steps::at(flow::DEFEAT_GANON, step));
    edits
}

/// The guide pop-ups a menu page opens the first time it fills, all off.
fn page_guides() -> Vec<String> {
    let pages = [
        field::UI_IS_NEED_OPEN_PLUS_MENU_EQUIP_ACTION_GUIDE,
        field::UI_IS_NEED_OPEN_PLUS_MENU_HEAD_ACTION_GUIDE,
        field::UI_IS_NEED_OPEN_PLUS_MENU_BOOT_ACTION_GUIDE,
        field::UI_IS_NEED_OPEN_PLUS_MENU_ITEM_ACTION_GUIDE,
    ]
    .map(|p| p.to_string());
    all(
        pages
            .into_iter()
            .chain([flag::UI_IS_NEED_OPEN_SHORT_CUT_ACTION_GUIDE.to_string()]),
        "false",
    )
}

/// Every `flag::ACTION_GUIDE_IS_END` member: one bool per guide, which the game
/// sets once the guide has been shown.
static GUIDES: LazyLock<[&str; 53]> = LazyLock::new(|| {
    [
        field::ACTIVATION_AMIIBO,
        flag::AGONY_STONE,
        flag::ALARM_CUCCO,
        "Amiibo",
        "Backflip",
        "Bomb",
        "Bombchu",
        "Boomerang",
        field::BOOT_SHORTCUT,
        "Bottle",
        "Bow",
        flag::DEKU_NUT,
        flag::DEKU_STICK,
        flag::DIN_FIRE,
        field::EQUIPMENT_MENU,
        field::EQUIPMENT_MENU_FOR_BOOT,
        field::EQUIPMENT_MENU_FOR_HEAD_ARMOR,
        flag::FARORE_WIND,
        flag::FIRE_ARROW,
        flag::FISHING_ROD,
        field::GORON_GAUNTLET,
        "Hammer",
        field::HEAD_SHORTCUT,
        "Hookshot",
        field::HORIZONTAL_SLASH,
        flag::ICE_ARROW,
        field::ITEM_MENU,
        field::ITEM_SHORTCUT,
        field::JUMP_SLASH,
        field::LIGHT_ARROW,
        field::LOCK_ON,
        field::LOCK_ON_SHIELD,
        field::LOCK_ON_SIDE_MOVE,
        "Longshot",
        flag::MAGIC_BEAN,
        "Navi",
        flag::NAYRU_LOVE,
        "Ocarina",
        field::OCARINA_SONG_ESTIMATE,
        flag::ROLL_ATTACK,
        field::ROTARY_SLASH,
        "Save",
        flag::SCRUB_BUSINESS_SEED,
        field::SIDE_JUMP,
        flag::SINKING_LURE,
        flag::SLING_SHOT,
        field::SYSTEM_MINUS,
        "Thrust",
        field::TIME_TRAIL,
        field::VERTICAL_SLASH,
        actor::WEAPON_SHIELD_HYLIAN,
        actor::WEAPON_SHIELD_MIRROR,
        flag::ZELDA_LETTER,
    ]
});

/// Every guide shown, and the House of Skulltula's own mark that its Echo
/// Gem guide has been, which skips the camera move that leads into it. The
/// Kokiri on the shop roof stops Link for her targeting guide until her
/// event counts as played.
fn no_tutorials() -> Vec<String> {
    let mut edits = page_guides();
    edits.extend(all(
        GUIDES.map(|g| format!("{}.{g}", flag::ACTION_GUIDE_IS_END)),
        "true",
    ));
    edits.push(format!(
        "{}=true",
        field::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_ACTION_GUIDE_AGONY_STONE_TRUE
    ));
    edits.push(format!(
        "{}.{}=true",
        flag::HAVE_PLAYED_EVENT,
        flag::KOKIRI_FOREST_KOKIRI_011_STOP_IN_FRONT_OF_SHOP_00
    ));
    edits
}

/// The drops whose first pickup stops Link with a get text ("You got a
/// recovery heart!"). Each one's `IsGet` only marks that text shown: the
/// `Item_` drops (a Deku nut, a bomb) are left out, as theirs is owning
/// the item.
const PICKUPS: [&Name; 13] = [
    actor::OBJ_HEART,
    actor::OBJ_FAIRY,
    actor::OBJ_RUPEE_GREEN,
    actor::OBJ_RUPEE_BLUE,
    actor::OBJ_RUPEE_RED,
    actor::OBJ_RUPEE_PURPLE,
    actor::OBJ_RUPEE_GOLD,
    actor::OBJ_MAGIC_JAR_SMALL,
    actor::OBJ_MAGIC_JAR_LARGE,
    actor::OBJ_ARROW_BUNDLE_SMALL,
    actor::OBJ_ARROW_BUNDLE_MEDIUM,
    actor::OBJ_ARROW_BUNDLE_LARGE,
    actor::OBJ_DEKU_SEED,
];

/// Every drop in [`PICKUPS`] counts as picked up before.
fn no_pickup_texts() -> Vec<String> {
    all(PICKUPS.map(|p| format!("IsGet.{p}")), "true")
}

const LIGHT_ARROWS: &[&Name] = &[item::ARROW_LIGHT];

/// The preset whose King Zora already stands aside.
pub const KING_ZORA_MOVED: &str = "king-zora-moved";

/// The preset whose Jabu-Jabu takes Link in without a fish.
pub const JABU_OPEN: &str = "jabu-open";

/// Jabu-Jabu's mouth, the door into his belly in child Zora's Fountain.
const JABU_MOUTH: u64 = 4228704915231375030;

/// The mouth door's first arrival runs what the fish's map change boots:
/// the only event that moves the water stone quest to `Step09`, which
/// Ruto's schedules and the belly's rooms wait on.
fn jabu_arrival() -> Vec<String> {
    let event = flow::GET_STONE_OF_WATER_ARRIVE_JABU_JABU_BELLY;
    let played = format!("{}.{event}", flag::HAVE_PLAYED_EVENT);
    [
        (field::INIT_EVENT_NAME, event.to_string()),
        (field::INIT_EVENT_FLAG, played),
    ]
    .map(|(k, v)| format!("{JABU_MOUTH}.{}.{k}={v}", field::DYNAMIC))
    .into()
}

/// The preset that opens the forest.
pub const OPEN_FOREST: &str = "open-forest";

/// The preset whose Deku Tree takes adult Link in.
pub const ADULT_DEKU_TREE: &str = "adult-deku-tree";

/// The debris the adult Kokiri Forest piles in front of the Deku Tree's
/// mouth, which the child map doesn't have. Their collision is baked into
/// the map's physics, so moving one leaves it behind. Kept from spawning,
/// the game drops it with them.
const DEKU_TREE_DEBRIS: [u64; 8] = [
    // The invisible collision the adult tree stands on: the root top that
    // walls Link in when he leaves the tree.
    1873139170811345887,
    // Two mossy rocks, three fallen props and a stump.
    15123958794573127274,
    3018160259590756533,
    3542030604015290706,
    16680444440944320445,
    14042205327208366681,
    3365184451401579984,
    // A ground piece that rested on the props, left floating without them.
    4492590912570771242,
];

/// Each piece of [`DEKU_TREE_DEBRIS`] spawns only for child Link. Its map
/// is adult-only, so never.
fn deku_tree_clear() -> Vec<String> {
    let child = flag::SEQUENCE_IS_CHILDHOOD;
    DEKU_TREE_DEBRIS
        .iter()
        .map(|h| format!("{h}.Presence.{}={child}", field::FLAG_NAME))
        .collect()
}

pub const PRESETS: [Preset; 17] = [
    Preset {
        name: OPEN_FOREST,
        label: "Open Forest",
        what: "Mido and the forest exit stop blocking Link, as child or adult",
        place: place::OPEN_AREAS,
        give: &[],
        progress: &[],
        fields: Vec::new,
        edits: || {
            all(
                [
                    flag::KOKIRI_FOREST_NPC_KOKIRI_MIDO_CHILD_BLOCKER,
                    flag::KOKIRI_FOREST_NPC_KOKIRI_007_CHILD_BLOCKER_00,
                    flag::KOKIRI_FOREST_NPC_KOKIRI_MIDO_ADULT_BLOCKER,
                ],
                "false",
            )
        },
    },
    Preset {
        name: "kakariko-gate",
        label: "Open Kakariko Gate",
        what: "The Kakariko gate guard has already seen Zelda's letter, so the Death Mountain trail is open from the start",
        place: place::OPEN_AREAS,
        edits: || all([flag::NPC_HYLIA_SOLDIER_003_GATE_OPEN], "true"),
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: KING_ZORA_MOVED,
        label: "King Zora Moved",
        what: "King Zora has read Ruto's letter and stands aside for child Link, as he does once he's shown it. Ruto's Letter becomes an empty bottle, shuffled from its spot at the bottom of the lake",
        place: place::OPEN_AREAS,
        // Showing him the letter moves the quest on, then plays the scene
        // whose played flag hides the air wall beside him. The quest step
        // is what moves him aside. The domain's first arrival from the
        // river sets an earlier step, so it counts as played too.
        edits: || {
            all(
                [
                    flag::HAVE_PLAYED_EVENT_DM_ZORA_0000,
                    flag::HAVE_PLAYED_EVENT_DM_ZORA_0010,
                ],
                "true",
            )
        },
        give: &[],
        progress: &[(flow::GET_STONE_OF_WATER, "Step07")],
        fields: Vec::new,
    },
    Preset {
        name: JABU_OPEN,
        label: "Jabu-Jabu Open",
        what: "Jabu-Jabu's mouth is open, so child Link walks in without giving him a fish",
        place: place::OPEN_AREAS,
        edits: || {
            all(
                [flag::JAB_JAB_OPENED_MOUTH, flag::GAVE_FISH_TO_JAB_JAB],
                "true",
            )
        },
        give: &[],
        progress: &[],
        fields: jabu_arrival,
    },
    Preset {
        name: "open-waterfall",
        label: "Zora's River Waterfall Open",
        what: "The waterfall at the end of Zora's River already stands split, so Link reaches Zora's Domain without Zelda's Lullaby",
        place: place::OPEN_AREAS,
        edits: || {
            all(
                [
                    flag::ZORARIVER_THE_WATERFALLSPLITS,
                    flag::ZORARIVER_THE_WATERFALLSPLITS_ADULT,
                ],
                "true",
            )
        },
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: ADULT_DEKU_TREE,
        label: "Adult Deku Tree",
        what: "The debris in front of the Deku Tree's mouth is gone, so adult Link walks in and out",
        place: place::OPEN_AREAS,
        edits: Vec::new,
        give: &[],
        progress: &[],
        fields: deku_tree_clear,
    },
    Preset {
        name: "menus",
        label: "Menus Open",
        what: "Menu pages skip their guide pop-up the first time they fill, and the challenge tapestry is open from the start, without the reveal it plays the first time it opens",
        place: place::COMFORT,
        // The tapestry's menu graph (`Sequence/ChallengeLog`) plays its
        // first-time reveal while the second flag reads false.
        edits: || {
            let mut edits = page_guides();
            edits.extend(all(
                [
                    field::UI_IS_FINISH_CHALLENGE_MENU_OPEN_EVENT,
                    field::UI_IS_OPEN_CHALLENGE_MENU_ONCE,
                ],
                "true",
            ));
            edits
        },
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "clock",
        label: "Clock Runs",
        what: "Time passes from the start, as it does once Link first walks into Hyrule Field. Off, the clock waits for the field, even in a seed that starts past it",
        place: place::COMFORT,
        // The field's first arrival sets it. The game's clock reads it by
        // hash, so nothing in the data names it as the gate.
        edits: || all([flag::HYRULE_FIELD_ARRIVE_ENTRANCE], "true"),
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "zelda-done",
        label: "Met Zelda",
        what: "Child Zelda met and her lullaby learned, with her letter in hand",
        place: place::QUEST,
        edits: || zelda("Step02"),
        give: &[item::ZELDA_LETTER],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "door-of-time",
        label: "Door of Time Open",
        what: "The three stones, the Ocarina of Time and its song, and the Door of Time open",
        place: place::QUEST,
        edits: || {
            let mut edits = zelda("Step06");
            edits.extend(all(STONES.map(crate::steps::saved), "Complete"));
            edits.extend(lit(&[
                &format!("IsGet.{}=true", item::MELODY_TIME_SONG),
                &format!("{}=true", flag::MARKET_CHILD_TIME_TEMPLE_DOOR_OPENED),
            ]));
            edits
        },
        give: &[
            item::STONE_FOREST,
            item::STONE_FIRE,
            item::STONE_WATER,
            item::OCARINA_TIME,
        ],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: CARPENTERS_FREED,
        label: "Carpenters Freed",
        what: "All four carpenters freed, the membership card in hand, the guards standing down and the valley bridge up",
        place: place::QUEST,
        edits: carpenters,
        give: &[item::TOKEN_GERUDO],
        progress: &[(flow::SAGE_OF_SPIRIT, "Step02")],
        fields: Vec::new,
    },
    Preset {
        name: CARPENTERS_FAST,
        label: "Gerudo Fortress Fast",
        what: "Freeing any one carpenter frees all four and earns the membership card. The other thieves still guard their cells",
        place: place::QUEST,
        edits: carpenters_fast,
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "rainbow-bridge",
        label: "Rainbow Bridge Up",
        what: "Every sage awake, the Light Arrows in hand and the bridge to Ganon's Castle up",
        place: place::QUEST,
        edits: || ganon("Step02"),
        give: LIGHT_ARROWS,
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "trials-done",
        label: "Trials Done",
        what: "The rainbow bridge, and all six barriers in Ganon's Castle down",
        place: place::QUEST,
        edits: || {
            let mut edits = ganon("Step04");
            edits.extend(lit(&[
                &format!("{}=0", field::INSIDE_GANON_CASTLE_DUNGEON_BARRIER_COUNT),
                &format!("{}=true", field::INSIDE_GANON_CASTLE_IS_ALL_BARRIER_CLEAR),
            ]));
            let removed = TRIALS.iter().flat_map(|t| {
                let flag = format!("InsideGanonCastle_Dungeon_BarrierRemove_{t}");
                [format!("{}.{flag}", flag::HAVE_PLAYED_EVENT), flag]
            });
            edits.extend(all(removed, "true"));
            edits
        },
        give: LIGHT_ARROWS,
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "claim-check",
        label: "Claim Check",
        what: "The trade quest's last step, with the Claim Check in hand",
        place: place::QUEST,
        edits: || {
            vec![crate::steps::at(
                flow::SUB_EVENT_GET_SWORD_BIG_GORON,
                "Step11",
            )]
        },
        give: &[item::CLAIM_CHECK],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "no-tutorials",
        label: "No Tutorials",
        what: "Every guide screen counts as already shown, so none pops up the first time, such as the Echo Gem's in the House of Skulltula. The Great Fairy's guide still shows",
        place: place::COMFORT,
        edits: no_tutorials,
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
    Preset {
        name: "no-pickup-texts",
        label: "No Pickup Texts",
        what: "Recovery hearts, fairies, rupees, magic jars, arrows and Deku seeds count as picked up before, so the first one doesn't stop Link with its get text",
        place: place::COMFORT,
        edits: no_pickup_texts,
        give: &[],
        progress: &[],
        fields: Vec::new,
    },
];

/// For clap: each preset's name, with what it does as its help.
pub fn parser() -> PossibleValuesParser {
    PossibleValuesParser::new(
        PRESETS
            .iter()
            .map(|p| PossibleValue::new(p.name).help(p.what)),
    )
}

/// The presets an existing save can take: those handing over no items and
/// moving no quest.
pub fn for_saves() -> impl Iterator<Item = &'static Preset> {
    PRESETS.iter().filter(|p| !p.new_only())
}

/// The presets `names` stand for.
fn find(names: &[String]) -> Result<Vec<&'static Preset>> {
    names
        .iter()
        .map(|n| {
            PRESETS
                .iter()
                .find(|p| p.name == n)
                .with_context(|| format!("no preset {n}"))
        })
        .collect()
}

/// Every edit `presets` stand for, followed by the explicit `edits`.
pub fn expand(presets: &[String], edits: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = find(presets)?.iter().flat_map(|p| (p.edits)()).collect();
    out.extend(edits.iter().cloned());
    Ok(out)
}

/// Every item `presets` hand a new game.
pub fn gives(presets: &[String]) -> Result<Vec<String>> {
    Ok(find(presets)?
        .iter()
        .flat_map(|p| p.give.iter().map(|&g| g.to_string()))
        .collect())
}

/// Every quest step `presets` move on in a new game's opening.
pub fn progress(presets: &[String]) -> Result<Vec<(String, String)>> {
    Ok(find(presets)?
        .iter()
        .flat_map(|p| p.progress.iter().map(|&(c, s)| (c.to_string(), s.into())))
        .collect())
}

/// Every placement field `presets` set in the seed's maps.
pub fn fields(presets: &[String]) -> Result<Vec<String>> {
    Ok(find(presets)?.iter().flat_map(|p| (p.fields)()).collect())
}

/// [`expand`] for an existing save, which can't be handed items or have its
/// quests moved.
pub fn for_save(presets: &[String], edits: &[String]) -> Result<Vec<String>> {
    if let Some(p) = find(presets)?.iter().find(|p| p.new_only()) {
        let what = match p.give.is_empty() {
            true if p.progress.is_empty() => "edits the seed's maps".to_owned(),
            true => "moves a quest on".to_owned(),
            false => format!(
                "hands over {}",
                p.give
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        bail!(
            "{} also {what}, which only a new game can. Start one with `shuffle --preset {}`",
            p.name,
            p.name
        );
    }
    expand(presets, edits)
}

/// Set each edit's `DefaultValue` in the GameData document. Returns each as
/// `name: old -> new`.
pub fn apply(gd: &GameData, root: &mut Byml, edits: &[String]) -> Result<Vec<String>> {
    let tables = root
        .as_mut_map()?
        .get_mut("Data")
        .context("no Data")?
        .as_mut_map()?;
    let mut log = Vec::new();
    for edit in edits {
        let (name, text) = savefile::split(edit)?;
        let hash = savefile::hash_of(name);
        let table = gd
            .kind(hash)
            .with_context(|| format!("{name} isn't in {}", flag::GAME_DATA))?;
        let raw = value::scalar(gd, table, text).context(name.to_owned())?;
        let hash = i64::from(hash);
        let entry = tables
            .get_mut(table)
            .context(table.to_owned())?
            .as_mut_array()?
            .iter_mut()
            .find(|e| e["Hash"].as_int::<i64>().is_ok_and(|h| h == hash))
            .context(name.to_owned())?
            .as_mut_map()?;
        let old = entry
            .get(&**field::DEFAULT_VALUE)
            .with_context(|| format!("{name} has no default"))?;
        let was = savefile::raw(old).map_or("?".into(), |r| value::render(gd, table, r));
        let new = savefile::to_default(old, raw)
            .with_context(|| format!("{table} defaults can't be set"))?;
        log.push(format!("{name}: {was} -> {text}"));
        entry.insert(field::DEFAULT_VALUE.to_string().into(), new);
    }
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, flow, item};
    use ounce_formats::murmur3;

    #[test]
    fn expands_presets_then_edits() {
        let edits = expand(&["open-forest".into()], &["X=1".into()]).unwrap();
        assert_eq!(edits.len(), 4);
        assert_eq!(
            edits[0],
            format!(
                "{}=false",
                flag::KOKIRI_FOREST_NPC_KOKIRI_MIDO_CHILD_BLOCKER
            )
        );
        assert_eq!(edits.last().unwrap(), "X=1");
        assert!(expand(&["nope".into()], &[]).is_err());
    }

    #[test]
    fn the_pickup_preset_marks_each_drop_got_and_no_item() {
        let edits = expand(&["no-pickup-texts".into()], &[]).unwrap();
        assert_eq!(edits.len(), PICKUPS.len());
        assert!(edits.contains(&format!("IsGet.{}=true", actor::OBJ_HEART)));
        assert!(edits.iter().all(|e| !e.contains("Item_")));
        assert!(DEFAULT_ON.contains(&"no-pickup-texts"));
    }

    #[test]
    fn the_clock_preset_marks_the_field_reached() {
        let edits = expand(&["clock".into()], &[]).unwrap();
        assert_eq!(
            edits,
            [format!("{}=true", flag::HYRULE_FIELD_ARRIVE_ENTRANCE)]
        );
        assert!(DEFAULT_ON.contains(&"clock"));
    }

    #[test]
    fn the_menus_preset_opens_the_tapestry_past_its_first_reveal() {
        let edits = expand(&["menus".into()], &[]).unwrap();
        for flag in [
            field::UI_IS_FINISH_CHALLENGE_MENU_OPEN_EVENT,
            field::UI_IS_OPEN_CHALLENGE_MENU_ONCE,
        ] {
            assert!(edits.contains(&format!("{flag}=true")), "{flag}");
        }
    }

    #[test]
    fn the_no_tutorials_preset_marks_every_guide_shown_and_closes_the_page_guides() {
        needs_names!();
        let edits = expand(&["no-tutorials".into()], &[]).unwrap();
        let has = |e: &str| edits.contains(&e.to_owned());
        assert_eq!(
            edits
                .iter()
                .filter(|e| e.starts_with(&format!("{}.", flag::ACTION_GUIDE_IS_END)))
                .count(),
            GUIDES.len()
        );
        assert!(
            has(&format!(
                "{}.{}=true",
                flag::ACTION_GUIDE_IS_END,
                flag::AGONY_STONE
            )) && has(&format!("{}.Navi=true", flag::ACTION_GUIDE_IS_END))
        );
        assert!(has(&format!(
            "{}=true",
            field::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_ACTION_GUIDE_AGONY_STONE_TRUE
        )));
        assert!(has(&format!(
            "{}.{}=true",
            flag::HAVE_PLAYED_EVENT,
            flag::KOKIRI_FOREST_KOKIRI_011_STOP_IN_FRONT_OF_SHOP_00
        )));
        assert!(has(&format!(
            "{}=false",
            field::UI_IS_NEED_OPEN_PLUS_MENU_EQUIP_ACTION_GUIDE
        )));
        assert!(has(&format!(
            "{}=false",
            flag::UI_IS_NEED_OPEN_SHORT_CUT_ACTION_GUIDE
        )));
        assert!(!has(&format!(
            "{}=true",
            field::UI_IS_FINISH_CHALLENGE_MENU_OPEN_EVENT
        )));
        assert!(DEFAULT_ON.contains(&"no-tutorials"));
    }

    #[test]
    fn the_guide_list_is_every_member_of_the_shown_struct() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let members = gd
            .members(murmur3(flag::ACTION_GUIDE_IS_END.as_bytes()))
            .unwrap();
        let mut ours: Vec<_> = GUIDES
            .iter()
            .map(|g| murmur3(format!("{}.{g}", flag::ACTION_GUIDE_IS_END).as_bytes()))
            .collect();
        let mut theirs: Vec<_> = members.iter().map(|&(_, bool)| bool).collect();
        assert!(theirs.iter().all(|&b| gd.kind(b) == Some("Bool")));
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn every_preset_applies_to_the_game_and_gives_pouch_items() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let mut doc =
            ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        let pouch = crate::items::scan(&romfs).unwrap();
        for p in &PRESETS {
            let edits = (p.edits)();
            let names: std::collections::HashSet<_> =
                edits.iter().map(|e| e.split_once('=').unwrap().0).collect();
            assert_eq!(names.len(), edits.len(), "{} sets a flag twice", p.name);
            apply(&gd, &mut doc.root, &edits).unwrap_or_else(|e| panic!("{}: {e:#}", p.name));
            for g in p.give {
                assert!(pouch.iter().any(|i| i.actor == *g), "{}: {g}", p.name);
            }
        }
    }

    #[test]
    fn quest_presets_land_on_their_step() {
        needs_names!();
        let edits = |p: &str| expand(&[p.into()], &[]).unwrap();
        assert!(edits("door-of-time").contains(&crate::steps::at(flow::DOOR_OF_TIME, "Step06")));
        assert!(edits("trials-done").contains(&crate::steps::at(flow::DEFEAT_GANON, "Step04")));
        assert_eq!(
            edits("carpenters-freed")
                .iter()
                .filter(|e| e.contains("IsAfterHelpCarpenter"))
                .count(),
            4
        );
        assert_eq!(
            gives(&["rainbow-bridge".into(), "claim-check".into()]).unwrap(),
            [item::ARROW_LIGHT, item::CLAIM_CHECK]
        );
        assert!(PRESETS.iter().any(|p| p.place == place::QUEST));
    }

    #[test]
    fn fast_carpenters_count_three_freed_and_take_a_save() {
        let edits = expand(&[CARPENTERS_FAST.into()], &[]).unwrap();
        let helped = field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED;
        let left = field::GERUDO_FORTRESS_MEET_FOUR_CARPENTER_NUM_OF_CARPENTER_HELPED_02;
        assert_eq!(edits, [format!("{helped}=3"), format!("{left}=1")]);
        assert!(for_save(&[CARPENTERS_FAST.into()], &[]).is_ok());
        let fast = PRESETS.iter().find(|p| p.name == CARPENTERS_FAST).unwrap();
        assert_eq!(fast.place, place::QUEST);
    }

    #[test]
    fn freed_carpenters_move_the_quest_in_the_opening_not_in_the_save() {
        let edits = expand(&["carpenters-freed".into()], &[]).unwrap();
        assert!(edits.contains(&format!(
            "{}=false",
            field::GERUDO_FORTRESS_NPC_GERUDO_001_IS_IN_FIRST_PLACE
        )));
        assert!(
            !edits
                .iter()
                .any(|e| e.starts_with(&**flag::STEP_SAGE_OF_SPIRIT))
        );
        assert_eq!(
            progress(&["carpenters-freed".into(), "open-forest".into()]).unwrap(),
            [(flow::SAGE_OF_SPIRIT.to_string(), "Step02".to_owned())]
        );
        assert!(progress(&["open-forest".into()]).unwrap().is_empty());
    }

    #[test]
    fn a_save_is_offered_only_presets_it_can_take() {
        let names: Vec<String> = for_saves().map(|p| p.name.into()).collect();
        assert!(!names.is_empty() && !names.iter().any(|n| n == "claim-check"));
        assert!(for_save(&names, &[]).is_ok());
    }

    #[test]
    fn a_save_refuses_presets_that_hand_over_items() {
        let err = for_save(&["open-forest".into(), "claim-check".into()], &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("claim-check") && err.contains(&**item::CLAIM_CHECK),
            "{err}"
        );
        assert_eq!(for_save(&["kakariko-gate".into()], &[]).unwrap().len(), 1);
        let err = for_save(&["carpenters-freed".into()], &[]).unwrap_err();
        assert!(err.to_string().contains("carpenters-freed"), "{err}");
        assert!(for_save(&["nope".into()], &[]).is_err());
    }

    #[test]
    fn the_zora_presets_copy_what_their_events_set() {
        needs_names!();
        let edits = |p: &str| expand(&[p.into()], &[]).unwrap();
        let on = |f: &Name| format!("{f}=true");
        assert_eq!(
            edits(KING_ZORA_MOVED),
            [
                on(flag::HAVE_PLAYED_EVENT_DM_ZORA_0000),
                on(flag::HAVE_PLAYED_EVENT_DM_ZORA_0010)
            ]
        );
        assert_eq!(
            progress(&[KING_ZORA_MOVED.into()]).unwrap(),
            [(flow::GET_STONE_OF_WATER.to_string(), "Step07".to_owned())]
        );
        assert!(edits(JABU_OPEN).contains(&on(flag::JAB_JAB_OPENED_MOUTH)));
        let door = fields(&[JABU_OPEN.into()]).unwrap();
        assert_eq!(door.len(), 2);
        assert!(
            door.iter()
                .all(|f| f.starts_with(&format!("{JABU_MOUTH}.")))
        );
        assert!(fields(&["open-waterfall".into()]).unwrap().is_empty());
        let place = |n: &str| PRESETS.iter().find(|p| p.name == n).unwrap().place;
        for name in [KING_ZORA_MOVED, JABU_OPEN, "open-waterfall"] {
            assert_eq!(place(name), place::OPEN_AREAS, "{name}");
        }
    }

    #[test]
    fn the_adult_deku_tree_gates_off_only_the_adult_forests_debris() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let fields = fields(&[ADULT_DEKU_TREE.into()]).unwrap();
        let forest = crate::checks::banc_files(&romfs).unwrap();
        let forest = forest.into_iter().filter(|f| f.contains("/KokiriForest_"));
        let (mut adult, mut child) = (Vec::new(), Vec::new());
        for (rel, doc) in crate::checks::load_bancs(&romfs, forest.collect()) {
            let changed = crate::checks::set_fields(&mut doc.unwrap().root, &fields).unwrap();
            match rel.contains("_Adult_") {
                true => adult.extend(changed),
                false => child.extend(changed),
            }
        }
        adult.sort();
        let mut debris = DEKU_TREE_DEBRIS.to_vec();
        debris.sort();
        assert_eq!(adult, debris);
        assert!(child.is_empty(), "{child:?}");
        assert!(
            PRESETS
                .iter()
                .any(|p| p.name == ADULT_DEKU_TREE && p.place == place::OPEN_AREAS)
        );
    }

    #[test]
    fn a_save_refuses_a_preset_that_edits_the_maps() {
        let err = for_save(&[JABU_OPEN.into()], &[]).unwrap_err().to_string();
        assert!(err.contains("edits the seed's maps"), "{err}");
        assert!(for_save(&["open-waterfall".into()], &[]).is_ok());
    }
}
