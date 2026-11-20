//! Save slots by flag name: read, compare and edit them for playtests.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use ounce_formats::{Byml, KEYED, Save, SaveEntry, murmur3};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::gamedata::GameData;
use crate::known::field;
use crate::plain::Plain;
use crate::{generate, value};

/// Slot `n`'s save under a save folder.
pub fn slot_file(dir: &Path, n: u8) -> PathBuf {
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
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))
        .context(Plain(
            "The save folder couldn't be read. Check the target's save folder".into(),
        ))?;
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
    let read = std::fs::read(path).with_context(|| format!("reading {}", path.display()));
    read.and_then(Save::parse).context(Plain(
        "This save couldn't be read. It may be damaged, or from another version of the game".into(),
    ))
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
    Some(
        raws.into_iter()
            .map(|r| value::render(gd, e.kind, r))
            .collect(),
    )
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

    /// A per-placement flag going `on` or off, named by its key.
    pub fn keyed(key: u64, on: bool) -> Self {
        Self {
            hash: 0,
            name: key.to_string(),
            label: None,
            kind: KEYED.into(),
            change: format!("{} -> {on}", !on),
        }
    }

    /// The key a per-placement flag's change names.
    pub fn key(&self) -> Option<u64> {
        (self.kind == KEYED).then(|| self.name.parse().ok())?
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

/// Each per-placement flag that went on or off from `was` to `now`.
pub fn keyed(was: &BTreeSet<u64>, now: &BTreeSet<u64>) -> Vec<Change> {
    let on = now.difference(was).map(|&k| Change::keyed(k, true));
    on.chain(was.difference(now).map(|&k| Change::keyed(k, false)))
        .collect()
}

/// Every difference from `a` to `b` among the entries `wanted` keeps, one
/// line each, including entries only one of them has. Per-placement flags
/// come one key each.
pub fn diff(gd: &GameData, a: &Save, b: &Save, wanted: impl Fn(SaveEntry) -> bool) -> Vec<Change> {
    let wanted = |e: SaveEntry| e.kind != KEYED && wanted(e);
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
    out.extend(keyed(&a.keys(), &b.keys()));
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

/// The slot under `dir` written last, if it was written at or after
/// `since` (the installed seed's install time): an older one belongs to
/// another seed.
pub fn newest_since(dir: &Path, since: Option<u32>) -> Result<Option<PathBuf>> {
    Ok(newest(dir)?.filter(|s| written_since(s, since)))
}

/// Whether `path` was written at or after `since`, in Unix seconds. With no
/// time, every save counts.
pub fn written_since(path: &Path, since: Option<u32>) -> bool {
    let Some(since) = since else {
        return true;
    };
    let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(since.into());
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|m| m >= at)
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

/// The hash a `NAME` in an edit stands for: `#` and eight hex digits for a
/// value the game doesn't name, like `PlayerStatus`'s members, else the
/// name's own hash.
pub fn hash_of(name: &str) -> u32 {
    (name.strip_prefix('#'))
        .filter(|hex| hex.len() == 8)
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| murmur3(name.as_bytes()))
}

/// Apply `edits` to the save at `path`, keeping the old file next to it as a
/// timestamped backup. Returns the edit log and the backup.
pub(crate) fn edit(gd: &GameData, path: &Path, edits: &[String]) -> Result<(Vec<String>, PathBuf)> {
    let mut save = load(path)?;
    let log = set(gd, &mut save, edits)?;
    Ok((log, write(path, &save)?))
}

/// Write `save` over the file at `path`, keeping the old one next to it as
/// a timestamped backup, which it returns.
pub fn write(path: &Path, save: &Save) -> Result<PathBuf> {
    let backup = path.with_extension(format!("sav.{}.bak", generate::time_seed() / 1_000_000_000));
    std::fs::copy(path, &backup)?;
    std::fs::write(path, save.bytes())?;
    Ok(backup)
}

/// One element of a save value to set, as text its type takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Edit {
    pub hash: u32,
    /// The element: 0 for anything that isn't an array.
    #[serde(default)]
    pub index: u32,
    pub value: String,
}

impl Edit {
    pub fn new(hash: u32, value: impl Into<String>) -> Self {
        Self {
            hash,
            index: 0,
            value: value.into(),
        }
    }

    /// `NAME=VALUE`.
    pub fn parse(edit: &str) -> Result<Self> {
        let (name, text) = split(edit)?;
        Ok(Self::new(hash_of(name), text))
    }
}

