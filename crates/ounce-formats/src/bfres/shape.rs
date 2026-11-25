//! What a shape draws with: its material, the bone it hangs from, and how
//! many bones weigh each of its vertices.

use anyhow::Result;

use super::names::{shape_name, shapes};
use crate::reader::Reader;

/// A shape header's material index (u16), bone index (u16) and skin
/// count (u8): the bones weighing each vertex.
const MATERIAL: usize = 0x52;
const BONE: usize = 0x54;
const SKIN: usize = 0x5a;

/// One shape of a model, as its header has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub name: String,
    /// Its material, by index in the model's materials.
    pub material: usize,
    /// The bone it hangs from, by index in the skeleton.
    pub bone: usize,
    /// How many bones weigh each vertex. With 0 or 1 its vertices sit in
    /// a bone's space; with more, in the model's.
    pub skin: u8,
}

/// Every shape of a model, in file order.
pub fn shapes_of(data: &[u8]) -> Result<Vec<Shape>> {
    let r = Reader(data);
    shapes(data)
        .map(|at| {
            Ok(Shape {
                name: String::from_utf8_lossy(shape_name(data, at).unwrap_or_default())
                    .into_owned(),
                material: r.u16(at + MATERIAL)? as usize,
                bone: r.u16(at + BONE)? as usize,
                skin: r.u8(at + SKIN)?,
            })
        })
        .collect()
}
