//! A model's material parameters (BFRES v10): the default values its shader
//! reads, like a glow's colour, found by their names' hashes.

use anyhow::{Context, Result, ensure};

use crate::bfres::find_magic;
use crate::reader::Reader;

const MAGIC: &[u8; 4] = b"FMAT";
/// Where a material points at its shader info and its parameters' values,
/// and where the shader info points at the parameter table.
const SHADER: usize = 0x10;
const VALUES: usize = 0x58;
const TABLE: usize = 0x20;
/// A table entry: its name's hash, then where its value is and its type.
const ENTRY: usize = 0x18;
const HASH: usize = 0x08;
const OFFSET: usize = 0x10;
const KIND: usize = 0x12;
/// The type of four floats.
const FLOAT4: u8 = 0x0f;

/// Where the value of the parameter whose name hashes to `hash` is, in a
/// model of one material. Only four floats are read and written.
fn locate(data: &[u8], hash: u64) -> Result<usize> {
    let r = Reader(data);
    // At any byte: whether a material is aligned isn't checked.
    let mut found = find_magic(data, MAGIC, MAGIC.len() - 1, 1);
    let material = found.next().context("no material")?;
    ensure!(found.next().is_none(), "more than one material");
    let shader = r.ptr(r.ptr(material + SHADER)?)?;
    let table = r.ptr(shader + TABLE)?;
    let values = r.ptr(material + VALUES)?;
    let hash = hash.to_le_bytes();
    // The table ends where an entry stops looking like one: its first
    // eight bytes are zero.
    let mut at = table;
    while data.get(at..at + 8) == Some(&[0; 8]) {
        if data.get(at + HASH..at + HASH + 8) == Some(&hash) {
            ensure!(r.u8(at + KIND)? == FLOAT4, "not four floats");
            return Ok(values + r.u16(at + OFFSET)? as usize);
        }
        at += ENTRY;
    }
    anyhow::bail!("no parameter {:016x}", u64::from_le_bytes(hash))
}

/// The four floats of the parameter whose name hashes to `hash`.
pub fn param(data: &[u8], hash: u64) -> Result<[f32; 4]> {
    let at = locate(data, hash)?;
    let bytes = data.get(at..at + 16).context("a value past the end")?;
    Ok(std::array::from_fn(|i| {
        f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().expect("four bytes"))
    }))
}

/// `data` with the parameter whose name hashes to `hash` set to `value`.
pub fn set_param(data: &[u8], hash: u64, value: [f32; 4]) -> Result<Vec<u8>> {
    let at = locate(data, hash)?;
    let mut out = data.to_vec();
    let slot = out.get_mut(at..at + 16).context("a value past the end")?;
    for (i, v) in value.iter().enumerate() {
        slot[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    Ok(out)
}
