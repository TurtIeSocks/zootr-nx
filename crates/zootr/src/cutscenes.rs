//! Cutscene skips. A cutscene is a `Dm_*` event. One that only shows
//! something is skipped two ways: every flow event that starts it is pointed
//! past it, and its played flag is set from the start, so the map doesn't
//! start it either. See `docs/research/cutscenes.md`.

mod catalogue;

pub use catalogue::{Catalogue, Cutscene, Tier};

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use ounce_formats::{Byml, EventKind, Flowchart, Link, Romfs, Value, murmur3, relink};

use crate::events::{self, Flows};
use crate::flows::{self, Rewrites};
use crate::gamedata::GameData;
use crate::known::{Name, field, flag, flow, map, scene};
use crate::{arrivals, checks, cut, gifts};

/// Setting id, label, and the name prefixes after `Dm_`. `SY` (system
/// events) and `Title` (title screen demos) aren't areas.
// Text: the command line reads these before a dump is open.
pub const AREAS: [(&str, &str, &[&str]); 27] = [
    ("kokiri", "Kokiri Forest", &["Kokiri"]),
    ("deku-tree", "Inside the Deku Tree", &["Deku"]),
    ("lost-woods", "Lost Woods", &["LostWood", "SacredForest"]),
    ("hyrule-field", "Hyrule Field", &["HrlField"]),
    ("castle", "Hyrule Castle and Market", &["HrlCastle"]),
    ("lon-lon", "Lon Lon Ranch", &["LonLon"]),
    ("kakariko", "Kakariko Village", &["Kakariko"]),
    ("graveyard", "Graveyard", &["Graveyard"]),
    (
        "death-mountain",
        "Death Mountain",
        &["DeathMtTrail", "DeathMtCrater"],
    ),
    ("goron", "Goron City", &["Goron"]),
    ("dodongo", "Dodongo's Cavern", &["DodongoCave"]),
    (
        "zora",
        "Zora's River, Domain and Fountain",
        &["ZoraRiver", "Zora", "ZoraFountain"],
    ),
    ("jabu", "Jabu-Jabu's Belly", &["InsideJabu"]),
    ("lake", "Lake Hylia", &["LakeHrl"]),
    (
        "gerudo",
        "Gerudo Valley and Fortress",
        &["GerudoValley", "Gerudo"],
    ),
    ("colossus", "Desert Colossus", &["DesertColossus"]),
    ("time", "Temple of Time", &["TimeTemple"]),
    ("forest-temple", "Forest Temple", &["ForestTemple"]),
    ("fire-temple", "Fire Temple", &["FireTemple"]),
    ("water-temple", "Water Temple", &["WaterTemple"]),
    ("shadow-temple", "Shadow Temple", &["ShadowTemple"]),
    ("spirit-temple", "Spirit Temple", &["SpiritTemple"]),
    (
        "ganon",
        "Ganon's Castle",
        &["GanonCastle", "InsideGanonCastle", "FinalBattle"],
    ),
    ("songs", "Learning Songs", &["MelodyGet"]),
    ("fairies", "Great Fairies", &["FairyFountain"]),
    ("sages", "Sages", &["Awake", "SageRoom"]),
    ("ending", "Ending", &["Ending", "StaffCredit"]),
];

