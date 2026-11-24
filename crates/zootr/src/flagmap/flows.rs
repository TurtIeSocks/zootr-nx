//! The flag map's edges from event flows: what each entry point sets and
//! asks, and which events it runs. Each event the game starts by name (a
//! row of its event table) is a node of its own: it runs its flow's entry
//! point and sets its played flag.

use std::collections::BTreeMap;

use ounce_formats::{EventKind, Flowchart, Params, Value};

use super::{Graph, Kind, Link, quests};
use crate::cutscenes::played_flag;
use crate::events::{FOLLOW, Flows};
use crate::gamedata::GameData;
use crate::known::{field, flow};
use crate::{flows, rsdb};

/// Actions that change the value they name. Every other action or query
/// naming GameData reads it.
pub(super) const WRITERS: [&str; 4] = ["Set", "AddOrSub", "Reset", "Toggle"];
/// The names actions share in front.
const PREFIXES: [&str; 3] = ["EventTrigger", "EventQuery", "Event"];

/// The arguments each entry point is called with: `(flow, entry)` to each
/// caller's params.
type Calls = BTreeMap<(String, String), Vec<Params>>;

pub(super) fn add(g: &mut Graph, flows: &Flows, gd: &GameData) {
    let setting = rsdb::path(rsdb::EVENTS);
    for (row, (flow, entry)) in flows.rows() {
        let ev = g.node(Kind::Event, row, || None);
        let played = g.state(gd, &played_flag(row));
        g.edge(ev, played, Link::Writes, "played".into(), &setting, None);
        let eps = flows.0.get(flow).into_iter().flat_map(|c| &c.entry_points);
        for ep in eps.filter(|e| entry.as_ref().is_none_or(|n| *n == e.name)) {
            let to = g.event(flow, &ep.name);
            g.edge(ev, to, Link::Runs, "plays".into(), &setting, None);
        }
    }
    let mut names: Vec<&String> = flows.0.keys().collect();
    names.sort();
    let calls = calls(flows);
    for name in names {
        let chart = &flows.0[name];
        for ep in &chart.entry_points {
            let ev = g.event(name, &ep.name);
            let args = calls.get(&(name.clone(), ep.name.clone()));
            // The events it runs itself: calls into other entry points are
            // `runs` edges of their own.
            for i in flows::runs_itself(chart, ep.main) {
                step(
                    g,
                    gd,
                    flows,
                    chart,
                    ev,
                    i,
                    args.map_or(&[][..], Vec::as_slice),
                );
            }
        }
    }
}

/// Every call's params, by the entry point it calls.
fn calls(flows: &Flows) -> Calls {
    let mut calls = Calls::new();
    for (name, chart) in &flows.0 {
        for e in &chart.events {
            if let EventKind::SubFlow {
                params,
                flowchart,
                entry_point,
                ..
            }
            | EventKind::SubFlowSwitch {
                params,
                flowchart,
                entry_point,
                ..
            } = &e.kind
            {
                let callee = if flowchart.is_empty() {
                    name
                } else {
                    flowchart
                };
                let key = (callee.clone(), entry_point.clone());
                calls.entry(key).or_default().push(params.clone());
            }
        }
    }
    calls
}

