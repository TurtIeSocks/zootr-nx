//! A route over the graph: A* over face centers, then pulled tight through
//! the edges it crosses, so a room becomes two or three waypoints. Links
//! (a door, a drop, a step) stay as waypoints.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt;
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde::Serialize;

use super::Level;
use super::graph::{Arc, Cross, Graph, Via, dist};
use crate::dev::Dev;
use crate::live::Memory;

/// A spot to walk to, and how it is reached from the one before.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Waypoint {
    pub at: [f32; 3],
    pub via: Via,
}

/// The waypoints from where Link is to the goal. The first is Link's own
/// spot, the last the goal on the mesh: a goal off it, like a chest in its
/// hole, is the nearest spot of the mesh.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Route {
    pub waypoints: Vec<Waypoint>,
    /// Straight lines from waypoint to waypoint, in m.
    pub length: f32,
}

/// A face waiting to be expanded, the lowest score first.
struct Open {
    score: f32,
    cost: f32,
    face: usize,
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other.score.total_cmp(&self.score)
    }
}

/// The arcs from face `from` to face `to`, each with the face it leaves, or
/// `None` when there is no way.
fn search(graph: &Graph, from: usize, to: usize) -> Option<Vec<(usize, Arc)>> {
    let heuristic = |f: usize| dist(graph.centers[f], graph.centers[to]);
    let mut best = vec![f32::INFINITY; graph.arcs.len()];
    let mut came: Vec<Option<(usize, Arc)>> = vec![None; graph.arcs.len()];
    let mut open = BinaryHeap::from([Open {
        score: heuristic(from),
        cost: 0.0,
        face: from,
    }]);
    best[from] = 0.0;
    while let Some(Open { cost, face, .. }) = open.pop() {
        if face == to {
            let mut path = vec![];
            let mut at = to;
            while let Some((prev, arc)) = came[at] {
                path.push((prev, arc));
                at = prev;
            }
            path.reverse();
            return Some(path);
        }
        if cost > best[face] {
            continue;
        }
        for arc in &graph.arcs[face] {
            let next = cost + arc.cost;
            if next < best[arc.to] {
                best[arc.to] = next;
                came[arc.to] = Some((face, *arc));
                open.push(Open {
                    score: next + heuristic(arc.to),
                    cost: next,
                    face: arc.to,
                });
            }
        }
    }
    None
}

/// Twice the signed area of a, b, c in x and z: positive when c is
/// counterclockwise of the ray a to b, which is to its left.
fn area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    (b[0] - a[0]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[0] - a[0])
}

fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    (a[0] - b[0]).hypot(a[2] - b[2]) < 1e-4
}

/// The edge `a`..`b` crossed from a face centered at `from` to one centered
/// at `to`, as its left end then its right.
fn portal(from: [f32; 3], to: [f32; 3], a: [f32; 3], b: [f32; 3]) -> [[f32; 3]; 2] {
    let ahead = [to[0] - from[0], 0.0, to[2] - from[2]];
    let side = |p: [f32; 3]| area([0.0; 3], ahead, [p[0] - from[0], 0.0, p[2] - from[2]]);
    if side(a) >= side(b) { [a, b] } else { [b, a] }
}

/// The corners of the tightest path from `start` to `end` through
/// `portals`, then `end`. The funnel algorithm: the apex holds while the
/// left and right edges of the funnel narrow, and moves to the end of one
/// edge when the other crosses it.
fn funnel(start: [f32; 3], portals: &[[[f32; 3]; 2]], end: [f32; 3]) -> Vec<[f32; 3]> {
    let all: Vec<[[f32; 3]; 2]> = [[start; 2]]
        .into_iter()
        .chain(portals.iter().copied())
        .chain([[end; 2]])
        .collect();
    let mut corners = vec![];
    let (mut apex, mut left, mut right) = (start, start, start);
    let (mut left_at, mut right_at) = (0, 0);
    let mut i = 1;
    while i < all.len() {
        let [l, r] = all[i];
        if area(apex, right, r) >= 0.0 {
            if same(apex, right) || area(apex, left, r) < 0.0 {
                (right, right_at) = (r, i);
            } else {
                corners.push(left);
                (apex, i) = (left, left_at + 1);
                (left, right, right_at) = (apex, apex, left_at);
                continue;
            }
        }
        if area(apex, left, l) <= 0.0 {
            if same(apex, left) || area(apex, right, l) > 0.0 {
                (left, left_at) = (l, i);
            } else {
                corners.push(right);
                (apex, i) = (right, right_at + 1);
                (left, right, left_at) = (apex, apex, right_at);
                continue;
            }
        }
        i += 1;
    }
    corners.push(end);
    corners
}

