//! A map's rooms as its navmesh splits them at locked doors.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{NavMesh, Romfs};

use crate::nav::graph::{Cross, Graph, Via, step_up};
use crate::nav::{kinds, navmesh_file};
use crate::warp::{self, Placement};

/// What marks a locked door, and how far from its door it sits.
pub(super) const LOCK: &str = "DoorChainLock";
pub(super) const LOCK_REACH: f32 = 1.5;

pub(super) fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

fn find(parent: &mut [usize], f: usize) -> usize {
    let mut r = f;
    while parent[r] != r {
        r = parent[r];
    }
    parent[f] = r;
    r
}

/// The doors a lock sits beside, by hash.
pub(super) fn locked_doors(placed: &[Placement]) -> BTreeSet<u64> {
    let locks: Vec<&Placement> = placed.iter().filter(|p| p.name.contains(LOCK)).collect();
    let near_lock = |at: [f32; 3]| locks.iter().any(|l| flat(l.pose.at, at) < LOCK_REACH);
    placed
        .iter()
        .filter(|p| kinds::word(&p.name) == Some("door") && near_lock(p.pose.at))
        .filter_map(|p| p.hash)
        .collect()
}

/// Each face's room: the faces joined by any crossing but a locked door,
/// named by one face of each.
pub(super) fn split(graph: &Graph, locked: &BTreeSet<u64>) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..graph.centers.len()).collect();
    for (from, arcs) in graph.arcs.iter().enumerate() {
        for arc in arcs {
            if let Cross::Link {
                via: Via::Door(Some(h)),
                ..
            } = arc.cross
                && locked.contains(&h)
            {
                continue;
            }
            let (a, b) = (find(&mut parent, from), find(&mut parent, arc.to));
            parent[a] = b;
        }
    }
    (0..parent.len()).map(|f| find(&mut parent, f)).collect()
}

/// Prints each room (the faces joined without a locked door) holding a
/// chest or a lock, and each locked door's two rooms.
pub(super) fn rooms(romfs: &Romfs, game: &Path, map: &str) -> Result<()> {
    let bytes = navmesh_file(romfs, game, map)?.with_context(|| format!("{map} has no navmesh"))?;
    let mesh = NavMesh::parse(&bytes)?;
    let placed = warp::placements_in(romfs, map)?;
    let graph = Graph::build(&mesh, &placed, step_up());
    let locked = locked_doors(&placed);
    let roots = split(&graph, &locked);
    let room = |at: [f32; 3]| graph.locate(at).map(|s| roots[s.face]);
    let mut held: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for p in &placed {
        let word = kinds::word(&p.name);
        if word != Some("chest") && !p.name.contains(LOCK) {
            continue;
        }
        let r = room(p.pose.at).map_or("off".into(), |r| r.to_string());
        let hash = p.hash.map_or("-".into(), |h| h.to_string());
        let [x, y, z] = p.pose.at;
        println!("{r} {} {hash} {x:.1},{y:.1},{z:.1}", p.name);
        if let Ok(r) = r.parse() {
            held.entry(r).or_default().push(p.name.clone());
        }
    }
    let rooms: BTreeSet<usize> = roots.iter().copied().collect();
    println!("{} rooms, {} locked doors", rooms.len(), locked.len());
    for door in placed
        .iter()
        .filter(|p| p.hash.is_some_and(|h| locked.contains(&h)))
    {
        let [x, y, z] = door.pose.at;
        let ahead = warp::landing_at(&door.pose, warp::Side::Front, 1.0);
        let behind = warp::landing_at(&door.pose, warp::Side::Back, 1.0);
        let (a, b) = (room(ahead), room(behind));
        println!(
            "door {} {x:.1},{y:.1},{z:.1}: {a:?} | {b:?}",
            door.hash.unwrap_or(0)
        );
    }
    Ok(())
}