/// The edges event `i` of `chart` adds for entry point `ev`. `args` are
/// the params its callers pass, for names given as `$argument`.
fn step(
    g: &mut Graph,
    gd: &GameData,
    flows: &Flows,
    chart: &Flowchart,
    ev: usize,
    i: u16,
    args: &[Params],
) {
    let edge = |g: &mut Graph, from, to, link, label: String| {
        g.edge(from, to, link, label, &chart.name, Some(i))
    };
    match &chart.events[i as usize].kind {
        EventKind::Action {
            actor,
            action,
            params,
            ..
        } => {
            let action = &chart.actor_of(*actor).actions[*action as usize];
            let link = match WRITERS.iter().any(|w| action.contains(w)) {
                true => Link::Writes,
                false => Link::Reads,
            };
            for name in named(params, args) {
                let st = g.state(gd, &name);
                let (from, to) = if link == Link::Writes {
                    (ev, st)
                } else {
                    (st, ev)
                };
                edge(g, from, to, link, label(action, params));
            }
            quest(g, gd, ev, params, args, (action, false), &edge);
            for (follows, key) in FOLLOW {
                if action == *follows
                    && let Some(target) = flows::param(params, key)
                {
                    let targets: Vec<usize> = match flows.rows().contains_key(target) {
                        true => vec![g.node(Kind::Event, target, || None)],
                        false => (flows.resolve(target).into_iter())
                            .flat_map(|(f, e)| {
                                entries(flows, f, e).into_iter().map(move |n| (f, n))
                            })
                            .map(|(f, n)| g.event(f, &n))
                            .collect(),
                    };
                    for to in targets {
                        edge(g, ev, to, Link::Runs, short(action).to_owned());
                    }
                }
            }
        }
        EventKind::Switch {
            actor,
            query,
            params,
            ..
        } => {
            let query = &chart.actor_of(*actor).queries[*query as usize];
            let mut names = named(params, args);
            if query == flow::EVENT_QUERY_HAVE_PLAYED_EVENT
                && let Some(event) = flows::param(params, flow::EVENT_NAME)
            {
                names.push(played_flag(event));
            }
            for name in names {
                let st = g.state(gd, &name);
                edge(g, st, ev, Link::Reads, label(query, params));
            }
            quest(g, gd, ev, params, args, (query, true), &edge);
        }
        EventKind::SubFlow {
            flowchart,
            entry_point,
            ..
        }
        | EventKind::SubFlowSwitch {
            flowchart,
            entry_point,
            ..
        } => {
            let callee = if flowchart.is_empty() {
                &chart.name
            } else {
                flowchart
            };
            let to = g.event(callee, entry_point);
            edge(g, ev, to, Link::Runs, "calls".into());
        }
        _ => {}
    }
}

/// The entry points a resolved event runs: the one it names, or every one.
fn entries(flows: &Flows, flow: &str, entry: Option<&str>) -> Vec<String> {
    match entry {
        Some(e) => vec![e.to_owned()],
        None => (flows.0.get(flow).into_iter())
            .flat_map(|c| c.entry_points.iter().map(|e| e.name.clone()))
            .collect(),
    }
}

