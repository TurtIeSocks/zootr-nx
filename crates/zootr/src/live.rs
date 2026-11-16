//! GameData values read and written live in the running PC build, from
//! outside the game. Plain memory access, no hooks. Layout in `docs/research/live-flags.md`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};

use ounce_formats::murmur3;

use crate::config::Target;
use crate::gamedata::GameData;
use crate::known::{field, flag};
use crate::patch::OFFSETS;
use crate::savefile::{self, Change};

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
const TABLE: u64 = 0x60;
/// More bytes than this in one read means the layout is off.
const MAX: usize = 1 << 24;
/// How often [`Poll`] is meant to be called.
pub const TICK: Duration = Duration::from_millis(100);
/// More changes than this in [`WINDOW`] mutes a value.
pub const BURST: u32 = 5;
pub const WINDOW: Duration = Duration::from_secs(10);

/// A running game's memory: the Mac game through Mach, or another
/// backend.
pub trait Memory {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>>;

    fn write(&self, at: u64, data: &[u8]) -> Result<()>;

    fn u32(&self, at: u64) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read(at, 4)?.try_into().unwrap()))
    }

    fn u64(&self, at: u64) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read(at, 8)?.try_into().unwrap()))
    }
}

impl<M: Memory + ?Sized> Memory for &M {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        (**self).read(at, len)
    }

    fn write(&self, at: u64, data: &[u8]) -> Result<()> {
        (**self).write(at, data)
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

/// Every mapped value, in manager order, given the global from [`global`].
pub fn snapshot(mem: &impl Memory, global: u64) -> Result<Vec<Value>> {
    let manager = mem.u64(mem.u64(global)?)?;
    let mut out = Vec::new();
    for &(slot, kind, stride, shape) in TYPES.iter() {
        let table = manager + 8 + TABLE * slot;
        let count = mem.u32(table + 8)? as usize;
        let mut hashes = vec![0; count];
        let cap = mem.u32(table + 0x24)? as usize;
        for pair in mem
            .read(mem.u64(table + 0x18)?, sized(kind, cap * 8)?)?
            .as_chunks::<8>()
            .0
        {
            let (hash, i) = (word(pair, 0), word(pair, 4) as usize);
            if hash != 0 && i < count {
                hashes[i] = hash;
            }
        }
        let entries = mem.read(mem.u64(table + 0x10)?, sized(kind, count * stride)?)?;
        for (e, hash) in entries.chunks_exact(stride).zip(hashes) {
            if hash != 0 {
                let items = items(mem, kind, e, shape)?;
                out.push(Value { kind, hash, items });
            }
        }
    }
    Ok(out)
}

fn sized(kind: &str, len: usize) -> Result<usize> {
    if len > MAX {
        bail!("a {kind} read of {len} bytes; is this the 1.1.0 update?");
    }
    Ok(len)
}

/// The current value of entry `e`.
fn items(mem: &impl Memory, kind: &str, e: &[u8], shape: Shape) -> Result<Vec<Vec<u8>>> {
    Ok(match shape {
        Inline(at, len) => vec![text(kind, &e[at..at + len])],
        Blob(at) => {
            let len = sized(kind, quad(e, at + 0x10) as usize)?;
            vec![mem.read(quad(e, at + 8), len)?]
        }
        List(at, width, skip) => {
            let len = sized(kind, quad(e, at) as usize * width)?;
            mem.read(quad(e, at + 8), len)?
                .chunks_exact(width)
                .map(|x| text(kind, &x[skip..]))
                .collect()
        }
    })
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
    let f = |at| f32::from_bits(word(b, at));
    match savefile::element(kind) {
        "Bool" => savefile::render(gd, kind, b[0].into()),
        "Int64" => (quad(b, 0) as i64).to_string(),
        k if k == field::U_INT64 => quad(b, 0).to_string(),
        "Vector3" => format!("({}, {}, {})", f(0), f(4), f(8)),
        k if k == field::W_STRING16 => {
            let units: Vec<u16> = b
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_le_bytes(c))
                .collect();
            format!("{:?}", String::from_utf16_lossy(&units))
        }
        k if k.starts_with("String") => format!("{:?}", String::from_utf8_lossy(b)),
        _ => savefile::render(gd, kind, word(b, 0)),
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

/// A running game, polled for what changed since the last look.
pub struct Poll<M> {
    mem: M,
    global: u64,
    was: Vec<Value>,
    noise: Noise,
    muted: HashSet<u32>,
}

impl Poll<Game> {
    pub fn new(game: Game) -> Result<Self> {
        let global = game.global;
        Self::over(game, global)
    }
}

impl<M: Memory> Poll<M> {
    pub fn over(mem: M, global: u64) -> Result<Self> {
        Ok(Self {
            was: snapshot(&mem, global)?,
            mem,
            global,
            noise: Noise::default(),
            muted: HashSet::new(),
        })
    }

    /// What changed since the last call among values `wanted` keeps, and
    /// the values muted for the first time. `all` mutes nothing.
    pub fn next(
        &mut self,
        gd: &GameData,
        all: bool,
        mut wanted: impl FnMut(u32) -> bool,
    ) -> Result<(Vec<Change>, Vec<u32>)> {
        let now = snapshot(&self.mem, self.global)?;
        let (at, mut muted) = (Instant::now(), Vec::new());
        let keep = |h| {
            if !wanted(h) {
                return false;
            }
            if all || self.noise.quiet(h, at) {
                return true;
            }
            if self.muted.insert(h) {
                muted.push(h);
            }
            false
        };
        let out = changes(gd, &self.was, &now, keep);
        self.was = now;
        Ok((out, muted))
    }
}

/// Mutes values that change too often to read, like Link's position and
/// the play clock. Once muted, a value stays muted.
#[derive(Default)]
pub struct Noise(HashMap<u32, (Instant, u32)>);

impl Noise {
    /// Whether a change to `hash` at `now` should still print.
    pub fn quiet(&mut self, hash: u32, now: Instant) -> bool {
        let (start, n) = self.0.entry(hash).or_insert((now, 0));
        if *n <= BURST && now - *start > WINDOW {
            (*start, *n) = (now, 0);
        }
        *n += 1;
        *n <= BURST
    }
}

/// The running game, attached for reads and writes.
pub struct Game {
    mem: Backend,
    global: u64,
}

/// How a [`Game`] reaches the game's memory.
enum Backend {
    /// The Mac game's task.
    Mach(u32),
    /// The native build's control socket.
    Bridge(Bridge),
}

/// Where the native build loads the game's `main`.
pub const NATIVE_MAIN: u64 = 0x8002_0000;

impl Memory for Game {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        match &self.mem {
            Backend::Mach(task) => mach::read(*task, at, len),
            Backend::Bridge(b) => b.read(at, len),
        }
    }

    fn write(&self, at: u64, data: &[u8]) -> Result<()> {
        match &self.mem {
            Backend::Mach(task) => mach::write(*task, at, data),
            Backend::Bridge(b) => b.write(at, data),
        }
    }
}

/// The OS refused to let this build read the game. The one attach error
/// that waiting for the game won't fix.
#[derive(Debug)]
pub struct NotSigned {
    pub pid: i32,
}

impl std::fmt::Display for NotSigned {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let exe = std::env::current_exe().unwrap_or_default();
        write!(
            f,
            "this build isn't signed for live reads of process {}\n\
             Build with `make zootr`, or sign this binary:\n  \
             codesign -f -s - --entitlements crates/zootr/live.entitlements {}",
            self.pid,
            exe.display()
        )
    }
}

