//! The Items tab and its pop out: what Link has, from the last save and the
//! watcher's rows.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;
use zootr::config::Config;
use zootr::hearts::{self, Hearts};
use zootr::history::History;
use zootr::inventory::{self, Cell, Inventory, TrackerOptions};
use zootr::savefile::{self, Change};
use zootr::tracker;

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
pub(crate) struct Items(Mutex<Option<Installed>>);

/// The grid for one install, and the heart checks that count its hearts.
struct Installed {
    since: Option<u32>,
    inv: Inventory,
    hearts: Hearts,
}

impl Installed {
    /// The installed seed's grid and heart checks, filled from the last
    /// save played since it went in.
    fn build(app: &AppHandle, config: &Config, history: &History) -> anyhow::Result<Self> {
        let since = history.installed().at;
        let gd = app.state::<Live>().gamedata(config)?;
        let romfs = romfs(config)?;
        let mut inv = Inventory::new(&gd, &romfs)?;
        let report = (history.installed().id)
            .map(|id| history.report(id))
            .transpose()?;
        let (spots, start) = report
            .as_ref()
            .map_or((&[][..], &[][..]), |r| (&r.spots[..], &r.start.items[..]));
        let mut hearts = Hearts::new(&gd, &hearts::vanilla(&romfs)?, spots, start)?;
        let listed = report.as_ref().map_or(&[][..], |r| &r.key_rings[..]);
        inv.set_rings(&romfs, listed)?;
        if let Some(save) = savefile::newest_since(&config.save_dir()?, since)? {
            let save = savefile::load(&save)?;
            inv.load(inventory::in_save(&save));
            hearts.load(tracker::in_save(&save), inventory::in_save(&save));
        }
        inv.set_counts(hearts.counts());
        Ok(Self { since, inv, hearts })
    }

    /// Takes one change the watcher saw. True when the grid changed.
    fn change(&mut self, change: &Change) -> bool {
        let held = self.inv.change(change);
        let counted = self.hearts.change(change) && self.inv.set_counts(self.hearts.counts());
        held || counted
    }
}

impl Items {
    pub(crate) fn state(&self, app: &AppHandle) -> anyhow::Result<ItemsState> {
        let config = Config::load()?;
        let history = History::load()?;
        let since = history.installed().at;
        let mut items = self.0.lock().unwrap();
        if items.as_ref().is_none_or(|held| held.since != since) {
            *items = Some(Installed::build(app, &config, &history)?);
        }
        Ok(ItemsState {
            cells: items.as_ref().expect("just built").inv.cells(),
            game_art: game_art(&config),
            options: config.tracker.tidy(),
        })
    }

    pub(crate) fn change(&self, app: &AppHandle, change: &Change) {
        let changed = (self.0.lock().unwrap().as_mut()).is_some_and(|held| held.change(change));
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
