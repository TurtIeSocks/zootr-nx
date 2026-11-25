//! BNTX textures, decoded to RGBA8, and redrawn in place. Decoding reads
//! the first mip level and array layer, which is all a UI icon has.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail, ensure};
use texture2ddecoder as tex;

use crate::blocks::{self, Rgba};
use crate::par::par_map;
use crate::reader::Reader;

#[derive(Clone)]
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
    let r = Reader(data);
    let count = r.offset(0x24)?;
    let infos = r.ptr(0x28)?;
    let mut out = Vec::new();
    for i in 0..count {
        let brti = r.ptr(infos + i * 8)?;
        if keep(&string(data, r.ptr(brti + 0x60)?)?) {
            out.push(texture(data, brti)?);
        }
    }
    Ok(out)
}

/// The texture at `brti`'s first level, read as [`top_level`] reads it.
fn texture(data: &[u8], brti: usize) -> Result<Texture> {
    let info = Info::read(data, brti)?;
    level(data, &info, 0)
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
        let r = Reader(data);
        let field = |off| r.offset(brti + off);
        let levels = (r.u16(brti + 0x16)? as usize).max(1);
        let offsets = r.ptr(brti + 0x70)?;
        let level_starts = (0..levels)
            .map(|i| r.ptr(offsets + 8 * i))
            .collect::<Result<Vec<_>>>()?;
        let format = r.u32(brti + 0x1c)?;
        let gobs = r.u32(brti + 0x34)?;
        Ok(Self {
            name: string(data, r.ptr(brti + 0x60)?)?,
            format,
            width: field(0x24)?,
            height: field(0x28)?,
            levels,
            block_height: 1usize
                .checked_shl(gobs)
                .with_context(|| format!("a block height of 2^{gobs} GOBs"))?,
            size: field(0x50)?,
            start: level_starts[0],
            level_starts,
        })
    }

    fn kind(&self) -> Result<Kind> {
        Kind::of(self.format)
    }

    /// Level `i`'s size in texels, its block height in GOBs and its bytes.
    fn level(&self, i: usize) -> Result<(usize, usize, usize, usize)> {
        let (bw, bh, unit) = self.kind()?.block();
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
    let kind = info.kind()?;
    let (bw, bh, bytes) = kind.block();
    let (cols, rows) = (width.div_ceil(bw), height.div_ceil(bh));
    let linear = deswizzle(raw, cols * bytes, rows, bytes, gobs)?;
    let mut pixels = vec![0u32; width * height];
    let decoded = match kind {
        Kind::Rgba8 => return Ok(linear[..width * height * 4].to_vec()),
        Kind::Bc1 => tex::decode_bc1(&linear, width, height, &mut pixels),
        Kind::Bc2 => tex::decode_bc2(&linear, width, height, &mut pixels),
        Kind::Bc3 => tex::decode_bc3(&linear, width, height, &mut pixels),
        Kind::Bc4 => tex::decode_bc4(&linear, width, height, &mut pixels),
        Kind::Bc5 => tex::decode_bc5(&linear, width, height, &mut pixels),
        Kind::Bc7 => tex::decode_bc7(&linear, width, height, &mut pixels),
        Kind::Astc(..) => tex::decode_astc(&linear, width, height, bw, bh, &mut pixels),
    };
    decoded.map_err(|e| anyhow::anyhow!("{}: {e}", info.name))?;
    Ok(pixels
        .iter()
        .flat_map(|&p| blocks::from_packed(p))
        .collect())
}

/// Every mip level of a file's first texture, decoded.
pub fn levels(data: &[u8]) -> Result<Vec<Texture>> {
    let info = Info::read(data, first(data)?)?;
    (0..info.levels).map(|i| level(data, &info, i)).collect()
}

/// A file's first texture's first mip level, decoded: the one [`redraw`]
/// takes.
pub fn top_level(data: &[u8]) -> Result<Texture> {
    level(data, &Info::read(data, first(data)?)?, 0)
}

fn level(data: &[u8], info: &Info, i: usize) -> Result<Texture> {
    let (w, h, gobs, bytes) = info.level(i)?;
    let at = info.level_starts[i];
    let raw = data.get(at..at + bytes).context("level past the end")?;
    Ok(Texture {
        name: info.name.clone(),
        width: w,
        height: h,
        rgba: decode(info, raw, w, h, gobs)?,
    })
}

