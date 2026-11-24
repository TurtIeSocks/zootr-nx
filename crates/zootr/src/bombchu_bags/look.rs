//! How a Bombchu Bag looks: its bomb bag's model painted the Bombchu's
//! blue, and the Small's bomb drawn as a Bombchu. The guard lets this file
//! spell the files out, since they're extracted before a dump's names can
//! be read.

use anyhow::{Context, Result};
use std::collections::BTreeMap;

use ounce_formats::{
    Lod, ModBuilder, Romfs, Vertex, bones, format, lods, pack, positions, redraw, rename,
    replace_mesh_like, top_level, unpack,
};

use super::BOMB_BAGS;
use crate::key_tints::paint::{Host, Look};
use crate::known::Name;
use crate::medallions::model_path;
use crate::medallions::paint::{self, Band, Hues};

/// The files [`LOOKS`] are made from, spelled out like
/// [`crate::key_tints::FILES`]: extracted before a dump's names are read.
pub const FILES: [&str; 31] = [
    "Model/Item_BombBag_Small.Item_BombBag_Small.bfres.zs",
    "Tex/Mt_Item_BombBag_Small_Alb.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_AO.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_Mtl.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_Nrm.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_Rgh.bntx.zs",
    "Model/Item_BombBag_Medium.Item_BombBag_Medium.bfres.zs",
    "Tex/Mt_Item_BombBag_Medium_Alb.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_AO.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_Mtl.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_Nrm.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_Rgh.bntx.zs",
    "Model/Item_BombBag_Large.Item_BombBag_Large.bfres.zs",
    "Tex/Mt_Item_BombBag_Large_Alb.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_AO.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_Mtl.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_Nrm.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_Rgh.bntx.zs",
    "Model/Item_Bombchu.Item_Bombchu.bfres.zs",
    "Tex/Mt_Obj_Bomb_AO.bntx.zs",
    "Tex/Mt_Obj_Bomb_Alb.bntx.zs",
    "Tex/Mt_Obj_Bomb_Emm.bntx.zs",
    "Tex/Mt_Obj_Bomb_Mtl.bntx.zs",
    "Tex/Mt_Obj_Bomb_Nrm.bntx.zs",
    "Tex/Mt_Obj_Bomb_Rgh.bntx.zs",
    "Tex/Mt_Item_Bombchu_AO.bntx.zs",
    "Tex/Mt_Item_Bombchu_Alb.bntx.zs",
    "Tex/Mt_Item_Bombchu_Emm.bntx.zs",
    "Tex/Mt_Item_Bombchu_Mtl.bntx.zs",
    "Tex/Mt_Item_Bombchu_Nrm.bntx.zs",
    "Tex/Mt_Item_Bombchu_Rgh.bntx.zs",
];

/// Each bag's look: its bomb bag's model and textures, the albedo in the
/// Bombchu's colours ([`colours`]). A copy's name is as long as its host's (`paint::Look`), so the
/// Medium's prefix is a letter longer.
static LOOKS: [Look; 3] = [
    look(BOMB_BAGS[0], "Item_RandoChuBag"),
    look(BOMB_BAGS[1], "Item_Rando_ChuBag"),
    look(BOMB_BAGS[2], "Item_RandoChuBag"),
];

/// The bag's leather and its blue patch, as hues, each worn in a part of
/// the Bombchu's colours: the leather in its body's navy, the patch in its
/// belly's gold. The red stripes, the chain and the trim keep theirs, as a
/// Bombchu has red and metal too (the maintainer's call, 2026-11-21:
/// "whichever looks most like a bombchu", over all-over blue).
const LEATHER: Hues = (12.0 / 360.0, 40.0 / 360.0);
const PATCH: Hues = (200.0 / 360.0, 290.0 / 360.0);
/// The Bombchu albedo's navy and gold, as hues.
const NAVY: Hues = (200.0 / 360.0, 270.0 / 360.0);
const GOLD: Hues = (35.0 / 360.0, 70.0 / 360.0);

/// The Medium and Large bags' red stripes, as hues either side of 0. The
/// Small bomb bag has plain straps where they run, its albedo laid out as
/// the Medium's, so the Small Bombchu Bag takes the Medium's stripes (the
/// maintainer's call, 2026-11-21).
const RED: [Hues; 2] = [(0.0, 11.9 / 360.0), (345.0 / 360.0, 1.0)];

