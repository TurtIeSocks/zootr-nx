//! Every seed the app generated, kept in `history.json` beside the config.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::generate::{Extras, Report, Spot};
use crate::icons::icons;
use crate::settings::{self, SHUFFLE_ITEMS, Settings};
use crate::shuffle::Fixed;
use crate::{places, share};
use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

/// Oldest entries past this are dropped.
const KEEP: usize = 100;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub installed: Option<u32>,
    /// When `installed` went in, in Unix seconds. A save older than this
    /// was played on another seed. Histories from before it was kept have
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<u32>,
    /// Oldest first.
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u32,
    /// Unix seconds.
    pub time: u32,
    pub settings: Settings,
    #[serde(default, skip_serializing_if = "Extras::is_empty")]
    pub extras: Extras,
    #[serde(flatten)]
    pub label: Label,
    pub report: Report,
    /// The player's marks on the check tracker, by spot.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub marks: BTreeMap<u64, Mark>,
}

/// A check the player set aside, or a note on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind")]
pub enum Mark {
    Skipped,
    Note { text: String },
}

/// What a seed is for, in the player's words. Both may be blank.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Label {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// An entry without its spots, for lists.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Summary {
    pub id: u32,
    time: u32,
    seed: String,
    /// Stand-ins for the seed, settings and extras together.
    icons: Vec<String>,
    /// The seed, settings and extras as one line to share.
    share: String,
    settings: Settings,
    label: Label,
    chests: u32,
    changed: u32,
    /// Built with a test setup: a spawn, items, flags or quest steps.
    test: bool,
    /// Built with a plando: items pinned to checks.
    plando: bool,
    /// The slot, for a seed opened from an Archipelago `.apznx`.
    ap_slot: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct View {
    installed: Option<Summary>,
    /// Newest first.
    entries: Vec<Summary>,
}

impl From<&Entry> for Summary {
    fn from(e: &Entry) -> Self {
        Self {
            id: e.id,
            time: e.time,
            seed: e.report.seed.to_string(),
            icons: icons(e.report.seed, &e.settings, &e.extras),
            share: share::encode(e.report.seed, &e.settings, &e.extras),
            settings: e.settings.clone(),
            label: e.label.clone(),
            chests: e.report.spots.len() as u32,
            changed: e.report.changed() as u32,
            test: !Extras {
                place: Fixed::new(),
                ap: None,
                ..e.extras.clone()
            }
            .is_empty(),
            plando: !e.extras.place.is_empty(),
            ap_slot: e.extras.ap.as_ref().map(|a| a.slot.clone()),
        }
    }
}

fn path() -> Result<PathBuf> {
    Ok(Config::path()?.with_file_name("history.json"))
}

