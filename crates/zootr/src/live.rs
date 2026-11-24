//! GameData values read and written live in the running game, from
//! outside the game. Plain memory access, no hooks. Layout in `docs/research/live-flags.md`.
//! Reading the table is here; `game` attaches to the running game and `poll`
//! watches it for changes.

use std::collections::BTreeSet;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, bail};

use ounce_formats::KEYED;
use zootr_hook::keys::{KEY_STRIDE, KEY_VALUE, KEYS, find};

use crate::gamedata::GameData;
use crate::known::field;
use crate::savefile::{self, Change};
use crate::value;

/// Where a type keeps its current value in an entry.
#[derive(Clone, Copy)]
enum Shape {
    /// This many bytes at an offset.
    Inline(usize, usize),
    /// A `(u32 capacity, pointer, u64 length)` triple at an offset.
    Blob(usize),
    /// A `(u64 length, pointer)` pair at an offset, to elements this wide
    /// whose value starts this far in.
    List(usize, usize, usize),
}
use Shape::*;

/// A type's manager slot, name, entry stride and current value.
type Row = (u64, &'static str, usize, Shape);

/// The types GameData holds. BoolArray and BinaryArray are left out until
/// their layout is confirmed.
static TYPES: LazyLock<[Row; 21]> = LazyLock::new(|| {
    [
        (0, "Bool", 0x18, Inline(0x16, 1)),
        (2, "Int", 0x28, Inline(0x1c, 4)),
        (3, field::INT_ARRAY, 0x40, List(0x20, 4, 0)),
        (4, "Float", 0x28, Inline(0x1c, 4)),
        (5, field::FLOAT_ARRAY, 0x40, List(0x20, 4, 0)),
        (6, "Enum", 0x38, Inline(0x1c, 4)),
        (7, field::ENUM_ARRAY, 0x50, List(0x20, 4, 0)),
        (10, "Vector3", 0x40, Inline(0x24, 12)),
        (12, "String16", 0x78, Inline(0x44, 16)),
        (14, "String32", 0xa8, Inline(0x54, 32)),
        (16, "String64", 0x108, Inline(0x74, 64)),
        (17, field::STRING64_ARRAY, 0x90, List(0x70, 0x50, 0xc)),
        (18, "Binary", 0x48, Blob(0x18)),
        (20, "UInt", 0x28, Inline(0x1c, 4)),
        (21, field::U_INT_ARRAY, 0x40, List(0x20, 4, 0)),
        (22, "Int64", 0x30, Inline(0x20, 8)),
        (23, field::INT64_ARRAY, 0x48, List(0x28, 8, 0)),
        (24, field::U_INT64, 0x30, Inline(0x20, 8)),
        (25, field::U_INT64_ARRAY, 0x48, List(0x28, 8, 0)),
        (26, field::W_STRING16, 0xa8, Inline(0x54, 32)),
        (27, field::W_STRING16_ARRAY, 0x70, List(0x50, 0x30, 0xc)),
    ]
});
/// Where the type tables start in the manager, and each one's size.
const TABLES: u64 = 8;
const TABLE: u64 = 0x60;
/// A table's header, from the table's start: its entry count, the pointer
/// to its entries, the pointer to its map and the map's capacity.
const COUNT: usize = 8;
const ENTRIES: usize = 0x10;
const MAP: usize = 0x18;
const MAP_CAP: usize = 0x24;
/// More bytes than this in one read means the layout is off.
const MAX: usize = 1 << 24;
/// How often [`Poll`] is meant to be called.
pub const TICK: Duration = Duration::from_millis(100);
/// More changes than this in [`WINDOW`] mutes a value.
pub const BURST: u32 = 5;
pub const WINDOW: Duration = Duration::from_secs(10);

/// A running game's memory: read directly through Mach, or another
/// backend.
pub trait Memory {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>>;

    fn write(&self, at: u64, data: &[u8]) -> Result<()>;

    /// Each range's bytes: over a bridge, in as few requests as it takes.
    fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        ranges.iter().map(|&(at, len)| self.read(at, len)).collect()
    }

    /// Writes each of `writes`, then reads `reads`: over a bridge, all in
    /// one go, where they may land in any order. So no write may overlap
    /// another, and no read may depend on a write.
    fn batch(&self, writes: &[(u64, &[u8])], reads: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        in_order(self, writes, reads)
    }

    /// Where values were found, when this memory keeps them.
    fn places(&self) -> Option<&Places> {
        None
    }

    fn u32(&self, at: u64) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read(at, 4)?.try_into().unwrap()))
    }

    fn u64(&self, at: u64) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read(at, 8)?.try_into().unwrap()))
    }
}

