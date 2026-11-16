//! A key ring's model: the small key repeated in a fan around a metal hoop
//! threaded through their bows, built from the key's own model. Each level
//! of detail is the key's at that level, so many times over, and a hoop as
//! fine. The hoop is drawn from a corner of the key's textures no key uses,
//! painted plain metal, so it stays untinted while the keys take the
//! dungeon's colour.

use std::f32::consts::{PI, TAU};

use anyhow::{Result, bail};
#[cfg(test)]
use ounce_formats::pointers;
use ounce_formats::{Lod, Texture, Vertex, lods, pack, replace_mesh, unpack};

use crate::known::{Name, field};

/// The key's one shape.
pub const SHAPE: &Name = field::DUNGEON_SMALL_KEY_MT_OBJ_DUNGEON_SMALL_KEY;

/// Where the hoop runs through a key's bow, near its hole's top, in the
/// key's own space: the bow is at +y, the key's face is z.
const BOW: [f32; 3] = [0.0, 0.23, 0.0];
/// The hoop's radius, from its middle above the bows to them, and its
/// tube's.
const HOOP: f32 = 0.3;
const TUBE: f32 = 0.014;
/// The angle between two keys around the hoop, and how far the hoop's axis
/// leans from the key's face toward its width. The lean steps each key
/// a little behind the one before, so neighbours don't run into each other.
const SLOT: f32 = 12.0 * PI / 180.0;
const LEAN: f32 = 30.0 * PI / 180.0;

/// The textures' corner no key uses (u, v from the top left), painted
/// plain metal, and the part of it the hoop's UVs span, kept off its
/// edges so the mips' blur doesn't reach the keys' padding.
const PAINTED: ([f32; 2], [f32; 2]) = ([0.02, 0.58], [0.14, 0.92]);
const MAPPED: ([f32; 2], [f32; 2]) = ([0.05, 0.65], [0.11, 0.85]);

/// The model `model`, the key's, as a ring of `keys` keys, as tall as the
/// key and centred where it is, so the get window frames it alike.
pub fn model(model: &[u8], keys: usize) -> Result<Vec<u8>> {
    let key = lods(model, SHAPE)?;
    let ring = place(&key, keys);
    replace_mesh(model, SHAPE, &ring)
}

/// Each of `key`'s levels as a ring of `keys`, fitted to the key's height.
fn place(key: &[Lod], keys: usize) -> Vec<Lod> {
    let hoop = Hoop::new();
    let mut ring: Vec<Lod> = (key.iter().enumerate())
        .map(|(level, lod)| {
            let mut out = Lod::default();
            for angle in angles(keys) {
                append(&mut out, &hoop.turned(lod, angle));
            }
            append(&mut out, &hoop.lod(level));
            out
        })
        .collect();
    let (lo, hi) = bounds(&key[0]);
    let (ring_lo, ring_hi) = bounds(&ring[0]);
    let scale = (hi[1] - lo[1]) / (ring_hi[1] - ring_lo[1]);
    let from: [f32; 3] = std::array::from_fn(|k| (ring_lo[k] + ring_hi[k]) / 2.0);
    let to: [f32; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
    for v in ring.iter_mut().flat_map(|l| &mut l.vertices) {
        v.position = std::array::from_fn(|k| to[k] + scale * (v.position[k] - from[k]));
    }
    ring
}

/// Each of `keys` keys' turn about the hoop: [`SLOT`] apart, centred on
/// the bottom.
fn angles(keys: usize) -> Vec<f32> {
    (0..keys)
        .map(|k| SLOT * (k as f32 - (keys as f32 - 1.0) / 2.0))
        .collect()
}

/// `lod`'s vertices and triangles added to `out`'s.
fn append(out: &mut Lod, lod: &Lod) {
    let base = out.vertices.len() as u16;
    out.vertices.extend_from_slice(&lod.vertices);
    out.indices.extend(lod.indices.iter().map(|i| base + i));
}

fn bounds(lod: &Lod) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in &lod.vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(v.position[k]);
            hi[k] = hi[k].max(v.position[k]);
        }
    }
    (lo, hi)
}

/// The hoop: its middle, its axis, and two directions across it, `across`
/// to the bows (down) and `along` the way they run.
struct Hoop {
    middle: [f32; 3],
    axis: [f32; 3],
    across: [f32; 3],
    along: [f32; 3],
}

impl Hoop {
    fn new() -> Self {
        let axis = [LEAN.sin(), 0.0, LEAN.cos()];
        let across = [0.0, -1.0, 0.0];
        Self {
            middle: [BOW[0], BOW[1] + HOOP, BOW[2]],
            axis,
            across,
            along: cross(axis, across),
        }
    }

    /// `lod` turned `angle` about the hoop's axis, through its middle.
    fn turned(&self, lod: &Lod, angle: f32) -> Lod {
        let turn = |p: [f32; 3]| rotate(p, self.axis, angle);
        let turn_packed = |packed: u32| {
            let [x, y, z, w] = unpack(packed);
            let [x, y, z] = turn([x, y, z]);
            pack([x, y, z, w])
        };
        let vertices = (lod.vertices.iter())
            .map(|v| Vertex {
                position: add(self.middle, turn(sub(v.position, self.middle))),
                normal: turn_packed(v.normal),
                tangent: turn_packed(v.tangent),
                ..*v
            })
            .collect();
        Lod {
            vertices,
            indices: lod.indices.clone(),
        }
    }