impl std::error::Error for NotSigned {}

/// Live reads need macOS, or a target with a bridge. Like [`NotSigned`],
/// waiting won't fix it.
#[derive(Debug)]
pub struct Unsupported;

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("live reads need macOS, or a target with a bridge")
    }
}

impl std::error::Error for Unsupported {}

impl Game {
    /// The target's running game: through its bridge when it has one,
    /// else among the processes started from its app.
    pub fn attach(target: &Target) -> Result<Self> {
        match &target.bridge {
            Some(socket) => Self::over(Bridge::connect(socket)?),
            None => Self::find(&target.app),
        }
    }

    /// The native build behind `bridge`, whose GameData global sits at a
    /// fixed offset from `main`.
    pub fn over(bridge: Bridge) -> Result<Self> {
        let global = NATIVE_MAIN + u64::from(OFFSETS.gamedata);
        bridge.u64(global)?;
        Ok(Self {
            mem: Backend::Bridge(bridge),
            global,
        })
    }

    fn find(app: &Path) -> Result<Self> {
        let pids = mach::pids(app)?;
        if pids.is_empty() {
            bail!("the game isn't running from {}", app.display());
        }
        // The game runs as two processes, and the OS may refuse one of them
        // while the other, the one holding GameData, lets this build in.
        let mut refused = None;
        for pid in pids {
            let Some(task) = mach::task(pid) else {
                refused.get_or_insert(pid);
                continue;
            };
            for (at, size) in mach::code(task) {
                let Ok(code) = mach::read(task, at, size) else {
                    continue;
                };
                // The system's shared library cache, never the game.
                if code.starts_with(b"dyld") {
                    continue;
                }
                if let Some(global) = global(&code, at) {
                    return Ok(Self {
                        mem: Backend::Mach(task),
                        global,
                    });
                }
            }
        }
        if let Some(pid) = refused {
            return Err(NotSigned { pid }.into());
        }
        bail!(
            "{} wasn't found in the game; is this the 1.1.0 update?",
            flag::GAME_DATA
        )
    }
}