fn first(data: &[u8]) -> Result<usize> {
    ensure!(data.get(..4) == Some(b"BNTX"), "not a BNTX file");
    let r = Reader(data);
    r.ptr(r.ptr(0x28)?)
}

/// A file's first texture's format, as its BRTI stores it.
pub fn format(data: &[u8]) -> Result<u32> {
    Ok(Info::read(data, first(data)?)?.format)
}

/// A texture format's kind, its high byte: what decodes it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Rgba8,
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc7,
    /// ASTC, by its block's width and height.
    Astc(usize, usize),
}

impl Kind {
    fn of(format: u32) -> Result<Self> {
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
            0x0b => Kind::Rgba8,
            0x1a => Kind::Bc1,
            0x1b => Kind::Bc2,
            0x1c => Kind::Bc3,
            0x1d => Kind::Bc4,
            0x1e => Kind::Bc5,
            0x20 => Kind::Bc7,
            f @ 0x2d..=0x3a => {
                let (w, h) = ASTC[f as usize - 0x2d];
                Kind::Astc(w, h)
            }
            _ => bail!("texture format {format:#06x} isn't decoded"),
        })
    }

    /// Block width, height and bytes per block.
    fn block(self) -> (usize, usize, usize) {
        match self {
            Kind::Rgba8 => (1, 1, 4),
            Kind::Bc1 | Kind::Bc4 => (4, 4, 8),
            Kind::Bc2 | Kind::Bc3 | Kind::Bc5 | Kind::Bc7 => (4, 4, 16),
            Kind::Astc(w, h) => (w, h, 16),
        }
    }
}

/// Whether [`redraw`] redraws a texture of `format`: ASTC, BC1, BC3, BC4
/// or BC7 (as ASTC).
pub fn redraws(format: u32) -> bool {
    matches!(
        Kind::of(format),
        Ok(Kind::Bc1 | Kind::Bc3 | Kind::Bc4 | Kind::Bc7 | Kind::Astc(..))
    )
}

/// ASTC 4x4's format byte. Its blocks are BC7's size and shape, so a BC7
/// texture becomes one in place: same levels, same layout, every block
/// encoded again and the format word changed.
const ASTC_4X4: u32 = 0x2d;

/// A file of one texture with its texels redrawn from `rgba`, its first
/// level row-major at its size, in every mip level, each level the one
/// above box-filtered. A block whose texels `keep` (by first-level x and y)
/// all takes keeps its bytes. The layout stays byte for byte: only blocks
/// change. Each ASTC block is the closest of one colour or two, each BC1
/// block of four. BC4 takes the red channel. BC7, which isn't encoded,
/// comes back as ASTC 4x4, every block encoded and `keep` ignored.
///
/// Each redraw is kept for the process, by its inputs: a seed redraws the
/// same textures in the same colours (a key's dungeon, a mark's kind), and
/// a process makes many seeds (the app, the tests), so each encodes once.
pub fn redraw(
    data: &[u8],
    rgba: &[u8],
    keep: impl Fn(usize, usize) -> bool + Sync,
) -> Result<Vec<u8>> {
    let info = Info::read(data, first(data)?)?;
    let (width, height) = (info.width, info.height);
    ensure!(
        rgba.len() == width * height * 4,
        "{}: wrong image size",
        info.name
    );
    let kept: Vec<bool> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| keep(x, y))
        .collect();
    let key = redraw_key(data, rgba, &kept);
    if let Some(out) = redrawn().lock().unwrap().get(&key) {
        return Ok(out.clone());
    }
    let out = encode(data, &info, rgba, &kept)?;
    redrawn().lock().unwrap().insert(key, out.clone());
    Ok(out)
}

/// The files [`redraw`] made, by [`redraw_key`].
fn redrawn() -> &'static Mutex<HashMap<[u64; 2], Vec<u8>>> {
    static REDRAWN: OnceLock<Mutex<HashMap<[u64; 2], Vec<u8>>>> = OnceLock::new();
    REDRAWN.get_or_init(Default::default)
}

/// A redraw's inputs hashed twice over, salted apart: 128 bits, so two
/// different redraws never share a key in practice.
fn redraw_key(data: &[u8], rgba: &[u8], kept: &[bool]) -> [u64; 2] {
    [0u8, 1].map(|salt| {
        let mut h = DefaultHasher::new();
        (salt, data, rgba, kept).hash(&mut h);
        h.finish()
    })
}