    /// The hoop at level of detail `level`: a torus, coarser each level, as
    /// the key's levels are. Its UVs span [`MAPPED`].
    fn lod(&self, level: usize) -> Lod {
        let (around, round) = (48 - 4 * level, 8 - level / 2);
        let mut lod = Lod::default();
        for i in 0..around {
            let a = TAU * i as f32 / around as f32;
            let out = add(scale(self.across, a.cos()), scale(self.along, a.sin()));
            let tangent = sub(scale(self.along, a.cos()), scale(self.across, a.sin()));
            for j in 0..round {
                let b = TAU * j as f32 / round as f32;
                let normal = add(scale(out, b.cos()), scale(self.axis, b.sin()));
                let position = add(self.middle, add(scale(out, HOOP), scale(normal, TUBE)));
                let (lo, hi) = MAPPED;
                let uv = [
                    lo[0] + (hi[0] - lo[0]) * j as f32 / round as f32,
                    lo[1] + (hi[1] - lo[1]) * i as f32 / around as f32,
                ];
                lod.vertices.push(Vertex {
                    position,
                    uv,
                    normal: pack([normal[0], normal[1], normal[2], 1.0]),
                    tangent: pack([tangent[0], tangent[1], tangent[2], 1.0]),
                });
            }
        }
        let at = |i: usize, j: usize| ((i % around) * round + j % round) as u16;
        for i in 0..around {
            for j in 0..round {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                lod.indices.extend([a, b, c, a, c, d]);
            }
        }
        lod
    }
}

/// `t`, a first level of the key's texture `kind`, with [`PAINTED`] plain
/// metal: an untinted grey, shiny, flat and unshaded.
pub fn paint(t: &mut Texture, kind: &str) -> Result<()> {
    let value: [u8; 4] = match kind {
        "Alb" => [168, 165, 158, 255],
        "Mtl" => [230, 0, 0, 255],
        "Rgh" => [70, 0, 0, 255],
        "AO" => [255, 255, 255, 255],
        "Nrm" => [128, 128, 255, 255],
        _ => bail!("no plain metal for {kind}"),
    };
    let (w, h) = (t.width, t.height);
    for y in 0..h {
        for x in 0..w {
            if painted(w, h, x, y) {
                let at = 4 * (y * w + x);
                t.rgba[at..at + 4].copy_from_slice(&value);
            }
        }
    }
    Ok(())
}

/// Whether texel `x`, `y` of a `w` by `h` texture is in [`PAINTED`].
pub fn painted(w: usize, h: usize, x: usize, y: usize) -> bool {
    let (lo, hi) = PAINTED;
    let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
    (lo[0]..hi[0]).contains(&u) && (lo[1]..hi[1]).contains(&v)
}

