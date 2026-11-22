//! A top-down picture of a level, for checking what derive sees: floors
//! filled, walls drawn as lines, seen from above with +x right and +z down.

use anyhow::Result;

pub type Rgb = [u8; 3];

/// A canvas over the floor plan from `lo` to `hi` (x, z), `scale` pixels
/// a metre.
pub struct Picture {
    lo: [f32; 2],
    scale: f32,
    width: u32,
    height: u32,
    pixels: Vec<Rgb>,
}

impl Picture {
    pub fn new(lo: [f32; 2], hi: [f32; 2], scale: f32, ground: Rgb) -> Self {
        let width = (((hi[0] - lo[0]) * scale).ceil() as u32).max(1);
        let height = (((hi[1] - lo[1]) * scale).ceil() as u32).max(1);
        Self {
            lo,
            scale,
            width,
            height,
            pixels: vec![ground; (width * height) as usize],
        }
    }

    fn px(&self, p: [f32; 2]) -> [f32; 2] {
        [
            (p[0] - self.lo[0]) * self.scale,
            (p[1] - self.lo[1]) * self.scale,
        ]
    }

    fn put(&mut self, x: i64, y: i64, c: Rgb) {
        if (0..self.width as i64).contains(&x) && (0..self.height as i64).contains(&y) {
            self.pixels[(y as u32 * self.width + x as u32) as usize] = c;
        }
    }

    /// Fills the polygon through `ring` (x, z), convex or not, even-odd.
    pub fn fill(&mut self, ring: &[[f32; 2]], c: Rgb) {
        let pts: Vec<[f32; 2]> = ring.iter().map(|&p| self.px(p)).collect();
        let (top, bottom) =
            (pts.iter()).fold((f32::MAX, f32::MIN), |(t, b), p| (t.min(p[1]), b.max(p[1])));
        for y in top.floor() as i64..=bottom.ceil() as i64 {
            let row = y as f32 + 0.5;
            let mut xs: Vec<f32> = (0..pts.len())
                .filter_map(|i| {
                    let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                    ((a[1] <= row) != (b[1] <= row))
                        .then(|| a[0] + (row - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
                })
                .collect();
            xs.sort_by(f32::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                for x in pair[0].round() as i64..pair[1].round() as i64 {
                    self.put(x, y, c);
                }
            }
        }
    }

    /// Draws a line from `a` to `b` (x, z), `width` pixels wide.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], c: Rgb, width: i64) {
        let (a, b) = (self.px(a), self.px(b));
        let steps = (b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil().max(1.0) as i64;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            for dx in 0..width {
                for dy in 0..width {
                    self.put(x as i64 + dx - width / 2, y as i64 + dy - width / 2, c);
                }
            }
        }
    }

    /// Writes `n` in seven-segment digits `size` m tall, centred on `at`.
    pub fn number(&mut self, n: u32, at: [f32; 2], size: f32, c: Rgb) {
        // Segments by their ends: corners 0 to 5 run top left, top right,
        // middle left, middle right, bottom left, bottom right.
        const SEGS: [(usize, usize); 7] = [(0, 1), (0, 2), (1, 3), (2, 3), (2, 4), (3, 5), (4, 5)];
        const LIT: [&[usize]; 10] = [
            &[0, 1, 2, 4, 5, 6],
            &[2, 5],
            &[0, 2, 3, 4, 6],
            &[0, 2, 3, 5, 6],
            &[1, 2, 3, 5],
            &[0, 1, 3, 5, 6],
            &[0, 1, 3, 4, 5, 6],
            &[0, 2, 5],
            &[0, 1, 2, 3, 4, 5, 6],
            &[0, 1, 2, 3, 5, 6],
        ];
        let digits: Vec<usize> = n.to_string().bytes().map(|b| (b - b'0') as usize).collect();
        let (w, h, gap) = (size / 2.0, size / 2.0, size * 0.75);
        let left = at[0] - gap * (digits.len() as f32 - 1.0) / 2.0 - w / 2.0;
        let width = ((size * self.scale) / 6.0).max(2.0) as i64;
        for (i, &d) in digits.iter().enumerate() {
            let x = left + i as f32 * gap;
            let z = at[1];
            let corners = [
                [x, z - h],
                [x + w, z - h],
                [x, z],
                [x + w, z],
                [x, z + h],
                [x + w, z + h],
            ];
            for &s in LIT[d] {
                let (a, b) = SEGS[s];
                self.line(corners[a], corners[b], c, width);
            }
        }
    }

    pub fn png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()?
            .write_image_data(self.pixels.as_flattened())?;
        Ok(out)
    }
}

/// The segment where triangle `t` crosses the level plane at height `y`,
/// as (x, z) ends.
pub fn cut(t: &[[f32; 3]; 3], y: f32) -> Option<[[f32; 2]; 2]> {
    let mut ends = vec![];
    for i in 0..3 {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        if (a[1] < y) != (b[1] < y) {
            let s = (y - a[1]) / (b[1] - a[1]);
            ends.push([a[0] + (b[0] - a[0]) * s, a[2] + (b[2] - a[2]) * s]);
        }
    }
    (ends.len() == 2).then(|| [ends[0], ends[1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_triangle_cuts_a_plane_in_a_segment_and_misses_one_above_it() {
        let t = [[0.0, 0.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 2.0]];
        let [a, b] = cut(&t, 1.0).unwrap();
        assert_eq!((a, b), ([1.0, 0.0], [0.0, 1.0]));
        assert!(cut(&t, 3.0).is_none());
    }

    #[test]
    fn a_filled_square_colours_its_pixels() {
        let mut p = Picture::new([0.0, 0.0], [4.0, 4.0], 1.0, [0; 3]);
        p.fill(&[[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]], [9; 3]);
        assert_eq!(p.pixels.iter().filter(|c| **c == [9; 3]).count(), 4);
    }

    /// A 1 lights only its right side; an 8 lights more than a 1.
    #[test]
    fn a_one_lights_its_right_side_only() {
        let lit = |n| {
            let mut p = Picture::new([0.0, 0.0], [4.0, 4.0], 10.0, [0; 3]);
            p.number(n, [2.0, 2.0], 2.0, [9; 3]);
            let on: Vec<usize> = (0..p.pixels.len())
                .filter(|&i| p.pixels[i] == [9; 3])
                .collect();
            (
                on.iter().map(|i| i % p.width as usize).min().unwrap(),
                on.len(),
            )
        };
        let ((one_left, one), (eight_left, eight)) = (lit(1), lit(8));
        assert!(one_left > eight_left + 5, "{one_left} {eight_left}");
        assert!(eight > one * 2, "{eight} {one}");
    }
}
