//! A model's material parameters (BFRES v10): the default values its shader
//! reads, like a glow's colour, found by their names' hashes.

use anyhow::{Context, Result, ensure};

use crate::bfres::u64_at;

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
    let mut found = data.windows(4).enumerate().filter(|(_, w)| w == MAGIC);
    let (material, _) = found.next().context("no material")?;
    ensure!(found.next().is_none(), "more than one material");
    let shader = u64_at(data, u64_at(data, material + SHADER)?)?;
    let table = u64_at(data, shader + TABLE)?;
    let values = u64_at(data, material + VALUES)?;
    let hash = hash.to_le_bytes();
    // The table ends where an entry stops looking like one: its first
    // eight bytes are zero.
    let mut at = table;
    while data.get(at..at + 8) == Some(&[0; 8]) {
        if data.get(at + HASH..at + HASH + 8) == Some(&hash) {
            ensure!(data[at + KIND] == FLOAT4, "not four floats");
            let offset = u16::from_le_bytes([data[at + OFFSET], data[at + OFFSET + 1]]);
            return Ok(values + offset as usize);
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
