//! The Live tab: its recording sessions, and the commands it labels,
//! marks and mutes values with.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event as _;
use zootr::config::Config;
use zootr::gamedata::GameData;
use zootr::recorder::{self, Ask, Event, Hub, LiveStatus, Numbered, Snapshot, Source, Watch};

use crate::checks::Checks;
use crate::items::Items;
use crate::{Res, cached, romfs, text};

/// Every Live tab event, to whichever page is listening.
#[derive(Clone, Serialize, Type)]
pub(crate) struct LiveEvent(Numbered);

impl tauri_specta::Event for LiveEvent {
    const NAME: &'static str = "live-event";
}

/// The Live tab's sessions, and the GameData names they show, loaded once.
pub(crate) struct Live {
    gd: Mutex<Option<Arc<GameData>>>,
    hub: Arc<Hub>,
    watch: Mutex<Option<Watch>>,
}

impl Live {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self {
            gd: Mutex::default(),
            hub: Hub::new(move |n| {
                match &n.event {
                    Event::Row { change, .. } => {
                        app.state::<Checks>().change(&app, change);
                        app.state::<Items>().change(&app, change);
                    }
                    Event::Status { status } => log_status(status),
                    _ => {}
                }
                let _ = LiveEvent(n.clone()).emit(&app);
            }),
            watch: Mutex::default(),
        }
    }

    pub(crate) fn gamedata(&self, config: &Config) -> anyhow::Result<Arc<GameData>> {
        cached(&self.gd, || recorder::gamedata(&romfs(config)?))
    }

    /// GameData, if something loaded it already.
    pub(crate) fn loaded(&self) -> Option<Arc<GameData>> {
        self.gd.lock().unwrap().clone()
    }

    /// Records the picked target whenever it runs, for as long as the app
    /// is open.
    pub(crate) fn watch(&self, app: &AppHandle) -> anyhow::Result<()> {
        let app = app.clone();
        // GameData, with the tracker and the inventory ready before the
        // first row, so no flip is missed.
        let ready = move || {
            let gd = app.state::<Live>().gamedata(&Config::load()?);
            if let Err(e) = app.state::<Checks>().state(&app) {
                log::debug!("Tracker not ready for the game: {e:#}");
            }
            if let Err(e) = app.state::<Items>().state(&app) {
                log::debug!("Inventory not ready for the game: {e:#}");
            }
            gd
        };
        let target = || Config::load()?.target();
        let logs = recorder::logs()?;
        *self.watch.lock().unwrap() = Some(Watch::start(self.hub.clone(), ready, target, logs));
        Ok(())
    }
}

/// The game's connection, for the app's log.
fn log_status(status: &LiveStatus) {
    let (what, why) = match status {
        LiveStatus::Waiting { reason } => ("waiting", reason.as_deref()),
        LiveStatus::Attaching { reason } => ("connecting", Some(reason.as_str())),
        LiveStatus::Recording { session } => match session.source {
            Source::Live => ("recording", None),
            Source::Saves => ("recording from autosaves", session.reason.as_deref()),
        },
    };
    match why {
        Some(why) => log::info!("Game: {what} ({why})"),
        None => log::info!("Game: {what}"),
    }
}

/// Where the Live tab picks up after a reload.
#[tauri::command]
#[specta::specta]
pub(crate) fn live_state(live: State<Live>) -> Snapshot {
    live.hub.snapshot()
}

/// Label an unnamed value, or drop its label for an empty one.
#[tauri::command]
#[specta::specta]
pub(crate) fn live_label(live: State<Live>, hash: u32, label: String) -> Res<()> {
    zootr::labels::set(&zootr::labels::path().map_err(text)?, hash, &label).map_err(text)?;
    if let Some(gd) = live.loaded() {
        gd.set_label(hash, &label);
    }
    Ok(())
}

/// Mark the session's log with what happened. Blank marks it "Mark".
#[tauri::command]
#[specta::specta]
pub(crate) fn live_mark(live: State<Live>, what: String) -> Res<()> {
    live.hub.ask(Ask::Mark(what)).map_err(text)
}

/// Leave a value out of the session from now on.
#[tauri::command]
#[specta::specta]
pub(crate) fn live_mute(live: State<Live>, hash: u32) -> Res<()> {
    live.hub.ask(Ask::Mute(hash)).map_err(text)
}
