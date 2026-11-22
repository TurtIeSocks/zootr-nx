//! A Phive shape file's Havok mesh (`Phive/Shape/Dcc/*.bphsh`): the
//! triangles a `PhshMesh` shape collides with, like a level piece's walls
//! (`__Physical`) or its invisible walls (`__Airwall`).
//!
//! Its first item is an `hknpMeshShape` split into geometry sections. A
//! section keeps each vertex as three `u16`s counted from the section's
//! offset, both in the shape's `bitScale16Inv` units, or, when its offset
//! is `i32::MAX` on every axis, as three plain `f32`s: a section too big to
//! count in `u16`s (15 of the game's 5,877 meshes have one). Its primitives are
//! four indices into the section's vertices: a quad, or a triangle when
//! the last two match. The layout follows the EPD ZeldaMods wiki's Bphsh
//! page (Havok 2023.2); this game's 2025.2 files name the same fields.

use anyhow::{Context, Result, ensure};

use crate::tagfile::Tagfile;

/// A shape's mesh in the actor's space.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ShapeMesh {
    pub vertices: Vec<[f32; 3]>,
    /// Indices into `vertices`, a quad split into two.
    pub triangles: Vec<[u32; 3]>,
}

/// Bytes per vertex: three `u16`s.
const VERTEX: usize = 6;
/// Bytes per vertex in a section of plain floats: three `f32`s.
const PLAIN: usize = 12;
/// The offset that marks a section of plain floats.
const UNPACKED: [i32; 3] = [i32::MAX; 3];

impl ShapeMesh {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let file = Tagfile::parse(bytes)?;
        let mesh = file.item(1, 0)?;
        ensure!(
            mesh.type_name() == "hknpMeshShape",
            "the shape is an {}, not a mesh",
            mesh.type_name()
        );
        let [x, y, z, _] = (mesh.field("vertexConversionUtil")?)
            .field("bitScale16Inv")?
            .floats()?;
        let scale = [x, y, z];
        let mut out = Self::default();
        for section in mesh.rel_array("geometrySections")? {
            let base = out.vertices.len();
            let offset: [i32; 3] = section.field("sectionOffset")?.ints()?;
            let buffer = section.rel_bytes("vertexBuffer")?;
            if offset == UNPACKED {
                out.vertices
                    .extend(buffer.as_chunks::<PLAIN>().0.iter().map(|v| {
                        std::array::from_fn(|i| {
                            f32::from_le_bytes(std::array::from_fn(|b| v[4 * i + b]))
                        })
                    }));
            } else {
                out.vertices
                    .extend(buffer.as_chunks::<VERTEX>().0.iter().map(|v| {
                        std::array::from_fn(|i| {
                            let q = u16::from_le_bytes([v[2 * i], v[2 * i + 1]]);
                            (offset[i] + i32::from(q)) as f32 * scale[i]
                        })
                    }));
            }
            for p in section.rel_array("primitives")? {
                let ids = p.bytes(4)?;
                let [a, b, c, d] = std::array::from_fn(|i| base + usize::from(ids[i]));
                let top = a.max(b).max(c).max(d);
                ensure!(
                    top < out.vertices.len(),
                    "a primitive names vertex {top} of {}",
                    out.vertices.len()
                );
                let id = |i: usize| u32::try_from(i).context("too many vertices");
                out.triangles.push([id(a)?, id(b)?, id(c)?]);
                if c != d {
                    out.triangles.push([id(a)?, id(c)?, id(d)?]);
                }
            }
        }
        Ok(out)
    }

    /// The lowest and highest corner of its vertices, or `None` without any.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = *self.vertices.first()?;
        Some(self.vertices.iter().fold((first, first), |(lo, hi), v| {
            (
                std::array::from_fn(|i| lo[i].min(v[i])),
                std::array::from_fn(|i| hi[i].max(v[i])),
            )
        }))
    }
}
