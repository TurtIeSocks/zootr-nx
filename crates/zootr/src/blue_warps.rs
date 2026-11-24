//! Blue warps that keep Link's age. Every boss's warp but King Dodongo's
//! names one era's map, and landing there turns Link that era. Once
//! dungeon doors shuffle a boss can fall to the other era, so each map
//! change in the warp flow asks Link's age first, as Dodongo's already
//! does, and the other era lands in its own copy of the same area.

use anyhow::{Context, Result};
use ounce_formats::{EventKind, Flowchart, Link, Romfs, Value};

use crate::flows::{self, Rewrites};
use crate::known::{Name, actor, field, flow, scene};
use crate::warp::{ADULT, CHILD};

/// The flow every boss's warp leaves through.
pub const FLOW: &Name = scene::SY_BANC_CHANGE_ESCAPE_DUNGEON;

/// The age query, as Dodongo's warp asks it. Case 0 is child.
const IS_CHILD: (&Name, &Name) = (actor::PLAYER_EVENT, flow::EVENT_QUERY_PLAYER_IS_CHILD);

/// Arrival points only one era's copy of a map has, each with where the
/// other copy lands instead.
const ONE_ERA_SPOTS: [(&Name, &Name); 3] = [
    (field::WARP_FROM_DEKU_TREE, field::FRONT_DEKU_TREE),
    (field::WARP_FROM_DEKU_TREE_2, field::FRONT_DEKU_TREE),
    (field::WARP_FROM_FOREST_TEMPLE, field::WARP_FROM_DEKU_TREE),
];

/// The map param of each event that names an era's map: a map change and
/// its preload.
const MAP_PARAMS: [(&Name, &Name); 2] = [
    (flow::EVENT_RESERVE_BANC_CHANGE, field::NEXT_BANC),
    (
        flow::EVENT_TRIGGER_BANC_REQUEST_BEFOREHAND_LOAD,
        field::BANC_PATH,
    ),
];

