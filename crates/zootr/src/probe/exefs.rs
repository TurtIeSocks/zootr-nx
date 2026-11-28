//! `zootr probe main`: a build's code, decoded from the dump. `zootr probe
//! port`: 1.1.1's table mapped into another build.

use std::path::Path;

use anyhow::{Context, Result, bail};
use nup_format::nso::{SEGMENTS, build_id_hex};
use zootr_hook::V131072;

use crate::game::{Release, main_image};
use crate::port::{
    CONFIRMED, Image, LIKE, SOLE_CALLS, base_source, hand, liked, port as map, sole_calls, usual,
};

/// Writes `build`'s `main` flat to `out` and prints its build ID, where
/// each segment lies and how it was stored, and MOD0.
pub(super) fn print_main(game: &Path, build: Release, out: &Path) -> Result<()> {
    let (nso, image) = main_image(game, build)?;
    println!("{} build ID {}", build.name(), build_id_hex(&nso.build_id));
    for (s, name) in nso.segments.iter().zip(SEGMENTS) {
        let (from, to) = (s.address, s.end());
        let how = format!("{:?}", s.compression);
        println!(
            "{name:<6} {from:#09x}..{to:#09x} {how}, {} bytes stored",
            s.stored
        );
    }
    println!(
        "{:<6} {:#09x}..{:#09x}",
        "bss",
        nso.segments[2].end(),
        nso.end()
    );
    println!("{:<6} {:#09x}", "MOD0", nso.mod0(&image)?);
    std::fs::write(out, &image).with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {} bytes to {}", image.len(), out.display());
    Ok(())
}

/// Maps 1.1.1's table into `to`'s `main`, prints or writes the report, and
/// writes `to`'s table to `out` once every row is matched.
pub(super) fn port(game: &Path, to: Release, report: Option<&Path>, out: &Path) -> Result<()> {
    if to == Release::V111 {
        bail!("1.1.1 is the build the table is mapped from: try --to 1.0.0");
    }
    let (from_nso, from_bytes) = main_image(game, Release::V111)?;
    let (to_nso, to_bytes) = main_image(game, to)?;
    let from = Image::new(&from_nso, &from_bytes).context("reading 1.1.1's main")?;
    let into =
        Image::new(&to_nso, &to_bytes).with_context(|| format!("reading {}'s main", to.name()))?;
    let ported = map(
        &from,
        &V131072,
        &into,
        usual(Release::V111, to),
        crate::patch::needs(),
    );
    let liked = liked(&from, &V131072, &into, CONFIRMED);
    let sole = sole_calls(&from, &V131072, &into, &ported);
    let text = ported.report("1.1.1", to.name(), CONFIRMED, &liked, &sole);
    match report {
        Some(path) => {
            std::fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?
        }
        None => print!("{text}"),
    }
    let rows = CONFIRMED.iter().zip(&liked);
    if let Some(((name, ..), l)) = rows.into_iter().find(|(_, l)| **l < LIKE) {
        bail!("{name} is only {l:.3} like its 1.1.1 code, under {LIKE}: not confirmed");
    }
    if let Some(((name, ..), _)) = SOLE_CALLS.iter().zip(&sole).find(|(_, h)| !**h) {
        bail!("{name} isn't the sole call where it sits: not confirmed");
    }
    let build = ported.build(&hand())?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    }
    std::fs::write(out, base_source(&build))
        .with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {}", out.display());
    Ok(())
}
