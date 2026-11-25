//! `zootr probe` on what the colours feature writes: a model's materials,
//! an effect bundle's colour constants, a texture's hue bands.

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
pub(super) fn band(romfs: &Romfs, game: &Path, texture: &str, bands: &[String]) -> Result<()> {
    use crate::colors::paint;
    let data = crate::nav::dump_file(romfs, game, texture)?
        .with_context(|| format!("no texture {texture}"))?;
    let rgba = ounce_formats::top_level(&data)?.rgba;
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
