//! Every seed the app generated, kept in the `history` folder beside the
//! config ([`store`]).

mod store;
mod view;

pub use view::{Summary, View, view};

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{Config, Target};
use crate::game::Release;
use crate::generate::{Extras, Report};
use crate::places;
use crate::settings::{self, SHUFFLE_ITEMS, Settings, SettingsVersion};
use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

/// Oldest entries past this are dropped. Starred ones count toward it.
const KEEP: usize = 1000;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    /// The machine's own game's installed seed. Read any game's with
    /// [`History::installed`].
    installed: Option<u32>,
    /// When `installed` went in, in Unix seconds. A save older than this
    /// was played on another seed. Histories from before it was kept have
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    installed_at: Option<u32>,
    /// The slots' installed seeds, by game: a machine with several games
    /// installs a seed in each.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    slots: BTreeMap<String, Installed>,
    /// The slot this shell plays ([`crate::turn::slot`]), whose installed
    /// seed [`History::installed`] and [`History::install`] mean. `None`
    /// is the machine's own game.
    #[serde(skip)]
    game: Option<String>,
    /// Oldest first.
    pub entries: Vec<Entry>,
    /// Which defaults the entries' settings were saved under. Loading
    /// brings them up to date.
    #[serde(default = "SettingsVersion::unwritten")]
    settings_version: SettingsVersion,
    /// The folder it was loaded from and saves to. `None` for a history
    /// only in memory, which can't be saved.
    #[serde(skip)]
    dir: Option<PathBuf>,
    /// Reports not in their files yet: new seeds', and ones filled in
    /// since loading. Saving writes them.
    #[serde(skip)]
    unsaved: BTreeMap<u32, Report>,
}

/// What lists and trimming read of a seed's report, kept in the index so
/// they never open the report's file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    pub seed: u64,
    /// How many spots it has.
    pub chests: u32,
    /// How many of them hold another item than the vanilla game's.
    pub changed: u32,
    /// The build of the game it was made for.
    pub release: Release,
}

impl From<&Report> for Brief {
    fn from(r: &Report) -> Self {
        Self {
            seed: r.seed,
            chests: r.spots.len() as u32,
            changed: r.changed() as u32,
            release: r.written.release,
        }
    }
}

/// A game's installed seed and when it went in, as [`History`] keeps them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Installed {
    pub id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<u32>,
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
    /// Its report's head. The whole report is [`History::report`].
    pub brief: Brief,
    /// The player's marks on the check tracker, by spot.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub marks: BTreeMap<u64, Mark>,
    /// The player's guesses at shuffled doors: door id to dungeon id, as
    /// the report's entrances name them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub picks: BTreeMap<String, String>,
    /// The player keeps it: trimming and Clear pass it by.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starred: bool,
}

impl Entry {
    /// `#id`, and its name when it has one.
    fn called(&self) -> String {
        match self.label.name.as_str() {
            "" => format!("#{}", self.id),
            name => format!("#{} \"{name}\"", self.id),
        }
    }
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

impl History {
    pub fn load() -> Result<Self> {
        Ok(Self {
            game: crate::turn::slot(&Config::load_all()?.turn),
            ..Self::load_from(store::dir()?)?
        })
    }

