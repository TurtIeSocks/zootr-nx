//! A Bool per dungeon, set as Link arrives through its front door, so the
//! tracker can show which door leads where once he's been through it.
//!
//! A door names the event its arrival plays: `NextEventName`, and
//! `InitEventName` the first time. Most are the dungeon's own, so the
//! Bool goes at the start of that event. The generic arrival several
//! dungeons share gets a copy per dungeon instead, which sets the Bool and
//! runs on as the shared one does. Renaming a dungeon's own event would
//! lose its played flag, which the game sets by event name.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use ounce_formats::{EntryPoint, EventKind, Romfs};

use super::{BLOCK, Blocks, pool};
use crate::flows::{self, GAME_DATA, Rewrites, SET_BOOL};
use crate::known::{Name, field};
use crate::rsdb::{self, Events, Runs};
use crate::settings::DungeonEntrances;

/// The block fields naming an arrival event.
const ARRIVALS: [&Name; 2] = [field::NEXT_EVENT_NAME, field::INIT_EVENT_NAME];

/// The Bool set once Link has come into dungeon `i` by its front door.
pub fn flag(i: usize) -> String {
    format!("Rando_Door_{i:02}")
}

/// The Bools `mode`'s doors set.
pub fn flags(mode: DungeonEntrances) -> Vec<String> {
    pool(mode).map(flag).collect()
}

/// An arrival event that sets dungeon `dungeon`'s Bool.
#[derive(Debug, PartialEq)]
pub struct Arrival {
    pub dungeon: usize,
    pub event: String,
    /// The per-dungeon copy a shared event gets, which its door names
    /// instead.
    pub copy: Option<String>,
}

/// Each arrival event of the doors `mode` shuffles, once per dungeon.
pub fn plan(blocks: &Blocks, mode: DungeonEntrances) -> Vec<Arrival> {
    let events = |i: usize| -> BTreeSet<String> {
        let block = &blocks.into[i];
        (BLOCK.iter().zip(block))
            .filter(|(k, _)| ARRIVALS.contains(k))
            .filter_map(|(_, v)| Some(v.as_ref()?.as_string().ok()?.to_string()))
            .collect()
    };
    let mut users: BTreeMap<String, usize> = BTreeMap::new();
    for i in 0..blocks.into.len() {
        for e in events(i) {
            *users.entry(e).or_default() += 1;
        }
    }
    (pool(mode))
        .flat_map(|i| {
            let shared = |e: &String| users[e] > 1;
            (events(i).into_iter()).map(move |event| Arrival {
                dungeon: i,
                copy: shared(&event).then(|| format!("{}_Arrive", flag(i))),
                event,
            })
        })
        .collect()
}

/// Points each door at its dungeon's copy of a shared arrival.
pub fn retarget(blocks: &mut Blocks, plan: &[Arrival]) -> Result<()> {
    let at = BLOCK.iter().position(|k| *k == field::NEXT_EVENT_NAME);
    let at = at.context("no next event in a door's block")?;
    for a in plan {
        let Some(copy) = &a.copy else { continue };
        let slot = &mut blocks.into[a.dungeon][at];
        if slot
            .as_ref()
            .and_then(|v| v.as_string().ok())
            .map(|s| s.as_str())
            == Some(a.event.as_str())
        {
            *slot = Some(ounce_formats::Byml::String(copy.as_str().into()));
        }
    }
    Ok(())
}

/// The event table rows each copy needs.
pub fn events(plan: &[Arrival], events: &mut Events) -> Result<()> {
    for a in plan {
        if let Some(copy) = &a.copy {
            events.copy(copy, &a.event)?;
        }
    }
    Ok(())
}

