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
//!
//! The Phive container around it (`Phive\0`, then at 0x0b a section count
//! N, at 0x0c N offsets and then N sizes) holds the tagfile first, then
//! the shape's material table and its hit masks, one entry per material.

use anyhow::{Context, Result, ensure};

use crate::tagfile::Tagfile;

/// A shape's mesh in the actor's space.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ShapeMesh {
    pub vertices: Vec<[f32; 3]>,
    /// Indices into `vertices`, a quad split into two.
    pub triangles: Vec<[u32; 3]>,
    /// Each triangle's index into `materials`, by the shape tag table:
    /// the run holding its key (`section << 9 | primitive << 1 | half`).
    /// 0 without a table.
    pub triangle_materials: Vec<u16>,
    /// Its material table, as the container's second section holds it.
    pub materials: Vec<Material>,
    /// Its shape tag table as stored: each (primitive key, tag) where a
    /// run of one tag starts.
    pub tag_runs: Vec<(u32, u16)>,
    /// Each section's primitive count, in order.
    pub section_sizes: Vec<usize>,
}

/// One entry of a shape's material table: the material and sub-material,
/// by their index in the Phive config's lists, the user shape tags it
/// carries, by bit, and its layer and sub-layer hit masks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    pub material: u32,
    pub sub: u32,
    pub tags: u64,
    pub layers: u64,
    pub sub_layers: u64,
}

/// The sections of a Phive container, by its offset and size tables.
fn sections(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    ensure!(bytes.starts_with(b"Phive\0"), "not a Phive file");
    let n = usize::from(*bytes.get(0x0b).context("short Phive header")?);
    let word = |at: usize| -> Result<usize> {
        let b = bytes.get(at..at + 4).context("short Phive header")?;
        Ok(u32::from_le_bytes(b.try_into()?) as usize)
    };
    (0..n)
        .map(|i| {
            let (at, len) = (word(0x0c + 4 * i)?, word(0x0c + 4 * (n + i))?);
            bytes
                .get(at..at + len)
                .context("a Phive section runs past the file")
        })
        .collect()
}

/// The material table: section 1 holds (material, sub-material, tags),
/// 16 bytes each, and section 2 the (layer, sub-layer) masks, 16 each.
/// A bare tagfile, with no container, has none.
fn materials(bytes: &[u8]) -> Result<Vec<Material>> {
    if !bytes.starts_with(b"Phive\0") {
        return Ok(vec![]);
    }
    let all = sections(bytes)?;
    let (Some(table), Some(masks)) = (all.get(1), all.get(2)) else {
        return Ok(vec![]);
    };
    let u64_at = |b: &[u8], at: usize| u64::from_le_bytes(std::array::from_fn(|i| b[at + i]));
    let u32_at = |b: &[u8], at: usize| u32::from_le_bytes(std::array::from_fn(|i| b[at + i]));
    let (table, masks) = (table.as_chunks::<16>().0, masks.as_chunks::<16>().0);
    Ok((table.iter().enumerate())
        .map(|(i, m)| {
            let mask = masks
                .get(i)
                .map_or([u64::MAX; 2], |k| [u64_at(k, 0), u64_at(k, 8)]);
            Material {
                material: u32_at(m, 0),
                sub: u32_at(m, 4),
                tags: u64_at(m, 8),
                layers: mask[0],
                sub_layers: mask[1],
            }
        })
        .collect())
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
        let mut out = Self {
            materials: materials(bytes)?,
            ..Self::default()
        };
        for e in mesh.rel_array("shapeTagTable")? {
            out.tag_runs.push((
                e.field("meshPrimitiveKey")?.u32()?,
                e.field("shapeTag")?.u16()?,
            ));
        }
        let material = |key: u32| {
            let at = out.tag_runs.partition_point(|&(k, _)| k <= key);
            at.checked_sub(1).map_or(0, |i| out.tag_runs[i].1)
        };
        let mut tags = vec![];
        for (s, section) in mesh.rel_array("geometrySections")?.into_iter().enumerate() {
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
            let primitives = section.rel_array("primitives")?;
            out.section_sizes.push(primitives.len());
            for (n, p) in primitives.into_iter().enumerate() {
                let key = (s as u32) << 9 | (n as u32) << 1;
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
                tags.push(material(key));
                if c != d {
                    out.triangles.push([id(a)?, id(c)?, id(d)?]);
                    tags.push(material(key | 1));
                }
            }
        }
        out.triangle_materials = tags;
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