/// [`redraw`]'s work: `kept` is `keep` for each first-level texel,
/// row-major.
fn encode(data: &[u8], info: &Info, rgba: &[u8], kept: &[bool]) -> Result<Vec<u8>> {
    let (width, height) = (info.width, info.height);
    ensure!(
        redraws(info.format),
        "{}: only ASTC, BC1, BC3, BC4 and BC7 are redrawn",
        info.name
    );
    let from_bc7 = info.kind()? == Kind::Bc7;
    let kind = if from_bc7 { Kind::Astc(4, 4) } else { info.kind()? };
    let (bw, bh, unit) = kind.block();
    ensure!(
        info.start + info.size <= data.len(),
        "{}: texture data past the end",
        info.name
    );
    let mut out = data.to_vec();
    if from_bc7 {
        let at = first(data)? + 0x1c;
        let format = ASTC_4X4 << 8 | info.format & 0xff;
        out[at..at + 4].copy_from_slice(&format.to_le_bytes());
    }
    let mut image = blocks::texels(rgba);
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
        // Each block encodes on its own, so they encode at once.
        let at_blocks: Vec<(usize, usize)> = (0..rows)
            .flat_map(|by| (0..cols).map(move |bx| (bx, by)))
            .collect();
        let encoded = par_map(&at_blocks, |&(bx, by)| {
            // The first level's texels the block covers.
            let (x0, y0) = ((bx * bw) << i, (by * bh) << i);
            let (x1, y1) = (((bx + 1) * bw) << i, ((by + 1) * bh) << i);
            let all_kept =
                (y0..y1.min(height)).all(|y| (x0..x1.min(width)).all(|x| kept[y * width + x]));
            if all_kept && !from_bc7 {
                return None;
            }
            let texels: Vec<Rgba> = (0..bh)
                .flat_map(|y| (0..bw).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let (x, y) = ((bx * bw + x).min(w - 1), (by * bh + y).min(h - 1));
                    image[y * w + x]
                })
                .collect();
            Some(match kind {
                Kind::Bc1 => blocks::bc1(&texels).to_vec(),
                Kind::Bc4 => {
                    let red: [u8; 16] = std::array::from_fn(|t| texels[t][0]);
                    blocks::bc4(&red).to_vec()
                }
                Kind::Bc3 => blocks::bc3(&texels).to_vec(),
                // ASTC, the one kind left that `redraws` takes.
                _ => blocks::astc(&texels, bw, bh).to_vec(),
            })
        });
        for (&(bx, by), encoded) in at_blocks.iter().zip(encoded) {
            let Some(encoded) = encoded else { continue };
            let (x, y) = (bx * unit, by);
            let to = at + gob_address(x, y, gobs_wide, gobs) + gob_offset(x, y);
            out[to..to + unit].copy_from_slice(&encoded);
        }
    }
    Ok(out)
}

/// [`halve`] over row-major RGBA8 bytes.
pub fn redraw_halve(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    halve(&blocks::texels(rgba), w, h).concat()
}

/// `image` at half its size, each texel the average of the two by two it
/// covers. An odd edge's last column or row is dropped; a one-texel edge
/// repeats its texel.
fn halve(image: &[Rgba], w: usize, h: usize) -> Vec<Rgba> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let near = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| {
                let (x, y) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                image[y * w + x]
            });
            out.push(blocks::average(&near));
        }
    }
    out
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
    let r = Reader(data);
    let bytes = r.bytes(at + 2, r.u16(at)? as usize)?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
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
    fn kinds_name_their_blocks_and_which_redraw() {
        assert_eq!(Kind::of(0x2d06).unwrap().block(), (4, 4, 16));
        assert_eq!(Kind::of(0x3a06).unwrap().block(), (12, 12, 16));
        assert_eq!(Kind::of(0x1a01).unwrap().block(), (4, 4, 8));
        assert!(Kind::of(0x2101).is_err());
        assert!(redraws(0x1d01) && redraws(0x3001) && redraws(0x1c06) && redraws(0x2001));
        assert!(!redraws(0x1e01) && !redraws(0x0b01));
    }

    #[test]
    fn halving_drops_an_odd_edge_and_repeats_a_thin_one() {
        let px = |v: u8| [v, v, v, 255];
        let image = [px(0), px(10), px(90), px(20), px(30), px(90)];
        assert_eq!(halve(&image, 3, 2), [px(15)]);
        assert_eq!(halve(&[px(8), px(4)], 1, 2), [px(6)]);
    }

    #[test]
    fn rejects_what_isnt_bntx() {
        assert!(textures(b"SARC....").is_err());
    }
}
