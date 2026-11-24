//! One build's `main` as the matcher reads it: its segments, its functions
//! (from the unwind table), its imports (from the dynamic table), and its
//! text with every operand that relocates masked out, indexed so a window
//! of it is found in milliseconds.

use std::ops::Range;

use anyhow::{Context, Result, bail, ensure};
use nup_format::nso::Nso;

use super::arm;

/// A window's longest run, in words.
pub const WINDOW: u32 = 65;
/// Its shortest: a smaller function's window runs on past its end.
const SHORTEST: u32 = 4;
/// How many matches a search lists before it stops counting.
pub const MANY: usize = 32;

/// `main` laid out in memory, read for matching.
pub struct Image<'a> {
    pub bytes: &'a [u8],
    pub text: Range<u32>,
    pub rodata: Range<u32>,
    pub data: Range<u32>,
    /// Where bss ends, as MOD0 says.
    pub bss_end: u32,
    /// Every function the unwind table lists, by start.
    funcs: Vec<Range<u32>>,
    /// Each text word with its relocating operands cleared.
    masked: Vec<u32>,
    /// Each pair of masked words' hash in the top half, the first word's
    /// index in the bottom, sorted.
    index: Vec<u64>,
    dynamic: Dynamic,
}

/// A run of text words, from a byte offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub start: u32,
    pub words: u32,
}

impl Window {
    pub fn end(self) -> u32 {
        self.start + 4 * self.words
    }

    pub fn contains(self, at: u32) -> bool {
        (self.start..self.end()).contains(&at)
    }

    /// Each word's offset.
    pub fn each(self) -> impl Iterator<Item = u32> {
        (0..self.words).map(move |i| self.start + 4 * i)
    }
}

impl<'a> Image<'a> {
    pub fn new(nso: &Nso, bytes: &'a [u8]) -> Result<Self> {
        let seg = |i: usize| {
            let s = nso.segments[i];
            s.address..s.address + s.size
        };
        let (text, rodata, data) = (seg(0), seg(1), seg(2));
        ensure!(text.start == 0, "text doesn't start the image");
        let mod0 = nso.mod0(bytes)?;
        let rel = |k: u32| {
            let field =
                i32::from_le_bytes(bytes[(mod0 + 4 * k) as usize..][..4].try_into().unwrap());
            mod0.wrapping_add_signed(field)
        };
        let funcs = functions(bytes, rel(4)).context("reading the unwind table")?;
        let masked = mask(bytes, &text, &funcs);
        let index = index(&masked);
        let dynamic = Dynamic::new(bytes, rel(1)).context("reading the dynamic table")?;
        Ok(Self {
            bytes,
            text,
            rodata,
            data,
            bss_end: rel(3),
            funcs,
            masked,
            index,
            dynamic,
        })
    }

    pub fn word(&self, at: u32) -> u32 {
        u32::from_le_bytes(self.bytes[at as usize..][..4].try_into().unwrap())
    }

    pub fn quad(&self, at: u32) -> Option<u64> {
        let b = self.bytes.get(at as usize..)?.get(..8)?;
        Some(u64::from_le_bytes(b.try_into().unwrap()))
    }

    /// Whether `at` is a text word.
    pub fn in_text(&self, at: u64) -> bool {
        at % 4 == 0 && at < u64::from(self.text.end)
    }

    /// The function holding `at`.
    pub fn function(&self, at: u32) -> Option<Range<u32>> {
        let i = self
            .funcs
            .partition_point(|f| f.start <= at)
            .checked_sub(1)?;
        let f = &self.funcs[i];
        f.contains(&at).then(|| f.clone())
    }

    /// The last function that starts before `at`.
    pub fn before(&self, at: u32) -> Option<Range<u32>> {
        let i = self
            .funcs
            .partition_point(|f| f.start < at)
            .checked_sub(1)?;
        Some(self.funcs[i].clone())
    }

    /// Where the function after the one holding `at` starts.
    pub fn next_function(&self, at: u32) -> Option<u32> {
        let i = self.funcs.partition_point(|f| f.start <= at);
        self.funcs.get(i).map(|f| f.start)
    }

    /// The window that finds `at`: its function's first words if `at` is
    /// among them, else the words round it, kept inside its function.
    /// `whole` lets the entry window run its full length past a short
    /// function's end.
    pub fn window(&self, at: u32, whole: bool) -> Window {
        let f = self.function(at).unwrap_or(at..at + 4 * WINDOW);
        let span = (f.end - f.start) / 4;
        if at - f.start < 4 * WINDOW {
            let words = if whole {
                WINDOW
            } else {
                span.clamp(SHORTEST, WINDOW)
            };
            return Window {
                start: f.start,
                words: words.min((self.text.end - f.start) / 4),
            };
        }
        let words = span.min(WINDOW);
        let end = (at + 4 * (WINDOW / 2 + 1)).min(f.end);
        let start = end.saturating_sub(4 * words).max(f.start);
        Window {
            start,
            words: (end - start) / 4,
        }
    }

