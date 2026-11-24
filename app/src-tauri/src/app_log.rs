//! The app's own log: what it does and what fails. Each line goes to
//! `app-<time>.log` beside the generate and live logs, to the Logs page
//! as it comes, and into a backlog the page picks up after a reload. The
//! home folder reads `~`, so a player can post it with a bug report.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use log::{LevelFilter, Log, Metadata, Record};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event as _;
use zootr::config::{self, Config};
use zootr::recorder;

/// The newest lines kept for the page.
const CAP: usize = 2000;

/// How long a line the same as one already logged is skipped, so a
/// polled command failing every poll logs once a minute.
const REPEAT: Duration = Duration::from_secs(60);

/// How bad a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl From<log::Level> for Level {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => Self::Error,
            log::Level::Warn => Self::Warn,
            log::Level::Info => Self::Info,
            log::Level::Debug | log::Level::Trace => Self::Debug,
        }
    }
}

impl From<Level> for log::Level {
    fn from(level: Level) -> Self {
        match level {
            Level::Error => Self::Error,
            Level::Warn => Self::Warn,
            Level::Info => Self::Info,
            Level::Debug => Self::Debug,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        };
        f.pad(name)
    }
}

/// One line of the log, numbered so a page that loads mid-run can tell
/// which lines its snapshot already holds.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub(crate) struct Line {
    pub(crate) seq: u32,
    /// The local time, as `HH:MM:SS`.
    pub(crate) time: String,
    pub(crate) level: Level,
    pub(crate) text: String,
}

/// Each line, to whichever page is listening.
#[derive(Clone, Serialize, Type)]
pub(crate) struct AppLogEvent(Line);

impl tauri_specta::Event for AppLogEvent {
    const NAME: &'static str = "app-log";
}

/// Where the Logs page picks up after a reload.
#[derive(Serialize, Type)]
pub(crate) struct AppLogState {
    /// The newest [`CAP`] lines, oldest first.
    lines: Vec<Line>,
    /// This run's file.
    file: Option<PathBuf>,
    /// Why there's no file.
    unsaved: Option<String>,
}

/// The lines so far, and what's needed to add one.
#[derive(Default)]
struct Book {
    lines: VecDeque<Line>,
    next: u32,
    /// When each repeat key was last logged.
    seen: HashMap<(Level, String), Instant>,
    file: Option<File>,
    /// The home folder, written `~`.
    home: Option<String>,
}

impl Book {
    /// Adds a line and writes it to the file, unless the same `key` (the
    /// text, when none) was logged within [`REPEAT`] of `now`.
    fn push(
        &mut self,
        level: Level,
        text: &str,
        key: Option<&str>,
        now: Instant,
        time: String,
    ) -> Option<Line> {
        let seen = (level, key.unwrap_or(text).to_owned());
        if (self.seen.get(&seen)).is_some_and(|&at| now.duration_since(at) < REPEAT) {
            return None;
        }
        if self.seen.len() > CAP {
            self.seen
                .retain(|_, &mut at| now.duration_since(at) < REPEAT);
        }
        self.seen.insert(seen, now);
        let text = match &self.home {
            Some(h) => text.replace(h.as_str(), "~"),
            None => text.to_owned(),
        };
        let line = Line {
            seq: self.next,
            time,
            level,
            text,
        };
        self.next += 1;
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{}  {:<5}  {}", line.time, line.level, line.text);
        }
        if self.lines.len() == CAP {
            self.lines.pop_front();
        }
        self.lines.push_back(line.clone());
        Some(line)
    }
}

/// The app's logger, installed once by [`install`].
pub(crate) struct AppLog {
    book: Mutex<Book>,
    file: Option<PathBuf>,
    unsaved: Option<String>,
    /// The window, once it's up, to send lines to.
    app: OnceLock<AppHandle>,
}

static LOG: OnceLock<AppLog> = OnceLock::new();

impl AppLog {
    /// A logger writing a new `app-<time>.log`, or none it can't open.
    fn open() -> Self {
        Self::new(recorder::logs().and_then(|dir| recorder::open(&dir, "app")))
    }

    /// A logger writing the file `opened`, or saying why there's none.
    fn new(opened: anyhow::Result<(PathBuf, File)>) -> Self {
        let home = config::home().map(|h| h.to_string_lossy().into_owned());
        let (file, unsaved, out) = match opened {
            Ok((path, file)) => (Some(path), None, Some(file)),
            Err(e) => (None, Some(format!("{e:#}")), None),
        };
        Self {
            book: Mutex::new(Book {
                file: out,
                home: home.filter(|h| !h.is_empty()),
                ..Book::default()
            }),
            file,
            unsaved,
            app: OnceLock::new(),
        }
    }

    /// Adds a line, then sends it to the page outside the lock.
    fn push(&self, level: Level, text: &str, key: Option<&str>) {
        let time = jiff::Zoned::now().strftime("%H:%M:%S").to_string();
        let line = (self.book.lock().unwrap()).push(level, text, key, Instant::now(), time);
        if let (Some(line), Some(app)) = (line, self.app.get()) {
            let _ = AppLogEvent(line).emit(app);
        }
    }
}

