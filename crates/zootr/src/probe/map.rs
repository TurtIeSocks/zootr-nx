//! A map's floor plan as JSON: its navmesh's faces by room and storey,
//! every placement on them, and each lock with the rooms on either side.
//! The ledger draws it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{NavMesh, Romfs};
use serde::Serialize;

use super::rooms::{LOCK, LOCK_REACH, flat, locked_doors, split};
use crate::key_rooms::{self, RoomKeys};
use crate::nav::graph::{Graph, STEP_UP};
use crate::nav::navmesh_file;
use crate::warp;

#[derive(Serialize)]
struct Plan {
    map: String,
    vertices: Vec<[f32; 3]>,
    floors: Vec<Floor>,
    faces: Vec<Face>,
    placed: Vec<Spot>,
    locks: Vec<Lock>,
    /// `None` outside a dungeon with small keys.
    keys: Option<RoomKeys>,
}

/// A storey: the dungeon rooms on it, lowest first, and its mean height.
#[derive(Serialize, Debug, PartialEq)]
struct Floor {
    rooms: Vec<u32>,
    height: f32,
}

/// A navmesh face: its vertices in order, its navmesh room, and the
/// dungeon room of the nearest placement naming one in that navmesh room.
#[derive(Serialize)]
struct Face {
    ring: Vec<usize>,
    room: usize,
    dungeon_room: Option<u32>,
    /// Its storey in `Plan::floors`: its room's, or the one nearest its
    /// height when it has no room.
    floor: usize,
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
    /// The dungeon room it names, which key logic counts by. A check
    /// naming none needs all its dungeon's keys.
    dungeon_room: Option<u32>,
    /// Its storey: its face's, or the one nearest its height off the mesh.
    floor: usize,
}

/// A lock: one per door side, the pair joined into one, and the dungeon
/// rooms each side names.
#[derive(Serialize)]
struct Lock {
    name: String,
    at: [f32; 3],
    floor: usize,
    rooms: Vec<u32>,
}

pub(super) fn map(romfs: &Romfs, game: &Path, map: &str) -> Result<()> {
    println!("{}", serde_json::to_string(&plan(romfs, game, map)?)?);
    Ok(())
}

fn plan(romfs: &Romfs, game: &Path, map: &str) -> Result<Plan> {
    let bytes = navmesh_file(romfs, game, map)?.with_context(|| format!("{map} has no navmesh"))?;
    let mesh = NavMesh::parse(&bytes)?;
    let placed = warp::placements_in(romfs, map)?;
    let graph = Graph::build(&mesh, &placed, STEP_UP);
    let locked = locked_doors(&placed);
    let roots = split(&graph, &locked);
    let room = |at: [f32; 3]| graph.locate(at).map(|s| roots[s.face]);
    let named = warp::rooms_in(romfs, map)?;
    let marks: Vec<(usize, [f32; 3], u32)> = placed
        .iter()
        .filter_map(|p| Some((room(p.pose.at)?, p.pose.at, *named.get(&p.hash?)?)))
        .collect();
    let rooms: Vec<Option<u32>> = (0..mesh.faces.len())
        .map(|f| nearest(&marks, roots[f], graph.centers[f]))
        .collect();
    let heights: Vec<(u32, f32)> = rooms
        .iter()
        .zip(&graph.centers)
        .filter_map(|(r, c)| Some(((*r)?, c[1])))
        .collect();
    let floors = storeys(&heights);
    let floor_of = |r: Option<u32>, y: f32| {
        let by_room = r.and_then(|r| floors.iter().position(|f| f.rooms.contains(&r)));
        by_room.unwrap_or_else(|| closest(&floors, y))
    };
    let storey: Vec<usize> = (0..mesh.faces.len())
        .map(|f| floor_of(rooms[f], graph.centers[f][1]))
        .collect();
    let face_at = |at: [f32; 3]| graph.locate(at).map(|s| s.face);
    let on_floor =
        |at: [f32; 3]| face_at(at).map_or_else(|| closest(&floors, at[1]), |f| storey[f]);
    let faces = (0..mesh.faces.len())
        .map(|f| Face {
            ring: mesh.ring(f).iter().map(|e| e.a).collect(),
            room: roots[f],
            dungeon_room: rooms[f],
            floor: storey[f],
        })
        .collect();
    let mut locks: Vec<Lock> = Vec::new();
    for p in placed.iter().filter(|p| p.name.contains(LOCK)) {
        let side = p.hash.and_then(|h| named.get(&h).copied());
        match locks
            .iter_mut()
            .find(|l| flat(l.at, p.pose.at) < LOCK_REACH)
        {
            Some(l) => l.rooms.extend(side.filter(|r| !l.rooms.contains(r))),
            None => locks.push(Lock {
                name: p.name.clone(),
                at: p.pose.at,
                floor: on_floor(p.pose.at),
                rooms: side.into_iter().collect(),
            }),
        }
    }
    locks.iter_mut().for_each(|l| l.rooms.sort_unstable());
    let spots = placed
        .iter()
        .map(|p| Spot {
            hash: p.hash.map(|h| h.to_string()),
            name: p.name.clone(),
            label: p.label.clone(),
            at: p.pose.at,
            room: room(p.pose.at),
            dungeon_room: p.hash.and_then(|h| named.get(&h).copied()),
            floor: on_floor(p.pose.at),
        })
        .collect();
    let area = warp::area_of(romfs, map)?;
    let numbered: BTreeSet<u32> = named.values().copied().collect();
    let keys = key_rooms::rooms_of(&area, numbered);
    Ok(Plan {
        map: map.to_owned(),
        vertices: mesh.vertices.clone(),
        floors,
        faces,
        placed: spots,
        locks,
        keys,
    })
}

