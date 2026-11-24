//! Terrain: the heightfield ground a level places as a terrain actor
//! (the Deku Tree's `InsideTheDekuTree_01` to `_05`), read into walls like
//! a piece's mesh. Its collision isn't shipped: the game builds it from
//! the heights, so derive does too.
//!
//! The actor's pack holds `TerrainArc/NAME.tscb`, whose header gives the
//! terrain's size and its top height. The heights live beside it in the
//! dump as a quadtree of tiles, `TerrainArc/NAME/LLIIIIIIII.hght`: `10` is
//! the root over the whole terrain, `0N` a tile of level N, its square
//! `size / 2^N` wide and placed by its Morton index `I`. A finer tile
//! replaces the coarser ones under it (`docs/research/room-science.md`,
//! "Terrain height format").

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{ActorPack, Romfs};

use super::mesh::Wall;
use super::shape::place;
use crate::warp::Placement;

const DIR: &str = "TerrainArc/";
/// Samples along a tile's side: 256 cells, the last sample its far edge.
const SAMPLES: usize = 257;
/// The heights' block-linear layout: 64-byte GOBs 8 rows tall, 16 GOBs to
/// a block.
const GOB_ROWS: usize = 8;
const BLOCK_GOBS: usize = 16;
/// The spacing derive samples terrain at, in m: finer tiles are read every
/// few samples so a level's triangle count stays near its root tile's.
const SPACING: f32 = 0.5;
/// Cells along a wall's side: terrain goes in square walls this size, so
/// their bounds keep a test to the few near it.
const CHUNK: usize = 16;
/// What a terrain wall's name ends with, after its actor's.
pub const SUFFIX: &str = "__Terrain";

/// One tile's heights, row by row along +z, each along +x, as fractions of
/// the terrain's top. 0.0 is a hole.
#[derive(Clone, Debug)]
struct Tile {
    level: u32,
    x: u32,
    z: u32,
    heights: Vec<f32>,
}

impl Tile {
    fn at(&self, col: usize, row: usize) -> f32 {
        self.heights[row * SAMPLES + col]
    }
}

/// The level and Morton index a height file's name gives, when it's one
/// derive reads: the root (`10`) or a level of its quadtree (`0N`).
fn tile_key(file: &str) -> Option<(u32, u32)> {
    let stem = file.strip_suffix(".hght")?;
    let (head, index) = (
        stem.get(..2)?,
        u32::from_str_radix(stem.get(2..)?, 16).ok()?,
    );
    match head.as_bytes() {
        b"10" => Some((0, index)),
        [b'0', n @ b'1'..=b'9'] => Some((u32::from(n - b'0'), index)),
        _ => None,
    }
}

/// The column and row of Morton index `i`, x on the low bit.
fn morton(i: u32) -> (u32, u32) {
    let spread = |v: u32| (0..16).fold(0, |acc, b| acc | (((v >> (2 * b)) & 1) << b));
    (spread(i), spread(i >> 1))
}

/// The byte offset of `(x_bytes, row)` in a block-linear surface whose rows
/// are `gobs_x` GOBs wide.
fn swizzled(x_bytes: usize, row: usize, gobs_x: usize) -> usize {
    let rows_per_block = GOB_ROWS * BLOCK_GOBS;
    let block = row / rows_per_block;
    let gob_in_block = (row % rows_per_block) / GOB_ROWS;
    let (xi, yi) = (x_bytes % 64, row % GOB_ROWS);
    let gob = (block * gobs_x + x_bytes / 64) * BLOCK_GOBS + gob_in_block;
    // Inside a GOB, 16-byte runs two rows tall, their bits interleaved
    // from y up: found by matching neighbours, not the textbook GOB.
    let unit = ((yi / 2) % 2) | (((xi / 16) % 2) << 1) | ((yi / 4) << 2) | ((xi / 32) << 3);
    gob * 512 + unit * 32 + (yi % 2) * 16 + xi % 16
}