impl History {
    pub fn load() -> Result<Self> {
        let path = path()?;
        match std::fs::read(&path) {
            Ok(b) => Self::parse(&b).with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Saved history, brought up to date: readable chest names, older
    /// settings, and the old `vanilla` extra as Shuffle items off.
    fn parse(json: &[u8]) -> Result<Self> {
        let mut history: Self = serde_json::from_slice(json)?;
        for e in &mut history.entries {
            e.report.spots.iter_mut().for_each(Spot::fill_names);
            settings::upgrade(&mut e.settings);
            if std::mem::take(&mut e.extras.vanilla) {
                e.settings.insert(SHUFFLE_ITEMS.into(), false.into());
            }
        }
        Ok(history)
    }

    pub fn save(&self) -> Result<()> {
        let path = path()?;
        std::fs::create_dir_all(path.parent().context("history path has no folder")?)?;
        std::fs::write(&path, serde_json::to_vec(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }

    pub fn get(&self, id: u32) -> Result<&Entry> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .with_context(|| format!("no history entry {id}"))
    }

    /// Record a seed in `history.json`, as the installed one if `installed`.
    pub fn record(
        settings: Settings,
        extras: Extras,
        label: Label,
        report: Report,
        installed: bool,
    ) -> Result<Summary> {
        let mut history = Self::load()?;
        let entry = history.add(settings, extras, label, report);
        let (id, summary) = (entry.id, Summary::from(entry));
        if installed {
            history.install(Some(id));
        }
        history.save()?;
        Ok(summary)
    }

    /// Makes `id` the installed seed, or none for vanilla. Installing the
    /// seed already in keeps its time, so the saves played on it still
    /// count.
    pub fn install(&mut self, id: Option<u32>) {
        if id != self.installed {
            self.installed_at = id.map(|_| now());
        }
        self.installed = id;
    }

    /// Record a seed. Installing it is up to the caller.
    pub fn add(
        &mut self,
        settings: Settings,
        extras: Extras,
        label: Label,
        report: Report,
    ) -> &Entry {
        let id = self.entries.last().map_or(1, |e| e.id + 1);
        self.entries.push(Entry {
            id,
            time: now(),
            settings,
            extras,
            label,
            report,
            marks: BTreeMap::new(),
        });
        let excess = self.entries.len().saturating_sub(KEEP);
        self.entries.drain(..excess);
        self.entries.last().expect("just pushed")
    }

    fn get_mut(&mut self, id: u32) -> Result<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|e| e.id == id)
            .with_context(|| format!("no history entry {id}"))
    }

    pub fn relabel(&mut self, id: u32, label: Label) -> Result<()> {
        self.get_mut(id)?.label = label;
        Ok(())
    }

    /// Mark a spot, or clear its mark with `None`.
    /// Places for a seed saved before they were kept.
    pub fn place(&mut self, id: u32, romfs: &Romfs) -> Result<()> {
        places::fill(romfs, &mut self.get_mut(id)?.report.spots)
    }

    pub fn mark(&mut self, id: u32, spot: u64, mark: Option<Mark>) -> Result<()> {
        let entry = self.get_mut(id)?;
        match mark {
            Some(m) => entry.marks.insert(spot, m),
            None => entry.marks.remove(&spot),
        };
        Ok(())
    }

    /// Forget one seed. Not the installed one: its mod is still in place.
    pub fn remove(&mut self, id: u32) -> Result<()> {
        ensure!(
            self.installed != Some(id),
            "The installed seed can't be deleted"
        );
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        ensure!(self.entries.len() < before, "no history entry {id}");
        Ok(())
    }

    /// Forget every seed but the installed one.
    pub fn clear(&mut self) {
        let installed = self.installed;
        self.entries.retain(|e| Some(e.id) == installed);
    }

    pub fn view(&self) -> View {
        let installed = self.installed.and_then(|id| self.get(id).ok());
        View {
            installed: installed.map(Summary::from),
            entries: self.entries.iter().rev().map(Summary::from).collect(),
        }
    }
}

/// Now, in Unix seconds.
fn now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32)
}

