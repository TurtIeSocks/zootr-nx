//! `ui.json`, the windows' view state, beside `config.json`. The frontend
//! owns its shape: each window's part is kept as given under the window's
//! label, and `shared` holds what every window shows but only the main one
//! sets, such as the theme. The CLI never reads it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::Serialize;
use serde_json::{Map, Value};
use specta::Type;
use tauri::{AppHandle, State, Window};
use tauri_specta::Event as _;
use zootr::config::Config;

use crate::{Res, text};

/// The part every window reads.
const SHARED: &str = "shared";
/// The only window that writes the shared part.
const MAIN: &str = "main";

/// Held while `ui.json` is read and rewritten, so windows saving at once
/// don't drop each other's parts.
#[derive(Default)]
pub(crate) struct UiFile(Mutex<()>);

/// A window's part of `ui.json`, and the shared part.
#[derive(Serialize, Type)]
pub(crate) struct UiParts {
    own: Option<Value>,
    shared: Option<Value>,
}

/// The shared part changed, so the pop outs follow the main window.
#[derive(Clone, Serialize, Type)]
pub(crate) struct UiShared(Value);

impl tauri_specta::Event for UiShared {
    const NAME: &'static str = "ui-shared";
}

fn path() -> anyhow::Result<PathBuf> {
    Ok(Config::path()?.with_file_name("ui.json"))
}

/// Every part by label, or none when there is no file yet. A file that
/// doesn't parse reads as none too, so the next save starts it over
/// rather than every load and save failing until it's deleted.
fn read(path: &Path) -> anyhow::Result<Map<String, Value>> {
    match std::fs::read(path) {
        Ok(b) => Ok(serde_json::from_slice(&b).unwrap_or_else(|e| {
            log::warn!("{} doesn't parse, starting it over: {e}", path.display());
            Map::new()
        })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn load(path: &Path, label: &str) -> anyhow::Result<UiParts> {
    let mut parts = read(path)?;
    Ok(UiParts {
        own: parts.remove(label),
        shared: parts.remove(SHARED),
    })
}

/// Rewrites `label`'s part, and the shared part when `label` is the main
/// window. True when the shared part changed.
fn save(path: &Path, label: &str, own: Value, shared: Value) -> anyhow::Result<bool> {
    let mut parts = read(path)?;
    parts.insert(label.to_owned(), own);
    let changed = label == MAIN && parts.get(SHARED) != Some(&shared);
    if changed {
        parts.insert(SHARED.to_owned(), shared);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Written beside it, then moved over it: a quit mid-write leaves the
    // old file whole.
    let part = path.with_extension("json.part");
    std::fs::write(&part, serde_json::to_vec_pretty(&parts)?)
        .with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(changed)
}

/// The calling window's part of `ui.json`, and the shared part.
#[tauri::command]
#[specta::specta]
pub(crate) fn ui_load(window: Window, file: State<UiFile>) -> Res<UiParts> {
    let _held = file.0.lock().unwrap();
    load(&path().map_err(text)?, window.label()).map_err(text)
}

/// Saves the calling window's part. Only the main window's `shared` is kept.
#[tauri::command]
#[specta::specta]
pub(crate) fn ui_save(
    app: AppHandle,
    window: Window,
    file: State<UiFile>,
    own: Value,
    shared: Value,
) -> Res<()> {
    let _held = file.0.lock().unwrap();
    if save(&path().map_err(text)?, window.label(), own, shared.clone()).map_err(text)? {
        let _ = UiShared(shared).emit(&app);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-ui-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("ui.json")
    }

    #[test]
    fn each_window_keeps_its_own_part() {
        let path = scratch("parts");
        assert!(load(&path, MAIN).unwrap().own.is_none());
        save(&path, MAIN, json!({"tab": "Live"}), json!({})).unwrap();
        save(&path, "dev", json!({"open": true}), json!({})).unwrap();
        save(&path, MAIN, json!({"tab": "Play"}), json!({})).unwrap();
        assert_eq!(load(&path, "dev").unwrap().own, Some(json!({"open": true})));
        assert_eq!(load(&path, MAIN).unwrap().own, Some(json!({"tab": "Play"})));
    }

    #[test]
    fn only_the_main_window_sets_the_shared_part() {
        let path = scratch("shared");
        let dark = json!({"theme": "dark"});
        assert!(save(&path, MAIN, json!({}), dark.clone()).unwrap());
        assert!(!save(&path, MAIN, json!({}), dark.clone()).unwrap());
        assert!(!save(&path, "tracker", json!({}), json!({"theme": "light"})).unwrap());
        assert_eq!(load(&path, "tracker").unwrap().shared, Some(dark));
    }

    #[test]
    fn a_file_cut_short_starts_over() {
        let path = scratch("cut");
        save(&path, MAIN, json!({"tab": "Live"}), json!({})).unwrap();
        std::fs::write(&path, "{\"main\": {\"ta").unwrap();
        assert!(load(&path, MAIN).unwrap().own.is_none());
        save(&path, MAIN, json!({"tab": "Play"}), json!({})).unwrap();
        assert_eq!(load(&path, MAIN).unwrap().own, Some(json!({"tab": "Play"})));
        assert!(!path.with_extension("json.part").exists());
    }
}
