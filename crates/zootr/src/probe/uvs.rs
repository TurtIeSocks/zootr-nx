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
/// dark the negative. Prints each shape's triangle count and UV bounds.
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
                Ok((uv, normal, triangles(data, &shape)?))
            };
            // A shape drawn without a texture (a fade) has no UVs.
            let (uv, normal, tris) = match read() {
                Ok(r) => r,
                Err(e) => {
                    println!("{model}__{shape}: {e:#}");
                    continue;
                }
            };
            let mut rgba = vec![0u8; SIDE * SIDE * 4];
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for tri in tris.as_chunks::<3>().0 {
                let at = [tri[0], tri[1], tri[2]].map(|i| [uv[i][0], uv[i][1]]);
                for p in &at {
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                let n = normal[tri[0]];
                fill(&mut rgba, at, facing([n[0], n[1], n[2]]));
            }
            let name = format!("{model}__{shape}");
            let picture = Texture {
                name: name.clone(),
                width: SIDE,
                height: SIDE,
                rgba,
            };
            std::fs::write(
                out.join(format!("{name}.png")),
                crate::sprites::png(&picture)?,
            )?;
            println!(
                "{name}: {} triangles, u {:.3}..{:.3}, v {:.3}..{:.3}",
                tris.len() / 3,
                lo[0],
                hi[0],
                lo[1],
                hi[1]
            );
        }
        Ok(())
    })
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

/// `tri`, in UV space wrapped into the picture, filled with `colour`.
fn fill(rgba: &mut [u8], tri: [[f32; 2]; 3], colour: [u8; 4]) {
    let px = tri.map(|[u, v]| {
        [
            u.rem_euclid(1.0) * SIDE as f32,
            v.rem_euclid(1.0) * SIDE as f32,
        ]
    });
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
