//! Event flows: loading them and printing them in a greppable form.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use anyhow::{Context, Result, bail};
use ounce_formats::{
    Actor, EntryPoint, EventKind, Flowchart, Link, Params, Romfs, Value, files_with_suffix,
};

use crate::known::{Name, field, flow};

const DIR: &str = "Event/EventFlow";
const SUFFIX: &str = ".bfevfl.zs";

/// Every flowchart name, as the game spells it.
pub fn names(romfs: &Romfs) -> Result<Vec<String>> {
    files_with_suffix(&romfs.root().join(DIR), SUFFIX)?
        .iter()
        .map(|p| {
            Ok(romfs
                .rel(p)?
                .trim_start_matches(DIR)
                .trim_start_matches('/')
                .trim_end_matches(SUFFIX)
                .to_owned())
        })
        .collect()
}

/// Rewritten flowcharts by romfs path, so edits from different settings land
/// in one file.
pub type Rewrites = std::collections::BTreeMap<String, Vec<u8>>;

/// A flowchart's romfs path.
pub fn rel(name: &str) -> String {
    format!("{DIR}/{name}{SUFFIX}")
}

/// Where a rewrite of `rel` goes: `rel` itself, plus the `.110` copy the
/// update ships beside 13 flows, like a cutscene's `NAME.110`. Which of
/// the two the game loads isn't known, so both get the rewrite.
pub fn copies(romfs: &Romfs, rel: &str) -> Vec<String> {
    let twin = rel
        .strip_suffix(SUFFIX)
        .map(|stem| format!("{stem}.110{SUFFIX}"))
        .filter(|twin| romfs.root().join(twin).is_file());
    std::iter::once(rel.to_owned()).chain(twin).collect()
}

/// `None` when the file only holds a cutscene timeline.
pub fn load(romfs: &Romfs, name: &str) -> Result<Option<Flowchart>> {
    let rel = rel(name);
    Flowchart::parse(&romfs.read(&rel)?).with_context(|| format!("parsing {rel}"))
}

/// `name`, which must be a flowchart rather than a cutscene timeline.
pub fn timeline(romfs: &Romfs, name: &str) -> Result<Flowchart> {
    load(romfs, name)?.with_context(|| timeline_error(name))
}

/// `data`, the bytes of flow `name`, parsed as [`timeline`] loads it.
pub fn parse(name: &str, data: &[u8]) -> Result<Flowchart> {
    Flowchart::parse(data)?.with_context(|| timeline_error(name))
}

fn timeline_error(name: &str) -> String {
    format!("{name} is a timeline")
}

/// `name` as `rewrites` has it, or the game's own when no other setting
/// rewrote it.
pub fn current(romfs: &Romfs, rewrites: &Rewrites, name: &str) -> Result<Flowchart> {
    let rel = rel(name);
    let data = match rewrites.get(&rel) {
        Some(data) => data.clone(),
        None => romfs.read(&rel)?,
    };
    parse(name, &data)
}

/// Load `name` as [`current`] does, run `f` on it and store the result.
pub fn edit(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    name: &str,
    f: impl FnOnce(&mut Flowchart) -> Result<()>,
) -> Result<()> {
    let mut flow = current(romfs, rewrites, name)?;
    let before = undeclared(&flow);
    f(&mut flow).with_context(|| format!("editing {name}"))?;
    let added: Vec<_> = undeclared(&flow).difference(&before).cloned().collect();
    if !added.is_empty() {
        bail!(
            "{name} names actors it doesn't declare: {}",
            added.join(", ")
        );
    }
    rewrites.insert(rel(name), flow.to_bytes());
    Ok(())
}

/// Actions a flow can end on without anything showing.
const QUIET_ENDS: [&Name; 2] = [flow::EVENT_WAIT, flow::EVENT_WAIT_CLOSE_LOADING_FADE];

/// A case can't end a flow, so one whose event is stepped over and ended
/// the flow goes here instead: an event that waits a moment and ends.
pub(crate) fn ending(flow: &Flowchart) -> Option<u16> {
    (0u16..).zip(&flow.events).find_map(|(i, e)| match &e.kind {
        EventKind::Action {
            next: None,
            actor,
            action,
            ..
        } if QUIET_ENDS
            .iter()
            .any(|n| **n == flow.actor_of(*actor).actions[*action as usize]) =>
        {
            Some(i)
        }
        _ => None,
    })
}