#[cfg(test)]
mod tests {
    #[test]
    fn installing_another_seed_restarts_its_clock() {
        let mut history = History::default();
        history.install(Some(1));
        let earlier = history.installed_at.unwrap() - 100;
        history.installed_at = Some(earlier);
        // The same seed again keeps its saves.
        history.install(Some(1));
        assert_eq!(history.installed_at, Some(earlier));
        history.install(Some(2));
        assert!(history.installed_at.unwrap() > earlier);
        history.install(None);
        assert_eq!((history.installed, history.installed_at), (None, None));
        // A history from before the time was kept reads with none.
        let old: History = serde_json::from_str(r#"{"installed": 3, "entries": []}"#).unwrap();
        assert_eq!(old.installed_at, None);
    }

    use super::*;
    use crate::generate::Written;
    use crate::known::{actor, item};

    fn report(seed: u64) -> Report {
        Report {
            seed,
            events: vec![],
            way: vec![],
            playthrough: vec![],
            unprovable: vec![],
            start: Default::default(),
            spots: vec![Spot {
                hash: 1,
                area: "Area".into(),
                holder: actor::OBJ_T_BOX_A_02.to_string(),
                vanilla: "Item_A".into(),
                item: "Item_B".into(),
                ..Default::default()
            }],
            written: Written {
                defaults: vec![],
                actors: 1,
                files: 2,
            },
        }
    }

    #[test]
    fn a_plando_badges_apart_from_a_test_setup() {
        let flags = |extras: Extras| {
            let mut history = History::default();
            history.add(Settings::new(), extras, Label::default(), report(1));
            let s = Summary::from(&history.entries[0]);
            (s.test, s.plando)
        };
        let place = Fixed::from([(1, item::HOOKSHOT.to_string())]);
        let give = vec![item::BOMB.to_string()];
        assert_eq!(flags(Extras::default()), (false, false));
        let plando = Extras {
            place: place.clone(),
            ..Default::default()
        };
        assert_eq!(flags(plando), (false, true));
        let both = Extras {
            place,
            give: give.clone(),
            ..Default::default()
        };
        assert_eq!(flags(both), (true, true));
        assert_eq!(
            flags(Extras {
                give,
                ..Default::default()
            }),
            (true, false)
        );
    }

    #[test]
    fn an_ap_seed_carries_its_slot_and_isnt_a_test() {
        let mut history = History::default();
        let ap = crate::ap::ApSeed {
            world_version: String::new(),
            settings: Settings::new(),
            seed: 1,
            slot: "FakeSlot".into(),
            player: 1,
            start: vec![],
            placements: BTreeMap::new(),
        };
        history.add(Settings::new(), Extras::ap(ap), Label::default(), report(1));
        let s = Summary::from(&history.entries[0]);
        assert_eq!((s.test, s.ap_slot.as_deref()), (false, Some("FakeSlot")));
    }

    #[test]
    fn round_trips_and_tracks_the_installed_seed() {
        let mut history = History::default();
        history.add(
            Settings::new(),
            Extras::default(),
            Label::default(),
            report(u64::MAX),
        );
        let extras = Extras {
            defaults: vec!["A=1".into()],
            ..Default::default()
        };
        let second = history
            .add(
                Settings::from([("warp-kit".into(), true.into())]),
                extras.clone(),
                Label {
                    name: "Gifts".into(),
                    note: String::new(),
                },
                report(7),
            )
            .id;
        assert_eq!(history.get(second).unwrap().extras, extras);
        assert_eq!(history.installed, None);
        history.installed = Some(second);
        let json = serde_json::to_vec(&history).unwrap();
        assert_eq!(serde_json::from_slice::<History>(&json).unwrap(), history);
        let view = history.view();
        assert_eq!(view.entries[0].seed, "7");
        assert_eq!(view.installed.unwrap().changed, 1);
        assert_eq!(view.entries[0].label.name, "Gifts");
        assert_eq!(view.entries[1].label, Label::default());
        assert_eq!((view.entries[0].test, view.entries[1].test), (true, false));
    }

    #[test]
    fn marks_are_kept_per_spot_and_cleared() {
        let mut history = History::default();
        history.add(
            Settings::new(),
            Extras::default(),
            Label::default(),
            report(7),
        );
        let json = serde_json::to_value(&history).unwrap();
        assert!(json["entries"][0].get("marks").is_none());
        let note = Mark::Note {
            text: "later".into(),
        };
        history.mark(1, 1, Some(note.clone())).unwrap();
        history.mark(1, 2, Some(Mark::Skipped)).unwrap();
        history.mark(1, 2, None).unwrap();
        assert!(history.mark(2, 1, None).is_err());
        let back = History::parse(&serde_json::to_vec(&history).unwrap()).unwrap();
        assert_eq!(back.entries[0].marks, BTreeMap::from([(1, note)]));
    }

    #[test]
    fn a_label_is_flat_optional_and_editable() {
        let mut history = History::default();
        history.add(
            Settings::new(),
            Extras::default(),
            Label::default(),
            report(7),
        );
        let json = serde_json::to_value(&history).unwrap();
        assert!(json["entries"][0].get("name").is_none());
        let label = Label {
            name: "Saria's Hookshot".into(),
            note: "Q31".into(),
        };
        history.relabel(1, label.clone()).unwrap();
        assert!(history.relabel(2, label.clone()).is_err());
        let json = serde_json::to_value(&history).unwrap();
        assert_eq!(json["entries"][0]["note"], "Q31");
        let back: History = serde_json::from_value(json).unwrap();
        assert_eq!(back.entries[0].label, label);
    }

    #[test]
    fn deleting_spares_the_installed_seed() {
        let mut history = History::default();
        for seed in 0..4 {
            history.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            );
        }
        history.installed = Some(2);
        assert!(history.remove(2).is_err());
        assert!(history.remove(9).is_err());
        history.remove(1).unwrap();
        let ids = |h: &History| h.entries.iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(&history), [2, 3, 4]);
        history.clear();
        assert_eq!(ids(&history), [2]);
    }

    #[test]
    fn keeps_the_newest_entries() {
        let mut history = History::default();
        for seed in 0..KEEP as u64 + 5 {
            history.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            );
        }
        assert_eq!(history.entries.len(), KEEP);
        assert_eq!(history.entries[0].report.seed, 5);
    }

    #[test]
    fn the_old_vanilla_extra_becomes_a_setting() {
        let mut history = History::default();
        history.add(
            Settings::new(),
            Extras::default(),
            Label::default(),
            report(7),
        );
        let mut json = serde_json::to_value(&history).unwrap();
        json["entries"][0]["extras"] = serde_json::json!({ "vanilla": true });
        let e = &History::parse(json.to_string().as_bytes()).unwrap().entries[0];
        assert_eq!(e.settings[SHUFFLE_ITEMS], false);
        assert!(e.extras.is_empty());
    }
}