fn rotate(p: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
    // Rodrigues: p cos + (axis × p) sin + axis (axis · p)(1 - cos).
    let (s, c) = angle.sin_cos();
    let along = scale(axis, dot(axis, p) * (1.0 - c));
    add(add(scale(p, c), scale(cross(axis, p), s)), along)
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| a[k] + b[k])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| a[k] - b[k])
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|x| x * s)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| a[k] * b[k]).sum()
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::actor;
    use crate::medallions::model_path;

    fn key() -> Option<Vec<u8>> {
        let romfs = dev_dump()?;
        Some(
            romfs
                .read(&model_path(actor::OBJ_DUNGEON_SMALL_KEY))
                .unwrap(),
        )
    }

    /// The facing of each triangle, by its winding, against its first
    /// vertex's normal: how many face out.
    fn facing_out(lod: &Lod) -> f32 {
        let tris = lod.indices.chunks(3);
        let out = tris
            .clone()
            .filter(|t| {
                let p = |i: u16| lod.vertices[i as usize].position;
                let n = unpack(lod.vertices[t[0] as usize].normal);
                let face = cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])));
                dot(face, [n[0], n[1], n[2]]) > 0.0
            })
            .count();
        out as f32 / tris.count() as f32
    }

    #[test]
    fn the_hoop_is_closed_and_faces_out_as_the_key_does() {
        let hoop = Hoop::new();
        for level in 0..9 {
            let lod = hoop.lod(level);
            let (around, round) = (48 - 4 * level, 8 - level / 2);
            assert_eq!(lod.vertices.len(), around * round);
            assert_eq!(lod.indices.len(), 6 * around * round);
            assert!(
                lod.indices
                    .iter()
                    .all(|&i| (i as usize) < lod.vertices.len())
            );
            for v in &lod.vertices {
                let n = unpack(v.normal);
                let len = dot([n[0], n[1], n[2]], [n[0], n[1], n[2]]).sqrt();
                assert!((len - 1.0).abs() < 0.01, "{len}");
                let (lo, hi) = MAPPED;
                assert!((lo[0]..=hi[0]).contains(&v.uv[0]) && (lo[1]..=hi[1]).contains(&v.uv[1]));
            }
            assert_eq!(facing_out(&lod), 1.0, "level {level}");
        }
        let Some(model) = key() else { return };
        let key = lods(&model, SHAPE).unwrap();
        assert!(facing_out(&key[0]) > 0.9);
    }

    #[test]
    fn the_hoop_runs_through_every_bow() {
        let hoop = Hoop::new();
        let bow = Lod {
            vertices: vec![Vertex {
                position: BOW,
                uv: [0.0; 2],
                normal: 0,
                tangent: 0,
            }],
            indices: vec![],
        };
        for k in -4..=4 {
            let p = hoop.turned(&bow, SLOT * k as f32).vertices[0].position;
            let from = sub(p, hoop.middle);
            assert!((dot(from, from).sqrt() - HOOP).abs() < 1e-5);
            assert!(dot(from, hoop.axis).abs() < 1e-5);
        }
    }

    #[test]
    fn the_fan_is_symmetric_about_the_bottom() {
        for keys in 1..=9 {
            let a = angles(keys);
            assert_eq!(a.len(), keys);
            for (x, y) in a.iter().zip(a.iter().rev()) {
                assert!((x + y).abs() < 1e-6, "{keys}: {a:?}");
            }
            assert!(a.windows(2).all(|w| (w[1] - w[0] - SLOT).abs() < 1e-6));
        }
    }

    #[test]
    fn the_ring_is_as_tall_as_the_key_and_centred_where_it_is() {
        let Some(model) = key() else { return };
        let key = lods(&model, SHAPE).unwrap();
        let ring = place(&key[..1], 4);
        let ((lo, hi), (key_lo, key_hi)) = (bounds(&ring[0]), bounds(&key[0]));
        assert!(((hi[1] - lo[1]) - (key_hi[1] - key_lo[1])).abs() < 1e-4);
        for k in 0..3 {
            assert!(((lo[k] + hi[k]) - (key_lo[k] + key_hi[k])).abs() < 1e-4);
        }
    }

    fn u32_at(d: &[u8], at: usize) -> usize {
        u32::from_le_bytes(d[at..at + 4].try_into().unwrap()) as usize
    }

    /// The end of a model's memory pool, and where its relocation table is.
    fn pool_end_and_table(d: &[u8]) -> (usize, usize) {
        let info = u32_at(d, 0xb0);
        (u32_at(d, info + 8) + u32_at(d, info + 4), u32_at(d, 0x18))
    }

    #[test]
    fn the_key_s_own_mesh_appended_reads_back_and_moves_every_pointer_past_it() {
        let Some(model) = key() else { return };
        let key = lods(&model, SHAPE).unwrap();
        let grown = replace_mesh(&model, SHAPE, &key).unwrap();
        assert_eq!(lods(&grown, SHAPE).unwrap(), key);
        let growth = grown.len() - model.len();
        assert!(growth > 0 && growth.is_multiple_of(0x1000), "{growth:#x}");
        assert_eq!(u32_at(&grown, 0x1c), grown.len());
        let (end, table) = pool_end_and_table(&model);
        assert_eq!(pool_end_and_table(&grown), (end + growth, table + growth));
        let shift = |o: usize| if o >= end { o + growth } else { o };
        let (before, after) = (pointers(&model).unwrap(), pointers(&grown).unwrap());
        assert!(
            before.iter().any(|p| p.1 >= end),
            "no pointer past the pool"
        );
        let moved: Vec<_> = before
            .iter()
            .map(|&(at, to)| (shift(at), shift(to)))
            .collect();
        assert_eq!(moved, after);
        assert_eq!(model[end..table], grown[end + growth..table + growth]);
    }

    #[test]
    fn a_ring_reads_back_with_its_keys_and_hoop_at_every_level() {
        let Some(model) = key() else { return };
        let key = lods(&model, SHAPE).unwrap();
        let hoop = Hoop::new();
        for keys in [1, 5, 9] {
            let ring = lods(&super::model(&model, keys).unwrap(), SHAPE).unwrap();
            assert_eq!(ring.len(), key.len());
            for (level, (r, k)) in ring.iter().zip(&key).enumerate() {
                let hoop = hoop.lod(level);
                assert_eq!(
                    r.vertices.len(),
                    keys * k.vertices.len() + hoop.vertices.len()
                );
                assert_eq!(r.indices.len(), keys * k.indices.len() + hoop.indices.len());
            }
        }
    }

    #[test]
    fn no_key_uv_lands_in_the_painted_corner() {
        let Some(model) = key() else { return };
        let size = 384;
        for lod in lods(&model, SHAPE).unwrap() {
            for t in lod.indices.chunks(3) {
                for &i in t {
                    let [u, v] = lod.vertices[i as usize].uv;
                    let (x, y) = ((u * size as f32) as usize, (v * size as f32) as usize);
                    assert!(!painted(size, size, x, y), "{u} {v}");
                }
            }
        }
    }
}
