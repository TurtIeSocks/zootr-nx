//! Texture block encoders, the few shapes a redrawn texture needs: an ASTC
//! block of one colour or of two, a BC1 block of four colours, and a BC4
//! block of up to eight values.
//!
//! A two-colour ASTC block is the simplest real one: one partition, a
//! weight per texel at the block's own size, and endpoints at full 8-bit
//! precision, so every field is plain binary.

/// One RGBA8 texel.
pub type Rgba = [u8; 4];

/// A texel as the decoder packs it: BGRA in a little-endian u32.
pub fn from_packed(p: u32) -> Rgba {
    let [b, g, r, a] = p.to_le_bytes();
    [r, g, b, a]
}

/// Row-major RGBA8 bytes as texels; a trailing partial texel is dropped.
pub fn texels(rgba: &[u8]) -> Vec<Rgba> {
    rgba.as_chunks::<4>().0.to_vec()
}

/// An ASTC block of one colour (the void-extent block): its mode bits and
/// an extent of "none", then RGBA as 16-bit values.
pub fn astc_solid(c: Rgba) -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(&0xFFFF_FFFF_FFFF_FDFCu64.to_le_bytes());
    for (i, v) in c.iter().enumerate() {
        out[8 + 2 * i..10 + 2 * i].copy_from_slice(&(*v as u16 * 257).to_le_bytes());
    }
    out
}

/// Endpoint modes: grey with alpha, colour, and a colour with a darker
/// copy of itself, each with an alpha of its own.
const LUMINANCE_ALPHA: u32 = 4;
const RGB: u32 = 8;
const RGB_SCALE_ALPHA: u32 = 10;
/// Where a block's fields start, from bit 0: its 11-bit block mode, then
/// (past a 2-bit partition count of one) its 4-bit endpoint mode, then the
/// endpoints.
const BLOCK_MODE: usize = 0;
const BLOCK_MODE_BITS: usize = 11;
const ENDPOINT_MODE: usize = 13;
const ENDPOINT_MODE_BITS: usize = 4;
const HEADER: usize = ENDPOINT_MODE + ENDPOINT_MODE_BITS;

/// The ASTC block closest to `texels` (row-major, `w` by `h`): one colour,
/// or two on whichever weight grid that fits decodes nearest.
pub fn astc(texels: &[Rgba], w: usize, h: usize) -> [u8; 16] {
    let solid = astc_solid(average(texels));
    two_colour(texels, w, h)
        .chain([solid])
        .min_by_key(|b| error(b, texels, w, h))
        .unwrap_or(solid)
}

/// The summed squared difference between `block` decoded and `texels`, or
/// the most there is when it doesn't decode.
fn error(block: &[u8; 16], texels: &[Rgba], w: usize, h: usize) -> u64 {
    let mut px = vec![0u32; w * h];
    if texture2ddecoder::decode_astc(block, w, h, w, h, &mut px).is_err() {
        return u64::MAX;
    }
    px.iter()
        .zip(texels)
        .map(|(&p, t)| {
            from_packed(p)
                .iter()
                .zip(t)
                .map(|(x, y)| (x.abs_diff(*y) as u64).pow(2))
                .sum::<u64>()
        })
        .sum()
}

