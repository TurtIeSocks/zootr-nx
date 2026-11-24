//! `zootr probe main`: a build's code, decoded from the dump.

use std::path::Path;

use anyhow::{Context, Result};
use nup_format::nso::{SEGMENTS, build_id_hex};

use crate::game::{Release, main_image};

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
