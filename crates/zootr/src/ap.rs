//! Archipelago: the data the `znx` world loads, exported from our logic so
//! the Python side only evaluates it. Everything in it is named by us,
//! never by the game's text or ids.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ounce_formats::Romfs;

mod export;
mod golden;
mod ids;
mod names;
mod rule;
mod seed;

/// The settings the AP world exposes, by id. Each passes the
/// `an_exposed_option_changes_only_rules` test.
pub const OPTIONS: &[&str] = &[
    "open-forest",
    "kakariko-gate",
    "open-door-of-time",
    "free-bombchus",
    "skip-cutscenes",
    "chest-size",
    "quiet-navi",
    "bunny-hood",
];

/// Where the world's data lives in the repo.
pub fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ap/znx/data")
}

/// Export into `dir`, keeping the ids its `ids.json` already gave out.
pub fn write(romfs: &Romfs, dir: &Path) -> Result<()> {
    let ids = std::fs::read_to_string(dir.join("ids.json")).ok();
    let files = export::export(romfs, ids.as_deref())?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for (name, text) in files {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::data_dir;

    #[test]
    fn the_world_version_is_the_app_version() {
        let manifest = data_dir().join("../archipelago.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
        assert_eq!(manifest["world_version"], env!("CARGO_PKG_VERSION"));
    }
}