/// Step over event `e`, an action or a call: whatever led to it goes on to
/// what follows it. A case it ended goes to [`ending`].
pub(crate) fn step_over(flow: &mut Flowchart, e: u16) -> Result<()> {
    let next = match flow.events[e as usize].kind {
        EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } => next,
        _ => bail!("event {e} of {} is neither an action nor a call", flow.name),
    };
    for link in flow.links_to(e) {
        let to = match (link, next) {
            (Link::Branch(..), None) => Some(ending(flow).with_context(|| {
                format!(
                    "a case of {} runs event {e} last, and nothing ends quietly",
                    flow.name
                )
            })?),
            _ => next,
        };
        flow.link(link, to)?;
    }
    Ok(())
}

/// Adds or removes a pouch item, e.g. Saria's ocarina.
pub(crate) const GRANT: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT,
);
/// The item a `GRANT` adds.
pub(crate) const ACTOR: &str = "Actor";
/// How many a `GRANT` adds, or takes when negative.
pub(crate) const NUM: &str = "Num";
/// The param naming the GameData entry an event reads or sets.
pub(crate) const GAME_DATA: &Name = field::GAME_DATA_NAME;
/// The query whether a GameData Bool is set. Case 0 is set.
pub(crate) const IS_BOOL: (&Name, &Name) =
    (field::GAME_DATA_ACTOR, flow::EVENT_QUERY_IS_GAME_DATA_BOOL);
/// The query reading a GameData BoolExp. Case 0 is true.
pub(crate) const IS_EXP: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_QUERY_IS_GAME_DATA_BOOL_EXP,
);
/// The query comparing a GameData Int with its `Value`. Case 1 is equal.
pub(crate) const IS_INT: (&Name, &Name) =
    (field::GAME_DATA_ACTOR, flow::EVENT_QUERY_IS_GAME_DATA_INT);
pub(crate) const SET_BOOL: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
);
/// The query whether Link holds an `Actor`.
pub(crate) const IN_POUCH: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_QUERY_IS_EXIST_IN_POUCH,
);
/// The query whether an `Actor` fits Link's pouch. Case 0 is yes.
pub(crate) const FITS_POUCH: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_QUERY_CAN_ADD_TO_PORCH,
);

/// Set `params`' existing `key`.
pub(crate) fn set(params: &mut Params, key: &str, value: Value) -> Result<()> {
    let slot = params.iter_mut().find(|(k, _)| k == key);
    slot.with_context(|| format!("no {key} param"))?.1 = value;
    Ok(())
}

/// Run `setter`, a copy of a GameData setter, setting `name` to `value`
/// right after the call at `at`.
pub(crate) fn set_after(
    flow: &mut Flowchart,
    setter: &Model,
    at: u16,
    name: &str,
    value: Value,
) -> Result<()> {
    let EventKind::SubFlow { next, .. } = flow.events[at as usize].kind else {
        bail!("{} event {at} isn't a call", flow.name);
    };
    let (actor, action) = flow.action(&setter.actor, setter.name);
    let mut params = setter.params.clone();
    set(&mut params, GAME_DATA, string(name))?;
    set(&mut params, field::VALUE, value)?;
    let after = flow.push(EventKind::Action {
        next,
        actor,
        action,
        params,
    });
    flow.link(Link::Next(at), Some(after))
}

/// A one-string param value.
pub(crate) fn string(s: &str) -> Value {
    Value::String(vec![s.into()])
}

/// An action's, query's or call's params.
pub(crate) fn params_of(kind: &EventKind) -> Option<&Params> {
    match kind {
        EventKind::Action { params, .. }
        | EventKind::Switch { params, .. }
        | EventKind::SubFlow { params, .. }
        | EventKind::SubFlowSwitch { params, .. } => Some(params),
        _ => None,
    }
}

/// [`params_of`], to change.
pub(crate) fn params_mut(kind: &mut EventKind) -> &mut Params {
    match kind {
        EventKind::Action { params, .. }
        | EventKind::Switch { params, .. }
        | EventKind::SubFlow { params, .. }
        | EventKind::SubFlowSwitch { params, .. } => params,
        _ => unreachable!("only actions and calls are looked up"),
    }
}