/// A table's row of [`TYPES`] and where `hash`'s entry sits in it, in any
/// table the game keeps.
fn find(mem: &impl Memory, global: u64, hash: u32) -> Result<Option<(&'static Row, u64)>> {
    let manager = mem.u64(mem.u64(global)?)?;
    for row in TYPES.iter() {
        let (slot, kind, stride, _) = *row;
        let table = manager + 8 + TABLE * slot;
        let count = mem.u32(table + 8)?;
        let cap = mem.u32(table + 0x24)? as usize;
        let map = mem.read(mem.u64(table + 0x18)?, sized(kind, cap * 8)?)?;
        let found = map
            .as_chunks::<8>()
            .0
            .iter()
            .find(|p| word(&p[..], 0) == hash);
        if let Some(pair) = found {
            let i = word(&pair[..], 4);
            if i < count {
                let entry = mem.u64(table + 0x10)? + u64::from(i) * stride as u64;
                return Ok(Some((row, entry)));
            }
        }
    }
    Ok(None)
}

/// Where a value held inline keeps its current value: its type, address
/// and width. A scalar's saved copy sits right after. See [`encode`] for
/// what's writable.
pub fn locate(mem: &impl Memory, global: u64, hash: u32) -> Result<(&'static str, u64, usize)> {
    if let Some(((_, kind, _, Inline(off, len)), entry)) = find(mem, global, hash)? {
        return Ok((kind, entry + *off as u64, *len));
    }
    bail!("{hash:#010x} isn't a value the game holds inline")
}

/// The most array elements [`read`] spells out.
const SHOWN: usize = 64;

/// The current value of any entry the game holds, as the Live tab shows it:
/// an array's elements in brackets, a binary's size, a struct's members one
/// per line, `?` for one the game doesn't hold.
pub fn read(mem: &impl Memory, gd: &GameData, global: u64, hash: u32) -> Result<String> {
    if gd.kind(hash) == Some("Struct") {
        let lines: Vec<String> = gd
            .members(hash)?
            .into_iter()
            .map(|(member, value)| {
                let now = read(mem, gd, global, value).unwrap_or_else(|_| "?".into());
                format!("{} = {now}", gd.bare(member))
            })
            .collect();
        return Ok(lines.join("\n"));
    }
    let Some(((_, kind, stride, shape), entry)) = find(mem, global, hash)? else {
        bail!("{hash:#010x} isn't a value the game holds");
    };
    let items = items(mem, kind, &mem.read(entry, *stride)?, *shape)?;
    Ok(match *kind {
        "Binary" => format!("{} bytes", items[0].len()),
        k if k.ends_with("Array") => {
            let more = items.len().saturating_sub(SHOWN);
            let mut shown: Vec<_> = items.iter().take(SHOWN).map(|b| show(gd, k, b)).collect();
            if more > 0 {
                shown.push(format!("... {more} more"));
            }
            format!("[{}]", shown.join(", "))
        }
        k => show(gd, k, &items[0]),
    })
}

