//! The app's preview of a palette: one of its game textures, painted as
//! the seed paints it, small enough to redraw as the picker moves, or a
//! sphere lit with a material's colours where the palette writes params.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use ounce_formats::{Romfs, Texture, top_level};

use super::paint::{self, Hues};
use super::{Kind, Palette, Picked, jobs, oklch, paint_jobs, palette};

/// The longest side of a preview, in pixels.
pub const SIZE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// The texture as the seed writes it.
    Paint,
    /// The same, with every texel the palette leaves alone greyed, so a
    /// player sees what changes (the Hylian Shield's bird stays red).
    Changes,
}

/// One picture a palette previews on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    /// A texture it paints, by path.
    Texture(&'static str),
    /// A material whose params it writes, shown as a lit sphere.
    Material(&'static str),
}

/// What `palette` previews on, each once, in catalog order: its textures,
/// then its materials.
pub fn sheets(palette: &Palette) -> Vec<Sheet> {
    let mut out = Vec::new();
    for w in palette.writes {
        let sheet = match (w.texture(), &w.kind) {
            (Some(t), _) => Sheet::Texture(t.path),
            (None, Kind::Param { material, .. }) => Sheet::Material(material),
            // An effect previews on a canvas the app draws.
            (None, Kind::Vfx { .. }) => continue,
            (None, _) => unreachable!("a band or tint has a texture"),
        };
        if !out.contains(&sheet) {
            out.push(sheet);
        }
    }
    out.sort_by_key(|s| matches!(s, Sheet::Material(_)));
    out
}

/// The vanilla textures previews start from, each decoded once.
#[derive(Default)]
pub struct Previews(Mutex<HashMap<&'static str, Arc<Texture>>>);

impl Previews {
    fn vanilla(&self, romfs: &Romfs, path: &'static str) -> Result<Arc<Texture>> {
        if let Some(kept) = self.0.lock().unwrap().get(path) {
            return Ok(kept.clone());
        }
        let decoded = Arc::new(top_level(&romfs.read(path)?)?);
        self.0.lock().unwrap().insert(path, decoded.clone());
        Ok(decoded)
    }

    /// Why `palette` can't be painted on this dump, if it can't: a write
    /// whose part isn't the colour the catalog recorded (a newer dump).
    pub fn check(&self, romfs: &Romfs, palette: &Palette) -> Result<()> {
        for write in palette.writes {
            match (write.texture(), &write.kind) {
                (Some(t), _) => super::check(&self.vanilla(romfs, t.path)?.rgba, write)?,
                (None, Kind::Param { models, .. }) => {
                    for model in *models {
                        super::check_param(&romfs.read(model)?, write)?;
                    }
                }
                (None, Kind::Vfx { bundle, .. }) => {
                    super::check_vfx(&romfs.read(bundle)?, write)?;
                }
                (None, _) => unreachable!("a band or tint has a texture"),
            }
        }
        Ok(())
    }

    /// `id`'s sheet number `sheet` as a PNG, at most `SIZE` a side:
    /// painted with `picked`, or vanilla without one.
    pub fn png(
        &self,
        romfs: &Romfs,
        id: &str,
        sheet: usize,
        picked: Option<&Picked>,
        view: View,
    ) -> Result<Vec<u8>> {
        let palette = palette(id)?;
        let path = match sheets(palette).get(sheet) {
            Some(Sheet::Texture(path)) => *path,
            Some(Sheet::Material(material)) => {
                return crate::sprites::png(&sphere(&material_colours(palette, material, picked)?));
            }
            None => anyhow::bail!("{id} has no sheet {sheet}"),
        };
        let vanilla = self.vanilla(romfs, path)?;
        let mut rgba = vanilla.rgba.clone();
        if let Some(picked) = picked {
            let here = jobs(palette, picked)?.into_iter();
            let here: Vec<_> = here
                .filter(|j| j.write.texture().is_some_and(|t| t.path == path))
                .collect();
            paint_jobs(&mut rgba, &here)?;
        }
        if vanilla.height <= GRADIENT {
            // A gradient (the hair's) reads better on a sphere than as a strip.
            let row = rgba[..vanilla.width * 4].chunks(4);
            let ramp: Vec<_> = row
                .map(|px| oklch::to_linear([px[0], px[1], px[2]]))
                .collect();
            return crate::sprites::png(&sphere(&ramp));
        }
        if view == View::Changes {
            grey_unchanged(&mut rgba, &vanilla.rgba, palette, path);
        }
        let small = shrink(&rgba, vanilla.width, vanilla.height);
        crate::sprites::png(&small)
    }
}

/// Every texel no write of `palette` on `path` moves, greyed and dimmed.
/// A tint moves every texel, so it greys none.
fn grey_unchanged(rgba: &mut [u8], vanilla: &[u8], palette: &Palette, path: &str) {
    let mut bands: Vec<Hues> = Vec::new();
    for write in palette
        .writes
        .iter()
        .filter(|w| w.texture().is_some_and(|t| t.path == path))
    {
        match write.kind {
            Kind::Band { hues, .. } => bands.push(hues),
            _ => return,
        }
    }
    for (px, was) in rgba.chunks_mut(4).zip(vanilla.chunks(4)) {
        if !bands.iter().any(|&b| paint::in_band(was, b).is_some()) {
            let dim = (paint::luma(px) / 4) as u8;
            px[..3].copy_from_slice(&[dim; 3]);
        }
    }
}

/// The colours `palette` writes to `material`, as `picked` sets them or
/// vanilla without a pick, in linear light.
fn material_colours(
    palette: &Palette,
    material: &str,
    picked: Option<&Picked>,
) -> Result<Vec<[f32; 3]>> {
    let on = |kind: &Kind| matches!(kind, Kind::Param { material: m, .. } if *m == material);
    let goals: Vec<_> = match picked {
        Some(picked) => {
            let here = jobs(palette, picked)?
                .into_iter()
                .filter(|j| on(&j.write.kind));
            here.map(|j| j.goal).collect()
        }
        None => palette
            .writes
            .iter()
            .filter(|w| on(&w.kind))
            .map(|w| w.vanilla())
            .collect(),
    };
    Ok(goals.into_iter().map(oklch::to_linear).collect())
}

/// The tallest texture shown as a gradient on a sphere: a LUT.
const GRADIENT: usize = 16;

/// Where the sphere's light comes from: up, left and toward the viewer.
const LIGHT: [f32; 3] = [-0.45, 0.55, 0.7];

/// A sphere `SIZE` across lit by `LIGHT`, its shading running through
/// `colours` from the darkest in shadow to the brightest in the light,
/// with a white glint. On a dark ground, so a pale gem still reads.
fn sphere(colours: &[[f32; 3]]) -> Texture {
    let mut ramp = colours.to_vec();
    let luma = |c: &[f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    ramp.sort_by(|a, b| luma(a).total_cmp(&luma(b)));
    let norm = LIGHT.iter().map(|v| v * v).sum::<f32>().sqrt();
    let light = LIGHT.map(|v| v / norm);
    let r = SIZE as f32 / 2.0 - 4.0;
    let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (nx, ny) = (
                (x as f32 + 0.5 - SIZE as f32 / 2.0) / r,
                (SIZE as f32 / 2.0 - y as f32 - 0.5) / r,
            );
            let d2 = nx * nx + ny * ny;
            let lin = if d2 > 1.0 {
                [0.012; 3]
            } else {
                let n = [nx, ny, (1.0 - d2).sqrt()];
                let lit = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
                let glint = lit.powi(40) * 0.6;
                ramp_at(&ramp, 0.15 + 0.85 * lit).map(|c| c + glint)
            };
            rgba.extend(oklch::from_linear(lin));
            rgba.push(255);
        }
    }
    Texture {
        name: String::new(),
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// `ramp` read at `t` from 0 to 1, blending its neighbours; grey if empty.
fn ramp_at(ramp: &[[f32; 3]], t: f32) -> [f32; 3] {
    match ramp {
        [] => [0.2; 3],
        [only] => only.map(|c| c * t),
        _ => {
            let at = t.clamp(0.0, 1.0) * (ramp.len() - 1) as f32;
            let (i, f) = (at.floor() as usize, at.fract());
            let (a, b) = (ramp[i], ramp[(i + 1).min(ramp.len() - 1)]);
            std::array::from_fn(|c| a[c] + (b[c] - a[c]) * f)
        }
    }
}

/// `rgba` scaled down by whole steps until it fits `SIZE`, each pixel the
/// average of the block it covers, opaque so an atlas reads plainly.
fn shrink(rgba: &[u8], width: usize, height: usize) -> Texture {
    let step = width.max(height).div_ceil(SIZE).max(1);
    let (w, h) = (width.div_ceil(step), height.div_ceil(step));
    let mut out = vec![255; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = ([0u32; 3], 0);
            for sy in y * step..((y + 1) * step).min(height) {
                for sx in x * step..((x + 1) * step).min(width) {
                    let at = (sy * width + sx) * 4;
                    (0..3).for_each(|c| sum[c] += rgba[at + c] as u32);
                    n += 1;
                }
            }
            let at = (y * w + x) * 4;
            (0..3).for_each(|c| out[at + c] = (sum[c] / n) as u8);
        }
    }
    Texture {
        name: String::new(),
        width: w,
        height: h,
        rgba: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_shrinks_by_whole_steps_to_fit() {
        let (w, h) = (600, 300);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| [(i % 2 * 200) as u8, 10, 20, 0])
            .collect();
        let small = shrink(&rgba, w, h);
        assert_eq!((small.width, small.height), (200, 100));
        // A 3x3 block of columns 0, 200, 0 averages 66, opaque.
        assert_eq!(small.rgba[0], 66);
        assert_eq!(&small.rgba[1..4], &[10, 20, 255]);
    }

    #[test]
    fn each_sheet_is_listed_once() {
        let sword = palette("kokiri-sword").unwrap();
        assert_eq!(sword.writes.len(), 2);
        assert_eq!(sheets(sword).len(), 1);
        assert_eq!(sheets(palette("kokiri-tunic").unwrap()).len(), 7);
        let brows = sheets(palette("brows").unwrap());
        assert_eq!(brows[0], Sheet::Material(super::super::files::EYEBROW));
        assert_eq!(sheets(palette("hair").unwrap()).len(), 3);
    }

    #[test]
    fn a_sphere_is_lit_through_its_colours() {
        let dark = [0.02, 0.0, 0.0];
        let bright = [0.9, 0.1, 0.1];
        let ball = sphere(&[bright, dark]);
        let px = |x: usize, y: usize| &ball.rgba[(y * SIZE + x) * 4..][..3];
        let (lit, shade) = (px(SIZE / 3, SIZE / 3), px(SIZE * 3 / 4, SIZE * 3 / 4));
        assert!(lit[0] > shade[0] + 60, "{lit:?} {shade:?}");
        assert!(lit[0] > lit[1] && lit[0] > lit[2]);
        assert_eq!(px(1, 1), px(SIZE - 2, 2), "the ground is flat");
    }

    #[test]
    fn every_palette_previews_each_texture_in_both_views() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let previews = Previews::default();
        let pink = Picked::of([220, 60, 160]);
        for palette in super::super::PALETTES {
            for t in 0..sheets(palette).len() {
                let vanilla = previews
                    .png(&romfs, palette.id, t, None, View::Paint)
                    .unwrap();
                let painted = previews.png(&romfs, palette.id, t, Some(&pink), View::Paint);
                let painted = painted.unwrap_or_else(|e| panic!("{} {t}: {e:#}", palette.id));
                let changes = previews.png(&romfs, palette.id, t, Some(&pink), View::Changes);
                assert_ne!(vanilla, painted, "{} {t} looks vanilla", palette.id);
                assert!(changes.is_ok(), "{} {t}", palette.id);
            }
        }
        assert!(
            previews
                .png(&romfs, "kokiri-tunic", 7, None, View::Paint)
                .is_err()
        );
        for palette in super::super::PALETTES {
            previews.check(&romfs, palette).unwrap();
        }
    }
}