/// Name `actor` in the actor param `key` of event `at`, declared like the
/// actor it replaces. The game finds a call's actor in the flow's table by
/// name: a name the table lacks indexes past it and crashes as the flow
/// loads.
pub(crate) fn rename_actor(
    flow: &mut Flowchart,
    at: u16,
    key: &str,
    actor: &str,
    instance: impl FnOnce(&str) -> String,
) -> Result<()> {
    let params = params_mut(&mut flow.events[at as usize].kind);
    let slot = params.iter_mut().find(|(k, _)| k == key);
    let slot = slot.with_context(|| format!("event {at} has no {key}"))?;
    let Value::ActorId { name, secondary } = &mut slot.1 else {
        bail!("{key} isn't an actor");
    };
    let old = std::mem::replace(name, actor.to_owned());
    let renamed = instance(secondary);
    let old_secondary = std::mem::replace(secondary, renamed.clone());
    let declared = flow
        .actors
        .iter()
        .find(|a| a.name == old && a.secondary_name == old_secondary);
    let template = declared.with_context(|| format!("{} doesn't declare {old}", flow.name))?;
    let template = Actor {
        name: actor.to_owned(),
        secondary_name: renamed,
        ..template.clone()
    };
    flow.actor_like(&template);
    Ok(())
}

/// Each actor an event's params name that `flow` doesn't declare.
pub(crate) fn undeclared(flow: &Flowchart) -> BTreeSet<String> {
    let declared = |name: &str, secondary: &str| {
        flow.actors
            .iter()
            .any(|a| a.name == name && a.secondary_name == secondary)
    };
    let params = flow.events.iter().filter_map(|e| params_of(&e.kind));
    (params.flatten())
        .filter_map(|(_, v)| match v {
            Value::ActorId { name, secondary } if !declared(name, secondary) => {
                Some(format!("<{name} {secondary}>"))
            }
            _ => None,
        })
        .collect()
}

/// The string `params` holds under `key`.
pub(crate) fn param<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == key)?.1.as_str()
}

/// A UI param's "none", as the game writes it.
const UNUSED: &str = "使用しない";

/// The UI actor every flow's fades go through.
const UI: &Name = field::UI_SYSTEM_ACTOR;
const FADE_IN: &Name = flow::EVENT_EVENT_FADE_IN;

/// Push a fade-in that runs `next` after it, a copy of the castle flow's own
/// (the one that goes to the castle): a second long, and the flow waits for it. Safe on
/// a clear screen: vanilla arrival flows fade in with no fade-out of their
/// own, as the Zora River arrival does with the same wait.
pub(crate) fn fade_in(flow: &mut Flowchart, next: Option<u16>) -> u16 {
    let ui = Actor {
        name: UI.to_string(),
        secondary_name: String::new(),
        argument: String::new(),
        argument_entry: None,
        actions: Vec::new(),
        queries: Vec::new(),
        params: vec![
            (
                flow::EVENT_MEMBER_CREATE_METHOD.to_string(),
                Value::Int(vec![0]),
            ),
            (
                field::IS_BIND_NEAREST_ACTOR.to_string(),
                Value::Bool(vec![false]),
            ),
            (flow::IS_GROUNDING.to_string(), Value::Bool(vec![false])),
            (field::IS_WORLD.to_string(), Value::Bool(vec![false])),
            (field::USE_EVENT_MODEL_ANIME.to_string(), string(UNUSED)),
            (field::USE_EVENT_AS.to_string(), string(UNUSED)),
        ],
    };
    let (actor, action) = flow.action(&ui, FADE_IN);
    flow.push(EventKind::Action {
        next,
        actor,
        action,
        params: vec![
            ("Length".into(), Value::Int(vec![1])),
            (field::IS_WAIT_END.to_string(), Value::Bool(vec![true])),
            (
                field::IS_NO_PLAY_ANIME.to_string(),
                Value::Bool(vec![false]),
            ),
            (field::COLOR.to_string(), Value::Int(vec![0])),
        ],
    })
}

