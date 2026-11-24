//! The stored forms of vertex values: packed normals and tangents, half
//! floats and 16-bit unorms.

use anyhow::{Result, ensure};

use crate::reader::Reader;

/// A normal or tangent as stored: x, y and z in 10 signed bits each, over
/// 511, and w (a tangent's handedness) in the top 2.
pub fn unpack(packed: u32) -> [f32; 4] {
    let signed = |shift: u32, bits: u32| {
        let v = (packed >> shift) & ((1 << bits) - 1);
        (v as i32) << (32 - bits) >> (32 - bits)
    };
    let xyz = |shift| (signed(shift, 10) as f32 / 511.0).max(-1.0);
    [xyz(0), xyz(10), xyz(20), signed(30, 2) as f32]
}

/// [`unpack`]'s inverse, each value rounded.
pub fn pack(v: [f32; 4]) -> u32 {
    let field = |f: f32, scale: f32, bits: u32| {
        let max = (1 << (bits - 1)) - 1;
        ((f * scale).round() as i32).clamp(-max, max) as u32 & ((1 << bits) - 1)
    };
    field(v[0], 511.0, 10)
        | field(v[1], 511.0, 10) << 10
        | field(v[2], 511.0, 10) << 20
        | field(v[3], 1.0, 2) << 30
}

/// The half float at `at`.
pub(super) fn half_at(r: &Reader, at: usize) -> Result<f32> {
    Ok(from_half(r.u16(at)?))
}

/// The 16-bit unorm at `at`, from 0 to 1.
pub(super) fn unorm_at(r: &Reader, at: usize) -> Result<f32> {
    Ok(r.u16(at)? as f32 / 65535.0)
}

/// A half float's value.
fn from_half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let (exp, mantissa) = ((h >> 10) & 0x1f, (h & 0x3ff) as f32);
    match exp {
        0 => sign * mantissa * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        e => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(e as i32 - 15),
    }
}

/// `f` as a half float, rounded. Too small for a normal half, it's a
/// subnormal one (steps of 2^-24, rounding up into the smallest normal);
/// too big, an error.
pub(super) fn to_half(f: f32) -> Result<u16> {
    let bits = f.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    if f.abs() < 2f32.powi(-14) {
        return Ok(sign | (f.abs() * 2f32.powi(24)).round() as u16);
    }
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    ensure!(exp < 31, "{f} is past a half");
    let mantissa = bits & 0x7f_ffff;
    // A carry out of the mantissa rounds up into the exponent, as it should.
    Ok(sign | (((exp as u32) << 10) + (mantissa >> 13) + ((mantissa >> 12) & 1)) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_normals_round_trip() {
        for packed in [0x4000_0000_u32, 0x7fd8_6abc, 0xc19f_8241, 0x5fff_fdff] {
            assert_eq!(pack(unpack(packed)), packed, "{packed:#x}");
        }
        assert_eq!(unpack(pack([1.0, -1.0, 0.0, -1.0])), [1.0, -1.0, 0.0, -1.0]);
    }

    #[test]
    fn halves_round_trip() {
        for h in [
            0x0000, 0x3c00, 0x2283, 0xa283, 0x1c00, 0x7bff, 0x0001, 0x8314,
        ] {
            assert_eq!(to_half(from_half(h)).unwrap(), h, "{h:#x}");
        }
        assert_eq!(from_half(0x3c00), 1.0);
        assert_eq!(to_half(0.013).unwrap(), 0x22a8);
        assert_eq!(to_half(1e-6).unwrap(), 17);
        assert_eq!(to_half(-4.7e-5).unwrap(), 0x8000 | 789);
        assert_eq!(from_half(to_half(6.103e-5).unwrap()), 2f32.powi(-14));
        assert_eq!(to_half(1e-9).unwrap(), 0);
        assert!(to_half(70000.0).is_err());
    }
}