/// The opening movie and Navi waking Link, both timelines in `OpeningEvent`.
const INTRO: [&Name; 2] = [scene::KOKIRI_0000, scene::KOKIRI_0030];
/// Leaving Link's house the first time: the forest pan, which the door's
/// actor pack plays, Saria running up, and her talk at the foot of the
/// ladder. Their warps only pose Link for the shot.
const SARIA: [&Name; 3] = [scene::KOKIRI_0045, scene::KOKIRI_0050, scene::KOKIRI_0060];
/// Set with Saria's played flags: her "come down" balloon otherwise.
const SARIA_FLAGS: [&Name; 1] = [scene::KOKIRI_0050_AFTER_NEAR_END];
/// Saria's farewell on the bridge out of the forest: her wave, a timeline,
/// then her gift. Both start from `BRIDGE`.
const SARIA_WAVE: &Name = scene::KOKIRI_0150;
const SARIA_GIFT: &Name = scene::KOKIRI_0155;
const BRIDGE: &Name = flow::GOTO_HYRULE_CASTLE;
/// Every cutscene Kaepora Gaebora talks in, rides included. Songs he only
/// shows up for are left out.
const OWL: [&Name; 15] = [
    scene::DEATH_MT_TRAIL_0030,
    scene::DEATH_MT_TRAIL_0035,
    scene::HRL_CASTLE_0000,
    scene::HRL_FIELD_0010,
    scene::HRL_FIELD_0030,
    scene::HRL_FIELD_0040,
    scene::HRL_FIELD_0050,
    scene::HRL_FIELD_0080,
    scene::LAKE_HRL_0010,
    scene::LAKE_HRL_0011,
    scene::LOST_WOOD_0000,
    scene::LOST_WOOD_0010,
    scene::ZORA_RIVER_0000,
    scene::ZORA_RIVER_0005,
    scene::LAKE_HRL_0003,
];
/// Set with the owl's played flags: he waits for Link to talk otherwise.
const OWL_FLAGS: [&Name; 1] = [flag::NPC_LAKE_HRL_0010_JUST_BEFORE_TALK];
/// Meeting the Great Deku Tree: his talk, then his mouth opening, which
/// moves the forest quest on. An event spot in front of him starts both,
/// until its own played flag is set. These are what the meeting leaves.
const GREAT_DEKU: [&Name; 2] = [scene::KOKIRI_0070, scene::KOKIRI_0075];
fn great_deku_flags() -> [String; 6] {
    [
        played_set(field::KOKIRI_FOREST_MEET_DEKU_TREE_MEET_DEKU_TREE_01),
        played_set(scene::KOKIRI_0070),
        played_set(scene::KOKIRI_0075),
        format!(
            "{}=true",
            field::KOKIRI_FOREST_MEET_DEKU_TREE_OPEN_DEKU_TREE_MOUTH
        ),
        crate::steps::at(flow::MEET_DEKU_TREE, "Complete"),
        crate::steps::at(flow::GET_STONE_OF_FOREST, field::GO_INSIDE),
    ]
}
/// Area intros whose one change is a warp that walks Link in from the door,
/// where he stands anyway. Skipped with their area.
const POSED: [&Name; 1] = [scene::TIME_TEMPLE_0000];
/// Map actors whose read of a played flag is only the area intro's "once"
/// guard: the entrance that boots the intro on the way in, and the area name
/// banner, held back until the intro has played.
const INTRO_GUARDS: [&Name; 2] = [arrivals::ENTRANCE, map::LOCATION_AREA];

