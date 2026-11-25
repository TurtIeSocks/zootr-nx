//! OKLCH: lightness, chroma and hue as the eye sees them. A Random color
//! is drawn here, and one color follows another by its offset here, so a
//! red pick keeps a two-tone part two-tone.

use super::paint::Color;

/// Lightness 0 to 1, chroma from 0 (about 0.37 at most in sRGB), hue in
/// turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

fn linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    match c <= 0.04045 {
        true => c / 12.92,
        false => ((c + 0.055) / 1.055).powf(2.4),
    }
}

fn gamma(c: f32) -> f32 {
    match c <= 0.003_130_8 {
        true => 12.92 * c,
        false => 1.055 * c.powf(1.0 / 2.4) - 0.055,
    }
}

/// A color in linear light, each channel 0 to 1, as shaders read one.
pub fn to_linear(c: Color) -> [f32; 3] {
    c.map(linear)
}

/// A linear color as sRGB, each channel clamped to 0 to 1 first.
pub fn from_linear(c: [f32; 3]) -> Color {
    c.map(|v| (gamma(v.clamp(0.0, 1.0)) * 255.0).round() as u8)
}

pub fn lch(c: Color) -> Lch {
    let [r, g, b] = to_linear(c);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    Lch {
        l: lightness,
        c: a.hypot(bb),
        h: (bb.atan2(a) / std::f32::consts::TAU).rem_euclid(1.0),
    }
}

/// `lch` in linear sRGB, out of gamut as it may be.
fn unclamped(Lch { l, c, h }: Lch) -> [f32; 3] {
    let (a, b) = (
        c * (h * std::f32::consts::TAU).cos(),
        c * (h * std::f32::consts::TAU).sin(),
    );
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
}

fn in_gamut(rgb: [f32; 3]) -> bool {
    rgb.iter().all(|c| (-1e-4..=1.0 + 1e-4).contains(c))
}

/// `lch` as a color, its chroma eased down until it fits in sRGB, so the
/// hue and lightness hold.
pub fn color(lch: Lch) -> Color {
    let lch = Lch {
        l: lch.l.clamp(0.0, 1.0),
        ..lch
    };
    let (mut lo, mut hi) = (0.0, lch.c.max(0.0));
    if !in_gamut(unclamped(Lch { c: hi, ..lch })) {
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            match in_gamut(unclamped(Lch { c: mid, ..lch })) {
                true => lo = mid,
                false => hi = mid,
            }
        }
        hi = lo;
    }
    from_linear(unclamped(Lch { c: hi, ..lch }))
}

/// Below this chroma a color is grey and its hue means nothing.
const GREY: f32 = 0.02;

/// Whether `c` is too grey to have a hue.
pub fn grey(c: Color) -> bool {
    lch(c).c < GREY
}

/// `other` moved as `from` moved to `to`: its hue turned by the same
/// angle, its chroma and lightness scaled by the same ratios. A grey
/// `from` has no hue or chroma to scale from: `other` takes `to`'s.
pub fn follow(to: Color, from: Color, other: Color) -> Color {
    moved(to, from, other, true)
}

/// As `follow`, with `other` keeping its lightness, and `from` itself
/// moving to `to` exactly: for an effect, whose brightness is its own,
/// where a lighter pick would only wash a follower out to white.
pub fn follow_hue(to: Color, from: Color, other: Color) -> Color {
    match other == from {
        true => to,
        false => moved(to, from, other, false),
    }
}

fn moved(to: Color, from: Color, other: Color, lightness: bool) -> Color {
    if to == from {
        return other;
    }
    let (to, from, other) = (lch(to), lch(from), lch(other));
    let ratio = |a: f32, b: f32| if b > 1e-4 { a / b } else { 1.0 };
    let (c, h) = match from.c < GREY || other.c < GREY {
        true => (to.c, to.h),
        false => (other.c * ratio(to.c, from.c), other.h + to.h - from.h),
    };
    color(Lch {
        l: match lightness {
            true => other.l * ratio(to.l, from.l),
            false => other.l,
        },
        c,
        h: h.rem_euclid(1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Color, b: Color, by: u8) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= by)
    }

    /// A light pick turns a lighter follower green, not white, when only
    /// its hue follows.
    #[test]
    fn an_effect_follower_keeps_its_lightness() {
        let (from, other, to) = ([170, 89, 255], [213, 156, 255], [32, 255, 64]);
        assert_eq!(follow(to, from, other), [255, 255, 255]);
        let got = follow_hue(to, from, other);
        assert!(got[1] > got[0] + 60 && got[1] > got[2] + 60, "{got:?}");
        assert_eq!(follow_hue(to, from, from), to);
    }

    #[test]
    fn a_color_comes_back_from_oklch() {
        for c in [
            [47, 143, 58],
            [200, 30, 40],
            [30, 80, 200],
            [128, 128, 128],
            [255, 255, 255],
        ] {
            assert!(near(color(lch(c)), c, 1), "{c:?} -> {:?}", color(lch(c)));
        }
    }

    #[test]
    fn white_is_light_and_grey() {
        let w = lch([255, 255, 255]);
        assert!((w.l - 1.0).abs() < 1e-3 && w.c < 1e-3, "{w:?}");
    }

    #[test]
    fn a_color_past_srgb_keeps_its_hue_and_loses_chroma() {
        let wild = color(Lch {
            l: 0.6,
            c: 0.5,
            h: 0.4,
        });
        let back = lch(wild);
        assert!((back.l - 0.6).abs() < 0.02, "{back:?}");
        assert!((back.h - 0.4).abs() < 0.03, "{back:?}");
    }

    #[test]
    fn a_follower_keeps_its_offset() {
        // Two greens a shade apart, both turned red.
        let (green, dark) = ([60, 160, 60], [40, 110, 40]);
        let red = [180, 40, 40];
        let moved = follow(red, green, dark);
        let (moved, red) = (lch(moved), lch(red));
        assert!((moved.h - red.h).abs() < 0.02, "the dark green turns red");
        assert!(moved.l < red.l, "and stays the darker");
        assert_eq!(follow(green, green, dark), dark, "no move, no change");
    }
}
