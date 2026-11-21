//! Archipelago: the data the `znx` world loads, exported from our logic so
//! the Python side only evaluates it. Everything in it is named by us,
//! never by the game's text or ids.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ounce_formats::Romfs;

pub mod client;
mod death;
mod export;
mod golden;
mod ids;
mod names;
mod notice;
mod open;
mod rule;
mod seed;
mod zip;

pub use open::{ApSeed, Placed, Resolved};

/// The game's name in Archipelago.
pub const GAME: &str = "Ocarina of Time (znx)";

/// The Int an AP seed adds, counting the received items the client has
/// given Link. It's kept in the save, so the count and the items Link holds
/// always roll back together.
pub const RECEIVED: &str = "Rando_ApReceived";

/// The settings the AP world exposes, by id. Each passes the
/// `an_exposed_option_changes_only_rules` test.
pub const OPTIONS: &[&str] = &[
    "open-forest",
    "kakariko-gate",
    "door-of-time",
    "jabu-open",
    "open-waterfall",
    "adult-deku-tree",
    "carpenters-fast",
    "bombchus",
    "skip-cutscenes",
    "chest-size",
    "quiet-navi",
    "bunny-hood",
    "ageless-weapons",
];

/// Choice options the world leaves out, each as setting id, option and
/// the sentence of the setting's description that offers it: Bombchu Bags
/// join the pool, which the world's pool doesn't hold.
const HELD: &[(&str, &str, &str)] = &[(
    "bombchus",
    "bag",
    " Or three Bombchu Bags in the pool, holding 20, 30 and 40, and none until the first",
)];

/// Whether the world offers `option` of choice `id`.
fn offers(id: &str, option: &str) -> bool {
    !HELD.iter().any(|h| (h.0, h.1) == (id, option))
}

/// Setting `id`'s description, less what offers options the world leaves
/// out.
fn described(id: &str, description: &str) -> String {
    let held = HELD.iter().filter(|h| h.0 == id);
    held.fold(description.to_owned(), |d, h| d.replace(h.2, ""))
}

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
    use super::{HELD, data_dir, described};

    /// Each held sentence is still in its setting's text, so the world's
    /// copy never offers an option it leaves out.
    #[test]
    fn a_held_option_leaves_the_worlds_description() {
        let defs = crate::settings::schema();
        for (id, option, sentence) in HELD {
            let def = defs.iter().find(|d| d.id == *id).unwrap();
            assert!(def.description.contains(sentence), "{id}");
            assert!(
                !described(id, def.description).contains(sentence),
                "{id}={option}"
            );
        }
    }

    #[test]
    fn the_world_version_is_the_app_version() {
        let manifest = data_dir().join("../archipelago.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
        assert_eq!(manifest["world_version"], env!("CARGO_PKG_VERSION"));
    }
}