/// Skipped cutscenes whose played flag gates a map trigger that starts a flow
/// doing more than play them: `scene::HRL_FIELD_0010` is the end of the
/// field's flow that goes to the castle, which clears the field's first-visit
/// sky, and `scene::TIME_TEMPLE_0020` the end of the Door of Time's discovery
/// flow, which finds the Master Sword. A new game that sets their flag stops
/// the trigger, and that whole flow with it. So each transition to one sets
/// the flag instead, where the cutscene would have ended.
/// `scene::TIME_TEMPLE_0040`, the first pull, is the same in a flow: set at
/// the start, its flag sent every pull of a closed door's sword to the plain
/// swap, past Rauru and the sword's grant. `scene::TIME_TEMPLE_0010`, the
/// door opening, too: the song's flow ends before opening it once its flag
/// is set.
const FLAG_AT_END: [&Name; 4] = [
    scene::HRL_FIELD_0010,
    scene::TIME_TEMPLE_0010,
    scene::TIME_TEMPLE_0020,
    scene::TIME_TEMPLE_0040,
];
/// Meeting Zelda in the castle courtyard, which the castle's meet-Zelda flow
/// chains five scene changes for. Any one skipped, or the movie among them
/// left, froze the game on a white screen, and the third, `0050`, is where
/// her letter is granted. So crossing her trigger skips the whole chain with
/// the castle's skips, each transition setting its played flag where it was
/// (see `FLAG_AT_END`), and the letter is granted at `0050`'s flag, see
/// `gifts`. The rest of the flow stays: her warp, both quest steps, the fades.
pub(crate) const ZELDA: [&Name; 5] = [
    scene::HRL_CASTLE_0030,
    scene::HRL_CASTLE_0035,
    scene::HRL_CASTLE_0045,
    scene::HRL_CASTLE_0050,
    scene::HRL_CASTLE_0060,
];
/// The area setting that skips `ZELDA`.
const ZELDA_AREA: &str = "castle";
/// Impa's lullaby, which `DoorOfTime` starts when Link talks to her: the
/// scene, then her talk, then the banc change out to the field, whose boot
/// event plays her escort. Crossing her gives the check and the banc change
/// and nothing else. The scene's transition becomes its played flag, the
/// grant goes right after it, see `gifts`, then comes `IMPA_BANC`. The
/// escort's transition becomes its flag too, so the quest steps after it run.
pub(crate) const IMPA: [&Name; 2] = [scene::MELODY_GET_ZELDA, scene::HRL_FIELD_0020];
/// The area setting that skips `IMPA`: the one `scene::MELODY_GET_ZELDA` is in.
const IMPA_AREA: &str = "songs";
const IMPA_SCENE: &Name = IMPA[0];
/// The banc change her talk ends on, by the boot event it names.
const IMPA_BANC: (&Name, &Name) = (field::GAME_SYSTEM_ACTOR, flow::EVENT_RESERVE_BANC_CHANGE);
const IMPA_BOOT: &Name = field::HYRULE_FIELD_DOOR_OF_TIME_SEND_OFF;
/// The field's arrival, started by the flow that goes to the castle. It sets the
/// clock, which the clock's start needs, so skipping it sets the time it ends
/// on: the 7:30 it eases to, and the owl's cutscene sets again.
const FIELD_ARRIVAL: &Name = scene::HRL_FIELD_0000;
const FIELD_ARRIVAL_TIME: [i32; 2] = [7, 30];
const WORLD: (&Name, &Name) = (
    flow::EVENT_WORLD_MANAGER_CONTROLLER,
    flow::EVENT_TRIGGER_SET_TIME,
);

pub(crate) const TRANSITION: &Name = flow::EVENT_TRANSITION;
/// How Link's warps' action names start.
pub(crate) const PLAYER_WARP: &str = "EventPlayerWarp";
const SKIPPED: &Name = flow::EVENT_QUERY_IS_EVENT_SKIPED_MOST_RECENT;
pub(crate) const PLAYED: &Name = flag::HAVE_PLAYED_EVENT;

/// `name`'s played flag.
pub(crate) fn played_flag(name: &str) -> String {
    format!("{PLAYED}.{name}")
}

/// The name a played flag is for, the inverse of [`played_flag`].
pub(crate) fn played_name(flag: &str) -> Option<&str> {
    flag.strip_prefix(&**PLAYED)?.strip_prefix('.')
}

/// `name`'s played flag, set, as a new-game flag.
pub(crate) fn played_set(name: &str) -> String {
    format!("{}=true", played_flag(name))
}

/// Actors whose `EventTrigger*` and `EventGet*` actions change the save.
const STATE_ACTORS: [&Name; 3] = [
    field::GAME_DATA_ACTOR,
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_REQUEST_ACTOR,
];
/// Actions that move Link or change the world, by part of their name. NPCs,
/// Navi and the camera warp too, but only into place for the shot.
const MOVES: [&str; 5] = [
    PLAYER_WARP,
    "BancChange",
    "Horse",
    "EventGet",
    "TimeElapsed",
];
/// Actions an object's own game runs inside a scene, by part of their
/// name: the spinning pot spins, reads its face and drops its prize in its
/// roulette and drop scenes. Skipped, a bomb left the game waiting on it.
const GAME_ACTIONS: [&str; 1] = ["GoronLargePot"];

/// Whether one of the skips above handles `name` by itself, which the cut of
/// the other story scenes leaves alone.
pub(crate) fn owned(name: &str) -> bool {
    [
        &INTRO[..],
        &SARIA,
        &OWL,
        &POSED,
        &FLAG_AT_END,
        &ZELDA,
        &IMPA,
        &GREAT_DEKU,
        &[SARIA_WAVE, SARIA_GIFT, FIELD_ARRIVAL],
    ]
    .iter()
    .any(|names| names.iter().any(|n| *n == name))
}