/// `w` by `h` texels between two colours, on each weight grid with room.
fn two_colour(texels: &[Rgba], w: usize, h: usize) -> impl Iterator<Item = [u8; 16]> + '_ {
    let grey = texels.iter().all(|t| t[0] == t[1] && t[1] == t[2]);
    let opaque = texels.iter().all(|t| t[3] == 255);
    // The endpoint values in the order the mode stores them, the colours
    // they decode to, and the channels that place a texel between them.
    let (mode, values, e0, e1, channels): (u32, Vec<u8>, Rgba, Rgba, &[usize]) =
        match (grey, opaque) {
            (true, _) => {
                let (a, b) = ends(texels, &[0, 3]);
                (LUMINANCE_ALPHA, vec![a[0], b[0], a[3], b[3]], a, b, &[0, 3])
            }
            (false, true) => {
                let rgb = &[0, 1, 2];
                let (mut a, mut b) = ends(texels, rgb);
                if sum(b, rgb) < sum(a, rgb) {
                    // The decoder swaps (and blends) endpoints in that order.
                    std::mem::swap(&mut a, &mut b);
                }
                let values = vec![a[0], b[0], a[1], b[1], a[2], b[2]];
                (RGB, values, a, b, rgb)
            }
            (false, false) => {
                let (mut lo, mut hi) = ends(texels, &[0, 1, 2, 3]);
                if sum(hi, &[0, 1, 2]) < sum(lo, &[0, 1, 2]) {
                    std::mem::swap(&mut lo, &mut hi);
                }
                let bright = sum(hi, &[0, 1, 2]).max(1);
                let scale = (256 * sum(lo, &[0, 1, 2]) / bright).min(255) as u8;
                let dim = |c: u8| ((c as u32 * scale as u32) >> 8) as u8;
                let e0 = [dim(hi[0]), dim(hi[1]), dim(hi[2]), lo[3]];
                let values = vec![hi[0], hi[1], hi[2], scale, lo[3], hi[3]];
                (RGB_SCALE_ALPHA, values, e0, hi, &[0, 1, 2, 3])
            }
        };
    // A grid smaller than the block is blended across it by the decoder.
    let grids = std::iter::once((w, h)).chain((2..w.min(h)).rev().map(|k| (k, k)));
    let endpoint_bits = 8 * values.len();
    grids
        .flat_map(|(gw, gh)| [3usize, 2, 1].map(|b| (gw, gh, b)))
        .filter(move |&(gw, gh, b)| {
            let n = gw * gh * b;
            HEADER + n + endpoint_bits <= 128 && (24..=96).contains(&n)
        })
        .filter_map(move |(gw, gh, bits)| {
            let mode_bits = block_mode(gw, gh, bits)?;
            let levels = (1u32 << bits) - 1;
            // Each grid point takes the texel nearest it.
            let at =
                |j: usize, grid: usize, size: usize| (j * (size - 1) + (grid - 1) / 2) / (grid - 1);
            let mut block = Bits::default();
            block.put(BLOCK_MODE, BLOCK_MODE_BITS, mode_bits as u128);
            block.put(ENDPOINT_MODE, ENDPOINT_MODE_BITS, mode as u128);
            for (i, v) in values.iter().enumerate() {
                block.put(HEADER + 8 * i, 8, *v as u128);
            }
            let points = (0..gh).flat_map(|j| (0..gw).map(move |i| (i, j)));
            for (n, (i, j)) in points.enumerate() {
                let t = texels[at(j, gh, h) * w + at(i, gw, w)];
                let weight = (project(t, e0, e1, channels) * levels as f32).round() as u32;
                for b in 0..bits {
                    if weight >> b & 1 == 1 {
                        block.put(127 - (n * bits + b), 1, 1);
                    }
                }
            }
            Some(block.0.to_le_bytes())
        })
}

/// The 11 bits naming a `w` by `h` weight grid of `bits` per weight, in the
/// layouts that fit the grids a block uses.
fn block_mode(w: usize, h: usize, bits: usize) -> Option<u32> {
    // Weight ranges 2, 4 and 8: the quant mode's low bit, its high two.
    let r: u32 = match bits {
        1 => 2,
        2 => 4,
        3 => 7,
        _ => return None,
    };
    let (w, h) = (w as u32, h as u32);
    let low = (r & 1) << 4;
    // Two-bit A and B, with the range's high bits low.
    let first = |kind: u32, a: u32, b: u32| (b << 7) | (a << 5) | low | (kind << 2) | (r >> 1);
    match (w, h) {
        (4..=7, 2..=5) => Some(first(0, h - 2, w - 4)),
        (8..=11, 2..=5) => Some(first(1, h - 2, w - 8)),
        (2..=5, 8..=11) => Some(first(2, w - 2, h - 8)),
        // A and B six up, the range's high bits above the low one.
        (6..=9, 6..=9) => Some(((h - 6) << 9) | (2 << 7) | ((w - 6) << 5) | low | ((r >> 1) << 2)),
        _ => None,
    }
}

