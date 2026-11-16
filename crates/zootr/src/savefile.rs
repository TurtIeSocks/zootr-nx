//! Save slots by flag name: read, compare and edit them for playtests.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use ounce_formats::{Byml, Romfs, Save, SaveEntry, murmur3};
use serde::Serialize;

use crate::config::Config;
use crate::gamedata::GameData;
use crate::known::field;
use crate::{generate, presets, value};

/// Slot `n`'s save under a save folder.
fn slot_file(dir: &Path, n: u8) -> PathBuf {
    dir.join(format!("slot_{n:02}/progress.sav"))
}

/// A slot number under the picked target's save folder, or a path to any
/// `progress.sav`.
pub fn path(slot: &str) -> Result<PathBuf> {
    match slot.parse::<u8>() {
        Ok(n) => Ok(slot_file(&Config::load()?.save_dir()?, n)),
        Err(_) => Ok(slot.into()),
    }
}

/// The slots under `dir` that hold a save.
pub fn slots(dir: &Path) -> Result<Vec<u8>> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut slots: Vec<u8> = entries
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_prefix("slot_")?
                .parse()
                .ok()
        })
        .filter(|&n| slot_file(dir, n).exists())
        .collect();
    slots.sort();
    Ok(slots)
}

pub fn load(path: &Path) -> Result<Save> {
    Save::parse(std::fs::read(path).with_context(|| format!("reading {}", path.display()))?)
}

/// A GameData `DefaultValue` as the raw bits a save would hold.
pub fn raw(value: &Byml) -> Option<u32> {
    Some(match value {
        Byml::Bool(b) => *b as u32,
        Byml::I32(n) => *n as u32,
        Byml::U32(n) => *n,
        Byml::Float(f) => f.to_bits(),
        _ => return None,
    })
}

/// `raw` as a `DefaultValue` of the same BYML type as `like`.
pub fn to_default(like: &Byml, raw: u32) -> Option<Byml> {
    Some(match like {
        Byml::Bool(_) => Byml::Bool(raw != 0),
        Byml::I32(_) => Byml::I32(raw as i32),
        Byml::U32(_) => Byml::U32(raw),
        Byml::Float(_) => Byml::Float(f32::from_bits(raw)),
        _ => return None,
    })
}

fn default(gd: &GameData, e: SaveEntry) -> Option<u32> {
    raw(gd
        .entry(e.hash)?
        .as_map()
        .ok()?
        .get(&**field::DEFAULT_VALUE)?)
}

/// Every value this tool can read, rendered: `None` for types it can't.
pub fn values(gd: &GameData, save: &Save, e: SaveEntry) -> Option<Vec<String>> {
    if let Some(text) = save.strings(e) {
        return Some(text);
    }
    let raws = match save.scalar(e) {
        Some(raw) => vec![raw],
        None => save.array(e)?,
    };
    Some(raws.into_iter().map(|r| value::render(gd, e.kind, r)).collect())
}

/// `e`'s value, or its size for types [`values`] can't read.
pub fn shown(gd: &GameData, save: &Save, e: SaveEntry) -> String {
    values(gd, save, e).map_or_else(|| format!("{} bytes", save.data(e).len()), |v| v.join(","))
}

/// One changed value, to print or to send to the app.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Change {
    pub hash: u32,
    /// The name, else `#hash`, with `[i]` for an array element.
    pub name: String,
    pub label: Option<String>,
    pub kind: String,
    /// `a -> b`, or what changed when the values can't be shown.
    pub change: String,
}

impl Change {
    pub fn new(gd: &GameData, hash: u32, kind: &str, change: impl Into<String>) -> Self {
        Self {
            hash,
            name: gd.bare(hash),
            label: gd.label(hash),
            kind: kind.into(),
            change: change.into(),
        }
    }

    /// The same change, as element `i` of an array.
    pub fn at(mut self, i: usize) -> Self {
        self.name = format!("{}[{i}]", self.name);
        self
    }
}

/// Name, type, change. Unnamed values show a hash and any label.
impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match &self.label {
            Some(label) => format!("{} ({label})", self.name),
            None => self.name.clone(),
        };
        write!(f, "{name:<56} {:<10} {}", self.kind, self.change)
    }
}

/// One line per entry: name, type, value. Unnamed entries show a hash.
pub fn line(gd: &GameData, hash: u32, kind: &str, value: &str) -> String {
    Change::new(gd, hash, kind, value).to_string()
}

