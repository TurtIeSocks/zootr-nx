//! The Havok meshes a level piece collides with, read straight from the
//! dump since zootr doesn't extract them: its walls and floors
//! (`__Physical`) and its invisible walls (`__Airwall`). Only meshes the
//! game's collision filter lets touch Link on foot count ([`filter`]).
//!
//! [`filter`]: super::filter

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use ounce_formats::{ActorPack, BymlMap, Romfs, ShapeMesh};

use super::filter::{Filter, Touch};
use super::shape::{
    BODY_SHAPE, ENTRY_NAME, FILE, Physics, RIGID_BODIES, SHAPES, maps, physics_of, place, text,
};
use super::vec3::{cross, dot, sub};
use crate::known::field;
use crate::warp::Placement;

/// Where the shape meshes live, by the name a `PhshMeshPath` gives.
const MESHES: &str = "Phive/Shape/Dcc/";

/// A mesh placed in the world, as triangles, with the bounds of them all.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    /// The mesh's name, like `<piece>__Physical`.
    pub name: String,
    pub triangles: Vec<[[f32; 3]; 3]>,
    pub lo: [f32; 3],
    pub hi: [f32; 3],
    /// Link stands on it where it faces up: no unridable tag.
    pub ridable: bool,
}

impl Wall {
    /// `mesh` moved into the world by `pose`.
    pub fn posed(name: &str, mesh: &ShapeMesh, pose: &crate::warp::Pose) -> Self {
        Self::posed_where(name, mesh, pose, |_| true)
    }

    /// The triangles of `mesh` that `keep` holds, by index, moved into the
    /// world by `pose`.
    pub fn posed_where(
        name: &str,
        mesh: &ShapeMesh,
        pose: &crate::warp::Pose,
        keep: impl Fn(usize) -> bool,
    ) -> Self {
        let at: Vec<[f32; 3]> = mesh.vertices.iter().map(|&v| place(v, pose)).collect();
        let triangles = (mesh.triangles.iter().enumerate())
            .filter(|(i, _)| keep(*i))
            .map(|(_, t)| t.map(|i| at[i as usize]))
            .collect();
        Self::new(name, triangles)
    }

    /// The wall of `triangles`, already in the world.
    pub fn new(name: &str, triangles: Vec<[[f32; 3]; 3]>) -> Self {
        let first = triangles.first().map(|t| t[0]).unwrap_or_default();
        let (lo, hi) = triangles
            .iter()
            .flatten()
            .fold((first, first), |(lo, hi), v| {
                (
                    std::array::from_fn(|i| lo[i].min(v[i])),
                    std::array::from_fn(|i| hi[i].max(v[i])),
                )
            });
        Self {
            name: name.to_owned(),
            triangles,
            lo,
            hi,
            ridable: true,
        }
    }

    /// Whether the segment from `a` to `b` passes through any triangle.
    pub fn crosses(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let apart = (0..3).any(|i| a[i].max(b[i]) < self.lo[i] || a[i].min(b[i]) > self.hi[i]);
        !apart && self.triangles.iter().any(|t| through(a, b, t))
    }

    /// The heights of its floor straight under or over `(x, z)`: the
    /// triangles there that face up, no steeper than Link walks.
    pub fn floors_at(&self, x: f32, z: f32) -> Vec<f32> {
        if x < self.lo[0] || x > self.hi[0] || z < self.lo[2] || z > self.hi[2] {
            return vec![];
        }
        (self.triangles.iter())
            .filter(|t| floor_normal(t) && facing_up(t))
            .filter_map(|t| height_in(t, x, z))
            .collect()
    }
}

/// The least a floor's normal points up: steeper ground is a wall.
const FLOOR_UP: f32 = 0.7;

/// Whether `t` lies no steeper than Link walks, facing up or down:
/// [`facing_up`] tells which.
pub fn floor_normal([p, q, r]: &[[f32; 3]; 3]) -> bool {
    let n = cross(sub(*q, *p), sub(*r, *p));
    let len = dot(n, n).sqrt();
    len > 1e-9 && n[1].abs() / len >= FLOOR_UP
}

