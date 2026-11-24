//! Area intros: the camera pan the first time Link reaches an area. The
//! entrance he arrives through boots `InitEventName` while `InitEventFlag` is
//! unset, and its plain arrival once it's set. See
//! `docs/research/cutscenes.md`.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Flowchart, Romfs, Value};

use crate::checks;
use crate::cutscenes::{PLAYED, lasting, transition};
use crate::events::Flows;
use crate::flows::{self, Rewrites, SET_BOOL};
use crate::gamedata::GameData;
use crate::known::{Name, field, flag, flow, scene};
use crate::per_dump::PerDump;

pub(crate) const ENTRANCE: &Name = field::FIELD_AND_DUNGEON_ENTRANCE_EVENT_REQUEST_ACTOR;
/// Intros a map's `EventRequestActor` starts instead of the entrance: the
/// flow, its cutscene, and the visit flag the cutscene sets. The cutscene's
/// played flag also clears air walls around the area.
const REQUESTED: [(&Name, &Name, &Name); 1] = [(
    flow::ENTRY_LON_LON_RANCH,
    scene::LON_LON_0010,
    flag::LON_LON_RANCH_IS_VISITED_CHILD,
)];

/// The action that shows an area's name banner. Opening it marks the area
/// visited, and the map and minimap show only visited areas.
const BANNER: &Name = flow::EVENT_TRIGGER_OPEN_LOCATION_UI;

/// `IsVisitLocation.<area>=true` for each area whose banner one of `scenes`
/// opens, so a skipped scene still puts its area on the map.
pub(crate) fn visits<'a>(flows: &Flows, scenes: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    scenes
        .into_iter()
        .filter_map(|s| flows.0.get(s))
        .flat_map(|f| f.events.iter().filter_map(|e| banner(f, &e.kind)))
        .map(|area| format!("{}.{area}=true", field::IS_VISIT_LOCATION))
        .collect()
}

/// The area whose banner `e` opens, named as its location file is.
fn banner<'a>(flow: &'a Flowchart, e: &'a EventKind) -> Option<&'a str> {
    let EventKind::Action {
        actor,
        action,
        params,
        ..
    } = e
    else {
        return None;
    };
    if flow.actor_of(*actor).actions[*action as usize] != BANNER {
        return None;
    }
    flows::param(params, field::LOCATION_NAME)?
        .strip_prefix("Work/Location/")?
        .strip_suffix(".game__location__Location.gyml")
}

