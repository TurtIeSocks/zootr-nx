//! Vfx3 effect bundles (`Effect/*.vsetb.byml`): the color constants in
//! their particle blob, and the effect set (eset) each one sits in.
//!
//! The blob (`PtclBin`, magic `BFFH`) is undocumented. Every pointer in it
//! is a u64 from its start, so it's only ever overwritten in place. A
//! property is a 16-byte descriptor, `[u64 name hash][u32 kind][u32 type]`,
//! and its value follows. A color constant has type `0x0c0d`, a kind whose
//! low byte is `0x09`, and four floats `(r, g, b, 0)`, linear and often
//! above 1.

use std::collections::HashMap;
use std::ops::Range;

use anyhow::{Context, Result, ensure};
use roead::byml::Byml;

use crate::reader::Reader;

/// A color constant's descriptor type, and its kind's low byte.
const CONSTANT_TYPE: u32 = 0x0c0d;
const CONSTANT_KIND: u32 = 0x09;
/// Where the blob's header gives the end of its data section, which the
/// string pool follows. The section starts this far in.
const DATA_END: usize = 0xb0;
const DATA_START: usize = 0x1000;

/// One color constant: where its value is in the file, its descriptor's
/// name hash, and its four floats.
#[derive(Clone, Debug, PartialEq)]
pub struct Constant {
    pub at: usize,
    pub hash: u64,
    pub value: [f32; 4],
}

/// Where the blob is in a decompressed bundle.
pub fn blob(data: &[u8]) -> Result<Range<usize>> {
    let at = data
        .windows(4)
        .position(|w| w == b"BFFH")
        .context("no Vfx3 blob")?;
    let size = Reader(data).u32(at + 8)? as usize;
    ensure!(at + size <= data.len(), "the Vfx3 blob runs past the file");
    Ok(at..at + size)
}

/// Every color constant in a decompressed bundle, in file order.
pub fn constants(data: &[u8]) -> Result<Vec<Constant>> {
    let (r, blob) = (Reader(data), blob(data)?);
    let end = data_end(data, &blob)?.saturating_sub(32);
    let mut out = Vec::new();
    for at in (blob.start + DATA_START..end).step_by(8) {
        if r.u32(at + 12)? != CONSTANT_TYPE || r.u32(at + 8)? & 0xff != CONSTANT_KIND {
            continue;
        }
        out.push(Constant {
            at: at + 16,
            hash: r.u64(at)?,
            value: [
                r.f32(at + 16)?,
                r.f32(at + 20)?,
                r.f32(at + 24)?,
                r.f32(at + 28)?,
            ],
        });
    }
    Ok(out)
}

/// Each eset the bundle lists, with where its data starts in the file,
/// sorted by that. A constant belongs, roughly, to the last eset starting
/// at or before it: esets lie one after another, but emitters can be
/// shared, so the bounds are approximate.
pub fn esets(data: &[u8]) -> Result<Vec<(String, usize)>> {
    let blob = blob(data)?;
    let names = eset_names(data)?;
    let end = data_end(data, &blob)?;
    // Each name's place in the string pool, as the blob points at it.
    let pool = &data[end..blob.end];
    let mut by_place: HashMap<u64, &str> = HashMap::new();
    for name in &names {
        let needle = [&[0][..], name.as_bytes(), &[0]].concat();
        if let Some(i) = pool.windows(needle.len()).position(|w| w == needle) {
            by_place.insert((end - blob.start + i + 1) as u64, name);
        }
    }
    let r = Reader(data);
    let mut out = Vec::new();
    // An eset's record is its name's place, then its emitters'.
    for at in (blob.start + DATA_START..end).step_by(8) {
        if let Some(name) = by_place.remove(&r.u64(at)?) {
            out.push((name.to_owned(), blob.start + r.ptr(at + 8)?));
        }
    }
    out.sort_by_key(|(_, at)| *at);
    Ok(out)
}

/// Every color constant in a decompressed bundle with the eset it falls
/// in (see `esets`), none for one before the first.
pub fn constants_by_eset(data: &[u8]) -> Result<Vec<(Option<String>, Constant)>> {
    let esets = esets(data)?;
    let eset = |at| esets.iter().rev().find(|(_, start)| *start <= at);
    Ok(constants(data)?
        .into_iter()
        .map(|c| (eset(c.at).map(|(name, _)| name.clone()), c))
        .collect())
}

/// The eset names a bundle's BYML lists.
fn eset_names(data: &[u8]) -> Result<Vec<String>> {
    let doc = Byml::from_binary(data)?;
    let esets = doc.as_map()?.get("Esets").context("no Esets")?.as_array()?;
    esets
        .iter()
        .map(|e| Ok(e.as_string()?.to_string()))
        .collect()
}

/// Where the blob's data section ends, in the file.
fn data_end(data: &[u8], blob: &Range<usize>) -> Result<usize> {
    let end = blob.start + DATA_START + Reader(data).offset(blob.start + DATA_END)?;
    ensure!(end <= blob.end, "the Vfx3 data runs past the blob");
    Ok(end)
}
