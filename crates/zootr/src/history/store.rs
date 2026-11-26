//! Where [`super::History`] lives on disk: a small index, and each seed's
//! report in a file of its own, packed with zstd. A report is written
//! once and read when asked for, so a mark or a label never rewrites
//! megabytes of spots.
//!
//! ```text
//! history/
//!   index.json        the history without reports
//!   reports/<id>.json.zst
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde_json::Value;

use super::Brief;
use crate::config::Config;
use crate::generate::{Report, Spot};

/// A report file this new may belong to a seed another process is still
/// adding (its report written, its index not yet), so a sweep spares it.
const SETTLING: Duration = Duration::from_secs(60);

/// zstd's level for reports: a report packs from about 200 KB to 13 KB.
const LEVEL: i32 = 3;

/// The history's folder, beside the config.
pub(super) fn dir() -> Result<PathBuf> {
    Ok(Config::path()?.with_file_name("history"))
}

pub(super) fn index(dir: &Path) -> PathBuf {
    dir.join("index.json")
}

fn reports(dir: &Path) -> PathBuf {
    dir.join("reports")
}

fn report_path(dir: &Path, id: u32) -> PathBuf {
    reports(dir).join(format!("{id}.json.zst"))
}

/// The history before reports got files of their own: one `history.json`
/// beside the folder.
fn old_file(dir: &Path) -> PathBuf {
    dir.with_extension("json")
}

/// Seed `id`'s report, with readable names for a seed saved before they
/// were kept.
pub(super) fn read_report(dir: &Path, id: u32) -> Result<Report> {
    let path = report_path(dir, id);
    let read = || -> Result<Report> {
        let json = zstd::decode_all(std::fs::File::open(&path)?)?;
        let mut report: Report = serde_json::from_slice(&json)?;
        report.spots.iter_mut().for_each(Spot::fill_names);
        Ok(report)
    };
    read().with_context(|| format!("reading {}", path.display()))
}

pub(super) fn write_report(dir: &Path, id: u32, report: &Report) -> Result<()> {
    let path = report_path(dir, id);
    let packed = zstd::encode_all(&serde_json::to_vec(report)?[..], LEVEL)?;
    crate::known::keep(&path, packed).with_context(|| format!("writing {}", path.display()))
}

pub(super) fn write_index(dir: &Path, json: &[u8]) -> Result<()> {
    let path = index(dir);
    crate::known::keep(&path, json).with_context(|| format!("writing {}", path.display()))
}

/// Removes the report files no entry in `ids` names, but a [`SETTLING`]
/// one. Each is tried, so one stuck file never keeps the rest.
pub(super) fn sweep(dir: &Path, ids: &[u32]) -> Result<()> {
    let listed = match std::fs::read_dir(reports(dir)) {
        Ok(listed) => listed,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).context("listing the history's reports"),
    };
    let now = SystemTime::now();
    let mut first = None;
    for file in listed.flatten() {
        let path = file.path();
        let id = (path.file_name().and_then(|n| n.to_str()))
            .and_then(|n| n.strip_suffix(".json.zst"))
            .and_then(|n| n.parse::<u32>().ok());
        let Some(id) = id.filter(|id| !ids.contains(id)) else {
            continue;
        };
        let modified = file.metadata().and_then(|m| m.modified());
        let age = modified.map_or(Duration::ZERO, |m| {
            now.duration_since(m).unwrap_or_default()
        });
        if age < SETTLING {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            let e = anyhow::Error::new(e).context(format!("removing report {id}"));
            first.get_or_insert(e);
        }
    }
    first.map_or(Ok(()), Err)
}

