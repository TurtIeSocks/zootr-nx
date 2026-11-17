//! A seed from Archipelago: the `.apznx` the `znx` world writes, read into
//! what a normal seed is made of.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

use super::GAME;
use super::ids::Ids;
use super::seed::{Dealt, base};
use crate::foreign::{self, Foreign};
use crate::known::{Name, item};
use crate::settings::Settings;
use crate::shuffle::Fixed;

/// The item another game's item shows until models by class arrive: one
/// Link gets once, so it reads as a get of its own.
const STAND_IN: &Name = item::TOKEN_GERUDO;

/// The ids the world was exported with.
const IDS: &str = include_str!("../../../../ap/znx/data/ids.json");

/// A `.apznx`, as kept with a seed so it reinstalls without the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApSeed {
    /// The version of the world that wrote it, the app's that reads it.
    pub world_version: String,
    /// The exposed options' values, by setting id.
    pub settings: Settings,
    /// For every roll that isn't a placement.
    pub seed: u64,
    pub slot: String,
    pub player: u32,
    /// The start inventory, as our AP item ids.
    #[serde(default)]
    pub start: Vec<u64>,
    /// By check hash.
    pub placements: BTreeMap<u64, Placed>,
}

/// What AP put at one check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placed {
    /// One of ours, by AP item id.
    Own(u64),
    Foreign(ForeignItem),
}

/// Another player's item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForeignItem {
    pub player: u32,
    pub player_name: String,
    pub game: String,
    pub item: String,
    /// AP's class: progression, useful, filler or trap.
    pub class: String,
    /// Our AP item id, when the game is ours.
    #[serde(default)]
    pub id: Option<u64>,
}

/// An [`ApSeed`] as the generator takes it.
pub struct Resolved {
    pub fixed: Fixed,
    pub foreign: Vec<Foreign>,
    /// The start inventory, by actor.
    pub give: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    game: String,
    world_version: String,
}

/// `name`'s JSON in `files`.
fn json<T: serde::de::DeserializeOwned>(
    files: &std::collections::HashMap<String, Vec<u8>>,
    name: &str,
) -> Result<T> {
    let data = files
        .get(name)
        .with_context(|| format!("the seed has no {name}"))?;
    serde_json::from_slice(data).with_context(|| format!("reading {name}"))
}

impl ApSeed {
    /// The `.apznx` at `path`. Refuses one from another version, whose
    /// checks and ids may differ from this one's.
    pub fn read(path: &Path) -> Result<Self> {
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let seed = Self::from_zip(&data).with_context(|| format!("reading {}", path.display()))?;
        let app = env!("CARGO_PKG_VERSION");
        ensure!(
            seed.world_version == app,
            "a seed from the {} world, and this is zootr-nx {app}: open it with the same version",
            seed.world_version
        );
        Ok(seed)
    }

    fn from_zip(data: &[u8]) -> Result<Self> {
        let files = super::zip::files(data)?;
        let manifest: Manifest = json(&files, "archipelago.json")?;
        Self::check_manifest(&manifest)?;
        #[derive(Deserialize)]
        struct SeedFile {
            settings: Settings,
            seed: u64,
            slot: String,
            player: u32,
            #[serde(default)]
            start: Vec<u64>,
        }
        let seed: SeedFile = json(&files, "seed.json")?;
        Ok(Self {
            world_version: manifest.world_version,
            settings: seed.settings,
            seed: seed.seed,
            slot: seed.slot,
            player: seed.player,
            start: seed.start,
            placements: json(&files, "placements.json")?,
        })
    }

    fn check_manifest(manifest: &Manifest) -> Result<()> {
        ensure!(
            manifest.game == GAME,
            "a seed for {}, not {GAME}",
            manifest.game
        );
        Ok(())
    }

    /// The seed's settings: the export's, with the file's options over them.
    pub fn full_settings(&self) -> Settings {
        let mut settings = base();
        settings.extend(self.settings.clone());
        settings
    }

