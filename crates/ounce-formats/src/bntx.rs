//! BNTX textures, decoded to RGBA8, and redrawn in place. Decoding reads
//! the first mip level and array layer, which is all a UI icon has.

use anyhow::{Context, Result, bail, ensure};
use texture2ddecoder as tex;

use crate::blocks::{self, Rgba};

pub struct Texture {
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// Row-major RGBA8.
    pub rgba: Vec<u8>,
}

pub fn textures(data: &[u8]) -> Result<Vec<Texture>> {
    textures_named(data, |_| true)
}

/// Only the textures whose names `keep` takes, so a big file costs only
/// what's wanted from it.
pub fn textures_named(data: &[u8], keep: impl Fn(&str) -> bool) -> Result<Vec<Texture>> {
    ensure!(data.get(..4) == Some(b"BNTX"), "not a BNTX file");
    let count = u32_at(data, 0x24)? as usize;
    let infos = u64_at(data, 0x28)?;
    let mut out = Vec::new();
    for i in 0..count {
        let brti = u64_at(data, infos + i * 8)?;
        if keep(&string(data, u64_at(data, brti + 0x60)?)?) {
            out.push(texture(data, brti)?);
        }
    }
    Ok(out)
}

fn texture(data: &[u8], brti: usize) -> Result<Texture> {
    let info = Info::read(data, brti)?;
    let raw = data
        .get(info.start..info.start + info.size)
        .context("texture data past the end")?;
    let rgba = decode(&info, raw, info.width, info.height, info.block_height)?;
    Ok(Texture {
        name: info.name,
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// What a BRTI says about its texture.
struct Info {
    name: String,
    format: u32,
    width: usize,
    height: usize,
    levels: usize,
    /// In GOBs, for the first level.
    block_height: usize,
    size: usize,
    /// Where the data starts, and where each level does.
    start: usize,
    level_starts: Vec<usize>,
}

impl Info {
    fn read(data: &[u8], brti: usize) -> Result<Self> {
        ensure!(
            data.get(brti..brti + 4) == Some(b"BRTI"),
            "no BRTI at {brti:#x}"
        );
        let field = |off| u32_at(data, brti + off);
        let levels = u16::from_le_bytes([data[brti + 0x16], data[brti + 0x17]]) as usize;
        let offsets = u64_at(data, brti + 0x70)?;
        let level_starts = (0..levels.max(1))
            .map(|i| u64_at(data, offsets + 8 * i))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            name: string(data, u64_at(data, brti + 0x60)?)?,
            format: field(0x1c)?,
            width: field(0x24)? as usize,
            height: field(0x28)? as usize,
            levels: levels.max(1),
            block_height: 1 << field(0x34)?,
            size: field(0x50)? as usize,
            start: level_starts[0],
            level_starts,
        })
    }

    /// Level `i`'s size in texels, its block height in GOBs and its bytes.
    fn level(&self, i: usize) -> Result<(usize, usize, usize, usize)> {
        let (bw, bh, unit) = block(self.format)?;
        let (w, h) = ((self.width >> i).max(1), (self.height >> i).max(1));
        let rows = h.div_ceil(bh);
        let mut gobs = self.block_height;
        while gobs > 1 && rows <= gobs / 2 * 8 {
            gobs /= 2;
        }
        let gobs_wide = (w.div_ceil(bw) * unit).div_ceil(64);
        let bytes = gobs_wide * rows.div_ceil(8 * gobs) * gobs * 512;
        Ok((w, h, gobs, bytes))
    }
}

/// One level's swizzled `raw` bytes, decoded to row-major RGBA8.
fn decode(info: &Info, raw: &[u8], width: usize, height: usize, gobs: usize) -> Result<Vec<u8>> {
    let (format, name) = (info.format, &info.name);
    let (bw, bh, bytes) = block(format)?;
    let (cols, rows) = (width.div_ceil(bw), height.div_ceil(bh));
    let linear = deswizzle(raw, cols * bytes, rows, bytes, gobs)?;
    let mut pixels = vec![0u32; width * height];
    let decoded = match format >> 8 {
        0x0b => return Ok(linear[..width * height * 4].to_vec()),
        0x1a => tex::decode_bc1(&linear, width, height, &mut pixels),
        0x1b => tex::decode_bc2(&linear, width, height, &mut pixels),
        0x1c => tex::decode_bc3(&linear, width, height, &mut pixels),
        0x1d => tex::decode_bc4(&linear, width, height, &mut pixels),
        0x1e => tex::decode_bc5(&linear, width, height, &mut pixels),
        0x20 => tex::decode_bc7(&linear, width, height, &mut pixels),
        _ => tex::decode_astc(&linear, width, height, bw, bh, &mut pixels),
    };
    decoded.map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
    // The decoder packs BGRA into each little-endian u32.
    Ok(pixels
        .iter()
        .flat_map(|p| {
            let [b, g, r, a] = p.to_le_bytes();
            [r, g, b, a]
        })
        .collect())
}

/// Every mip level of a file's first texture, decoded.
pub fn levels(data: &[u8]) -> Result<Vec<Texture>> {
    let info = Info::read(data, first(data)?)?;
    (0..info.levels)
        .map(|i| {
            let (w, h, gobs, bytes) = info.level(i)?;
            let at = info.level_starts[i];
            let raw = data.get(at..at + bytes).context("level past the end")?;
            Ok(Texture {
                name: info.name.clone(),
                width: w,
                height: h,
                rgba: decode(&info, raw, w, h, gobs)?,
            })
        })
        .collect()
}

fn first(data: &[u8]) -> Result<usize> {
    ensure!(data.get(..4) == Some(b"BNTX"), "not a BNTX file");
    u64_at(data, u64_at(data, 0x28)?)
}

/// A file of one texture with its texels redrawn from `rgba`, its first
/// level row-major at its size, in every mip level, each level the one
/// above box-filtered. A block whose texels `keep` (by first-level x and y)
/// all takes keeps its bytes. The layout stays byte for byte: only blocks
/// change. Each ASTC block is the closest of one colour or two. BC4
/// takes the red channel.
pub fn redraw(data: &[u8], rgba: &[u8], keep: impl Fn(usize, usize) -> bool) -> Result<Vec<u8>> {
    let info = Info::read(data, first(data)?)?;
    let (width, height) = (info.width, info.height);
    ensure!(
        rgba.len() == width * height * 4,
        "{}: wrong image size",
        info.name
    );
    let (bw, bh, unit) = block(info.format)?;
    let bc4 = info.format >> 8 == 0x1d;
    ensure!(
        bc4 || (0x2d..=0x3a).contains(&(info.format >> 8)),
        "{}: only ASTC and BC4 are redrawn",
        info.name
    );
    let mut out = data.to_vec();
    let mut image: Vec<Rgba> = rgba.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    for i in 0..info.levels {
        let (w, h, gobs, bytes) = info.level(i)?;
        let at = info.level_starts[i];
        if let Some(next) = info.level_starts.get(i + 1) {
            ensure!(
                at + bytes <= *next,
                "{}: level {i} overruns the next",
                info.name
            );
        }
        ensure!(
            at + bytes <= info.start + info.size,
            "{}: level {i} past the data",
            info.name
        );
        if i > 0 {
            image = halve(
                &image,
                (width >> (i - 1)).max(1),
                (height >> (i - 1)).max(1),
            );
        }
        let (cols, rows) = (w.div_ceil(bw), h.div_ceil(bh));
        let row_bytes = cols * unit;
        let gobs_wide = row_bytes.div_ceil(64);
        for by in 0..rows {
            for bx in 0..cols {
                // The first level's texels the block covers.
                let (x0, y0) = ((bx * bw) << i, (by * bh) << i);
                let (x1, y1) = (((bx + 1) * bw) << i, ((by + 1) * bh) << i);
                let kept = (y0..y1.min(height)).all(|y| (x0..x1.min(width)).all(|x| keep(x, y)));
                if kept {
                    continue;
                }
                let texels: Vec<Rgba> = (0..bh)
                    .flat_map(|y| (0..bw).map(move |x| (x, y)))
                    .map(|(x, y)| {
                        let (x, y) = ((bx * bw + x).min(w - 1), (by * bh + y).min(h - 1));
                        image[y * w + x]
                    })
                    .collect();
                let encoded: Vec<u8> = match bc4 {
                    true => {
                        let red: [u8; 16] = std::array::from_fn(|t| texels[t][0]);
                        blocks::bc4(&red).to_vec()
                    }
                    false => blocks::astc(&texels, bw, bh).to_vec(),
                };
                let (x, y) = (bx * unit, by);
                let to = at + gob_address(x, y, gobs_wide, gobs) + gob_offset(x, y);
                out[to..to + unit].copy_from_slice(&encoded);
            }
        }
    }
    Ok(out)
}

/// [`halve`] over row-major RGBA8 bytes.
pub fn redraw_halve(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    let image: Vec<Rgba> = rgba.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    halve(&image, w, h).concat()
}

/// `image` at half its size, each texel the average of the two by two it
/// covers (or fewer at an odd edge).
fn halve(image: &[Rgba], w: usize, h: usize) -> Vec<Rgba> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let near: Vec<Rgba> = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(dx, dy)| ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1)))
                .map(|(x, y)| image[y * w + x])
                .collect();
            out.push(blocks::average(&near));
        }
    }
    out
}