/// [`Memory::batch`] one step at a time, in order.
fn in_order(
    mem: &(impl Memory + ?Sized),
    writes: &[(u64, &[u8])],
    reads: &[(u64, usize)],
) -> Result<Vec<Vec<u8>>> {
    for &(at, data) in writes {
        mem.write(at, data)?;
    }
    mem.read_many(reads)
}

impl<M: Memory + ?Sized> Memory for &M {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        (**self).read(at, len)
    }

    fn write(&self, at: u64, data: &[u8]) -> Result<()> {
        (**self).write(at, data)
    }

    fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        (**self).read_many(ranges)
    }

    fn batch(&self, writes: &[(u64, &[u8])], reads: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        (**self).batch(writes, reads)
    }

    fn places(&self) -> Option<&Places> {
        (**self).places()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub kind: &'static str,
    pub hash: u32,
    /// The value's bytes, one item per element for arrays.
    pub items: Vec<Vec<u8>>,
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn quad(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// The global holding the GameData manager, found through the inlined bool
/// getter in `code`, which is loaded at `base`.
pub fn global(code: &[u8], base: u64) -> Option<u64> {
    let w = |at| word(code, at);
    (20..=code.len().checked_sub(28)?).step_by(4).find_map(|p| {
        let ldr = w(p);
        let getter = ldr & 0xffc0_03ff == 0xf940_0108
            && w(p + 4) == 0xf940_0108
            && w(p + 8) >> 24 == 0x37
            && w(p + 12) == 0xf940_1109
            && w(p + 16) >> 24 == 0xb4
            && w(p + 20) == 0xb940_2d0a
            && w(p + 24) >> 24 == 0x34;
        if !getter {
            return None;
        }
        let at = (p - 20..p)
            .step_by(4)
            .rev()
            .find(|&a| w(a) & 0x9f00_001f == 0x9000_0008)?;
        let adrp = w(at);
        let pages = ((adrp >> 29) & 3) | (((adrp >> 5) & 0x7ffff) << 2);
        let pages = i64::from(((pages << 11) as i32) >> 11);
        let page = ((base + at as u64) & !0xfff).wrapping_add_signed(pages << 12);
        Some(page + u64::from((ldr >> 10) & 0xfff) * 8)
    })
}

/// GameData's manager, given the global from [`global`].
fn manager(mem: &impl Memory, global: u64) -> Result<u64> {
    mem.u64(mem.u64(global)?)
}

/// Where `row`'s table sits in the manager.
fn table(manager: u64, row: &Row) -> u64 {
    manager + TABLES + TABLE * row.0
}

/// Where `row`'s table keeps the pointer to its entries.
fn entries_pointer(manager: u64, row: &Row) -> u64 {
    table(manager, row) + ENTRIES as u64
}

/// Every mapped value, in manager order, given the global from [`global`].
pub fn snapshot(mem: &impl Memory, global: u64) -> Result<Vec<Value>> {
    values(mem, manager(mem, global)?)
}

/// The per-placement flags (Bool64bitKey) that are true, given the global
/// from [`global`].
pub fn keys(mem: &impl Memory, global: u64) -> Result<BTreeSet<u64>> {
    keys_of(mem, manager(mem, global)?)
}

/// Where the per-placement flags' entries sit, and all of them
/// ([`zootr_hook::keys`]).
fn key_entries(mem: &impl Memory, manager: u64) -> Result<(u64, Vec<u8>)> {
    let count = mem.u32(manager + KEYS as u64)? as usize;
    let at = mem.u64(manager + KEYS as u64 + 8)?;
    Ok((at, mem.read(at, sized(KEYED, count * KEY_STRIDE)?)?))
}

fn keys_of(mem: &impl Memory, manager: u64) -> Result<BTreeSet<u64>> {
    let (_, entries) = key_entries(mem, manager)?;
    let (entries, _) = entries.as_chunks::<KEY_STRIDE>();
    Ok((entries.iter())
        .filter(|e| e[KEY_VALUE] != 0)
        .map(|e| quad(e, 0))
        .collect())
}

/// Where placement `key`'s own flag keeps its value, and that value, given
/// the global from [`global`]. `None` when GameData has no entry for it.
pub fn key_value(mem: &impl Memory, global: u64, key: u64) -> Result<Option<(u64, u8)>> {
    let (at, entries) = key_entries(mem, manager(mem, global)?)?;
    Ok(find(&entries, key).map(|i| {
        let entry = i * KEY_STRIDE;
        (at + (entry + KEY_VALUE) as u64, entries[entry + KEY_VALUE])
    }))
}

/// Every value in the type tables, in three requests over a bridge: the
/// table headers, every map and entry list, then every value kept outside
/// its entry.
fn values(mem: &impl Memory, manager: u64) -> Result<Vec<Value>> {
    let slots = TYPES.iter().map(|row| row.0).max().unwrap_or(0) + 1;
    let heads = mem.read(manager + TABLES, (TABLE * slots) as usize)?;
    let mut lists = Vec::new();
    for row in TYPES.iter() {
        let &(_, kind, stride, _) = row;
        let head = &heads[(TABLE * row.0) as usize..];
        let (count, cap) = (word(head, COUNT) as usize, word(head, MAP_CAP) as usize);
        lists.push((quad(head, MAP), sized(kind, cap * 8)?));
        lists.push((quad(head, ENTRIES), sized(kind, count * stride)?));
    }
    let lists = mem.read_many(&lists)?;
    let mut found = Vec::new();
    for (row, [map, entries]) in TYPES.iter().zip(lists.as_chunks::<2>().0) {
        let &(_, kind, stride, shape) = row;
        let entries = entries.chunks_exact(stride);
        let mut hashes = vec![0; entries.len()];
        for pair in map.as_chunks::<8>().0 {
            let (hash, i) = (word(pair, 0), word(pair, 4) as usize);
            if hash != 0 && i < hashes.len() {
                hashes[i] = hash;
            }
        }
        for (e, hash) in entries.zip(hashes).filter(|(_, hash)| *hash != 0) {
            found.push((kind, hash, e, shape, outside(kind, e, shape)?));
        }
    }
    let ranges: Vec<_> = (found.iter()).filter_map(|f| f.4).collect();
    let mut kept = mem.read_many(&ranges)?.into_iter();
    (found.into_iter())
        .map(|(kind, hash, e, shape, at)| {
            let bytes = if at.is_some() {
                kept.next().unwrap_or_default()
            } else {
                Vec::new()
            };
            let items = value_of(kind, e, shape, &bytes);
            Ok(Value { kind, hash, items })
        })
        .collect()
}

fn sized(kind: &str, len: usize) -> Result<usize> {
    if len > MAX {
        bail!("a {kind} read of {len} bytes; is this the 1.1.1 update?");
    }
    Ok(len)
}

/// The current value of entry `e`.
fn items(mem: &impl Memory, kind: &str, e: &[u8], shape: Shape) -> Result<Vec<Vec<u8>>> {
    let bytes = match outside(kind, e, shape)? {
        Some((at, len)) => mem.read(at, len)?,
        None => Vec::new(),
    };
    Ok(value_of(kind, e, shape, &bytes))
}

/// Where entry `e` keeps its value, when that's outside the entry.
fn outside(kind: &str, e: &[u8], shape: Shape) -> Result<Option<(u64, usize)>> {
    Ok(match shape {
        Inline(..) => None,
        Blob(at) => Some((quad(e, at + 8), sized(kind, quad(e, at + 0x10) as usize)?)),
        List(at, width, _) => Some((quad(e, at + 8), sized(kind, quad(e, at) as usize * width)?)),
    })
}

/// Entry `e`'s value, given the `bytes` that [`outside`] points at.
fn value_of(kind: &str, e: &[u8], shape: Shape, bytes: &[u8]) -> Vec<Vec<u8>> {
    match shape {
        Inline(at, len) => vec![text(kind, &e[at..at + len])],
        Blob(_) => vec![bytes.to_vec()],
        List(_, width, skip) => (bytes.chunks_exact(width))
            .map(|x| text(kind, &x[skip..]))
            .collect(),
    }
}

/// Text cut at its terminator, so leftovers past it don't count as changes.
/// Other values pass through.
fn text(kind: &str, b: &[u8]) -> Vec<u8> {
    let end = if kind.starts_with("WString") {
        b.as_chunks::<2>()
            .0
            .iter()
            .position(|&c| c == [0, 0])
            .map(|i| i * 2)
    } else if kind.starts_with("String") {
        b.iter().position(|&c| c == 0)
    } else {
        None
    };
    b[..end.unwrap_or(b.len())].to_vec()
}

fn show(gd: &GameData, kind: &str, b: &[u8]) -> String {
    let text = value::decode(gd, kind, b);
    match value::input(kind) {
        value::Input::Text => format!("{text:?}"),
        value::Input::Vector => format!("({})", text.replace(',', ", ")),
        _ => text,
    }
}

/// Each value that changed from `was` to `now` and `wanted` keeps. Arrays
/// get a row per changed element.
pub fn changes(
    gd: &GameData,
    was: &[Value],
    now: &[Value],
    mut wanted: impl FnMut(u32) -> bool,
) -> Vec<Change> {
    let mut out = Vec::new();
    for (a, b) in was.iter().zip(now) {
        if a.hash != b.hash || a.items == b.items || !wanted(b.hash) {
            continue;
        }
        let kind = b.kind;
        let row = |change: String| Change::new(gd, b.hash, kind, change);
        let moved = |x: &[u8], y: &[u8]| format!("{} -> {}", show(gd, kind, x), show(gd, kind, y));
        if kind == "Binary" {
            out.push(row(savefile::bytes(&a.items[0], &b.items[0])));
        } else if !kind.ends_with("Array") {
            out.push(row(moved(&a.items[0], &b.items[0])));
        } else {
            let (n, m) = (a.items.len(), b.items.len());
            if n != m {
                out.push(row(format!("{n} -> {m} items")));
            }
            out.extend(
                a.items
                    .iter()
                    .zip(&b.items)
                    .enumerate()
                    .filter(|(_, (x, y))| x != y)
                    .map(|(i, (x, y))| row(moved(x, y)).at(i)),
            );
        }
    }
    out
}

/// `[mm:ss]` for a time in seconds.
pub fn stamp(secs: u64) -> String {
    format!("[{:02}:{:02}]", secs / 60, secs % 60)
}

/// The processes started from `app`.
pub(crate) use mach::pids;

pub mod actors;
mod bridge;
mod game;
mod lookup;
mod mach;
#[cfg(windows)]
mod pipe;
mod poll;
pub mod snap;

pub use bridge::{Bridge, CLOSED, GONE, PATIENCE, Refused};
pub use game::{Game, NATIVE_MAIN, NotSigned, SLOW, STARTING, Unsupported};
pub use lookup::{Places, bools, hash, locate, read, set, value, value_with};
pub use poll::{Noise, Poll};

#[cfg(test)]
pub(crate) mod tests {
    use anyhow::Context;

    use super::*;

    /// Memory in mapped blocks, each a start and its bytes.
    pub(crate) struct Fake(std::cell::RefCell<Vec<(u64, Vec<u8>)>>);

    impl Fake {
        pub(crate) fn new(blocks: Vec<(u64, Vec<u8>)>) -> Self {
            Self(std::cell::RefCell::new(blocks))
        }

        /// The block holding `at`, and how far into it `at` is.
        fn find<T>(&self, at: u64, f: impl FnOnce(&mut Vec<u8>, usize) -> Result<T>) -> Result<T> {
            let mut blocks = self.0.borrow_mut();
            let (start, bytes) = (blocks.iter_mut())
                .find(|(s, b)| (*s..*s + b.len() as u64).contains(&at))
                .context("unmapped")?;
            let from = (at - *start) as usize;
            f(bytes, from)
        }
    }

    impl Memory for Fake {
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            if len == 0 {
                return Ok(Vec::new());
            }
            self.find(at, |bytes, from| {
                Ok(bytes.get(from..from + len).context("short")?.to_vec())
            })
        }

        fn write(&self, at: u64, data: &[u8]) -> Result<()> {
            self.find(at, |bytes, from| {
                (bytes.get_mut(from..from + data.len()).context("short")?).copy_from_slice(data);
                Ok(())
            })
        }
    }

    fn put(b: &mut [u8], at: usize, v: &[u8]) {
        b[at..at + v.len()].copy_from_slice(v);
    }

    /// A game whose tables hold these entries. Each map lists its entries
    /// backwards, so a snapshot has to go by the map. Pointers in entries
    /// lead into `heap`.
    pub(crate) fn game(tables: &[(u64, Vec<Vec<u8>>)], heap: &[(u64, Vec<u8>)]) -> Fake {
        keyed(tables, heap, &[])
    }

    /// [`game`], with these per-placement flags and whether each is set.
    pub(crate) fn keyed(
        tables: &[(u64, Vec<Vec<u8>>)],
        heap: &[(u64, Vec<u8>)],
        keys: &[(u64, bool)],
    ) -> Fake {
        let mut manager = vec![0; KEYS + 0x10];
        let at = 0x80000u64;
        put(&mut manager, KEYS, &(keys.len() as u32).to_le_bytes());
        put(&mut manager, KEYS + 8, &at.to_le_bytes());
        let entries = (keys.iter()).flat_map(|&(k, on)| {
            let mut e = [0u8; KEY_STRIDE];
            put(&mut e, 0, &k.to_le_bytes());
            e[KEY_VALUE] = on.into();
            e
        });
        let mut mem = vec![
            (0x1000, 0x2000u64.to_le_bytes().into()),
            (0x2000, 0x3000u64.to_le_bytes().into()),
        ];
        for (n, (slot, entries)) in tables.iter().enumerate() {
            let (table, at) = (8 + 0x60 * *slot as usize, 0x10000 * (n as u64 + 1));
            let count = entries.len() as u32;
            put(&mut manager, table + 8, &count.to_le_bytes());
            put(&mut manager, table + 0x10, &at.to_le_bytes());
            put(&mut manager, table + 0x18, &(at + 0x8000).to_le_bytes());
            put(&mut manager, table + 0x24, &(count * 2).to_le_bytes());
            let map: Vec<u8> = (0..count)
                .rev()
                .flat_map(|i| [hash(*slot, i), i])
                .chain((0..count * 2).map(|_| 0))
                .flat_map(u32::to_le_bytes)
                .collect();
            mem.push((at, entries.concat()));
            mem.push((at + 0x8000, map));
        }
        mem.push((0x3000, manager));
        mem.push((at, entries.collect()));
        mem.extend(heap.iter().cloned());
        Fake::new(mem)
    }

    pub(crate) fn hash(slot: u64, i: u32) -> u32 {
        0xa000 | (slot as u32) << 4 | i
    }

    /// An entry of `slot`'s type with these bytes set.
    pub(crate) fn entry(slot: u64, fields: &[(usize, &[u8])]) -> Vec<u8> {
        let (_, _, stride, _) = TYPES.iter().find(|t| t.0 == slot).unwrap();
        let mut e = vec![0; *stride];
        for (at, v) in fields {
            put(&mut e, *at, v);
        }
        e
    }

    pub(crate) fn ints(values: [u32; 2]) -> Fake {
        let e = values.map(|v| entry(2, &[(0x1c, &v.to_le_bytes())]));
        game(&[(2, e.into())], &[])
    }

    #[test]
    fn the_getter_pattern_resolves_its_global() {
        let base = 0x1_0000_0000;
        // adrp x8, +3 pages (immlo 3, immhi 0); ldr x8, [x8, #0x180].
        let adrp = 0x9000_0008 | 3 << 29;
        let ldr = 0xf940_0108 | (0x180 / 8) << 10;
        let code = [
            0xd503_201f, // nop, so the pattern isn't at the very start
            0xd503_201f,
            0xd503_201f,
            0xd503_201f,
            adrp,
            ldr,
            0xf940_0108,
            0x3700_0009,
            0xf940_1109,
            0xb400_0009,
            0xb940_2d0a,
            0x3400_000a,
        ]
        .map(u32::to_le_bytes)
        .concat();
        assert_eq!(global(&code, base), Some(base + 0x3000 + 0x180));
        assert_eq!(global(&code[..40], base), None);
    }

    #[test]
    fn a_snapshot_names_values_by_their_map_slot() {
        let values = snapshot(&ints([5, 7]), 0x1000).unwrap();
        let int = |i, raw: u32| Value {
            kind: "Int",
            hash: hash(2, i),
            items: vec![raw.to_le_bytes().into()],
        };
        assert_eq!(values, [int(0, 5), int(1, 7)]);
    }

    #[test]
    fn changes_list_what_moved() {
        let gd = GameData::default();
        let was = snapshot(&ints([5, 7]), 0x1000).unwrap();
        let now = snapshot(&ints([5, 8]), 0x1000).unwrap();
        let change = Change::new(&gd, hash(2, 1), "Int", "7 -> 8");
        assert_eq!(changes(&gd, &was, &now, |_| true), [change]);
        assert!(changes(&gd, &was, &now, |h| h != hash(2, 1)).is_empty());
    }

    /// One entry of every shape, with values that differ between `n` 0 and 1.
    fn shapes(n: usize) -> Fake {
        let pick = |a: &'static [u8], b: &'static [u8]| [a, b][n];
        let floats = |z: f32| [1f32, 2.0, z].map(f32::to_le_bytes).concat();
        let utf16 = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<u8>>()
        };
        let mut slot = vec![0; 0x50];
        put(&mut slot, 0xc, pick(b"a", b"b"));
        let list = |len: u64, at: u64| [len, at].map(u64::to_le_bytes).concat();
        game(
            &[
                (0, vec![entry(0, &[(0x16, pick(&[0], &[1]))])]),
                (3, vec![entry(3, &[(0x20, &list(3, 0x171000))])]),
                (10, vec![entry(10, &[(0x24, &floats([3.0, 4.0][n]))])]),
                (
                    16,
                    vec![
                        entry(16, &[(0x74, pick(b"Kokiri\0xx", b"\0okiri\0yy"))]),
                        entry(16, &[(0x74, pick(b"\0abc", b"\0xyz"))]),
                    ],
                ),
                (17, vec![entry(17, &[(0x70, &list(1, 0x172000))])]),
                (18, vec![entry(18, &[(0x20, &list(0x170000, 4))])]),
                (22, vec![entry(22, &[(0x20, &[-1i64, 5][n].to_le_bytes())])]),
                (26, vec![entry(26, &[(0x54, &utf16(["Hi", "Ho"][n]))])]),
            ],
            &[
                (0x170000, pick(&[1, 2, 3, 4], &[1, 2, 9, 4]).into()),
                (0x171000, [1, [7, 8][n], 3].map(u32::to_le_bytes).concat()),
                (0x172000, slot),
            ],
        )
    }

    #[test]
    fn every_shape_shows_its_change() {
        needs_names!();
        let gd = GameData::default();
        let [was, now] = [0, 1].map(|n| snapshot(&shapes(n), 0x1000).unwrap());
        let row = |slot, i, kind, change| Change::new(&gd, hash(slot, i), kind, change);
        let element = |slot, i, kind, change| row(slot, 0, kind, change).at(i as usize);
        assert_eq!(
            changes(&gd, &was, &now, |_| true),
            [
                row(0, 0, "Bool", "false -> true"),
                element(3, 1, field::INT_ARRAY, "7 -> 8"),
                row(10, 0, "Vector3", "(1, 2, 3) -> (1, 2, 4)"),
                row(16, 0, "String64", "\"Kokiri\" -> \"\""),
                element(17, 0, field::STRING64_ARRAY, "\"a\" -> \"b\""),
                row(18, 0, "Binary", "4 -> 4 bytes, changed at 2"),
                row(22, 0, "Int64", "-1 -> 5"),
                row(26, 0, field::W_STRING16, "\"Hi\" -> \"Ho\""),
            ]
        );
    }

    /// Memory that counts requests: a read, or a whole `read_many`.
    struct Counted(Fake, std::cell::Cell<usize>);

    impl Memory for Counted {
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            self.1.set(self.1.get() + 1);
            self.0.read(at, len)
        }

        fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            self.1.set(self.1.get() + 1);
            ranges
                .iter()
                .map(|&(at, len)| self.0.read(at, len))
                .collect()
        }

        fn write(&self, _: u64, _: &[u8]) -> Result<()> {
            bail!("read only")
        }
    }

    #[test]
    fn a_snapshot_takes_three_requests_whatever_it_holds() {
        let (plain, counted) = (shapes(0), Counted(shapes(0), Default::default()));
        let manager = manager(&plain, 0x1000).unwrap();
        let want = values(&plain, manager).unwrap();
        assert_eq!(values(&counted, manager).unwrap(), want);
        // The table headers, then every map and entry list, then every
        // value kept outside its entry.
        assert_eq!(counted.1.get(), 3);
    }

    /// Memory that moves to the next game on every snapshot.
    pub(crate) struct Flip(pub Vec<Fake>, pub std::cell::Cell<usize>);

    impl Memory for Flip {
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let n = self.1.get();
            // The manager pointer is the first read of a snapshot.
            if at == 0x1000 {
                self.1.set(n + 1);
            }
            self.0[n % self.0.len()].read(at, len)
        }

        fn write(&self, _: u64, _: &[u8]) -> Result<()> {
            bail!("read only")
        }
    }
}