    /// The pins, copies and start inventory, checked against the deal: AP
    /// must have placed every check the deal shuffles, and no other.
    pub fn resolve(&self, romfs: &Romfs) -> Result<Resolved> {
        let dealt = Dealt::new(romfs, &self.full_settings())?;
        let placed: BTreeSet<u64> = self.placements.keys().copied().collect();
        let missing: Vec<_> = dealt.free.difference(&placed).collect();
        let extra: Vec<_> = placed.difference(&dealt.free).collect();
        ensure!(
            missing.is_empty() && extra.is_empty(),
            "the seed doesn't match this version's checks: none placed at {missing:?}, unknown {extra:?}"
        );
        let ids: Ids = serde_json::from_str(IDS)?;
        let actors = ids.actors(dealt.pool.iter().map(String::as_str));
        let actor = |id: u64| match actors.get(&id) {
            Some(a) => Ok(a.to_string()),
            None => bail!("no item has AP id {id}"),
        };

        let mut fixed = Fixed::new();
        let mut copies: BTreeMap<(String, String, String), String> = BTreeMap::new();
        for (&hash, placed) in &self.placements {
            let item = match placed {
                Placed::Own(id) => actor(*id)?,
                Placed::Foreign(f) => {
                    let shows = match (f.game == GAME, f.id) {
                        (true, Some(id)) => actor(id)?,
                        _ => STAND_IN.get().to_owned(),
                    };
                    let key = (shows, f.item.clone(), f.player_name.clone());
                    let next = foreign::actor(copies.len() + 1);
                    copies.entry(key).or_insert(next).clone()
                }
            };
            fixed.insert(hash, item);
        }
        let foreign = copies
            .into_iter()
            .map(|((shows, item, player), actor)| Foreign {
                actor,
                shows,
                item,
                player,
            })
            .collect();
        let give = self
            .start
            .iter()
            .map(|&id| actor(id))
            .collect::<Result<_>>()?;
        Ok(Resolved {
            fixed,
            foreign,
            give,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::generate::{Extras, generate};
    use crate::progress::Run;

    /// Slot 1 of a two-slot znx multiworld from AP's `Generate.py`.
    const TWO_SLOTS: &[u8] = include_bytes!("testdata/two_slots.apznx");

    #[test]
    fn a_seed_from_ap_reads_its_settings_placements_and_players() {
        let seed = ApSeed::from_zip(TWO_SLOTS).unwrap();
        assert_eq!((seed.slot.as_str(), seed.player), ("Link", 1));
        assert_eq!(seed.settings.get("open-forest"), Some(&true.into()));
        let foreign: Vec<_> = (seed.placements.values())
            .filter_map(|p| match p {
                Placed::Foreign(f) => Some(f),
                Placed::Own(_) => None,
            })
            .collect();
        assert!(!foreign.is_empty());
        assert!(
            foreign
                .iter()
                .all(|f| f.player_name == "Zelda" && f.id.is_some())
        );
        assert!(
            seed.placements
                .values()
                .any(|p| matches!(p, Placed::Own(_)))
        );
    }

    #[test]
    fn a_seed_of_another_game_is_refused() {
        let manifest = Manifest {
            game: "Ocarina of Time".into(),
            world_version: env!("CARGO_PKG_VERSION").into(),
        };
        let refused = ApSeed::check_manifest(&manifest).unwrap_err().to_string();
        assert!(refused.contains("not Ocarina of Time (znx)"), "{refused}");
    }

    #[test]
    fn a_seed_resolves_to_a_pin_for_every_free_check_and_a_copy_per_item_and_player() {
        let Some(romfs) = dev_dump() else { return };
        let seed = ApSeed::from_zip(TWO_SLOTS).unwrap();
        let resolved = seed.resolve(&romfs).unwrap();
        assert!(resolved.fixed.keys().eq(seed.placements.keys()));
        let copies: BTreeSet<_> = (resolved.fixed.values())
            .filter(|i| foreign::is_foreign(i))
            .collect();
        assert_eq!(copies.len(), resolved.foreign.len());
        assert!(resolved.foreign.iter().all(|f| f.player == "Zelda"));
        // Zelda's items are ours, so each copy shows the item itself.
        assert!(resolved.foreign.iter().all(|f| f.shows != STAND_IN.get()));
    }

    /// What `extras` installs from `seed` with the fixture's settings.
    fn installed(romfs: &Romfs, seed: u64, extras: &Extras) -> crate::generate::Report {
        let settings = ApSeed::from_zip(TWO_SLOTS).unwrap().full_settings();
        let dir = std::env::temp_dir().join(format!("zootr-ap-{}-{seed}", std::process::id()));
        let report = generate(romfs, &dir, seed, &settings, extras, &Run::silent()).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        report
    }

    #[test]
    fn a_seed_from_ap_always_installs_the_same_mod_with_its_items_for_others() {
        let Some(romfs) = dev_dump() else { return };
        let ap = ApSeed::from_zip(TWO_SLOTS).unwrap();
        let extras = Extras::ap(ap.clone());
        let report = installed(&romfs, ap.seed, &extras);
        assert_eq!(report, installed(&romfs, ap.seed, &extras));
        let others = (report.spots.iter())
            .filter(|s| foreign::is_foreign(&s.item))
            .count();
        let foreign = (ap.placements.values())
            .filter(|p| matches!(p, Placed::Foreign(_)))
            .count();
        assert_eq!(others, foreign);
        assert!(
            report
                .spots
                .iter()
                .any(|s| s.item_name.ends_with(" for Zelda"))
        );
    }

    #[test]
    fn a_seed_from_ap_of_our_own_items_installs_what_the_shuffle_dealt() {
        let Some(romfs) = dev_dump() else { return };
        let shuffled = installed(&romfs, 7, &Extras::default());
        let ids: Ids = serde_json::from_str(IDS).unwrap();
        let mut ap = ApSeed::from_zip(TWO_SLOTS).unwrap();
        let free = Dealt::new(&romfs, &ap.full_settings()).unwrap().free;
        ap.placements = (shuffled.spots.iter())
            .filter(|s| free.contains(&s.hash))
            .map(|s| (s.hash, Placed::Own(ids.item_id(&s.item).unwrap())))
            .collect();
        ap.start.clear();
        let from_ap = installed(&romfs, 7, &Extras::ap(ap));
        let items = |r: &crate::generate::Report| {
            (r.spots.iter())
                .map(|s| (s.hash, s.item.clone()))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(items(&from_ap), items(&shuffled));
    }

    #[test]
    fn a_seed_missing_a_check_is_refused() {
        let Some(romfs) = dev_dump() else { return };
        let mut seed = ApSeed::from_zip(TWO_SLOTS).unwrap();
        seed.placements.pop_first();
        let refused = seed.resolve(&romfs).err().unwrap().to_string();
        assert!(refused.contains("none placed at"), "{refused}");
    }
}
