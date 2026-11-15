//! New-game presets: GameData defaults a fresh save starts from, or edits
//! applied to an existing save.

use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use clap::builder::{PossibleValue, PossibleValuesParser};
use ounce_formats::{Byml, murmur3};

use crate::gamedata::GameData;
use crate::known::{Name, actor, field, flag, flow, item};
use crate::logic::WARP_SONGS;
use crate::savefile;
use crate::settings::group;
use crate::starting;

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
    /// The settings panel it sits on.
    pub group: &'static str,
    /// The checklist it sits in on that panel, if any.
    pub subgroup: Option<&'static str>,
    /// Its `NAME=VALUE` edits.
    edits: fn() -> Vec<String>,
    /// Pouch items a new game is handed, by actor. A save can't take these.
    pub give: &'static [&'static Name],
    /// Quest steps a new game's opening moves on, as `(challenge, step)`.
    /// A save edit can't: the challenge system keeps its own state.
    pub progress: &'static [(&'static Name, &'static str)],
}

/// The preset whose four carpenters are already freed.
pub const CARPENTERS_FREED: &str = "carpenters-freed";

/// Presets a new game takes unless turned off.
pub const DEFAULT_ON: [&str; 2] = ["menus", "clock"];

const QUEST: Option<&str> = Some("Quest progress");
const STONES: [&str; 3] = ["Forest", "Fire", "Water"];
const TRIALS: [&str; 6] = ["Light", "Forest", "Water", "Shadow", "Fire", "Spirit"];

/// Zelda met and her lullaby learned, with the Door of Time quest at `door`.
fn zelda(door: &str) -> Vec<String> {
    let mut edits = lit(&[
        "Step_Goto_HyruleCastle=Complete",
        &format!("IsGet.{}=true", item::MELODY_ZELDA_LULLABY),
    ]);
    edits.push(format!("Step_DoorOfTime={door}"));
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
        (1..=4).map(|n| format!("GerudoFortress_WarpPoint0{n}_ClearEvent")),
        "true",
    ));
    edits
}

/// Every sage awake and the rainbow bridge up, with Ganon's quest at `step`.
fn ganon(step: &str) -> Vec<String> {
    let mut edits = lit(&[
        "Step_AllSage=Complete",
        &format!(
            "{}.{}=true",
            flag::HAVE_PLAYED_EVENT,
            field::GANON_CASTLE_ADULT_FIELD_RAINBOW
        ),
    ]);
    edits.push(format!("Step_Defeat_Ganon={step}"));
    edits
}

/// A preset that hands over a bag, as the first nut or stick Link picks up
/// does: its `SmallBagActor` is what sets the capacity.
const fn bag(
    name: &'static str,
    label: &'static str,
    what: &'static str,
    give: &'static [&'static Name],
) -> Preset {
    Preset {
        name,
        label,
        what,
        group: group::INVENTORY,
        subgroup: Some(starting::SUBGROUP),
        edits: Vec::new,
        give,
        progress: &[],
    }
}

/// The guide pop-ups a menu page opens the first time it fills, all off.
fn page_guides() -> Vec<String> {
    let pages = ["Equip", "Head", "Boot", "Item"]
        .map(|p| format!("UI_IsNeedOpenPlusMenu{p}{}", flow::ACTION_GUIDE));
    all(
        pages
            .into_iter()
            .chain([flag::UI_IS_NEED_OPEN_SHORT_CUT_ACTION_GUIDE.to_string()]),
        "false",
    )
}

/// Every `ActionGuide_IsEnd` member: one bool per guide, which the game
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
/// Gem guide has been, which skips the camera move that leads into it.
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
    edits
}

const LIGHT_ARROWS: &[&Name] = &[item::ARROW_LIGHT];