/// Which bytes changed, for types [`values`] can't read.
pub(crate) fn bytes(a: &[u8], b: &[u8]) -> String {
    let at: Vec<String> = (0..a.len().min(b.len()))
        .filter(|&i| a[i] != b[i])
        .map(|i| i.to_string())
        .collect();
    let at = match at.len() {
        0 => String::new(),
        n if n > 12 => format!(", changed at {}... ({n})", at[..12].join(",")),
        _ => format!(", changed at {}", at.join(",")),
    };
    format!("{} -> {} bytes{at}", a.len(), b.len())
}

/// Every difference from `a` to `b` among the entries `wanted` keeps, one
/// line each, including entries only one of them has.
pub fn diff(gd: &GameData, a: &Save, b: &Save, wanted: impl Fn(SaveEntry) -> bool) -> Vec<Change> {
    let mut out: Vec<Change> = b
        .entries()
        .iter()
        .filter(|&&e| wanted(e))
        .filter_map(|&e| {
            let Some(old) = a.get(e.hash) else {
                return Some(Change::new(
                    gd,
                    e.hash,
                    e.kind,
                    format!("new: {}", shown(gd, b, e)),
                ));
            };
            let change = match (values(gd, a, old), values(gd, b, e)) {
                (Some(x), Some(y)) => {
                    (x != y).then(|| format!("{} -> {}", x.join(","), y.join(",")))
                }
                _ => (a.data(old) != b.data(e)).then(|| bytes(a.data(old), b.data(e))),
            };
            change.map(|c| Change::new(gd, e.hash, e.kind, c))
        })
        .collect();
    out.extend(
        a.entries()
            .iter()
            .filter(|&&e| wanted(e) && b.get(e.hash).is_none())
            .map(|&e| Change::new(gd, e.hash, e.kind, "gone")),
    );
    out
}

/// The saves to watch: one slot or path, or every slot in the save folder.
pub fn watched(slot: Option<&str>) -> Result<Vec<PathBuf>> {
    match slot {
        Some(slot) => Ok(vec![path(slot)?]),
        None => files(&Config::load()?.save_dir()?),
    }
}

/// Every slot's save under `dir`.
pub fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(slots(dir)?.into_iter().map(|n| slot_file(dir, n)).collect())
}

/// The slot under `dir` written last, if any.
pub fn newest(dir: &Path) -> Result<Option<PathBuf>> {
    let modified = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    Ok(files(dir)?.into_iter().max_by_key(modified))
}

/// Each save as last read, to diff the next write against.
#[derive(Default)]
pub struct Watcher {
    seen: HashMap<PathBuf, (SystemTime, Save)>,
}

impl Watcher {
    /// What changed in each of `paths` since the last look, skipping saves
    /// with no change. A save seen for the first time, or caught mid-write,
    /// gives nothing yet.
    pub fn look(
        &mut self,
        gd: &GameData,
        paths: &[PathBuf],
        wanted: impl Fn(SaveEntry) -> bool,
    ) -> Vec<(PathBuf, Vec<Change>)> {
        let mut out = Vec::new();
        for path in paths {
            let Ok(time) = std::fs::metadata(path).and_then(|m| m.modified()) else {
                continue;
            };
            if self.seen.get(path).is_some_and(|(t, _)| *t == time) {
                continue;
            }
            let Ok(save) = load(path) else { continue };
            if let Some((_, old)) = self.seen.get(path) {
                let lines = diff(gd, old, &save, &wanted);
                if !lines.is_empty() {
                    out.push((path.clone(), lines));
                }
            }
            self.seen.insert(path.clone(), (time, save));
        }
        out
    }
}

/// Whether `e` holds anything other than its GameData default.
pub fn changed(gd: &GameData, save: &Save, e: SaveEntry) -> bool {
    let Some(d) = default(gd, e) else {
        return false;
    };
    match save.scalar(e) {
        Some(raw) => raw != d,
        None => save.array(e).is_some_and(|a| a.iter().any(|&r| r != d)),
    }
}

/// `NAME=VALUE` as its two halves.
pub fn split(edit: &str) -> Result<(&str, &str)> {
    edit.split_once('=').context("edits look like NAME=VALUE")
}