/// Bag `at`'s albedo paint: on the Small, the Medium's red stripes, then
/// on every bag the Bombchu's colours.
fn albedo(romfs: &Romfs, at: usize) -> Result<impl Fn(&mut [u8])> {
    let bands = colours(romfs)?;
    let stripes = match at {
        0 => {
            let alb = |at: usize| format!("Tex/Mt_{}_Alb.bntx.zs", LOOKS[at].host);
            let small = top_level(&romfs.read(&alb(0))?)?;
            Some(resample(&romfs.read(&alb(1))?, small.width, small.height)?)
        }
        _ => None,
    };
    Ok(move |rgba: &mut [u8]| {
        for (from, hues) in stripes.iter().flat_map(|s| RED.map(|h| (s, h))) {
            paint::copy(rgba, from, hues);
        }
        paint::shift(rgba, &bands);
    })
}

/// Each bag band and the colour it moves to, read from the Bombchu's
/// albedo.
fn colours(romfs: &Romfs) -> Result<[Band; 2]> {
    let chu = top_level(&romfs.read(&textures(CHU_TEXTURES, "Alb"))?)?;
    let like = |hues| paint::mean(&chu.rgba, hues).context("the Bombchu lacks a colour");
    Ok([
        Band {
            hues: LEATHER,
            to: like(NAVY)?,
        },
        Band {
            hues: PATCH,
            to: like(GOLD)?,
        },
    ])
}

const fn look(host: &'static Name, copy: &'static str) -> Look {
    Look {
        host,
        material: None,
        kinds: &["Alb", "AO", "Mtl", "Nrm", "Rgh"],
        others: &[],
        copy,
        matte: false,
    }
}

/// Bag `at`'s model, counting from 0.
pub(super) fn model(at: usize) -> String {
    LOOKS[at].name(at + 1)
}

/// Writes bag `at`'s painted model. Returns its name. The Small's bomb is
/// a Bombchu (the maintainer's call, 2026-11-20): a bag of Bombchus
/// showing a bomb read as a bomb bag.
pub(super) fn paint(romfs: &Romfs, builder: &mut ModBuilder, at: usize) -> Result<String> {
    let host = Host::read(romfs, &LOOKS[at])?;
    let albedo = albedo(romfs, at)?;
    if at != 0 {
        return host.paint_with(builder, at + 1, &albedo);
    }
    let name = model(at);
    host.write_with(builder, &name, &chus(romfs, &host.model)?, &albedo, false)?;
    for kind in KINDS {
        let like = textures(BOMB, kind);
        let bomb = romfs.read(&like)?;
        let size = top_level(&bomb)?;
        let chu = romfs.read(&textures(CHU_TEXTURES, kind))?;
        let rgba = resample(&chu, size.width, size.height)?;
        let copy = redraw(&rename(&bomb, BOMB, OURS)?, &rgba, |_, _| false)?;
        builder.add(&textures(OURS, kind), &copy, &like)?;
    }
    Ok(name)
}

/// The Small bag's bomb and its cap, each drawn as a part of the Bombchu.
const SWAPS: [(&str, &str); 2] = [
    ("Bomb_2__Mt_Obj_Bomb", "Body__Mt_Item_Bombchu"),
    ("Metal_1_1__Mt_Obj_Bomb_Metal", "Beard__Mt_Item_Bombchu_Mtl"),
];
const CHU: &str = "Item_Bombchu";
/// The bomb's textures' prefix, and the copies' as long, so the bag's
/// model names them in the same bytes. The copies are the bomb's files
/// redrawn with the Bombchu's texels: its own are named longer.
const BOMB: &str = "Mt_Obj_Bomb";
const OURS: &str = "Mt_RandoChu";
const CHU_TEXTURES: &str = "Mt_Item_Bombchu";
const KINDS: [&str; 6] = ["AO", "Alb", "Emm", "Mtl", "Nrm", "Rgh"];
/// How far the Bombchu's nose is raised out of the bag, in radians.
const RAISE: f32 = 50.0 * std::f32::consts::PI / 180.0;

fn textures(prefix: &str, kind: &str) -> String {
    format!("Tex/{prefix}_{kind}.bntx.zs")
}

