//! Recording for the app's Live tab: live reads when the game allows them,
//! autosave diffs otherwise. Every row goes to the app and to the session's
//! log file. [`Watch`] starts a session whenever the game runs.

use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;
use serde::Serialize;

use crate::config::{Config, Target};
use crate::gamedata::GameData;
use crate::live::{self, Game, Poll};
use crate::savefile::{self, Change, Watcher};

mod watch;
pub use watch::{Next, Watch, decide};

/// Why a session ends when the game does.
const CLOSED: &str = "the game closed";
/// Autosaves are looked at every this many ticks.
const SAVE_TICKS: u32 = 20;
/// Session logs kept.
const KEEP: usize = 20;
/// Events kept for a page that loads mid-session.
pub const CAP: usize = 2000;
/// How often [`Watch`] looks for the game.
const WATCH: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Source {
    Live,
    Saves,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Session {
    pub source: Source,
    /// Why live reads aren't used.
    pub reason: Option<String>,
    pub log: Option<PathBuf>,
    /// Why the log isn't being written.
    pub unsaved: Option<String>,
    /// Local time it started, `HH:MM`.
    pub started: String,
}

/// What recording is doing between sessions.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "state")]
pub enum LiveStatus {
    /// The game isn't running, or can't be read until it starts again, with
    /// the reason.
    Waiting {
        reason: Option<String>,
    },
    /// The game is running but live reads aren't ready yet.
    Attaching {
        reason: String,
    },
    Recording {
        session: Session,
    },
}

impl Default for LiveStatus {
    fn default() -> Self {
        Self::Waiting { reason: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type")]
pub enum Event {
    Started {
        session: Session,
    },
    /// A change `at` seconds into the session, from an autosave if `save`.
    Row {
        at: u64,
        save: bool,
        change: Change,
    },
    /// A value muted for changing too often, or by hand if `manual`.
    Muted {
        hash: u32,
        name: String,
        manual: bool,
    },
    /// A marker `at` seconds into the session, saying what happened.
    Marked {
        at: u64,
        text: String,
    },
    Unsaved {
        reason: String,
    },
    Ended {
        reason: String,
    },
    Status {
        status: LiveStatus,
    },
}

/// An event and where it falls in the stream, so a page that loads
/// mid-session can tell which events its snapshot already holds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Numbered {
    pub seq: u64,
    pub event: Event,
}

/// Everything a page needs to pick up where the stream is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Snapshot {
    pub status: LiveStatus,
    /// The newest [`CAP`] events, across sessions.
    pub events: Vec<Numbered>,
    /// Events that fell off the top.
    pub dropped: u64,
    /// The next event's number.
    pub next: u64,
}

/// Numbers events and keeps the newest, along with the status they lead to.
#[derive(Debug, Default)]
pub struct Backlog {
    events: VecDeque<Numbered>,
    dropped: u64,
    next: u64,
    status: LiveStatus,
}

impl Backlog {
    /// Numbers `event`, and follows it with a status event when it changes
    /// the status. A status that changes nothing is dropped.
    pub fn push(&mut self, event: Event) -> Vec<Numbered> {
        let status = match &event {
            Event::Status { status } => Some(status.clone()),
            Event::Started { session } => Some(LiveStatus::Recording {
                session: session.clone(),
            }),
            Event::Ended { .. } => Some(LiveStatus::default()),
            _ => None,
        }
        .filter(|s| *s != self.status);
        let mut out = Vec::new();
        if !matches!(event, Event::Status { .. }) {
            let n = self.number(event);
            self.events.push_back(n.clone());
            if self.events.len() > CAP {
                self.events.pop_front();
                self.dropped += 1;
            }
            out.push(n);
        }
        if let Some(status) = status {
            self.status = status.clone();
            out.push(self.number(Event::Status { status }));
        }
        out
    }

    fn number(&mut self, event: Event) -> Numbered {
        self.next += 1;
        Numbered {
            seq: self.next - 1,
            event,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            status: self.status.clone(),
            events: self.events.iter().cloned().collect(),
            dropped: self.dropped,
            next: self.next,
        }
    }
}

/// What the app asks of the running session.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Mark the log with what happened.
    Mark(String),
    /// Leave this value out for the rest of the session.
    Mute(u32),
}

/// What a session's asks come from.
type Asks = Box<dyn Fn() -> Vec<Ask> + Send>;

/// The [`Backlog`], where its events go as they're numbered, and what the
/// app has asked of the session.
pub struct Hub {
    backlog: Mutex<Backlog>,
    emit: Box<dyn Fn(&Numbered) + Send + Sync>,
    asks: Mutex<Vec<Ask>>,
}

impl Hub {
    pub fn new(emit: impl Fn(&Numbered) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            backlog: Mutex::default(),
            emit: Box::new(emit),
            asks: Mutex::default(),
        })
    }

    /// Emits while still locked, so events go out in number order.
    fn with(&self, f: impl FnOnce(&mut Backlog) -> Vec<Numbered>) {
        let mut backlog = self.backlog.lock().unwrap();
        f(&mut backlog).iter().for_each(&self.emit);
    }

    pub fn send(&self, event: Event) {
        self.with(|b| b.push(event));
    }

    pub fn snapshot(&self) -> Snapshot {
        self.backlog.lock().unwrap().snapshot()
    }

    /// Queues `ask` for the session, which takes it at its next tick.
    pub fn ask(&self, ask: Ask) -> Result<()> {
        let status = &self.backlog.lock().unwrap().status;
        if !matches!(status, LiveStatus::Recording { .. }) {
            bail!("Nothing is recording");
        }
        self.asks.lock().unwrap().push(ask);
        Ok(())
    }

    /// Takes what the session's been asked since it last looked.
    pub fn asks(self: &Arc<Self>) -> Asks {
        let hub = self.clone();
        Box::new(move || std::mem::take(&mut *hub.asks.lock().unwrap()))
    }

    /// Sends to this hub from a session's thread.
    pub fn sender(self: &Arc<Self>) -> impl Fn(Event) + Send + 'static {
        let hub = self.clone();
        move |e| hub.send(e)
    }
}