    /// The history in `dir`, an old `history.json` beside it moved in
    /// first.
    fn load_from(dir: PathBuf) -> Result<Self> {
        store::move_old(&dir)?;
        let path = store::index(&dir);
        let history = match std::fs::read(&path) {
            Ok(b) => Self::parse(&b).with_context(|| format!("reading {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        Ok(Self {
            dir: Some(dir),
            ..history
        })
    }

    /// Saved history, brought up to date: older settings, the old
    /// `vanilla` extra as Shuffle items off, and the build each seed was
    /// made for kept in its extras ([`History::add`]). Reports catch up
    /// as they're read ([`store::read_report`]).
    fn parse(json: &[u8]) -> Result<Self> {
        let mut history: Self = serde_json::from_slice(json)?;
        let from = std::mem::take(&mut history.settings_version);
        for e in &mut history.entries {
            e.extras.release.get_or_insert(e.brief.release);
            settings::upgrade(&mut e.settings, from);
            if std::mem::take(&mut e.extras.vanilla) {
                e.settings.insert(SHUFFLE_ITEMS.into(), false.into());
            }
        }
        Ok(history)
    }

    /// Writes the new reports, then the index, then removes the reports
    /// of seeds gone from it: an index never names a missing report.
    pub fn save(&self) -> Result<()> {
        let dir = (self.dir.as_deref()).context("a history only in memory can't be saved")?;
        let ids: Vec<u32> = self.entries.iter().map(|e| e.id).collect();
        for (id, report) in self.unsaved.iter().filter(|(id, _)| ids.contains(id)) {
            store::write_report(dir, *id, report)?;
        }
        store::write_index(dir, &serde_json::to_vec(self)?)?;
        store::sweep(dir, &ids)
    }

    /// Seed `id`'s whole report, read from its file.
    pub fn report(&self, id: u32) -> Result<Report> {
        self.get(id)?;
        if let Some(report) = self.unsaved.get(&id) {
            return Ok(report.clone());
        }
        let dir = (self.dir.as_deref()).with_context(|| format!("seed {id} has no report"))?;
        store::read_report(dir, id)
    }

    pub fn get(&self, id: u32) -> Result<&Entry> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .with_context(|| format!("no history entry {id}"))
    }

    /// Record a seed in the history, as the installed one if `installed`.
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

    /// The installed seed of the game this shell plays.
    pub fn installed(&self) -> Installed {
        match &self.game {
            None => Installed {
                id: self.installed,
                at: self.installed_at,
            },
            Some(g) => self.slots.get(g).copied().unwrap_or_default(),
        }
    }

    /// Every game's installed seed, which trimming and deleting spare: its
    /// mod is still in place.
    fn in_place(&self) -> impl Iterator<Item = u32> + '_ {
        let slots = self.slots.values().filter_map(|i| i.id);
        self.installed.into_iter().chain(slots)
    }

    /// The installed seed's id, an error when none is installed.
    fn installed_id(&self) -> Result<u32> {
        self.installed().id.context("no seed is installed")
    }

    /// The installed seed's entry, an error when none is installed.
    pub fn installed_entry(&self) -> Result<&Entry> {
        self.get(self.installed_id()?)
    }

    /// The installed seed's report, an error when none is installed.
    pub fn installed_report(&self) -> Result<Report> {
        self.report(self.installed_id()?)
    }

    /// Makes `id` the installed seed, or none for vanilla. Installing the
    /// seed already in keeps its time, so the saves played on it still
    /// count.
    pub fn install(&mut self, id: Option<u32>) {
        let was = self.installed();
        let at = if id == was.id {
            was.at
        } else {
            id.map(|_| now())
        };
        match self.game.clone() {
            None => (self.installed, self.installed_at) = (id, at),
            Some(g) if id.is_none() => {
                self.slots.remove(&g);
            }
            Some(g) => {
                self.slots.insert(g, Installed { id, at });
            }
        }
    }

    /// Record a seed. Installing it is up to the caller. Its extras keep
    /// the build it was made for, so a reinstall rebuilds that build
    /// whichever target is picked by then.
    pub fn add(
        &mut self,
        settings: Settings,
        extras: Extras,
        label: Label,
        report: Report,
    ) -> &Entry {
        let id = self.entries.last().map_or(1, |e| e.id + 1);
        let extras = Extras {
            release: Some(report.written.release),
            ..extras
        };
        self.entries.push(Entry {
            id,
            time: now(),
            settings,
            extras,
            label,
            brief: Brief::from(&report),
            marks: BTreeMap::new(),
            picks: BTreeMap::new(),
            starred: false,
        });
        self.unsaved.insert(id, report);
        // The oldest go first, but never a spared one or this one.
        let kept: Vec<u32> = self.spared().chain([id]).collect();
        let mut excess = self.entries.len().saturating_sub(KEEP);
        self.entries.retain(|e| {
            let drop = excess > 0 && !kept.contains(&e.id);
            excess -= usize::from(drop);
            !drop
        });
        self.entries.last().expect("just pushed")
    }

