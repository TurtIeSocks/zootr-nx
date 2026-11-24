//! The Archipelago client on the Play page: the installed seed's slot
//! connected to a room, its notes sent to the page as events.

use std::sync::Mutex;
use std::sync::mpsc::{Sender, channel};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;
use zootr::ap::client::{self, Note, Room};
use zootr::config::Config;
use zootr::live;

use crate::{Live, Res, blocking, romfs};

/// The most log lines kept for a page opened mid-run.
const KEPT: usize = 200;

#[derive(Clone, Copy, Default, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ApStatus {
    #[default]
    Offline,
    Connecting,
    Connected,
    Refused,
}

/// What the page shows: the status and the log so far.
#[derive(Clone, Default, Serialize, Type)]
pub(crate) struct ApState {
    status: ApStatus,
    log: Vec<String>,
}

/// A status change or a log line.
#[derive(Clone, Serialize, Type)]
pub(crate) struct ApEvent {
    status: ApStatus,
    line: Option<String>,
}

impl tauri_specta::Event for ApEvent {
    const NAME: &'static str = "ap-event";
}

/// The running client: what stops it, and what the page shows.
#[derive(Default)]
pub(crate) struct Ap(Mutex<Held>);

#[derive(Default)]
struct Held {
    /// Dropped to stop the running client.
    stop: Option<Sender<()>>,
    /// The latest run's number. A run stopped by Disconnect can still be
    /// winding down when the next one starts, and must leave it alone.
    run: u64,
    state: ApState,
}

impl Ap {
    /// Start a run stopped by dropping `stop`, its number, unless one is
    /// running.
    fn start(&self, stop: Sender<()>) -> Res<u64> {
        let mut held = self.0.lock().unwrap();
        if held.stop.is_some() {
            return Err("The client is already running".into());
        }
        held.run += 1;
        held.stop = Some(stop);
        held.state.log.clear();
        Ok(held.run)
    }

    /// Whether `run` is the latest one.
    fn current(&self, run: u64) -> bool {
        self.0.lock().unwrap().run == run
    }

    /// `run` ended: whether it was refused, or `None` when a newer run
    /// owns the page now.
    fn finish(&self, run: u64) -> Option<bool> {
        let mut held = self.0.lock().unwrap();
        if held.run != run {
            return None;
        }
        held.stop = None;
        Some(held.state.status == ApStatus::Refused)
    }

    /// Keep and send a status, and a line when there is one.
    fn say(&self, app: &AppHandle, status: Option<ApStatus>, line: Option<String>) {
        let mut held = self.0.lock().unwrap();
        let state = &mut held.state;
        state.status = status.unwrap_or(state.status);
        if let Some(l) = &line {
            state.log.push(l.clone());
            let over = state.log.len().saturating_sub(KEPT);
            state.log.drain(..over);
        }
        let status = state.status;
        drop(held);
        let _ = ApEvent { status, line }.emit(app);
    }

    fn note(&self, app: &AppHandle, note: &Note) {
        let status = match note {
            Note::Connecting | Note::Lost(..) => Some(ApStatus::Connecting),
            Note::Connected => Some(ApStatus::Connected),
            Note::Refused(_) => Some(ApStatus::Refused),
            _ => None,
        };
        // The server's confirmations are for timings, not the page.
        let line =
            (!matches!(note, Note::Connecting | Note::Confirmed(_))).then(|| note.to_string());
        self.say(app, status, line);
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn ap_state(app: AppHandle) -> ApState {
    app.state::<Ap>().0.lock().unwrap().state.clone()
}

/// Connect the running game to `server` as the installed seed's slot. It
/// keeps reconnecting until [`ap_disconnect`], or a refusal.
#[tauri::command]
#[specta::specta]
pub(crate) async fn ap_connect(
    app: AppHandle,
    server: String,
    password: Option<String>,
) -> Res<()> {
    let handle = app.clone();
    let mut parts = blocking(move || {
        let config = Config::load()?;
        let gd = handle.state::<Live>().gamedata(&config)?;
        let target = live::Game::attach(&config.target()?)?;
        client::installed(&romfs(&config)?, gd, target)
    })
    .await?;
    let (send, stop) = channel();
    let run = app.state::<Ap>().start(send)?;
    let room = Room {
        server,
        slot: parts.slot.clone(),
        password: password.filter(|p| !p.is_empty()),
    };
    std::thread::spawn(move || {
        let ap = app.state::<Ap>();
        client::run(&room, &parts.book, &mut parts.game, &stop, &mut |n| {
            if ap.current(run) {
                ap.note(&app, &n);
            }
        });
        if ap.finish(run) == Some(false) {
            ap.say(&app, Some(ApStatus::Offline), Some("disconnected".into()));
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn ap_disconnect(app: AppHandle) {
    // Dropping the sender stops the client at its next look.
    app.state::<Ap>().0.lock().unwrap().stop = None;
}

/// This build's znx.apworld, or empty when it has none (see build.rs).
const APWORLD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/znx.apworld"));

/// Whether this build carries its znx.apworld. Release builds do.
#[tauri::command]
#[specta::specta]
pub(crate) fn apworld_bundled() -> bool {
    !APWORLD.is_empty()
}

/// Saves this build's znx.apworld to `path`.
#[tauri::command]
#[specta::specta]
pub(crate) fn save_apworld(path: String) -> Res<()> {
    if APWORLD.is_empty() {
        return Err("This build has no znx.apworld".into());
    }
    std::fs::write(&path, APWORLD).map_err(|e| format!("Couldn't save {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stopped_run_winding_down_leaves_the_next_one_alone() {
        let ap = Ap::default();
        let (send, _old) = channel();
        let old = ap.start(send).unwrap();
        let (send, _) = channel();
        assert!(ap.start(send).is_err(), "one run at a time");
        ap.0.lock().unwrap().stop = None;
        let (send, new_stop) = channel();
        let new = ap.start(send).unwrap();
        assert!(!ap.current(old) && ap.current(new));
        assert_eq!(ap.finish(old), None);
        assert!(
            ap.0.lock().unwrap().stop.is_some(),
            "the new run still runs"
        );
        drop(new_stop);
        assert_eq!(ap.finish(new), Some(false));
        assert!(ap.0.lock().unwrap().stop.is_none());
    }
}