    /// `w`'s masked words.
    pub fn masked(&self, w: Window) -> &[u32] {
        &self.masked[(w.start / 4) as usize..(w.end() / 4) as usize]
    }

    /// Whether the word at `at` was left whole by the masking.
    pub fn unmasked(&self, at: u32) -> bool {
        self.masked[(at / 4) as usize] == self.word(at)
    }

    /// Where `pattern` (masked words, two or more) lies in this text, at
    /// most [`MANY`] + 1 of them.
    pub fn search(&self, pattern: &[u32]) -> Vec<u32> {
        let range = |k: usize| {
            let key = u64::from(pair(pattern[k], pattern[k + 1])) << 32;
            let from = self.index.partition_point(|&e| e < key);
            let to = self.index.partition_point(|&e| e < key | 0xffff_ffff);
            from..to
        };
        // The rarest pair finds the fewest places to check.
        let k = (0..pattern.len() - 1)
            .min_by_key(|&k| range(k).len())
            .unwrap();
        let n = self.masked.len();
        (self.index[range(k)].iter())
            .filter_map(|&e| (e as u32 as usize).checked_sub(k))
            .filter(|&i| i + pattern.len() <= n && self.masked[i..i + pattern.len()] == *pattern)
            .map(|i| 4 * i as u32)
            .take(MANY + 1)
            .collect()
    }

    /// Every `bl` in text that calls `to`.
    pub fn callers(&self, to: u32) -> Vec<u32> {
        (self.text.clone().step_by(4))
            .filter(|&at| arm::is_bl(self.word(at)) && arm::target(self.word(at), at) == Some(to))
            .collect()
    }

    /// The address the `adrp` at `at` and the first `add`, `ldr` or `str`
    /// after it on its register make, with where that second one is.
    pub fn resolve(&self, at: u32) -> Option<(u32, u32)> {
        let (rd, page) = arm::adrp(self.word(at), at)?;
        let end = (at + 4 * arm::REACH).min(self.text.end);
        ((at + 4..end).step_by(4)).find_map(|lo| match arm::low12(self.word(lo)) {
            Some((rn, off)) if rn == rd => Some((lo, page.wrapping_add(off))),
            _ => None,
        })
    }

    /// Every `adrp` whose address lands on one of `wanted`, by address.
    pub fn references(&self, wanted: &[u32]) -> Vec<(u32, u32)> {
        let pages: Vec<u32> = wanted.iter().map(|w| w & !0xfff).collect();
        (self.text.clone().step_by(4))
            .filter(|&at| arm::adrp(self.word(at), at).is_some_and(|(_, p)| pages.contains(&p)))
            .filter_map(|at| self.resolve(at).map(|(_, to)| (to, at)))
            .filter(|(to, _)| wanted.contains(to))
            .collect()
    }

    /// Every 8 byte word in data equal to one of `values`, with where.
    pub fn data_holding(&self, values: &[u64]) -> Vec<(u64, u32)> {
        (self.data.clone().step_by(8))
            .filter_map(|at| {
                self.quad(at)
                    .filter(|v| values.contains(v))
                    .map(|v| (v, at))
            })
            .collect()
    }

    /// The import the stub at `at` jumps through.
    pub fn import_name(&self, at: u32) -> Option<&str> {
        let slot = self.stub_slot(at)?;
        let (_, sym) = self.dynamic.jumps(self.bytes).find(|&(s, _)| s == slot)?;
        self.dynamic.name(self.bytes, sym)
    }

    /// The stub that jumps through the import named `name`.
    pub fn import_stub(&self, name: &str) -> Option<u32> {
        let named = |&(_, sym): &(u32, u32)| self.dynamic.name(self.bytes, sym) == Some(name);
        let (slot, _) = self.dynamic.jumps(self.bytes).find(named)?;
        (self.text.clone().step_by(4))
            .rev()
            .find(|&at| self.stub_slot(at) == Some(slot))
    }

    /// The jump slot of a stub at `at`: `adrp x16`, `ldr x17, [x16, #lo]`.
    fn stub_slot(&self, at: u32) -> Option<u32> {
        let (rd, page) = arm::adrp(self.word(at), at)?;
        let ldr = self.word(at + 4);
        (rd == 16 && ldr & 0xffc0_03ff == 0xf940_0211).then(|| page + ((ldr >> 10) & 0xfff) * 8)
    }
}

