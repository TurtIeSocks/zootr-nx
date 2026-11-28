//! The key ring's icon, which the dump lacks: the small key's icon three
//! times, fanned about its bow's hole, on an iron hoop through the hole.

use ounce_formats::Texture;

use super::{EDGE, drawn_box};
use crate::inventory::KEY_RING;

/// Where the bow's hole and the bit's end sit in the small key's icon, as
/// shares of its drawing's box from the top left. The key lies diagonal,
/// its bow to the top right.
const HOLE: [f32; 2] = [0.72, 0.21];
const TIP: [f32; 2] = [0.2, 0.92];
/// Each key's turn about the hole in degrees, drawn back to front.
const FAN: [f32; 3] = [-30.0, 30.0, 0.0];
/// The hoop's radius and thickness, as shares of the key's longer side.
const HOOP: f32 = 0.2;
const TUBE: f32 = 0.04;

/// The ring painted from `key`, the small key's icon. None for a blank
/// one.
pub(super) fn paint(key: &Texture) -> Option<Texture> {
    let (x0, y0, x1, y1) = drawn_box(key)?;
    let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
    let at = |[u, v]: [f32; 2]| [x0 as f32 + u * w, y0 as f32 + v * h];
    let (hole, tip) = (at(HOLE), at(TIP));
    let long = w.max(h);
    // Room for the fan either side: the frame crops it after.
    let side = (2.2 * long) as usize;
    let mid = side as f32 / 2.0;
    let mut rgba = vec![0; side * side * 4];

    // The hoop lies past the hole, away from the bit, and runs through it.
    let away = [hole[0] - tip[0], hole[1] - tip[1]];
    let len = away[0].hypot(away[1]);
    let r = HOOP * long;
    let centre = [mid + away[0] / len * r, mid + away[1] / len * r];
    let iron = iron(key);
    for (i, px) in rgba.chunks_mut(4).enumerate() {
        let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let off = ((x - centre[0]).hypot(y - centre[1]) - r).abs();
        let cover = (TUBE * long / 2.0 - off + 0.5).clamp(0.0, 1.0);
        over(px, [iron[0], iron[1], iron[2], (cover * 255.0) as u8]);
    }

    for turn in FAN {
        let (sin, cos) = turn.to_radians().sin_cos();
        for (i, px) in rgba.chunks_mut(4).enumerate() {
            let (dx, dy) = ((i % side) as f32 - mid, (i / side) as f32 - mid);
            let sx = hole[0] + dx * cos + dy * sin;
            let sy = hole[1] - dx * sin + dy * cos;
            if sx < 0.0 || sy < 0.0 || sx >= key.width as f32 || sy >= key.height as f32 {
                continue;
            }
            let at = (sy as usize * key.width + sx as usize) * 4;
            over(px, key.rgba[at..at + 4].try_into().unwrap());
        }
    }
    Some(Texture {
        name: KEY_RING.into(),
        width: side,
        height: side,
        rgba,
    })
}

/// The key's own metal: its drawn pixels' mean color.
fn iron(key: &Texture) -> [u8; 3] {
    let drawn = key.rgba.chunks(4).filter(|p| p[3] > EDGE);
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for p in drawn {
        (0..3).for_each(|c| sum[c] += p[c] as u64);
        n += 1;
    }
    sum.map(|s| (s / n.max(1)) as u8)
}

/// `src` laid over `dst`, both straight RGBA.
fn over(dst: &mut [u8], src: [u8; 4]) {
    let (sa, da) = (src[3] as f32 / 255.0, dst[3] as f32 / 255.0);
    let a = sa + da * (1.0 - sa);
    if a == 0.0 {
        return;
    }
    for c in 0..3 {
        let mixed = src[c] as f32 * sa + dst[c] as f32 * da * (1.0 - sa);
        dst[c] = (mixed / a).round() as u8;
    }
    dst[3] = (a * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake key: a diagonal bar from the bottom left to the top right.
    fn fake_key() -> Texture {
        let side = 40;
        let mut rgba = vec![0; side * side * 4];
        for i in 4..36 {
            let at = ((side - 1 - i) * side + i) * 4;
            rgba[at..at + 4].copy_from_slice(&[150, 150, 160, 255]);
        }
        Texture {
            name: "FakeKey".into(),
            width: side,
            height: side,
            rgba,
        }
    }

    #[test]
    fn the_ring_fans_the_key_and_draws_wider_than_it() {
        let key = fake_key();
        let ring = paint(&key).unwrap();
        assert_eq!(ring.width, ring.height);
        let (x0, _, x1, _) = drawn_box(&ring).unwrap();
        let (k0, _, k1, _) = drawn_box(&key).unwrap();
        assert!(x1 - x0 > k1 - k0, "{} vs {}", x1 - x0, k1 - k0);
        // The hoop and keys share the key's metal.
        assert_eq!(iron(&ring), [150, 150, 160]);
    }

    #[test]
    fn a_blank_key_paints_nothing() {
        let blank = Texture {
            rgba: vec![0; 16],
            width: 2,
            height: 2,
            ..fake_key()
        };
        assert!(paint(&blank).is_none());
    }

    #[test]
    fn over_keeps_what_shows_through() {
        let mut px = [0, 0, 255, 255];
        over(&mut px, [255, 0, 0, 0]);
        assert_eq!(px, [0, 0, 255, 255]);
        over(&mut px, [255, 0, 0, 255]);
        assert_eq!(px, [255, 0, 0, 255]);
    }
}
