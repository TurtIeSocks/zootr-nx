//! The marks painted on a breakable's albedo: a gold band, gold Triforces
//! cut from a texture in the dump.

use anyhow::{Result, ensure};
use ounce_formats::{Romfs, Texture, top_level};

use crate::key_tints::paint::luma;
use crate::known::texture;
use crate::medallions::Colour;

/// The mark's colour.
pub(super) const GOLD: Colour = [255, 200, 60];

/// A texel's brightness lifted and squeezed before it takes the gold, so a
/// dark texel still reads gold and the grain stays.
const LIFT: (f32, f32) = (0.42, 0.72);

/// How light a texel of the graveyard's Triforce has to be to count as a
/// triangle and not the stone around it.
const LIGHT: u32 = 185;

/// An albedo being painted: its first level, and which texels changed.
pub(super) struct Canvas {
    pub(super) texture: Texture,
    pub(super) painted: Vec<bool>,
}

impl Canvas {
    pub(super) fn new(texture: Texture) -> Self {
        let painted = vec![false; texture.width * texture.height];
        Self { texture, painted }
    }

    fn at(&self, x: usize, y: usize) -> usize {
        y * self.texture.width + x
    }

    /// Texel (`x`, `y`) a `share` of the way (0 to 1) toward `colour`.
    fn blend(&mut self, x: usize, y: usize, colour: [f32; 3], share: f32) {
        if share <= 0.0 {
            return;
        }
        let at = self.at(x, y);
        let px = &mut self.texture.rgba[4 * at..4 * at + 3];
        for (c, to) in px.iter_mut().zip(colour) {
            *c = (*c as f32 + (to - *c as f32) * share.min(1.0)).round() as u8;
        }
        self.painted[at] = true;
    }

    /// Texel (`x`, `y`)'s brightened gold.
    fn gold(&self, x: usize, y: usize) -> [f32; 3] {
        let at = self.at(x, y);
        brightened(luma(&self.texture.rgba[4 * at..4 * at + 4]))
    }

    /// The rows between `top` and `bottom` (as fractions of the height) in
    /// brightened gold, fading in and out over `feather`.
    pub(super) fn band(&mut self, top: f32, bottom: f32, feather: f32) {
        self.patch((f32::NEG_INFINITY, top), (f32::INFINITY, bottom), feather);
    }

    /// The rectangle from (`left`, `top`) to (`right`, `bottom`), fractions
    /// of the texture, in brightened gold, fading in and out over
    /// `feather`.
    pub(super) fn patch(
        &mut self,
        (left, top): (f32, f32),
        (right, bottom): (f32, f32),
        feather: f32,
    ) {
        let (width, height) = (self.texture.width as f32, self.texture.height as f32);
        let inside = |at: f32, from: f32, to: f32| {
            ((at - from) / feather + 0.5).min((to - at) / feather + 0.5)
        };
        for y in 0..self.texture.height {
            let down = inside((y as f32 + 0.5) / height, top, bottom);
            if down <= 0.0 {
                continue;
            }
            for x in 0..self.texture.width {
                let share = down.min(inside((x as f32 + 0.5) / width, left, right));
                self.blend(x, y, self.gold(x, y), share.clamp(0.0, 1.0));
            }
        }
    }

    /// `mark` stamped in gold, centred at (`u`, `v`) and `height` tall
    /// (fractions of the texture), `width` as wide as `height` makes it
    /// on the model at `stretch`, metres per v over metres per u. It's
    /// turned upside down in the image when the model shows the image so.
    pub(super) fn stamp(
        &mut self,
        mark: &Mark,
        (u, v): (f32, f32),
        height: f32,
        stretch: f32,
        upright: Upright,
    ) {
        let (w, h) = (self.texture.width as f32, self.texture.height as f32);
        let tall = height * h;
        let wide = height * mark.aspect() * stretch * w;
        let (left, top) = (u * w - wide / 2.0, v * h - tall / 2.0);
        let xs = left.floor().max(0.0) as usize..((left + wide).ceil() as usize).min(w as usize);
        let ys = top.floor().max(0.0) as usize..((top + tall).ceil() as usize).min(h as usize);
        for y in ys {
            for x in xs.clone() {
                let down = (y as f32 + 0.5 - top) / tall;
                let down = match upright {
                    Upright::AsIs => down,
                    Upright::Flipped => 1.0 - down,
                };
                let share = mark.sample((x as f32 + 0.5 - left) / wide, down);
                // The stone's grain shows through the gold, a little.
                let grain = 0.8 + 0.2 * self.gold(x, y)[0] / 255.0;
                self.blend(x, y, GOLD.map(|c| c as f32 * grain), share);
            }
        }
    }
}

/// Which way up a model shows its texture: image rows running down it
/// (`AsIs`) or up it (`Flipped`).
#[derive(Clone, Copy)]
pub(super) enum Upright {
    #[allow(dead_code, reason = "the small crate's faces, next")]
    AsIs,
    Flipped,
}

/// The gold a texel of brightness `luma` (0 to 255) becomes.
fn brightened(luma: u32) -> [f32; 3] {
    let lit = (LIFT.0 + LIFT.1 * luma as f32 / 255.0).min(1.0);
    GOLD.map(|c| c as f32 * lit)
}