/// Moves an old `history.json` into `dir` once: every report to its
/// file, then the index, then the old file renamed `history.json.bak`.
/// An older app then finds no history rather than one it can't read.
pub(super) fn move_old(dir: &Path) -> Result<()> {
    let old = old_file(dir);
    if index(dir).exists() || !old.exists() {
        return Ok(());
    }
    let mut json: Value = serde_json::from_slice(&std::fs::read(&old)?)
        .with_context(|| format!("reading {}", old.display()))?;
    let entries = (json.get_mut("entries").and_then(Value::as_array_mut)).into_iter();
    for entry in entries.flatten() {
        let map = entry
            .as_object_mut()
            .context("a history entry isn't a map")?;
        let id: u32 = serde_json::from_value(map["id"].clone())?;
        let report = map
            .remove("report")
            .context("a history entry has no report")?;
        let report: Report = serde_json::from_value(report)
            .with_context(|| format!("reading seed {id}'s report"))?;
        write_report(dir, id, &report)?;
        map.insert("brief".into(), serde_json::to_value(Brief::from(&report))?);
    }
    write_index(dir, &serde_json::to_vec(&json)?)?;
    let bak = old.with_extension("json.bak");
    std::fs::rename(&old, &bak).with_context(|| format!("renaming {}", old.display()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::history::tests::report;

    /// An empty folder for one test, `name` keeping tests apart.
    pub(crate) fn temp(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("zootr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root.join("history")
    }

    /// [`report`] as reading it back gives it: its names filled.
    pub(crate) fn filled(seed: u64) -> Report {
        let mut report = report(seed);
        report.spots.iter_mut().for_each(Spot::fill_names);
        report
    }

    /// Backdates `path` past [`SETTLING`].
    fn settle(path: &Path) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - SETTLING * 2).unwrap();
    }

    #[test]
    fn a_report_round_trips_packed() {
        let dir = temp("report-round-trip");
        write_report(&dir, 3, &report(7)).unwrap();
        assert_eq!(read_report(&dir, 3).unwrap(), filled(7));
        let packed = std::fs::metadata(report_path(&dir, 3)).unwrap().len();
        let plain = serde_json::to_vec(&report(7)).unwrap().len() as u64;
        assert!(packed < plain, "{packed} packed, {plain} plain");
        assert!(read_report(&dir, 4).is_err());
    }

    #[test]
    fn a_sweep_removes_unnamed_settled_reports_only() {
        let dir = temp("sweep");
        for id in 1..=3 {
            write_report(&dir, id, &report(7)).unwrap();
        }
        settle(&report_path(&dir, 1));
        settle(&report_path(&dir, 2));
        std::fs::write(reports(&dir).join("notes.txt"), "not a report").unwrap();
        sweep(&dir, &[2]).unwrap();
        assert!(!report_path(&dir, 1).exists(), "unnamed and settled");
        assert!(report_path(&dir, 2).exists(), "named");
        assert!(report_path(&dir, 3).exists(), "still settling");
        assert!(reports(&dir).join("notes.txt").exists());
        sweep(&temp("sweep-empty"), &[]).unwrap();
    }

    #[test]
    fn an_old_history_moves_into_the_folder_once() {
        let dir = temp("move-old");
        let old = serde_json::json!({
            "installed": 2,
            "settings_version": 2,
            "entries": [
                { "id": 1, "time": 5, "settings": {}, "report": report(7) },
                { "id": 2, "time": 6, "settings": {}, "name": "Ice", "report": report(9) },
            ],
        });
        std::fs::write(old_file(&dir), old.to_string()).unwrap();
        move_old(&dir).unwrap();
        assert!(!old_file(&dir).exists());
        assert!(dir.with_extension("json.bak").exists());
        assert_eq!(read_report(&dir, 2).unwrap(), filled(9));
        let index: Value = serde_json::from_slice(&std::fs::read(index(&dir)).unwrap()).unwrap();
        let second = &index["entries"][1];
        assert!(second.get("report").is_none());
        assert_eq!(
            second["brief"],
            serde_json::to_value(Brief::from(&report(9))).unwrap()
        );
        assert_eq!(
            (second["name"].as_str(), index["installed"].as_u64()),
            (Some("Ice"), Some(2))
        );
        // A second history.json, as an older app would write, stays put.
        std::fs::write(old_file(&dir), "{}").unwrap();
        move_old(&dir).unwrap();
        assert!(old_file(&dir).exists());
    }
}
