//! Archipelago: the data the `znx` world loads, exported from our logic so
//! the Python side only evaluates it. Everything in it is named by us,
//! never by the game's text or ids.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use crate::settings::Archipelago::{self, Later, Never, Offered};
use crate::settings::SettingDef;

pub mod bench;
pub mod bench_live;
pub mod client;
mod death;
mod export;
mod golden;
mod ids;
pub(crate) mod names;
mod notice;
mod open;
mod presets;
pub mod room;
mod rule;
mod seed;
pub mod there;
pub mod timing;
mod when;
mod zip;

pub use open::{ApSeed, Placed, Resolved};

/// The game's name in Archipelago.
pub const GAME: &str = "Ocarina of Time (znx)";

/// The Int an AP seed adds, counting the received items the client has
/// given Link. It's kept in the save, so the count and the items Link holds
/// always roll back together.
pub const RECEIVED: &str = "Rando_ApReceived";

/// The settings the AP world exposes, by id. An option changes the
/// export's rules, or the checks and pool it holds through `when` rows.
pub const OPTIONS: &[&str] = &[
    "open-forest",
    "kakariko-gate",
    "met-zelda",
    "door-of-time",
    "jabu-open",
    "open-waterfall",
    "adult-deku-tree",
    "gerudo-fortress",
    "npc-gifts",
    "songs",
    "hundred-token-reward",
    "trade-chain",
    "merchants",
    "master-sword-shuffle",
    "shopsanity",
    "shop-slots",
    "freestanding",
    "cows",
    "gold-skulltulas",
    "boss-hearts",
    "dungeon-rewards",
    "maps-compasses",
    "keys",
    "fortress-keys",
    "bombchus",
    "skip-cutscenes",
    "chest-size",
    "quiet-navi",
    "bunny-hood",
    "ageless-weapons",
    "scrubs",
    "running-race",
    "pots",
    "crates",
    "rocks",
    "grass",
    "hives",
    "shuffle-masks",
    "ice-traps",
];

/// Every setting the world doesn't offer, and why: each one coming in a
/// stage of the "Missing from AP" queue, or never. With [`OPTIONS`], it
/// names every setting the app has, so one the app gains waits on a call
/// here (a test holds it), and the app marks these as not in AP.
pub const LEFT_OUT: &[(&str, Archipelago)] = &[
    // Off, Dodongo's Cavern's Bomb Bag chest keeps its bag, and logic
    // shows no child way to it.
    ("chests", Later(CHECK_KINDS)),
    // With progressive items, each makes its line start a level lower, so
    // the Nut Bag (30) a gift or scrub deals becomes the Nut Bag (20) and
    // Double Magic the Magic Meter: which item a check deals depends on
    // two options, NPC Gifts or Scrubs with this.
    ("shuffle-nut-bag", Later(CHECK_KINDS)),
    ("shuffle-stick-bag", Later(CHECK_KINDS)),
    ("magic-with-first-item", Later(COMFORT)),
    ("progressive-items", Later(ITEM_POOL)),
    ("logic", Later(RULES)),
    ("starting-age", Later(RULES)),
    ("rainbow-bridge", Later(RULES)),
    ("ganons-trials", Later(RULES)),
    ("king-zora-moved", Later(RULES)),
    ("adult-jabu-jabu", Later(RULES)),
    ("hints", Later(RULES)),
    ("day-skulltulas", Later(RULES)),
    ("shuffle-dungeon-entrances", Later(RULES)),
    ("scrub-price", Later(COMFORT)),
    ("ageless-items", Later(COMFORT)),
    ("quiet-enemies", Later(COMFORT)),
    ("menus", Later(COMFORT)),
    ("clock", Later(COMFORT)),
    ("no-tutorials", Later(COMFORT)),
    ("no-pickup-texts", Later(COMFORT)),
    (
        "shuffle-items",
        Never("off, every check holds what it always does, which leaves nothing to share"),
    ),
    (
        "start-inventory",
        Never("Archipelago sets what Link starts with from its own start inventory"),
    ),
];

