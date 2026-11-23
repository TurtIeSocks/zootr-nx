//! What a placed actor is solid as, read from its pack at run time like
//! the navmesh: its physics names its controller set, whose main rigid
//! body names the shape Link bumps into. A shape is primitives (a box,
//! a sphere, a capsule, a polytope), each with its bounds in the actor's
//! space, so no Havok mesh is read. Placed with its actor's pose, a shape
//! is a footprint on the floor that a route keeps clear of.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlMap, Romfs, pack_file};

use super::kinds;
use crate::items::{component, follow};
use crate::known::{Name, field};
use crate::warp::{Placement, Pose};

const PHYSICS: &Name = field::PHYSICS_REF;
const CONTROLLER_SET: &Name = field::CONTROLLER_SET_PATH;
const ALIASES: &Name = field::ALIAS_ENTITY_LIST;
const ALIAS: &Name = field::ALIAS_ENTITY;
const RIGID_BODY: &Name = field::RIGID_BODY_NAME;
pub(super) const RIGID_BODIES: &Name = field::RIGID_BODY_ENTITY_NAME_PATH_ARY;
pub(super) const BODY_SHAPE: &Name = field::SHAPE_NAME;
const LAYER: &Name = field::LAYER_ENTITY;
pub(super) const SHAPES: &Name = field::SHAPE_NAME_PATH_ARY;
pub(super) const FILE: &Name = field::FILE_PATH;
const BOUNDS: &Name = field::AUTO_CALC;
const CUTS_MESH: &Name = field::NAV_MESH_OBJECT_REF;
/// An entry's own name in a controller set's lists.
pub(super) const ENTRY_NAME: &str = "Name";
/// The alias of the rigid body the actor stands as.
const MAIN: &str = "Main";
const MIN: &str = "Min";
const MAX: &str = "Max";

/// One primitive of a shape: its kind, like `Box`, and the lowest and
/// highest corner of its bounds in the actor's space.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub kind: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// An actor's solid shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub parts: Vec<Part>,
    /// Its pack cuts the navmesh round it at run time, so the mesh in the
    /// dump doesn't know it's there.
    pub cuts_mesh: bool,
    /// Its body collides: it has a collision layer, or its body file isn't
    /// in the pack to say. A body without one, like an area's that only
    /// cuts the mesh for the actors that walk it, stops nothing.
    pub collides: bool,
}

pub(super) fn text<'a>(map: &'a BymlMap, key: &str) -> Option<&'a str> {
    map.get(key)?.as_string().ok().map(|s| s.as_str())
}

pub(super) fn maps<'a>(map: &'a BymlMap, key: &str) -> impl Iterator<Item = &'a BymlMap> {
    (map.get(key).and_then(|v| v.as_array().ok()).into_iter())
        .flatten()
        .filter_map(|v| v.as_map().ok())
}

fn point(map: &BymlMap, key: &str) -> Option<[f32; 3]> {
    let p = map.get(key)?.as_map().ok()?;
    let f = |k: &str| match p.get(k)? {
        Byml::Float(f) => Some(*f),
        Byml::Double(f) => Some(*f as f32),
        Byml::I32(i) => Some(*i as f32),
        _ => None,
    };
    Some([f("X")?, f("Y")?, f("Z")?])
}

/// The bounds a map carries under `AutoCalc`, when they hold any space.
pub(super) fn bounds(map: &BymlMap) -> Option<([f32; 3], [f32; 3])> {
    let bounds = map.get(BOUNDS.get())?.as_map().ok()?;
    let (min, max) = (point(bounds, MIN)?, point(bounds, MAX)?);
    (0..3).all(|i| max[i] >= min[i]).then_some((min, max))
}

/// The parts of a shape file: each primitive in its lists, by the list's
/// name, with its own bounds. A shape whose lists hold none with bounds,
/// like a `PhshMesh` (a Havok mesh), is one part: the bounds of the whole.
fn parts(shape: &BymlMap) -> Vec<Part> {
    let mut out = vec![];
    let mut whole = None;
    for (kind, list) in shape {
        let Ok(list) = list.as_array() else {
            continue;
        };
        whole = whole.or(Some(kind));
        for prim in list.iter().filter_map(|p| p.as_map().ok()) {
            if let Some((min, max)) = bounds(prim) {
                out.push(Part {
                    kind: kind.to_string(),
                    min,
                    max,
                });
            }
        }
    }
    if out.is_empty()
        && let (Some(kind), Some((min, max))) = (whole, bounds(shape))
    {
        out.push(Part {
            kind: kind.to_string(),
            min,
            max,
        });
    }
    out
}

