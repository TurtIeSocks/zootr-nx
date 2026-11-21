//! The Live tab: its recording sessions, and the commands it labels,
//! marks and mutes values with.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event as _;
use zootr::config::Config;
use zootr::gamedata::GameData;
use zootr::recorder::{self, Ask, Event, Hub, Numbered, Snapshot, Watch};

use crate::checks::Checks;
use crate::{Items, Res, romfs, text};

/// Every Live tab event, to whichever page is listening.
#[derive(Clone, Serialize, Type)]
pub(crate) struct LiveEvent(Numbered);

impl tauri_specta::Event for LiveEvent {
    const NAME: &'static str = "live-event";
}

/// The Live tab's sessions, and the GameData names they show, loaded once.
pub(crate) struct Live {
    pub(crate) gd: Mutex<Option<Arc<GameData>>>,
    hub: Arc<Hub>,
    watch: Mutex<Option<Watch>>,
}

impl Live {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self {
            gd: Mutex::default(),
            hub: Hub::new(move |n| {
                if let Event::Row { change, .. } = &n.event {
                    app.state::<Checks>().change(&app, change);
                    app.state::<Items>().change(&app, change);
                }
                let _ = LiveEvent(n.clone()).emit(&app);
            }),
            watch: Mutex::default(),
        }
    }

    pub(crate) fn gamedata(&self, config: &Config) -> anyhow::Result<Arc<GameData>> {
        let mut gd = self.gd.lock().unwrap();
        if gd.is_none() {
            *gd = Some(Arc::new(recorder::gamedata(&romfs(config)?)?));
        }
        Ok(gd.clone().expect("just loaded"))
    }

    /// Records the picked target whenever it runs, for as long as the app
    /// is open.
    pub(crate) fn watch(&self, app: &AppHandle) -> anyhow::Result<()> {
        let app = app.clone();
        let gd = move || {
            let gd = app.state::<Live>().gamedata(&Config::load()?);
            // Ready before the first row, so no flip is missed.
            let _ = app.state::<Checks>().state(&app);
            let _ = app.state::<Items>().state(&app);
            gd
        };
        let target = || Config::load()?.target();
        let logs = recorder::logs()?;
        *self.watch.lock().unwrap() = Some(Watch::start(self.hub.clone(), gd, target, logs));
        Ok(())
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
    if let Some(gd) = &*live.gd.lock().unwrap() {
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
