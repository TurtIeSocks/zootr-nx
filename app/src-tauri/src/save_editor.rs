//! The Save page's commands. Each takes the slot and the pending edits and
//! reads the file again, so the page always shows it as Save would leave
//! it. Only the grid's layout and the pouch table are kept, since they come
//! from the dump and never change.

use std::sync::{Arc, Mutex};

use anyhow::bail;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use zootr::config::Config;
use zootr::gamedata::GameData;
use zootr::inventory::{Intent, Inventory, Pouch};
use zootr::save_editor::{
    self as editor, FlagQuery, FlagsView, ItemsView, Opened, Pending, SlotRow, Staged,
};
use zootr::savefile::Edit;
use zootr::settings::{NewGame, Settings};
use zootr::starting::{self, Start};

use crate::{Live, Res, blocking, cached, romfs};

/// The grid with no save loaded, and the pouch table.
struct Grid {
    inv: Inventory,
    pouch: Pouch,
}

/// The [`Grid`], read once.
#[derive(Default)]
pub(crate) struct Layout(Mutex<Option<Arc<Grid>>>);

/// The config, GameData and the layout.
fn loaded(app: &AppHandle) -> anyhow::Result<(Config, Arc<GameData>, Arc<Grid>)> {
    let config = Config::load()?;
    let gd = app.state::<Live>().gamedata(&config)?;
    let grid = cached(&app.state::<Layout>().0, || {
        let romfs = romfs(&config)?;
        Ok(Grid {
            inv: Inventory::new(&gd, &romfs)?,
            pouch: Pouch::load(&romfs)?,
        })
    })?;
    Ok((config, gd, grid))
}

/// What every view needs: GameData, the layout, and the slot opened with
/// the pending edits.
struct Ready {
    gd: Arc<GameData>,
    grid: Arc<Grid>,
    opened: Opened,
}

fn ready(app: &AppHandle, slot: u8, pending: &Pending) -> anyhow::Result<Ready> {
    let (config, gd, grid) = loaded(app)?;
    let opened = editor::open(&gd, &config.save_dir()?, slot, pending)?;
    Ok(Ready { gd, grid, opened })
}

/// Every slot holding a save, newest first.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_slots(app: AppHandle) -> Res<Vec<SlotRow>> {
    with_layout(app, |config, _, grid| {
        editor::slots(&grid.inv, &config.save_dir()?)
    })
    .await
}

/// A preset an existing save can take.
#[derive(Serialize, Type)]
pub(crate) struct SavePreset {
    name: String,
    label: String,
    what: String,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_presets() -> Vec<SavePreset> {
    zootr::presets::for_saves()
        .map(|p| SavePreset {
            name: p.name.into(),
            label: p.label.into(),
            what: p.what.into(),
        })
        .collect()
}

/// The Items tab.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_items(app: AppHandle, slot: u8, pending: Pending) -> Res<ItemsView> {
    blocking(move || {
        let r = ready(&app, slot, &pending)?;
        Ok(editor::items(&r.gd, &r.grid.inv, &r.grid.pouch, &r.opened))
    })
    .await
}

/// `pending` cut to the edits that change the slot, and how many things
/// they change. Refuses an edit the save won't take.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_stage(app: AppHandle, slot: u8, pending: Pending) -> Res<Staged> {
    with_layout(app, move |config, gd, grid| {
        let dir = config.save_dir()?;
        editor::stage(gd, &grid.inv, &grid.pouch, &dir, slot, &pending)
    })
    .await
}

/// The edits a click on a grid cell makes, to add to the pending ones.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_click(
    app: AppHandle,
    slot: u8,
    pending: Pending,
    at: u32,
    intent: Intent,
) -> Res<Vec<Edit>> {
    blocking(move || {
        let r = ready(&app, slot, &pending)?;
        editor::click(&r.grid.inv, &r.grid.pouch, &r.opened, at as usize, intent)
    })
    .await
}

/// The grid and caps of a new game starting with `start`, for the
/// settings' Inventory panel, as a seed with `settings` deals it, with the
/// words each setting in `given` gives shown held and locked.
#[tauri::command]
#[specta::specta]
pub(crate) async fn start_items(
    app: AppHandle,
    start: Start,
    given: Vec<Vec<String>>,
    settings: Settings,
) -> Res<ItemsView> {
    with_layout(app, move |_, gd, grid| {
        let pouch = seeded(&grid.pouch, &settings)?;
        starting::view(gd, &grid.inv, &pouch, &start, &given)
    })
    .await
}

/// `start` after a click on a grid cell, as a seed with `settings` deals it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn start_click(
    app: AppHandle,
    start: Start,
    at: u32,
    intent: Intent,
    settings: Settings,
) -> Res<Start> {
    with_layout(app, move |_, gd, grid| {
        let pouch = seeded(&grid.pouch, &settings)?;
        starting::click(gd, &grid.inv, &pouch, &start, at as usize, intent)
    })
    .await
}

/// The pouch table as a seed with `settings` writes it.
fn seeded(pouch: &Pouch, settings: &Settings) -> anyhow::Result<Pouch> {
    Ok(pouch
        .clone()
        .seeded(&NewGame::from_settings(settings, &[])?))
}

/// `start` holding all of Link's gear too (`--start arsenal`).
#[tauri::command]
#[specta::specta]
pub(crate) fn start_arsenal(mut start: Start) -> Res<Start> {
    start
        .add(starting::ARSENAL, 1)
        .map_err(|e| format!("{e:#}"))?;
    Ok(start)
}

/// `f` with the config, GameData and the layout, off the main thread.
async fn with_layout<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Config, &GameData, &Grid) -> anyhow::Result<T> + Send + 'static,
) -> Res<T> {
    blocking(move || {
        let (config, gd, grid) = loaded(&app)?;
        f(&config, &gd, &grid)
    })
    .await
}

/// The Flags tab.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_flags(
    app: AppHandle,
    slot: u8,
    pending: Pending,
    query: FlagQuery,
) -> Res<FlagsView> {
    blocking(move || {
        let r = ready(&app, slot, &pending)?;
        Ok(editor::flags(
            &r.gd,
            &r.opened.edited,
            &r.opened.staged(&r.gd),
            &query,
        ))
    })
    .await
}

/// What saving changed.
#[derive(Serialize, Type)]
pub(crate) struct SaveEdit {
    /// Each value as `name: old -> new`.
    log: Vec<String>,
    backup: String,
}

/// Write the pending edits to a slot, keeping a backup. Refused while the
/// game runs, or when the file changed since `stamp`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_write(
    app: AppHandle,
    slot: u8,
    pending: Pending,
    stamp: u64,
) -> Res<SaveEdit> {
    blocking(move || {
        let config = Config::load()?;
        if config.targets.iter().any(|t| t.running()) {
            bail!("Close the game first");
        }
        let gd = app.state::<Live>().gamedata(&config)?;
        let (log, backup) = editor::write(&gd, &config.save_dir()?, slot, &pending, stamp)?;
        Ok(SaveEdit {
            log,
            backup: backup.display().to_string(),
        })
    })
    .await
}