/// A `.hght` file's heights: a `TARB` header, then a zstd frame holding
/// two blobs, each named by a self-relative offset and a size. The second
/// is the heights, block-linear.
fn heights(file: &[u8]) -> Result<Vec<f32>> {
    ensure!(file.starts_with(b"TARB"), "not a TARB file");
    let data = zstd::decode_all(file.get(20..).context("a short TARB file")?)?;
    let word = |at: usize| -> Result<usize> {
        let b = data.get(at..at + 4).context("a short height file")?;
        Ok(u32::from_le_bytes(b.try_into()?) as usize)
    };
    let (start, len) = (0x2c + word(0x2c)?, word(0x30)?);
    let blob = data
        .get(start..start + len)
        .context("heights past the file")?;
    let gobs_x = (SAMPLES * 4).div_ceil(64);
    let mut out = Vec::with_capacity(SAMPLES * SAMPLES);
    for row in 0..SAMPLES {
        for col in 0..SAMPLES {
            let at = swizzled(col * 4, row, gobs_x);
            let b = blob.get(at..at + 4).context("a sample past the heights")?;
            out.push(f32::from_le_bytes(b.try_into()?));
        }
    }
    Ok(out)
}

/// A terrain's size and top height, from its `.tscb` header.
fn header(tscb: &[u8]) -> Result<(f32, f32)> {
    if !tscb.starts_with(b"TSCB") {
        bail!("not a TSCB file");
    }
    let f = |at: usize| -> Result<f32> {
        let b = tscb.get(at..at + 4).context("a short TSCB file")?;
        Ok(f32::from_le_bytes(b.try_into()?))
    };
    Ok((f(0x10)?, f(0x14)?))
}

/// The triangles of `tiles`, in the terrain's own space (centered on its
/// actor, y up from it), where no finer tile covers them.
fn triangles(tiles: &[Tile], size: f32, top: f32) -> Vec<Vec<[[f32; 3]; 3]>> {
    let have: HashSet<(u32, u32, u32)> = tiles.iter().map(|t| (t.level, t.x, t.z)).collect();
    let deepest = tiles.iter().map(|t| t.level).max().unwrap_or(0);
    let mut chunks = Vec::new();
    for tile in tiles {
        let across = (1u32 << tile.level) as f32;
        let cell = size / across / (SAMPLES - 1) as f32;
        let mut step = 1;
        while (step * 2) as f32 * cell <= SPACING * 1.01 {
            step *= 2;
        }
        // A cell is covered when a finer tile holds its middle.
        let covered = |col: usize, row: usize| {
            let mid =
                |i: usize, t: u32| (t as f32 + (i as f32 + step as f32 / 2.0) / 256.0) / across;
            let (u, w) = (mid(col, tile.x), mid(row, tile.z));
            (tile.level + 1..=deepest).any(|l| {
                let n = (1u32 << l) as f32;
                have.contains(&(l, (u * n) as u32, (w * n) as u32))
            })
        };
        let corner = |col: usize, row: usize| {
            let h = tile.at(col, row);
            let u = (tile.x as f32 + col as f32 / 256.0) / across;
            let w = (tile.z as f32 + row as f32 / 256.0) / across;
            (h != 0.0).then_some([(u - 0.5) * size, h * top, (w - 0.5) * size])
        };
        let side = CHUNK * step;
        for row0 in (0..SAMPLES - 1).step_by(side) {
            for col0 in (0..SAMPLES - 1).step_by(side) {
                let mut chunk = Vec::new();
                for row in (row0..(row0 + side).min(SAMPLES - 1)).step_by(step) {
                    for col in (col0..(col0 + side).min(SAMPLES - 1)).step_by(step) {
                        if covered(col, row) {
                            continue;
                        }
                        let (c1, r1) = ((col + step).min(256), (row + step).min(256));
                        let corners = [corner(col, row), corner(col, r1), corner(c1, row)];
                        let far = corner(c1, r1);
                        // Wound so the up side is the one Link stands on.
                        if let ([Some(p), Some(q), Some(r)], Some(s)) = (corners, far) {
                            chunk.push([p, q, r]);
                            chunk.push([r, q, s]);
                        }
                    }
                }
                if !chunk.is_empty() {
                    chunks.push(chunk);
                }
            }
        }
    }
    chunks
}

