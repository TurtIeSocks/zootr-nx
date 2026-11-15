//! The pictures a song's medallion wears: the host's, recoloured to the
//! song's colour, with its songbook icon on the face.

use ounce_formats::Texture;

pub type Colour = [u8; 3];

/// Where the face sits on every host texture, as fractions of its size:
/// the front disc's centre and the radius inside its rim. The model maps
/// its face flat onto the disc, `FACE_SCALE` texture units to its one.
pub const FACE: (f32, f32) = (0.2693, 0.2667);
const FACE_RADIUS: f32 = 0.153;
pub const FACE_SCALE: f32 = 1.389;
/// How much of the face's width the icon spans.
const ICON_SPAN: f32 = 1.6;
/// The host's green's saturation, which a song's colour's stands in for.
const HOST_SATURATION: f32 = 0.75;
/// How much of the face's own light is left under the icon: dark enough
/// for the glyph to read through the glass's glare.
const FACE_SHADE: f32 = 0.4;
/// How steep the icon's edges stand in the normal map.
const RELIEF: f32 = 1.2;

/// Whether texel `x`, `y` of a `w` by `h` texture is on the face.
pub fn on_face(w: usize, h: usize, x: usize, y: usize) -> bool {
    let (dx, dy) = (x as f32 / w as f32 - FACE.0, y as f32 / h as f32 - FACE.1);
    dx * dx + dy * dy < FACE_RADIUS * FACE_RADIUS
}

/// Every texel turned to `colour`'s hue, its saturation scaled as the
/// host's green's is to the colour's, its brightness and alpha kept.
pub fn recolour(t: &mut Texture, colour: Colour) {
    let (hue, saturation, _) = hsv(colour);
    for px in t.rgba.chunks_mut(4) {
        let (_, s, v) = hsv([px[0], px[1], px[2]]);
        let s = (s * saturation / HOST_SATURATION).min(1.0);
        px[..3].copy_from_slice(&rgb(hue, s, v));
    }
}

/// The icon on the face, `colour` lightened, over the face dimmed.
pub fn paint_icon(t: &mut Texture, icon: &Texture, colour: Colour) {
    let glyph = colour.map(|c| c / 2 + 128);
    for (x, y) in face(t) {
        let at = 4 * (y * t.width + x);
        let cover = coverage(icon, t, x as f32, y as f32);
        for (c, g) in glyph.iter().enumerate() {
            let under = t.rgba[at + c] as f32 * FACE_SHADE;
            t.rgba[at + c] = (under + (*g as f32 - under) * cover).round() as u8;
        }
    }
}

/// A normal map's face made flat, with the icon raised on it. The map
/// keeps X in its colour and Y in its alpha, 128 being level.
pub fn emboss(t: &mut Texture, icon: &Texture) {
    for (x, y) in face(t) {
        let (fx, fy) = (x as f32, y as f32);
        let height = |dx: f32, dy: f32| coverage(icon, t, fx + dx, fy + dy);
        let nx = (height(-1.0, 0.0) - height(1.0, 0.0)) * RELIEF;
        let ny = (height(0.0, -1.0) - height(0.0, 1.0)) * RELIEF;
        let level = |n: f32| (128.0 + 127.0 * n.clamp(-1.0, 1.0)).round() as u8;
        let at = 4 * (y * t.width + x);
        let (xv, yv) = (level(nx), level(ny));
        t.rgba[at..at + 4].copy_from_slice(&[xv, xv, xv, yv]);
    }
}

/// The face filled with `value`, as an occlusion map's left unshaded.
pub fn fill(t: &mut Texture, value: u8) {
    for (x, y) in face(t) {
        let at = 4 * (y * t.width + x);
        t.rgba[at..at + 4].copy_from_slice(&[value, value, value, 255]);
    }
}

fn face(t: &Texture) -> Vec<(usize, usize)> {
    let (w, h) = (t.width, t.height);
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| on_face(w, h, x, y))
        .collect()
}

