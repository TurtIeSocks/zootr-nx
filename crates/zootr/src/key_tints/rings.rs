//! A key ring's model: the small key repeated in a fan around an iron hoop
//! threaded through their bows, with gold Triforces on the hoop's faces,
//! built from the key's own model. Each level of detail is the key's at
//! that level, so many times over, and a hoop as fine. The hoop and its
//! Triforces are drawn from a corner of the key's textures no key uses,
//! painted iron and gold, so they stay untinted while the keys take the
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
const TUBE: f32 = 0.03;
/// The angle between two keys around the hoop, and how far the hoop's axis
/// leans from the key's face toward its width. The lean steps each key
/// a little behind the one before, so neighbours don't run into each other.
const SLOT: f32 = 12.0 * PI / 180.0;
const LEAN: f32 = 30.0 * PI / 180.0;

/// The Triforces on each face of the hoop: how many, the angle between two
/// around its top, each one's height, and how far each stands above the
/// tube.
const TRIFORCES: usize = 5;
const TRIFORCE_STEP: f32 = 40.0 * PI / 180.0;
const TRIFORCE: f32 = 2.2 * TUBE;
const RAISED: f32 = 0.005;

/// A flat finish painted into the textures' corner no key uses: where
/// (u, v from the top left), and its albedo, metalness and roughness.
struct Finish {
    painted: ([f32; 2], [f32; 2]),
    albedo: [u8; 3],
    metal: u8,
    rough: u8,
}

/// The hoop's dark iron and the Triforces' gold, one above the other.
const IRON: Finish = Finish {
    painted: ([0.02, 0.58], [0.14, 0.75]),
    albedo: [92, 90, 88],
    metal: 220,
    rough: 100,
};
const GOLD: Finish = Finish {
    painted: ([0.02, 0.75], [0.14, 0.92]),
    albedo: [232, 184, 72],
    metal: 255,
    rough: 55,
};
const FINISHES: [&Finish; 2] = [&IRON, &GOLD];
/// How far UVs keep in from a finish's edges, so the mips' blur doesn't
/// reach past it.
const MARGIN: [f32; 2] = [0.03, 0.04];

impl Finish {
    /// The part of it UVs span.
    fn mapped(&self) -> ([f32; 2], [f32; 2]) {
        let (lo, hi) = self.painted;
        (
            [lo[0] + MARGIN[0], lo[1] + MARGIN[1]],
            [hi[0] - MARGIN[0], hi[1] - MARGIN[1]],
        )
    }

    fn middle(&self) -> [f32; 2] {
        let (lo, hi) = self.painted;
        [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
    }

    /// Its texel in a texture of `kind`, flat and unshaded.
    fn texel(&self, kind: &str) -> Result<[u8; 4]> {
        let [r, g, b] = self.albedo;
        Ok(match kind {
            "Alb" => [r, g, b, 255],
            "Mtl" => [self.metal, 0, 0, 255],
            "Rgh" => [self.rough, 0, 0, 255],
            "AO" => [255, 255, 255, 255],
            "Nrm" => [128, 128, 255, 255],
            _ => bail!("no finish for {kind}"),
        })
    }

    /// Whether texel `x`, `y` of a `w` by `h` texture is in it.
    fn holds(&self, w: usize, h: usize, x: usize, y: usize) -> bool {
        let (lo, hi) = self.painted;
        let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
        (lo[0]..hi[0]).contains(&u) && (lo[1]..hi[1]).contains(&v)
    }
}

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
            append(&mut out, &hoop.triforces());
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
    /// the key's levels are. Its UVs span [`IRON`]'s.
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
                let (lo, hi) = IRON.mapped();
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

    /// Where the hoop's `k`th Triforce sits: the tube's middle there, the
    /// way out from the hoop's middle, and the way around it.
    fn triforce_at(&self, k: usize) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let a = PI + TRIFORCE_STEP * (k as f32 - (TRIFORCES as f32 - 1.0) / 2.0);
        let out = add(scale(self.across, a.cos()), scale(self.along, a.sin()));
        let around = sub(scale(self.along, a.cos()), scale(self.across, a.sin()));
        (add(self.middle, scale(out, HOOP)), out, around)
    }

    /// The gold Triforces on both of the hoop's faces, over its top, each
    /// pointing out from its middle: three prisms standing from the hoop's
    /// middle plane to [`RAISED`] above the tube. Every level has them all.
    fn triforces(&self) -> Lod {
        let (height, uv) = (TRIFORCE, GOLD.middle());
        let (half, side) = (height / 2.0, height / 3f32.sqrt());
        // Each small triangle's corners, across and out from the big one's
        // centre.
        let small = |x: f32, y: f32| [[x, y + half], [x - side / 2.0, y], [x + side / 2.0, y]];
        let tops = [
            small(0.0, height / 6.0),
            small(-side / 2.0, -height / 3.0),
            small(side / 2.0, -height / 3.0),
        ];
        let mut lod = Lod::default();
        for k in 0..TRIFORCES {
            let (centre, out, around) = self.triforce_at(k);
            for face in [1.0, -1.0] {
                let rise = scale(self.axis, face * (TUBE + RAISED));
                for corners in &tops {
                    let base =
                        corners.map(|[x, y]| add(centre, add(scale(around, x), scale(out, y))));
                    prism(&mut lod, base, rise, uv);
                }
            }
        }
        lod
    }
}

