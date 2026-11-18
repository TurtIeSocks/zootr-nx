//! Ladders and climbable walls (ivy, grating): placed actors Link climbs,
//! which the navmesh leaves out. Each is read from its pack's shape like a
//! solid, as one box placed in the world. Its most upright side is the
//! climb and its thinnest the way its face looks, so a box modelled lying
//! down and stood up by its placement reads the same as one modelled
//! upright. A route climbs one as a link between the floor at its foot and
//! the floor at its top, like a door's.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::Romfs;

use super::kinds;
use super::shape::{Shape, place, shape_of};
use crate::warp::Placement;

/// The least a climb rises to be one, in m: lower, Link steps or pulls
/// himself up instead.
const RISE: f32 = 1.0;
/// How near, flat, the foot of a climb is to the top of the one below for
/// the two to be one climb, in m.
const JOIN: f32 = 1.0;
/// How far over the top of the climb below the foot of the one above may
/// start, in m.
const GAP: f32 = 0.5;

/// A placed climb, in the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Climb {
    pub name: String,
    pub hash: Option<u64>,
    /// The middle of its lowest and of its highest side.
    pub foot: [f32; 3],
    pub top: [f32; 3],
    /// Flat and of length 1, out of one of its faces. The other looks the
    /// opposite way.
    pub out: [f32; 2],
    /// Half its thickness, in m.
    pub half: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn length(v: [f32; 3]) -> f32 {
    v[0].hypot(v[1]).hypot(v[2])
}

/// The climb of the box `min` to `max` in the actor's space, placed as
/// `placed`, or `None` when it rises under [`RISE`] or its thinnest side
/// stands upright.
pub fn climb(min: [f32; 3], max: [f32; 3], placed: &Placement) -> Option<Climb> {
    let mid: [f32; 3] = std::array::from_fn(|i| (min[i] + max[i]) / 2.0);
    // The middle of the box's side at `min` or `max` of `axis`, placed.
    let side = |axis: usize, end: [f32; 3]| {
        let mut p = mid;
        p[axis] = end[axis];
        place(p, &placed.pose)
    };
    let across: [[f32; 3]; 3] = std::array::from_fn(|i| sub(side(i, max), side(i, min)));
    let up = (0..3).max_by(|&a, &b| across[a][1].abs().total_cmp(&across[b][1].abs()))?;
    let thin = (0..3)
        .filter(|&i| i != up)
        .min_by(|&a, &b| length(across[a]).total_cmp(&length(across[b])))?;
    let [x, _, z] = across[thin];
    let flat = x.hypot(z);
    if across[up][1].abs() < RISE || flat < 1e-4 {
        return None;
    }
    let (low, high) = (side(up, min), side(up, max));
    let (foot, top) = if low[1] <= high[1] {
        (low, high)
    } else {
        (high, low)
    };
    Some(Climb {
        name: placed.name.clone(),
        hash: placed.hash,
        foot,
        top,
        out: [x / flat, z / flat],
        half: length(across[thin]) / 2.0,
    })
}

/// The lowest and highest corner of a box.
type Bounds = ([f32; 3], [f32; 3]);

/// The bounds of all of `shape`'s parts together.
fn bounds(shape: &Shape) -> Option<Bounds> {
    shape.parts.iter().fold(None, |acc, p| {
        Some(match acc {
            None => (p.min, p.max),
            Some((lo, hi)) => (
                std::array::from_fn(|i| p.min[i].min(lo[i])),
                std::array::from_fn(|i| p.max[i].max(hi[i])),
            ),
        })
    })
}

/// `climbs` with each stack of them joined into one: a climb whose foot
/// starts by the top of a lower one, up to [`GAP`] over it, and whose face
/// looks the same way carries the lower one up to its own top. A vine wall
/// is pieces of ivy, and the invisible climb walls in front of one stack
/// the same way.
pub fn joined(mut climbs: Vec<Climb>) -> Vec<Climb> {
    climbs.sort_by(|a, b| a.foot[1].total_cmp(&b.foot[1]));
    let mut out: Vec<Climb> = vec![];
    for c in climbs {
        let below = out.iter_mut().find(|b| {
            let flat = (b.top[0] - c.foot[0]).hypot(b.top[2] - c.foot[2]);
            let same_way = (b.out[0] * c.out[0] + b.out[1] * c.out[1]).abs() > 0.9;
            flat <= JOIN && c.foot[1] <= b.top[1] + GAP && c.top[1] > b.top[1] && same_way
        });
        match below {
            Some(b) => b.top = c.top,
            None => out.push(c),
        }
    }
    out
}

