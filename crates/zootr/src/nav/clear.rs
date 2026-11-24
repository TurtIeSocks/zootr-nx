//! Routes that keep clear of placed solids. The navmesh doesn't know a
//! prop that cuts it at run time (a chest, a pot, a sign, a bush), so a
//! leg pulled tight across the mesh can run straight through one. A
//! walked leg that crosses a solid's footprint, grown by the mesh's
//! erosion (how far Link's middle keeps from a wall), goes round it
//! instead: by the shortest way that stays on the mesh among the grown
//! footprints' corners and the mesh's own.

use super::graph::{Graph, Via, dist};
use super::route::Waypoint;
use super::shape::inside;

/// How much wider than Link's girth a way round a solid passes its
/// corners, in m.
const MARGIN: f32 = 0.1;
/// A solid whose top is at most this above the floor is walked over, in
/// m. A placeholder like `step_up()`, until measured live.
const STEP_OVER: f32 = 0.2;
/// How tall Link is, in m: a solid wholly higher above the floor than
/// this is walked under. A placeholder until measured live.
const TALL: f32 = 2.0;
/// How far from a leg's own bounds a solid may be and still be gone round
/// with the ones in its way, in m.
const NEAR: f32 = 3.0;
/// How far apart the spots a way round checks for mesh under are, in m.
const SAMPLE: f32 = 0.25;
/// How far from the leg's height the floor under a way round may be, in m.
const LEDGE: f32 = 1.0;
/// A leg that runs this far or less inside a footprint only touches it.
const TOUCH: f32 = 1e-3;

fn flat(p: [f32; 3]) -> [f32; 2] {
    [p[0], p[2]]
}

/// The outward normal of the edge `a`..`b` of a counterclockwise ring.
fn normal(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dz).max(1e-6);
    [dz / length, -dx / length]
}

/// The convex counterclockwise `ring` grown outward by `by`. A ring of one
/// or two points (a footprint seen edge on) grows to the box round them.
pub fn grown(ring: &[[f32; 2]], by: f32) -> Vec<[f32; 2]> {
    match ring {
        [] => vec![],
        [p] => grown(&[[p[0] - 1e-3, p[1]], [p[0] + 1e-3, p[1]]], by),
        [a, b] => {
            let n = normal(*a, *b);
            let ahead = [-n[1] * by, n[0] * by];
            let at = |p: [f32; 2], along: f32, side: f32| {
                [
                    p[0] + ahead[0] * along + n[0] * by * side,
                    p[1] + ahead[1] * along + n[1] * by * side,
                ]
            };
            vec![
                at(*a, -1.0, 1.0),
                at(*b, 1.0, 1.0),
                at(*b, 1.0, -1.0),
                at(*a, -1.0, -1.0),
            ]
        }
        _ => {
            let n = ring.len();
            (0..n)
                .map(|i| {
                    let (before, here, after) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
                    let (n1, n2) = (normal(before, here), normal(here, after));
                    // The miter: out along both edges' normals by `by`,
                    // held to a few times `by` at a sharp corner.
                    let k = by / (1.0 + n1[0] * n2[0] + n1[1] * n2[1]).max(0.1);
                    [here[0] + (n1[0] + n2[0]) * k, here[1] + (n1[1] + n2[1]) * k]
                })
                .collect()
        }
    }
}

/// Whether the leg `p`..`q` runs through the inside of the convex
/// counterclockwise `ring`, more than touching it.
pub fn crosses(ring: &[[f32; 2]], p: [f32; 2], q: [f32; 2]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let d = [q[0] - p[0], q[1] - p[1]];
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let n = normal(a, b);
        let side = n[0] * (p[0] - a[0]) + n[1] * (p[1] - a[1]);
        let toward = n[0] * d[0] + n[1] * d[1];
        if toward.abs() < 1e-9 {
            if side >= 0.0 {
                return false;
            }
            continue;
        }
        let t = -side / toward;
        if toward < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 >= t1 {
            return false;
        }
    }
    (t1 - t0) * d[0].hypot(d[1]) > TOUCH
}