/// Whether `cutscene` is an intro a map's `EventRequestActor` starts, which
/// the arrival skip sets the visit flag for.
pub(crate) fn requested(cutscene: &str) -> bool {
    REQUESTED.iter().any(|r| r.1 == cutscene)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Skip {
    /// Nothing in it lasts, so the new game sets this flag: the entrance's,
    /// or for a BoolExp, the played flag it reads.
    Flag(String),
    /// The event also moves a quest along, so it still runs once, with each
    /// transition to `cutscene` in `flow` setting the flag instead.
    Swap { flow: String, cutscene: String },
    /// Something in it lasts that a swap can't keep.
    Plays(String),
}

#[derive(Clone)]
pub struct Arrival {
    pub event: String,
    pub flag: String,
    pub skip: Skip,
}

/// Every entrance's intro, once per event and flag, kept per dump.
/// `flows` is every flow.
pub fn all(romfs: &Romfs, flows: &Flows) -> Result<Vec<Arrival>> {
    static KEPT: PerDump<Vec<Arrival>> = PerDump::new();
    KEPT.get(romfs, || find(romfs, flows))
}

fn find(romfs: &Romfs, flows: &Flows) -> Result<Vec<Arrival>> {
    let gd = GameData::load(romfs)?;
    let mut found = BTreeSet::new();
    for (_, doc) in checks::bancs(romfs)? {
        let doc = doc?;
        for a in checks::actors(&doc.root)? {
            if checks::str_field(a, checks::GYAML) != Some(ENTRANCE) {
                continue;
            }
            let Some(d) = a.as_map()?.get(checks::DYNAMIC) else {
                continue;
            };
            let text = |k| checks::str_field(d, k).map(String::from);
            if let (Some(event), Some(flag)) =
                (text(field::INIT_EVENT_NAME), text(field::INIT_EVENT_FLAG))
            {
                found.insert((event, flag));
            }
        }
    }
    found
        .into_iter()
        .map(|(event, flag)| {
            let skip = skip_for(flows, &gd, &event, &flag)?;
            Ok(Arrival { event, flag, skip })
        })
        .collect()
}

fn skip_for(flows: &Flows, gd: &GameData, event: &str, flag: &str) -> Result<Skip> {
    let first = |e: &str| {
        flows
            .effects_where(e, lasting)
            .and_then(|e| e.into_iter().next())
            .map(|e| e.to_string())
    };
    let cutscene = played_cutscene(flag);
    if let Some(why) = cutscene.and_then(first) {
        return Ok(Skip::Plays(why));
    }
    let Some(why) = first(event) else {
        return Ok(match settable(gd, event, flag)? {
            Some(flag) => Skip::Flag(flag),
            None => Skip::Plays(format!("{flag} can't be set")),
        });
    };
    if let Some(cutscene) = cutscene {
        for (flow, entry) in flows.resolve(event) {
            let chart = &flows.0[flow];
            if flow == cutscene {
                continue;
            }
            let Some(entry) = entry else {
                continue;
            };
            let runs = flows::reachable(chart, entry)?
                .into_iter()
                .any(|e| transition(chart, &chart.events[e as usize].kind) == Some(cutscene));
            if runs {
                let (flow, cutscene) = (flow.to_owned(), cutscene.to_owned());
                return Ok(Skip::Swap { flow, cutscene });
            }
        }
    }
    Ok(Skip::Plays(why))
}

/// The cutscene a `PLAYED.<cutscene>` flag names.
fn played_cutscene(flag: &str) -> Option<&str> {
    flag.strip_prefix(&**PLAYED)?.strip_prefix('.')
}

/// The flag that stops the entrance booting `event`: `flag` itself, or the
/// played flag a BoolExp `flag` reads.
fn settable(gd: &GameData, event: &str, flag: &str) -> Result<Option<String>> {
    let played = format!("{PLAYED}.{event}");
    Ok(match gd.table(flag) {
        Some(t) if t == flag::BOOL_EXP && gd.implied_by(flag, &played)? => Some(played),
        Some("Bool") => Some(flag.to_owned()),
        _ => None,
    })
}

/// Swap each transition to `cutscene` in `flow` for setting `flag`.
pub(crate) fn swap(flow: &mut Flowchart, cutscene: &str, flag: &str) -> Result<()> {
    let params = [
        (
            field::GAME_DATA_NAME.to_string(),
            Value::String(vec![flag.into()]),
        ),
        ("Index".into(), Value::Int(vec![-1])),
        (field::VALUE.get().into(), Value::Bool(vec![true])),
    ];
    replace(flow, cutscene, SET_BOOL, &params)
}

/// Swap each transition to `cutscene` in `flow` for `with`, an `(actor,
/// action)` the flow already has an actor of, and its `params`.
pub(crate) fn replace(
    flow: &mut Flowchart,
    cutscene: &str,
    with: (impl AsRef<str>, impl AsRef<str>),
    params: &[(String, Value)],
) -> Result<()> {
    let template = flow
        .actors
        .iter()
        .find(|a| a.name == with.0.as_ref())
        .cloned()
        .with_context(|| format!("{} has no {}", flow.name, with.0.as_ref()))?;
    let (actor, action) = flow.action(&template, with.1.as_ref());
    let at: Vec<usize> = (0..flow.events.len())
        .filter(|&i| transition(flow, &flow.events[i].kind) == Some(cutscene))
        .collect();
    ensure!(!at.is_empty(), "{} never plays {cutscene}", flow.name);
    for i in at {
        let EventKind::Action { next, .. } = flow.events[i].kind else {
            unreachable!("a transition is an action");
        };
        flow.events[i].kind = EventKind::Action {
            actor,
            action,
            params: params.to_vec(),
            next,
        };
    }
    Ok(())
}

/// Skip every intro that can be: swapped flows into `rewrites`, and the
/// new-game flags of the rest into `flags`. `cut` are scenes an area skip
/// already swapped.
pub fn skip(
    romfs: &Romfs,
    flows: &Flows,
    cut: &BTreeSet<&str>,
    rewrites: &mut Rewrites,
    flags: &mut Vec<String>,
) -> Result<()> {
    for a in all(romfs, flows)? {
        if !matches!(a.skip, Skip::Plays(_)) {
            let cutscene = played_cutscene(&a.flag);
            flags.extend(visits(
                flows,
                [a.event.as_str()].into_iter().chain(cutscene),
            ));
        }
        match a.skip {
            Skip::Flag(flag) => flags.push(format!("{flag}=true")),
            Skip::Swap { cutscene, .. } if cut.contains(cutscene.as_str()) => {}
            Skip::Swap { flow, cutscene } => {
                flows::edit(romfs, rewrites, &flow, |f| swap(f, &cutscene, &a.flag))?;
            }
            Skip::Plays(_) => {}
        }
    }
    for (flow, cutscene, visit) in REQUESTED {
        flags.extend(visits(flows, [&**cutscene]));
        flows::edit(romfs, rewrites, flow, |f| swap(f, cutscene, visit))?;
        flags.push(format!("{PLAYED}.{cutscene}=true"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, flow, scene};

    const MARKET: &Name = flag::HAVE_PLAYED_EVENT_DM_HRL_CASTLE_0005;
    const GANON: &Name = flag::HAVE_PLAYED_EVENT_DM_GANON_CASTLE_0000;

    #[test]
    fn the_market_swaps_and_ganons_castle_sets_its_flag() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let flows = Flows::load(&romfs).unwrap();
        let mut rewrites = Rewrites::new();
        let mut flags = Vec::new();
        skip(&romfs, &flows, &BTreeSet::new(), &mut rewrites, &mut flags).unwrap();
        assert!(flags.contains(&format!("{GANON}=true")));
        assert!(!flags.contains(&format!("{MARKET}=true")));
        let rel = flows::rel(scene::SY_BANC_CHANGE_ARRIVE);
        let flow = Flowchart::parse(&rewrites[&rel]).unwrap().unwrap();
        let rendered = flows::render(&flow);
        assert!(!rendered.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::HRL_CASTLE_0005
        )));
        assert!(rendered.contains(&format!(
            r#"{}.{}({}="{MARKET}" Index=-1 Value=true)"#,
            field::GAME_DATA_ACTOR,
            flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
            field::GAME_DATA_NAME
        )));
        assert!(rendered.contains(&format!(
            "call {}::{}()",
            flow::GOTO_HYRULE_CASTLE,
            field::GOTO_HYRULE_CASTLE_ENTRY_MARKET
        )));
    }

    #[test]
    fn lon_lon_ranch_sets_its_visit_instead_of_the_intro() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let flows = Flows::load(&romfs).unwrap();
        let mut rewrites = Rewrites::new();
        let mut flags = Vec::new();
        skip(&romfs, &flows, &BTreeSet::new(), &mut rewrites, &mut flags).unwrap();
        assert!(flags.contains(&format!("{PLAYED}.{}=true", scene::LON_LON_0010)));
        let rel = flows::rel(flow::ENTRY_LON_LON_RANCH);
        let rendered = flows::render(&Flowchart::parse(&rewrites[&rel]).unwrap().unwrap());
        assert!(!rendered.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::LON_LON_0010
        )));
        assert!(rendered.contains(&format!(
            r#"{}="{}" Index=-1 Value=true"#,
            field::GAME_DATA_NAME,
            flag::LON_LON_RANCH_IS_VISITED_CHILD
        )));
    }
}
