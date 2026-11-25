//! Recoloring texels: a band of hues moved to a color, or each texel
//! tinted by its brightness. The one copy of each, for Colors and every
//! feature that paints a model (medallions, key tints, tunics, bags).

pub type Color = [u8; 3];

/// Hues from one to another, in turns. A band whose first hue is past its
/// last wraps through red: `(0.92, 0.06)` is the reds either side of 0.
pub type Hues = (f32, f32);

/// A band of hues and the hue, saturation and value its texels move to.
pub struct Band {
    pub hues: Hues,
    pub to: (f32, f32, f32),
}

/// Below this a texel is too grey to count as its hue (a chain, a seam).
const BAND_SATURATION: f32 = 0.25;

/// A texel's hue, saturation and value, when it's colored and its hue is
/// in `hues`. A wrapping band's hues past red come back above 1, so a
/// band's hues average and subtract without the seam.
pub(crate) fn in_band(px: &[u8], (lo, hi): Hues) -> Option<(f32, f32, f32)> {
    let (h, s, v) = hsv([px[0], px[1], px[2]]);
    let h = match lo <= hi {
        true => (lo..=hi).contains(&h).then_some(h),
        false if h >= lo => Some(h),
        false => (h <= hi).then_some(h + 1.0),
    };
    (s > BAND_SATURATION).then_some((h?, s, v))
}

/// The average hue, saturation and value of `rgba`'s texels in `hues`. A
/// wrapping band's hue may come back above 1.
pub fn mean(rgba: &[u8], hues: Hues) -> Option<(f32, f32, f32)> {
    let texels: Vec<_> = rgba.chunks(4).filter_map(|px| in_band(px, hues)).collect();
    let n = texels.len() as f32;
    let sum = |f: fn(&(f32, f32, f32)) -> f32| texels.iter().map(f).sum::<f32>() / n;
    (!texels.is_empty()).then(|| (sum(|t| t.0), sum(|t| t.1), sum(|t| t.2)))
}

/// Each band's texels moved to its color: hue shifted, saturation and
/// value scaled by the band's own average to the color's, so the band
/// keeps its grain. Texels of no band, and greys, keep theirs. Every band
/// reads the texels as they were, so one band's new color never falls
/// into the next.
pub fn shift(rgba: &mut [u8], bands: &[Band]) {
    let from: Vec<_> = bands.iter().map(|b| mean(rgba, b.hues)).collect();
    for px in rgba.chunks_mut(4) {
        let moved = bands.iter().zip(&from).find_map(|(band, from)| {
            let (h, s, v) = in_band(px, band.hues)?;
            let ((fh, fs, fv), (th, ts, tv)) = ((*from)?, band.to);
            Some(rgb(
                h - fh + th,
                (s * ts / fs).min(1.0),
                (v * tv / fv).min(1.0),
            ))
        });
        if let Some(c) = moved {
            px[..3].copy_from_slice(&c);
        }
    }
}

/// Each texel of `from` in `hues` drawn over `rgba`'s, the two one size.
pub fn copy(rgba: &mut [u8], from: &[u8], hues: Hues) {
    for (px, src) in rgba.chunks_mut(4).zip(from.chunks(4)) {
        if in_band(src, hues).is_some() {
            px.copy_from_slice(src);
        }
    }
}

/// Each texel in `color`, scaled by its brightness, so a colored
/// albedo (the boss key's gold) takes the color as cleanly as a grey one.
pub fn tint(rgba: &mut [u8], color: Color) {
    for px in rgba.chunks_mut(4) {
        let luma = luma(px);
        for (c, tint) in px.iter_mut().zip(color) {
            *c = ((luma * tint as u32 + 127) / 255) as u8;
        }
    }
}

/// A metal map with no metal left (its red channel is how metal each
/// texel is), so a painted albedo shows instead of the shine.
pub fn matte(rgba: &mut [u8]) {
    rgba.chunks_mut(4).for_each(|px| px[0] = 0);
}

/// A texel's brightness: Rec. 709 luma, weights out of 256.
pub fn luma(px: &[u8]) -> u32 {
    (px[0] as u32 * 54 + px[1] as u32 * 183 + px[2] as u32 * 19 + 128) / 256
}