/// Whether the mesh lies under the leg `a`..`b` all along, near the
/// height between its ends.
fn on_mesh(graph: &Graph, a: [f32; 3], b: [f32; 3]) -> bool {
    let steps = (dist(a, b) / SAMPLE).ceil().max(1.0) as usize;
    (1..steps).all(|i| {
        let t = i as f32 / steps as f32;
        let p: [f32; 3] = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
        graph.floor(p, LEDGE).is_some()
    })
}

/// The route's waypoints, each walked leg that crosses a solid gone round
/// it.
pub fn clear(graph: &Graph, route: Vec<Waypoint>) -> Vec<Waypoint> {
    if graph.solids.is_empty() || route.is_empty() {
        return route;
    }
    let mut out = vec![route[0]];
    for leg in route.windows(2) {
        if leg[1].via == Via::Walk {
            let by = around(graph, leg[0].at, leg[1].at);
            out.extend(by.into_iter().map(|at| Waypoint { at, via: Via::Walk }));
        }
        out.push(leg[1]);
    }
    out
}

/// The corners to walk by from `a` to `b` round the solids in the way:
/// none when nothing is in the way, or when no way round stays on the
/// mesh. A solid either end stands in is no obstacle to that leg, so a
/// walk to a holder ends at it.
fn around(graph: &Graph, a: [f32; 3], b: [f32; 3]) -> Vec<[f32; 3]> {
    let girth = graph.mesh.erosion;
    let (fa, fb) = (flat(a), flat(b));
    let (floor, top) = (a[1].min(b[1]), a[1].max(b[1]));
    let (lo, hi) = (
        [fa[0].min(fb[0]) - NEAR, fa[1].min(fb[1]) - NEAR],
        [fa[0].max(fb[0]) + NEAR, fa[1].max(fb[1]) + NEAR],
    );
    // A ring's bounds overlapping the leg's: a long solid across a short
    // leg has no corner near it.
    let near = |ring: &[[f32; 2]]| {
        (0..2).all(|k| {
            let (min, max) = (ring.iter())
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), p| {
                    (min.min(p[k]), max.max(p[k]))
                });
            min <= hi[k] && max >= lo[k]
        })
    };
    let walls: Vec<Vec<[f32; 2]>> = (graph.solids.iter())
        .filter(|s| s.high > floor + STEP_OVER && s.low < top + TALL && near(&s.ring))
        .map(|s| grown(&s.ring, girth))
        .filter(|g| !inside(g, fa) && !inside(g, fb))
        .collect();
    if !walls.iter().any(|g| crosses(g, fa, fb)) {
        return vec![];
    }
    // The leg's height across from (x, z).
    let span = (b[0] - a[0]).hypot(b[2] - a[2]).max(1e-6);
    let height = |c: [f32; 2]| {
        let along = (c[0] - a[0]) * (b[0] - a[0]) + (c[1] - a[2]) * (b[2] - a[2]);
        a[1] + (b[1] - a[1]) * (along / (span * span)).clamp(0.0, 1.0)
    };
    let free = |c: [f32; 2]| !walls.iter().any(|g| inside(g, c));
    // The spots a way round may pass: each wall's corners, a little out,
    // on the floor under them, and the mesh's own corners nearby, for a
    // way that hugs a wall.
    let mut spots = vec![a, b];
    for c in walls
        .iter()
        .flat_map(|g| grown(g, MARGIN))
        .filter(|&c| free(c))
    {
        if let Some(f) = graph.floor([c[0], height(c), c[1]], LEDGE) {
            spots.push([c[0], f.height, c[1]]);
        }
    }
    let middle = [(a[0] + b[0]) / 2.0, (a[2] + b[2]) / 2.0];
    for c in graph.border_corners(middle[0], middle[1], span / 2.0 + NEAR) {
        if (c[1] - height(flat(c))).abs() <= LEDGE && free(flat(c)) {
            spots.push(c);
        }
    }
    let open = |i: usize, j: usize| {
        let (p, q) = (spots[i], spots[j]);
        !walls.iter().any(|g| crosses(g, flat(p), flat(q))) && on_mesh(graph, p, q)
    };
    // Dijkstra over the spots, from `a` (0) to `b` (1).
    let n = spots.len();
    let mut best = vec![f32::INFINITY; n];
    let mut came = vec![usize::MAX; n];
    let mut done = vec![false; n];
    best[0] = 0.0;
    while let Some(i) = (0..n)
        .filter(|&i| !done[i] && best[i].is_finite())
        .min_by(|&i, &j| best[i].total_cmp(&best[j]))
    {
        done[i] = true;
        if i == 1 {
            break;
        }
        for j in (0..n).filter(|&j| !done[j]) {
            let cost = best[i] + dist(spots[i], spots[j]);
            if cost < best[j] && open(i, j) {
                (best[j], came[j]) = (cost, i);
            }
        }
    }
    if !best[1].is_finite() {
        return vec![];
    }
    let mut by = vec![];
    let mut at = came[1];
    while at != 0 {
        by.push(spots[at]);
        at = came[at];
    }
    by.reverse();
    by
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map;
    use crate::nav::Level;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph::step_up;
    use crate::nav::route::{Route, plan};
    use crate::nav::shape::Solid;

    fn crate_at(x0: f32, z0: f32, x1: f32, z1: f32, low: f32, high: f32) -> Solid {
        Solid {
            name: "FakeCrate".into(),
            hash: Some(1),
            ring: vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]],
            low,
            high,
        }
    }

    /// The route over a 10 by 4 m floor with `solids` on it.
    fn route(solids: &[Solid], from: [f32; 3], to: [f32; 3]) -> Route {
        let floor = mesh(&[quad(0.0, 0.0, 10.0, 4.0, 0.0)]);
        let graph = Graph::build(&floor, &[], step_up()).with_solids(solids);
        plan(&graph, from, to).unwrap()
    }

    /// Whether no leg of `route` comes nearer than `by` to `solid`.
    fn clear_of(route: &Route, solid: &Solid, by: f32) -> bool {
        let wall = grown(&solid.ring, by);
        (route.waypoints.windows(2)).all(|w| !crosses(&wall, flat(w[0].at), flat(w[1].at)))
    }

    #[test]
    fn a_ring_grows_out_by_its_edges_normals() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let g = grown(&square, 0.5);
        assert_eq!(g, [[-0.5, -0.5], [1.5, -0.5], [1.5, 1.5], [-0.5, 1.5]]);
        // Seen edge on, a footprint is a line, and grows to a box.
        let g = grown(&[[0.0, 0.0], [2.0, 0.0]], 0.5);
        assert!(inside(&g, [1.0, 0.4]) && inside(&g, [-0.4, -0.4]));
        assert!(!inside(&g, [1.0, 0.6]) && !inside(&g, [2.6, 0.0]));
    }

    #[test]
    fn a_leg_crosses_a_ring_only_through_its_inside() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert!(crosses(&square, [-1.0, 0.5], [2.0, 0.5]));
        assert!(crosses(&square, [0.5, 0.5], [3.0, 3.0]));
        // Along an edge, past a corner, or short of it, it doesn't.
        assert!(!crosses(&square, [-1.0, 0.0], [2.0, 0.0]));
        assert!(!crosses(&square, [-1.0, 1.0], [1.0, 3.0]));
        assert!(!crosses(&square, [-2.0, 0.5], [-0.5, 0.5]));
    }

    #[test]
    fn a_walk_past_a_solid_goes_round_it_on_the_mesh() {
        let crate_ = crate_at(4.5, 1.5, 5.5, 2.5, 0.0, 1.0);
        let (from, to) = ([1.0, 0.0, 2.0], [9.0, 0.0, 2.0]);
        assert_eq!(route(&[], from, to).waypoints.len(), 2);
        let r = route(std::slice::from_ref(&crate_), from, to);
        assert!(r.waypoints.len() > 2, "{r:?}");
        assert!(clear_of(&r, &crate_, 0.39), "{r:?}");
        assert!(r.waypoints.iter().all(|w| w.via == Via::Walk));
        assert_eq!(
            (r.waypoints[0].at, r.waypoints.last().unwrap().at),
            (from, to)
        );
        assert!(r.length > 8.0 && r.length < 9.0, "{}", r.length);
    }

    #[test]
    fn a_walk_to_a_solid_ends_at_it() {
        let crate_ = crate_at(4.5, 1.5, 5.5, 2.5, 0.0, 1.0);
        let r = route(&[crate_], [1.0, 0.0, 2.0], [5.0, 0.0, 2.0]);
        assert_eq!(r.waypoints.len(), 2, "{r:?}");
    }

    #[test]
    fn a_solid_low_enough_to_step_over_or_high_enough_to_pass_under_is_walked_straight() {
        let (from, to) = ([1.0, 0.0, 2.0], [9.0, 0.0, 2.0]);
        let low = crate_at(4.5, 1.5, 5.5, 2.5, 0.0, 0.1);
        assert_eq!(route(&[low], from, to).waypoints.len(), 2);
        let high = crate_at(4.5, 1.5, 5.5, 2.5, 2.5, 3.0);
        assert_eq!(route(&[high], from, to).waypoints.len(), 2);
    }

    #[test]
    fn a_long_solid_across_a_short_walk_is_gone_round() {
        // A fence 13 m long across a 2 m walk: no corner of it lies near
        // the walk, but it's in the way all the same.
        let fence = crate_at(4.8, 3.5, 5.2, 16.5, 0.0, 1.0);
        let floor = mesh(&[quad(0.0, 0.0, 10.0, 20.0, 0.0)]);
        let graph = Graph::build(&floor, &[], step_up()).with_solids(std::slice::from_ref(&fence));
        let r = plan(&graph, [4.0, 0.0, 10.0], [6.0, 0.0, 10.0]).unwrap();
        assert!(r.waypoints.len() > 2, "{r:?}");
        assert!(clear_of(&r, &fence, 0.39), "{r:?}");
    }

    #[test]
    fn a_solid_from_wall_to_wall_leaves_the_walk_as_it_was() {
        let (from, to) = ([1.0, 0.0, 2.0], [9.0, 0.0, 2.0]);
        let across = crate_at(4.5, -1.0, 5.5, 5.0, 0.0, 1.0);
        assert_eq!(route(&[across], from, to).waypoints.len(), 2);
    }

    /// A signboard on the forest floor in Kokiri Forest: a
    /// plank a metre wide and a hand thick, its pack cutting the mesh at
    /// run time. A walk across it either way goes round it, Link's girth
    /// from its surface, where the mesh alone walks straight through.
    #[test]
    fn a_route_across_the_forest_keeps_clear_of_a_sign() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = std::path::Path::new(crate::game::DEV_DUMP);
        let level = Level::of(&romfs, game, map::KOKIRI_FOREST_CHILD).unwrap();
        let sign = (level.solids.iter())
            .find(|s| s.hash == Some(12912552292955267819))
            .unwrap();
        let n = sign.ring.len() as f32;
        let [x, z] = (sign.ring.iter()).fold([0.0, 0.0], |m, c| [m[0] + c[0] / n, m[1] + c[1] / n]);
        let girth = level.mesh.erosion;
        let bare = Graph::build(&level.mesh, &level.placed, step_up());
        let graph = level.graph();
        for [dx, dz] in [[2.5, 0.0], [0.0, 2.5]] {
            let (from, to) = ([x - dx, sign.low, z - dz], [x + dx, sign.low, z + dz]);
            let straight = plan(&bare, from, to).unwrap();
            assert!(!clear_of(&straight, sign, 0.0), "{straight:?}");
            let r = plan(&graph, from, to).unwrap();
            assert!(clear_of(&r, sign, girth - 0.01), "{r:?}");
            for s in &level.solids {
                assert!(clear_of(&r, s, 0.0), "{r:?} through {s:?}");
            }
            assert!(r.length < straight.length + 1.0, "{r:?}");
        }
    }
}
