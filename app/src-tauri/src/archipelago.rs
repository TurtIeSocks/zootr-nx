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

/// A status change or a log line, or the page's log started over.
#[derive(Clone, Serialize, Type)]
pub(crate) struct ApEvent {
    status: ApStatus,
    line: Option<String>,
    /// The log so far is another seed's: drop it before taking `line`.
    clear: bool,
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

    /// Applies `change` to what's held and sends the event it returns. The
    /// change and the send share one lock, so a run's check that it's still
    /// the latest can't go stale before its change lands.
    fn apply(&self, app: &AppHandle, change: impl FnOnce(&mut Held) -> Option<ApEvent>) {
        let mut held = self.0.lock().unwrap();
        if let Some(event) = change(&mut held) {
            let _ = event.emit(app);
        }
    }

    /// The installed seed changed: stop the running client and start the page
    /// over, so the new seed's panel never shows the old slot's run.
    pub(crate) fn reset(&self, app: &AppHandle) {
        self.apply(app, |held| Some(held.reset()));
    }

    fn note(&self, app: &AppHandle, run: u64, note: &Note) {
        let status = match note {
            Note::Connecting | Note::Lost(..) => Some(ApStatus::Connecting),
            Note::Connected => Some(ApStatus::Connected),
            Note::Refused(_) => Some(ApStatus::Refused),
            _ => None,
        };
        // The server's confirmations are for timings, not the page.
        let line =
            (!matches!(note, Note::Connecting | Note::Confirmed(_))).then(|| note.to_string());
        self.apply(app, |held| held.say(run, status, line));
    }
}

impl Held {
    /// Keep `run`'s status, and a line when there is one, and the event that
    /// sends them. `None` when a newer run owns the page now.
    fn say(&mut self, run: u64, status: Option<ApStatus>, line: Option<String>) -> Option<ApEvent> {
        if self.run != run {
            return None;
        }
        let state = &mut self.state;
        state.status = status.unwrap_or(state.status);
        if let Some(l) = &line {
            state.log.push(l.clone());
            let over = state.log.len().saturating_sub(KEPT);
            state.log.drain(..over);
        }
        Some(ApEvent {
            status: state.status,
            line,
            clear: false,
        })
    }

    /// Stop the client and forget its run. A new run number leaves the old
    /// thread, still winding down, unable to write to the page.
    fn reset(&mut self) -> ApEvent {
        self.stop = None;
        self.run += 1;
        self.state = ApState::default();
        ApEvent {
            status: ApStatus::Offline,
            line: None,
            clear: true,
        }
    }

    /// `run` ended: it reads as disconnected unless it was refused. `None`
    /// when there's nothing to send, or a newer run owns the page now.
    fn end(&mut self, run: u64) -> Option<ApEvent> {
        if self.run != run {
            return None;
        }
        self.stop = None;
        if self.state.status == ApStatus::Refused {
            return None;
        }
        self.say(run, Some(ApStatus::Offline), Some("disconnected".into()))
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
            ap.note(&app, run, &n);
        });
        ap.apply(&app, |held| held.end(run));
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
        let (send, _new_stop) = channel();
        let new = ap.start(send).unwrap();
        let mut held = ap.0.lock().unwrap();
        assert!(
            held.say(new, Some(ApStatus::Connected), Some("in".into()))
                .is_some()
        );
        assert!(
            held.say(old, Some(ApStatus::Connecting), Some("lost".into()))
                .is_none()
        );
        assert!(held.end(old).is_none());
        assert!(held.stop.is_some(), "the new run still runs");
        assert!(held.state.status == ApStatus::Connected);
        assert_eq!(held.state.log, ["in"]);
        let ended = held.end(new).expect("an ended run says so");
        assert!(ended.status == ApStatus::Offline);
        assert_eq!(ended.line.as_deref(), Some("disconnected"));
        assert!(held.stop.is_none());
    }

    #[test]
    fn a_new_install_stops_the_run_and_clears_the_page() {
        let ap = Ap::default();
        let (send, stop) = channel();
        let run = ap.start(send).unwrap();
        let mut held = ap.0.lock().unwrap();
        held.say(run, Some(ApStatus::Refused), Some("refused".into()));
        let event = held.reset();
        assert!(event.clear && event.status == ApStatus::Offline);
        assert!(held.stop.is_none(), "the sender is dropped");
        assert!(stop.recv().is_err(), "the client sees the stop");
        assert!(held.state.status == ApStatus::Offline);
        assert!(held.state.log.is_empty());
        assert!(held.say(run, Some(ApStatus::Connected), None).is_none());
        assert!(held.end(run).is_none(), "the old run can't write back");
        drop(held);
        let (send, _stop) = channel();
        assert!(ap.start(send).is_ok(), "a new seed can connect");
    }

    #[test]
    fn a_refused_run_ends_without_a_disconnect() {
        let ap = Ap::default();
        let (send, _stop) = channel();
        let run = ap.start(send).unwrap();
        let mut held = ap.0.lock().unwrap();
        held.say(run, Some(ApStatus::Refused), Some("refused".into()));
        assert!(held.end(run).is_none());
        assert!(held.stop.is_none());
        assert!(held.state.status == ApStatus::Refused);
    }
}
