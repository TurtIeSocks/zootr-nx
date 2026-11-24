//! A map's navmesh (`Phive/NavMesh/*.bphnm`): the floor Havok's AI walks
//! on, as faces ringed by edges over a vertex list. An edge with no face
//! across it is the mesh's border: a wall, a ledge or the map's end.

use anyhow::{Context, Result, ensure};

use crate::tagfile::{Obj, Tagfile};
use crate::vec3::sub;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub start_edge: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    /// The face across this edge, or `None` on the mesh's border.
    pub across: Option<usize>,
}

#[derive(Debug, Default)]
pub struct NavMesh {
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// How far the mesh keeps from walls and obstacles: a spot this close
    /// to one has no face under it.
    pub erosion: f32,
}

/// Where a point stands on the mesh: the face and the floor's height there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footing {
    pub face: usize,
    pub height: f32,
}

/// A border edge near a point, and what lies a step past it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub edge: usize,
    /// Flat distance from the point to the edge.
    pub distance: f32,
    /// Flat direction from the point to the edge's nearest spot, in x and z.
    pub toward: [f32; 2],
    /// The floor a step past the edge, relative to the point's floor, or
    /// `None` where no mesh lies there.
    pub beyond: Option<f32>,
}

const NONE: u32 = u32::MAX;

fn index(o: Obj, name: &str) -> Result<u32> {
    o.field(name)?.u32()
}

impl NavMesh {
    /// Reads the `NavMesh` root of a Phive navmesh file.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let file = Tagfile::parse(bytes)?;
        let roots = file.roots()?;
        let (_, mesh) = roots
            .iter()
            .find(|(name, _)| name == "NavMesh")
            .context("no NavMesh root")?;
        let up: [f32; 4] = mesh.field("up")?.field("up")?.floats()?;
        ensure!(
            up[..3] == [0.0, 1.0, 0.0],
            "the mesh's up is {up:?}, not +Y"
        );
        let aabb = mesh.field("aabb")?;
        let corner = |name| -> Result<[f32; 3]> {
            let [x, y, z, _] = aabb.field(name)?.floats()?;
            Ok([x, y, z])
        };
        let faces = (mesh.array("faces")?.into_iter())
            .map(|f| {
                Ok(Face {
                    start_edge: index(f, "startEdgeIndex")? as usize,
                    edges: f.field("numEdges")?.u16()? as usize,
                })
            })
            .collect::<Result<_>>()?;
        let edges = (mesh.array("edges")?.into_iter())
            .map(|e| {
                let across = index(e, "oppositeFaceIndex")?;
                Ok(Edge {
                    a: index(e, "a")? as usize,
                    b: index(e, "b")? as usize,
                    across: (across != NONE).then_some(across as usize),
                })
            })
            .collect::<Result<_>>()?;
        let vertices = (mesh.array("vertices")?.into_iter())
            .map(|v| {
                let [x, y, z, _] = v.floats()?;
                Ok([x, y, z])
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            vertices,
            faces,
            edges,
            min: corner("min")?,
            max: corner("max")?,
            erosion: mesh.field("erosionRadius")?.floats::<1>()?[0],
        })
    }

    /// The edges ringing face `f`, in order.
    pub fn ring(&self, f: usize) -> &[Edge] {
        let face = self.faces[f];
        &self.edges[face.start_edge..face.start_edge + face.edges]
    }

    /// The floor's height on face `f` above (x, z), when the face covers
    /// that spot. Faces are fanned into triangles from their first vertex.
    pub fn height_on(&self, f: usize, x: f32, z: f32) -> Option<f32> {
        let ring = self.ring(f);
        let v = |i: usize| self.vertices[i];
        let first = v(ring.first()?.a);
        ring.iter().skip(1).find_map(|e| {
            // Solve (x, z) - p = s * u + t * w over the triangle p, q, first.
            let (p, q) = (v(e.a), v(e.b));
            let (u, w) = (sub(first, p), sub(q, p));
            let (rx, rz) = (x - p[0], z - p[2]);
            let det = u[0] * w[2] - w[0] * u[2];
            if det.abs() < 1e-9 {
                return None;
            }
            let s = (rx * w[2] - w[0] * rz) / det;
            let t = (u[0] * rz - rx * u[2]) / det;
            let slack = 1e-4;
            let inside = s >= -slack && t >= -slack && s + t <= 1.0 + slack;
            inside.then(|| p[1] + s * u[1] + t * w[1])
        })
    }

    /// The face under `p` whose floor is nearest its height, within `reach`.
    pub fn footing(&self, p: [f32; 3], reach: f32) -> Option<Footing> {
        self.footing_among(0..self.faces.len(), p, reach)
    }

    /// [`footing`](Self::footing), looking only at the faces `among` names,
    /// for a caller that has already narrowed them down.
    pub fn footing_among(
        &self,
        among: impl IntoIterator<Item = usize>,
        p: [f32; 3],
        reach: f32,
    ) -> Option<Footing> {
        among
            .into_iter()
            .filter_map(|face| {
                let height = self.height_on(face, p[0], p[2])?;
                ((height - p[1]).abs() <= reach).then_some(Footing { face, height })
            })
            .min_by(|a, b| (a.height - p[1]).abs().total_cmp(&(b.height - p[1]).abs()))
    }

    /// Border edges within `radius` (flat) of `p` and within `reach` of its
    /// height, nearest first, each with the floor a `step` past it.
    pub fn borders(&self, p: [f32; 3], radius: f32, reach: f32, step: f32) -> Vec<Border> {
        let mut out: Vec<Border> = (0..self.edges.len())
            .filter(|&i| self.edges[i].across.is_none())
            .filter_map(|i| {
                let e = self.edges[i];
                let (a, b) = (self.vertices[e.a], self.vertices[e.b]);
                let near = closest(a, b, p);
                let (dx, dz) = (near[0] - p[0], near[2] - p[2]);
                let distance = dx.hypot(dz);
                let level = (near[1] - p[1]).abs() <= reach;
                (distance <= radius && level).then(|| {
                    let toward = if distance > 1e-4 {
                        [dx / distance, dz / distance]
                    } else {
                        [0.0, 0.0]
                    };
                    let past = [near[0] + toward[0] * step, p[1], near[2] + toward[1] * step];
                    let beyond = self.footing(past, 50.0).map(|f| f.height - p[1]);
                    Border {
                        edge: i,
                        distance,
                        toward,
                        beyond,
                    }
                })
            })
            .collect();
        out.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        out
    }
}