/// The Small bag's model `bag` with each bomb a Bombchu on its bone, nose
/// raised and fitted to where the bomb and its fuse stood, drawn from
/// textures named [`OURS`]. The bombs share one shape and one cap shape,
/// each bomb's vertices on a bone of its own.
fn chus(romfs: &Romfs, bag: &[u8]) -> Result<Vec<u8>> {
    let chu = romfs.read(&model_path(CHU))?;
    let parts = (SWAPS.iter())
        .map(|(_, part)| lods(&chu, part))
        .collect::<Result<Vec<_>>>()?;
    let raised: Vec<[f32; 3]> = (parts.iter())
        .flat_map(|p| p[0].vertices.iter().map(|v| raise(v.position)))
        .collect();
    let raised = bounds(&raised);
    let mut bombs: BTreeMap<u32, Bomb> = BTreeMap::new();
    for (s, (shape, _)) in SWAPS.iter().enumerate() {
        let each = bones(bag, shape)?.into_iter().zip(positions(bag, shape)?);
        for (k, (bone, p)) in each.enumerate() {
            let (points, first) = bombs.entry(bone).or_default();
            points.push(p);
            first[s].get_or_insert(k);
        }
    }
    let mut model = bag.to_vec();
    for (s, ((shape, _), part)) in SWAPS.iter().zip(&parts).enumerate() {
        let n = lods(&model, shape)?.len();
        let mut levels = Vec::new();
        let mut like = Vec::new();
        for i in 0..n {
            let mut level = Lod::default();
            let mut from = Vec::new();
            for (points, first) in bombs.values() {
                let Some(k) = first[s] else { continue };
                let one = Fit::new(&raised, &bounds(points)).lod(&part[i * part.len() / n]);
                let base = level.vertices.len() as u16;
                level.indices.extend(one.indices.iter().map(|x| x + base));
                from.extend(std::iter::repeat_n(k, one.vertices.len()));
                level.vertices.extend(one.vertices);
            }
            levels.push(level);
            like.push(from);
        }
        model = replace_mesh_like(&model, shape, &levels, &like)?;
    }
    for kind in KINDS {
        model = rename(&model, &format!("{BOMB}_{kind}"), &format!("{OURS}_{kind}"))?;
    }
    Ok(model)
}

/// One bomb: its points through both shapes, and its first vertex in each.
type Bomb = (Vec<[f32; 3]>, [Option<usize>; 2]);

fn raise(p: [f32; 3]) -> [f32; 3] {
    let (sin, cos) = RAISE.sin_cos();
    [p[0], p[1] * cos + p[2] * sin, p[2] * cos - p[1] * sin]
}

/// The lowest and highest corner of `points`.
fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// Raised Bombchu space to the bomb's: as big as fits where the bomb
/// stood, standing on its bottom, centred on it across.
struct Fit {
    scale: f32,
    offset: [f32; 3],
}

impl Fit {
    fn new((lo, hi): &([f32; 3], [f32; 3]), (to_lo, to_hi): &([f32; 3], [f32; 3])) -> Self {
        let scale = (0..3)
            .map(|i| (to_hi[i] - to_lo[i]) / (hi[i] - lo[i]))
            .fold(f32::MAX, f32::min);
        let mid = |l: f32, h: f32| (l + h) / 2.0;
        let offset = [
            mid(to_lo[0], to_hi[0]) - mid(lo[0], hi[0]) * scale,
            to_lo[1] - lo[1] * scale,
            mid(to_lo[2], to_hi[2]) - mid(lo[2], hi[2]) * scale,
        ];
        Self { scale, offset }
    }

    fn lod(&self, lod: &Lod) -> Lod {
        let turn = |packed: u32| {
            let [x, y, z, w] = unpack(packed);
            let [x, y, z] = raise([x, y, z]);
            pack([x, y, z, w])
        };
        let vertices = (lod.vertices.iter())
            .map(|v| {
                let p = raise(v.position);
                Vertex {
                    position: std::array::from_fn(|i| p[i] * self.scale + self.offset[i]),
                    normal: turn(v.normal),
                    tangent: turn(v.tangent),
                    ..*v
                }
            })
            .collect();
        Lod {
            vertices,
            indices: lod.indices.clone(),
        }
    }
}

/// BNTX's format id for BC4, a red-only block format. A texture's format
/// holds it above a low byte giving its type.
const BC4: u32 = 0x1d;

