//! Each navmesh face's room: the room of the nearest placement the game
//! numbers, by walking distance, stopping at obstacles.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use super::Segment;
use crate::nav::graph::{Arc, Cross, Graph, Via, dist};
use crate::warp::Placement;

/// The widest level gap a room spreads across, in m: a short hop over a
/// crack or a ledge's lip, well inside any jump. For labelling only, never a
/// reach claim (`limits.toml` holds those).
const GAP: f32 = 2.0;

/// A distance the heap can order.
#[derive(Clone, Copy)]
struct Far(f32);
impl PartialEq for Far {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Far {}
impl PartialOrd for Far {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Far {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// The path an arc takes: face to face across a shared edge (a shutter in
/// the doorway sits on it), or a link's own ends.
pub(super) fn path_of(graph: &Graph, from: usize, arc: &Arc) -> Segment {
    match arc.cross {
        Cross::Edge(_) => (graph.centers[from], graph.centers[arc.to]),
        Cross::Link { from: a, to: b, .. } => (a, b),
    }
}

/// Each face's room and part.
pub(super) struct Labels {
    /// `None` for a face no named placement reaches.
    pub room: Vec<Option<u32>>,
    /// The faces joined with nothing in the way share a part: a chest
    /// behind a web is in another part of its room.
    pub part: Vec<usize>,
}

/// Each face's room. A room spreads across anything but a door or a path
/// `blocked` says an obstacle stands on, so rooms meet at their doors,
/// shutters and webs.
pub(super) fn label(
    graph: &Graph,
    placed: &[Placement],
    rooms: &HashMap<u64, u32>,
    first_floor: usize,
    blocked: impl Fn([f32; 3], [f32; 3]) -> bool,
) -> Labels {
    let n = graph.centers.len();
    // Both ways: a ledge whose only arc is a drop off it is still its room's.
    // `near` leaves out what an obstacle stands on, `all` keeps it.
    let (mut near, mut all): (Vec<Vec<usize>>, Vec<Vec<usize>>) =
        (vec![vec![]; n], vec![vec![]; n]);
    // One way, against the arc: who gets to each face with nothing in the way.
    let mut back: Vec<Vec<usize>> = vec![vec![]; n];
    let mut join = |a: usize, b: usize, open: bool| {
        all[a].push(b);
        all[b].push(a);
        if open {
            near[a].push(b);
            near[b].push(a);
            back[b].push(a);
        }
    };
    for (from, arcs) in graph.arcs.iter().enumerate() {
        for arc in arcs {
            if !matches!(
                arc.cross,
                Cross::Link {
                    via: Via::Door(_),
                    ..
                }
            ) {
                let (a, b) = path_of(graph, from, arc);
                // A crawl may join two rooms (the basement's has a room
                // change at each mouth), so a room crosses one only to floor
                // no placement reached first, like floor between obstacles.
                let crawl = matches!(
                    arc.cross,
                    Cross::Link {
                        via: Via::Crawl(_),
                        ..
                    }
                );
                join(from, arc.to, !crawl && !blocked(a, b));
            }
        }
    }
    for hop in graph.gaps(GAP) {
        join(hop.from, hop.to, !blocked(hop.leave, hop.land));
    }
    let mut best: Vec<Option<(f32, u32)>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    let seeds: Vec<([f32; 3], u32)> = (placed.iter())
        .filter_map(|p| Some((p.pose.at, *rooms.get(&p.hash?)?)))
        .collect();
    for (at, room) in doorway_seeds(&seeds) {
        // On the navmesh: a door's leaf stands where the collision floors
        // run on through the doorway, on whichever side.
        let Some(snap) = graph.locate_where(at, |f| f < first_floor) else {
            continue;
        };
        let d = dist(snap.at, graph.centers[snap.face]);
        if best[snap.face].is_none_or(|(b, _)| d < b) {
            best[snap.face] = Some((d, room));
            heap.push(Reverse((Far(d), snap.face, room)));
        }
    }
    spread(graph, &near, &mut best, heap, &vec![false; n]);
    // The floor between two obstacles (Deku hangs shutters in pairs) joins
    // the nearest room past them; a face pass one placed keeps its room.
    // Collision floor takes its room only from the first spread: one no room
    // reached without crossing an obstacle stays roomless, never the
    // nearest room through the shutter beside it.
    let kept: Vec<bool> = (best.iter().enumerate())
        .map(|(f, b)| b.is_some() || f >= first_floor)
        .collect();
    let heap = (0..n)
        .filter_map(|f| Some(Reverse((Far(0.0), f, best[f]?.1))))
        .collect();
    for b in best.iter_mut().flatten() {
        b.0 = 0.0;
    }
    spread(graph, &all, &mut best, heap, &kept);
    let mut room: Vec<Option<u32>> = best.into_iter().map(|b| b.map(|(_, r)| r)).collect();
    let home = homeward(&room, &back, first_floor);
    for (f, r) in room.iter_mut().enumerate().skip(first_floor) {
        if !home[f] {
            *r = None;
        }
    }
    Labels {
        room,
        part: parts(&near),
    }
}

/// Which faces lead Link back to their room's navmesh (the faces before
/// `first_floor`) through their own room's faces, by `back`'s arcs
/// reversed. Collision floor that doesn't (a corridor's roof, reached only
/// by dropping from the floor above) is in no room: standing on it puts
/// Link in none of the rooms round it.
fn homeward(room: &[Option<u32>], back: &[Vec<usize>], first_floor: usize) -> Vec<bool> {
    let mut home: Vec<bool> = (0..room.len())
        .map(|f| f < first_floor && room[f].is_some())
        .collect();
    let mut stack: Vec<usize> = (0..room.len()).filter(|&f| home[f]).collect();
    while let Some(f) = stack.pop() {
        for &from in &back[f] {
            if !home[from] && room[from] == room[f] {
                home[from] = true;
                stack.push(from);
            }
        }
    }
    home
}

/// How far past its doorway a door leaf seeds its room, in m: the game's
/// room change volumes stand about that far out from a shutter.
const DOORWAY_OUT: f32 = 1.0;

/// How near two placements of different rooms stand to be one door's
/// leaves, in m: a Deku shutter's are 0.4 to 0.9 m apart.
const LEAVES: f32 = 1.2;

/// `seeds` with each one on a doorway moved to its own side. Placements of
/// two rooms side by side are a door's leaves (a Deku shutter's carry the
/// room on each side), so each moves [`DOORWAY_OUT`] further along the line
/// from the other rooms' leaves to it. Side by side, not on one face: the
/// collision floors' small faces put a doorway's leaves on several.
fn doorway_seeds(seeds: &[([f32; 3], u32)]) -> Vec<([f32; 3], u32)> {
    (seeds.iter())
        .map(|&(at, room)| {
            let others: Vec<[f32; 3]> = (seeds.iter())
                .filter(|(o, r)| *r != room && dist(*o, at) <= LEAVES)
                .map(|(o, _)| *o)
                .collect();
            if others.is_empty() {
                return (at, room);
            }
            let n = others.len() as f32;
            let mid: [f32; 3] =
                std::array::from_fn(|k| others.iter().map(|o| o[k]).sum::<f32>() / n);
            let away = [at[0] - mid[0], 0.0, at[2] - mid[2]];
            let len = dist(away, [0.0; 3]).max(f32::EPSILON);
            (
                std::array::from_fn(|k| at[k] + away[k] / len * DOORWAY_OUT),
                room,
            )
        })
        .collect()
}

/// Each face's part: the faces `near` joins, numbered by their first face.
fn parts(near: &[Vec<usize>]) -> Vec<usize> {
    let mut part = vec![usize::MAX; near.len()];
    for start in 0..near.len() {
        if part[start] != usize::MAX {
            continue;
        }
        part[start] = start;
        let mut stack = vec![start];
        while let Some(f) = stack.pop() {
            for &to in &near[f] {
                if part[to] == usize::MAX {
                    part[to] = start;
                    stack.push(to);
                }
            }
        }
    }
    part
}

/// Spreads each room in `heap` over `near`, nearest first, never into a
/// face `kept` holds.
fn spread(
    graph: &Graph,
    near: &[Vec<usize>],
    best: &mut [Option<(f32, u32)>],
    mut heap: BinaryHeap<Reverse<(Far, usize, u32)>>,
    kept: &[bool],
) {
    while let Some(Reverse((Far(d), face, room))) = heap.pop() {
        if best[face].is_some_and(|(b, r)| b < d || (b == d && r != room)) {
            continue;
        }
        for &to in near[face].iter().filter(|&&to| !kept[to]) {
            let next = d + dist(graph.centers[face], graph.centers[to]);
            if best[to].is_none_or(|(b, _)| next < b) {
                best[to] = Some((next, room));
                heap.push(Reverse((Far(next), to, room)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph;
    use crate::warp::Pose;

    #[test]
    fn collision_floor_that_never_leads_back_to_its_room_is_in_none() {
        // Faces 0 and 1 are navmesh, 2 and 3 collision floor. 2 drops back
        // into 1; 3 is reached from 2 only, and leads nowhere.
        let room = vec![Some(1), Some(1), Some(1), Some(1)];
        let back = vec![vec![1], vec![0, 2], vec![1], vec![2]];
        assert_eq!(homeward(&room, &back, 2), vec![true, true, true, false]);
        // A face of another room on the way home doesn't count.
        let room = vec![Some(1), Some(2), Some(1), Some(1)];
        assert_eq!(homeward(&room, &back, 2), vec![true, true, false, false]);
    }

    fn tag(hash: u64, at: [f32; 3]) -> Placement {
        Placement {
            name: "Obj_FakeTag".into(),
            hash: Some(hash),
            label: None,
            pose: Pose {
                at,
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        }
    }

    /// Four floors in a row, x 0 to 8, a room tagged at each end, and one
    /// floor far off that nothing tags.
    fn row() -> (ounce_formats::NavMesh, Vec<Placement>, HashMap<u64, u32>) {
        let m = mesh(&[
            quad(0.0, 0.0, 2.0, 2.0, 0.0),
            quad(2.0, 0.0, 4.0, 2.0, 0.0),
            quad(4.0, 0.0, 6.0, 2.0, 0.0),
            quad(6.0, 0.0, 8.0, 2.0, 0.0),
            quad(50.0, 0.0, 52.0, 2.0, 0.0),
        ]);
        let placed = vec![tag(1, [1.0, 0.0, 1.0]), tag(2, [7.0, 0.0, 1.0])];
        (m, placed, HashMap::from([(1, 1), (2, 2)]))
    }

    #[test]
    fn each_floor_takes_the_room_of_the_nearest_tag_by_walking() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let labels = label(&g, &placed, &rooms, usize::MAX, |_, _| false).room;
        assert_eq!(labels, [Some(1), Some(1), Some(2), Some(2), None]);
    }

    /// An obstacle at x 2 holds room 1 back, so the floor past it is room 2's.
    #[test]
    fn a_room_stops_at_an_obstacle() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let at_two = |a: [f32; 3], b: [f32; 3]| (a[0] - 2.0) * (b[0] - 2.0) < 0.0;
        let labels = label(&g, &placed, &rooms, usize::MAX, at_two).room;
        assert_eq!(labels, [Some(1), Some(2), Some(2), Some(2), None]);
    }

    /// The floor between two obstacles joins the nearest room past them.
    #[test]
    fn the_floor_between_two_obstacles_joins_a_room() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let between =
            |a: [f32; 3], b: [f32; 3]| [2.0, 6.0].iter().any(|x| (a[0] - x) * (b[0] - x) < 0.0);
        let labels = label(&g, &placed, &rooms, usize::MAX, between).room;
        assert!(labels[1].is_some() && labels[2].is_some(), "{labels:?}");
    }
}