/// Anything a skip would lose: a save change, or Link or the world moving.
pub(crate) fn changes(actor: &str, action: &str) -> bool {
    let trigger = action.starts_with("EventTrigger") || action.starts_with("EventGet");
    (STATE_ACTORS.iter().any(|a| *a == actor) && trigger)
        || MOVES.iter().any(|m| action.contains(m))
        || GAME_ACTIONS.iter().any(|g| action.contains(g))
        || events::saves(actor, action)
}

/// What `changes` counts, less Link's warps, which only pose him for a
/// shot when whatever runs next places him anyway.
pub(crate) fn lasting(actor: &str, action: &str) -> bool {
    changes(actor, action) && !action.starts_with(PLAYER_WARP)
}

/// The area setting id for a cutscene name.
pub fn area(name: &str) -> Option<&'static str> {
    let rest = name.strip_prefix("Dm_")?;
    AREAS
        .iter()
        .find(|(_, _, prefixes)| {
            prefixes
                .iter()
                .any(|p| rest.strip_prefix(p).is_some_and(|r| r.starts_with('_')))
        })
        .map(|a| a.0)
}

/// Which skips are on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skips {
    pub intro: bool,
    pub saria: bool,
    pub owl: bool,
    pub great_deku: bool,
    /// Every area's first-arrival intro, see `arrivals`.
    pub arrivals: bool,
    /// Area setting ids.
    pub areas: BTreeSet<&'static str>,
}

impl Skips {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Every string in a Banc actor, nested ones included.
fn strings<'a>(v: &'a Byml, out: &mut Vec<&'a str>) {
    match v {
        Byml::String(s) => out.push(s),
        Byml::Array(a) => a.iter().for_each(|v| strings(v, out)),
        Byml::Map(m) => m.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// A transition's literal target, if `e` is one.
pub(crate) fn transition<'a>(flow: &'a Flowchart, e: &'a EventKind) -> Option<&'a str> {
    let EventKind::Action {
        actor,
        action,
        params,
        ..
    } = e
    else {
        return None;
    };
    if flow.actor_of(*actor).actions[*action as usize] != TRANSITION {
        return None;
    }
    match params.iter().find(|(k, _)| k == flow::EVENT_NAME)?.1 {
        ounce_formats::Value::String(ref v) if v.len() == 1 => Some(&v[0]),
        _ => None,
    }
}

/// Every event a scene change boots into.
fn boots(all: &Flows) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for flow in all.charts.values() {
        for e in &flow.events {
            if let EventKind::Action { params, .. } = &e.kind
                && let Some((_, v)) = params.iter().find(|(k, _)| k == field::BOOT_EVENT_NAME)
                && let Some(s) = v.as_str()
                && s.starts_with("Dm_")
            {
                out.insert(s.to_owned());
            }
        }
    }
    out
}

/// `HavePlayedEvent.<name>` minus its prefix, for every string that is one.
fn played<'a>(strings: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    strings
        .into_iter()
        .filter_map(|s| played_name(s).map(String::from))
        .collect()
}