#[derive(Debug)]
pub enum Pick<G> {
    Live(G),
    Saves(PathBuf),
}

/// The game is gone, or goes within a second. Its memory can go before
/// its process does, so a failed read waits a moment before blaming itself.
fn closing(running: impl Fn() -> bool) -> bool {
    (0..10).any(|_| {
        let gone = !running();
        if !gone {
            std::thread::sleep(live::TICK);
        }
        gone
    })
}

/// What Live shows for `e`: its player wording, or its first line.
fn why(e: &anyhow::Error) -> String {
    match crate::plain::split(e) {
        Some((plain, _)) => plain.into(),
        None => e.to_string().lines().next().unwrap_or_default().into(),
    }
}

/// The game's GameData names, which take a while to gather.
pub fn gamedata(romfs: &Romfs) -> Result<GameData> {
    Ok(crate::flags::Survey::load(romfs, &crate::events::Flows::load(romfs)?)?.gamedata)
}

/// Where session logs go, next to the config file.
pub fn logs() -> Result<PathBuf> {
    Ok(Config::path()?.with_file_name("logs"))
}

/// A new `<kind>-<time>.log` in `dir`, keeping the [`KEEP`] newest of its
/// kind.
pub fn open(dir: &Path, kind: &str) -> Result<(PathBuf, File)> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let prefix = format!("{kind}-");
    let stamp = jiff::Zoned::now().strftime("%Y-%m-%d-%H%M%S.log");
    let path = dir.join(format!("{prefix}{stamp}"));
    let file = File::options()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("writing {}", path.display()))?;
    let prune = || -> Result<()> {
        let logs = std::fs::read_dir(dir)?
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                name.starts_with(&prefix) && name.ends_with(".log")
            });
        keep_newest(logs, KEEP)
    };
    // Pruning is housekeeping: an old log it can't remove (one still held
    // open, say) mustn't cost this session its own.
    if let Err(e) = prune() {
        log::warn!("pruning old logs in {}: {e:#}", dir.display());
    }
    Ok((path, file))
}