/// Apply `edits` in order. Returns each as `name: old -> new`. Stops at the
/// first one the save can't take, so a caller that writes only on success
/// never writes half of them.
pub fn apply(gd: &GameData, save: &mut Save, edits: &[Edit]) -> Result<Vec<String>> {
    let mut log = Vec::new();
    for edit in edits {
        let mut name = gd.name(edit.hash);
        let e = save
            .get(edit.hash)
            .with_context(|| format!("{name} isn't in this save"))?;
        if e.kind.ends_with("Array") {
            name = format!("{name}[{}]", edit.index);
        }
        let i = edit.index as usize;
        let width = ounce_formats::save_width(e.kind)
            .with_context(|| format!("{name}: {} values can't be written", e.kind))?;
        let bytes = match value::element(e.kind) {
            "Enum" => value::scalar(gd, e.kind, &edit.value).map(|r| r.to_le_bytes().to_vec()),
            _ => value::encode(e.kind, &edit.value, width),
        }
        .context(name.clone())?;
        let old = save
            .element(e, i)
            .map(|b| value::decode(gd, e.kind, b))
            .with_context(|| format!("{name} has no element {i}"))?;
        save.set(e, i, &bytes)?;
        log.push(format!(
            "{name}: {old} -> {}",
            value::decode(gd, e.kind, &bytes)
        ));
    }
    Ok(log)
}

/// Apply `NAME=VALUE` edits, as [`apply`] does.
pub fn set(gd: &GameData, save: &mut Save, edits: &[String]) -> Result<Vec<String>> {
    let edits: Vec<Edit> = edits
        .iter()
        .map(|e| Edit::parse(e))
        .collect::<Result<_>>()?;
    apply(gd, save, &edits)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_save_counts_from_the_install_on() {
        let dir = std::env::temp_dir().join(format!("zootr-since-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let save = dir.join("progress.sav");
        std::fs::write(&save, b"save").unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        assert!(written_since(&save, None));
        assert!(written_since(&save, Some(now - 60)));
        assert!(!written_since(&save, Some(now + 60)));
        assert!(!written_since(&dir.join("gone.sav"), Some(0)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_newest_save_counts_only_once_played_since_the_install() {
        let dir = std::env::temp_dir().join(format!("zootr-newest-{}", std::process::id()));
        let save = slot_file(&dir, 1);
        std::fs::create_dir_all(save.parent().unwrap()).unwrap();
        std::fs::write(&save, b"save").unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        assert_eq!(newest_since(&dir, None).unwrap(), Some(save.clone()));
        assert_eq!(newest_since(&dir, Some(now - 60)).unwrap(), Some(save));
        assert_eq!(newest_since(&dir, Some(now + 60)).unwrap(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    use super::*;

    #[test]
    fn keys_that_flip_come_one_change_each() {
        let changes = keyed(&BTreeSet::from([1, 2]), &BTreeSet::from([2, 3]));
        let shown: Vec<_> = (changes.iter())
            .map(|c| (c.key(), c.change.as_str()))
            .collect();
        assert_eq!(
            shown,
            [(Some(3), "false -> true"), (Some(1), "true -> false")]
        );
        assert_eq!(
            Change::new(&GameData::default(), 3, "Bool", "x").key(),
            None
        );
    }

    /// A save with bools `(hash, value)` and binaries `(hash, data)`.
    fn save(bools: &[(u32, u32)], binaries: &[(u32, &[u8])]) -> Vec<u8> {
        typed(bools, 18, binaries)
    }

    /// A save with bools `(hash, value)`, then values of the save type
    /// numbered `kind` kept out of the table, `(hash, data)`.
    fn typed(bools: &[(u32, u32)], kind: u32, binaries: &[(u32, &[u8])]) -> Vec<u8> {
        const TABLE: usize = 0x20;
        let mut pairs = vec![(0, 0)];
        pairs.extend(bools);
        pairs.push((0, kind));
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
    fn edits_write_each_type_they_name_and_stop_at_a_bad_one() {
        let gd = GameData::default();
        // String16 is the save's type 12.
        let mut save = Save::parse(typed(&[(11, 0)], 12, &[(22, &[0; 16])])).unwrap();
        let log = apply(
            &gd,
            &mut save,
            &[Edit::new(11, "true"), Edit::new(22, "Kak")],
        )
        .unwrap();
        assert_eq!(log, ["#0000000b: false -> true", "#00000016:  -> Kak"]);
        assert_eq!(
            save.strings(save.get(22).unwrap()),
            Some(vec!["Kak".into()])
        );
        assert!(apply(&gd, &mut save, &[Edit::new(11, "maybe")]).is_err());
        assert!(apply(&gd, &mut save, &[Edit::new(33, "true")]).is_err());
        assert!(apply(&gd, &mut save, &[Edit::new(22, "far too long for this")]).is_err());
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
