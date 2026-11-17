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
pub(crate) struct Ap(Mutex<(Option<Sender<()>>, ApState)>);

impl Ap {
    /// Keep and send a status, and a line when there is one.
    fn say(&self, app: &AppHandle, status: Option<ApStatus>, line: Option<String>) {
        let mut held = self.0.lock().unwrap();
        let state = &mut held.1;
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
        let line = (!matches!(note, Note::Connecting)).then(|| note.to_string());
        self.say(app, status, line);
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn ap_state(app: AppHandle) -> ApState {
    app.state::<Ap>().0.lock().unwrap().1.clone()
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
    let ap = app.state::<Ap>();
    let (send, stop) = channel();
    {
        let mut held = ap.0.lock().unwrap();
        if held.0.is_some() {
            return Err("The client is already running".into());
        }
        held.0 = Some(send);
        held.1.log.clear();
    }
    let room = Room {
        server,
        slot: parts.slot.clone(),
        password: password.filter(|p| !p.is_empty()),
    };
    std::thread::spawn(move || {
        let ap = app.state::<Ap>();
        client::run(&room, &parts.book, &mut parts.game, &stop, &mut |n| {
            ap.note(&app, &n)
        });
        let refused = ap.0.lock().unwrap().1.status == ApStatus::Refused;
        ap.0.lock().unwrap().0 = None;
        if !refused {
            ap.say(&app, Some(ApStatus::Offline), Some("disconnected".into()));
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn ap_disconnect(app: AppHandle) {
    // Dropping the sender stops the client at its next look.
    app.state::<Ap>().0.lock().unwrap().0 = None;
}