/// The events in `flow` running `what`, an action or a query, with their
/// params.
pub(crate) fn events(
    flow: &Flowchart,
    what: (impl AsRef<str>, impl AsRef<str>),
) -> Vec<(u16, &Params)> {
    let (actor_name, call) = (what.0.as_ref(), what.1.as_ref());
    (0u16..)
        .zip(&flow.events)
        .filter_map(|(i, e)| {
            let (actor, name, params) = match &e.kind {
                EventKind::Action {
                    actor,
                    action,
                    params,
                    ..
                } => (
                    *actor,
                    &flow.actor_of(*actor).actions[*action as usize],
                    params,
                ),
                EventKind::Switch {
                    actor,
                    query,
                    params,
                    ..
                } => (
                    *actor,
                    &flow.actor_of(*actor).queries[*query as usize],
                    params,
                ),
                _ => return None,
            };
            (flow.actor_of(actor).name == actor_name && name == call).then_some((i, params))
        })
        .collect()
}

/// The events in `flow` running `what`.
pub(crate) fn running(flow: &Flowchart, what: (impl AsRef<str>, impl AsRef<str>)) -> Vec<u16> {
    events(flow, what).into_iter().map(|(i, _)| i).collect()
}

/// An event of a template flow, kept to build copies of.
#[derive(Clone)]
pub(crate) struct Model {
    pub actor: Actor,
    pub name: &'static str,
    pub params: Params,
}

/// A call's actor and name as text, to compare with a flow's own.
#[cfg(test)]
pub(crate) fn said((actor, name): (&Name, &Name)) -> (&'static str, &'static str) {
    (actor.get(), name.get())
}

/// A copy of the call at event `e` of `flow`, which runs `(actor, name)`.
pub(crate) fn model(flow: &Flowchart, (actor, name): (&Name, &Name), e: u16) -> Result<Model> {
    let (a, params) = match &flow.events[e as usize].kind {
        EventKind::Action { actor, params, .. } | EventKind::Switch { actor, params, .. } => {
            (flow.actor_of(*actor).clone(), params.clone())
        }
        _ => bail!("{} event {e} isn't a call", flow.name),
    };
    debug_assert_eq!(a.name, actor);
    Ok(Model {
        actor: a,
        name: name.get(),
        params,
    })
}

/// The first event of `flow` that runs query `what`.
pub(crate) fn switch(flow: &Flowchart, what: (impl AsRef<str>, impl AsRef<str>)) -> Option<u16> {
    running(flow, what).first().copied()
}

/// `flow`'s one event running `what`.
pub(crate) fn only(flow: &Flowchart, what: (impl AsRef<str>, impl AsRef<str>)) -> Result<u16> {
    let (actor, call) = (what.0.as_ref(), what.1.as_ref());
    match running(flow, (actor, call))[..] {
        [at] => Ok(at),
        ref all => bail!(
            "{} runs {actor}.{call} {} times, not once",
            flow.name,
            all.len()
        ),
    }
}

/// Every event `flow`'s entry point runs, through calls into its own
/// entry points.
pub(crate) fn reachable(flow: &Flowchart, entry: &str) -> Result<BTreeSet<u16>> {
    let start = flow.entry_points.iter().find(|e| e.name == entry);
    let start = start.with_context(|| format!("no entry point {entry}"))?;
    Ok(reachable_from(flow, start.main))
}

/// Every event that running `start` reaches, through calls into the flow's
/// own entry points.
pub(crate) fn reachable_from(flow: &Flowchart, start: Option<u16>) -> BTreeSet<u16> {
    reachable_avoiding(flow, start, None)
}

/// [`reachable_from`], never running past event `avoid`: what the flow
/// reaches on paths that don't go through it.
pub(crate) fn reachable_avoiding(
    flow: &Flowchart,
    start: Option<u16>,
    avoid: Option<u16>,
) -> BTreeSet<u16> {
    let main = |name: &str| flow.entry_points.iter().find(|e| e.name == name);
    let (mut seen, mut todo) = (BTreeSet::new(), Vec::from_iter(start));
    while let Some(e) = todo.pop() {
        if !seen.insert(e) || Some(e) == avoid {
            continue;
        }
        let kind = &flow.events[e as usize].kind;
        todo.extend(successors(kind));
        if let EventKind::SubFlow {
            flowchart,
            entry_point,
            ..
        }
        | EventKind::SubFlowSwitch {
            flowchart,
            entry_point,
            ..
        } = kind
            && is_local(flow, flowchart)
        {
            todo.extend(main(entry_point).and_then(|e| e.main));
        }
    }
    seen
}