/// `value` as the game stores a `kind` `width` bytes wide: true/false, a
/// number, an Enum value's name, or text padded with zeros.
pub fn encode(kind: &str, value: &str, width: usize) -> Result<Vec<u8>> {
    Ok(match kind {
        "String16" | "String32" | "String64" => {
            ensure!(value.len() < width, "{value} is too long for a {kind}");
            let mut out = value.as_bytes().to_vec();
            out.resize(width, 0);
            out
        }
        "Bool" => vec![u8::from(value.parse::<bool>()?)],
        "Int" => value.parse::<i32>()?.to_le_bytes().into(),
        "UInt" => value.parse::<u32>()?.to_le_bytes().into(),
        "Float" => value.parse::<f32>()?.to_le_bytes().into(),
        "Enum" => murmur3(value.as_bytes()).to_le_bytes().into(),
        _ => bail!("{kind} isn't writable"),
    })
}

/// Writes the current value, and for a scalar the copy after it, which
/// is the one the game saves from. Data pages only.
pub fn set(mem: &impl Memory, global: u64, hash: u32, value: &str) -> Result<()> {
    let (kind, at, width) = locate(mem, global, hash)?;
    let bytes = encode(kind, value, width)?;
    let copies = if kind.starts_with("String") { 1 } else { 2 };
    mem.write(at, &bytes.repeat(copies))
}

/// A GameData name's hash, or a hash as given: `0x612320f9`, or
/// `#612320f9` as `probe data` and the live log print it.
pub fn hash(name: &str) -> Result<u32> {
    Ok(
        match (name.strip_prefix("0x")).or_else(|| name.strip_prefix('#')) {
            Some(hex) => u32::from_str_radix(hex, 16)?,
            None => murmur3(name.as_bytes()),
        },
    )
}

impl Game {
    pub fn global(&self) -> u64 {
        self.global
    }

    /// The current value, as the Live tab shows it. See [`read`].
    pub fn get(&self, gd: &GameData, hash: u32) -> Result<String> {
        read(self, gd, self.global, hash)
    }

    /// See [`set`].
    pub fn set(&self, hash: u32, value: &str) -> Result<()> {
        set(self, self.global, hash, value)
    }
}

/// The processes started from `app`.
pub(crate) use mach::pids;

mod bridge;
mod mach;
#[cfg(windows)]
mod pipe;