/// Texture file `data`'s first level, resampled to `w` by `h` (nearest),
/// grey where it holds only red (BC4).
fn resample(data: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    let top = top_level(data)?;
    let red_only = format(data)? >> 8 == BC4;
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x * top.width / w, y * top.height / h);
            let at = 4 * (sy * top.width + sx);
            let mut px: [u8; 4] = top.rgba[at..at + 4].try_into()?;
            if red_only {
                px = [px[0], px[0], px[0], 255];
            }
            out.extend_from_slice(&px);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_extracted_files_are_the_looks() {
        needs_names!();
        let mut files: Vec<_> = LOOKS.iter().flat_map(Look::files).collect();
        files.push(model_path(CHU));
        for prefix in [BOMB, CHU_TEXTURES] {
            files.extend(KINDS.map(|k| textures(prefix, k)));
        }
        assert_eq!(files, FILES);
    }

    /// The Small's bomb and cap draw the Bombchu's body and beard, finest
    /// level alike, inside where the bomb stood.
    #[test]
    fn the_small_bags_bomb_is_a_bombchu() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let bag = romfs.read(&model_path(LOOKS[0].host)).unwrap();
        let chu = romfs.read(&model_path(CHU)).unwrap();
        let ours = chus(&romfs, &bag).unwrap();
        let (lo, hi) = bounds(&SWAPS.map(|(s, _)| positions(&bag, s).unwrap()).concat());
        for (shape, part) in SWAPS {
            let drawn = lods(&ours, shape).unwrap();
            assert_eq!(drawn.len(), lods(&bag, shape).unwrap().len());
            let bombs: BTreeSet<u32> = bones(&bag, shape).unwrap().into_iter().collect();
            let one = lods(&chu, part).unwrap()[0].indices.len();
            assert_eq!(drawn[0].indices.len(), one * bombs.len());
            let drawn_bones: BTreeSet<u32> = bones(&ours, shape).unwrap().into_iter().collect();
            assert_eq!(drawn_bones, bombs);
            for p in positions(&ours, shape).unwrap() {
                assert!(
                    (0..3).all(|i| p[i] >= lo[i] - 0.01 && p[i] <= hi[i] + 0.01),
                    "{p:?}"
                );
            }
        }
        assert!(ours.windows(OURS.len()).any(|w| w == OURS.as_bytes()));
    }

    /// Each bag's leather wears the Bombchu's navy, none of it left tan,
    /// and each has the red stripes.
    #[test]
    fn each_bag_wears_the_bombchus_colours() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let chu = top_level(&romfs.read(&textures(CHU_TEXTURES, "Alb")).unwrap()).unwrap();
        let navy = paint::mean(&chu.rgba, NAVY).unwrap();
        let alb = |at: usize| format!("Tex/Mt_{}_Alb.bntx.zs", LOOKS[at].host);
        let red = |px: &[u8]| RED.iter().any(|&h| paint::mean(px, h).is_some());
        let medium = top_level(&romfs.read(&alb(1)).unwrap()).unwrap().rgba;
        for (at, look) in LOOKS.iter().enumerate() {
            let path = format!("Tex/Mt_{}_Alb.bntx.zs", look.host);
            let before = top_level(&romfs.read(&path).unwrap()).unwrap().rgba;
            let mut rgba = before.clone();
            albedo(&romfs, at).unwrap()(&mut rgba);
            // The Small's plain straps take the Medium's red stripes.
            let stripes = if at == 0 { &medium } else { &before };
            let lost = (stripes.chunks(4).zip(rgba.chunks(4)))
                .filter(|(stripe, now)| red(stripe) && !red(now))
                .count();
            assert_eq!(lost, 0, "{}", look.host);
            let leather = |px: &[u8]| paint::mean(px, LEATHER).is_some();
            let moved = (before.chunks(4).zip(rgba.chunks(4))).filter(|(was, _)| leather(was));
            let (n, still) = moved.fold((0, 0), |(n, s), (_, now)| {
                (n + 1, s + leather(now) as usize)
            });
            assert!(
                n > 0 && still == 0,
                "{}: {still} of {n} still tan",
                look.host
            );
            let (h, _, _) = paint::mean(&rgba, NAVY).unwrap();
            assert!((h - navy.0).abs() < 0.02, "{}: {h}", look.host);
        }
    }

    /// A painted copy keeps every offset of its host's model.
    #[test]
    fn each_model_is_named_as_long_as_its_bomb_bags() {
        needs_names!();
        for (at, look) in LOOKS.iter().enumerate() {
            assert_eq!(model(at).len(), look.host.get().len(), "{}", model(at));
        }
    }
}