pub(crate) fn is_local(flow: &Flowchart, callee: &str) -> bool {
    callee.is_empty() || callee == flow.name
}

/// The events an event runs next, not following calls into other entry
/// points.
pub(crate) fn successors(kind: &EventKind) -> Vec<u16> {
    match kind {
        EventKind::Action { next, .. }
        | EventKind::Join { next }
        | EventKind::SubFlow { next, .. } => next.iter().copied().collect(),
        EventKind::Switch { cases, .. } | EventKind::SubFlowSwitch { cases, .. } => {
            cases.iter().map(|c| c.1).collect()
        }
        EventKind::Fork { join, branches } => branches.iter().chain([join]).copied().collect(),
        EventKind::Return { .. } => Vec::new(),
    }
}

/// The events running `start` runs itself, not following calls into other
/// entry points.
pub(crate) fn runs_itself(flow: &Flowchart, start: Option<u16>) -> BTreeSet<u16> {
    let (mut seen, mut todo) = (BTreeSet::new(), Vec::from_iter(start));
    while let Some(e) = todo.pop() {
        if seen.insert(e) {
            todo.extend(successors(&flow.events[e as usize].kind));
        }
    }
    seen
}

/// `kind` with every event it runs next looked up in `to`, and its actor
/// swapped through `actors`.
fn moved(kind: &EventKind, to: &BTreeMap<u16, u16>, actors: &dyn Fn(u16) -> u16) -> EventKind {
    let go = |e: &u16| to[e];
    let mut kind = kind.clone();
    match &mut kind {
        EventKind::Action { next, actor, .. } => {
            *next = next.as_ref().map(go);
            *actor = actors(*actor);
        }
        EventKind::Join { next } | EventKind::SubFlow { next, .. } => *next = next.as_ref().map(go),
        EventKind::Switch { actor, cases, .. } => {
            cases.iter_mut().for_each(|c| c.1 = go(&c.1));
            *actor = actors(*actor);
        }
        EventKind::SubFlowSwitch { cases, .. } => cases.iter_mut().for_each(|c| c.1 = go(&c.1)),
        EventKind::Fork { join, branches } => {
            *join = go(join);
            branches.iter_mut().for_each(|b| *b = go(b));
        }
        EventKind::Return { .. } => {}
    }
    kind
}

/// Add `name`, a copy of the entry point `entry` and the events it runs in
/// this flow, calls into other entry points left as calls. The entry
/// point's argument actor, if it has one, is copied for it. Returns each
/// event's copy.
pub(crate) fn copy_entry(
    flow: &mut Flowchart,
    entry: &str,
    name: &str,
) -> Result<BTreeMap<u16, u16>> {
    let at = flow.entry_points.iter().position(|e| e.name == entry);
    let at = at.with_context(|| format!("no entry point {entry}"))?;
    if flow.entry_points.iter().any(|e| e.name == name) {
        bail!("{} already has {name}", flow.name);
    }
    let old = flow.entry_points[at].clone();
    let seen = runs_itself(flow, old.main);
    let first = flow.events.len() as u16;
    let to: BTreeMap<u16, u16> = seen.iter().copied().zip(first..).collect();
    let argument = flow
        .actors
        .iter()
        .position(|a| a.argument_entry == Some(at as u16));
    let new_actor = flow.actors.len() as u16;
    let actors = |a: u16| match argument {
        Some(i) if i == a as usize => new_actor,
        _ => a,
    };
    let kinds: Vec<EventKind> = seen
        .iter()
        .map(|&e| moved(&flow.events[e as usize].kind, &to, &actors))
        .collect();
    if let Some(i) = argument {
        let mut actor = flow.actors[i].clone();
        actor.secondary_name = format!("{name}({})", actor.argument);
        actor.argument_entry = Some(flow.entry_points.len() as u16);
        flow.actors.push(actor);
    }
    for (kind, &new) in kinds.into_iter().zip(to.values()) {
        let pushed = flow.push(kind);
        debug_assert_eq!(pushed, new);
    }
    flow.entry_points.push(EntryPoint {
        name: name.into(),
        main: old.main.map(|m| to[&m]),
        vars: old.vars,
    });
    Ok(to)
}