/// A shape to stamp: how much of each texel it covers, 0 to 1, apex up.
pub(super) struct Mark {
    width: usize,
    height: usize,
    cover: Vec<f32>,
}

impl Mark {
    /// The Triforce, cut by brightness from the graveyard's mark, which
    /// points right, and turned apex up.
    pub(super) fn triforce(romfs: &Romfs) -> Result<Self> {
        let name = texture::TRI_FORCE_MARK_GRAVEYARD_ASSETS_A_ALB;
        let source = top_level(&romfs.read(&super::texture_path(name))?)?;
        Self::cut(&source)
    }

    fn cut(source: &Texture) -> Result<Self> {
        let (w, h) = (source.width, source.height);
        let light = |x: usize, y: usize| luma(&source.rgba[4 * (y * w + x)..][..4]) > LIGHT;
        // Turned a quarter left: (x, y) here is (w - 1 - y, x) there.
        let (side, across) = (w, h);
        let turned = |x: usize, y: usize| light(side - 1 - y, x);
        let (mut x0, mut y0, mut x1, mut y1) = (across, side, 0, 0);
        for y in 0..side {
            for x in 0..across {
                if turned(x, y) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        ensure!(x1 > x0 && y1 > y0, "no light triangles in the Triforce");
        let (width, height) = (x1 - x0 + 1, y1 - y0 + 1);
        let cover = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| turned(x0 + x, y0 + y) as u8 as f32)
            .collect();
        Ok(Self {
            width,
            height,
            cover,
        })
    }

    /// Width over height.
    fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// How much the mark covers at (`x`, `y`), fractions of its size,
    /// bilinear between texels. Nothing outside it.
    fn sample(&self, x: f32, y: f32) -> f32 {
        if !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
            return 0.0;
        }
        let (fx, fy) = (x * self.width as f32 - 0.5, y * self.height as f32 - 0.5);
        let at = |x: f32, y: f32| {
            let x = (x.max(0.0) as usize).min(self.width - 1);
            let y = (y.max(0.0) as usize).min(self.height - 1);
            self.cover[y * self.width + x]
        };
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1.0, y0) * tx;
        let bottom = at(x0, y0 + 1.0) * (1.0 - tx) + at(x0 + 1.0, y0 + 1.0) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// How many texels it covers.
    #[cfg(test)]
    fn covered(&self) -> usize {
        self.cover.iter().filter(|&&c| c > 0.5).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_triforce_is_three_triangles_apex_up() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mark = Mark::triforce(&romfs).unwrap();
        // Three triangles fill three quarters of the big one: about 3/8 of
        // its box.
        let share = mark.covered() as f32 / (mark.width * mark.height) as f32;
        assert!((0.3..0.45).contains(&share), "{share}");
        // Apex up: the top row is narrow, the bottom row wide.
        let row = |y: f32| {
            (0..20)
                .filter(|&x| mark.sample(x as f32 / 20.0, y) > 0.5)
                .count()
        };
        assert!(
            row(0.05) <= 4 && row(0.95) >= 16,
            "{} {}",
            row(0.05),
            row(0.95)
        );
        // The hole in the middle.
        assert!(mark.sample(0.5, 0.6) < 0.5);
        // About as wide as an equilateral triangle is.
        assert!((1.05..1.3).contains(&mark.aspect()), "{}", mark.aspect());
    }

    #[test]
    fn brightened_gold_keeps_the_grain() {
        let (dark, light) = (brightened(40), brightened(200));
        assert!(dark[0] < light[0] && dark[1] < light[1]);
        // A dark texel still reads gold, not brown.
        assert!(dark[0] > 120.0, "{dark:?}");
        assert!(brightened(255)[0] <= 255.0);
    }

    #[test]
    fn a_band_fades_in_and_paints_only_its_rows() {
        let texture = Texture {
            name: String::new(),
            width: 4,
            height: 100,
            rgba: vec![100; 4 * 4 * 100],
        };
        let mut canvas = Canvas::new(texture);
        canvas.band(0.2, 0.5, 0.04);
        let painted_row = |y: usize| canvas.painted[y * 4];
        assert!(!painted_row(10) && painted_row(35) && !painted_row(70));
        let red = |y: usize| canvas.texture.rgba[4 * y * 4];
        // Gold is redder than the grey it covers, fully in the middle.
        assert!(red(35) > red(19) && red(19) >= 100);
    }

    #[test]
    fn a_patch_paints_only_its_rectangle() {
        let texture = Texture {
            name: String::new(),
            width: 100,
            height: 100,
            rgba: vec![100; 4 * 100 * 100],
        };
        let mut canvas = Canvas::new(texture);
        canvas.patch((0.2, 0.4), (0.3, 0.8), 0.02);
        let painted = |x: usize, y: usize| canvas.painted[y * 100 + x];
        assert!(painted(25, 60));
        assert!(!painted(10, 60) && !painted(40, 60), "outside it across");
        assert!(!painted(25, 30) && !painted(25, 90), "outside it down");
    }
}
