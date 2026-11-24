//! The few AArch64 encodings the matcher reads, by their bit patterns:
//! branches and their targets, `adrp` pages, the low 12 bits an `add`,
//! `ldr` or `str` adds to them, and the immediate offsets a load or store
//! reads a field at. No disassembler: everything else is just a word.

/// How far past an `adrp` its `add`, `ldr` or `str` may sit, in words.
pub const REACH: u32 = 16;
/// The registers a call keeps: `x19` to `x29`.
const KEPT: u32 = 0x3ff8_0000;

fn bit(r: u32) -> u32 {
    1 << r
}

/// `w` with what relocates cleared, given the registers an `adrp` set
/// (`paged`, one bit each), and those registers after it.
pub fn mask(w: u32, paged: u32) -> (u32, u32) {
    let rd = w & 31;
    // adrp and adr: the page or address.
    if w & 0x1f00_0000 == 0x1000_0000 {
        let after = if w >> 31 == 1 {
            paged | bit(rd)
        } else {
            paged & !bit(rd)
        };
        return (w & 0x9f00_001f, after);
    }
    // b and bl: a call keeps only what the callee saves.
    if w & 0x7c00_0000 == 0x1400_0000 {
        return (w & 0xfc00_0000, if w >> 31 == 1 { paged & KEPT } else { 0 });
    }
    // b.cond, cbz and cbnz.
    if w & 0xff00_0010 == 0x5400_0000 || w & 0x7e00_0000 == 0x3400_0000 {
        return (w & 0xff00_001f, paged);
    }
    // tbz and tbnz.
    if w & 0x7e00_0000 == 0x3600_0000 {
        return (w & 0xfff8_001f, paged);
    }
    // ret, br and blr (and their authenticating forms): leaving, or an
    // indirect call, forgets every register.
    if w & 0xffff_f000 == 0xd65f_0000 || w & 0xfe9f_f000 == 0xd61f_0000 {
        return (w, 0);
    }
    // A literal load: the literal's address.
    if w & 0x3b00_0000 == 0x1800_0000 {
        return (w & 0xff00_001f, paged & !written(w));
    }
    let masked = match low12(w) {
        Some((rn, _)) if paged & bit(rn) != 0 => w & !(0xfff << 10),
        _ => w,
    };
    (masked, paged & !written(w))
}

/// The general registers `w` writes, near enough: its first register
/// field, the second too for a load pair, none for a store or a vector
/// load. Writing too many only masks less.
fn written(w: u32) -> u32 {
    if w & 0x0a00_0000 != 0x0800_0000 {
        return bit(w & 31);
    }
    let pair = w & 0x3a00_0000 == 0x2800_0000;
    let literal = w & 0x3b00_0000 == 0x1800_0000;
    let load = literal
        || if pair {
            w & (1 << 22) != 0
        } else {
            (w >> 22) & 3 != 0
        };
    let vector = w & (1 << 26) != 0;
    match (load && !vector, pair) {
        (false, _) => 0,
        (true, false) => bit(w & 31),
        (true, true) => bit(w & 31) | bit((w >> 10) & 31),
    }
}

pub fn is_bl(w: u32) -> bool {
    w >> 26 == 0x25
}

/// Where the branch `w` at `at` goes: `b`, `bl`, `b.cond`, `cbz`, `cbnz`,
/// `tbz` or `tbnz`.
pub fn target(w: u32, at: u32) -> Option<u32> {
    let delta = if w & 0x7c00_0000 == 0x1400_0000 {
        ((w & 0x03ff_ffff) << 6) as i32 >> 4
    } else if w & 0xff00_0010 == 0x5400_0000 || w & 0x7e00_0000 == 0x3400_0000 {
        (((w >> 5) & 0x7ffff) << 13) as i32 >> 11
    } else if w & 0x7e00_0000 == 0x3600_0000 {
        (((w >> 5) & 0x3fff) << 18) as i32 >> 16
    } else {
        return None;
    };
    Some(at.wrapping_add_signed(delta))
}

/// The register an `adrp` at `at` sets, and the page.
pub fn adrp(w: u32, at: u32) -> Option<(u32, u32)> {
    if w & 0x9f00_0000 != 0x9000_0000 {
        return None;
    }
    let pages = (((w >> 5) & 0x7ffff) << 2 | (w >> 29) & 3) << 11;
    let pages = i64::from(pages as i32 >> 11);
    Some((w & 31, ((i64::from(at) & !0xfff) + (pages << 12)) as u32))
}