/// Skip what `skips` covers: the flows that start it, relinked, and the
/// new-game flags that stop the map starting it. `all` is every flow.
pub fn skip(romfs: &Romfs, all: &Flows, skips: &Skips) -> Result<(Rewrites, Vec<String>)> {
    let mut out = Rewrites::new();
    if skips.is_empty() {
        return Ok((out, vec![]));
    }
    let catalogue = Catalogue::load(romfs)?;
    let picked = catalogue.picked(skips);
    let cuts = catalogue.cut(skips, all);
    let at_end = |n: &str| {
        [&FLAG_AT_END[..], &ZELDA, &IMPA]
            .iter()
            .any(|g| g.iter().any(|x| *x == n))
    };
    let swapped = |n: &str| at_end(n) || n == FIELD_ARRIVAL;
    let names: BTreeSet<&str> = picked
        .iter()
        .map(|c| c.name.as_str())
        .filter(|n| !swapped(n))
        .collect();
    let callers: BTreeSet<&String> = picked.iter().flat_map(|c| &c.flows).collect();
    for flow in callers {
        let rel = flows::rel(flow);
        let data = relinked(&romfs.read(&rel)?, &names).with_context(|| format!("in {flow}"))?;
        out.insert(rel, data);
    }
    for c in picked.iter().filter(|c| swapped(&c.name)) {
        for flow in &c.flows {
            flows::edit(romfs, &mut out, flow, |f| {
                if c.name == FIELD_ARRIVAL {
                    let [hour, min] = FIELD_ARRIVAL_TIME;
                    let params = [
                        (field::TIME_HOUR.to_string(), Value::Int(vec![hour])),
                        (field::TIME_MIN.to_string(), Value::Int(vec![min])),
                    ];
                    arrivals::replace(f, &c.name, WORLD, &params)
                } else {
                    cut::swap(f, &c.name, &[])?;
                    if c.name == IMPA_SCENE {
                        banc_after(f)?;
                    }
                    Ok(())
                }
            })?;
        }
    }
    // A cut scene sets its flag where it played, if anything plays it.
    for (c, plan) in &cuts {
        for flow in &c.flows {
            flows::edit(romfs, &mut out, flow, |f| {
                cut::swap(f, &c.name, &plan.copies)
            })?;
        }
    }
    let mut flags: Vec<String> = picked
        .iter()
        .filter(|c| c.flag && !at_end(&c.name))
        .map(|c| played_set(&c.name))
        .collect();
    flags.extend(
        cuts.iter()
            .filter(|(c, _)| c.flows.is_empty())
            .map(|(c, _)| played_set(&c.name)),
    );
    if skips.owl {
        flags.extend(OWL_FLAGS.iter().map(|f| format!("{f}=true")));
    }
    if skips.saria {
        flags.extend(SARIA_FLAGS.iter().map(|f| format!("{f}=true")));
        bridge(romfs, &mut out)?;
    }
    if skips.great_deku {
        flags.extend(great_deku_flags());
    }
    let skipped = picked.iter().copied().chain(cuts.iter().map(|(c, _)| *c));
    flags.extend(arrivals::visits(all, skipped.map(|c| c.name.as_str())));
    if skips.arrivals {
        let cut_names: BTreeSet<&str> = cuts.iter().map(|(c, _)| c.name.as_str()).collect();
        arrivals::skip(romfs, all, &cut_names, &mut out, &mut flags)?;
    }
    flags.sort();
    flags.dedup();
    Ok((out, flags))
}

/// Where Impa's lullaby scene stood, her talk's banc change instead, after
/// the played flag it was swapped for. A copy, since the talk is skipped too.
fn banc_after(flow: &mut Flowchart) -> Result<()> {
    let boots = |i: &u16| {
        matches!(&flow.events[*i as usize].kind,
            EventKind::Action { params, .. } if flows::param(params, field::BOOT_EVENT_NAME) == Some(IMPA_BOOT))
    };
    let banc = flows::running(flow, IMPA_BANC).into_iter().find(boots);
    let banc = banc.with_context(|| format!("{} has no banc change to {IMPA_BOOT}", flow.name))?;
    let copy = flow.push(flow.events[banc as usize].kind.clone());
    let [at] = gifts::scene_marks(flow, IMPA_SCENE).1[..] else {
        bail!("{} doesn't set {IMPA_SCENE}'s flag once", flow.name);
    };
    flow.link(Link::Next(at), Some(copy))
}

/// Saria's farewell, shortened: her wave sets its played flag where it
/// played, and her gift opens straight onto the grant. Its played flag is
/// still left to the game, so the gift counts as found only on the bridge.
fn bridge(romfs: &Romfs, out: &mut Rewrites) -> Result<()> {
    let played = played_flag(SARIA_WAVE);
    flows::edit(romfs, out, BRIDGE, |f| {
        arrivals::swap(f, SARIA_WAVE, &played)
    })?;
    flows::edit(romfs, out, SARIA_GIFT, gifts::straight)
}