/// A triangular prism added to `lod`: `base` lifted by `rise`, with its top
/// and walls but no bottom, every face out, all at texel `uv`.
fn prism(lod: &mut Lod, base: [[f32; 3]; 3], rise: [f32; 3], uv: [f32; 2]) {
    let up = normalized(rise);
    let mut base = base;
    if dot(cross(sub(base[1], base[0]), sub(base[2], base[0])), up) < 0.0 {
        base.swap(1, 2);
    }
    let top = base.map(|p| add(p, rise));
    let mut face = |corners: &[[f32; 3]], normal: [f32; 3], tangent: [f32; 3]| {
        let first = lod.vertices.len() as u16;
        for &position in corners {
            lod.vertices.push(Vertex {
                position,
                uv,
                normal: pack([normal[0], normal[1], normal[2], 1.0]),
                tangent: pack([tangent[0], tangent[1], tangent[2], 1.0]),
            });
        }
        for i in 1..corners.len() as u16 - 1 {
            lod.indices.extend([first, first + i, first + i + 1]);
        }
    };
    face(&top, up, normalized(sub(top[1], top[0])));
    for i in 0..3 {
        let j = (i + 1) % 3;
        let edge = sub(base[j], base[i]);
        let wall = [base[i], base[j], top[j], top[i]];
        face(&wall, normalized(cross(edge, up)), normalized(edge));
    }
}

/// `t`, a first level of the key's texture `kind`, with each [`Finish`]
/// painted in.
pub fn paint(t: &mut Texture, kind: &str) -> Result<()> {
    let (w, h) = (t.width, t.height);
    for finish in FINISHES {
        let value = finish.texel(kind)?;
        for y in 0..h {
            for x in 0..w {
                if finish.holds(w, h, x, y) {
                    let at = 4 * (y * w + x);
                    t.rgba[at..at + 4].copy_from_slice(&value);
                }
            }
        }
    }
    Ok(())
}

/// Whether texel `x`, `y` of a `w` by `h` texture is in a [`Finish`].
pub fn painted(w: usize, h: usize, x: usize, y: usize) -> bool {
    FINISHES.iter().any(|f| f.holds(w, h, x, y))
}

fn normalized(a: [f32; 3]) -> [f32; 3] {
    scale(a, 1.0 / dot(a, a).sqrt())
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
                let (lo, hi) = IRON.mapped();
                assert!((lo[0]..=hi[0]).contains(&v.uv[0]) && (lo[1]..=hi[1]).contains(&v.uv[1]));
            }
            assert_eq!(facing_out(&lod), 1.0, "level {level}");
        }
        let Some(model) = key() else { return };
        let key = lods(&model, SHAPE).unwrap();
        assert!(facing_out(&key[0]) > 0.9);
    }

    #[test]
    fn the_triforces_face_out_in_gold_on_both_faces_over_the_top_clear_of_the_keys() {
        let hoop = Hoop::new();
        let lod = hoop.triforces();
        // Five Triforces a face, three prisms each: a top and three walls.
        assert_eq!(lod.vertices.len(), TRIFORCES * 2 * 3 * (3 + 3 * 4));
        assert_eq!(facing_out(&lod), 1.0);
        assert!(lod.vertices.iter().all(|v| v.uv == GOLD.middle()));
        let (lo, hi) = GOLD.mapped();
        let [u, v] = GOLD.middle();
        assert!((lo[0]..=hi[0]).contains(&u) && (lo[1]..=hi[1]).contains(&v));
        // The widest fan's outer key, plus a slot, stays below the lowest.
        let fan = angles(9)[8] + SLOT;
        for k in 0..TRIFORCES {
            let (centre, out, _) = hoop.triforce_at(k);
            assert!(dot(out, hoop.across) < fan.cos(), "{k}");
            let from = sub(centre, hoop.middle);
            assert!((dot(from, from).sqrt() - HOOP).abs() < 1e-5);
        }
        let faces = |sign: f32| {
            let above = |v: &&Vertex| sign * dot(sub(v.position, hoop.middle), hoop.axis) > TUBE;
            lod.vertices.iter().filter(above).count()
        };
        assert_eq!(faces(1.0), faces(-1.0));
        assert!(faces(1.0) > 0);
    }

    #[test]
    fn the_corner_is_painted_iron_above_gold_and_nothing_past_it() {
        let size = 64;
        let mut t = Texture {
            name: String::new(),
            width: size,
            height: size,
            rgba: vec![0; 4 * size * size],
        };
        paint(&mut t, "Alb").unwrap();
        let at = |[u, v]: [f32; 2]| {
            let (x, y) = ((u * size as f32) as usize, (v * size as f32) as usize);
            t.rgba[4 * (y * size + x)..4 * (y * size + x) + 3].to_vec()
        };
        assert_eq!(at(IRON.middle()), IRON.albedo);
        assert_eq!(at(GOLD.middle()), GOLD.albedo);
        assert_eq!(at([0.5, 0.5]), [0, 0, 0]);
        assert!(paint(&mut t, "Fake").is_err());
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
                let gold = Hoop::new().triforces();
                assert_eq!(
                    r.vertices.len(),
                    keys * k.vertices.len() + hoop.vertices.len() + gold.vertices.len()
                );
                assert_eq!(
                    r.indices.len(),
                    keys * k.indices.len() + hoop.indices.len() + gold.indices.len()
                );
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