/// The base register and byte offset of an `add` (unshifted immediate) or
/// an `ldr`/`str` with an unsigned immediate.
pub fn low12(w: u32) -> Option<(u32, u32)> {
    let (rn, imm) = ((w >> 5) & 31, (w >> 10) & 0xfff);
    if w & 0x7fc0_0000 == 0x1100_0000 {
        return Some((rn, imm));
    }
    if w & 0x3b00_0000 != 0x3900_0000 {
        return None;
    }
    let (size, vector, opc) = (w >> 30, w & (1 << 26) != 0, (w >> 22) & 3);
    let scale = if vector && size == 0 && opc & 2 != 0 {
        4
    } else {
        size
    };
    Some((rn, imm << scale))
}

/// The field offset a load, a store or an `add` reads at, if positive:
/// an unsigned immediate, an unscaled one, or a pair's.
pub fn field(w: u32) -> Option<u32> {
    if let Some((_, off)) = low12(w) {
        return Some(off);
    }
    let signed = |v: u32, bits: u32| ((v << (32 - bits)) as i32) >> (32 - bits);
    let off = if w & 0x3b20_0c00 == 0x3800_0000 {
        signed((w >> 12) & 0x1ff, 9)
    } else if w & 0x3a00_0000 == 0x2800_0000 {
        let opc = w >> 30;
        let scale = if w & (1 << 26) != 0 {
            2 + opc
        } else {
            2 + (opc >> 1)
        };
        signed((w >> 15) & 0x7f, 7) << scale
    } else {
        return None;
    };
    u32::try_from(off).ok().filter(|&o| o > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branches_read_their_targets() {
        // A call site's bl in 1.1.1, back to the event setter.
        assert_eq!(target(0x9794_0dee, 0x2ae_0828), Some(0xfe_3fe0));
        // cbz w8, +8 and tbz w9, #8, +0xa0.
        assert_eq!(target(0x3400_0048, 0x17d_59bc), Some(0x17d_59c4));
        assert_eq!(target(0x3640_0509, 0x41_0250), Some(0x41_02f0));
        assert_eq!(target(0x9100_0000, 0), None);
        assert!(is_bl(0x9794_0dee) && !is_bl(0x1400_0002));
    }

    #[test]
    fn an_adrp_pairs_with_its_low_bits() {
        // adrp x16, +0x83f0 pages from 0x3c42800, then ldr x17, [x16, #0x5c8].
        let (rd, page) = adrp(0xb000_83f0 | 16, 0x3c4_2800).unwrap();
        assert_eq!((rd, page), (16, 0x3c4_2000 + (0x107d << 12)));
        assert_eq!(low12(0xf942_e611), Some((16, 0x5c8)));
        // add x8, x8, #0x10 and add x8, x8, #1, lsl #12.
        assert_eq!(low12(0x9100_4108), Some((8, 0x10)));
        assert_eq!(low12(0x9140_0508), None);
    }

    #[test]
    fn masking_clears_what_moves_and_keeps_fields() {
        let adrp_x8 = 0x9023_4568;
        let (m, paged) = mask(adrp_x8, 0);
        assert_eq!((m, paged), (0x9000_0008, bit(8)));
        // ldr x9, [x8, #0x18]: the page's low bits, then x9 holds a value.
        let (m, paged) = mask(0xf940_0d09, paged);
        assert_eq!((m, paged), (0xf940_0109, bit(8)));
        // ldr x10, [x9, #0x18]: a field of what was loaded.
        assert_eq!(mask(0xf940_0d2a, paged).0, 0xf940_0d2a);
        // A call forgets x8, a return forgets everything.
        assert_eq!(mask(0x9400_0010, paged), (0x9400_0000, 0));
        assert_eq!(mask(0xd65f_03c0, bit(20)).1, 0);
        assert_eq!(mask(0x9400_0010, bit(20)).1, bit(20));
    }

    #[test]
    fn fields_read_from_each_form() {
        // ldr w8, [x0, #0x12b4], ldur x8, [x0, #0x31], stp x1, x2, [x0, #0x1f0].
        assert_eq!(field(0xb952_b408), Some(0x12b4));
        assert_eq!(field(0xf843_1008), Some(0x31));
        assert_eq!(field(0xa91f_0801), Some(0x1f0));
        // ldp x1, x2, [sp, #-0x10] reads below its base: no field.
        assert_eq!(field(0xa97f_0be1), None);
        assert_eq!(field(0xd503_237f), None);
    }
}