/// `data` with every transition to one of `names` stepped over.
fn relinked(data: &[u8], names: &BTreeSet<&str>) -> Result<Vec<u8>> {
    let flow = Flowchart::parse(data)?.context("a timeline starts nothing")?;
    let skipped = |i: u16| {
        transition(&flow, &flow.events[i as usize].kind).is_some_and(|t| names.contains(t))
    };
    let mut out = data.to_vec();
    let end = flows::ending(&flow);
    for at in (0u16..)
        .zip(&flow.events)
        .map(|(i, _)| i)
        .filter(|&i| skipped(i))
    {
        let to = past(&flow, at, &skipped)?;
        for link in flow.links_to(at) {
            match (link, to) {
                (Link::Next(i), _) if skipped(i) => {}
                (Link::Branch(..), None) => {
                    let end = end.context("a case runs it last, and nothing else ends quietly")?;
                    relink(&mut out, link, Some(end))?;
                }
                _ => relink(&mut out, link, to)?,
            }
        }
    }
    Ok(out)
}

/// Where to go instead of transition `at`: its next event, through the
/// not-skipped case of a "was it skipped" switch, past any further skipped
/// transitions. `None` is the end of the flow.
fn past(flow: &Flowchart, mut at: u16, skipped: &impl Fn(u16) -> bool) -> Result<Option<u16>> {
    for _ in 0..flow.events.len() {
        let EventKind::Action { next, .. } = &flow.events[at as usize].kind else {
            bail!("event {at} isn't a transition");
        };
        let Some(next) = *next else { return Ok(None) };
        let to = match &flow.events[next as usize].kind {
            EventKind::Switch {
                actor,
                query,
                cases,
                ..
            } if flow.actor_of(*actor).queries[*query as usize] == SKIPPED => cases
                .iter()
                .find(|c| c.0 == 0)
                .map(|c| c.1)
                .with_context(|| format!("no not-skipped case after event {at}"))?,
            _ => next,
        };
        if !skipped(to) {
            return Ok(Some(to));
        }
        at = to;
    }
    bail!("skipped transitions loop back on themselves")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, flow, scene};

    pub(super) fn every() -> Skips {
        Skips {
            intro: true,
            saria: true,
            owl: true,
            great_deku: true,
            arrivals: true,
            areas: AREAS.iter().map(|a| a.0).collect(),
        }
    }

    #[test]
    fn a_skipped_banner_still_marks_its_area_visited() {
        // The map and minimap show only visited areas (Mac, 2026-11-15).
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (_, flags) = skip(&romfs, &Flows::load(&romfs).unwrap(), &every()).unwrap();
        for area in [
            flag::KOKIRI_FOREST,
            flag::DEATH_MOUNTAIN_TRAIL,
            flag::KAKARIKO_VILLAGE,
        ] {
            let visit = format!("{}.{area}=true", field::IS_VISIT_LOCATION);
            assert!(flags.contains(&visit), "{visit}");
        }
        let (_, none) = skip(&romfs, &Flows::load(&romfs).unwrap(), &Skips::default()).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn areas_come_from_the_name() {
        needs_names!();
        assert_eq!(area(scene::KOKIRI_0060), Some("kokiri"));
        assert_eq!(area(scene::INSIDE_JABU_JABU_0010), Some("jabu"));
        assert_eq!(area(scene::ZORA_0000), Some("zora"));
        assert_eq!(area(scene::ZORA_FOUNTAIN_0010), Some("zora"));
        assert_eq!(area(scene::GERUDO_VALLEY_0000), Some("gerudo"));
        assert_eq!(area(scene::SY_OCARINA), None);
        assert_eq!(area("Dm_X_0000"), None);
        let mut ids: Vec<_> = AREAS.iter().map(|a| a.0).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), AREAS.len());
    }

    #[test]
    fn named_skips_set_their_flags() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let skips = Skips {
            owl: true,
            saria: true,
            great_deku: true,
            ..Default::default()
        };
        let (_, flags) = skip(&romfs, &Flows::load(&romfs).unwrap(), &skips).unwrap();
        for f in [
            &played_set(scene::LAKE_HRL_0010),
            &format!("{}=true", flag::NPC_LAKE_HRL_0010_JUST_BEFORE_TALK),
            &played_set(scene::KOKIRI_0045),
            &format!("{}=true", scene::KOKIRI_0050_AFTER_NEAR_END),
            &format!(
                "{}=true",
                field::KOKIRI_FOREST_MEET_DEKU_TREE_OPEN_DEKU_TREE_MOUTH
            ),
            &crate::steps::at(flow::GET_STONE_OF_FOREST, field::GO_INSIDE),
        ] {
            assert!(flags.contains(&f.to_owned()), "{f} in {flags:?}");
        }
    }

    #[test]
    fn saria_waves_off_screen_and_her_gift_still_counts_on_the_bridge() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let skips = Skips {
            saria: true,
            ..Default::default()
        };
        let (out, flags) = skip(&romfs, &Flows::load(&romfs).unwrap(), &skips).unwrap();
        let bridge = Flowchart::parse(&out[&flows::rel(BRIDGE)])
            .unwrap()
            .unwrap();
        let bridge = flows::render(&bridge);
        assert!(!bridge.contains(&format!(r#"{}="{SARIA_WAVE}""#, flow::EVENT_NAME)));
        assert!(bridge.contains(&format!(
            r#"{}="{PLAYED}.{SARIA_WAVE}" Index=-1 Value=true"#,
            field::GAME_DATA_NAME
        )));
        assert!(bridge.contains(&format!(r#"{}="{SARIA_GIFT}""#, flow::EVENT_NAME)));
        assert!(out.contains_key(&flows::rel(SARIA_GIFT)));
        assert!(!flags.iter().any(|f| f.contains(&**SARIA_GIFT)));
    }

    #[test]
    fn the_fields_arrival_still_sets_the_clock_and_the_owls_flag_comes_at_his_end() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let skips = every();
        let (out, flags) = skip(&romfs, &Flows::load(&romfs).unwrap(), &skips).unwrap();
        let rendered = |flow: &str| {
            let f = Flowchart::parse(&out[&flows::rel(flow)]).unwrap().unwrap();
            flows::render(&f)
        };
        // The new game leaves the triggers' flags to the flows.
        for n in FLAG_AT_END {
            assert!(!flags.iter().any(|f| f.contains(&**n)), "{n} in {flags:?}");
        }
        let hyrule = rendered(flow::GOTO_HYRULE_CASTLE);
        assert!(!hyrule.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::HRL_FIELD_0000
        )));
        assert!(!hyrule.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::HRL_FIELD_0010
        )));
        assert!(hyrule.contains(&format!(
            "{}({}=7 {}=30)",
            flow::EVENT_TRIGGER_SET_TIME,
            field::TIME_HOUR,
            field::TIME_MIN
        )));
        assert!(hyrule.contains(&format!(
            r#"{}="{PLAYED}.{}" Index=-1 Value=true"#,
            field::GAME_DATA_NAME,
            scene::HRL_FIELD_0010
        )));
        let door = rendered(flow::DOOR_OF_TIME);
        let opens = [
            scene::TIME_TEMPLE_0010,
            scene::TIME_TEMPLE_0020,
            scene::TIME_TEMPLE_0040,
        ];
        for n in opens {
            let plays = format!(r#"{}="{n}""#, flow::EVENT_NAME);
            let transition = flow::EVENT_TRANSITION.get();
            assert!(
                !door
                    .lines()
                    .any(|l| l.contains(transition) && l.contains(&plays))
            );
            assert!(door.contains(&format!(
                r#"{}="{PLAYED}.{n}" Index=-1 Value=true"#,
                field::GAME_DATA_NAME,
            )));
        }
        // Everything else skipped still sets its flag.
        assert!(flags.contains(&played_set(scene::HRL_FIELD_0000)));
    }

    /// What event `e` of `flow` runs when it is the copy `c`, and the event
    /// it goes on to.
    fn runs(flow: &Flowchart, e: u16, c: &cut::Copy) -> Option<Option<u16>> {
        match (&flow.events[e as usize].kind, c) {
            (
                EventKind::Action {
                    actor,
                    action,
                    params,
                    next,
                },
                cut::Copy::Action {
                    actor: a,
                    action: b,
                    params: p,
                },
            ) => {
                let actor = flow.actor_of(*actor);
                (actor.name == a.name && actor.actions[*action as usize] == *b && params == p)
                    .then_some(*next)
            }
            (
                EventKind::SubFlow {
                    flowchart,
                    entry_point,
                    params,
                    next,
                },
                cut::Copy::Call {
                    flowchart: f,
                    entry_point: g,
                    params: p,
                },
            ) => (flowchart == f && entry_point == g && params == p).then_some(*next),
            _ => None,
        }
    }

    #[test]
    fn each_caller_of_a_cut_scene_sets_its_flag_where_the_transition_was_and_runs_its_effects_once()
    {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = Catalogue::load(&romfs).unwrap();
        let all = Flows::load(&romfs).unwrap();
        // The arrival skip sets some of the same flags.
        let skips = Skips {
            arrivals: false,
            ..every()
        };
        let (out, flags) = skip(&romfs, &all, &skips).unwrap();
        let cuts = catalogue.cut(&skips, &all);
        assert!(cuts.len() > 60, "{}", cuts.len());
        let mut copied = 0;
        for (c, plan) in &cuts {
            let flag = played_flag(&c.name);
            // A scene nothing plays has its flag from the new game.
            assert_eq!(
                flags.contains(&format!("{flag}=true")),
                c.flows.is_empty(),
                "{}",
                c.name
            );
            for flow in &c.flows {
                let vanilla = &all.charts[flow];
                let after = Flowchart::parse(&out[&flows::rel(flow)]).unwrap().unwrap();
                let was = gifts::scene_marks(vanilla, &c.name).0.len();
                let (plays, sets) = gifts::scene_marks(&after, &c.name);
                assert!(plays.is_empty(), "{flow} still plays {}", c.name);
                assert_eq!(sets.len(), was, "{flow} sets {flag} once per transition");
                for at in sets {
                    let EventKind::Action { next, .. } = after.events[at as usize].kind else {
                        panic!("{flag} isn't set by an action");
                    };
                    let mut at = next;
                    for copy in &plan.copies {
                        let e = at.unwrap_or_else(|| panic!("{flow} ends before {}", c.name));
                        at = runs(&after, e, copy).unwrap_or_else(|| {
                            panic!("{flow} runs something else than {copy:?} after {flag}")
                        });
                        copied += 1;
                    }
                }
            }
        }
        assert!(copied > 20, "{copied}");
    }

    #[test]
    fn a_scene_outside_its_areas_setting_is_untouched() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = Catalogue::load(&romfs).unwrap();
        let all = Flows::load(&romfs).unwrap();
        let skips = Skips {
            areas: BTreeSet::from(["goron"]),
            ..Default::default()
        };
        let (out, flags) = skip(&romfs, &all, &skips).unwrap();
        let cuts = catalogue.cut(&skips, &all);
        let names: Vec<&str> = cuts.iter().map(|(c, _)| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                scene::GORON_0000,
                scene::GORON_0020,
                scene::GORON_0025,
                scene::GORON_0030
            ]
        );
        // Only the callers of Goron City's scenes are written, not the song
        // scenes' or the dungeon exits'.
        let theirs: BTreeSet<String> = catalogue
            .picked(&skips)
            .iter()
            .flat_map(|c| &c.flows)
            .chain(cuts.iter().flat_map(|(c, _)| &c.flows))
            .map(|f| flows::rel(f))
            .collect();
        assert!(out.keys().all(|w| theirs.contains(w)), "{:?}", out.keys());
        assert!(out.contains_key(&flows::rel(flow::GET_STONE_OF_FIRE)));
        assert!(!out.contains_key(&flows::rel(flow::SAGE_OF_FIRE)));
        let goron = |f: &String| f.contains("Dm_Goron_") || f == "IsVisitLocation.GoronCity=true";
        assert!(flags.iter().all(goron), "{flags:?}");
    }

    /// The pot's roulette spins it, reads its face and drops its prize. Cut,
    /// a bomb in the pot left the game waiting.
    #[test]
    fn the_spinning_pots_scenes_play() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = Catalogue::load(&romfs).unwrap();
        for n in [scene::GORON_0040, scene::GORON_0045] {
            let c = catalogue.get(n).unwrap();
            assert!(!c.tier.skippable(), "{n}: {:?}", c.tier);
        }
    }
}
