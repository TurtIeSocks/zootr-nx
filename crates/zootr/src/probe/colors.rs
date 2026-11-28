//! `zootr probe` on what the colors feature writes: a model's materials,
//! an effect bundle's color constants, a texture's hue bands.

use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::Romfs;

/// A built file (a mod's) by its path, or the dump's by `piece`, a piece
/// of its path.
fn load(romfs: &Romfs, game: &Path, path: &str, piece: &str, what: &str) -> Result<Vec<u8>> {
    match Path::new(path).is_file() {
        true => romfs.dicts().decompress(path, &std::fs::read(path)?),
        false => {
            crate::nav::dump_file(romfs, game, piece)?.with_context(|| format!("no {what} {path}"))
        }
    }
}

/// `probe materials`.
pub(super) fn materials(romfs: &Romfs, game: &Path, model: &str) -> Result<()> {
    let data = load(romfs, game, model, model, "model")?;
    for m in ounce_formats::materials(&data)? {
        println!("{}", m.name);
        println!("  textures {}", m.textures.join(" "));
        for (hash, [r, g, b, a]) in &m.params {
            println!("  {hash:016x}  {r:.4} {g:.4} {b:.4} {a:.4}");
        }
    }
    Ok(())
}

/// `probe model`.
pub(super) fn model(romfs: &Romfs, game: &Path, model: &str, raw: Option<&Path>) -> Result<()> {
    let data = load(romfs, game, model, model, "model")?;
    if let Some(raw) = raw {
        std::fs::write(raw, &data)?;
    }
    let materials = ounce_formats::materials(&data)?;
    let skeleton = ounce_formats::skeleton(&data)?;
    for s in ounce_formats::shapes_of(&data)? {
        let placed = ounce_formats::model_vertices(&data, &skeleton, &s.name)?;
        let mut centre = [0.0f32; 3];
        for (p, _) in &placed {
            (0..3).for_each(|i| centre[i] += p[i] / placed.len() as f32);
        }
        let [x, y, z] = centre;
        let low = placed
            .iter()
            .map(|(p, _)| p[1])
            .fold(f32::INFINITY, f32::min);
        println!(
            "{}  bone {}  skin {}  {} vertices  centre {x:.2} {y:.2} {z:.2}  lowest {low:.3}",
            s.name,
            s.bone,
            s.skin,
            placed.len()
        );
        if let Ok(uvs) = ounce_formats::attribute(&data, &s.name, "_u0") {
            let range = |c: usize| {
                let each = uvs.iter().map(|uv| uv[c]);
                let lo = each.clone().fold(f32::INFINITY, f32::min);
                (lo, each.fold(f32::NEG_INFINITY, f32::max))
            };
            let ((u0, u1), (v0, v1)) = (range(0), range(1));
            println!("  uv {u0:.2}..{u1:.2} x {v0:.2}..{v1:.2}");
        }
        match materials.get(s.material) {
            Some(m) => println!("  {} {}", m.name, m.textures.join(" ")),
            None => println!("  material {} missing", s.material),
        }
    }
    // Where each bone sits at rest, which the vertices of a shape skinned
    // to it (skin 1) are carried by.
    for (i, b) in skeleton.bones.iter().enumerate() {
        let [x, y, z] = [b[0][3], b[1][3], b[2][3]];
        println!("bone {i}  at {x:.4} {y:.4} {z:.4}  y-axis {:.3}", b[1][1]);
    }
    println!("matrices name bones {:?}", skeleton.matrices);
    Ok(())
}

/// `probe vfx`.
pub(super) fn vfx(
    romfs: &Romfs,
    game: &Path,
    bundle: &str,
    sum: &[String],
    raw: Option<&Path>,
) -> Result<()> {
    use ounce_formats::vfx;
    let data = load(romfs, game, bundle, &format!("{bundle}."), "bundle")?;
    if let Some(raw) = raw {
        std::fs::write(raw, &data)?;
    }
    let mut last = None;
    for (eset, c) in vfx::constants_by_eset(&data)? {
        if last.as_ref() != Some(&eset) {
            println!("{}", eset.as_deref().unwrap_or("(before any eset)"));
            last = Some(eset);
        }
        let [r, g, b, a] = c.value;
        println!(
            "  {:#08x}  {:016x}  {r:.4} {g:.4} {b:.4} {a:.4}",
            c.at, c.hash
        );
    }
    if !sum.is_empty() {
        let esets: Vec<&str> = sum.iter().map(String::as_str).collect();
        let (sum, found) = crate::colors::in_esets(&data, &esets)?;
        println!("sum {sum:#010x} over {} constants", found.len());
    }
    Ok(())
}