/// Hue in turns, saturation and value, each 0 to 1.
pub fn hsv([r, g, b]: Color) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let hue = match d {
        0.0 => 0.0,
        _ if max == r => ((g - b) / d).rem_euclid(6.0),
        _ if max == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    } / 6.0;
    let s = if max == 0.0 { 0.0 } else { d / max };
    (hue, s, max)
}

/// [`hsv`] back to a color. The hue may be any number of turns.
pub fn rgb(hue: f32, s: f32, v: f32) -> Color {
    let h = hue.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|ch| ((ch + m) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_and_back_keeps_a_color() {
        for c in [[170, 120, 255], [255, 70, 40], [90, 230, 90], [10, 10, 10]] {
            let (h, s, v) = hsv(c);
            let back = rgb(h, s, v);
            assert!(back.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 1), "{c:?}");
        }
    }

    #[test]
    fn a_band_moves_to_its_color_and_keeps_its_grain() {
        // Two tans, a grey and a red.
        let mut rgba = [
            [200, 120, 40, 9],
            [150, 90, 30, 9],
            [128, 128, 128, 9],
            [220, 30, 30, 9],
        ]
        .concat();
        let before = rgba.clone();
        let tan = (20.0 / 360.0, 40.0 / 360.0);
        let navy = (0.67, 0.6, 0.35);
        shift(
            &mut rgba,
            &[Band {
                hues: tan,
                to: navy,
            }],
        );
        let (h, s, v) = mean(&rgba, (0.6, 0.75)).unwrap();
        assert!(
            (h - navy.0).abs() < 0.01 && (s - navy.1).abs() < 0.01 && (v - navy.2).abs() < 0.01
        );
        let (light, dark) = (
            hsv([rgba[0], rgba[1], rgba[2]]),
            hsv([rgba[4], rgba[5], rgba[6]]),
        );
        assert!(light.2 > dark.2, "the lighter tan stays the lighter");
        assert_eq!(rgba[8..], before[8..], "the grey and the red keep theirs");
        assert!(rgba.chunks(4).all(|px| px[3] == 9));
    }

    #[test]
    fn a_band_through_red_takes_both_sides_and_averages_across_the_seam() {
        // A crimson just under 1 and a scarlet just over 0, and a gold.
        let mut rgba = [[220, 20, 60, 255], [220, 40, 20, 255], [220, 180, 40, 255]].concat();
        let reds = (0.9, 0.05);
        let (h, _, _) = mean(&rgba, reds).unwrap();
        assert!(
            (0.98..1.02).contains(&h),
            "the reds' middle is red, not cyan: {h}"
        );
        let green = hsv([40, 200, 40]);
        shift(
            &mut rgba,
            &[Band {
                hues: reds,
                to: green,
            }],
        );
        for px in rgba[..8].chunks(4) {
            let (h, ..) = hsv([px[0], px[1], px[2]]);
            assert!((h - green.0).abs() < 0.05, "both reds turned green: {h}");
        }
        assert_eq!(rgba[8..11], [220, 180, 40], "the gold is out of the band");
    }

    #[test]
    fn a_copy_draws_only_the_band() {
        let mut rgba = [[200, 120, 40, 255], [200, 120, 40, 255]].concat();
        let from = [[220, 30, 30, 200], [128, 128, 128, 255]].concat();
        copy(&mut rgba, &from, (0.0, 12.0 / 360.0));
        assert_eq!(rgba, [[220, 30, 30, 200], [200, 120, 40, 255]].concat());
    }

    #[test]
    fn a_grey_texel_takes_the_color_scaled_by_its_grey() {
        let mut rgba = [255, 255, 255, 255, 128, 128, 128, 200];
        tint(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [60, 220, 60, 255, 30, 110, 30, 200]);
    }

    #[test]
    fn a_gold_texel_takes_the_color_by_its_brightness_not_its_hue() {
        let mut rgba = [255, 200, 100, 255];
        tint(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [48, 176, 48, 255]);
    }
}