/// Block width, height and bytes per block for a format's high byte.
fn block(format: u32) -> Result<(usize, usize, usize)> {
    const ASTC: [(usize, usize); 14] = [
        (4, 4),
        (5, 4),
        (5, 5),
        (6, 5),
        (6, 6),
        (8, 5),
        (8, 6),
        (8, 8),
        (10, 5),
        (10, 6),
        (10, 8),
        (10, 10),
        (12, 10),
        (12, 12),
    ];
    Ok(match format >> 8 {
        0x0b => (1, 1, 4),
        0x1a | 0x1d => (4, 4, 8),
        0x1b | 0x1c | 0x1e | 0x20 => (4, 4, 16),
        f @ 0x2d..=0x3a => {
            let (w, h) = ASTC[f as usize - 0x2d];
            (w, h, 16)
        }
        _ => bail!("texture format {format:#06x} isn't decoded"),
    })
}

/// Tegra block-linear to row-major. `row_bytes` is one row of blocks,
/// `rows` the rows of blocks, `block_height` in GOBs.
fn deswizzle(
    raw: &[u8],
    row_bytes: usize,
    rows: usize,
    unit: usize,
    block_height: usize,
) -> Result<Vec<u8>> {
    let gobs_wide = row_bytes.div_ceil(64);
    let mut out = vec![0; row_bytes * rows];
    for y in 0..rows {
        for x in (0..row_bytes).step_by(unit) {
            let from = gob_address(x, y, gobs_wide, block_height) + gob_offset(x, y);
            let src = raw
                .get(from..from + unit)
                .context("swizzled data too short")?;
            out[y * row_bytes + x..][..unit].copy_from_slice(src);
        }
    }
    Ok(out)
}