/// Apply `edits` to the save at `path`, keeping the old file next to it as a
/// timestamped backup. Returns the edit log and the backup.
pub(crate) fn edit(gd: &GameData, path: &Path, edits: &[String]) -> Result<(Vec<String>, PathBuf)> {
    let mut save = load(path)?;
    let log = set(gd, &mut save, edits)?;
    let backup = path.with_extension(format!("sav.{}.bak", generate::time_seed() / 1_000_000_000));
    std::fs::copy(path, &backup)?;
    std::fs::write(path, save.bytes())?;
    Ok((log, backup))
}

/// Apply `presets` to slot `n` under `dir`, as [`edit`] does.
pub fn edit_slot(
    romfs: &Romfs,
    dir: &Path,
    n: u8,
    presets: &[String],
) -> Result<(Vec<String>, PathBuf)> {
    let edits = presets::for_save(presets, &[])?;
    edit(&GameData::load(romfs)?, &slot_file(dir, n), &edits)
}

/// Apply `NAME=VALUE` edits. Returns each edit as `name: old -> new`.
pub fn set(gd: &GameData, save: &mut Save, edits: &[String]) -> Result<Vec<String>> {
    let mut log = Vec::new();
    for edit in edits {
        let (name, text) = split(edit)?;
        let e = save
            .get(murmur3(name.as_bytes()))
            .with_context(|| format!("{name} isn't in this save"))?;
        let old = values(gd, save, e).unwrap_or_default().join(",");
        save.set_scalar(e.hash, value::scalar(gd, e.kind, text).context(name.to_owned())?)?;
        log.push(format!("{name}: {old} -> {text}"));
    }
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A save with bools `(hash, value)` and binaries `(hash, data)`.
    fn save(bools: &[(u32, u32)], binaries: &[(u32, &[u8])]) -> Vec<u8> {
        const TABLE: usize = 0x20;
        let mut pairs = vec![(0, 0)];
        pairs.extend(bools);
        pairs.push((0, 18));
        let end = TABLE + (pairs.len() + binaries.len()) * 8;
        let mut at = end;
        for (h, data) in binaries {
            pairs.push((*h, at as u32));
            at += data.len();
        }
        let mut b = vec![0u8; TABLE];
        b[..4].copy_from_slice(&0x0102_0304u32.to_le_bytes());
        b[8..12].copy_from_slice(&(end as u32).to_le_bytes());
        b.extend(
            pairs
                .iter()
                .flat_map(|(h, v)| [h.to_le_bytes(), v.to_le_bytes()])
                .flatten(),
        );
        binaries.iter().for_each(|(_, data)| b.extend(*data));
        b
    }

    #[test]
    fn a_watched_save_reports_each_write() {
        let dir = std::env::temp_dir().join(format!("zootr-test-{}-watch", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("progress.sav");
        let paths = [file.clone()];
        let gd = GameData::default();
        let mut watcher = Watcher::default();
        std::fs::write(&file, save(&[(1, 0), (2, 1)], &[(3, &[0; 8]), (4, &[5])])).unwrap();
        assert!(watcher.look(&gd, &paths, |_| true).is_empty());
        assert!(watcher.look(&gd, &paths, |_| true).is_empty());
        let after = save(
            &[(1, 1), (5, 0)],
            &[(3, &[0, 9, 0, 0, 0, 0, 0, 0, 7]), (4, &[5])],
        );
        std::fs::write(&file, after).unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let [(path, lines)] = &watcher.look(&gd, &paths, |_| true)[..] else {
            panic!("one changed save")
        };
        assert_eq!(path, &file);
        let lines: Vec<String> = lines
            .iter()
            .map(|l| l.to_string())
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(
            lines,
            [
                "#00000001 Bool false -> true",
                "#00000005 Bool new: false",
                "#00000003 Binary 8 -> 9 bytes, changed at 1",
                "#00000002 Bool gone",
            ]
        );
        assert!(watcher.look(&gd, &paths, |e| e.hash == 1).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn slots_are_the_folders_holding_a_save() {
        let dir = std::env::temp_dir().join(format!("zootr-test-{}-slots", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for n in [2, 0] {
            let file = slot_file(&dir, n);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"").unwrap();
        }
        for empty in ["slot_01", "slot_xx", "other"] {
            std::fs::create_dir_all(dir.join(empty)).unwrap();
        }
        assert_eq!(slots(&dir).unwrap(), [0, 2]);
        assert!(slots(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