/// How much of texel `x`, `y` of `t` the icon covers, from 0 to 1: the
/// icon's alpha, sampled between its texels, centred on the face.
fn coverage(icon: &Texture, t: &Texture, x: f32, y: f32) -> f32 {
    let span = ICON_SPAN * FACE_RADIUS * t.width as f32;
    let scale = icon.width.max(icon.height) as f32 / span;
    let u = (x - FACE.0 * t.width as f32) * scale + icon.width as f32 / 2.0 - 0.5;
    let v = (y - FACE.1 * t.height as f32) * scale + icon.height as f32 / 2.0 - 0.5;
    let alpha = |i: i64, j: i64| -> f32 {
        let inside = (0..icon.width as i64).contains(&i) && (0..icon.height as i64).contains(&j);
        match inside {
            true => icon.rgba[4 * (j as usize * icon.width + i as usize) + 3] as f32 / 255.0,
            false => 0.0,
        }
    };
    let (i, j) = (u.floor() as i64, v.floor() as i64);
    let (fu, fv) = (u - u.floor(), v - v.floor());
    let top = alpha(i, j) * (1.0 - fu) + alpha(i + 1, j) * fu;
    let bottom = alpha(i, j + 1) * (1.0 - fu) + alpha(i + 1, j + 1) * fu;
    top * (1.0 - fv) + bottom * fv
}

/// Hue in turns, saturation and value, each 0 to 1.
fn hsv([r, g, b]: Colour) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let hue = match d {
        0.0 => 0.0,
        _ if max == r => ((g - b) / d).rem_euclid(6.0),
        _ if max == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    } / 6.0;
    let s = if max == 0.0 { 0.0 } else { d / max };
    (hue, s, max)
}

fn rgb(hue: f32, s: f32, v: f32) -> Colour {
    let h = hue.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|ch| ((ch + m) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(w: usize, h: usize, px: [u8; 4]) -> Texture {
        Texture {
            name: String::new(),
            width: w,
            height: h,
            rgba: px.repeat(w * h),
        }
    }

    /// An 8 by 8 icon, its middle 4 by 4 a white glyph.
    fn icon() -> Texture {
        let mut t = texture(8, 8, [255, 255, 255, 0]);
        for y in 2..6 {
            for x in 2..6 {
                t.rgba[4 * (y * 8 + x) + 3] = 255;
            }
        }
        t
    }

    #[test]
    fn hsv_and_back_keeps_a_colour() {
        for c in [[170, 120, 255], [255, 70, 40], [90, 230, 90], [10, 10, 10]] {
            let (h, s, v) = hsv(c);
            let back = rgb(h, s, v);
            assert!(back.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 1), "{c:?}");
        }
    }

    #[test]
    fn the_host_green_takes_the_songs_hue_and_keeps_its_brightness() {
        let mut t = texture(1, 1, [76, 126, 31, 200]);
        recolour(&mut t, [255, 70, 40]);
        let (h, _, v) = hsv([t.rgba[0], t.rgba[1], t.rgba[2]]);
        assert!((h - hsv([255, 70, 40]).0).abs() < 0.01);
        assert!((v - hsv([76, 126, 31]).2).abs() < 0.01);
        assert_eq!(t.rgba[3], 200);
    }

    #[test]
    fn the_icon_lights_the_face_centre_and_leaves_the_rim() {
        let mut t = texture(100, 100, [100, 0, 0, 255]);
        paint_icon(&mut t, &icon(), [200, 100, 50]);
        let at = |x: usize, y: usize| t.rgba[4 * (y * 100 + x)];
        assert_eq!(at(27, 27), 228, "the glyph's colour at the centre");
        assert_eq!(at(27, 15), 40, "the face dimmed beside the glyph");
        assert_eq!(at(90, 90), 100, "off the face");
    }

    #[test]
    fn an_embossed_face_is_level_away_from_the_icons_edges() {
        let mut t = texture(100, 100, [0, 0, 0, 0]);
        emboss(&mut t, &icon());
        let at = |x: usize, y: usize| &t.rgba[4 * (y * 100 + x)..][..4];
        assert_eq!(at(27, 27), [128, 128, 128, 128], "inside the glyph");
        assert_eq!(at(27, 15), [128, 128, 128, 128], "outside it");
        assert_eq!(at(0, 0), [0, 0, 0, 0], "off the face");
    }
}
