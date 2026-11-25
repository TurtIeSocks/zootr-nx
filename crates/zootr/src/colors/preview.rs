//! The app's preview of a palette: one of its game textures, painted as
//! the seed paints it, small enough to redraw as the picker moves.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use ounce_formats::{Romfs, Texture, top_level};

use super::paint::{self, Hues};
use super::{Palette, Picked, Write, jobs, paint_jobs, palette};

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

/// The textures `palette` paints, each once, in catalog order.
pub fn textures(palette: &Palette) -> Vec<&'static str> {
    let mut out = Vec::new();
    for path in palette.writes.iter().map(|w| w.texture().path) {
        if !out.contains(&path) {
            out.push(path);
        }
    }
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
            super::check(&self.vanilla(romfs, write.texture().path)?.rgba, write)?;
        }
        Ok(())
    }

    /// `id`'s texture number `texture` as a PNG, at most `SIZE` a side:
    /// painted with `picked`, or vanilla without one.
    pub fn png(
        &self,
        romfs: &Romfs,
        id: &str,
        texture: usize,
        picked: Option<&Picked>,
        view: View,
    ) -> Result<Vec<u8>> {
        let palette = palette(id)?;
        let path = *textures(palette)
            .get(texture)
            .with_context(|| format!("{id} has no texture {texture}"))?;
        let vanilla = self.vanilla(romfs, path)?;
        let mut rgba = vanilla.rgba.clone();
        if let Some(picked) = picked {
            let here = jobs(palette, picked)?.into_iter();
            let here: Vec<_> = here.filter(|j| j.write.texture().path == path).collect();
            paint_jobs(&mut rgba, &here)?;
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
    for write in palette.writes.iter().filter(|w| w.texture().path == path) {
        match write {
            Write::Band { hues, .. } => bands.push(*hues),
            Write::Tint { .. } => return,
        }
    }
    for (px, was) in rgba.chunks_mut(4).zip(vanilla.chunks(4)) {
        if !bands.iter().any(|&b| paint::in_band(was, b).is_some()) {
            let dim = (paint::luma(px) / 4) as u8;
            px[..3].copy_from_slice(&[dim; 3]);
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
    fn each_texture_is_listed_once() {
        let sword = palette("kokiri-sword").unwrap();
        assert_eq!(sword.writes.len(), 2);
        assert_eq!(textures(sword).len(), 1);
        assert_eq!(textures(palette("kokiri-tunic").unwrap()).len(), 7);
    }

    #[test]
    fn every_palette_previews_each_texture_in_both_views() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let previews = Previews::default();
        let pink = Picked::of([220, 60, 160]);
        for palette in super::super::PALETTES {
            for t in 0..textures(palette).len() {
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