/// Gives each era map the warp flow names an age branch, and returns how
/// many it branched.
pub fn keep_age(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<usize> {
    let mut branched = 0;
    flows::edit(romfs, rewrites, FLOW, |flow| {
        branched = branch(flow)?;
        Ok(())
    })?;
    Ok(branched)
}

fn branch(flow: &mut Flowchart) -> Result<usize> {
    let template = (flow.actors.iter())
        .find(|a| a.name == **IS_CHILD.0)
        .cloned()
        .context("the warp flow has no player actor")?;
    let (actor, query) = flow.query(&template, IS_CHILD.1);
    let asked = asked(flow, actor, query);
    let mut branched = 0;
    for e in 0..flow.events.len() as u16 {
        let Some((key, map, child)) = era_map(flow, e) else {
            continue;
        };
        if asked.contains(&e) {
            continue;
        }
        let other = other_era(&map, child);
        let EventKind::Action {
            next,
            actor: a,
            action,
            params,
        } = flow.events[e as usize].kind.clone()
        else {
            unreachable!("era_map only finds actions")
        };
        let mut params = params;
        flows::set(&mut params, key, Value::String(vec![other]))?;
        move_spot(&mut params)?;
        let copy = flow.push(EventKind::Action {
            next,
            actor: a,
            action,
            params,
        });
        let (kid, grown) = if child { (e, copy) } else { (copy, e) };
        let ask = flow.push(EventKind::Switch {
            actor,
            query,
            params: vec![],
            cases: vec![(0, kid), (1, grown)],
        });
        for link in flow.links_to(e) {
            if link != Link::Branch(ask, if child { 0 } else { 1 }) {
                flow.link(link, Some(ask))?;
            }
        }
        branched += 1;
    }
    Ok(branched)
}

/// The events an age query already picks between, like Dodongo's maps.
fn asked(flow: &Flowchart, actor: u16, query: u16) -> Vec<u16> {
    (flow.events.iter())
        .filter_map(|e| match &e.kind {
            EventKind::Switch {
                actor: a,
                query: q,
                cases,
                ..
            } if (*a, *q) == (actor, query) => Some(cases.iter().map(|c| c.1)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The param naming an era's map on event `e`, the map, and whether it's
/// the child one.
fn era_map(flow: &Flowchart, e: u16) -> Option<(&'static Name, String, bool)> {
    let EventKind::Action {
        actor,
        action,
        params,
        ..
    } = &flow.events[e as usize].kind
    else {
        return None;
    };
    let name = &flow.actor_of(*actor).actions[*action as usize];
    let (_, key) = MAP_PARAMS.iter().find(|(a, _)| **a == *name)?;
    let map = flows::param(params, key)?.to_owned();
    let stem = map.strip_suffix(".bcett.json").unwrap_or(&map);
    let child = match (stem.ends_with(CHILD), stem.ends_with(ADULT)) {
        (true, _) => true,
        (_, true) => false,
        _ => return None,
    };
    Some((key, map, child))
}

/// `map` with its era swapped, a preload's path too.
fn other_era(map: &str, child: bool) -> String {
    let (from, to) = if child {
        (CHILD, ADULT)
    } else {
        (ADULT, CHILD)
    };
    match map.strip_suffix(".bcett.json") {
        Some(stem) => format!("{}{to}.bcett.json", stem.strip_suffix(from).unwrap_or(stem)),
        None => format!("{}{to}", map.strip_suffix(from).unwrap_or(map)),
    }
}

/// Moves a map change's arrival point to one the other copy has, and drops
/// its fixed position, which belongs to the old point.
fn move_spot(params: &mut ounce_formats::Params) -> Result<()> {
    let Some(spot) = flows::param(params, field::NEXT_START_POS) else {
        return Ok(());
    };
    let Some((_, to)) = ONE_ERA_SPOTS.iter().find(|(from, _)| ***from == *spot) else {
        return Ok(());
    };
    flows::set(
        params,
        field::NEXT_START_POS,
        Value::String(vec![to.to_string()]),
    )?;
    for key in [field::NEXT_POSITION, field::NEXT_START_ROT] {
        flows::set(params, key, Value::Float(vec![0.0; 3]))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::flag;

    fn branched(romfs: &Romfs) -> Flowchart {
        let mut rewrites = Rewrites::new();
        assert!(keep_age(romfs, &mut rewrites).unwrap() > 0);
        Flowchart::parse(&rewrites[&flows::rel(FLOW)])
            .unwrap()
            .unwrap()
    }

    /// The map param and arrival point of each map change.
    fn changes(flow: &Flowchart) -> Vec<(String, String)> {
        (0..flow.events.len() as u16)
            .filter_map(|e| {
                let (key, map, _) = era_map(flow, e)?;
                let EventKind::Action { params, .. } = &flow.events[e as usize].kind else {
                    return None;
                };
                (*key == **field::NEXT_BANC).then(|| {
                    let spot = flows::param(params, field::NEXT_START_POS).unwrap_or_default();
                    (map, spot.to_owned())
                })
            })
            .collect()
    }

    #[test]
    fn every_era_map_change_asks_links_age_first() {
        let Some(romfs) = dev_dump() else { return };
        let flow = branched(&romfs);
        let (actor, query) = (0..flow.actors.len() as u16)
            .find_map(|a| {
                let q = flow
                    .actor_of(a)
                    .queries
                    .iter()
                    .position(|q| *q == **IS_CHILD.1)?;
                (flow.actor_of(a).name == **IS_CHILD.0).then_some((a, q as u16))
            })
            .unwrap();
        let asked = asked(&flow, actor, query);
        for e in 0..flow.events.len() as u16 {
            if era_map(&flow, e).is_some() {
                assert!(asked.contains(&e), "event {e} changes map without asking");
                assert!(
                    flow.links_to(e)
                        .iter()
                        .all(|l| matches!(l, Link::Branch(..))),
                    "event {e} runs unasked"
                );
            }
        }
    }

    #[test]
    fn each_copy_lands_on_an_arrival_point_its_map_has() {
        let Some(romfs) = dev_dump() else { return };
        for (map, spot) in changes(&branched(&romfs)) {
            let placed = crate::warp::placements_in(&romfs, &map).unwrap();
            let start = |p: &crate::warp::Placement| {
                p.name == **flag::START_POS && p.label.as_deref() == Some(spot.as_str())
            };
            assert!(placed.iter().any(start), "{map} has no {spot}");
        }
    }

    #[test]
    fn a_second_pass_branches_nothing_more() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        keep_age(&romfs, &mut rewrites).unwrap();
        assert_eq!(keep_age(&romfs, &mut rewrites).unwrap(), 0);
    }
}