/// The two texels farthest apart along the block's main axis of change.
fn ends(texels: &[Rgba], channels: &[usize]) -> (Rgba, Rgba) {
    let n = texels.len() as f32;
    let k = channels.len();
    let mut mean = [0.0; 4];
    for (m, &c) in mean.iter_mut().zip(channels) {
        *m = texels.iter().map(|t| t[c] as f32).sum::<f32>() / n;
    }
    let mean = &mean[..k];
    let centred: Vec<Picked> = (texels.iter())
        .map(|t| picked(channels, |i, c| t[c] as f32 - mean[i]))
        .collect();
    // Power iteration on the covariance, from the farthest texel.
    let far = (centred.iter())
        .max_by(|a, b| norm_sq(&a[..k]).total_cmp(&norm_sq(&b[..k])))
        .copied()
        .unwrap_or_default();
    let mut axis = far;
    for _ in 0..4 {
        let mut next = [0.0; 4];
        for d in &centred {
            let along = dot(&d[..k], &axis[..k]);
            for (n, v) in next[..k].iter_mut().zip(&d[..k]) {
                *n += along * v;
            }
        }
        if norm_sq(&next[..k]) == 0.0 {
            break;
        }
        axis = next;
    }
    let along: Vec<f32> = centred.iter().map(|d| dot(&d[..k], &axis[..k])).collect();
    let by_along = |a: &(&Rgba, &f32), b: &(&Rgba, &f32)| a.1.total_cmp(b.1);
    let lo = texels.iter().zip(&along).min_by(by_along);
    let hi = texels.iter().zip(&along).max_by(by_along);
    (*lo.expect("a texel").0, *hi.expect("a texel").0)
}

/// Up to four channels' values, the rest zero: a block's channels without
/// a heap allocation per texel.
type Picked = [f32; 4];

/// `value(i, channel)` for each of `channels`.
fn picked(channels: &[usize], value: impl Fn(usize, usize) -> f32) -> Picked {
    let mut out = [0.0; 4];
    for (i, (o, &c)) in out.iter_mut().zip(channels).enumerate() {
        *o = value(i, c);
    }
    out
}

/// Where `t` falls from `e0` (0) to `e1` (1).
fn project(t: Rgba, e0: Rgba, e1: Rgba, channels: &[usize]) -> f32 {
    let k = channels.len();
    let d = picked(channels, |_, c| e1[c] as f32 - e0[c] as f32);
    let p = picked(channels, |_, c| t[c] as f32 - e0[c] as f32);
    let len = dot(&d[..k], &d[..k]);
    match len {
        0.0 => 0.0,
        _ => (dot(&p[..k], &d[..k]) / len).clamp(0.0, 1.0),
    }
}

/// Each channel's mean, rounded.
pub fn average(texels: &[Rgba]) -> Rgba {
    let n = texels.len() as u32;
    std::array::from_fn(|c| ((texels.iter().map(|t| t[c] as u32).sum::<u32>() + n / 2) / n) as u8)
}