pub fn value(v: &Value) -> String {
    let list = |items: Vec<String>| match items.len() {
        1 => items.into_iter().next().unwrap(),
        _ => format!("[{}]", items.join(", ")),
    };
    match v {
        Value::Argument(s) => format!("${s}"),
        Value::Container(p) => format!("{{{}}}", params(p)),
        Value::Int(v) => list(v.iter().map(i32::to_string).collect()),
        Value::Bool(v) => list(v.iter().map(bool::to_string).collect()),
        Value::Float(v) => list(v.iter().map(f32::to_string).collect()),
        Value::String(v) => list(v.iter().map(|s| format!("{s:?}")).collect()),
        Value::ActorId { name, secondary } => format!("<{name} {secondary}>"),
    }
}

pub fn params(p: &Params) -> String {
    p.iter()
        .map(|(k, v)| format!("{k}={}", value(v)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn next(n: &Option<u16>) -> String {
    n.map_or("end".into(), |n| format!("#{n}"))
}

/// One line per entry point and per event.
pub fn render(flow: &Flowchart) -> String {
    let mut out = String::new();
    for ep in &flow.entry_points {
        let calls: Vec<String> = flow
            .calls_from(ep.main)
            .iter()
            .map(|e| format!("#{e}"))
            .collect();
        let calls = match calls.is_empty() {
            true => String::new(),
            false => format!(" (calls {})", calls.join(" ")),
        };
        writeln!(out, "entry {} -> {}{calls}", ep.name, next(&ep.main)).unwrap();
    }
    for (i, e) in flow.events.iter().enumerate() {
        let line = match &e.kind {
            EventKind::Action {
                next: n,
                actor,
                action,
                params: p,
            } => {
                let a = flow.actor_of(*actor);
                format!(
                    "{}.{}({}) -> {}",
                    a.name,
                    a.actions[*action as usize],
                    params(p),
                    next(n)
                )
            }
            EventKind::Switch {
                actor,
                query,
                params: p,
                cases,
            } => {
                let a = flow.actor_of(*actor);
                let cases: Vec<_> = cases.iter().map(|(v, e)| format!("{v}:#{e}")).collect();
                format!(
                    "{}.{}({})? {}",
                    a.name,
                    a.queries[*query as usize],
                    params(p),
                    cases.join(" ")
                )
            }
            EventKind::Fork { join, branches } => {
                let b: Vec<_> = branches.iter().map(|b| format!("#{b}")).collect();
                format!("fork {} join #{join}", b.join(" "))
            }
            EventKind::Join { next: n } => format!("join -> {}", next(n)),
            EventKind::SubFlow {
                next: n,
                params: p,
                flowchart,
                entry_point,
            } => {
                format!(
                    "call {flowchart}::{entry_point}({}) -> {}",
                    params(p),
                    next(n)
                )
            }
            EventKind::SubFlowSwitch {
                params: p,
                flowchart,
                entry_point,
                cases,
            } => {
                let cases: Vec<_> = cases.iter().map(|(v, e)| format!("{v:#x}:#{e}")).collect();
                format!(
                    "call {flowchart}::{entry_point}({})? {}",
                    params(p),
                    cases.join(" ")
                )
            }
            EventKind::Return { code } => format!("return {code:#x}"),
        };
        writeln!(out, "#{i} {line}").unwrap();
    }
    out
}

/// Every string a flowchart holds: its name, entry points, sub-flow
/// targets and string params, nested containers included.
pub fn strings(flow: &Flowchart) -> Vec<&str> {
    fn collect<'a>(p: &'a Params, out: &mut Vec<&'a str>) {
        for (_, v) in p {
            match v {
                Value::String(s) => out.extend(s.iter().map(String::as_str)),
                Value::Container(c) => collect(c, out),
                _ => {}
            }
        }
    }
    let mut out = vec![flow.name.as_str()];
    out.extend(flow.entry_points.iter().map(|e| e.name.as_str()));
    for e in &flow.events {
        match &e.kind {
            EventKind::Action { params, .. } | EventKind::Switch { params, .. } => {
                collect(params, &mut out)
            }
            EventKind::SubFlow {
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
            } => {
                collect(params, &mut out);
                out.extend([flowchart.as_str(), entry_point.as_str()]);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// Every vanilla flow writes back byte for byte, so the calls each entry
    /// point lists, which `to_bytes` works out from the events, are the
    /// game's own.
    #[test]
    fn every_flow_writes_back_as_it_was() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut differ = Vec::new();
        for name in super::names(&romfs).unwrap() {
            let data = romfs.read(&super::rel(&name)).unwrap();
            let Some(flow) = ounce_formats::Flowchart::parse(&data).unwrap() else {
                continue;
            };
            if flow.to_bytes() != data {
                differ.push(name);
            }
        }
        assert!(
            differ.is_empty(),
            "{} differ: {:?}",
            differ.len(),
            &differ[..differ.len().min(10)]
        );
    }

    use ounce_formats::{Link, relink};

    use super::*;
    use crate::known::{actor, flow, scene};

    #[test]
    fn a_rewrite_reaches_the_110_twin_too() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let twinned = rel(scene::AWAKE_FIRE_GET);
        assert_eq!(
            copies(&romfs, &twinned),
            [twinned, rel(&format!("{}.110", scene::AWAKE_FIRE_GET))]
        );
        let single = rel(actor::NPC_KAEPORA_LAKE_HYLIA);
        assert_eq!(copies(&romfs, &single), [single]);
    }

    #[test]
    fn a_copied_entry_runs_its_own_events() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = load(&romfs, flow::ENEMY_SCRUB_BUSINESS_COMMON)
            .unwrap()
            .unwrap();
        let one_time = flow::SIMPLE_SHOP_SUB_SCRUB_BUSINESS_ONE_TIME;
        let before = reachable(&flow, one_time).unwrap();
        let events = flow.events.len();
        let map = copy_entry(&mut flow, one_time, "Copy").unwrap();
        assert_eq!(map.keys().copied().collect::<BTreeSet<_>>(), before);
        assert_eq!(flow.events.len(), events + before.len());
        let copy = reachable(&flow, "Copy").unwrap();
        assert_eq!(copy, map.values().copied().collect::<BTreeSet<_>>());
        assert!(copy.is_disjoint(&before));
        assert_eq!(reachable(&flow, one_time).unwrap(), before);
        // The copy takes the same calls, and reads back from the file.
        let calls = |flow: &Flowchart, set: &BTreeSet<u16>| {
            set.iter()
                .filter(|&&e| matches!(flow.events[e as usize].kind, EventKind::SubFlow { .. }))
                .count()
        };
        assert_eq!(calls(&flow, &copy), calls(&flow, &before));
        let back = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert_eq!(reachable(&back, "Copy").unwrap(), copy);
        assert!(copy_entry(&mut flow, one_time, "Copy").is_err());
        assert!(copy_entry(&mut flow, "Nope", "Other").is_err());
    }

    /// Moves one link of each kind somewhere else, then reads the file back.
    /// Needs the dev dump, so it passes vacuously without one.
    #[test]
    fn relink_moves_entries_nexts_and_branches() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rel = rel(actor::NPC_KAEPORA_LAKE_HYLIA);
        let data = romfs.read(&rel).unwrap();
        let flow = Flowchart::parse(&data).unwrap().unwrap();
        let mut seen = [false; 3];
        for (i, e) in (0u16..).zip(&flow.events) {
            for link in flow.links_to(i) {
                let kind = match link {
                    Link::Entry(_) => 0,
                    Link::Next(_) => 1,
                    Link::Branch(..) => 2,
                    Link::Join(_) => continue,
                };
                let to = (i + 1) % flow.events.len() as u16;
                let mut moved = data.clone();
                relink(&mut moved, link, Some(to)).unwrap();
                let after = Flowchart::parse(&moved).unwrap().unwrap();
                assert!(after.links_to(to).contains(&link), "{link:?} of {}", e.name);
                assert!(!after.links_to(i).contains(&link));
                if kind < 2 {
                    relink(&mut moved, link, None).unwrap();
                    let after = Flowchart::parse(&moved).unwrap().unwrap();
                    assert!(!after.links_to(to).contains(&link));
                } else {
                    assert!(relink(&mut moved, link, None).is_err());
                }
                seen[kind] = true;
            }
        }
        assert_eq!(seen, [true; 3]);
    }
}
