//! Where each face of a model lands on its textures: every shape's finest
//! triangles painted in UV space, coloured by the way each faces.

use std::path::Path;

use anyhow::Result;
use ounce_formats::{Texture, attribute, shape_names, triangles};

use super::unpacked;

/// The picture's side, in pixels.
const SIDE: usize = 512;

/// Writes one PNG per shape of each model whose path contains `pattern` to
/// `out`, its triangles filled in UV space by the axis their normal leans
/// on most: X red, Y green, Z blue, bright facing the positive way and
/// dark the negative. Prints each shape's triangle count and UV bounds, and
/// writes a TSV beside each PNG: a row a triangle, each corner's position,
/// normal and UV, to find where a face of the model sits on the texture.
pub(super) fn uvs(
    romfs: &ounce_formats::Romfs,
    game: &Path,
    pattern: &str,
    out: &Path,
) -> Result<()> {
    std::fs::create_dir_all(out)?;
    unpacked(romfs, game, pattern, |path, data, _| {
        if !path.contains(".bfres") {
            return Ok(());
        }
        let model = path.rsplit('/').next().unwrap_or(path);
        let model = model.split(".bfres").next().unwrap_or(model);
        for shape in shape_names(data) {
            let read = || -> Result<_> {
                let uv = attribute(data, &shape, "_u0")?;
                let normal = attribute(data, &shape, "_n0")?;
                let position = attribute(data, &shape, "_p0")?;
                Ok((uv, normal, position, triangles(data, &shape)?))
            };
            // A shape drawn without a texture (a fade) has no UVs.
            let (uv, normal, position, tris) = match read() {
                Ok(r) => r,
                Err(e) => {
                    println!("{model}__{shape}: {e:#}");
                    continue;
                }
            };
            let mut rgba = vec![0u8; SIDE * SIDE * 4];
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            let mut stretch = Stretch::default();
            let mut heights = Heights::default();
            let mut rows = String::new();
            for tri in tris.as_chunks::<3>().0 {
                let at = [tri[0], tri[1], tri[2]].map(|i| [uv[i][0], uv[i][1]]);
                for p in &at {
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                stretch.add(tri.map(|i| position[i]), at);
                for i in tri {
                    heights.add(uv[*i][1], position[*i][1]);
                }
                let n = normal[tri[0]];
                fill(&mut rgba, at, facing([n[0], n[1], n[2]]));
                for &i in tri {
                    let (p, uv) = (position[i], uv[i]);
                    rows += &format!("{:.4}\t{:.4}\t{:.4}\t", p[0], p[1], p[2]);
                    rows += &format!("{:.3}\t{:.3}\t{:.3}\t", n[0], n[1], n[2]);
                    rows += &format!("{:.4}\t{:.4}\t", uv[0], uv[1]);
                }
                rows.pop();
                rows.push('\n');
            }
            let name = format!("{model}__{shape}");
            let picture = Texture {
                name: name.clone(),
                width: SIDE,
                height: SIDE,
                rgba,
            };
            std::fs::write(out.join(format!("{name}.tsv")), rows)?;
            std::fs::write(
                out.join(format!("{name}.png")),
                crate::sprites::png(&picture)?,
            )?;
            println!(
                "{name}: {} triangles, u {:.3}..{:.3}, v {:.3}..{:.3}, {:.3} m per u, {:.3} m per v",
                tris.len() / 3,
                lo[0],
                hi[0],
                lo[1],
                hi[1],
                stretch.per(0),
                stretch.per(1),
            );
            heights.print();
        }
        Ok(())
    })
}

/// How far a step along u or v goes on the model, in metres: each
/// triangle's own, weighted by its area.
#[derive(Default)]
struct Stretch {
    sum: [f32; 2],
    area: f32,
}

impl Stretch {
    fn add(&mut self, p: [[f32; 4]; 3], uv: [[f32; 2]; 3]) {
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let (e1, e2) = (sub(p[1], p[0]), sub(p[2], p[0]));
        let (d1, d2) = (
            [uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]],
            [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]],
        );
        let det = d1[0] * d2[1] - d2[0] * d1[1];
        if det.abs() < 1e-9 {
            return;
        }
        // The model's direction along u and along v: [e1 e2] times the
        // inverse of [d1 d2].
        let along = |a: f32, b: f32| [0, 1, 2].map(|k| (e1[k] * a + e2[k] * b) / det);
        let length = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let area = length(cross) / 2.0;
        self.sum[0] += area * length(along(d2[1], -d1[1]));
        self.sum[1] += area * length(along(-d2[0], d1[0]));
        self.area += area;
    }

    fn per(&self, axis: usize) -> f32 {
        self.sum[axis] / self.area.max(f32::MIN_POSITIVE)
    }
}

