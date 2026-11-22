//! The Havok meshes a level piece collides with, read straight from the
//! dump since zootr doesn't extract them: its walls and floors
//! (`__Physical`) and its invisible walls (`__Airwall`).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use ounce_formats::{Romfs, ShapeMesh};

use super::shape::{
    BODY_SHAPE, ENTRY_NAME, FILE, Physics, RIGID_BODIES, SHAPES, maps, physics_of, place, text,
};
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
}

impl Wall {
    /// `mesh` moved into the world by `pose`.
    pub fn posed(name: &str, mesh: &ShapeMesh, pose: &crate::warp::Pose) -> Self {
        let at: Vec<[f32; 3]> = mesh.vertices.iter().map(|&v| place(v, pose)).collect();
        let triangles = (mesh.triangles.iter())
            .map(|t| t.map(|i| at[i as usize]))
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
        }
    }

    /// Whether the segment from `a` to `b` passes through any triangle.
    pub fn crosses(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let apart = (0..3).any(|i| a[i].max(b[i]) < self.lo[i] || a[i].min(b[i]) > self.hi[i]);
        !apart && self.triangles.iter().any(|t| through(a, b, t))
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Whether the segment from `a` to `b` passes through triangle `t`
/// (Moller and Trumbore's test, kept to the segment).
fn through(a: [f32; 3], b: [f32; 3], [p, q, r]: &[[f32; 3]; 3]) -> bool {
    let d = sub(b, a);
    let (e1, e2) = (sub(*q, *p), sub(*r, *p));
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-9 {
        return false;
    }
    let s = sub(a, *p);
    let u = dot(s, h) / det;
    let k = cross(s, e1);
    let v = dot(d, k) / det;
    let t = dot(e2, k) / det;
    (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)
}

/// The meshes of `actor`'s shapes, each with whether it's an invisible
/// wall: the shape of a rigid body on the player's airwall sublayer.
pub fn mesh_names(romfs: &Romfs, actor: &str) -> Result<Vec<(String, bool)>> {
    let Some(Physics { pack, set, .. }) = physics_of(romfs, actor)? else {
        return Ok(vec![]);
    };
    let mut air = HashSet::new();
    for body in maps(&set, RIGID_BODIES.get()) {
        let file = text(body, FILE.get());
        let Some(body) = file.map(|f| pack.resolve(f)).transpose()?.flatten() else {
            continue;
        };
        if text(&body, field::SUB_LAYER_ENTITY.get()) == Some(field::AIR_WALL.get()) {
            air.extend(text(&body, BODY_SHAPE.get()).map(str::to_owned));
        }
    }
    let mut out = vec![];
    for shape in maps(&set, SHAPES.get()) {
        let name = text(shape, ENTRY_NAME).unwrap_or_default();
        let file = text(shape, FILE.get());
        let Some(param) = file.map(|f| pack.resolve(f)).transpose()?.flatten() else {
            continue;
        };
        for mesh in maps(&param, field::PHSH_MESH.get()) {
            if let Some(path) = text(mesh, field::PHSH_MESH_PATH.get()) {
                let file = path.rsplit('/').next().unwrap_or(path);
                out.push((
                    file.trim_end_matches(".phsh").to_owned(),
                    air.contains(name),
                ));
            }
        }
    }
    Ok(out)
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

/// The collision meshes placed in a level, from one walk of the dump.
pub fn walls(romfs: &Romfs, game: &Path, placed: &[Placement]) -> Result<Walls> {
    let mut names: HashMap<&str, Vec<(String, bool)>> = HashMap::new();
    for p in placed {
        if !names.contains_key(p.name.as_str()) {
            names.insert(&p.name, mesh_names(romfs, &p.name)?);
        }
    }
    let wanted: HashSet<&str> = names.values().flatten().map(|(n, _)| n.as_str()).collect();
    let mut walls = Walls::default();
    if wanted.is_empty() {
        return Ok(walls);
    }
    let mut meshes = HashMap::new();
    let keep = |path: &str| mesh_name(path).is_some_and(|n| wanted.contains(n));
    crate::game::each_where(game, keep, |path, data| {
        let mesh = ShapeMesh::parse(&romfs.dicts().decompress(&path, &data)?)?;
        meshes.insert(mesh_name(&path).unwrap_or_default().to_owned(), mesh);
        Ok(())
    })?;
    for p in placed {
        for (name, air) in &names[p.name.as_str()] {
            let Some(mesh) = meshes.get(name) else {
                continue;
            };
            let wall = Wall::posed(name, mesh, &p.pose);
            if *air {
                walls.air.push(wall)
            } else {
                walls.solid.push(wall)
            }
        }
    }
    Ok(walls)
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
    use ounce_formats::{ActorPack, pack_file};

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
            .any(|(a, b, ..)| island[a] != island[b])
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
        let game = Path::new("../../dump/oot.nup");
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

    /// Every Deku Tree piece's mesh reads to the bounds its shape file
    /// stores, to the mesh's 1/512 m step.
    #[test]
    fn each_deku_mesh_reads_to_its_shapes_bounds() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new("../../dump/oot.nup");
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
}
