//! A map's rooms as its navmesh splits them at locked doors.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{NavMesh, Romfs};

use super::xyz;
use crate::checks::flat;
use crate::nav::disjoint::DisjointSet;
use crate::nav::graph::{Cross, Graph, Via, step_up};
use crate::nav::{kinds, navmesh_file};
use crate::warp::{self, Placement};

/// What marks a locked door, and how far from its door it sits.
pub(super) const LOCK: &str = "DoorChainLock";
pub(super) const LOCK_REACH: f32 = 1.5;

/// A map's navmesh and its placements, to split into rooms.
pub(super) struct Rooms {
    pub(super) mesh: NavMesh,
    pub(super) placed: Vec<Placement>,
}

impl Rooms {
    pub(super) fn load(romfs: &Romfs, game: &Path, map: &str) -> Result<Self> {
        let bytes =
            navmesh_file(romfs, game, map)?.with_context(|| format!("{map} has no navmesh"))?;
        Ok(Self {
            mesh: NavMesh::parse(&bytes)?,
            placed: warp::placements_in(romfs, map)?,
        })
    }

    /// The map's graph, its locked doors, and each face's room as `split`
    /// names it.
    pub(super) fn split(&self) -> (Graph<'_>, BTreeSet<u64>, Vec<usize>) {
        let graph = Graph::build(&self.mesh, &self.placed, step_up());
        let locked = locked_doors(&self.placed);
        let roots = split(&graph, &locked);
        (graph, locked, roots)
    }
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
    let mut sets = DisjointSet::new(graph.centers.len());
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
            sets.union(from, arc.to);
        }
    }
    sets.roots()
}

/// Prints each room (the faces joined without a locked door) holding a
/// chest or a lock, and each locked door's two rooms.
pub(super) fn rooms(romfs: &Romfs, game: &Path, map: &str) -> Result<()> {
    let level = Rooms::load(romfs, game, map)?;
    let placed = &level.placed;
    let (graph, locked, roots) = level.split();
    let room = |at: [f32; 3]| graph.locate(at).map(|s| roots[s.face]);
    for p in placed {
        let word = kinds::word(&p.name);
        if word != Some("chest") && !p.name.contains(LOCK) {
            continue;
        }
        let r = room(p.pose.at).map_or("off".into(), |r| r.to_string());
        let hash = p.hash.map_or("-".into(), |h| h.to_string());
        println!("{r} {} {hash} {}", p.name, xyz(p.pose.at));
    }
    let rooms: BTreeSet<usize> = roots.iter().copied().collect();
    println!("{} rooms, {} locked doors", rooms.len(), locked.len());
    for door in placed
        .iter()
        .filter(|p| p.hash.is_some_and(|h| locked.contains(&h)))
    {
        let ahead = warp::landing_at(&door.pose, warp::Side::Front, 1.0);
        let behind = warp::landing_at(&door.pose, warp::Side::Back, 1.0);
        let (a, b) = (room(ahead), room(behind));
        println!(
            "door {} {}: {a:?} | {b:?}",
            door.hash.unwrap_or(0),
            xyz(door.pose.at)
        );
    }
    Ok(())
}