/// Where each tenth of the texture's height lands on the model: the
/// heights of the vertices whose v falls in it.
#[derive(Default)]
struct Heights {
    rows: [(f32, f32, usize); 10],
}

impl Heights {
    fn add(&mut self, v: f32, y: f32) {
        let row = ((v.rem_euclid(1.0) * 10.0) as usize).min(9);
        let (lo, hi, n) = &mut self.rows[row];
        (*lo, *hi) = if *n == 0 {
            (y, y)
        } else {
            (lo.min(y), hi.max(y))
        };
        *n += 1;
    }

    fn print(&self) {
        for (row, (lo, hi, n)) in self.rows.iter().enumerate() {
            if *n > 0 {
                println!(
                    "  v {:.1}..{:.1}: y {lo:.3}..{hi:.3} ({n})",
                    row as f32 / 10.0,
                    (row + 1) as f32 / 10.0
                );
            }
        }
    }
}

/// The colour of a face whose normal is `n`.
fn facing(n: [f32; 3]) -> [u8; 4] {
    let axis = (0..3)
        .max_by(|&a, &b| n[a].abs().total_cmp(&n[b].abs()))
        .unwrap_or(0);
    let mut c = [0, 0, 0, 255];
    c[axis] = if n[axis] >= 0.0 { 255 } else { 110 };
    c
}

/// `tri`, in UV space wrapped into the picture, filled with `colour`. It
/// wraps whole, so a triangle across a tile's edge stays one piece: moved
/// to start in the first tile, then painted again a tile back on each axis
/// for the part that runs into the next.
fn fill(rgba: &mut [u8], tri: [[f32; 2]; 3], colour: [u8; 4]) {
    let start = |k: usize| tri.iter().map(|p| p[k]).fold(f32::MAX, f32::min).floor();
    let (u0, v0) = (start(0), start(1));
    let px = tri.map(|[u, v]| [(u - u0) * SIDE as f32, (v - v0) * SIDE as f32]);
    let back = -(SIDE as f32);
    for dx in [0.0, back] {
        for dy in [0.0, back] {
            paint(rgba, px.map(|[x, y]| [x + dx, y + dy]), colour);
        }
    }
}

/// `px`, in pixels, filled with `colour` where it overlaps the picture.
fn paint(rgba: &mut [u8], px: [[f32; 2]; 3], colour: [u8; 4]) {
    let x0 = px.iter().map(|p| p[0]).fold(f32::MAX, f32::min).max(0.0) as usize;
    let x1 = (px.iter().map(|p| p[0]).fold(0.0, f32::max) as usize).min(SIDE - 1);
    let y0 = px.iter().map(|p| p[1]).fold(f32::MAX, f32::min).max(0.0) as usize;
    let y1 = (px.iter().map(|p| p[1]).fold(0.0, f32::max) as usize).min(SIDE - 1);
    let edge = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    let area = edge(px[0], px[1], px[2]);
    if area == 0.0 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let w = [
                edge(px[1], px[2], p),
                edge(px[2], px[0], p),
                edge(px[0], px[1], p),
            ];
            if w.iter().all(|w| w * area >= 0.0) {
                let at = (y * SIDE + x) * 4;
                rgba[at..at + 4].copy_from_slice(&colour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A triangle across the right edge shows at that edge and wraps to
    /// the left one, not as a sliver across the whole picture.
    #[test]
    fn a_triangle_across_an_edge_wraps_whole() {
        let mut rgba = vec![0u8; SIDE * SIDE * 4];
        fill(&mut rgba, [[0.9, 0.1], [1.1, 0.1], [1.0, 0.3]], [255; 4]);
        let painted = |u: f32, v: f32| {
            let (x, y) = ((u * SIDE as f32) as usize, (v * SIDE as f32) as usize);
            rgba[(y * SIDE + x) * 4 + 3] != 0
        };
        assert!(painted(0.98, 0.15));
        assert!(painted(0.02, 0.15));
        assert!(!painted(0.5, 0.15));
    }
}
