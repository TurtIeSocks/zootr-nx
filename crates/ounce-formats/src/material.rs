//! A model's material parameters (BFRES v10): the default values its shader
//! reads, like a glow's colour, found by their names' hashes.

use anyhow::{Context, Result, ensure};

use crate::bfres::{find_magic, name_at};
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

/// Where a material points at its name, its textures' names, and the
/// texture count.
const NAME: usize = 0x08;
const TEXTURES: usize = 0x20;
const TEXTURE_COUNT: usize = 0xa2;

/// One material of a model: its name, the textures it samples, and its
/// four-float parameters by their names' hashes.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMaterial {
    pub name: String,
    pub textures: Vec<String>,
    pub params: Vec<(u64, [f32; 4])>,
}

/// Every material in a model, in file order.
pub fn materials(data: &[u8]) -> Result<Vec<ModelMaterial>> {
    starts(data).map(|at| read(data, at)).collect()
}

/// Each material's offset. At any byte: whether one is aligned isn't
/// checked.
fn starts(data: &[u8]) -> impl Iterator<Item = usize> + '_ {
    find_magic(data, MAGIC, MAGIC.len(), 1)
}

fn read(data: &[u8], material: usize) -> Result<ModelMaterial> {
    let r = Reader(data);
    let text = |at: usize| -> Result<String> {
        let name = name_at(data, at).with_context(|| format!("no name at {at:#x}"))?;
        Ok(String::from_utf8_lossy(name).into_owned())
    };
    let names = r.ptr(material + TEXTURES)?;
    let textures = (0..r.u8(material + TEXTURE_COUNT)? as usize)
        .map(|i| text(names + 8 * i))
        .collect::<Result<_>>()?;
    let params = entries(data, material)?
        .into_iter()
        .filter(|e| e.kind == FLOAT4)
        .map(|e| Ok((e.hash, floats(data, e.value)?)))
        .collect::<Result<_>>()?;
    Ok(ModelMaterial {
        name: text(material + NAME)?,
        textures,
        params,
    })
}

struct Entry {
    hash: u64,
    kind: u8,
    /// Where its value is in the file.
    value: usize,
}

/// A material's parameter table.
fn entries(data: &[u8], material: usize) -> Result<Vec<Entry>> {
    let r = Reader(data);
    let shader = r.ptr(r.ptr(material + SHADER)?)?;
    let table = r.ptr(shader + TABLE)?;
    let values = r.ptr(material + VALUES)?;
    // The table ends where an entry stops looking like one: its first
    // eight bytes are zero.
    let mut out = Vec::new();
    let mut at = table;
    while data.get(at..at + 8) == Some(&[0; 8]) {
        out.push(Entry {
            hash: r.u64(at + HASH)?,
            kind: r.u8(at + KIND)?,
            value: values + r.u16(at + OFFSET)? as usize,
        });
        at += ENTRY;
    }
    Ok(out)
}

/// Where the four floats of `material`'s parameter `hash` are.
fn locate_in(data: &[u8], material: usize, hash: u64) -> Result<usize> {
    let entry = entries(data, material)?
        .into_iter()
        .find(|e| e.hash == hash);
    let entry = entry.with_context(|| format!("no parameter {hash:016x}"))?;
    ensure!(entry.kind == FLOAT4, "not four floats");
    Ok(entry.value)
}

/// Where the value of the parameter whose name hashes to `hash` is, in a
/// model of one material.
fn locate(data: &[u8], hash: u64) -> Result<usize> {
    let mut found = starts(data);
    let material = found.next().context("no material")?;
    ensure!(found.next().is_none(), "more than one material");
    locate_in(data, material, hash)
}

/// Where the value of the parameter `hash` is in every material named
/// `name`. A model may hold one name more than once (a level of detail).
fn locate_named(data: &[u8], name: &str, hash: u64) -> Result<Vec<usize>> {
    let named = starts(data).filter(|&at| name_at(data, at + NAME) == Some(name.as_bytes()));
    let found: Vec<_> = named
        .map(|at| locate_in(data, at, hash))
        .collect::<Result<_>>()?;
    ensure!(!found.is_empty(), "no material {name}");
    Ok(found)
}

fn floats(data: &[u8], at: usize) -> Result<[f32; 4]> {
    let bytes = data.get(at..at + 16).context("a value past the end")?;
    Ok(std::array::from_fn(|i| {
        f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().expect("four bytes"))
    }))
}

fn write(data: &[u8], at: &[usize], value: [f32; 4]) -> Result<Vec<u8>> {
    let mut out = data.to_vec();
    for &at in at {
        let slot = out.get_mut(at..at + 16).context("a value past the end")?;
        for (i, v) in value.iter().enumerate() {
            slot[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    Ok(out)
}

/// The four floats of the parameter whose name hashes to `hash`, in a
/// model of one material.
pub fn param(data: &[u8], hash: u64) -> Result<[f32; 4]> {
    floats(data, locate(data, hash)?)
}

/// `data` with the parameter whose name hashes to `hash` set to `value`,
/// in a model of one material.
pub fn set_param(data: &[u8], hash: u64, value: [f32; 4]) -> Result<Vec<u8>> {
    write(data, &[locate(data, hash)?], value)
}

/// The parameter `hash` of the material named `material`: its first copy.
pub fn material_param(data: &[u8], material: &str, hash: u64) -> Result<[f32; 4]> {
    floats(data, locate_named(data, material, hash)?[0])
}

/// `data` with the parameter `hash` set to `value` in every material
/// named `material`.
pub fn set_material_param(
    data: &[u8],
    material: &str,
    hash: u64,
    value: [f32; 4],
) -> Result<Vec<u8>> {
    write(data, &locate_named(data, material, hash)?, value)
}