/// The flow edits: each arrival sets its dungeon's Bool first.
pub fn flows(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    runs: &BTreeMap<String, Runs>,
    plan: &[Arrival],
) -> Result<()> {
    let model = {
        let warp = flows::current(romfs, rewrites, crate::blue_warps::FLOW)?;
        let sets = flows::running(&warp, SET_BOOL);
        let at = *sets.first().context("the warp flow sets no Bool")?;
        flows::model(&warp, SET_BOOL, at)?
    };
    for a in plan {
        let (flow, entry) =
            (runs.get(&a.event)).with_context(|| format!("no event row for {}", a.event))?;
        flows::edit(romfs, rewrites, flow, |flow| {
            let at = match entry {
                Some(name) => flow.entry_points.iter().position(|e| e.name == *name),
                None => (!flow.entry_points.is_empty()).then_some(0),
            };
            let at = at.with_context(|| format!("{} has no entry for {}", flow.name, a.event))?;
            let (actor, action) = flow.action(&model.actor, model.name);
            let mut params = model.params.clone();
            flows::set(&mut params, GAME_DATA, flows::string(&flag(a.dungeon)))?;
            let old = flow.entry_points[at].clone();
            let set = flow.push(EventKind::Action {
                next: old.main,
                actor,
                action,
                params,
            });
            match &a.copy {
                Some(copy) => flow.entry_points.push(EntryPoint {
                    name: copy.clone(),
                    main: Some(set),
                    vars: old.vars,
                }),
                None => flow.entry_points[at].main = Some(set),
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// [`flows`] for a seed whose doors `mode` shuffles, reading what it needs.
pub fn flows_for(romfs: &Romfs, rewrites: &mut Rewrites, mode: DungeonEntrances) -> Result<()> {
    let plan = plan(&Blocks::read(romfs)?, mode);
    flows(romfs, rewrites, &rsdb::events(romfs)?, &plan)
}

#[cfg(test)]
mod tests {
    use super::super::{Doors, Role, actors, bancs, door_of_actor, rewrite, role_of_door};
    use super::*;
    use crate::checks::str_field;
    use crate::game::dev_dump;
    use crate::world::node_of;

    fn arrivals(romfs: &Romfs, mode: DungeonEntrances) -> (Blocks, Vec<Arrival>) {
        let blocks = Blocks::read(romfs).unwrap();
        let plan = plan(&blocks, mode);
        (blocks, plan)
    }

    #[test]
    fn a_shared_arrival_gets_a_copy_per_dungeon_and_the_rest_take_the_bool_in_place() {
        let Some(romfs) = dev_dump() else { return };
        let (_, plan) = arrivals(&romfs, DungeonEntrances::OnGanon);
        let dungeons: BTreeSet<usize> = plan.iter().map(|a| a.dungeon).collect();
        assert_eq!(dungeons, pool(DungeonEntrances::OnGanon).collect());
        let users = |e: &str| {
            let of = plan.iter().filter(|a| a.event == e);
            of.map(|a| a.dungeon).collect::<BTreeSet<_>>().len()
        };
        for a in &plan {
            assert_eq!(a.copy.is_some(), users(&a.event) > 1, "{a:?}");
        }
        assert!(plan.iter().any(|a| a.copy.is_some()));
        assert!(plan.iter().any(|a| a.copy.is_none()));
    }

    #[test]
    fn every_arrival_sets_its_dungeons_bool_first() {
        let Some(romfs) = dev_dump() else { return };
        let (_, plan) = arrivals(&romfs, DungeonEntrances::OnGanon);
        let runs = rsdb::events(&romfs).unwrap();
        let mut rewrites = Rewrites::new();
        flows(&romfs, &mut rewrites, &runs, &plan).unwrap();
        for a in &plan {
            let (name, entry) = &runs[&a.event];
            let flow = flows::current(&romfs, &rewrites, name).unwrap();
            let entry = match (&a.copy, entry) {
                (Some(copy), _) => flow.entry_points.iter().find(|e| e.name == *copy),
                (None, Some(entry)) => flow.entry_points.iter().find(|e| e.name == *entry),
                (None, None) => flow.entry_points.first(),
            };
            let first = entry.and_then(|e| e.main).unwrap();
            let EventKind::Action {
                actor,
                action,
                params,
                ..
            } = &flow.events[first as usize].kind
            else {
                panic!("{a:?} starts with a switch")
            };
            assert_eq!(
                flow.actor_of(*actor).actions[*action as usize],
                **SET_BOOL.1
            );
            assert_eq!(
                flows::param(params, GAME_DATA),
                Some(flag(a.dungeon).as_str())
            );
        }
    }

    #[test]
    fn an_unmoved_door_names_its_dungeons_copy_too() {
        let Some(romfs) = dev_dump() else { return };
        let (mut blocks, plan) = arrivals(&romfs, DungeonEntrances::On);
        retarget(&mut blocks, &plan).unwrap();
        let mut named: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        for (file, doc) in bancs(&romfs).unwrap() {
            let mut root = doc.unwrap().root;
            rewrite(&file, &mut root, &Doors::default(), &blocks).unwrap();
            let node = node_of(&file).0;
            for d in actors(&root).unwrap().iter().filter_map(door_of_actor) {
                if let Some(Role::Into(i)) = role_of_door(&node, d) {
                    let next = str_field(d, field::NEXT_EVENT_NAME).unwrap_or("-");
                    named.entry(i).or_default().insert(next.to_owned());
                }
            }
        }
        for a in plan.iter().filter(|a| a.copy.is_some()) {
            assert_eq!(named[&a.dungeon], BTreeSet::from([a.copy.clone().unwrap()]));
        }
    }
}
