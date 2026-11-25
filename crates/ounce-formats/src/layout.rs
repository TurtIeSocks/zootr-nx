//! UI layouts (`.buil` in `UI/Layout2Archive/*.blarc`): the colour
//! palette each one keeps.
//!
//! A layout is a `BFFH` file. Its palette is a table of RGBA floats whose
//! header, at 0x2c0, is `[u64 entries][u32 count][u32 stride]`. A material
//! names its colours by palette index (its white colour at +0x28 tints a
//! one-channel texture whole), so an entry is written in place, same size.
//!
//! An entry no record names isn't free: the item menu's count digits glow
//! with one, picked by the game or an animation by index. So a material
//! can't safely be pointed at an "unused" entry.

use anyhow::{Result, ensure};

use crate::reader::Reader;

/// Where the palette's header is.
const PALETTE: usize = 0x2c0;
/// The size of one entry: four floats.
const STRIDE: usize = 16;
/// Where the root record starts.
const ROOT: usize = 0xf8;
/// The root's dictionary of materials.
const MATERIALS: usize = ROOT + 0x40;
/// A material's white index, which tints its texture. Its four corners'
/// indices come before it, its black one after.
const WHITE: usize = 0x28;

/// One palette entry: where its floats are in the layout, and their value.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub at: usize,
    pub rgba: [f32; 4],
}

/// Every entry in `layout`'s palette, in order.
pub fn palette(layout: &[u8]) -> Result<Vec<Entry>> {
    let r = Reader(layout);
    ensure!(r.bytes(0, 4)? == b"BFFH", "not a UI layout");
    let start = r.ptr(PALETTE)?;
    let count = r.u32(PALETTE + 8)? as usize;
    let stride = r.u32(PALETTE + 12)? as usize;
    ensure!(stride == STRIDE, "a palette stride of {stride}");
    (0..count)
        .map(|i| {
            let at = start + i * STRIDE;
            let rgba = [r.f32(at)?, r.f32(at + 4)?, r.f32(at + 8)?, r.f32(at + 12)?];
            Ok(Entry { at, rgba })
        })
        .collect()
}

/// A material: its name, and where its white index is in the layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub white: usize,
}

/// Every material in `layout`, in order.
pub fn materials(layout: &[u8]) -> Result<Vec<Material>> {
    let r = Reader(layout);
    let named = dict(&r, MATERIALS)?;
    let material = |(name, at)| Material {
        name,
        white: at + WHITE,
    };
    Ok(named.into_iter().map(material).collect())
}

/// The root's dictionaries of fixed-size records: each one's kind, its
/// offset in the root, and its records' size.
pub const RECORDS: [(&str, usize, usize); 5] = [
    ("pane", 0x20, 0x44),
    ("material", 0x40, 0x40),
    ("picture", 0x60, 0x18),
    ("text", 0x80, 0xa8),
    ("part", 0xc0, 0x48),
];

/// The root's tables of u32s, `{ptr, count, 4}`: their offsets in the
/// root. What they hold isn't known; +0x208 has one word per material.
pub const WORD_TABLES: [usize; 2] = [0x1d8, 0x208];

/// The words in the root's table at `slot` (see `WORD_TABLES`).
pub fn words(layout: &[u8], slot: usize) -> Result<Vec<u32>> {
    let r = Reader(layout);
    let (start, count) = (r.ptr(ROOT + slot)?, r.u32(ROOT + slot + 8)? as usize);
    (0..count).map(|i| r.u32(start + 4 * i)).collect()
}

/// Each record in the root's dictionary at `slot` (see `RECORDS`): its
/// name and where it is.
pub fn records(layout: &[u8], slot: usize) -> Result<Vec<(String, usize)>> {
    dict(&Reader(layout), ROOT + slot)
}

/// A dictionary's names and where each record is: its header is
/// `[u64 names][u64 records][u64 hashes][u32 count][u32 stride]`.
fn dict(r: &Reader, at: usize) -> Result<Vec<(String, usize)>> {
    let (names, records) = (r.ptr(at)?, r.ptr(at + 8)?);
    let count = r.u32(at + 24)? as usize;
    ensure!(r.u32(at + 28)? == 8, "a dictionary stride other than 8");
    (0..count)
        .map(|i| {
            let name = r.ptr(names + 8 * i)?;
            let len = r.0[name..].iter().position(|&b| b == 0).unwrap_or(0);
            let name = String::from_utf8_lossy(r.bytes(name, len)?).into_owned();
            Ok((name, r.ptr(records + 8 * i)?))
        })
        .collect()
}