/// The point on segment a..b nearest `p`, measured flat.
pub fn closest(a: [f32; 3], b: [f32; 3], p: [f32; 3]) -> [f32; 3] {
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    let len = dx * dx + dz * dz;
    let t = if len > 0.0 {
        (((p[0] - a[0]) * dx + (p[2] - a[2]) * dz) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [a[0] + t * dx, a[1] + t * (b[1] - a[1]), a[2] + t * dz]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 square at height `y` from x0, as one face, edges listed `a..`.
    fn square(mesh: &mut NavMesh, x0: f32, y: f32) -> usize {
        let v = mesh.vertices.len();
        for [x, z] in [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] {
            mesh.vertices.push([x0 + x, y, z]);
        }
        let start_edge = mesh.edges.len();
        for i in 0..4 {
            let (a, b) = (v + i, v + (i + 1) % 4);
            mesh.edges.push(Edge { a, b, across: None });
        }
        mesh.faces.push(Face {
            start_edge,
            edges: 4,
        });
        mesh.faces.len() - 1
    }

    #[test]
    fn a_point_stands_on_the_floor_nearest_its_height() {
        let mut mesh = NavMesh::default();
        square(&mut mesh, 0.0, 0.0);
        square(&mut mesh, 0.0, 5.0);
        assert_eq!(mesh.height_on(0, 1.5, 0.5), Some(0.0));
        assert_eq!(mesh.height_on(0, 3.0, 0.5), None);
        let on = |y| mesh.footing([1.0, y, 1.0], 3.0).map(|f| f.face);
        assert_eq!((on(0.4), on(4.0), on(10.0)), (Some(0), Some(1), None));
    }

    #[test]
    fn heights_follow_a_sloped_face() {
        let mut mesh = NavMesh::default();
        square(&mut mesh, 0.0, 0.0);
        mesh.vertices[1][1] = 2.0;
        mesh.vertices[2][1] = 2.0;
        let h = mesh.height_on(0, 1.0, 1.0).unwrap();
        assert!((h - 1.0).abs() < 1e-5, "{h}");
    }

    #[test]
    fn borders_say_what_lies_past_them() {
        let mut mesh = NavMesh::default();
        square(&mut mesh, 0.0, 0.0);
        square(&mut mesh, 2.0, -1.5);
        // Within 1.0 of the floor's height, so the step's lower edge is out.
        let borders = mesh.borders([1.5, 0.0, 1.0], 0.9, 1.0, 0.5);
        assert_eq!(borders.len(), 1, "{borders:?}");
        let east = borders[0];
        assert!((east.distance - 0.5).abs() < 1e-5);
        assert_eq!(east.toward, [1.0, 0.0]);
        assert_eq!(east.beyond, Some(-1.5));
        let west = mesh.borders([0.5, 0.0, 1.0], 0.9, 1.0, 0.5)[0];
        assert_eq!((west.toward, west.beyond), ([-1.0, 0.0], None));
    }
}