impl Log for AppLog {
    fn enabled(&self, meta: &Metadata) -> bool {
        zootr::logging::ours(meta, log::max_level())
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            self.push(record.level().into(), &record.args().to_string(), None);
        }
    }

    fn flush(&self) {
        if let Some(file) = &mut self.book.lock().unwrap().file {
            let _ = file.flush();
        }
    }
}

/// Starts the app's log, at debug with Developer Tools on, with every
/// panic in it, where it happened too. A logger already set stays.
pub(crate) fn install(dev_tools: bool) {
    start(AppLog::open, dev_tools);
    let shown = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("{info}");
        shown(info);
    }));
}

/// Starts a log with no file, for tests that read what was logged.
#[cfg(test)]
pub(crate) fn install_bare() {
    start(|| AppLog::new(Err(anyhow::anyhow!("a test"))), false);
}

fn start(open: impl FnOnce() -> AppLog, dev_tools: bool) {
    if log::set_logger(LOG.get_or_init(open)).is_ok() {
        set_level(dev_tools);
    }
}

/// Info and above, and debug too with Developer Tools on.
pub(crate) fn set_level(dev_tools: bool) {
    log::set_max_level(match dev_tools {
        true => LevelFilter::Debug,
        false => LevelFilter::Info,
    });
}

/// Sends each line to the window from now on.
pub(crate) fn attach(app: &AppHandle) {
    if let Some(log) = LOG.get() {
        let _ = log.app.set(app.clone());
    }
}

/// The run's first lines: what it runs on, and with which game and target.
pub(crate) fn header(config: &anyhow::Result<Config>) {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    log::info!("zootr {} on {os} {arch}", env!("CARGO_PKG_VERSION"));
    let config = match config {
        Ok(c) => c,
        Err(e) => return log::warn!("reading the config: {e:#}"),
    };
    match &config.game {
        Some(game) => log::info!("dump {}", game.display()),
        None => log::info!("no game dump set"),
    }
    if let Ok(t) = config.target() {
        log::info!("target {}: {}", t.name, t.mod_dir.display());
    }
}

/// Where the Logs page picks up after a reload.
#[tauri::command]
#[specta::specta]
pub(crate) fn app_log_state() -> AppLogState {
    let Some(log) = LOG.get() else {
        return AppLogState {
            lines: Vec::new(),
            file: None,
            unsaved: Some("The log didn't start".into()),
        };
    };
    AppLogState {
        lines: log.book.lock().unwrap().lines.iter().cloned().collect(),
        file: log.file.clone(),
        unsaved: log.unsaved.clone(),
    }
}

/// Logs a line from the page, skipped as a repeat of the last line with
/// the same `key` (its text, when none) within a minute.
#[tauri::command]
#[specta::specta]
pub(crate) fn app_log_write(level: Level, text: String, key: Option<String>) {
    if let Some(log) = LOG.get()
        && log::Level::from(level) <= log::max_level()
    {
        log.push(level, &text, key.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Book {
        Book {
            home: Some("/home/fake".into()),
            ..Book::default()
        }
    }

    fn push(b: &mut Book, text: &str, now: Instant) -> Option<Line> {
        b.push(Level::Info, text, None, now, "12:00:00".into())
    }

    #[test]
    fn lines_are_numbered_and_the_home_folder_reads_tilde() {
        let (mut b, now) = (book(), Instant::now());
        push(&mut b, "a", now);
        let line = push(&mut b, "dump /home/fake/oot.nup", now).unwrap();
        assert_eq!(line.seq, 1);
        assert_eq!(line.text, "dump ~/oot.nup");
    }

    #[test]
    fn the_backlog_keeps_the_newest_lines() {
        let (mut b, now) = (book(), Instant::now());
        for i in 0..CAP + 5 {
            push(&mut b, &i.to_string(), now);
        }
        assert_eq!(b.lines.len(), CAP);
        assert_eq!(b.lines.front().unwrap().text, "5");
        assert_eq!(b.lines.back().unwrap().seq as usize, CAP + 4);
    }

    #[test]
    fn a_repeat_waits_a_minute() {
        let (mut b, now) = (book(), Instant::now());
        assert!(push(&mut b, "no game", now).is_some());
        assert!(push(&mut b, "no game", now + REPEAT / 2).is_none());
        assert!(push(&mut b, "other", now + REPEAT / 2).is_some());
        assert!(push(&mut b, "no game", now + REPEAT).is_some());
    }

    #[test]
    fn a_key_stands_for_its_text_in_repeats() {
        let (mut b, now) = (book(), Instant::now());
        let mut timed = |text: &str| b.push(Level::Debug, text, Some("status ok"), now, "".into());
        assert!(timed("→ status 0.01 s").is_some());
        assert!(timed("→ status 0.02 s").is_none());
    }

    #[test]
    fn each_line_goes_to_the_file() {
        let dir = std::env::temp_dir().join(format!("zootr-applog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (path, file) = recorder::open(&dir, "app").unwrap();
        let mut b = Book {
            file: Some(file),
            ..book()
        };
        b.push(
            Level::Warn,
            "x at /home/fake/a",
            None,
            Instant::now(),
            "12:00:00".into(),
        );
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(text, "12:00:00  warn   x at ~/a\n");
    }
}