/// How far apart two rooms' mean heights are when they're on different
/// storeys. Stairs and ramps join a dungeon's heights with no gap, so
/// storeys come from rooms, not faces.
const STOREY: f32 = 5.0;

/// A storey with fewer faces than this joins the one nearest its height.
const LEAST: usize = 20;

/// The storeys of each face's `(room, height)`: rooms by mean height,
/// split where two neighbours differ by more than [`STOREY`].
fn storeys(faces: &[(u32, f32)]) -> Vec<Floor> {
    let mut by_room: BTreeMap<u32, (f32, usize)> = BTreeMap::new();
    for &(r, y) in faces {
        let e = by_room.entry(r).or_default();
        *e = (e.0 + y, e.1 + 1);
    }
    let mut rooms: Vec<(u32, f32, usize)> = by_room
        .into_iter()
        .map(|(r, (sum, n))| (r, sum / n as f32, n))
        .collect();
    rooms.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut groups: Vec<Vec<(u32, f32, usize)>> = Vec::new();
    for room in rooms {
        match groups.last_mut() {
            Some(g) if room.1 - g.last().map_or(room.1, |l| l.1) <= STOREY => g.push(room),
            _ => groups.push(vec![room]),
        }
    }
    let mean = |g: &[(u32, f32, usize)]| {
        let n: usize = g.iter().map(|r| r.2).sum();
        (
            g.iter().map(|r| r.1 * r.2 as f32).sum::<f32>() / n as f32,
            n,
        )
    };
    while let Some(i) = groups
        .iter()
        .position(|g| mean(g).1 < LEAST)
        .filter(|_| groups.len() > 1)
    {
        let small = groups.remove(i);
        let y = mean(&small).0;
        let j = (0..groups.len()).min_by(|&a, &b| {
            (mean(&groups[a]).0 - y)
                .abs()
                .total_cmp(&(mean(&groups[b]).0 - y).abs())
        });
        let into = &mut groups[j.unwrap_or(0)];
        into.extend(small);
        into.sort_by(|a, b| a.1.total_cmp(&b.1));
    }
    groups
        .iter()
        .map(|g| Floor {
            rooms: g.iter().map(|r| r.0).collect(),
            height: mean(g).0,
        })
        .collect()
}

/// The storey whose mean height is nearest `y`.
fn closest(floors: &[Floor], y: f32) -> usize {
    let off = |f: &Floor| (f.height - y).abs();
    (0..floors.len())
        .min_by(|&a, &b| off(&floors[a]).total_cmp(&off(&floors[b])))
        .unwrap_or(0)
}

/// The dungeon room of the mark nearest `at` in navmesh room `room`.
fn nearest(marks: &[(usize, [f32; 3], u32)], room: usize, at: [f32; 3]) -> Option<u32> {
    let far = |p: [f32; 3]| (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f32>();
    let here = marks.iter().filter(|m| m.0 == room);
    here.min_by(|a, b| far(a.1).total_cmp(&far(b.1)))
        .map(|m| m.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map::FOREST_TEMPLE;

    #[test]
    fn storeys_split_at_height_gaps_and_fold_in_small_ones() {
        let faces = |r: u32, y: f32, n: usize| std::iter::repeat_n((r, y), n);
        let all: Vec<(u32, f32)> = faces(1, 100.0, 30)
            .chain(faces(2, 103.0, 30))
            .chain(faces(3, 120.0, 30))
            .chain(faces(4, 60.0, 2))
            .collect();
        let floors = storeys(&all);
        let rooms: Vec<&[u32]> = floors.iter().map(|f| f.rooms.as_slice()).collect();
        assert_eq!(rooms, [&[4, 1, 2][..], &[3]]);
        assert_eq!(closest(&floors, 118.0), 1);
    }

    #[test]
    fn forest_temple_plan_has_its_locks_and_chests_on_floors() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let plan = plan(&romfs, Path::new("../../dump/oot.nup"), FOREST_TEMPLE).unwrap();
        let locks: Vec<&[u32]> = plan.locks.iter().map(|l| l.rooms.as_slice()).collect();
        assert_eq!(
            locks.len(),
            6,
            "5 small key locks and the boss's: {locks:?}"
        );
        assert!(
            locks.contains(&&[0, 1][..]) && locks.contains(&&[9, 10][..]),
            "{locks:?}"
        );
        assert!(plan.floors.windows(2).all(|w| w[0].height < w[1].height));
        assert!(plan.faces.iter().all(|f| f.floor < plan.floors.len()));
        let chests: Vec<&Spot> = plan
            .placed
            .iter()
            .filter(|p| p.name.starts_with("Obj_TBox_A_") && p.room.is_some())
            .collect();
        assert_eq!(chests.len(), 11);
        assert!(
            chests.iter().all(|c| c.dungeon_room.is_some()),
            "{:?}",
            chests.iter().map(|c| &c.hash).collect::<Vec<_>>()
        );
    }
}