/// Add a walked-to spot, unless Link is already there.
fn walk(out: &mut Vec<Waypoint>, at: [f32; 3]) {
    if out.last().is_none_or(|w| dist(w.at, at) >= 0.05) {
        out.push(Waypoint { at, via: Via::Walk });
    }
}

/// The waypoints of `path` from `start`, Link's own spot first.
fn pull(
    graph: &Graph,
    from: [f32; 3],
    start: [f32; 3],
    path: &[(usize, Arc)],
    goal: [f32; 3],
) -> Vec<Waypoint> {
    let mut out = vec![Waypoint {
        at: from,
        via: Via::Walk,
    }];
    let (mut run_from, mut portals) = (start, vec![]);
    for &(face, arc) in path {
        match arc.cross {
            Cross::Edge([a, b]) => {
                portals.push(portal(graph.centers[face], graph.centers[arc.to], a, b));
            }
            Cross::Link { via, from, to } => {
                funnel(run_from, &portals, from)
                    .into_iter()
                    .for_each(|at| walk(&mut out, at));
                out.push(Waypoint { at: to, via });
                (run_from, portals) = (to, vec![]);
            }
        }
    }
    funnel(run_from, &portals, goal)
        .into_iter()
        .for_each(|at| walk(&mut out, at));
    out
}

/// The route from `from` to `to`. `Err` says `no route` and why when a
/// spot is off the mesh or the graph has no way between them.
pub fn plan(graph: &Graph, from: [f32; 3], to: [f32; 3]) -> Result<Route> {
    // A map with no mesh of its own (Hyrule Field's is in tiles) is walked
    // in a straight line.
    if graph.mesh.faces.is_empty() {
        let waypoints = [from, to].map(|at| Waypoint { at, via: Via::Walk });
        return Ok(Route {
            length: dist(from, to),
            waypoints: waypoints.into(),
        });
    }
    let start = graph
        .locate(from)
        .context("no route: no mesh within 1.5 m of Link")?;
    let goal = graph
        .locate(to)
        .context("no route: no mesh within 1.5 m of the goal")?;
    let path = search(graph, start.face, goal.face).context("no route")?;
    let waypoints = super::clear::clear(graph, pull(graph, from, start.at, &path, goal.at));
    let length = waypoints.windows(2).map(|w| dist(w[0].at, w[1].at)).sum();
    Ok(Route { waypoints, length })
}

/// A route to a placement of the map.
#[derive(Debug, Serialize)]
pub struct Plan {
    pub map: String,
    pub name: String,
    pub hash: u64,
    pub goal: [f32; 3],
    #[serde(flatten)]
    pub route: Route,
}

/// The route from `from` to the placement with `hash` in `level`.
pub fn plan_to(level: &Level, from: [f32; 3], hash: u64) -> Result<Plan> {
    let (map, placed) = (&level.map, &level.placed);
    let goal = (placed.iter().find(|p| p.hash == Some(hash)))
        .with_context(|| format!("{hash} isn't placed in {map}"))?;
    let graph = level.graph();
    Ok(Plan {
        map: map.to_owned(),
        name: goal.name.clone(),
        hash,
        goal: goal.pose.at,
        route: plan(&graph, from, goal.pose.at)?,
    })
}