/// Whether triangle `t` faces up by its winding: the side Link stands on.
pub fn facing_up([p, q, r]: &[[f32; 3]; 3]) -> bool {
    cross(sub(*q, *p), sub(*r, *p))[1] > 0.0
}

/// The height of triangle `t` at `(x, z)`, when it lies over that spot.
pub fn height_in([p, q, r]: &[[f32; 3]; 3], x: f32, z: f32) -> Option<f32> {
    let det = (q[2] - r[2]) * (p[0] - r[0]) + (r[0] - q[0]) * (p[2] - r[2]);
    if det.abs() < 1e-9 {
        return None;
    }
    let u = ((q[2] - r[2]) * (x - r[0]) + (r[0] - q[0]) * (z - r[2])) / det;
    let v = ((r[2] - p[2]) * (x - r[0]) + (p[0] - r[0]) * (z - r[2])) / det;
    let w = 1.0 - u - v;
    (u >= 0.0 && v >= 0.0 && w >= 0.0).then(|| u * p[1] + v * q[1] + w * r[1])
}

/// Where the line from `o` by `d` meets triangle `t`'s plane, Moller and
/// Trumbore's way: `(u, v, t)`, the hit's weights along the triangle's
/// second and third corners and how far along `d` (times its length).
/// `None` when the line runs along the plane.
pub(super) fn moller(o: [f32; 3], d: [f32; 3], [p, q, r]: &[[f32; 3]; 3]) -> Option<[f32; 3]> {
    let (e1, e2) = (sub(*q, *p), sub(*r, *p));
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = sub(o, *p);
    let k = cross(s, e1);
    Some([dot(s, h) / det, dot(d, k) / det, dot(e2, k) / det])
}

/// Whether the segment from `a` to `b` passes through triangle `t`.
pub(super) fn through(a: [f32; 3], b: [f32; 3], tri: &[[f32; 3]; 3]) -> bool {
    moller(a, sub(b, a), tri).is_some_and(|[u, v, t]| {
        (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)
    })
}

/// One mesh of an actor's shapes: whether its rigid body stands on the
/// player's airwall sub-layer (an invisible wall), and that body's
/// `Enable` masks (all ones without a body).
#[derive(Clone, Debug, PartialEq)]
pub struct MeshUse {
    pub name: String,
    pub air: bool,
    pub body: [u64; 2],
}

/// The file `file` names in `pack`, if it names one the pack holds.
fn resolved(pack: &ActorPack, file: Option<&str>) -> Result<Option<BymlMap>> {
    Ok(file.map(|f| pack.resolve(f)).transpose()?.flatten())
}

/// The meshes of `actor`'s shapes. A body or shape whose file the pack
/// lacks is left out: a shape with no body is no airwall and has all
/// ones for its masks, so it touches everything.
pub fn mesh_names(romfs: &Romfs, filter: &Filter, actor: &str) -> Result<Vec<MeshUse>> {
    let Some(Physics { pack, set, .. }) = physics_of(romfs, actor)? else {
        return Ok(vec![]);
    };
    let mut bodies = HashMap::new();
    for body in maps(&set, RIGID_BODIES.get()) {
        let Some(body) = resolved(&pack, text(body, FILE.get()))? else {
            continue;
        };
        let Some(shape) = text(&body, BODY_SHAPE.get()) else {
            continue;
        };
        let air = text(&body, field::SUB_LAYER_ENTITY.get()) == Some(field::AIR_WALL.get());
        let masks = [
            filter.layer_mask(text(&body, field::ENABLE_LAYER_HIT_MASK.get())),
            filter.sub_layer_mask(text(&body, field::ENABLE_SUB_LAYER_HIT_MASK.get())),
        ];
        bodies.insert(shape.to_owned(), (air, masks));
    }
    let mut out = vec![];
    for shape in maps(&set, SHAPES.get()) {
        let name = text(shape, ENTRY_NAME).unwrap_or_default();
        let Some(param) = resolved(&pack, text(shape, FILE.get()))? else {
            continue;
        };
        let (air, body) = bodies.get(name).copied().unwrap_or((false, [u64::MAX; 2]));
        for mesh in maps(&param, field::PHSH_MESH.get()) {
            if let Some(path) = text(mesh, field::PHSH_MESH_PATH.get()) {
                let file = path.rsplit('/').next().unwrap_or(path);
                out.push(MeshUse {
                    name: file.trim_end_matches(".phsh").to_owned(),
                    air,
                    body,
                });
            }
        }
    }
    Ok(out)
}

