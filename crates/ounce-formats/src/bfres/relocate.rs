//! The memory pool and what moves when it grows: the relocation table, the
//! blocks' sizes and links, and the pointers into the pool's far side.

use anyhow::{Context, Result, ensure};

use crate::reader::Reader;

/// Where the header keeps its first block (u16) and the relocation table
/// (u32).
const FIRST_BLOCK: usize = 0x16;
const RELOCATIONS: usize = 0x18;
/// Where the header points at the memory pool's info: its size, then
/// where its buffers start.
const POOL_INFO: usize = 0xb0;

/// The memory pool: where its info header is, and where the pool starts
/// and how big it is.
pub(super) struct Pool {
    pub info: usize,
    pub start: usize,
    pub size: usize,
}

impl Pool {
    pub(super) fn read(data: &[u8]) -> Result<Self> {
        let r = Reader(data);
        let info = r.ptr(POOL_INFO)?;
        Ok(Self {
            info,
            start: r.ptr(info + 8)?,
            size: r.offset(info + 4)?,
        })
    }
}

/// Every pointer the relocation table lists: where it is, and where it
/// points.
pub fn pointers(data: &[u8]) -> Result<Vec<(usize, usize)>> {
    let r = Reader(data);
    let table = Table::find(data)?;
    let mut out = Vec::new();
    for e in 0..table.entries {
        let entry = table.entry(e);
        let structs = r.u16(entry + 4)?;
        let (count, padding) = (r.u8(entry + 6)?, r.u8(entry + 7)? as usize);
        let mut p = r.offset(entry)?;
        for _ in 0..structs {
            for _ in 0..count {
                out.push((p, r.ptr(p)?));
                p += 8;
            }
            p += 8 * padding;
        }
    }
    Ok(out)
}

/// The relocation table: where it is, how many sections and entries.
struct Table {
    at: usize,
    sections: usize,
    entries: usize,
}

impl Table {
    fn find(data: &[u8]) -> Result<Self> {
        let r = Reader(data);
        let at = r.offset(RELOCATIONS)?;
        ensure!(data.get(at..at + 4) == Some(b"_RLT"), "no relocation table");
        let sections = r.offset(at + 8)?;
        let mut entries = 0;
        for s in 0..sections {
            let section = at + 0x10 + 0x18 * s;
            entries = entries.max(r.offset(section + 16)? + r.offset(section + 20)?);
        }
        Ok(Self {
            at,
            sections,
            entries,
        })
    }

    fn section(&self, s: usize) -> usize {
        self.at + 0x10 + 0x18 * s
    }

    fn entry(&self, e: usize) -> usize {
        self.at + 0x10 + 0x18 * self.sections + 8 * e
    }
}

/// Moves the relocation table's sections, entries and pointers for
/// `growth` bytes inserted at `at`. The table follows the pool, so it has
/// moved too: the header learns where.
pub(super) fn relocate(out: &mut [u8], at: usize, growth: usize) -> Result<()> {
    let old = Reader(out).offset(RELOCATIONS)?;
    ensure!(old >= at, "the relocation table is before the pool's end");
    put(out, RELOCATIONS, old + growth)?;
    let table = Table::find(out)?;
    put(out, table.at + 4, table.at)?;
    for s in 0..table.sections {
        let section = table.section(s);
        let r = Reader(out);
        let (offset, size) = (r.offset(section + 8)?, r.offset(section + 12)?);
        if offset >= at {
            put(out, section + 8, offset + growth)?;
        } else if offset + size >= at {
            put(out, section + 12, size + growth)?;
        }
    }
    for e in 0..table.entries {
        let location = Reader(out).offset(table.entry(e))?;
        if location >= at {
            put(out, table.entry(e), location + growth)?;
        }
    }
    for (p, to) in pointers(out)? {
        if to >= at {
            out[p..p + 8].copy_from_slice(&((to + growth) as u64).to_le_bytes());
        }
    }
    Ok(())
}

/// Each block whose span holds `at` grown by `growth`, and each link to a
/// block past it lengthened.
pub(super) fn grow_blocks(out: &mut [u8], at: usize, growth: usize) -> Result<()> {
    let mut block = Reader(out).u16(FIRST_BLOCK)? as usize;
    while block != 0 {
        let r = Reader(out);
        let (next, size) = (r.offset(block + 4)?, r.offset(block + 8)?);
        if block < at && block + size >= at {
            put(out, block + 8, size + growth)?;
        }
        if next == 0 {
            break;
        }
        let to = block + next;
        if block < at && to >= at {
            put(out, block + 4, next + growth)?;
        }
        block = if to >= at { to + growth } else { to };
    }
    Ok(())
}

/// `out` padded with zeros to a multiple of `align`.
pub(super) fn pad(out: &mut Vec<u8>, align: usize) {
    out.resize(out.len().next_multiple_of(align), 0);
}

/// `value` written as a u32 at `at`.
pub(super) fn put(out: &mut [u8], at: usize, value: usize) -> Result<()> {
    let value = u32::try_from(value).context("past u32")?;
    out.get_mut(at..at + 4)
        .context("past the end")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
