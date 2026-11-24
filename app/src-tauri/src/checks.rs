//! The Checks tab: the installed seed's checks and shuffled doors, kept
//! current by the watcher, and the player's marks and picks.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;
use zootr::config::Config;
use zootr::history::{History, Mark};
use zootr::savefile::{self, Change};
use zootr::tracker::{self, DoorRow, Row, Tracker};

use crate::live_tab::Live;
use crate::{Res, blocking, edit_history, romfs};

/// A spot checked or unchecked in the installed seed.
#[derive(Clone, Serialize, Type)]
pub(crate) struct ChecksEvent {
    hash: String,
    checked: bool,
}

impl tauri_specta::Event for ChecksEvent {
    const NAME: &'static str = "checks-event";
}

/// The installed seed's doors, once one is revealed or hidden again.
#[derive(Clone, Serialize, Type)]
pub(crate) struct DoorsEvent {
    doors: Vec<DoorRow>,
}

impl tauri_specta::Event for DoorsEvent {
    const NAME: &'static str = "doors-event";
}

#[derive(Serialize, Type)]
pub(crate) struct ChecksState {
    /// The installed seed's history entry, if one is installed.
    id: Option<u32>,
    rows: Vec<Row>,
    /// Its shuffled doors, none with the setting off.
    doors: Vec<DoorRow>,
}

/// The installed seed's tracker, kept current by the watcher's rows.
#[derive(Default)]
pub(crate) struct Checks(Mutex<Option<(u32, Tracker)>>);

impl Checks {
    /// Builds the tracker for a newly installed seed from the last save,
    /// first finding places for a seed saved before they were kept.
    pub(crate) fn state(&self, app: &AppHandle) -> anyhow::Result<ChecksState> {
        let mut history = History::load()?;
        let Some(id) = history.installed().id else {
            *self.0.lock().unwrap() = None;
            return Ok(ChecksState {
                id: None,
                rows: vec![],
                doors: vec![],
            });
        };
        let mut tracker = self.0.lock().unwrap();
        if tracker.as_ref().is_none_or(|(had, _)| *had != id) {
            let config = Config::load()?;
            if history
                .get(id)?
                .report
                .spots
                .iter()
                .all(|s| s.place.is_empty())
            {
                history.place(id, &romfs(&config)?)?;
                history.save()?;
            }
            let gd = app.state::<Live>().gamedata(&config)?;
            let report = &history.get(id)?.report;
            let mut t = Tracker::new(&gd, &report.spots)?.with_doors(&report.entrances);
            // Only a save played since this seed went in: an older one
            // holds another seed's checks.
            let since = history.installed().at;
            if let Some(save) = savefile::newest_since(&config.save_dir()?, since)? {
                t.load(tracker::in_save(&savefile::load(&save)?));
            }
            *tracker = Some((id, t));
        }
        let (_, t) = tracker.as_ref().expect("just built");
        let gd = app.state::<Live>().loaded();
        let entry = history.get(id)?;
        Ok(ChecksState {
            id: Some(id),
            rows: t.rows(entry, |f| gd.as_ref()?.label(f)),
            doors: t.door_rows(entry),
        })
    }

    pub(crate) fn change(&self, app: &AppHandle, change: &Change) {
        let Some((id, t)) = &mut *self.0.lock().unwrap() else {
            return;
        };
        let flips = t.change(change);
        if flips.doors
            && let Ok(history) = History::load()
            && let Ok(entry) = history.get(*id)
        {
            let _ = DoorsEvent {
                doors: t.door_rows(entry),
            }
            .emit(app);
        }
        let events = flips.checked.iter().map(|h| (h, true));
        for (hash, checked) in events.chain(flips.unchecked.iter().map(|h| (h, false))) {
            let hash = hash.to_string();
            let _ = ChecksEvent { hash, checked }.emit(app);
        }
    }
}

/// The Checks tab's rows for the installed seed.
#[tauri::command]
#[specta::specta]
pub(crate) async fn checks_state(app: AppHandle) -> Res<ChecksState> {
    blocking(move || app.state::<Checks>().state(&app)).await
}

/// Mark a check in a seed, or clear its mark.
#[tauri::command]
#[specta::specta]
pub(crate) fn checks_mark(id: u32, hash: String, mark: Option<Mark>) -> Res<()> {
    let hash = hash.parse().map_err(|_| format!("{hash} isn't a spot"))?;
    edit_history(|h| h.mark(id, hash, mark))
}

/// Guess the dungeon behind a seed's shuffled door, or clear the guess.
#[tauri::command]
#[specta::specta]
pub(crate) fn checks_pick(id: u32, door: String, dungeon: Option<String>) -> Res<()> {
    edit_history(|h| h.pick(id, &door, dungeon.as_deref()))
}