/// `probe band`.
pub(super) fn band(
    romfs: &Romfs,
    game: &Path,
    texture: &str,
    bands: &[String],
    png: Option<&Path>,
) -> Result<()> {
    use crate::colors::paint;
    let data = crate::nav::dump_file(romfs, game, texture)?
        .with_context(|| format!("no texture {texture}"))?;
    let top = ounce_formats::top_level(&data)?;
    if let Some(png) = png {
        std::fs::write(png, crate::sprites::png(&top)?)?;
    }
    println!("{}x{}", top.width, top.height);
    let rgba = top.rgba;
    let n = (rgba.len() / 4).max(1) as u64;
    let mean: [u64; 3] =
        std::array::from_fn(|c| rgba.chunks(4).map(|px| px[c] as u64).sum::<u64>() / n);
    println!("all        {mean:?}");
    for b in bands {
        let (lo, hi) = b.split_once('-').context("a band is LO-HI")?;
        let hues = (lo.parse()?, hi.parse()?);
        let share = rgba
            .chunks(4)
            .filter(|px| paint::in_band(px, hues).is_some())
            .count();
        match paint::mean(&rgba, hues) {
            Some((h, s, v)) => println!(
                "{b:<10} {:?}  {:.1}% of texels",
                paint::rgb(h, s, v),
                100.0 * share as f64 / n as f64
            ),
            None => println!("{b:<10} no texel"),
        }
    }
    Ok(())
}

/// `probe layout`.
pub(super) fn layout(
    romfs: &Romfs,
    name: &str,
    uses: Option<u32>,
    record: Option<&str>,
) -> Result<()> {
    use ounce_formats::layout::palette;
    for archive in crate::colors::LAYOUT_ARCHIVES {
        let data = romfs.read(archive)?;
        let sarc = roead::sarc::Sarc::new(&data)?;
        let mut seen = std::collections::BTreeSet::new();
        for file in sarc.files() {
            let Some(file_name) = file.name() else {
                continue;
            };
            if !file_name.contains(name) || !seen.insert(file_name.to_owned()) {
                continue;
            }
            let copies = ounce_formats::copies(&data, file_name)?.len();
            println!("{file_name}  {copies} copies in {archive}");
            for (i, e) in palette(file.data())?.iter().enumerate() {
                let [r, g, b, a] = e.rgba;
                // UI colors are stored as they show, not in linear light.
                let hex = crate::colors::hex([r, g, b].map(|c| (c * 255.0).round() as u8));
                println!(
                    "  [{i:>2}] {hex}  {r:.4} {g:.4} {b:.4} {a:.4}  at {:#x}",
                    e.at
                );
            }
            let layout = file.data();
            for m in ounce_formats::layout::materials(layout)? {
                let white = u32::from_le_bytes(layout[m.white..m.white + 4].try_into()?);
                println!("  {}  white [{white}]", m.name);
            }
            if let Some(index) = uses {
                for (kind, slot, size) in ounce_formats::layout::RECORDS {
                    for (record, at) in ounce_formats::layout::records(layout, slot)? {
                        let word = |off: usize| {
                            u32::from_le_bytes(layout[at + off..at + off + 4].try_into().unwrap())
                        };
                        for off in (0..size).step_by(4).filter(|&off| word(off) == index) {
                            println!("  [{index}] in {kind} {record} +{off:#x}");
                        }
                    }
                }
            }
            for (kind, slot, size) in ounce_formats::layout::RECORDS {
                for (named, at) in ounce_formats::layout::records(layout, slot)? {
                    if record != Some(named.as_str()) {
                        continue;
                    }
                    println!("  {kind} {named} at {at:#x}");
                    for row in (0..size).step_by(16) {
                        let bytes = &layout[at + row..at + (row + 16).min(size)];
                        let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
                        println!("    +{row:02x}  {}", hex.join(" "));
                    }
                }
            }
            if uses.is_some() {
                for slot in ounce_formats::layout::WORD_TABLES {
                    let words = ounce_formats::layout::words(layout, slot)?;
                    println!("  root +{slot:#x}: {words:?}");
                }
            }
        }
    }
    Ok(())
}

/// `probe atlas`.
pub(super) fn atlas(romfs: &Romfs, name: &str, out: Option<&Path>, hues: bool) -> Result<()> {
    let data = romfs.read(crate::sprites::LAYOUT)?;
    for t in ounce_formats::textures_named(&data, |n| n.contains(name))? {
        let mean = |px: &mut dyn Iterator<Item = &[u8]>| {
            let (mut sum, mut n) = ([0u64; 3], 0u64);
            for p in px {
                (0..3).for_each(|c| sum[c] += p[c] as u64);
                n += 1;
            }
            let color = sum.map(|s| (s / n.max(1)) as u8);
            crate::colors::hex(color)
        };
        let all = mean(&mut t.rgba.chunks(4));
        let shown = mean(&mut t.rgba.chunks(4).filter(|p| p[3] > 0));
        println!(
            "{}  {}x{}  all {all}  shown {shown}",
            t.name, t.width, t.height
        );
        if hues {
            let mut steps = std::collections::BTreeMap::new();
            for p in t.rgba.chunks(4).filter(|p| p[3] > 0) {
                let (h, s, _) = crate::colors::paint::hsv([p[0], p[1], p[2]]);
                // Greys have no hue to band.
                if s > 0.25 {
                    *steps
                        .entry((h * 36.0).round() as u32 % 36 * 10)
                        .or_insert(0) += 1;
                }
            }
            for (degrees, n) in steps {
                println!("  {degrees:>3}°  {n}");
            }
        }
        if let Some(out) = out {
            std::fs::create_dir_all(out)?;
            std::fs::write(
                out.join(format!("{}.png", t.name)),
                crate::sprites::png(&t)?,
            )?;
        }
    }
    Ok(())
}