/// The climbs among `placed`, [`joined`]: each placement
/// [`kinds::climbs`] names that has a shape. Each actor's shape is read
/// once.
pub fn climbs(romfs: &Romfs, placed: &[Placement]) -> Result<Vec<Climb>> {
    let mut shapes: HashMap<&str, Option<Bounds>> = HashMap::new();
    let mut out = vec![];
    for p in placed.iter().filter(|p| kinds::climbs(&p.name)) {
        if !shapes.contains_key(p.name.as_str()) {
            let shape = shape_of(romfs, &p.name)?;
            shapes.insert(&p.name, shape.as_ref().and_then(bounds));
        }
        if let Some((min, max)) = shapes[p.name.as_str()] {
            out.extend(climb(min, max, p));
        }
    }
    Ok(joined(out))
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;
    use crate::nav::graph::{Graph, STEP_UP, Via};
    use crate::warp::Pose;

    fn placed(at: [f32; 3], rotate: [f32; 3]) -> Placement {
        Placement {
            name: "Obj_FakeLadder".into(),
            hash: Some(1),
            label: None,
            pose: Pose {
                at,
                rotate,
                scale: [1.0; 3],
            },
            presence: None,
        }
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        length(sub(a, b)) < 1e-3
    }

    #[test]
    fn an_upright_ladder_climbs_from_its_foot_to_its_top() {
        // The well's ladder: 1.1 m wide, 6.5 m tall, 0.2 m thick.
        let c = climb(
            [-0.57, 0.0, 0.0],
            [0.56, 6.5, 0.2],
            &placed([10.0, 2.0, 5.0], [0.0; 3]),
        )
        .unwrap();
        assert!(near(c.foot, [9.995, 2.0, 5.1]), "{:?}", c.foot);
        assert!(near(c.top, [9.995, 8.5, 5.1]), "{:?}", c.top);
        assert_eq!(c.out.map(f32::abs), [0.0, 1.0]);
        assert!((c.half - 0.1).abs() < 1e-4);
    }

    #[test]
    fn a_wall_modelled_lying_down_climbs_once_stood_up() {
        // Ivy is modelled flat, 3 m along z, and stood up by its rotation.
        let c = climb(
            [-1.6, -0.9, 0.0],
            [1.6, 0.1, 3.0],
            &placed([0.0; 3], [-FRAC_PI_2, 0.0, 0.0]),
        )
        .unwrap();
        assert!((c.top[1] - c.foot[1] - 3.0).abs() < 1e-3, "{c:?}");
        assert!((c.half - 0.5).abs() < 1e-3);
        assert!(c.out[0].abs() < 1e-3 && (c.out[1].abs() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_stack_of_pieces_is_one_climb_but_pieces_side_by_side_are_two() {
        let piece = |x: f32, y: f32| {
            climb(
                [0.0, 0.0, 0.0],
                [1.0, 3.0, 0.2],
                &placed([x, y, 0.0], [0.0; 3]),
            )
            .unwrap()
        };
        let joined = joined(vec![piece(0.0, 3.2), piece(0.0, 0.0), piece(4.0, 0.0)]);
        assert_eq!(joined.len(), 2);
        assert_eq!((joined[0].foot[1], joined[0].top[1]), (0.0, 6.2));
        assert_eq!((joined[1].foot[1], joined[1].top[1]), (0.0, 3.0));
    }

    /// The Forest Temple's 12.5 m ladder: a route from its foot to the
    /// ledge it reaches goes up it, where without it the way is far round
    /// or none.
    #[test]
    fn a_route_up_to_the_forest_temple_ledge_climbs_its_ladder() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = std::path::Path::new("../../dump/oot.nup");
        let level = crate::nav::Level::of(&romfs, game, crate::known::map::FOREST_TEMPLE).unwrap();
        let hash = 6306446343189810327;
        let ladder = level.climbs.iter().find(|c| c.hash == Some(hash)).unwrap();
        let graph = level.graph();
        let (foot, top) = graph.climb_ends(ladder).unwrap();
        assert!(top.at[1] - foot.at[1] > 10.0, "{foot:?} {top:?}");
        let route = crate::nav::route::plan(&graph, foot.at, top.at).unwrap();
        let vias: Vec<Via> = route.waypoints.iter().map(|w| w.via).collect();
        assert!(vias.contains(&Via::Climb(Some(hash))), "{route:?}");
        let bare = Graph::build(&level.mesh, &level.placed, STEP_UP).with_solids(&level.solids);
        if let Ok(round) = crate::nav::route::plan(&bare, foot.at, top.at) {
            assert!(round.length > route.length + 10.0, "{round:?}");
        }
    }

    #[test]
    fn a_low_or_flat_box_is_no_climb() {
        let low = climb([0.0; 3], [1.0, 0.8, 0.2], &placed([0.0; 3], [0.0; 3]));
        assert_eq!(low, None);
        // Lying flat, it rises too little: a floor, not a wall.
        let flat = climb([0.0; 3], [3.0, 0.2, 3.0], &placed([0.0; 3], [0.0; 3]));
        assert_eq!(flat, None);
    }
}
