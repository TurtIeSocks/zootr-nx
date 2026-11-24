//! A model copied in a colour: its albedo tinted by each texel's
//! brightness, and a key ring's iron and gold corner where it has one.
//! Keys take their dungeon's colour, another game's item its class's.

use anyhow::Result;
use ounce_formats::{ModBuilder, Romfs, Texture, redraw, rename, top_level};

use super::rings;
use crate::known::Name;
use crate::medallions::{Colour, model_path};

/// One model to copy: the actor it's named like, its textures by suffix,
/// and the prefix of a copy, which with a two-digit number is as long as
/// the host so no offset in a copy moves.
pub(crate) struct Look {
    pub(crate) host: &'static Name,
    /// The material its textures are named by, before the host's name:
    /// `Mt` when `None`.
    pub(crate) material: Option<&'static Name>,
    pub(crate) kinds: &'static [&'static str],
    /// Textures named another way, `{}` for the host's or a copy's name,
    /// the kind after the last `_`.
    pub(crate) others: &'static [&'static str],
    pub(crate) copy: &'static str,
    /// The copy's metal map cleared, so its painted albedo shows as its
    /// own colour. A metal reflects the room around it, which drowns a
    /// painted colour (the Magic Meter's sigil stayed gold, live).
    pub(crate) matte: bool,
}

impl Look {
    /// Copy `n`, counting from 1.
    pub(crate) fn name(&self, n: usize) -> String {
        format!("{}{n:02}", self.copy)
    }

    /// Each texture of `name`, the host or a copy, by kind.
    fn textures(&self, name: &str) -> Vec<(&'static str, String)> {
        let material = self.material.map_or("Mt", |m| m.get());
        let named = (self.kinds.iter()).map(|k| (*k, format!("{material}_{name}_{k}")));
        let others = self.others.iter().map(|o| {
            let kind = o.rsplit('_').next().unwrap_or(o);
            (kind, o.replace("{}", name))
        });
        (named.chain(others))
            .map(|(kind, file)| (kind, format!("Tex/{file}.bntx.zs")))
            .collect()
    }

    /// The model and texture files a copy is made from.
    #[cfg(test)]
    pub(crate) fn files(&self) -> impl Iterator<Item = String> {
        let textures = self.textures(self.host).into_iter().map(|(_, path)| path);
        std::iter::once(model_path(self.host)).chain(textures)
    }
}

/// A host model and its textures, read and decoded once.
pub(crate) struct Host {
    look: &'static Look,
    pub(crate) model: Vec<u8>,
    /// By kind and the host's path.
    textures: Vec<(&'static str, String, Vec<u8>, Texture)>,
}

impl Host {
    pub(crate) fn read(romfs: &Romfs, look: &'static Look) -> Result<Self> {
        let textures = (look.textures(look.host).into_iter())
            .map(|(kind, path)| {
                let data = romfs.read(&path)?;
                let top = top_level(&data)?;
                Ok((kind, path, data, top))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            look,
            model: romfs.read(&model_path(look.host))?,
            textures,
        })
    }

    /// Model `n`, its albedo tinted `tint`. Returns its name.
    pub(crate) fn paint(&self, builder: &mut ModBuilder, n: usize, tint: Colour) -> Result<String> {
        self.paint_with(builder, n, &|rgba| self::tint(rgba, tint))
    }

    /// Model `n`, its albedo's first level painted by `albedo`. Returns its
    /// name.
    pub(crate) fn paint_with(
        &self,
        builder: &mut ModBuilder,
        n: usize,
        albedo: Albedo,
    ) -> Result<String> {
        let name = self.look.name(n);
        self.write_with(builder, &name, &self.model, albedo, false)?;
        Ok(name)
    }

    /// `model`, a copy of the host's, written as `name` with the host's
    /// textures, the albedo tinted `tint`, and with `ring` each with the
    /// ring's plain metal corner.
    pub(crate) fn write_as(
        &self,
        builder: &mut ModBuilder,
        name: &str,
        model: &[u8],
        tint: Colour,
        ring: bool,
    ) -> Result<()> {
        self.write_with(builder, name, model, &|rgba| self::tint(rgba, tint), ring)
    }

    /// [`Self::write_as`] with the albedo painted by `albedo`.
    pub(crate) fn write_with(
        &self,
        builder: &mut ModBuilder,
        name: &str,
        model: &[u8],
        albedo: Albedo,
        ring: bool,
    ) -> Result<()> {
        let (look, host) = (self.look, self.look.host);
        builder.add(
            &model_path(name),
            &rename(model, host, name)?,
            &model_path(host),
        )?;
        let copies = look.textures(name).into_iter().map(|(_, path)| path);
        for ((kind, like, data, top), path) in self.textures.iter().zip(copies) {
            let matte = look.matte && *kind == METAL;
            let data = texture(&rename(data, host, name)?, top, kind, albedo, ring, matte)?;
            builder.add(&path, &data, like)?;
        }
        Ok(())
    }
}

/// Paints an albedo's first level, its texels RGBA.
pub(crate) type Albedo<'a> = &'a dyn Fn(&mut [u8]);

/// The metal map's kind. Its red channel is how metal each texel is.
const METAL: &str = "Mtl";

/// A texture of `kind`, `top` its first level decoded: an albedo's first
/// level painted by `albedo`, the other maps kept, so the metal maps keep
/// the shine. With `ring`, any kind with the ring's iron and gold corner.
/// `matte`, a metal map with no metal left.
fn texture(
    data: &[u8],
    top: &Texture,
    kind: &str,
    albedo_paint: Albedo,
    ring: bool,
    matte: bool,
) -> Result<Vec<u8>> {
    // Every texel redrawn: the albedo, or a metal map made matte.
    let whole = kind == "Alb" || matte;
    if !whole && !ring {
        return Ok(data.to_vec());
    }
    let mut t = top.clone();
    if kind == "Alb" {
        albedo_paint(&mut t.rgba);
    }
    if matte {
        t.rgba.chunks_mut(4).for_each(|px| px[0] = 0);
    }
    if ring {
        rings::paint(&mut t, &top.rgba, kind)?;
    }
    let (w, h) = (t.width, t.height);
    redraw(data, &t.rgba, |x, y| !whole && !rings::painted(w, h, x, y))
}

/// Each texel in `colour`, scaled by its brightness, so a coloured
/// albedo (the boss key's gold) takes the colour as cleanly as a grey one.
fn tint(rgba: &mut [u8], colour: Colour) {
    for px in rgba.chunks_mut(4) {
        let luma = luma(px);
        for (c, tint) in px.iter_mut().zip(colour) {
            *c = ((luma * tint as u32 + 127) / 255) as u8;
        }
    }
}

/// A texel's brightness: Rec. 709 luma, weights out of 256.
pub(crate) fn luma(px: &[u8]) -> u32 {
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