/// The route from where Link is in the running game to a placement of his
/// map, without moving him.
pub fn read<M: Memory>(dev: &Dev<M>, romfs: &Romfs, game: &Path, hash: u64) -> Result<Plan> {
    let level = Level::read(dev, romfs, game)?;
    plan_to(&level, dev.link()?, hash)
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let xyz = |p: [f32; 3]| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]);
        writeln!(
            f,
            "Route in {} to {} {} at {}: {} waypoints, {:.1} m",
            self.map,
            self.name,
            self.hash,
            xyz(self.goal),
            self.route.waypoints.len(),
            self.route.length
        )?;
        for (i, w) in self.route.waypoints.iter().enumerate() {
            let last = i + 1 == self.route.waypoints.len();
            write!(f, "{:>2}  {}  {}", i + 1, xyz(w.at), w.via)?;
            if !last {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ounce_formats::NavMesh;

    use super::*;
    use crate::checks::flat;
    use crate::known::{actor, map};
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph::STEP_UP;
    use crate::warp::{Placement, Pose};

    fn route(m: &NavMesh, placed: &[Placement], from: [f32; 3], to: [f32; 3]) -> Route {
        plan(&Graph::build(m, placed, STEP_UP), from, to).unwrap()
    }

    fn spots(r: &Route) -> Vec<[f32; 2]> {
        r.waypoints.iter().map(|w| [w.at[0], w.at[2]]).collect()
    }

    #[test]
    fn a_map_with_no_mesh_is_a_straight_line() {
        let r = route(&NavMesh::default(), &[], [0.0, 0.0, 0.0], [3.0, 0.0, 4.0]);
        assert_eq!(spots(&r), [[0.0, 0.0], [3.0, 4.0]]);
        assert!((r.length - 5.0).abs() < 1e-4, "{}", r.length);
    }

    #[test]
    fn a_straight_corridor_of_many_faces_is_start_and_goal() {
        let rings: Vec<_> = (0..8)
            .map(|i| quad(i as f32 * 2.0, 0.0, i as f32 * 2.0 + 2.0, 2.0, 0.0))
            .collect();
        let r = route(&mesh(&rings), &[], [0.5, 0.0, 1.0], [15.5, 0.0, 1.0]);
        assert_eq!(spots(&r), [[0.5, 1.0], [15.5, 1.0]]);
        assert!((r.length - 15.0).abs() < 1e-4, "{}", r.length);
    }

    /// An L: an arm along x from 0 to 10, and one up z from 2 to 10 at its
    /// east end, mirrored in x when `flip`.
    fn l_room(flip: bool) -> NavMesh {
        let mut rings: Vec<_> = (0..5)
            .map(|i| quad(i as f32 * 2.0, 0.0, i as f32 * 2.0 + 2.0, 2.0, 0.0))
            .collect();
        rings.extend(
            (0..4).map(|i| quad(8.0, 2.0 + i as f32 * 2.0, 10.0, 4.0 + i as f32 * 2.0, 0.0)),
        );
        if flip {
            for p in rings.iter_mut().flatten() {
                p[0] = -p[0];
            }
            for ring in &mut rings {
                ring.reverse();
            }
        }
        mesh(&rings)
    }

    #[test]
    fn an_l_shaped_room_is_start_one_corner_and_goal() {
        let r = route(&l_room(false), &[], [0.5, 0.0, 1.0], [9.0, 0.0, 9.5]);
        assert_eq!(spots(&r), [[0.5, 1.0], [8.0, 2.0], [9.0, 9.5]]);
        // The same room turning the other way, and walked back.
        let m = l_room(true);
        let r = route(&m, &[], [-0.5, 0.0, 1.0], [-9.0, 0.0, 9.5]);
        assert_eq!(spots(&r), [[-0.5, 1.0], [-8.0, 2.0], [-9.0, 9.5]]);
        let r = route(&m, &[], [-9.0, 0.0, 9.5], [-0.5, 0.0, 1.0]);
        assert_eq!(spots(&r), [[-9.0, 9.5], [-8.0, 2.0], [-0.5, 1.0]]);
    }

    #[test]
    fn a_door_stays_a_waypoint_and_a_drop_too() {
        needs_names!();
        let door = Placement {
            name: actor::OBJ_KOKIRI_DOOR_A_02.to_string(),
            hash: Some(7),
            label: None,
            pose: Pose {
                at: [1.0, 0.0, 2.0],
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        };
        // Through the door, then down a 2 m ledge to the east.
        let m = mesh(&[
            quad(0.0, -3.0, 2.0, 0.5, 0.0),
            quad(0.0, 3.5, 2.0, 7.0, 0.0),
            quad(2.3, 3.5, 5.0, 7.0, -2.0),
        ]);
        let r = route(&m, &[door], [1.0, 0.0, -2.0], [4.0, -2.0, 5.0]);
        let vias: Vec<_> = r.waypoints.iter().map(|w| w.via).collect();
        assert_eq!(vias.first(), Some(&Via::Walk));
        let door_at = vias.iter().position(|v| *v == Via::Door(Some(7)));
        let drop_at = vias.iter().position(|v| *v == Via::Drop);
        assert!(door_at.is_some() && drop_at > door_at, "{vias:?}");
        assert_eq!(r.waypoints.last().unwrap().at, [4.0, -2.0, 5.0]);
        // Back up the ledge there is no way.
        let g = Graph::build(&m, &[], STEP_UP);
        let err = plan(&g, [4.0, -2.0, 5.0], [1.0, 0.0, 5.0]).unwrap_err();
        assert_eq!(err.to_string(), "no route");
    }

    #[test]
    fn a_start_or_goal_off_the_mesh_snaps_or_is_refused() {
        let m = mesh(&[quad(0.0, 0.0, 4.0, 2.0, 0.0)]);
        let r = route(&m, &[], [1.0, 0.0, 1.0], [5.0, 0.0, 1.0]);
        assert_eq!(spots(&r), [[1.0, 1.0], [4.0, 1.0]]);
        let g = Graph::build(&m, &[], STEP_UP);
        let err = plan(&g, [1.0, 0.0, 1.0], [7.0, 0.0, 1.0]).unwrap_err();
        assert!(err.to_string().starts_with("no route: "), "{err}");
    }

    /// Mido's house chest in Kokiri Forest. The runs D start stands on the
    /// hill by Link's house, which the mesh leaves only by vines (climbing
    /// is not a link yet), so there is no route from it. From the forest
    /// floor the route goes in at Mido's door, whose link the graph has both
    /// ways.
    #[test]
    fn a_route_from_the_forest_floor_goes_in_at_mido_s_door() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new("../../dump/oot.nup");
        let level = Level::of(&romfs, game, map::KOKIRI_FOREST_CHILD).unwrap();
        let (chest, door) = (10831981390482426294, 5297260056723171008);
        let graph = level.graph();
        let door_links = graph.arcs.iter().flatten().filter(
            |a| matches!(a.cross, Cross::Link { via: Via::Door(Some(h)), .. } if h == door),
        );
        assert_eq!(door_links.count(), 2);

        let plan = plan_to(&level, [-31.0, 110.0, -20.0], chest).unwrap();
        let at = (level.placed)
            .iter()
            .find(|p| p.hash == Some(door))
            .unwrap()
            .pose
            .at;
        let spots: Vec<_> = plan.route.waypoints.iter().map(|w| w.at).collect();
        assert!(
            spots[0][2] > at[2] && spots.last().unwrap()[2] < at[2],
            "{plan}"
        );
        let nearest = spots.iter().map(|&s| flat(s, at)).fold(f32::MAX, f32::min);
        assert!(nearest < 1.5, "{nearest} from the door in {plan}");
        assert!(flat(*spots.last().unwrap(), [-50.3, 112.3, -42.0]) < 1.5);

        let err = plan_to(&level, [-32.0, 121.2, -65.2], chest).unwrap_err();
        assert_eq!(err.to_string(), "no route");
    }
}
