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
    blocking(move || {
        let config = Config::load()?;
        let gd = app.state::<Live>().gamedata(&config)?;
        let layout = app.state::<Layout>().get(&gd, &config)?;
        editor::slots(&layout.0, &config.save_dir()?)
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
    blocking(move || {
        let config = Config::load()?;
        let gd = app.state::<Live>().gamedata(&config)?;
        let layout = app.state::<Layout>().get(&gd, &config)?;
        let (inv, pouch) = &*layout;
        editor::stage(&gd, inv, pouch, &config.save_dir()?, slot, &pending)
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