/// An actor's pack, its actor param and its physics' controller set.
pub(super) struct Physics {
    pub pack: ActorPack,
    pub param: BymlMap,
    pub set: BymlMap,
}

/// `actor`'s [`Physics`], or `None` when it has no pack, no physics or no
/// controller set in its pack.
pub(super) fn physics_of(romfs: &Romfs, actor: &str) -> Result<Option<Physics>> {
    if !romfs.root().join(pack_file(actor)).exists() {
        return Ok(None);
    }
    let pack = ActorPack::load(romfs, actor)?;
    let Some(param) = pack.actor_param()? else {
        return Ok(None);
    };
    let Some(physics) = follow(&pack, &param, PHYSICS.get())? else {
        return Ok(None);
    };
    let Some(set) = text(&physics, CONTROLLER_SET.get()) else {
        return Ok(None);
    };
    let set = pack.resolve(set)?;
    Ok(set.map(|set| Physics { pack, param, set }))
}

/// The file of the entry named `name` in the controller set's `list`.
pub(super) fn entry<'a>(set: &'a BymlMap, list: &Name, name: &str) -> Option<&'a str> {
    maps(set, list.get())
        .find(|e| text(e, ENTRY_NAME) == Some(name))
        .and_then(|e| text(e, FILE.get()))
}

/// The shape of `actor`'s main rigid body, or `None` when it has no pack,
/// no physics, no main body, or a shape file from outside its pack.
pub fn shape_of(romfs: &Romfs, actor: &str) -> Result<Option<Shape>> {
    let Some(Physics { pack, param, set }) = physics_of(romfs, actor)? else {
        return Ok(None);
    };
    let cuts_mesh = component(&param, CUTS_MESH.get()).is_some();
    // The main alias names a rigid body, whose file names its shape.
    // Without aliases, as a pot's set has none, its first rigid body.
    // Without rigid bodies, as a static prop's set has none, its one shape.
    let main = maps(&set, ALIASES.get())
        .find(|a| text(a, ALIAS.get()) == Some(MAIN))
        .and_then(|m| text(m, RIGID_BODY.get()));
    let first = || maps(&set, RIGID_BODIES.get()).find_map(|b| text(b, ENTRY_NAME));
    let only = || match maps(&set, SHAPES.get()).collect::<Vec<_>>()[..] {
        [one] => text(one, ENTRY_NAME),
        _ => None,
    };
    let Some(name) = main.or_else(first).or_else(only) else {
        return Ok(None);
    };
    // A rigid body file shared from outside the pack isn't in the dump's
    // pack: its shape then goes by the body's own name.
    let body = match entry(&set, RIGID_BODIES, name) {
        Some(file) => pack.resolve(file)?,
        None => None,
    };
    let shape = body.as_ref().and_then(|b| text(b, BODY_SHAPE.get()));
    let Some(file) = entry(&set, SHAPES, shape.unwrap_or(name)) else {
        return Ok(None);
    };
    let Some(shape) = pack.resolve(file)? else {
        return Ok(None);
    };
    Ok(Some(Shape {
        parts: parts(&shape),
        cuts_mesh,
        collides: body.is_none_or(|b| text(&b, LAYER.get()).is_some()),
    }))
}

/// A part placed in the world: its footprint on the floor, a convex ring
/// of (x, z) counterclockwise seen from above, and the heights it spans.
#[derive(Clone, Debug, PartialEq)]
pub struct Solid {
    pub name: String,
    pub hash: Option<u64>,
    pub ring: Vec<[f32; 2]>,
    pub low: f32,
    pub high: f32,
}

impl Solid {
    /// Whether (x, z) lies inside its footprint, its edge included.
    pub fn holds(&self, p: [f32; 2]) -> bool {
        inside(&self.ring, p)
    }

    /// The share of the way from `a` to `b` (each x, z) over its
    /// footprint, as the part from `t0` to `t1`: the line clipped by each
    /// edge of its convex ring in turn. `None` when it misses.
    pub fn over(&self, a: [f32; 2], b: [f32; 2]) -> Option<(f32, f32)> {
        let ring = &self.ring;
        if ring.len() < 3 {
            return None;
        }
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for i in 0..ring.len() {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            let (fa, fb) = (turn(p, q, a), turn(p, q, b));
            match (fa < 0.0, fb < 0.0) {
                (true, true) => return None,
                (true, false) => t0 = t0.max(fa / (fa - fb)),
                (false, true) => t1 = t1.min(fa / (fa - fb)),
                (false, false) => {}
            }
        }
        (t0 <= t1).then_some((t0, t1))
    }
}