/// What each of a mesh's materials does to Link under a body with
/// `body` masks: its masks ANDed with the body's. One for a mesh with no
/// material table.
pub fn touches(filter: &Filter, body: [u64; 2], mesh: &ShapeMesh) -> Vec<Touch> {
    let one = |(layers, subs, tags): (u64, u64, u64)| {
        filter.touch(body[0] & layers, body[1] & subs, tags)
    };
    if mesh.materials.is_empty() {
        return vec![one((u64::MAX, u64::MAX, 0))];
    }
    (mesh.materials.iter())
        .map(|m| one((m.layers, m.sub_layers, m.tags)))
        .collect()
}

/// What a mesh does to Link as a whole: it touches him if any of its
/// materials does, and is ridable only if all are.
pub fn touch(filter: &Filter, body: [u64; 2], mesh: &ShapeMesh) -> Touch {
    (touches(filter, body, mesh).into_iter()).fold(
        Touch {
            on_foot: false,
            climbing: false,
            ridable: true,
        },
        |a, t| Touch {
            on_foot: a.on_foot || t.on_foot,
            climbing: a.climbing || t.climbing,
            ridable: a.ridable && t.ridable,
        },
    )
}

/// The mesh name a dump path under [`MESHES`] holds.
fn mesh_name(path: &str) -> Option<&str> {
    path.strip_prefix(MESHES)?.split('.').next()
}

/// A level's collision meshes, each posed by its placement.
#[derive(Clone, Debug, Default)]
pub struct Walls {
    /// The invisible walls, which the navmesh runs through.
    pub air: Vec<Wall>,
    /// The walls and floors Link sees, which the navmesh keeps off.
    pub solid: Vec<Wall>,
}

impl Walls {
    /// Seen walls `solid`, no invisible ones, as a test builds them.
    #[cfg(test)]
    pub fn solid_only(solid: Vec<Wall>) -> Self {
        Self { air: vec![], solid }
    }
}

/// The collision meshes placed in a level, from one walk of the dump.
pub fn walls(romfs: &Romfs, game: &Path, placed: &[Placement]) -> Result<Walls> {
    let filter = Filter::get(romfs)?;
    let mut names: HashMap<&str, Vec<MeshUse>> = HashMap::new();
    for p in placed {
        if !names.contains_key(p.name.as_str()) {
            names.insert(&p.name, mesh_names(romfs, filter, &p.name)?);
        }
    }
    let wanted: HashSet<&str> = names.values().flatten().map(|u| u.name.as_str()).collect();
    let mut walls = Walls::default();
    let meshes = meshes(romfs, game, &wanted)?;
    for p in placed {
        for u in &names[p.name.as_str()] {
            let Some(mesh) = meshes.get(&u.name) else {
                continue;
            };
            // Each triangle goes by its own material: one that never
            // touches Link on foot (a camera's, an animal's, a climbing
            // wall's) is no wall to a walk or a fall, and an unridable one
            // is a wall he can't stand on.
            let touches = touches(filter, u.body, mesh);
            let of = |i: usize| {
                let m = mesh.triangle_materials.get(i).copied().unwrap_or(0);
                touches
                    .get(usize::from(m))
                    .or(touches.first())
                    .copied()
                    .unwrap_or_default()
            };
            for ridable in [true, false] {
                let keep = |i: usize| of(i).on_foot && of(i).ridable == ridable;
                let wall = Wall {
                    ridable,
                    ..Wall::posed_where(&u.name, mesh, &p.pose, keep)
                };
                match (wall.triangles.is_empty(), u.air) {
                    (true, _) => {}
                    (false, true) => walls.air.push(wall),
                    (false, false) => walls.solid.push(wall),
                }
            }
        }
    }
    walls
        .solid
        .extend(super::terrain::walls(romfs, game, placed)?);
    Ok(walls)
}

