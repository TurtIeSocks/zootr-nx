//! The Items tab and its pop out: what Link has, from the last save and the
//! watcher's rows.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;
use zootr::config::Config;
use zootr::history::History;
use zootr::inventory::{self, Cell, Inventory, TrackerOptions};
use zootr::savefile::{self, Change};

use crate::live_tab::Live;
use crate::{Res, blocking, game_art, romfs};

/// What the Items tab and the pop out draw.
#[derive(Clone, Serialize, Type)]
pub(crate) struct ItemsState {
    cells: Vec<Cell>,
    options: TrackerOptions,
    /// The icons come from the dump: game assets are on and a dump is set.
    game_art: bool,
}

/// The grid changed, or its options did.
#[derive(Clone, Serialize, Type)]
pub(crate) struct ItemsEvent(ItemsState);

impl tauri_specta::Event for ItemsEvent {
    const NAME: &'static str = "items-event";
}

/// What Link has, from the last save played since the installed seed went
/// in and then the watcher's rows. Kept with that install time, so a new
/// install starts it over.
#[derive(Default)]
pub(crate) struct Items(Mutex<Option<(Option<u32>, Inventory)>>);

impl Items {
    pub(crate) fn state(&self, app: &AppHandle) -> anyhow::Result<ItemsState> {
        let config = Config::load()?;
        let since = History::load()?.installed().at;
        let mut items = self.0.lock().unwrap();
        if items.as_ref().is_none_or(|(at, _)| *at != since) {
            let gd = app.state::<Live>().gamedata(&config)?;
            let mut inv = Inventory::new(&gd, &romfs(&config)?)?;
            if let Some(save) = savefile::newest_since(&config.save_dir()?, since)? {
                inv.load(inventory::in_save(&savefile::load(&save)?));
            }
            *items = Some((since, inv));
        }
        Ok(ItemsState {
            cells: items.as_ref().expect("just built").1.cells(),
            game_art: game_art(&config),
            options: config.tracker.tidy(),
        })
    }

    pub(crate) fn change(&self, app: &AppHandle, change: &Change) {
        let changed = (self.0.lock().unwrap().as_mut()).is_some_and(|(_, inv)| inv.change(change));
        if changed {
            self.emit(app);
        }
    }

    /// Tells every window, so the tab and the pop out never disagree.
    pub(crate) fn emit(&self, app: &AppHandle) {
        if let Ok(state) = self.state(app) {
            let _ = ItemsEvent(state).emit(app);
        }
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn items_state(app: AppHandle) -> Res<ItemsState> {
    blocking(move || app.state::<Items>().state(&app)).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn items_options(app: AppHandle, options: TrackerOptions) -> Res<()> {
    blocking(move || {
        let config = Config {
            tracker: options.tidy(),
            ..Config::load()?
        };
        config.save()?;
        app.state::<Items>().emit(&app);
        Ok(())
    })
    .await
}