pub use bridge::{Bridge, PATIENCE, Refused};

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn a_hash_is_taken_as_printed() {
        let h = 0x612320f9;
        assert_eq!(super::hash("0x612320f9").unwrap(), h);
        assert_eq!(super::hash("#612320f9").unwrap(), h);
        assert_eq!(
            super::hash("IsGet.Item_X").unwrap(),
            murmur3(b"IsGet.Item_X")
        );
        assert!(super::hash("#nothex").is_err());
    }

    use anyhow::Context;

    use super::*;

    pub(crate) struct Fake(Vec<(u64, Vec<u8>)>);

    impl Memory for Fake {
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            if len == 0 {
                return Ok(Vec::new());
            }
            let (start, bytes) = self
                .0
                .iter()
                .find(|(s, b)| (*s..*s + b.len() as u64).contains(&at))
                .context("unmapped")?;
            let from = (at - start) as usize;
            Ok(bytes.get(from..from + len).context("short")?.to_vec())
        }

        fn write(&self, _: u64, _: &[u8]) -> Result<()> {
            bail!("read only")
        }
    }

    fn put(b: &mut [u8], at: usize, v: &[u8]) {
        b[at..at + v.len()].copy_from_slice(v);
    }

    /// A game whose tables hold these entries. Each map lists its entries
    /// backwards, so a snapshot has to go by the map. Pointers in entries
    /// lead into `heap`.
    pub(crate) fn game(tables: &[(u64, Vec<Vec<u8>>)], heap: &[(u64, Vec<u8>)]) -> Fake {
        let mut manager = vec![0; 8 + 0x60 * 28];
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
        mem.extend(heap.iter().cloned());
        Fake(mem)
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
    fn locate_finds_a_scalar_through_its_map() {
        let mem = ints([5, 7]);
        let (kind, at, len) = locate(&mem, 0x1000, hash(2, 1)).unwrap();
        assert_eq!((kind, at, len), ("Int", 0x10000 + 0x28 + 0x1c, 4));
        assert_eq!(mem.u32(at).unwrap(), 7);
        assert!(locate(&mem, 0x1000, 0xdead).is_err());
    }

    #[test]
    fn read_shows_any_kind_the_way_the_live_tab_does() {
        needs_names!();
        let gd = GameData::default();
        let list = entry(
            3,
            &[
                (0x20, &3u64.to_le_bytes()),
                (0x28, &0x50000u64.to_le_bytes()),
            ],
        );
        let items: Vec<u8> = [1i32, -2, 3]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect();
        let blob = entry(
            18,
            &[
                (0x20, &0x60000u64.to_le_bytes()),
                (0x28, &0x300u64.to_le_bytes()),
            ],
        );
        let mem = game(
            &[
                (2, vec![entry(2, &[(0x1c, &(-5i32).to_le_bytes())])]),
                (3, vec![list]),
                (18, vec![blob]),
            ],
            &[(0x50000, items), (0x60000, vec![0; 0x300])],
        );
        let at = |slot| read(&mem, &gd, 0x1000, hash(slot, 0)).unwrap();
        assert_eq!(at(2), "-5");
        assert_eq!(at(3), "[1, -2, 3]");
        assert_eq!(at(18), "768 bytes");
        assert!(read(&mem, &gd, 0x1000, 0xdead).is_err());
    }

    #[test]
    fn values_encode_as_the_game_stores_them() {
        assert_eq!(encode("Bool", "true", 1).unwrap(), [1]);
        assert_eq!(encode("Int", "-1", 4).unwrap(), [0xff; 4]);
        assert_eq!(
            encode("Enum", "New", 4).unwrap(),
            murmur3(b"New").to_le_bytes()
        );
        assert_eq!(
            encode("String16", "Kak", 16).unwrap(),
            *b"Kak\0\0\0\0\0\0\0\0\0\0\0\0\0"
        );
        assert!(encode("String16", "0123456789abcdef", 16).is_err());
        assert!(encode("Bool", "yes", 1).is_err());
        assert!(encode("Vector3", "0", 12).is_err());
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

    #[test]
    fn a_poll_reports_a_value_muted_once() {
        let gd = GameData::default();
        let flip = Flip(vec![ints([5, 1]), ints([5, 2])], 0.into());
        let mut poll = Poll::over(flip, 0x1000).unwrap();
        let mut seen = Vec::new();
        for _ in 0..8 {
            let (changes, muted) = poll.next(&gd, false, |_| true).unwrap();
            seen.push((changes.len(), muted));
        }
        let mut expected = vec![(1, vec![]); 5];
        expected.extend([(0, vec![hash(2, 1)]), (0, vec![]), (0, vec![])]);
        assert_eq!(seen, expected);
        assert_eq!(poll.next(&gd, true, |_| true).unwrap().0.len(), 1);
    }

    #[test]
    fn noise_mutes_a_burst_for_good() {
        let (mut noise, start) = (Noise::default(), Instant::now());
        let at = |ms| start + Duration::from_millis(ms);
        let slow: Vec<bool> = (0..8).map(|k| noise.quiet(1, at(k * 4000))).collect();
        assert!(slow.iter().all(|&q| q));
        let fast: Vec<bool> = (0..7).map(|k| noise.quiet(2, at(k * 100))).collect();
        assert_eq!(fast, [true, true, true, true, true, false, false]);
        assert!(!noise.quiet(2, at(60_000)));
    }
}