pub const PRESETS: [Preset; 15] = [
    Preset {
        name: "open-forest",
        label: "Open forest",
        what: "Mido and the forest exit stop blocking Link, as child or adult",
        group: group::LOGIC,
        subgroup: None,
        give: &[],
        progress: &[],
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
        label: "Open Kakariko gate",
        what: "The Kakariko gate guard has already seen Zelda's letter, so the Death Mountain trail is open from the start",
        group: group::LOGIC,
        subgroup: None,
        edits: || all([flag::NPC_HYLIA_SOLDIER_003_GATE_OPEN], "true"),
        give: &[],
        progress: &[],
    },
    Preset {
        name: "menus",
        label: "Menus open",
        what: "Menu pages skip their guide pop-up the first time they fill, and the challenge tapestry is open from the start",
        group: group::GENERAL,
        subgroup: None,
        edits: || {
            let mut edits = page_guides();
            edits.push(format!(
                "{}=true",
                field::UI_IS_FINISH_CHALLENGE_MENU_OPEN_EVENT
            ));
            edits
        },
        give: &[],
        progress: &[],
    },
    Preset {
        name: "clock",
        label: "Clock runs",
        what: "Time passes from the start, as it does once Link first walks into Hyrule Field. Off, the clock waits for the field, even in a seed that starts past it",
        group: group::GENERAL,
        subgroup: None,
        // The field's first arrival sets it. The game's clock reads it by
        // hash, so nothing in the data names it as the gate.
        edits: || all([flag::HYRULE_FIELD_ARRIVE_ENTRANCE], "true"),
        give: &[],
        progress: &[],
    },
    Preset {
        name: "warp-kit",
        label: "Warp kit",
        what: "A new game starts with the Fairy Ocarina and the six warp songs",
        group: group::INVENTORY,
        subgroup: None,
        edits: || {
            let songs = WARP_SONGS.iter().map(|s| format!("Item_Melody_{s}"));
            let items = songs.chain([item::OCARINA_FAIRY.to_string()]);
            all(items.map(|i| format!("IsGet.{i}")), "true")
        },
        give: &[],
        progress: &[],
    },
    Preset {
        name: "zelda-done",
        label: "Met Zelda",
        what: "Child Zelda met and her lullaby learned, with her letter in hand",
        group: group::INVENTORY,
        subgroup: QUEST,
        edits: || zelda("Step02"),
        give: &[item::ZELDA_LETTER],
        progress: &[],
    },
    Preset {
        name: "door-of-time",
        label: "Door of Time open",
        what: "The three stones, the Ocarina of Time and its song, and the Door of Time open",
        group: group::INVENTORY,
        subgroup: QUEST,
        edits: || {
            let mut edits = zelda("Step06");
            edits.extend(all(
                STONES.map(|s| format!("Step_Get_StoneOf{s}")),
                "Complete",
            ));
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
    },
    Preset {
        name: CARPENTERS_FREED,
        label: "Carpenters freed",
        what: "All four carpenters freed, the membership card in hand, the guards standing down and the valley bridge up",
        group: group::INVENTORY,
        subgroup: QUEST,
        edits: carpenters,
        give: &[item::TOKEN_GERUDO],
        progress: &[(flow::SAGE_OF_SPIRIT, "Step02")],
    },
    Preset {
        name: "rainbow-bridge",
        label: "Rainbow bridge up",
        what: "Every sage awake, the Light Arrows in hand and the bridge to Ganon's Castle up",
        group: group::INVENTORY,
        subgroup: QUEST,
        edits: || ganon("Step02"),
        give: LIGHT_ARROWS,
        progress: &[],
    },
    Preset {
        name: "trials-done",
        label: "Trials done",
        what: "The rainbow bridge, and all six barriers in Ganon's Castle down",
        group: group::INVENTORY,
        subgroup: QUEST,
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
    },
    Preset {
        name: "claim-check",
        label: "Claim Check",
        what: "The trade quest's last step, with the Claim Check in hand",
        group: group::INVENTORY,
        subgroup: QUEST,
        edits: || lit(&["Step_SubEvent_GetSwordBigGoron=Step11"]),
        give: &[item::CLAIM_CHECK],
        progress: &[],
    },
    Preset {
        name: "magic",
        label: "Magic meter",
        what: "The Great Fairy's magic meter, handed over the way her fountain does",
        group: group::INVENTORY,
        subgroup: QUEST,
        edits: Vec::new,
        give: &[item::FAIRY_BLESS_MP],
        progress: &[],
    },
    Preset {
        name: "no-tutorials",
        label: "No tutorials",
        what: "Every guide screen counts as already shown, so none pops up the first time, such as the Echo Gem's in the House of Skulltula. The Great Fairy's guide still shows",
        group: group::GENERAL,
        subgroup: None,
        edits: no_tutorials,
        give: &[],
        progress: &[],
    },
    bag(
        "nut-bag",
        "Deku Nut bag",
        "A new game starts with the 20-nut bag, which otherwise comes with the first nut picked up",
        &[item::NUT_BAG_SMALL],
    ),
    bag(
        "stick-bag",
        "Deku Stick bag",
        "A new game starts with the 10-stick bag, which otherwise comes with the first stick picked up",
        &[item::STICK_BAG_SMALL],
    ),
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
    PRESETS
        .iter()
        .filter(|p| p.give.is_empty() && p.progress.is_empty())
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

/// [`expand`] for an existing save, which can't be handed items or have its
/// quests moved.
pub fn for_save(presets: &[String], edits: &[String]) -> Result<Vec<String>> {
    let new_only = |p: &&&Preset| !p.give.is_empty() || !p.progress.is_empty();
    if let Some(p) = find(presets)?.iter().find(new_only) {
        let what = match p.give.is_empty() {
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
        let table = gd
            .table(name)
            .with_context(|| format!("{name} isn't in {}", flag::GAME_DATA))?;
        let raw = savefile::parse(gd, table, text).context(name.to_owned())?;
        let hash = i64::from(murmur3(name.as_bytes()));
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
        let was = savefile::raw(old).map_or("?".into(), |r| savefile::render(gd, table, r));
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
    fn the_clock_preset_marks_the_field_reached() {
        let edits = expand(&["clock".into()], &[]).unwrap();
        assert_eq!(
            edits,
            [format!("{}=true", flag::HYRULE_FIELD_ARRIVE_ENTRANCE)]
        );
        assert!(DEFAULT_ON.contains(&"clock"));
    }

    #[test]
    fn the_nut_and_stick_presets_hand_over_the_small_bags_and_sit_with_the_starting_items() {
        let both = ["nut-bag".to_owned(), "stick-bag".to_owned()];
        assert_eq!(
            gives(&both).unwrap(),
            [item::NUT_BAG_SMALL, item::STICK_BAG_SMALL]
        );
        assert!(expand(&both, &[]).unwrap().is_empty());
        for name in &both {
            let preset = PRESETS.iter().find(|p| p.name == name).unwrap();
            assert_eq!(preset.subgroup, Some(starting::SUBGROUP));
            assert!(!DEFAULT_ON.contains(&preset.name));
        }
    }

    #[test]
    fn the_no_tutorials_preset_marks_every_guide_shown_and_closes_the_page_guides() {
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
        assert!(!DEFAULT_ON.contains(&"no-tutorials"));
    }

    #[test]
    fn the_guide_list_is_every_member_of_the_shown_struct() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let members = gd.members(murmur3(b"ActionGuide_IsEnd")).unwrap();
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
    fn warp_kit_grants_ocarina_and_songs() {
        let edits = expand(&["warp-kit".into()], &[]).unwrap();
        assert_eq!(edits.len(), WARP_SONGS.len() + 1);
        assert!(edits.contains(&format!("IsGet.{}=true", item::OCARINA_FAIRY)));
        assert!(edits.contains(&format!("IsGet.{}=true", item::MELODY_LIGHT_PRELUDE)));
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
        let edits = |p: &str| expand(&[p.into()], &[]).unwrap();
        assert!(edits("door-of-time").contains(&"Step_DoorOfTime=Step06".into()));
        assert!(edits("trials-done").contains(&"Step_Defeat_Ganon=Step04".into()));
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
        assert!(edits("magic").is_empty());
        assert_eq!(gives(&["magic".into()]).unwrap(), [item::FAIRY_BLESS_MP]);
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
        assert!(!names.is_empty() && !names.iter().any(|n| n == "magic"));
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
}