/// Remove all but the `keep` last of `paths` by name (their names start
/// with the time), files and folders alike. Each is tried, so one stuck
/// path never keeps the rest. The first failure is returned.
pub fn keep_newest(paths: impl Iterator<Item = PathBuf>, keep: usize) -> Result<()> {
    let mut old: Vec<PathBuf> = paths.collect();
    old.sort();
    let mut first = None;
    for p in &old[..old.len().saturating_sub(keep)] {
        let removed = match p.is_dir() {
            true => std::fs::remove_dir_all(p),
            false => std::fs::remove_file(p),
        };
        if let Err(e) = removed {
            first.get_or_insert(anyhow::Error::new(e).context(format!("removing {}", p.display())));
        }
    }
    first.map_or(Ok(()), Err)
}

type Rows = (Vec<Change>, Vec<u32>);

/// Where a session's rows go, and what it's asked.
struct Out<S> {
    send: S,
    asks: Asks,
    log: Option<File>,
    start: Instant,
    save: bool,
    gd: Arc<GameData>,
}

impl<S: Fn(Event)> Out<S> {
    fn write(&mut self, line: String) {
        let at = live::stamp(self.at());
        let Some(file) = &mut self.log else { return };
        if let Err(e) = writeln!(file, "{at} {line}") {
            self.log = None;
            (self.send)(Event::Unsaved {
                reason: e.to_string(),
            });
        }
    }

    fn at(&self) -> u64 {
        self.start.elapsed().as_secs()
    }

    fn rows(&mut self, (changes, muted): Rows) {
        for change in changes {
            self.write(format!("{}{change}", if self.save { "save " } else { "" }));
            let (at, save) = (self.at(), self.save);
            (self.send)(Event::Row { at, save, change });
        }
        for hash in muted {
            let name = self.gd.name(hash);
            (self.send)(Event::Muted {
                hash,
                name,
                manual: false,
            });
        }
    }

    /// Does what the app asked. A hand mute joins `muted`, which every
    /// later read leaves out.
    fn ask(&mut self, ask: Ask, muted: &mut HashSet<u32>) {
        match ask {
            Ask::Mark(text) => {
                let text = match text.trim() {
                    "" => "Mark".to_string(),
                    t => t.to_string(),
                };
                self.write(format!("mark: {text}"));
                let at = self.at();
                (self.send)(Event::Marked { at, text });
            }
            Ask::Mute(hash) if muted.insert(hash) => {
                let name = self.gd.name(hash);
                self.write(format!("muted: {name}"));
                (self.send)(Event::Muted {
                    hash,
                    name,
                    manual: true,
                });
            }
            Ask::Mute(_) => {}
        }
    }

    fn end(&mut self, reason: String) {
        self.write(format!("ended: {reason}"));
        (self.send)(Event::Ended { reason });
    }
}

/// Calls `next` every `every` ticks until `stop` is set or it fails, and
/// then ends the session with `why` it failed. Takes the app's asks every
/// tick, and hands `next` the values muted by hand.
fn record<S: Fn(Event)>(
    mut next: impl FnMut(&HashSet<u32>) -> Result<Rows>,
    every: u32,
    out: &mut Out<S>,
    stop: &AtomicBool,
    why: impl Fn(anyhow::Error) -> String,
) {
    let mut muted = HashSet::new();
    for tick in 1.. {
        std::thread::sleep(live::TICK);
        if stop.load(Ordering::Relaxed) {
            return out.end("stopped".into());
        }
        for ask in (out.asks)() {
            out.ask(ask, &mut muted);
        }
        if tick % every != 0 {
            continue;
        }
        match next(&muted) {
            Ok(rows) => out.rows(rows),
            Err(e) => return out.end(why(e)),
        }
    }
}

