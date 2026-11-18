//! Each cow's milk, once Epona's Song is right: if its Bool is set, the milk
//! as before. Else set it and grant the seed's item.
//!
//! A cow's song event first takes the ocarina out, then asks whether the
//! melody played was the right one. The gate goes on that answer's right
//! side, inside the two sub-entries every cow's song calls
//! (`flag::COW_GIVE_MILK`, and its Gerudo variant for the Gerudo cows). Each
//! cow's call passes its Bool and item as arguments. The grant is inline,
//! as the plain cows' and Link's house's events never bind
//! `GetPouchContent`.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, BymlDoc, EventKind, Flowchart, Params, Romfs, Value};

use super::{Cow, FLOW, PLAIN, event_of, item};
use crate::checks::Drop;
use crate::flows::{
    self, ACTOR, GAME_DATA, IS_BOOL, Model, Rewrites, SET_BOOL, running, set, string, switch,
};
use crate::gifts::levels::Level;
use crate::grant;
use crate::known::{Name, field, flag, flow};
use crate::rsdb;
use crate::standins::row;

/// Whether the melody just played was the one the event asked for. Case 0
/// is the right one.
pub(super) const MELODY: (&Name, &Name) = (
    field::GAME_SYSTEM_ACTOR,
    flow::EVENT_QUERY_IS_RECOMMENDED_MELODY,
);
/// The arguments a cow's call passes the gate.
pub(super) const ARG_FLAG: &str = "ZootrCowFlag";
pub(crate) const ARG_ITEM: &str = "ZootrCowItem";
/// The sub-entries every cow's song calls, which ask the melody.
pub(super) const MILK: [&Name; 2] = [flag::COW_GIVE_MILK, flow::COW_GIVE_MILK_GERUDO];
/// The milk sub-entries' argument naming the cow by actor name.
const COW: &str = "Cow";
const FLOW_FILE: &Name = field::FLOWCHART_FILE_NAME;
const ENTRY: &Name = field::ENTRY_POINT_NAME;

/// The GameData events the gate copies from the cow flow.
struct Models {
    is_bool: Model,
    set_bool: Model,
}

impl Models {
    fn load(flow: &Flowchart) -> Result<Self> {
        let asks = switch(flow, IS_BOOL).with_context(|| format!("{FLOW} asks no Bool"))?;
        let sets = *running(flow, SET_BOOL)
            .first()
            .with_context(|| format!("{FLOW} sets no Bool"))?;
        Ok(Self {
            is_bool: flows::model(flow, IS_BOOL, asks)?,
            set_bool: flows::model(flow, SET_BOOL, sets)?,
        })
    }
}

/// The flow and the entry an `EventSetting` row runs.
pub(super) fn target(events: &BymlDoc, id: &str) -> Result<(String, String)> {
    let row = row(events, id)?;
    let text = |key: &str| {
        row.get(key)
            .and_then(|v| v.as_string().ok())
            .map(|s| s.to_string())
            .with_context(|| format!("{id} has no {key}"))
    };
    let flow = crate::rsdb::flow_name(&text(FLOW_FILE)?).to_owned();
    Ok((flow, text(ENTRY)?))
}

/// One cow's song entry, and what its milk call passes the gate.
struct Call<'a> {
    entry: String,
    /// For a plain cow: the shared entry its own is copied from, and its
    /// clone's actor name, which the copy names in place of the plain one.
    plain: Option<(String, String)>,
    flag: String,
    item: &'a str,
}

/// Gate the milk on every cow's Bool, granting its item from `drops`. Ammo,
/// and a line's item with progressive items on, give the step Link needs.
pub fn give(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    cows: &[Cow],
    drops: &HashMap<u64, Drop>,
    level: Level,
) -> Result<()> {
    if cows.is_empty() {
        return Ok(());
    }
    let events = BymlDoc::parse(&romfs.read(&rsdb::path(rsdb::EVENTS))?)?;
    let (plain_flow, milk) = target(&events, &event_of(&ActorPack::load(romfs, PLAIN)?)?)?;
    let mut by_flow: BTreeMap<String, Vec<Call>> = BTreeMap::new();
    for cow in cows {
        let (flow, call) = match cow.plain() {
            true => (
                plain_flow.clone(),
                Call {
                    entry: cow.event.clone(),
                    plain: Some((milk.clone(), cow.clone_name())),
                    flag: cow.flag(),
                    item: item(drops, cow),
                },
            ),
            false => {
                let (flow, entry) = target(&events, &cow.event)?;
                let call = Call {
                    entry,
                    plain: None,
                    flag: cow.flag(),
                    item: item(drops, cow),
                };
                (flow, call)
            }
        };
        by_flow.entry(flow).or_default().push(call);
    }
    let (template, pouch) = grant::Models::load(romfs)?;
    let inline = grant::Models::of(&template, &pouch);
    let own = by_flow.remove(&**FLOW).unwrap_or_default();
    flows::edit(romfs, rewrites, FLOW, |flow| {
        gate(flow, &inline)?;
        own.iter().try_for_each(|c| pass(flow, c, level))
    })?;
    for (name, calls) in by_flow {
        flows::edit(romfs, rewrites, &name, |flow| {
            calls.iter().try_for_each(|c| pass(flow, c, level))
        })?;
    }
    Ok(())
}

