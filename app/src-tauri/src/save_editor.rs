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
use zootr::starting::{self, Start};

use crate::{Live, Res, blocking, romfs};

/// The grid with no save loaded, and the pouch table, read once.
#[derive(Default)]
pub(crate) struct Layout(Mutex<Option<Arc<(Inventory, Pouch)>>>);

impl Layout {
    fn get(&self, gd: &GameData, config: &Config) -> anyhow::Result<Arc<(Inventory, Pouch)>> {
        let mut layout = self.0.lock().unwrap();
        if layout.is_none() {
            let romfs = romfs(config)?;
            *layout = Some(Arc::new((
                Inventory::new(gd, &romfs)?,
                Pouch::load(&romfs)?,
            )));
        }
        Ok(layout.clone().expect("just loaded"))
    }
}

/// What every view needs: GameData, the layout, and the slot opened with
/// the pending edits.
struct Ready {
    gd: Arc<GameData>,
    layout: Arc<(Inventory, Pouch)>,
    opened: Opened,
}

fn ready(app: &AppHandle, slot: u8, pending: &Pending) -> anyhow::Result<Ready> {
    let config = Config::load()?;
    let gd = app.state::<Live>().gamedata(&config)?;
    let layout = app.state::<Layout>().get(&gd, &config)?;
    let opened = editor::open(&gd, &config.save_dir()?, slot, pending)?;
    Ok(Ready { gd, layout, opened })
}

/// Every slot holding a save, newest first.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_slots(app: AppHandle) -> Res<Vec<SlotRow>> {
    with_layout(app, |_, (inv, _)| {
        editor::slots(inv, &Config::load()?.save_dir()?)
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
        Ok(editor::items(&r.gd, &r.layout.0, &r.layout.1, &r.opened))
    })
    .await
}

/// `pending` cut to the edits that change the slot, and how many things
/// they change. Refuses an edit the save won't take.
#[tauri::command]
#[specta::specta]
pub(crate) async fn save_stage(app: AppHandle, slot: u8, pending: Pending) -> Res<Staged> {
    with_layout(app, move |gd, (inv, pouch)| {
        editor::stage(gd, inv, pouch, &Config::load()?.save_dir()?, slot, &pending)
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
        editor::click(&r.layout.0, &r.layout.1, &r.opened, at as usize, intent)
    })
    .await
}

/// The grid, caps and Status panel of a new game starting with `start`,
/// for the settings' Inventory panel, with the words each setting in
/// `given` gives shown held and locked.
#[tauri::command]
#[specta::specta]
pub(crate) async fn start_items(
    app: AppHandle,
    start: Start,
    given: Vec<Vec<String>>,
) -> Res<ItemsView> {
    with_layout(app, move |gd, (inv, pouch)| {
        starting::view(gd, inv, pouch, &start, &given)
    })
    .await
}

/// `start` after a click on a grid cell.
#[tauri::command]
#[specta::specta]
pub(crate) async fn start_click(
    app: AppHandle,
    start: Start,
    at: u32,
    intent: Intent,
) -> Res<Start> {
    with_layout(app, move |gd, (inv, pouch)| {
        starting::click(gd, inv, pouch, &start, at as usize, intent)
    })
    .await
}

/// `start` after a Status field's commit.
#[tauri::command]
#[specta::specta]
pub(crate) async fn start_set(app: AppHandle, start: Start, edit: Edit) -> Res<Start> {
    with_layout(app, move |gd, (inv, pouch)| {
        starting::set(gd, inv, pouch, &start, edit)
    })
    .await
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

/// `f` with GameData and the layout, off the main thread.
async fn with_layout<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&GameData, &(Inventory, Pouch)) -> anyhow::Result<T> + Send + 'static,
) -> Res<T> {
    blocking(move || {
        let config = Config::load()?;
        let gd = app.state::<Live>().gamedata(&config)?;
        let layout = app.state::<Layout>().get(&gd, &config)?;
        f(&gd, &layout)
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