    fn get_mut(&mut self, id: u32) -> Result<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|e| e.id == id)
            .with_context(|| format!("no history entry {id}"))
    }

    /// The colors seed `id` installs with from now on.
    /// Entry `id` in other cosmetics: its colors and music.
    pub fn restyle(
        &mut self,
        id: u32,
        colors: crate::colors::Colors,
        audio: crate::audio::Audio,
        drafts: crate::generate::Drafts,
    ) -> Result<()> {
        let extras = &mut self.get_mut(id)?.extras;
        (extras.colors, extras.audio, extras.drafts) = (colors, audio, drafts);
        Ok(())
    }

    pub fn relabel(&mut self, id: u32, label: Label) -> Result<()> {
        self.get_mut(id)?.label = label;
        Ok(())
    }

    /// Places for a seed saved before they were kept.
    pub fn place(&mut self, id: u32, romfs: &Romfs) -> Result<()> {
        let mut report = self.report(id)?;
        places::fill(romfs, &mut report.spots)?;
        self.unsaved.insert(id, report);
        Ok(())
    }

    /// Mark a spot, or clear its mark with `None`.
    pub fn mark(&mut self, id: u32, spot: u64, mark: Option<Mark>) -> Result<()> {
        let entry = self.get_mut(id)?;
        match mark {
            Some(m) => entry.marks.insert(spot, m),
            None => entry.marks.remove(&spot),
        };
        Ok(())
    }

    /// Guess the dungeon behind a seed's shuffled door, or clear the guess
    /// with `None`. Both are ids from the seed's entrances.
    pub fn pick(&mut self, id: u32, door: &str, dungeon: Option<&str>) -> Result<()> {
        let entrances = self.report(id)?.entrances;
        let known = |i: &str| entrances.iter().any(|d| d.id == i);
        ensure!(known(door), "{door} isn't a shuffled door of seed {id}");
        if let Some(d) = dungeon {
            ensure!(known(d), "{d} isn't a shuffled dungeon of seed {id}");
        }
        let entry = self.get_mut(id)?;
        match dungeon {
            Some(d) => entry.picks.insert(door.into(), d.into()),
            None => entry.picks.remove(door),
        };
        Ok(())
    }

    /// Forget one seed. Not an installed one, any game's.
    /// Forget the seeds `ids`, starred ones too, but not an installed one
    /// (any game's): how many it kept for being installed.
    pub fn remove(&mut self, ids: &[u32]) -> Result<usize> {
        for &id in ids {
            self.get(id)?;
        }
        let installed: Vec<u32> = self.in_place().collect();
        let kept = ids.iter().filter(|id| installed.contains(id)).count();
        self.entries
            .retain(|e| !ids.contains(&e.id) || installed.contains(&e.id));
        Ok(kept)
    }

    /// Star the seeds `ids`, or unstar them. Refused whole when it would
    /// star more than [`KEEP`].
    pub fn star(&mut self, ids: &[u32], starred: bool) -> Result<()> {
        for &id in ids {
            self.get(id)?;
        }
        let after = (self.entries.iter())
            .filter(|e| {
                if ids.contains(&e.id) {
                    starred
                } else {
                    e.starred
                }
            })
            .count();
        ensure!(
            !starred || after <= KEEP,
            "History keeps {KEEP} seeds: unstar one first"
        );
        (self.entries.iter_mut())
            .filter(|e| ids.contains(&e.id))
            .for_each(|e| e.starred = starred);
        Ok(())
    }

    /// The seeds trimming and Clear pass by: every game's installed one,
    /// and the starred ones.
    fn spared(&self) -> impl Iterator<Item = u32> + '_ {
        let starred = self.entries.iter().filter(|e| e.starred).map(|e| e.id);
        self.in_place().chain(starred)
    }

    /// Forget every seed but the spared ones ([`History::spared`]).
    pub fn clear(&mut self) {
        let kept: Vec<u32> = self.spared().collect();
        self.entries.retain(|e| kept.contains(&e.id));
    }

    /// What to warn when `target`'s mod folder holds another seed than the
    /// installed one, as when an install elsewhere wrote over it without
    /// telling this history. Names that seed when the history has it. A
    /// target running another build than the seed's was skipped by its
    /// install, so it keeps whatever it held.
    pub fn foreign_mod(&self, target: &Target) -> Option<String> {
        let installed = self.installed_entry().ok()?;
        if target.skips(installed.brief.release).is_some() {
            return None;
        }
        let spoiler =
            std::fs::read_to_string(target.mod_dir.join(crate::generate::SPOILER)).ok()?;
        let seed = Report::spoiler_seed(&spoiler)?;
        if seed == installed.brief.seed {
            return None;
        }
        let there = (self.entries.iter().rev())
            .find(|e| e.brief.seed == seed)
            .map_or("a seed from elsewhere".into(), Entry::called);
        Some(format!(
            "The mod folder holds {there}, not {}. Install it again before playing.",
            installed.called()
        ))
    }
}

