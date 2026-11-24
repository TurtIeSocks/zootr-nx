//! Archipelago: the data the `znx` world loads, exported from our logic so
//! the Python side only evaluates it. Everything in it is named by us,
//! never by the game's text or ids.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use crate::settings::Archipelago::{self, AlwaysOn, Later, Never, Offered, OnUntil};
use crate::settings::{Kind, SKIP_ALL, SettingDef, schema};

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
    "progressive-items",
    "shuffle-nut-bag",
    "shuffle-stick-bag",
    "magic-with-first-item",
    "logic",
    "starting-age",
    "rainbow-bridge",
    "ganons-trials",
    "king-zora-moved",
    "adult-jabu-jabu",
    "day-skulltulas",
    "scrub-price",
    "ageless-items",
    "quiet-enemies",
    "menus",
    "clock",
    "no-tutorials",
    "no-pickup-texts",
    "hints",
];

/// Settings the world offers together as one AP option, a list of the
/// children of one parent setting, which turns them all on.
pub struct Set {
    pub id: &'static str,
    pub label: &'static str,
    pub parent: &'static str,
    pub description: &'static str,
}

pub const SETS: &[Set] = &[Set {
    id: "skipped-cutscenes",
    label: "Skipped Cutscenes",
    parent: SKIP_ALL,
    description: "Each cutscene skipped, with Skip Cutscenes off. On, it skips them all",
}];

impl Set {
    /// The settings this set lists.
    pub fn members(&self) -> Vec<SettingDef> {
        let parent = Some(self.parent);
        schema()
            .into_iter()
            .filter(|d| d.parent == parent)
            .collect()
    }
}

/// Every setting the world sets: each option, then each set's children.
pub fn exposed() -> Vec<String> {
    let options = OPTIONS.iter().map(|id| (*id).to_owned());
    options
        .chain(
            SETS.iter()
                .flat_map(|s| s.members().into_iter().map(|d| d.id)),
        )
        .collect()
}

/// Every setting the world doesn't offer, and why: each one coming in a
/// stage of the "Missing from AP" queue, or never. With [`OPTIONS`], it
/// names every setting the app has, so one the app gains waits on a call
/// here (a test holds it), and the app marks these as not in AP.
pub const LEFT_OUT: &[(&str, Archipelago)] = &[
    // Off, Dodongo's Cavern's Bomb Bag chest keeps its bag, and logic
    // shows no child way to it.
    ("chests", Later(CHECK_KINDS)),
    ("shuffle-dungeon-entrances", Later(RULES)),
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
const RULES: &str = "goal and rules";
const KEYS: &str = "keys";

/// Whether the world offers `def`, alone or in a set. A toggle left out
/// but on in every Archipelago seed (its default, or pinned like Chests)
/// says so. The schema calls it, so it reads `def` alone.
pub fn archipelago(def: &SettingDef) -> Option<Archipelago> {
    let in_set = SETS.iter().any(|s| def.parent == Some(s.parent));
    if in_set || OPTIONS.contains(&def.id.as_str()) {
        return Some(Offered);
    }
    let (_, left) = LEFT_OUT.iter().find(|(id, _)| *id == def.id)?;
    Some(match (*left, on_in_ap(def)) {
        (Later(why), true) => OnUntil(why),
        (Never(why), true) => AlwaysOn(why),
        (left, _) => left,
    })
}

/// Whether every Archipelago seed holds toggle `def` on: the world's base
/// pins it on, or leaves it at a default of on.
fn on_in_ap(def: &SettingDef) -> bool {
    match seed::pinned().get(&def.id) {
        Some(value) => value == &serde_json::Value::Bool(true),
        None => matches!(def.kind, Kind::Toggle { default: true }),
    }
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
/// card's check), with Cows (the gifts' bottles fill the pouch's five,
/// so the cows' milk deals as rupees) and with King Zora Moved (either
/// makes Ruto's Letter ✅ a check). And Progressive Items with the
/// settings that start a line a level lower (a starter bag shuffled, the
/// Magic Meter pooled): which levels a line's name stands for depends on
/// all four.
pub const JOINT: &[&[&str]] = &[
    &[
        "npc-gifts",
        "met-zelda",
        "gerudo-fortress",
        "cows",
        "king-zora-moved",
    ],
    &[
        "progressive-items",
        "shuffle-nut-bag",
        "shuffle-stick-bag",
        "magic-with-first-item",
    ],
];

/// Choice options the world leaves out, each as setting id, option and
/// a sentence of the setting's description that offers it. Dungeon keys
/// offer Vanilla and Start only, so far.
const HELD: &[(&str, &str, &str)] = &[
    // AP's own `random` in a YAML rolls the age, before the fill needs it.
    (
        "starting-age",
        "random",
        " Random rolls one from the seed, and opens the door and the forest whichever it rolls",
    ),
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
    fn a_toggle_left_out_but_on_in_every_seed_is_forced() {
        let defs = crate::settings::schema();
        use super::{AlwaysOn, CHECK_KINDS, Later, Never, OnUntil};
        let ap = |id: &str| defs.iter().find(|d| d.id == id).unwrap().archipelago;
        assert_eq!(ap(crate::settings::CHESTS), OnUntil(CHECK_KINDS));
        assert!(matches!(ap("shuffle-items"), AlwaysOn(_)));
        assert!(matches!(ap("start-inventory"), Never(_)));
        assert!(matches!(ap("shuffle-dungeon-entrances"), Later(_)));
    }

    #[test]
    fn the_world_version_is_the_app_version() {
        let manifest = data_dir().join("../archipelago.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
        assert_eq!(manifest["world_version"], env!("CARGO_PKG_VERSION"));
    }
}
