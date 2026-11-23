//! Where `log` lines go: the CLI's goes to stderr here; the app installs
//! its own. Only zootr's own crates log, not the libraries under them.

use log::{LevelFilter, Log, Metadata, Record};

/// Whether a line from `meta` is zootr's own and at `max` or above.
pub fn ours(meta: &Metadata, max: LevelFilter) -> bool {
    meta.level() <= max && meta.target().starts_with("zootr")
}

/// Warnings and errors, printed to stderr as plain lines.
struct Stderr;

/// The CLI's level: what it printed before it logged.
const CLI: LevelFilter = LevelFilter::Warn;

impl Log for Stderr {
    fn enabled(&self, meta: &Metadata) -> bool {
        ours(meta, CLI)
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{}", record.args());
        }
    }

    fn flush(&self) {}
}

/// Sends the CLI's warnings and errors to stderr. A logger already set
/// stays.
pub fn to_stderr() {
    if log::set_logger(&Stderr).is_ok() {
        log::set_max_level(CLI);
    }
}

#[cfg(test)]
mod tests {
    use log::Level;

    use super::*;

    fn meta(level: Level, target: &str) -> Metadata<'_> {
        Metadata::builder().level(level).target(target).build()
    }

    #[test]
    fn the_cli_prints_our_warnings_and_errors_only() {
        assert!(Stderr.enabled(&meta(Level::Warn, "zootr::known")));
        assert!(Stderr.enabled(&meta(Level::Error, "zootr_app")));
        assert!(!Stderr.enabled(&meta(Level::Info, "zootr::known")));
        assert!(!Stderr.enabled(&meta(Level::Warn, "tao::platform")));
    }
}