/// Now, in Unix seconds.
fn now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32)
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn each_game_keeps_its_own_installed_seed() {
        let reload = |h: &History, game: Option<&str>| History {
            game: game.map(Into::into),
            ..History::parse(&serde_json::to_vec(h).unwrap()).unwrap()
        };
        let mut own = History::default();
        for seed in 1..=3 {
            own.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            );
        }
        own.install(Some(1));
        let mut two = reload(&own, Some("FakeHost-2"));
        assert_eq!(two.installed(), Installed::default());
        two.install(Some(2));
        assert_eq!(reload(&two, None).installed().id, Some(1));
        assert_eq!(reload(&two, Some("FakeHost-2")).installed().id, Some(2));
        assert_eq!(reload(&two, Some("FakeHost-3")).installed().id, None);
        // Every game's installed seed is spared, whichever game asks.
        let mut own = reload(&two, None);
        assert_eq!(own.remove(&[2]).unwrap(), 1, "kept, being installed");
        own.clear();
        assert_eq!(own.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [1, 2]);
        // Taking a slot's seed off leaves the machine's own.
        two.install(None);
        assert_eq!(
            reload(&two, Some("FakeHost-2")).installed(),
            Installed::default()
        );
        assert_eq!(reload(&two, None).installed().id, Some(1));
        // A history from before slots: its one seed is the machine's own.
        let old = br#"{"installed": 3, "installed_at": 5, "entries": []}"#;
        let old = History::parse(old).unwrap();
        assert_eq!(
            old.installed(),
            Installed {
                id: Some(3),
                at: Some(5)
            }
        );
        let slot = History {
            game: Some("FakeHost-2".into()),
            ..old
        };
        assert_eq!(slot.installed(), Installed::default());
    }

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
    use crate::generate::{Spot, Written};
    use crate::known::actor;

    pub(crate) fn report(seed: u64) -> Report {
        Report {
            seed,
            events: vec![],
            way: vec![],
            playthrough: vec![],
            unprovable: vec![],
            start: Default::default(),
            entrances: vec![],
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
                stand_ins: vec![],
                release: Default::default(),
            },
        }
    }

    #[test]
    fn a_mod_folder_holding_another_seed_is_named() {
        let mut history = History::default();
        let label = |name: &str| Label {
            name: name.into(),
            note: String::new(),
        };
        let ours = (history.add(Settings::new(), Extras::default(), label("Ice"), report(5))).id;
        history.add(
            Settings::new(),
            Extras::default(),
            label("Other"),
            report(9),
        );
        history.installed = Some(ours);
        let dir = std::env::temp_dir().join(format!("zootr-foreign-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = Target {
            mod_dir: dir.clone(),
            ..crate::config::tests::target("FakeTarget", None)
        };
        let spoiler = |seed: u64| {
            let text = report(seed).spoiler(&[]);
            std::fs::write(dir.join(crate::generate::SPOILER), text).unwrap();
        };
        spoiler(5);
        assert_eq!(
            history.foreign_mod(&target),
            None,
            "the installed seed's own"
        );
        spoiler(9);
        let warning = history.foreign_mod(&target).unwrap();
        assert!(
            warning.starts_with("The mod folder holds #2 \"Other\", not #1 \"Ice\""),
            "{warning}"
        );
        spoiler(3);
        assert!(
            history
                .foreign_mod(&target)
                .unwrap()
                .contains("a seed from elsewhere")
        );
        std::fs::remove_dir_all(&dir).unwrap();
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
                Settings::from([("kakariko-gate".into(), true.into())]),
                extras.clone(),
                Label {
                    name: "Gifts".into(),
                    note: String::new(),
                },
                report(7),
            )
            .id;
        let kept = Extras {
            release: Some(Release::V111),
            ..extras
        };
        assert_eq!(history.get(second).unwrap().extras, kept, "with its build");
        assert_eq!(history.installed, None);
        history.installed = Some(second);
        let dir = store::tests::temp("round-trip");
        history.dir = Some(dir.clone());
        history.save().unwrap();
        let back = History::load_from(dir).unwrap();
        assert_eq!(back.entries, history.entries);
        assert_eq!(back.installed, history.installed);
        assert_eq!(back.report(second).unwrap(), store::tests::filled(7));
        let view = history.view();
        assert_eq!(view.entries[0].seed, "7");
        assert_eq!(view.installed.unwrap().changed, 1);
        assert_eq!(view.entries[0].label.name, "Gifts");
        assert_eq!(view.entries[1].label, Label::default());
        assert_eq!((view.entries[0].test, view.entries[1].test), (true, false));
    }

    #[test]
    fn saving_writes_new_reports_but_not_a_removed_seeds() {
        let dir = store::tests::temp("save-reports");
        let mut history = History::load_from(dir.clone()).unwrap();
        for seed in [5, 6] {
            history.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            );
        }
        history.remove(&[1]).unwrap();
        history.save().unwrap();
        assert!(!dir.join("reports/1.json.zst").exists());
        assert!(dir.join("reports/2.json.zst").exists());
        let back = History::load_from(dir).unwrap();
        assert_eq!(back.report(2).unwrap().seed, 6);
        assert!(back.report(1).is_err(), "gone with its entry");
        assert!(History::default().save().is_err(), "only in memory");
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
    fn a_pick_names_a_shuffled_door_and_dungeon_and_clears() {
        let mut history = History::default();
        let door = crate::generate::Door {
            id: "deku-tree".into(),
            area: "FakeArea".into(),
            vanilla: "Deku Tree".into(),
            dungeon: "deku-tree".into(),
            dungeon_name: "Deku Tree".into(),
        };
        let report = Report {
            entrances: vec![door],
            ..report(7)
        };
        history.add(Settings::new(), Extras::default(), Label::default(), report);
        let json = serde_json::to_value(&history).unwrap();
        assert!(json["entries"][0].get("picks").is_none());
        assert!(history.pick(1, "fire-temple", Some("deku-tree")).is_err());
        assert!(history.pick(1, "deku-tree", Some("fire-temple")).is_err());
        history.pick(1, "deku-tree", Some("deku-tree")).unwrap();
        let back = History::parse(&serde_json::to_vec(&history).unwrap()).unwrap();
        let picked = BTreeMap::from([("deku-tree".to_string(), "deku-tree".to_string())]);
        assert_eq!(back.entries[0].picks, picked);
        history.pick(1, "deku-tree", None).unwrap();
        assert!(history.get(1).unwrap().picks.is_empty());
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
        assert!(history.remove(&[1, 9]).is_err(), "an unknown id");
        assert_eq!(history.entries.len(), 4, "refused whole");
        history.star(&[3], true).unwrap();
        assert_eq!(history.remove(&[1, 2]).unwrap(), 1);
        let ids = |h: &History| h.entries.iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(&history), [2, 3, 4]);
        history.clear();
        assert_eq!(ids(&history), [2, 3], "Clear spares installed and starred");
        assert_eq!(history.remove(&[3]).unwrap(), 0, "a starred one when asked");
        assert_eq!(ids(&history), [2]);
    }

    #[test]
    fn stars_round_trip_and_stop_at_the_cap() {
        let mut history = History::default();
        for seed in 0..KEEP as u64 + 1 {
            history.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            );
        }
        let json = serde_json::to_value(&history).unwrap();
        assert!(
            json["entries"][0].get("starred").is_none(),
            "left out when off"
        );
        let all: Vec<u32> = history.entries.iter().map(|e| e.id).collect();
        assert_eq!(all.len(), KEEP);
        history.star(&all, true).unwrap();
        let newest = (history.add(
            Settings::new(),
            Extras::default(),
            Label::default(),
            report(0),
        ))
        .id;
        let err = history.star(&[newest], true).unwrap_err().to_string();
        assert_eq!(err, format!("History keeps {KEEP} seeds: unstar one first"));
        history.star(&all[1..2], false).unwrap();
        history.star(&[newest], true).unwrap();
        assert!(history.star(&[9999], true).is_err());
        let back = History::parse(&serde_json::to_vec(&history).unwrap()).unwrap();
        assert!(back.entries[0].starred && !back.entries[1].starred);
        assert!(Summary::from(&back.entries[0]).starred);
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
        assert_eq!(history.entries[0].brief.seed, 5);
    }

    #[test]
    fn trimming_spares_the_installed_seed() {
        let mut history = History::default();
        let add = |history: &mut History, seed| {
            let entry = history.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            );
            entry.id
        };
        let first = add(&mut history, 0);
        history.install(Some(first));
        for seed in 1..=KEEP as u64 + 5 {
            add(&mut history, seed);
        }
        assert_eq!(history.entries.len(), KEEP);
        assert_eq!(history.entries[0].id, first);
        assert_eq!(history.entries[1].brief.seed, 7);
        assert!(history.view().installed.is_some());
    }

    #[test]
    fn trimming_spares_starred_seeds_and_the_newest() {
        let mut history = History::default();
        let add = |h: &mut History, seed| {
            (h.add(
                Settings::new(),
                Extras::default(),
                Label::default(),
                report(seed),
            ))
            .id
        };
        let first = add(&mut history, 0);
        history.star(&[first], true).unwrap();
        for seed in 1..=KEEP as u64 + 5 {
            add(&mut history, seed);
        }
        assert_eq!(history.entries.len(), KEEP);
        assert_eq!(history.entries[0].id, first, "starred");
        // Every seed starred: a new one still goes in, and stays while newest.
        let ids: Vec<u32> = history.entries.iter().map(|e| e.id).collect();
        history.star(&ids, true).unwrap();
        let newest = add(&mut history, 999);
        assert_eq!(history.entries.len(), KEEP + 1);
        assert_eq!(history.entries.last().unwrap().id, newest);
        let next = add(&mut history, 1000);
        assert_eq!(
            history.entries.len(),
            KEEP + 1,
            "the last unstarred one went"
        );
        assert!(history.get(newest).is_err() && history.get(next).is_ok());
    }

    #[test]
    fn entries_saved_before_vanilla_defaults_keep_the_randomizers() {
        let mut history = History::default();
        history.add(
            Settings::new(),
            Extras::default(),
            Label::default(),
            report(7),
        );
        let mut json = serde_json::to_value(&history).unwrap();
        assert_eq!(json["settings_version"], 2);
        let current = History::parse(json.to_string().as_bytes()).unwrap();
        assert_eq!(current.entries[0].settings, Settings::new());
        json.as_object_mut().unwrap().remove("settings_version");
        let old = History::parse(json.to_string().as_bytes()).unwrap();
        let want = crate::settings::favorites::randomizer();
        assert_eq!(old.entries[0].settings, want);
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
        assert_eq!(e.extras.release, Some(Release::V111), "its build kept");
        assert!(!e.extras.vanilla);
    }

    #[test]
    fn a_seed_keeps_the_build_it_was_made_for_and_old_ones_gain_it() {
        let mut history = History::default();
        let mut base = report(7);
        base.written.release = Release::V100;
        let asked = Extras {
            release: Some(Release::V111),
            ..Default::default()
        };
        let e = history.add(Settings::new(), asked, Label::default(), base);
        assert_eq!(
            e.extras.release,
            Some(Release::V100),
            "the build it was made for"
        );
        assert!(!Summary::from(e).test, "a build isn't a test setup");
        let mut json = serde_json::to_value(&history).unwrap();
        json["entries"][0]["extras"] = serde_json::json!({});
        let old = History::parse(json.to_string().as_bytes()).unwrap();
        assert_eq!(old.entries[0].extras.release, Some(Release::V100));
    }
}