/// The terrain placed in a level, as walls, from one walk of the dump.
pub fn walls(romfs: &Romfs, game: &Path, placed: &[Placement]) -> Result<Vec<Wall>> {
    let names: HashSet<&str> = placed.iter().map(|p| p.name.as_str()).collect();
    fn owner<'a>(names: &HashSet<&str>, path: &'a str) -> Option<(&'a str, &'a str)> {
        let (name, file) = path.strip_prefix(DIR)?.split_once('/')?;
        (names.contains(name) && tile_key(file).is_some()).then_some((name, file))
    }
    let mut tiles: BTreeMap<String, Vec<Tile>> = BTreeMap::new();
    crate::game::each_where(
        game,
        |p| owner(&names, p).is_some(),
        |path, data| {
            let (name, file) = owner(&names, &path).context("a kept path has an owner")?;
            let (level, index) = tile_key(file).context("a kept path is a tile")?;
            let (x, z) = morton(index);
            let heights = heights(&data).with_context(|| path.clone())?;
            tiles.entry(name.to_owned()).or_default().push(Tile {
                level,
                x,
                z,
                heights,
            });
            Ok(())
        },
    )?;
    let mut shapes: HashMap<&str, Vec<Vec<[[f32; 3]; 3]>>> = HashMap::new();
    for (name, tiles) in &tiles {
        let pack = ActorPack::load(romfs, name)?;
        let tscb = (pack.raw(&format!("{DIR}{name}.tscb")))
            .with_context(|| format!("{name} has heights but no .tscb"))?;
        let (size, top) = header(tscb).with_context(|| name.clone())?;
        shapes.insert(name, triangles(tiles, size, top));
    }
    let mut walls = Vec::new();
    for p in placed {
        for chunk in shapes.get(p.name.as_str()).into_iter().flatten() {
            let posed = chunk.iter().map(|t| t.map(|v| place(v, &p.pose))).collect();
            walls.push(Wall::new(&format!("{}{SUFFIX}", p.name), posed));
        }
    }
    Ok(walls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_height_file_name_gives_its_level_and_place() {
        assert_eq!(tile_key("1000000000.hght"), Some((0, 0)));
        assert_eq!(tile_key("020000000C.hght"), Some((2, 12)));
        assert_eq!(tile_key("2100000003.hght"), None);
        assert_eq!(tile_key("0100000001.mate"), None);
        // Index 12 at level 2 is x 2, z 2: x takes the low bit.
        assert_eq!(morton(12), (2, 2));
        assert_eq!(morton(1), (1, 0));
        assert_eq!(morton(2), (0, 1));
    }

    #[test]
    fn a_finer_tile_replaces_the_coarse_one_under_it() {
        let flat = |level, x, z, h| Tile {
            level,
            x,
            z,
            heights: vec![h; SAMPLES * SAMPLES],
        };
        let size = (SAMPLES - 1) as f32 * SPACING;
        let root = triangles(&[flat(0, 0, 0, 0.5)], size, 10.0);
        let both = triangles(&[flat(0, 0, 0, 0.5), flat(1, 1, 0, 0.25)], size, 10.0);
        let count = |c: &Vec<Vec<[[f32; 3]; 3]>>| c.iter().map(Vec::len).sum::<usize>();
        assert_eq!(count(&root), 256 * 256 * 2);
        // The finer tile's quarter is read every other sample: as many
        // triangles, at its own height.
        assert_eq!(count(&both), count(&root));
        let at = |y: f32| both.iter().flatten().filter(|t| t[0][1] == y).count();
        assert_eq!(at(2.5), 128 * 128 * 2);
        assert!(both.iter().flatten().all(super::super::mesh::facing_up));
    }

    /// The Deku Tree's terrain lies under its navmesh: most of the mesh's
    /// corners over the root tile stand on it.
    #[test]
    fn deku_terrain_lies_under_its_navmesh() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new(crate::game::DEV_DUMP);
        let deku = crate::known::map::INSIDE_THE_DEKU_TREE.get();
        let placed = crate::warp::placements_in(&romfs, deku).unwrap();
        let walls = walls(&romfs, game, &placed).unwrap();
        let names: HashSet<&str> = walls.iter().map(|w| w.name.as_str()).collect();
        assert!(names.len() >= 5, "terrain of {names:?}");
        let mesh = super::super::load(&romfs, game, deku).unwrap();
        let (mut over, mut on) = (0, 0);
        for v in &mesh.vertices {
            let floors: Vec<f32> = walls.iter().flat_map(|w| w.floors_at(v[0], v[2])).collect();
            if !floors.is_empty() {
                over += 1;
                on += usize::from(floors.iter().any(|h| (h - v[1]).abs() < 0.3));
            }
        }
        assert!(
            over > 1000 && on * 3 > over,
            "{on} of {over} corners on terrain"
        );
    }
}
