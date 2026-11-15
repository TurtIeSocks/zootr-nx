//! The game's names, kept out of our source. Each is written as its hash,
//! `n!(0x…)`, and read back from the user's dump the first time a name is
//! asked for. [`crate::game::prepare`] says which dump.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use ounce_formats::{Romfs, files_with_suffix};

/// The dump folders and files that hold the names we use, in two tiers.
/// The first holds almost all of them, the save flags among them, and
/// reads in a few seconds on a debug build. The second (actor packs,
/// layouts, labels) is read only when a name isn't in the first. Each is
/// cached once read (see [`load`]).
const SOURCES: [&[&str]; 2] = [
    &["Event/EventFlow", "RSDB", "Banc", "Logic", "GameData"],
    &["Pack/Actor", "UI", "Mals/USen.Product.110.sarc.zs", "Model"],
];

/// A game name by its [`hash`]. Derefs to the name read from the dump.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(pub u64);

/// A game name written as its hash: `n!(0x…)`. Works in `const` items.
#[macro_export]
macro_rules! n {
    ($hash:literal) => {
        $crate::known::Name($hash)
    };
}

pub mod actor;
pub mod field;
pub mod flag;
pub mod flow;
pub mod item;
pub mod map;
pub mod scene;

/// FNV-1a over the name's bytes. `const`, so a test can hash a name it
/// spells out for itself.
pub const fn hash(name: &str) -> u64 {
    let bytes = name.as_bytes();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    h
}

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static TABLES: [OnceLock<HashMap<u64, &'static str>>; 2] = [OnceLock::new(), OnceLock::new()];

/// Names come from the romfs at `root`, once it has opened. The first
/// call wins: one dump per process.
pub fn found(root: &Path) {
    let _ = ROOT.set(root.into());
}

/// The name hashed `hash`, reading each tier of [`SOURCES`] on first use
/// until one has it. `None` before any dump is found, or when none has it.
fn lookup(hash: u64) -> Option<&'static str> {
    if hash == self::hash("") {
        return Some("");
    }
    #[cfg(test)]
    if ROOT.get().is_none() {
        // Tests read names whichever runs first: open the dev dump if there is one.
        let _ = crate::game::dev_dump();
    }
    let root = ROOT.get()?;
    TABLES.iter().enumerate().find_map(|(tier, table)| {
        let table = table.get_or_init(|| {
            load(root, tier)
                .inspect_err(|e| eprintln!("reading the game's names: {e:#}"))
                .unwrap_or_default()
        });
        table.get(&hash).copied()
    })
}

/// Tier `tier`'s names by hash. Read from the dump the first time, which
/// takes seconds for the actor packs, then from a copy in the cache.
fn load(root: &Path, tier: usize) -> Result<HashMap<u64, &'static str>> {
    let file = crate::game::cached(root, "names")?.join(format!("tier{tier}.txt"));
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(_) => {
            let text = read(&Romfs::open(root)?, SOURCES[tier])?;
            if let Err(e) = keep(&file, &text) {
                eprintln!("caching the game's names: {e:#}");
            }
            text
        }
    };
    Ok(text.leak().lines().map(|name| (hash(name), name)).collect())
}

/// Writes `text` to `file` through a temporary, so a reader never sees half.
fn keep(file: &Path, text: &str) -> Result<()> {
    let part = file.with_extension(format!("{}.part", std::process::id()));
    std::fs::create_dir_all(file.parent().unwrap_or(file))?;
    std::fs::write(&part, text)?;
    Ok(std::fs::rename(part, file)?)
}

/// Every name in `sources`, one a line, each hash once.
fn read(romfs: &Romfs, sources: &[&str]) -> Result<String> {
    let (mut seen, mut names) = (HashSet::new(), String::new());
    for source in sources {
        let at = romfs.root().join(source);
        if !at.exists() {
            // An older extract lacks some folders: read what it has.
            continue;
        }
        let files = if at.is_file() {
            vec![at]
        } else {
            files_with_suffix(&at, "")?
        };
        for path in files {
            let rel = path.strip_prefix(romfs.root())?.to_string_lossy();
            let raw = std::fs::read(&path)?;
            let data = romfs.dicts().decompress(&rel, &raw).unwrap_or(raw);
            for word in words(&data).filter(|w| seen.insert(hash(w))) {
                names += word;
                names.push('\n');
            }
        }
    }
    Ok(names)
}

/// The runs of name characters in `data` that could be a name.
fn words(data: &[u8]) -> impl Iterator<Item = &str> {
    data.split(|b| !(b.is_ascii_alphanumeric() || b"._:".contains(b)))
        .filter(|s| s.len() >= 4 && s[0].is_ascii_alphabetic())
        .filter_map(|s| std::str::from_utf8(s).ok())
}

