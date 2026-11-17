//! A key's model copied in a dungeon's colour: its albedo tinted by each
//! texel's brightness, and a ring's iron and gold corner where it has one.

use anyhow::Result;
use ounce_formats::{ModBuilder, Romfs, Texture, redraw, rename, top_level};

use super::{Look, rings};
use crate::medallions::{Colour, model_path, texture_path};

/// A key's host model and its textures, read and decoded once.
pub(super) struct Host {
    look: &'static Look,
    pub(super) model: Vec<u8>,
    textures: Vec<(&'static str, Vec<u8>, Texture)>,
}

impl Host {
    pub(super) fn read(romfs: &Romfs, look: &'static Look) -> Result<Self> {
        let textures = look
            .kinds
            .iter()
            .map(|kind| {
                let data = romfs.read(&texture_path(look.host, kind))?;
                let top = top_level(&data)?;
                Ok((*kind, data, top))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            look,
            model: romfs.read(&model_path(look.host))?,
            textures,
        })
    }

    /// Model `n`, its albedo tinted `tint`. Returns its name.
    pub(super) fn paint(&self, builder: &mut ModBuilder, n: usize, tint: Colour) -> Result<String> {
        let name = self.look.name(n);
        self.write_as(builder, &name, &self.model, tint, false)?;
        Ok(name)
    }

    /// `model`, a copy of the host's, written as `name` with the host's
    /// textures, the albedo tinted `tint`, and with `ring` each with the
    /// ring's plain metal corner.
    pub(super) fn write_as(
        &self,
        builder: &mut ModBuilder,
        name: &str,
        model: &[u8],
        tint: Colour,
        ring: bool,
    ) -> Result<()> {
        let host = self.look.host;
        builder.add(
            &model_path(name),
            &rename(model, host, name)?,
            &model_path(host),
        )?;
        for (kind, data, top) in &self.textures {
            let data = texture(&rename(data, host, name)?, top, kind, tint, ring)?;
            builder.add(&texture_path(name, kind), &data, &texture_path(host, kind))?;
        }
        Ok(())
    }
}

/// A texture of `kind`, `top` its first level decoded: an albedo's first level in `colour`, each texel
/// scaled by its brightness, so the metal maps keep the shine and a
/// coloured albedo (the boss key's gold) takes the colour as cleanly as a
/// grey one. With `ring`, any kind with the ring's iron and gold corner.
fn texture(data: &[u8], top: &Texture, kind: &str, colour: Colour, ring: bool) -> Result<Vec<u8>> {
    let albedo = kind == "Alb";
    if !albedo && !ring {
        return Ok(data.to_vec());
    }
    let mut t = top.clone();
    let key = t.rgba.clone();
    if albedo {
        tint(&mut t.rgba, colour);
    }
    if ring {
        rings::paint(&mut t, &key, kind)?;
    }
    let (w, h) = (t.width, t.height);
    redraw(data, &t.rgba, |x, y| !albedo && !rings::painted(w, h, x, y))
}

fn tint(rgba: &mut [u8], colour: Colour) {
    for px in rgba.chunks_mut(4) {
        let luma = luma(px);
        for (c, tint) in px.iter_mut().zip(colour) {
            *c = ((luma * tint as u32 + 127) / 255) as u8;
        }
    }
}

/// A texel's brightness: Rec. 709 luma, weights out of 256.
pub(super) fn luma(px: &[u8]) -> u32 {
    (px[0] as u32 * 54 + px[1] as u32 * 183 + px[2] as u32 * 19 + 128) / 256
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grey_texel_takes_the_colour_scaled_by_its_grey() {
        let mut rgba = [255, 255, 255, 255, 128, 128, 128, 200];
        tint(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [60, 220, 60, 255, 30, 110, 30, 200]);
    }

    #[test]
    fn a_gold_texel_takes_the_colour_by_its_brightness_not_its_hue() {
        let mut rgba = [255, 200, 100, 255];
        tint(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [48, 176, 48, 255]);
    }
}