/// The queue's stages, as the app names them.
const CHECK_KINDS: &str = "check kinds";
const ITEM_POOL: &str = "the item pool";
const RULES: &str = "goal and rules";
const KEYS: &str = "keys";
const COMFORT: &str = "comfort";

/// Whether the world offers `def`. A single cutscene skip under one the
/// world offers waits for the comfort stage: the world offers them only
/// all at once.
pub fn archipelago(def: &SettingDef) -> Option<Archipelago> {
    if OPTIONS.contains(&def.id.as_str()) {
        return Some(Offered);
    }
    if let Some((_, left)) = LEFT_OUT.iter().find(|(id, _)| *id == def.id) {
        return Some(*left);
    }
    (def.parent.is_some_and(|p| OPTIONS.contains(&p))).then_some(Later(COMFORT))
}

/// Whether the world offers `option` of choice `id`, as the app marks it:
/// the key options wait for the keys.
pub fn archipelago_option(id: &str, option: &str) -> Archipelago {
    match offers(id, option) {
        true => Offered,
        false => Later(KEYS),
    }
}

/// Exposed options that change the checks or the pool together, beyond
/// what each changes alone, so the export builds each group's
/// combinations whole: NPC Gifts with Met Zelda (Zelda's Letter ✅ stays
/// put, no Weird Egg), with Gerudo Fortress (Freed keeps the membership
/// card's check) and with Cows (the gifts' bottles fill the pouch's five,
/// so the cows' milk deals as rupees).
pub const JOINT: &[&[&str]] = &[&["npc-gifts", "met-zelda", "gerudo-fortress", "cows"]];

/// Choice options the world leaves out, each as setting id, option and
/// a sentence of the setting's description that offers it. Dungeon keys
/// offer Vanilla and Start only, so far.
const HELD: &[(&str, &str, &str)] = &[
    (
        "keys",
        "rings",
        " With Key rings, each dungeon's small keys come as one ring that opens all its small-key doors.",
    ),
    (
        "keys",
        "rings",
        " With Key rings, one of a dungeon's rupees becomes its ring and another its boss key.",
    ),
    (
        "keys",
        "singles",
        " With Keysanity, each small key comes on its own.",
    ),
    (
        "keys",
        "singles",
        " Rings, small keys and boss keys turn up in random chests and gifts, never in their own dungeon.",
    ),
];

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

/// Where the world's data lives in the repo. Cargo's run-time
/// `CARGO_MANIFEST_DIR` comes first: CI's test binary can be another job's
/// build, whose baked-in checkout is gone.
pub fn data_dir() -> PathBuf {
    let crate_dir = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
    let crate_dir = crate_dir.unwrap_or_else(|| env!("CARGO_MANIFEST_DIR").into());
    crate_dir.join("../../ap/znx/data")
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

    /// Every setting is offered or left out, never both, never neither.
    #[test]
    fn every_setting_is_offered_or_left_out_with_a_reason() {
        let defs = crate::settings::schema();
        let unsorted: Vec<_> = (defs.iter())
            .filter(|d| super::archipelago(d).is_none())
            .map(|d| &d.id)
            .collect();
        assert!(
            unsorted.is_empty(),
            "the AP world neither offers nor leaves out (ap::LEFT_OUT): {unsorted:?}"
        );
        for (id, why) in super::LEFT_OUT {
            assert!(!super::OPTIONS.contains(id), "{id} is offered and left out");
            assert!(!why.why().is_empty(), "{id} is left out without a reason");
            assert!(defs.iter().any(|d| d.id == *id), "{id} isn't a setting");
            let twice = super::LEFT_OUT.iter().filter(|(other, _)| other == id);
            assert_eq!(twice.count(), 1, "{id} is left out twice");
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