impl Name {
    /// The name up to and including its last `_`, for matching a family of
    /// names: the bag stem `X_Y_` from `X_Y_Large`. Empty without one.
    pub fn stem(self) -> &'static str {
        let name = self.get();
        &name[..name.rfind('_').map_or(0, |i| i + 1)]
    }

    /// The name. In tests, its hash in hex when there's no dump or the dump
    /// lacks it. Outside tests either is a bug, and panics: the name would
    /// reach the game as its hash.
    pub fn get(self) -> &'static str {
        static MISSING: OnceLock<std::sync::Mutex<HashMap<u64, &'static str>>> = OnceLock::new();
        if let Some(name) = lookup(self.0) {
            return name;
        }
        if !cfg!(test) {
            let at = ROOT.get().map_or(" (none open yet)".into(), |r| {
                format!(" at {}", r.display())
            });
            panic!("game name #{:016x} is not in the dump{at}", self.0);
        }
        let missing = MISSING.get_or_init(Default::default);
        let mut missing = missing.lock().unwrap_or_else(|e| e.into_inner());
        missing
            .entry(self.0)
            .or_insert_with(|| format!("#{:016x}", self.0).leak())
    }
}

impl Deref for Name {
    type Target = str;
    fn deref(&self) -> &str {
        self.get()
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        self.get()
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.get())
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.get())
    }
}

// `==` with strings both ways. std's `&A == &B` covers the references,
// and `String == &Name` reads a field against a constant.
impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        hash(other) == self.0
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        *self == **other
    }
}

impl PartialEq<Name> for str {
    fn eq(&self, other: &Name) -> bool {
        *other == *self
    }
}

impl PartialEq<Name> for String {
    fn eq(&self, other: &Name) -> bool {
        *other == **self
    }
}

impl PartialEq<&Name> for String {
    fn eq(&self, other: &&Name) -> bool {
        **other == **self
    }
}

impl PartialEq<Name> for &str {
    fn eq(&self, other: &Name) -> bool {
        *other == **self
    }
}

impl PartialEq<&Name> for str {
    fn eq(&self, other: &&Name) -> bool {
        **other == *self
    }
}

#[cfg(test)]
mod guard;

#[cfg(test)]
mod tests {
    use super::*;

    // The two tests below spell names out on purpose: they check the hash and
    // the reader themselves, which a constant would only repeat.
    #[test]
    fn a_name_compares_by_hash_with_or_without_the_dump() {
        let navi = Name(hash("Npc_N"));
        let owned = "Npc_N".to_owned();
        assert!(navi == *"Npc_N" && "Npc_N" == navi && owned == navi);
        assert!(navi != *"Npc_S");
    }

    #[test]
    fn the_dump_names_what_we_use() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for sources in SOURCES {
            let started = std::time::Instant::now();
            let names = read(&romfs, sources).unwrap();
            eprintln!("{} names in {:?}", names.lines().count(), started.elapsed());
        }
        assert_ne!(item::BOMB.get(), "");
        assert!(!item::BOMB.get().starts_with('#'));
        assert_eq!(
            Name(hash("OcarinaMelodyIcon_01")).get(),
            "OcarinaMelodyIcon_01"
        );
    }

    #[test]
    fn a_name_stops_at_its_last_underscore() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let stem = item::BOMB_BAG_LARGE.stem();
        assert_eq!(stem, item::BOMB_BAG_SMALL.stem());
        assert!(stem.ends_with('_') && item::BOMB_BAG_LARGE.starts_with(stem));
        assert_eq!(Name(hash("Navi")).stem(), "");
    }

    #[test]
    fn the_dump_has_every_name_in_the_catalog() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let kinds = [
            ("actor", include_str!("known/actor.rs")),
            ("field", include_str!("known/field.rs")),
            ("flag", include_str!("known/flag.rs")),
            ("flow", include_str!("known/flow.rs")),
            ("item", include_str!("known/item.rs")),
            ("map", include_str!("known/map.rs")),
            ("scene", include_str!("known/scene.rs")),
        ];
        let mut lost = Vec::new();
        for (kind, text) in kinds {
            for item in text.split("pub const ").skip(1) {
                let (ident, rest) = item.split_once(':').unwrap();
                let hex = rest
                    .split("n!(0x")
                    .nth(1)
                    .unwrap()
                    .split(')')
                    .next()
                    .unwrap();
                if lookup(u64::from_str_radix(hex, 16).unwrap()).is_none() {
                    lost.push(format!("{kind}::{ident}"));
                }
            }
        }
        assert!(lost.is_empty(), "not in the dump: {lost:?}");
    }
}