/// Every function the unwind table's search table lists, from
/// `.eh_frame_hdr` at `hdr`: each entry's start and its FDE's length.
fn functions(bytes: &[u8], hdr: u32) -> Result<Vec<Range<u32>>> {
    let h = hdr as usize;
    let i32_at = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    // Version 1, a 4 byte count, the table data-relative 4 byte pairs.
    let head = bytes.get(h..h + 4).context("no .eh_frame_hdr")?;
    ensure!(
        head[0] == 1 && head[2] == 0x03 && head[3] == 0x3b,
        "an .eh_frame_hdr encoded {head:02x?}"
    );
    let count = i32_at(h + 8) as usize;
    let mut funcs = Vec::with_capacity(count);
    for k in 0..count {
        let at = h + 12 + 8 * k;
        let start = hdr.wrapping_add_signed(i32_at(at));
        let fde = h.wrapping_add_signed(i32_at(at + 4) as isize);
        // The FDE: length, CIE pointer, pc-relative start, length.
        let begin = (fde as u32 + 8).wrapping_add_signed(i32_at(fde + 8));
        if begin != start {
            bail!("the FDE for {start:#x} starts at {begin:#x}");
        }
        funcs.push(start..start + i32_at(fde + 12) as u32);
    }
    ensure!(
        funcs.windows(2).all(|w| w[0].start < w[1].start),
        "the unwind table isn't sorted"
    );
    Ok(funcs)
}

/// Each text word with what relocates cleared: branch and literal
/// targets, `adrp` and `adr` pages, and the low 12 bits an `add`, `ldr` or
/// `str` adds to a register an `adrp` set. Registers are followed within a
/// function, from its start to a return or jump away.
fn mask(bytes: &[u8], text: &Range<u32>, funcs: &[Range<u32>]) -> Vec<u32> {
    let mut starts = funcs.iter().map(|f| f.start).peekable();
    let mut paged = 0u32;
    (text.clone().step_by(4))
        .map(|at| {
            while let Some(start) = starts.next_if(|&s| s <= at) {
                if start == at {
                    paged = 0;
                }
            }
            let w = u32::from_le_bytes(bytes[at as usize..][..4].try_into().unwrap());
            let (masked, after) = arm::mask(w, paged);
            paged = after;
            masked
        })
        .collect()
}

fn pair(a: u32, b: u32) -> u32 {
    let h = (u64::from(a) << 32 | u64::from(b)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (h >> 32) as u32
}

fn index(masked: &[u32]) -> Vec<u64> {
    let mut index: Vec<u64> = (masked.windows(2).enumerate())
        .map(|(i, w)| u64::from(pair(w[0], w[1])) << 32 | i as u64)
        .collect();
    index.sort_unstable();
    index
}

/// What the dynamic table says about imports: the symbols, their names,
/// and the jump slots' relocations.
struct Dynamic {
    symbols: u32,
    strings: u32,
    jumps: Range<u32>,
}

const DT_PLTRELSZ: u64 = 2;
const DT_STRTAB: u64 = 5;
const DT_SYMTAB: u64 = 6;
const DT_JMPREL: u64 = 23;
/// A symbol's size, and a relocation's.
const SYM: u32 = 24;
const RELA: u32 = 24;

impl Dynamic {
    fn new(bytes: &[u8], at: u32) -> Result<Self> {
        let quad = |a: u32| {
            bytes
                .get(a as usize..a as usize + 8)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        };
        let mut tags = [0u64; 24];
        for a in (at..).step_by(16) {
            match quad(a).context("the dynamic table runs off the image")? {
                0 => break,
                t if (t as usize) < tags.len() => tags[t as usize] = quad(a + 8).unwrap_or(0),
                _ => {}
            }
        }
        let tag = |t: u64| u32::try_from(tags[t as usize]).ok().filter(|&v| v != 0);
        let jumps = tag(DT_JMPREL).context("no jump slots")?;
        Ok(Self {
            symbols: tag(DT_SYMTAB).context("no symbols")?,
            strings: tag(DT_STRTAB).context("no strings")?,
            jumps: jumps..jumps + tag(DT_PLTRELSZ).context("no jump slots' size")?,
        })
    }

    /// Each jump slot and the symbol it's for.
    fn jumps<'b>(&self, bytes: &'b [u8]) -> impl Iterator<Item = (u32, u32)> + 'b {
        (self.jumps.clone().step_by(RELA as usize)).map(move |r| {
            let q = |a: u32| u64::from_le_bytes(bytes[a as usize..][..8].try_into().unwrap());
            (q(r) as u32, (q(r + 8) >> 32) as u32)
        })
    }

    fn name<'b>(&self, bytes: &'b [u8], sym: u32) -> Option<&'b str> {
        let at = self.symbols.checked_add(sym.checked_mul(SYM)?)? as usize;
        let off = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
        let from = (self.strings + off) as usize;
        let len = bytes.get(from..)?.iter().position(|&b| b == 0)?;
        std::str::from_utf8(&bytes[from..from + len]).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_hash_tells_order_apart() {
        assert_ne!(pair(1, 2), pair(2, 1));
        assert_eq!(pair(7, 9), pair(7, 9));
    }

    #[test]
    fn a_window_starts_at_the_entry_or_sits_round_the_word() {
        let w = Window {
            start: 0x100,
            words: 3,
        };
        assert_eq!(w.end(), 0x10c);
        assert!(w.contains(0x108) && !w.contains(0x10c));
        assert_eq!(w.each().collect::<Vec<_>>(), [0x100, 0x104, 0x108]);
    }
}
