//! Each navmesh face's room: the room of the nearest placement the game
//! numbers, by walking distance, stopping at obstacles.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

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
pub(super) fn path_of(graph: &Graph, from: usize, arc: &Arc) -> ([f32; 3], [f32; 3]) {
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
    blocked: impl Fn([f32; 3], [f32; 3]) -> bool,
) -> Labels {
    let n = graph.centers.len();
    // Both ways: a ledge whose only arc is a drop off it is still its room's.
    // `near` leaves out what an obstacle stands on, `all` keeps it.
    let (mut near, mut all): (Vec<Vec<usize>>, Vec<Vec<usize>>) =
        (vec![vec![]; n], vec![vec![]; n]);
    let mut join = |a: usize, b: usize, open: bool| {
        for list in [&mut all].into_iter().chain(open.then_some(&mut near)) {
            list[a].push(b);
            list[b].push(a);
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
                join(from, arc.to, !blocked(a, b));
            }
        }
    }
    for (a, b, from, to) in graph.gaps(GAP) {
        join(a, b, !blocked(from, to));
    }
    let mut best: Vec<Option<(f32, u32)>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    for p in placed {
        let Some(&room) = p.hash.and_then(|h| rooms.get(&h)) else {
            continue;
        };
        let Some(snap) = graph.locate(p.pose.at) else {
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
    let kept: Vec<bool> = best.iter().map(Option::is_some).collect();
    let heap = (0..n)
        .filter_map(|f| Some(Reverse((Far(0.0), f, best[f]?.1))))
        .collect();
    for b in best.iter_mut().flatten() {
        b.0 = 0.0;
    }
    spread(graph, &all, &mut best, heap, &kept);
    Labels {
        room: best.into_iter().map(|b| b.map(|(_, r)| r)).collect(),
        part: parts(&near),
    }
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
        let labels = label(&g, &placed, &rooms, |_, _| false).room;
        assert_eq!(labels, [Some(1), Some(1), Some(2), Some(2), None]);
    }

    /// An obstacle at x 2 holds room 1 back, so the floor past it is room 2's.
    #[test]
    fn a_room_stops_at_an_obstacle() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let at_two = |a: [f32; 3], b: [f32; 3]| (a[0] - 2.0) * (b[0] - 2.0) < 0.0;
        let labels = label(&g, &placed, &rooms, at_two).room;
        assert_eq!(labels, [Some(1), Some(2), Some(2), Some(2), None]);
    }

    /// The floor between two obstacles joins the nearest room past them.
    #[test]
    fn the_floor_between_two_obstacles_joins_a_room() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let between =
            |a: [f32; 3], b: [f32; 3]| [2.0, 6.0].iter().any(|x| (a[0] - x) * (b[0] - x) < 0.0);
        let labels = label(&g, &placed, &rooms, between).room;
        assert!(labels[1].is_some() && labels[2].is_some(), "{labels:?}");
    }
}