/// A running session. Dropping it stops the session.
pub struct Recorder {
    pub session: Session,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Recorder {
    /// Records from `pick`, which [`Watch`] chooses.
    fn run(
        pick: Pick<Poll<Game>>,
        reason: Option<String>,
        target: &Target,
        gd: Arc<GameData>,
        logs: &Path,
        send: impl Fn(Event) + Send + 'static,
        asks: Asks,
    ) -> Self {
        let source = match pick {
            Pick::Live(_) => Source::Live,
            Pick::Saves(_) => Source::Saves,
        };
        let mut session = Session {
            source,
            reason,
            log: None,
            unsaved: None,
            started: jiff::Zoned::now().strftime("%H:%M").to_string(),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let log = match open(logs, "live") {
            Ok((path, file)) => {
                session.log = Some(path);
                Some(file)
            }
            Err(e) => {
                session.unsaved = Some(format!("{e:#}"));
                None
            }
        };
        send(Event::Started {
            session: session.clone(),
        });
        // Anything asked before this session is for one that's gone.
        asks();
        let mut out = Out {
            send,
            asks,
            log,
            start: Instant::now(),
            save: source == Source::Saves,
            gd: gd.clone(),
        };
        let (flag, target) = (stop.clone(), target.clone());
        let thread = std::thread::spawn(move || match pick {
            Pick::Live(mut poll) => {
                let why = |e: anyhow::Error| {
                    if closing(|| target.running()) {
                        CLOSED.into()
                    } else {
                        format!("reading the game failed: {e:#}")
                    }
                };
                let next = |muted: &HashSet<u32>| poll.next(&gd, false, |h| !muted.contains(&h));
                record(next, 1, &mut out, &flag, why);
            }
            Pick::Saves(dir) => {
                let mut watcher = Watcher::default();
                let next = |muted: &HashSet<u32>| {
                    if !target.running() {
                        bail!(CLOSED);
                    }
                    let wanted = |e: ounce_formats::SaveEntry| !muted.contains(&e.hash);
                    let looked = watcher.look(&gd, &savefile::files(&dir)?, wanted);
                    Ok((looked.into_iter().flat_map(|(_, c)| c).collect(), vec![]))
                };
                record(next, SAVE_TICKS, &mut out, &flag, |e| format!("{e:#}"));
            }
        });
        Self {
            session,
            stop,
            thread: Some(thread),
        }
    }

    /// The session ended on its own.
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-rec-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_session_logs_its_rows_and_why_it_ended() {
        let dir = scratch("log");
        let (path, log) = open(&dir, "live").unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sent = events.clone();
        let gd = Arc::new(GameData::default());
        let mut out = Out {
            send: move |e| sent.lock().unwrap().push(e),
            asks: Box::new(Vec::new),
            log: Some(log),
            start: Instant::now(),
            save: true,
            gd: gd.clone(),
        };
        let mut left = vec![(vec![Change::new(&gd, 1, "Int", "1 -> 2")], vec![2])];
        let next = |_: &HashSet<u32>| match left.pop() {
            Some(rows) => Ok(rows),
            None => bail!("gone"),
        };
        record(next, 1, &mut out, &AtomicBool::new(false), |e| {
            e.to_string()
        });
        let kinds: Vec<_> = events
            .lock()
            .unwrap()
            .iter()
            .map(|e| match e {
                Event::Row { save, .. } => format!("row {save}"),
                Event::Muted { name, .. } => format!("muted {name}"),
                Event::Unsaved { .. } => "unsaved".into(),
                Event::Ended { reason } => format!("ended {reason}"),
                e => format!("{e:?}"),
            })
            .collect();
        assert_eq!(kinds, ["row true", "muted #00000002", "ended gone"]);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = text.lines().map(|l| &l[8..]).collect();
        assert!(lines[0].starts_with("save #00000001 ") && lines[0].ends_with("1 -> 2"));
        assert_eq!(lines[1], "ended: gone");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_session_marks_and_mutes_by_hand_when_asked() {
        let dir = scratch("asks");
        let (path, log) = open(&dir, "live").unwrap();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let events = sent.clone();
        let asked = Mutex::new(vec![vec![
            Ask::Mark("  ".into()),
            Ask::Mute(7),
            Ask::Mute(7),
        ]]);
        let gd = Arc::new(GameData::default());
        let mut out = Out {
            send: move |e| events.lock().unwrap().push(e),
            asks: Box::new(move || asked.lock().unwrap().pop().unwrap_or_default()),
            log: Some(log),
            start: Instant::now(),
            save: false,
            gd: gd.clone(),
        };
        let mut reads = 0;
        let next = |muted: &HashSet<u32>| {
            reads += 1;
            assert!(muted.contains(&7), "a hand mute reaches the read");
            match reads {
                1 => Ok((vec![Change::new(&gd, 1, "Int", "1 -> 2")], vec![])),
                _ => bail!("gone"),
            }
        };
        record(next, 1, &mut out, &AtomicBool::new(false), |e| {
            e.to_string()
        });
        let kinds: Vec<_> = sent
            .lock()
            .unwrap()
            .iter()
            .map(|e| match e {
                Event::Marked { text, .. } => format!("marked {text}"),
                Event::Muted { name, manual, .. } => format!("muted {name} {manual}"),
                Event::Row { .. } => "row".into(),
                Event::Ended { reason } => format!("ended {reason}"),
                e => format!("{e:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            ["marked Mark", "muted #00000007 true", "row", "ended gone"]
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = text.lines().map(|l| &l[8..]).collect();
        assert_eq!(lines[..2], ["mark: Mark", "muted: #00000007"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_hub_takes_asks_only_while_recording() {
        let hub = Hub::new(|_| {});
        let err = hub.ask(Ask::Mute(1)).unwrap_err();
        assert_eq!(err.to_string(), "Nothing is recording");
        let session = Session {
            source: Source::Live,
            reason: None,
            log: None,
            unsaved: None,
            started: "12:00".into(),
        };
        hub.send(Event::Started { session });
        hub.ask(Ask::Mark("door".into())).unwrap();
        let asks = hub.asks();
        assert_eq!(asks(), [Ask::Mark("door".into())]);
        assert!(asks().is_empty());
    }

    #[test]
    fn stopping_ends_a_session_without_reading() {
        let sent = Mutex::new(Vec::new());
        let mut out = Out {
            send: |e| sent.lock().unwrap().push(e),
            asks: Box::new(Vec::new),
            log: None,
            start: Instant::now(),
            save: false,
            gd: Arc::default(),
        };
        let next = |_: &HashSet<u32>| -> Result<Rows> { bail!("never read") };
        record(next, 1, &mut out, &AtomicBool::new(true), |e| e.to_string());
        let reason = "stopped".into();
        assert_eq!(sent.into_inner().unwrap(), [Event::Ended { reason }]);
    }

    #[test]
    fn a_read_that_fails_as_the_game_exits_blames_the_exit() {
        let left = std::cell::Cell::new(3);
        assert!(closing(|| {
            left.set(left.get() - 1);
            left.get() > 0
        }));
        assert!(!closing(|| true));
    }

    fn session() -> Session {
        Session {
            source: Source::Live,
            reason: None,
            log: None,
            unsaved: None,
            started: "22:51".into(),
        }
    }

    #[test]
    fn the_backlog_numbers_events_and_follows_the_status() {
        let mut b = Backlog::default();
        let seqs = |ns: Vec<Numbered>| ns.iter().map(|n| n.seq).collect::<Vec<_>>();
        assert_eq!(b.snapshot().status, LiveStatus::Waiting { reason: None });
        let waiting = Event::Status {
            status: LiveStatus::Waiting { reason: None },
        };
        assert!(b.push(waiting).is_empty());
        let started = b.push(Event::Started { session: session() });
        assert_eq!(seqs(started), [0, 1]);
        let session = session();
        assert_eq!(b.snapshot().status, LiveStatus::Recording { session });
        let reason = "the game closed".into();
        assert_eq!(seqs(b.push(Event::Ended { reason })), [2, 3]);
        let snap = b.snapshot();
        assert_eq!(snap.status, LiveStatus::Waiting { reason: None });
        assert_eq!((seqs(snap.events), snap.next), (vec![0, 2], 4));
    }

    #[test]
    fn the_backlog_keeps_the_newest_events() {
        let mut b = Backlog::default();
        for _ in 0..CAP + 5 {
            b.push(Event::Unsaved { reason: "x".into() });
        }
        let snap = b.snapshot();
        assert_eq!((snap.events.len(), snap.dropped), (CAP, 5));
        assert_eq!(snap.events[0].seq, 5);
    }

    #[test]
    fn an_autosave_session_ends_when_the_game_closes() {
        let dir = scratch("closed");
        let saves = dir.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        let target = Target {
            name: "gone".into(),
            app: dir.join("never-running.app"),
            mod_dir: dir.join("mods"),
            settings_file: None,
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
            rebuild: None,
            game: None,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let send = move |e| {
            let _ = tx.send(e);
        };
        let rec = Recorder::run(
            Pick::Saves(saves),
            None,
            &target,
            Arc::default(),
            &dir,
            send,
            Box::new(Vec::new),
        );
        assert_eq!(rec.session.source, Source::Saves);
        let started = rx.recv().unwrap();
        assert!(matches!(started, Event::Started { .. }));
        let ended = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let reason = CLOSED.into();
        assert_eq!(ended, Event::Ended { reason });
        drop(rec);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An old log that can't be removed leaves the new one written all the
    /// same.
    #[cfg(unix)]
    #[test]
    fn a_failed_prune_keeps_the_new_log() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("prune");
        let held = dir.join("live-2000-01-01-000000.log").join("held");
        std::fs::create_dir_all(&held).unwrap();
        std::fs::write(held.join("file"), "").unwrap();
        for i in 1..=KEEP {
            std::fs::write(dir.join(format!("live-2000-01-01-{i:06}.log")), "").unwrap();
        }
        let mode = |m| std::fs::set_permissions(&held, std::fs::Permissions::from_mode(m)).unwrap();
        mode(0o555);
        let opened = open(&dir, "live");
        mode(0o755);
        assert!(opened.unwrap().0.exists());
        // The stuck log doesn't keep the next oldest.
        assert!(!dir.join("live-2000-01-01-000001.log").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_path_that_wont_go_neither_stops_the_rest_nor_hides() {
        let dir = scratch("stuck");
        std::fs::create_dir_all(&dir).unwrap();
        let log = |i| dir.join(format!("live-{i}.log"));
        (1..=2).for_each(|i| std::fs::write(log(i), "").unwrap());
        // Gone before it's removed, as a stuck one fails.
        let paths = [log(0), log(1), log(2)].into_iter();
        let err = format!("{:#}", keep_newest(paths, 1).unwrap_err());
        assert!(err.contains("live-0.log"), "{err}");
        assert!(!log(1).exists() && log(2).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_newest_logs_survive() {
        let dir = scratch("keep");
        std::fs::create_dir_all(&dir).unwrap();
        let old = |i| dir.join(format!("live-2000-01-01-{i:06}.log"));
        (0..25).for_each(|i| std::fs::write(old(i), "").unwrap());
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        let (path, _) = open(&dir, "live").unwrap();
        assert!(path.exists() && dir.join("notes.txt").exists());
        assert!(!old(5).exists() && old(6).exists());
        let logs = std::fs::read_dir(&dir).unwrap().count() - 1;
        assert_eq!(logs, KEEP);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