/// Whether `p` lies inside the convex counterclockwise `ring`, its edge
/// included.
pub fn inside(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    ring.len() >= 3 && (0..ring.len()).all(|i| turn(ring[i], ring[(i + 1) % ring.len()], p) >= 0.0)
}

/// The rotation by `Rotate` angles X, Y, Z, in radians: about X first,
/// then Y, then Z, each counterclockwise looking down its axis.
pub(super) fn rotation([x, y, z]: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, sx * cy, cx * cy],
    ]
}

/// `p` in the actor's space moved to the world by `pose`: scaled, turned,
/// then moved.
pub fn place(p: [f32; 3], pose: &Pose) -> [f32; 3] {
    let r = rotation(pose.rotate);
    let s: [f32; 3] = std::array::from_fn(|i| p[i] * pose.scale[i]);
    std::array::from_fn(|i| pose.at[i] + r[i][0] * s[0] + r[i][1] * s[1] + r[i][2] * s[2])
}

/// How near two corners of a footprint are to be one, in m.
const SAME: f32 = 0.01;
/// How little a footprint's corner may turn and still be one: twice the
/// area it adds, in square m.
const STRAIGHT: f32 = 1e-4;

/// Twice the signed area of a, b, c: positive when they turn
/// counterclockwise.
fn turn(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// The convex hull of `points`, counterclockwise, with no two corners
/// `SAME` and none `STRAIGHT`.
pub fn hull(points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    let mut points = points
        .into_iter()
        .fold(vec![], |mut kept: Vec<[f32; 2]>, p| {
            if kept
                .iter()
                .all(|q| (p[0] - q[0]).hypot(p[1] - q[1]) >= SAME)
            {
                kept.push(p);
            }
            kept
        });
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    if points.len() < 3 {
        return points;
    }
    // Andrew's monotone chain: the lower side left to right, then the
    // upper side back.
    let mut ring: Vec<[f32; 2]> = vec![];
    let backward: Vec<[f32; 2]> = points.iter().rev().copied().collect();
    for side in [&points, &backward] {
        let start = ring.len();
        for &p in side {
            while ring.len() >= start + 2
                && turn(ring[ring.len() - 2], ring[ring.len() - 1], p) <= STRAIGHT
            {
                ring.pop();
            }
            ring.push(p);
        }
        ring.pop();
    }
    ring
}

/// `part` of the placement `placed`, in the world.
pub fn solid(part: &Part, placed: &Placement) -> Solid {
    let corners = (0..8).map(|i| {
        let pick = |axis: usize| match i >> axis & 1 {
            0 => part.min[axis],
            _ => part.max[axis],
        };
        place([pick(0), pick(1), pick(2)], &placed.pose)
    });
    let corners: Vec<[f32; 3]> = corners.collect();
    let heights = corners.iter().map(|c| c[1]);
    Solid {
        name: placed.name.clone(),
        hash: placed.hash,
        ring: hull(corners.iter().map(|c| [c[0], c[2]]).collect()),
        low: heights.clone().fold(f32::MAX, f32::min),
        high: heights.fold(f32::MIN, f32::max),
    }
}

/// The solids of `placed` that the map's navmesh doesn't know: the parts
/// of each placement whose actor cuts the mesh at run time and collides. A door is
/// passed through by its link in the graph, so it's no solid here. Each
/// actor's shape is read once.
pub fn solids(romfs: &Romfs, placed: &[Placement]) -> Result<Vec<Solid>> {
    let mut shapes: HashMap<&str, Option<Shape>> = HashMap::new();
    let mut out = vec![];
    let doors = |p: &&Placement| kinds::word(&p.name) == Some("door");
    for p in placed.iter().filter(|p| !doors(p)) {
        if !shapes.contains_key(p.name.as_str()) {
            shapes.insert(&p.name, shape_of(romfs, &p.name)?);
        }
        let Some(shape) = &shapes[p.name.as_str()] else {
            continue;
        };
        if shape.cuts_mesh && shape.collides {
            out.extend(shape.parts.iter().map(|part| solid(part, p)));
        }
    }
    Ok(out)
}

/// Every collision part of `placed`, mesh-cutting or not: what a path
/// through it meets.
pub fn parts_of(romfs: &Romfs, placed: &Placement) -> Result<Vec<Solid>> {
    Ok((shape_of(romfs, &placed.name)?.into_iter())
        .flat_map(|shape| {
            shape
                .parts
                .into_iter()
                .map(|part| solid(&part, placed))
                .collect::<Vec<_>>()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;
    use std::path::Path;

    use super::*;
    use crate::known::{actor, map};
    use crate::nav::Level;

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3
    }

    fn placed(at: [f32; 3], rotate: [f32; 3], scale: [f32; 3]) -> Placement {
        Placement {
            name: "FakeCrate".into(),
            hash: Some(1),
            label: None,
            pose: Pose { at, rotate, scale },
            presence: None,
        }
    }

    #[test]
    fn a_hull_keeps_the_outer_corners_counterclockwise() {
        let ring = hull(vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [1.0, 0.5],
            [2.0, 1.0],
            [0.0, 1.0],
            [2.004, 1.0],
            [1.0, 0.0],
        ]);
        assert_eq!(ring.len(), 4, "{ring:?}");
        for i in 0..4 {
            assert!(turn(ring[i], ring[(i + 1) % 4], ring[(i + 2) % 4]) > 0.0);
        }
        assert!(inside(&ring, [1.0, 0.5]) && !inside(&ring, [2.5, 0.5]));
    }

    #[test]
    fn a_part_is_scaled_turned_and_moved_into_the_world() {
        // A box 2 m along its x and 1 m along its z, 1 m tall, doubled
        // across its x, turned a quarter about y: its x lies along -z.
        let part = Part {
            kind: "Box".into(),
            min: [-1.0, 0.0, -0.5],
            max: [1.0, 1.0, 0.5],
        };
        let s = solid(
            &part,
            &placed([10.0, 5.0, 20.0], [0.0, FRAC_PI_2, 0.0], [2.0, 1.0, 1.0]),
        );
        assert_eq!(s.ring.len(), 4, "{:?}", s.ring);
        for corner in [[9.5, 18.0], [10.5, 18.0], [10.5, 22.0], [9.5, 22.0]] {
            assert!(
                s.ring.iter().any(|&c| near(c, corner)),
                "{corner:?} {:?}",
                s.ring
            );
        }
        assert!((s.low - 5.0).abs() < 1e-4 && (s.high - 6.0).abs() < 1e-4);
        assert!(s.holds([10.0, 21.5]) && !s.holds([11.0, 20.0]));
    }

    /// A small chest's body: one polytope, 0.73 by 0.57 m and 0.38 m tall,
    /// standing on its origin, and its pack cuts the mesh round it.
    #[test]
    fn a_chest_reads_to_the_bounds_of_its_body() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let shape = shape_of(&romfs, actor::OBJ_T_BOX_A_01).unwrap().unwrap();
        assert!(shape.cuts_mesh);
        let [part] = &shape.parts[..] else {
            panic!("{shape:?}");
        };
        assert_eq!(part.kind, "Polytope");
        let want = ([-0.364, 0.0, -0.283], [0.364, 0.378, 0.283]);
        for i in 0..3 {
            assert!((part.min[i] - want.0[i]).abs() < 1e-3, "{part:?}");
            assert!((part.max[i] - want.1[i]).abs() < 1e-3, "{part:?}");
        }
    }

    /// Kokiri Forest as a child: the props whose packs cut the mesh (chests,
    /// pots, rocks, bushes, signs) each get a footprint, and its doors none.
    #[test]
    fn the_forest_s_props_stand_as_solids() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new("../../dump/oot.nup");
        let level = Level::of(&romfs, game, map::KOKIRI_FOREST_CHILD).unwrap();
        let named = |name: &str| level.solids.iter().filter(|s| s.name == name).count();
        assert_eq!(named(actor::OBJ_T_BOX_A_01), 5);
        assert!(level.solids.len() >= 50, "{}", level.solids.len());
        assert!(
            level
                .solids
                .iter()
                .all(|s| kinds::word(&s.name) != Some("door"))
        );
        for s in &level.solids {
            assert!(s.ring.len() >= 3, "{s:?}");
            assert!(s.high > s.low && s.high - s.low < 10.0, "{s:?}");
        }
    }
}