fn sum(e: Rgba, channels: &[usize]) -> u32 {
    channels.iter().map(|&c| e[c] as u32).sum()
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm_sq(a: &[f32]) -> f32 {
    dot(a, a)
}

/// A 128-bit block, filled a field at a time from bit 0 up.
#[derive(Default)]
struct Bits(u128);

impl Bits {
    fn put(&mut self, at: usize, len: usize, v: u128) {
        self.0 |= (v & ((1u128 << len) - 1)) << at;
    }
}

/// A BC1 block of 16 texels: the darkest and brightest as endpoints, and
/// each texel's nearest of the four colours they make. With any texel
/// mostly transparent, the three-colour mode, those texels transparent.
pub fn bc1(texels: &[Rgba]) -> [u8; 8] {
    let luma = |c: &Rgba| c[0] as u32 * 54 + c[1] as u32 * 183 + c[2] as u32 * 19;
    let seen = || texels.iter().filter(|c| c[3] >= 128);
    let (Some(lo), Some(hi)) = (
        seen().min_by_key(|c| luma(c)),
        seen().max_by_key(|c| luma(c)),
    ) else {
        // Every texel transparent: both endpoints black, every index 3.
        return [0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];
    };
    let clear = texels.iter().any(|c| c[3] < 128);
    let (mut c0, mut c1) = (rgb565(hi), rgb565(lo));
    // Four colours when the first endpoint is the greater, else three and
    // transparent.
    if (c0 < c1) != clear {
        std::mem::swap(&mut c0, &mut c1);
    }
    let (a, b) = (rgb888(c0), rgb888(c1));
    let mix = |wa: u32, wb: u32| -> [u32; 3] {
        std::array::from_fn(|i| (wa * a[i] + wb * b[i]) / (wa + wb))
    };
    let (palette, colours) = match c0 > c1 {
        true => ([a, b, mix(2, 1), mix(1, 2)], 4),
        false => ([a, b, mix(1, 1), [0; 3]], 3),
    };
    let mut indices = 0u32;
    for (i, c) in texels.iter().enumerate() {
        let index = match c[3] < 128 {
            true => 3,
            false => (0..colours)
                .min_by_key(|&p| {
                    (0..3)
                        .map(|k| (palette[p][k] as i32 - c[k] as i32).pow(2))
                        .sum::<i32>()
                })
                .expect("a palette") as u32,
        };
        indices |= index << (2 * i);
    }
    let mut out = [0; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..].copy_from_slice(&indices.to_le_bytes());
    out
}

fn rgb565(c: &Rgba) -> u16 {
    let q = |v: u8, bits: u32| ((v as u32 * ((1 << bits) - 1) + 127) / 255) as u16;
    q(c[0], 5) << 11 | q(c[1], 6) << 5 | q(c[2], 5)
}

fn rgb888(c: u16) -> [u32; 3] {
    let (r, g, b) = ((c >> 11) as u32, (c >> 5 & 0x3f) as u32, (c & 0x1f) as u32);
    [r * 255 / 31, g * 255 / 63, b * 255 / 31]
}

/// A BC3 block of 16 texels: their alpha as a BC4 block, then their
/// colour as a BC1 block in its four-colour mode, the only one BC3 reads.
pub fn bc3(texels: &[Rgba]) -> [u8; 16] {
    let alpha: [u8; 16] = std::array::from_fn(|t| texels[t][3]);
    let opaque: Vec<Rgba> = texels.iter().map(|c| [c[0], c[1], c[2], 255]).collect();
    let mut block = [0; 16];
    block[..8].copy_from_slice(&bc4(&alpha));
    block[8..].copy_from_slice(&bc1(&opaque));
    block
}

/// A BC4 block of 16 values: the darkest and brightest, and each value's
/// nearest of the eight between them.
pub fn bc4(values: &[u8; 16]) -> [u8; 8] {
    let (lo, hi) = (*values.iter().min().unwrap(), *values.iter().max().unwrap());
    if lo == hi {
        return [hi, lo, 0, 0, 0, 0, 0, 0];
    }
    // With the first endpoint brighter, codes 0 and 1 are the endpoints and
    // 2 to 7 step from the first toward the second.
    let level = |i: u32| match i {
        0 => hi as f32,
        1 => lo as f32,
        _ => ((8 - i) as f32 * hi as f32 + (i - 1) as f32 * lo as f32) / 7.0,
    };
    let mut codes = 0u64;
    for (i, &v) in values.iter().enumerate() {
        let code = (0..8)
            .min_by(|&a, &b| {
                (level(a) - v as f32)
                    .abs()
                    .total_cmp(&(level(b) - v as f32).abs())
            })
            .expect("eight codes");
        codes |= (code as u64) << (3 * i);
    }
    let mut out = [hi, lo, 0, 0, 0, 0, 0, 0];
    out[2..].copy_from_slice(&codes.to_le_bytes()[..6]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use texture2ddecoder as tex;

    /// One BC1 block decoded to RGBA8, its three-colour mode's last
    /// colour transparent.
    fn decode_bc1(block: &[u8; 8]) -> Vec<Rgba> {
        let mut px = vec![0u32; 16];
        tex::decode_bc1a(block, 4, 4, &mut px).unwrap();
        px.into_iter().map(from_packed).collect()
    }

    #[test]
    fn a_bc1_block_of_two_colours_and_a_third_between_decodes_within_565() {
        let (dark, light) = ([40, 20, 200, 255], [220, 180, 30, 255]);
        let third = average(&[dark, dark, light]);
        let texels: Vec<Rgba> = (0..16).map(|i| [dark, light, third][i % 3]).collect();
        let back = decode_bc1(&bc1(&texels));
        assert!(worst(&back, &texels) <= 8, "{back:?}");
    }

    #[test]
    fn a_bc1_block_keeps_its_transparent_texels_transparent() {
        let texels: Vec<Rgba> = (0..16)
            .map(|i| match i % 2 {
                0 => [200, 100, 50, 255],
                _ => [0, 0, 0, 0],
            })
            .collect();
        let back = decode_bc1(&bc1(&texels));
        assert!(
            back.iter()
                .zip(&texels)
                .all(|(b, t)| (b[3] < 128) == (t[3] < 128))
        );
        assert!(worst(&back[..1], &texels[..1]) <= 8);
    }

    #[test]
    fn a_bc1_block_of_one_colour_decodes_to_it() {
        let texels = [[128, 64, 32, 255]; 16];
        assert!(worst(&decode_bc1(&bc1(&texels)), &texels) <= 4);
    }

    #[test]
    fn a_bc3_block_keeps_its_colour_and_its_alpha() {
        let (dark, light) = ([40, 20, 200, 30], [220, 180, 30, 250]);
        let texels: Vec<Rgba> = (0..16).map(|i| [dark, light][i % 2]).collect();
        let mut px = vec![0u32; 16];
        tex::decode_bc3(&bc3(&texels), 4, 4, &mut px).unwrap();
        let back: Vec<Rgba> = px.into_iter().map(from_packed).collect();
        assert!(worst(&back, &texels) <= 8, "{back:?}");
    }

    /// One ASTC block decoded to RGBA8.
    fn decode(block: &[u8; 16], w: usize, h: usize) -> Vec<Rgba> {
        let mut px = vec![0u32; w * h];
        tex::decode_astc(block, w, h, w, h, &mut px).unwrap();
        px.into_iter().map(from_packed).collect()
    }

    fn worst(a: &[Rgba], b: &[Rgba]) -> u8 {
        a.iter()
            .zip(b)
            .flat_map(|(x, y)| (0..4).map(move |c| x[c].abs_diff(y[c])))
            .max()
            .unwrap()
    }

    #[test]
    fn a_solid_block_decodes_to_its_colour() {
        let c = [200, 30, 90, 255];
        assert_eq!(decode(&astc_solid(c), 12, 12), vec![c; 144]);
    }

    #[test]
    fn a_two_colour_block_keeps_a_glyph_edge() {
        // Half a block bright, half dark, as an icon's edge is.
        let (dark, bright) = ([40, 20, 90, 255], [230, 210, 255, 255]);
        let texels: Vec<Rgba> = (0..48)
            .map(|i| if i % 8 < 3 { bright } else { dark })
            .collect();
        let block = astc(&texels, 8, 6);
        assert!(worst(&decode(&block, 8, 6), &texels) <= 2);
    }

    #[test]
    fn a_grey_block_keeps_its_ramp_and_alpha() {
        // A normal map's: X in the colour, Y in alpha.
        let texels: Vec<Rgba> = (0..25u8)
            .map(|i| [100 + 4 * i, 100 + 4 * i, 100 + 4 * i, 200 - 4 * i])
            .collect();
        let block = astc(&texels, 5, 5);
        assert!(worst(&decode(&block, 5, 5), &texels) <= 8);
    }

    #[test]
    fn a_colour_with_alpha_scales_toward_black() {
        let (dim, bright) = ([30, 60, 15, 90], [120, 240, 60, 250]);
        let texels: Vec<Rgba> = (0..48)
            .map(|i| if i % 2 == 0 { dim } else { bright })
            .collect();
        let block = astc(&texels, 8, 6);
        assert!(worst(&decode(&block, 8, 6), &texels) <= 3);
    }

    #[test]
    fn a_big_block_blends_a_smaller_grid_across_a_gradient() {
        let texels: Vec<Rgba> = (0..144)
            .map(|i| {
                let v = (40 + (i % 12) * 15) as u8;
                [v, v / 2, 255 - v, 255]
            })
            .collect();
        let block = astc(&texels, 12, 12);
        assert!(worst(&decode(&block, 12, 12), &texels) <= 24);
    }

    #[test]
    fn a_bc4_block_decodes_to_its_nearest_levels() {
        let mut values = [0u8; 16];
        for (i, v) in values.iter_mut().enumerate() {
            *v = (i * 17) as u8;
        }
        let mut px = vec![0u32; 16];
        tex::decode_bc4(&bc4(&values), 4, 4, &mut px).unwrap();
        for (p, v) in px.iter().zip(values) {
            let red = p.to_le_bytes()[2];
            assert!(red.abs_diff(v) <= 19, "{red} for {v}");
        }
        let mut flat = vec![0u32; 16];
        tex::decode_bc4(&bc4(&[255; 16]), 4, 4, &mut flat).unwrap();
        assert!(flat.iter().all(|p| p.to_le_bytes()[2] == 255));
    }
}