/// The meshes named in `wanted`, by name, from one walk of the dump.
pub fn meshes(
    romfs: &Romfs,
    game: &Path,
    wanted: &HashSet<&str>,
) -> Result<HashMap<String, ShapeMesh>> {
    let mut meshes = HashMap::new();
    if wanted.is_empty() {
        return Ok(meshes);
    }
    let keep = |path: &str| mesh_name(path).is_some_and(|n| wanted.contains(n));
    crate::game::each_where(game, keep, |path, data| {
        let mesh = ShapeMesh::parse(&romfs.dicts().decompress(&path, &data)?)?;
        meshes.insert(mesh_name(&path).unwrap_or_default().to_owned(), mesh);
        Ok(())
    })?;
    Ok(meshes)
}

/// The mesh named `name`, like `<piece>__Airwall`, in its
/// actor's space, or `None` when the dump has none.
pub fn mesh(romfs: &Romfs, game: &Path, name: &str) -> Result<Option<ShapeMesh>> {
    (super::dump_file(romfs, game, &format!("{MESHES}{name}."))?)
        .map(|bytes| ShapeMesh::parse(&bytes))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ounce_formats::pack_file;

    /// The faces the graph reaches from `start`.
    fn reached(graph: &super::super::graph::Graph, start: usize) -> Vec<bool> {
        let mut seen = vec![false; graph.arcs.len()];
        let mut stack = vec![start];
        while let Some(f) = stack.pop() {
            if !std::mem::replace(&mut seen[f], true) {
                stack.extend(graph.arcs[f].iter().map(|a| a.to));
            }
        }
        seen
    }

    /// Whether a jump or a fall joins the faces `island` holds to the rest.
    fn joined(graph: &super::super::graph::Graph, island: &[bool]) -> bool {
        let jump = super::super::limits::limits().level_jump();
        (graph.gaps(jump).into_iter().chain(graph.plunges()))
            .any(|hop| island[hop.from] != island[hop.to])
    }

    #[test]
    fn a_walls_floor_is_only_what_faces_up() {
        let floor = [[0.0, 0.0, 0.0], [0.0, 0.0, 4.0], [4.0, 0.0, 0.0]];
        let ceiling = [[0.0, 2.0, 0.0], [4.0, 2.0, 0.0], [0.0, 2.0, 4.0]];
        let wall = Wall::new("FakeRoom", vec![floor, ceiling]);
        assert_eq!(wall.floors_at(1.0, 1.0), [0.0]);
    }

    /// The Deku basement's strip beside the ivy, where derive once measured
    /// the vine gold skulltula from, stands behind the lobby shaft's solid
    /// wall. The navmesh covers it and no walk reaches it, but a level gap
    /// from the shallows did, through the wall.
    #[test]
    fn a_solid_wall_cuts_the_deku_basement_strip_off() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new(crate::game::DEV_DUMP);
        let deku = crate::known::map::INSIDE_THE_DEKU_TREE.get();
        let mut level = crate::nav::Level::of(&romfs, game, deku).unwrap();
        assert!(!level.walls.air.is_empty() && !level.walls.solid.is_empty());
        let (shallows, strip) = ([3.05, 81.31, 1.04], [5.2, 81.76, 1.35]);
        let walled = level.graph();
        assert!(walled.leap_walled(shallows, strip));
        let island = reached(&walled, walled.locate(strip).unwrap().face);
        assert!(!island[walled.locate(shallows).unwrap().face]);
        assert!(
            !joined(&walled, &island),
            "a jump or fall still joins the strip"
        );
        drop(walled);
        level.walls.solid.clear();
        assert!(
            joined(&level.graph(), &island),
            "nothing joined the strip to cut"
        );
    }

    /// Every mesh in the game reads, and lands within 10 km of its
    /// piece: none of them is far off from a section read the wrong way.
    #[test]
    fn every_mesh_reads_near_its_piece() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new(crate::game::DEV_DUMP);
        let mut read = 0;
        crate::game::each(game, MESHES, |path, data| {
            let mesh = ShapeMesh::parse(&romfs.dicts().decompress(&path, &data)?)?;
            let far = mesh.vertices.iter().flatten().any(|c| c.abs() > 10_000.0);
            assert!(!far, "{path} has a vertex past 10 km");
            read += 1;
            Ok(())
        })
        .unwrap();
        assert!(read > 5000, "only {read} meshes read");
    }

    /// Every Deku Tree piece's mesh reads to the bounds its shape file
    /// stores, to the mesh's 1/512 m step.
    #[test]
    fn each_deku_mesh_reads_to_its_shapes_bounds() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new(crate::game::DEV_DUMP);
        let mut checked = 0;
        crate::game::each(game, &format!("{MESHES}Land_Deku_"), |path, data| {
            let file = path.rsplit('/').next().unwrap_or_default();
            let name = file.split('.').next().unwrap_or_default();
            let actor = name.split("__").next().unwrap_or_default();
            let mesh = ShapeMesh::parse(&romfs.dicts().decompress(&path, &data)?)?;
            // A mesh can be named for a piece with no pack of its own.
            if !romfs.root().join(pack_file(actor)).exists() {
                return Ok(());
            }
            let pack = ActorPack::load(&romfs, actor)?;
            let param = format!("Phive/ShapeParam/{name}.phive__ShapeParam.gyml");
            let Some(shape) = pack.resolve(&param)? else {
                return Ok(());
            };
            let (min, max) = super::super::shape::bounds(&shape).unwrap();
            let (lo, hi) = mesh.bounds().unwrap();
            // Each triangle's shape tag run names one of its materials.
            let materials = mesh.materials.len().max(1);
            assert_eq!(
                mesh.triangle_materials.len(),
                mesh.triangles.len(),
                "{name}"
            );
            assert!(
                (mesh.triangle_materials.iter()).all(|&m| usize::from(m) < materials),
                "{name}: a triangle's tag is past its {materials} materials"
            );
            for i in 0..3 {
                assert!((lo[i] - min[i]).abs() < 0.003, "{name} min {lo:?} {min:?}");
                assert!((hi[i] - max[i]).abs() < 0.003, "{name} max {hi:?} {max:?}");
            }
            checked += 1;
            Ok(())
        })
        .unwrap();
        assert!(checked > 20, "only {checked} meshes checked");
    }

    /// The Deku shaft's big mesh mixes floor Link stands on with walls
    /// tagged unridable, so its triangles split by their own materials:
    /// some go to a ridable wall, some to an unridable one. A camera's
    /// airwall that never touches Link on foot is no wall at all.
    #[test]
    fn deku_walls_split_by_each_triangles_material() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new(crate::game::DEV_DUMP);
        let deku = crate::known::map::INSIDE_THE_DEKU_TREE.get();
        let placed = crate::warp::placements_in(&romfs, deku).unwrap();
        let walls = walls(&romfs, game, &placed).unwrap();
        let named = |ridable: bool| {
            (walls.solid.iter())
                .filter(|w| w.ridable == ridable)
                .map(|w| w.name.as_str())
                .collect::<HashSet<_>>()
        };
        let (ridable, unridable) = (named(true), named(false));
        let both: Vec<_> = ridable.intersection(&unridable).collect();
        assert!(!both.is_empty(), "no mesh splits by material");
        let filter = Filter::get(&romfs).unwrap();
        let uses: Vec<MeshUse> = (placed.iter())
            .flat_map(|p| mesh_names(&romfs, filter, &p.name).unwrap())
            .collect();
        let wanted: HashSet<&str> = uses.iter().map(|u| u.name.as_str()).collect();
        let meshes = meshes(&romfs, game, &wanted).unwrap();
        let untouched: Vec<&str> = (uses.iter())
            .filter(|u| {
                meshes
                    .get(&u.name)
                    .is_some_and(|m| !touch(filter, u.body, m).on_foot)
            })
            .map(|u| u.name.as_str())
            .collect();
        assert!(!untouched.is_empty(), "every Deku mesh touches Link");
        for name in untouched {
            let kept = walls.air.iter().chain(&walls.solid).any(|w| w.name == name);
            assert!(!kept, "{name} touches nothing on foot but stays a wall");
        }
    }
}