/// After each right melody, ask the Bool the caller passed: set, the milk
/// as before. Else set it, then grant the item the caller passed.
fn gate(flow: &mut Flowchart, inline: &grant::Models) -> Result<()> {
    let models = Models::load(flow)?;
    let asks = melodies(flow);
    ensure!(!asks.is_empty(), "{FLOW} never asks the melody");
    let flag = || Value::Argument(ARG_FLAG.into());
    for at in asks {
        let EventKind::Switch { cases, .. } = &flow.events[at as usize].kind else {
            unreachable!("melodies finds switches");
        };
        let right = cases.iter().find(|&&(case, _)| case == 0);
        let (_, right) = *right.with_context(|| format!("{FLOW} event {at} has no right side"))?;
        let grant = inline.push(flow, Value::Argument(ARG_ITEM.into()), None)?;
        let mut params = models.set_bool.params.clone();
        set(&mut params, GAME_DATA, flag())?;
        set(&mut params, "Value", Value::Bool(vec![true]))?;
        let (actor, action) = flow.action(&models.set_bool.actor, models.set_bool.name);
        let mark = flow.push(EventKind::Action {
            next: Some(grant),
            actor,
            action,
            params,
        });
        let mut params = models.is_bool.params.clone();
        set(&mut params, GAME_DATA, flag())?;
        let (actor, query) = flow.query(&models.is_bool.actor, models.is_bool.name);
        // Case 0 is true.
        let head = flow.push(EventKind::Switch {
            actor,
            query,
            params,
            cases: vec![(0, right), (1, mark)],
        });
        let EventKind::Switch { cases, .. } = &mut flow.events[at as usize].kind else {
            unreachable!("melodies finds switches");
        };
        cases
            .iter_mut()
            .filter(|(case, _)| *case == 0)
            .for_each(|c| c.1 = head);
    }
    Ok(())
}

/// The events of `flow` asking whether the melody was right.
pub(super) fn melodies(flow: &Flowchart) -> Vec<u16> {
    (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| match &e.kind {
            EventKind::Switch { actor, query, .. } => {
                let a = flow.actor_of(*actor);
                a.name == MELODY.0 && a.queries[*query as usize] == MELODY.1
            }
            _ => false,
        })
        .map(|(i, _)| i)
        .collect()
}

/// Make `c`'s entry pass its Bool and item to every milk call it makes. A
/// plain cow's entry is first copied from the shared one, its clone named
/// in place of the plain actor. A call passing ammo or a line's item asks
/// the pouch first and passes the step Link needs.
fn pass(flow: &mut Flowchart, c: &Call, level: Level) -> Result<()> {
    let calls: Vec<u16> = match &c.plain {
        Some((shared, clone)) => {
            let copy = flows::copy_entry(flow, shared, &c.entry)?;
            let plain = flow
                .actors
                .iter()
                .find(|a| a.name == PLAIN && a.secondary_name.is_empty());
            let mut actor = plain
                .with_context(|| format!("{} has no {PLAIN}", flow.name))?
                .clone();
            (actor.name, actor.actions, actor.queries) = (clone.clone(), vec![], vec![]);
            flow.actors.push(actor);
            let calls: Vec<u16> = copy.values().copied().filter(|&e| milk(flow, e)).collect();
            for &at in &calls {
                rename(params(flow, at), clone)?;
            }
            calls
        }
        None => {
            let all = flows::reachable(flow, &c.entry)?;
            all.into_iter().filter(|&e| milk(flow, e)).collect()
        }
    };
    ensure!(!calls.is_empty(), "{} never calls the milk", c.entry);
    for at in calls {
        let params = params(flow, at);
        params.push((ARG_FLAG.into(), string(&c.flag)));
        params.push((ARG_ITEM.into(), string(c.item)));
        level.chain(flow, c.item, at)?;
    }
    Ok(())
}

/// Whether event `e` calls one of the milk sub-entries that ask the melody.
fn milk(flow: &Flowchart, e: u16) -> bool {
    matches!(&flow.events[e as usize].kind,
        EventKind::SubFlow { flowchart, entry_point, .. }
            if (flowchart.is_empty() || flowchart == FLOW) && MILK.iter().any(|m| entry_point == m))
}

fn params(flow: &mut Flowchart, at: u16) -> &mut Params {
    flows::params_mut(&mut flow.events[at as usize].kind)
}

/// Name `clone` where a call names the plain cow: its `Cow` and `Actor`.
fn rename(params: &mut Params, clone: &str) -> Result<()> {
    set(params, COW, string(clone))?;
    set(
        params,
        ACTOR,
        Value::ActorId {
            name: clone.into(),
            secondary: String::new(),
        },
    )
}