/// The GameData names `params` gives, a `$argument` looked up in each
/// caller's params. One nobody passes stays `$argument`.
fn named(params: &Params, args: &[Params]) -> Vec<String> {
    let mut out: Vec<String> = (params.iter())
        .filter(|(k, _)| k.starts_with(&**field::GAME_DATA_NAME))
        .flat_map(|(_, value)| strings(value, args))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A param's strings: its own, or each caller's for a `$argument`. One
/// nobody passes stays `$argument`.
fn strings(value: &Value, args: &[Params]) -> Vec<String> {
    match value {
        Value::Argument(a) => {
            let passed: Vec<_> = (args.iter())
                .filter_map(|p| flows::param(p, a))
                .map(str::to_owned)
                .collect();
            match passed.is_empty() {
                true => vec![format!("${a}")],
                false => passed,
            }
        }
        v => v.as_str().map(str::to_owned).into_iter().collect(),
    }
}

/// The step edges of an action or query `class` that names a quest: a
/// write when it moves the quest, a read when it's a query (`asks`). An
/// action that names one without moving it (the progress UI) is neither.
fn quest(
    g: &mut Graph,
    gd: &GameData,
    ev: usize,
    params: &Params,
    args: &[Params],
    (class, asks): (&str, bool),
    edge: &impl Fn(&mut Graph, usize, usize, Link, String),
) {
    if !asks && !quests::moves(class) {
        return;
    }
    let Some((_, named)) = params.iter().find(|(k, _)| k == field::CHALLENGE_NAME) else {
        return;
    };
    let step = (flows::param(params, field::STEP_NAME))
        .or_else(|| flows::param(params, field::CHALLENGE_STEP_NAME));
    let label = quests::label(short(class), step, &[]);
    for q in strings(named, args) {
        let st = g.state(gd, &quests::state(&q));
        match quests::moves(class) {
            true => edge(g, ev, st, Link::Writes, label.clone()),
            false => edge(g, st, ev, Link::Reads, label.clone()),
        }
    }
}

/// An action's short name, with the value it sets and the array index.
fn label(action: &str, params: &Params) -> String {
    let mut label = short(action).to_owned();
    if let Some((_, v)) = params.iter().find(|(k, _)| k == field::VALUE) {
        label.push_str(&format!(" = {}", flows::value(v)));
    }
    if let Some((_, Value::Int(i))) = params.iter().find(|(k, _)| k == field::INDEX)
        && let [i] = i[..]
        && i >= 0
    {
        label.push_str(&format!(" [{i}]"));
    }
    label
}

fn short(action: &str) -> &str {
    PREFIXES
        .iter()
        .find_map(|p| action.strip_prefix(p))
        .unwrap_or(action)
}

#[cfg(test)]
mod tests {
    use ounce_formats::Value;

    use super::super::Graph;
    use super::{named, played_flag};
    use crate::known::{field, flag, flow, map, scene};

    #[test]
    fn the_door_of_times_draw_reads_the_first_pulls_played_flag_and_the_scene_sets_it() {
        let Some(g) = Graph::dev() else {
            return;
        };
        let played = format!("state:{}", played_flag(scene::TIME_TEMPLE_0040));
        let played = g.find(&played).unwrap();
        let draw = format!(
            "event:{}/{}",
            &**flow::DOOR_OF_TIME,
            &**field::DOOR_OF_TIME_DRAW
        );
        assert!(g.readers(played).edges.iter().any(|l| l.to.id == draw));
        let pull = format!("event:{}", &**scene::TIME_TEMPLE_0040);
        assert!(g.writers(played).edges.iter().any(|l| l.from.id == pull));
        assert!(!g.trace(g.find(&draw).unwrap(), played).paths.is_empty());
    }

    #[test]
    fn opening_the_door_sets_its_flag_and_cites_the_event_that_does() {
        let Some(g) = Graph::dev() else {
            return;
        };
        let opened = g.find(flag::MARKET_CHILD_TIME_TEMPLE_DOOR_OPENED).unwrap();
        let open = format!(
            "event:{}/{}",
            &**flow::DOOR_OF_TIME,
            &**field::DOOR_OF_TIME_OPEN_DOOR
        );
        let writers = g.writers(opened);
        let line = writers.edges.iter().find(|l| l.from.id == open).unwrap();
        assert!(line.label.ends_with("= true"), "{}", line.label);
        assert_eq!(line.file, &**flow::DOOR_OF_TIME);
        let cited = g.source(line.id).unwrap();
        assert!(
            cited.contains(&**flag::MARKET_CHILD_TIME_TEMPLE_DOOR_OPENED),
            "{cited}"
        );
    }

    /// The escape moves the Door of Time quest to Step04, and the moat's
    /// graph waits for it: the gate that held the moat checks (#298).
    #[test]
    fn the_escape_moves_the_door_quest_and_the_moat_graph_waits_for_it() {
        let Some(g) = Graph::dev() else {
            return;
        };
        let step = format!("state:{}{}", crate::steps::SAVED, &**flow::DOOR_OF_TIME);
        let step = g.find(&step).unwrap();
        let escape = format!(
            "event:{}/{}",
            &**flow::DOOR_OF_TIME,
            &**field::DOOR_OF_TIME_ESCAPE
        );
        let writers = g.writers(step);
        let line = writers.edges.iter().find(|l| l.from.id == escape).unwrap();
        assert!(line.label.ends_with(" to Step04"), "{}", line.label);
        let moat = format!(
            "graph:{}.logic.module",
            &**map::HYRULE_FIELD_CHILD_PREFAB_32F7
        );
        let readers = g.readers(step);
        let line = readers.edges.iter().find(|l| l.to.id == moat).unwrap();
        assert!(
            line.label.ends_with("IsAfter|IsEqual Step04"),
            "{}",
            line.label
        );
        assert!(
            !readers.edges.iter().any(|l| l.label.contains("ProgressUI")),
            "showing the progress UI checks nothing"
        );
    }

    #[test]
    fn an_argument_takes_each_callers_name_and_stays_an_argument_when_none_passes_it() {
        if crate::game::no_names() {
            return;
        }
        let key = field::GAME_DATA_NAME.to_string();
        let params = vec![(key, Value::Argument("FakeArg".into()))];
        let caller = |name: &str| vec![("FakeArg".into(), Value::String(vec![name.into()]))];
        let callers = [caller("FakeDoor_Opened"), caller("FakeGate_Opened")];
        assert_eq!(
            named(&params, &callers),
            ["FakeDoor_Opened", "FakeGate_Opened"]
        );
        assert_eq!(named(&params, &[]), ["$FakeArg"]);
    }
}