/// Each GOB is 64 bytes by 8 rows, stacked `block_height` high.
fn gob_address(x: usize, y: usize, gobs_wide: usize, block_height: usize) -> usize {
    let tall = 8 * block_height;
    (y / tall) * 512 * block_height * gobs_wide
        + (x / 64) * 512 * block_height
        + (y % tall / 8) * 512
}

fn gob_offset(x: usize, y: usize) -> usize {
    (x % 64 / 32) * 256
        + (y % 8 / 4) * 128
        + (x % 32 / 16) * 64
        + (y % 4 / 2) * 32
        + (y % 2) * 16
        + x % 16
}

/// A `_STR` entry: a u16 length, then the bytes.
fn string(data: &[u8], at: usize) -> Result<String> {
    let len = u16::from_le_bytes(
        data.get(at..at + 2)
            .context("name past the end")?
            .try_into()?,
    ) as usize;
    let bytes = data
        .get(at + 2..at + 2 + len)
        .context("name past the end")?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4).context("BNTX too short")?.try_into()?,
    ))
}

fn u64_at(data: &[u8], at: usize) -> Result<usize> {
    Ok(u64::from_le_bytes(data.get(at..at + 8).context("BNTX too short")?.try_into()?) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gob_offsets_follow_the_tegra_layout() {
        assert_eq!(gob_offset(16, 0), 64);
        assert_eq!(gob_offset(0, 1), 16);
        assert_eq!(gob_offset(0, 2), 32);
        assert_eq!(gob_offset(0, 4), 128);
        assert_eq!(gob_offset(32, 0), 256);
        assert_eq!(gob_offset(63, 7), 511);
    }

    #[test]
    fn gobs_stack_down_a_block_before_moving_right() {
        assert_eq!(gob_address(0, 8, 4, 2), 512);
        assert_eq!(gob_address(64, 0, 4, 2), 1024);
        assert_eq!(gob_address(0, 16, 4, 2), 4096);
    }

    #[test]
    fn rejects_what_isnt_bntx() {
        assert!(textures(b"SARC....").is_err());
    }
}
