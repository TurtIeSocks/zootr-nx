//! A map's floor plan as JSON: its navmesh's faces by room, every placement
//! on them, and each locked door's two rooms. The ledger draws it.

use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{NavMesh, Romfs};
use serde::Serialize;

use super::rooms::{locked_doors, split};
use crate::nav::graph::{Graph, STEP_UP};
use crate::nav::navmesh_file;
use crate::warp::{self, Side};

#[derive(Serialize)]
struct Plan {
    map: String,
    vertices: Vec<[f32; 3]>,
    faces: Vec<Face>,
    placed: Vec<Spot>,
    doors: Vec<Door>,
}

/// A navmesh face: its vertices in order, and its room.
#[derive(Serialize)]
struct Face {
    ring: Vec<usize>,
    room: usize,
}

/// A placement. Hashes are strings, past what a JS number holds.
#[derive(Serialize)]
struct Spot {
    hash: Option<String>,
    name: String,
    label: Option<String>,
    at: [f32; 3],
    /// Its navmesh room (`Face::room`).
    room: Option<usize>,
    /// The dungeon room it names, which key logic counts by.
    dungeon_room: Option<u32>,
}

/// A locked door and the rooms before and behind it.
#[derive(Serialize)]
struct Door {
    hash: String,
    at: [f32; 3],
    rooms: [Option<usize>; 2],
}

pub(super) fn map(romfs: &Romfs, game: &Path, map: &str) -> Result<()> {
    let bytes = navmesh_file(romfs, game, map)?.with_context(|| format!("{map} has no navmesh"))?;
    let mesh = NavMesh::parse(&bytes)?;
    let placed = warp::placements_in(romfs, map)?;
    let graph = Graph::build(&mesh, &placed, STEP_UP);
    let locked = locked_doors(&placed);
    let roots = split(&graph, &locked);
    let room = |at: [f32; 3]| graph.locate(at).map(|s| roots[s.face]);
    let named = warp::rooms_in(romfs, map)?;
    let faces = (0..mesh.faces.len())
        .map(|f| Face {
            ring: mesh.ring(f).iter().map(|e| e.a).collect(),
            room: roots[f],
        })
        .collect();
    let doors = placed
        .iter()
        .filter_map(|p| Some((p, p.hash.filter(|h| locked.contains(h))?)))
        .map(|(p, h)| Door {
            hash: h.to_string(),
            at: p.pose.at,
            rooms: [Side::Front, Side::Back].map(|s| room(warp::landing_at(&p.pose, s, 1.0))),
        })
        .collect();
    let spots = placed
        .iter()
        .map(|p| Spot {
            hash: p.hash.map(|h| h.to_string()),
            name: p.name.clone(),
            label: p.label.clone(),
            at: p.pose.at,
            room: room(p.pose.at),
            dungeon_room: p.hash.and_then(|h| named.get(&h).copied()),
        })
        .collect();
    let plan = Plan {
        map: map.to_owned(),
        vertices: mesh.vertices.clone(),
        faces,
        placed: spots,
        doors,
    };
    println!("{}", serde_json::to_string(&plan)?);
    Ok(())
}
