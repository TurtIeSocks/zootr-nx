//! The game dump: a `.nup`, extracted into a cache on first use, or an
//! already extracted romfs folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use nup_format::{Kind, Node, Nup};
use ounce_formats::{Romfs, files_with_suffix, murmur3};

use crate::platform::{Folder, user_dir};
use crate::progress::{Run, Step};

/// Ocarina of Time's base title. Its update is `TITLE | 0x800`.
pub const TITLE: u64 = 0x0400_4860_24DE_C000;
/// The 1.1.1 update, the only one zootr reads. Its romfs names its
/// resource size table `Product.111`.
pub const UPDATE: u32 = 131072;
/// The single files zootr reads, under the romfs root: what the custom
/// models are made from, the pause menu's table its tests check, and the
/// effect file the ice traps' look fits to child Link.
pub fn files() -> impl Iterator<Item = &'static str> {
    crate::medallions::FILES
        .into_iter()
        .chain(crate::key_tints::FILES)
        .chain(crate::foreign::FILES)
        .chain(crate::trade_slot::FILES)
        .chain(crate::ice_traps::FILES)
}

/// What zootr reads, under the romfs root, besides [`files`].
pub const SUBTREES: [&str; 9] = [
    "Banc",            // actor placement: chests, pots, freestanding items
    "GameData",        // save flag table
    "Pack",            // actor packs and the zstd dictionaries
    "System/Resource", // RESTBL
    "Mals",            // message text
    "Event/EventFlow", // cutscenes and NPC gifts
    "Logic",           // logic graphs: what makes a chest appear or unlock
    "RSDB",            // per-actor tables, for the actors shopsanity adds
    "Sequence",        // menu graphs: the flags a menu reads, like the tapestry's
];

/// Overrides where extracted dumps are cached.
const ENV: &str = "ZOOTR_CACHE";

/// Held while extracting, so a second caller waits and reuses the result.
static EXTRACTING: Mutex<()> = Mutex::new(());

/// Where extracted dumps and what's built from them are cached.
pub fn cache() -> Result<PathBuf> {
    match std::env::var_os(ENV) {
        Some(p) => Ok(p.into()),
        None => user_dir(Folder::Cache, ENV),
    }
}

/// The update's romfs. The base isn't read: the update's romfs is complete,
/// and the base's files carry older versions in their names.
fn romfs(nup: &Nup) -> Result<nup_format::Dir> {
    let found = nup
        .packages()
        .iter()
        .find(|p| p.kind == Kind::Update && p.title_id == TITLE | 0x800 && p.version == UPDATE);
    match found.and_then(|p| p.romfs) {
        Some(d) => Ok(d),
        None => bail!(
            "this dump has no 1.1.1 update of Ocarina of Time (title {:016X}, version {UPDATE})",
            TITLE | 0x800
        ),
    }
}

fn open(path: &Path) -> Result<Nup> {
    Nup::open(path).with_context(|| format!("opening {}", path.display()))
}

/// The romfs folder for `path`: a folder as is, a `.nup` extracted into the
/// cache first.
pub fn folder(path: &Path, run: &Run) -> Result<PathBuf> {
    if path.is_dir() {
        run.detail("a folder");
        return Ok(path.into());
    }
    extract(path, &cache()?.join("romfs"), run)
}

pub fn prepare(path: &Path, run: &Run) -> Result<Romfs> {
    run.step(Step::Dump)?;
    let romfs = Romfs::open(folder(path, run)?)?;
    crate::known::found(romfs.root());
    Ok(romfs)
}

/// Extract into `<cache>/<uuid>` each subtree an earlier run didn't finish,
/// so a cache from before a subtree was added gains it. Builds in
/// `<uuid>.part` so an interrupted run never looks finished, and a stop
/// lands between subtrees.
fn extract(path: &Path, cache: &Path, run: &Run) -> Result<PathBuf> {
    let nup = open(path)?;
    let romfs = romfs(&nup)?;
    let uuid: String = nup.uuid().iter().map(|b| format!("{b:02x}")).collect();
    let out = cache.join(&uuid);
    let _lock = EXTRACTING.lock().unwrap_or_else(|e| e.into_inner());
    let missing: Vec<&str> = SUBTREES
        .into_iter()
        .chain(files())
        .filter(|sub| !out.join(sub).exists())
        .collect();
    if missing.is_empty() {
        run.detail("cached");
        return Ok(out);
    }
    let part = out.with_extension("part");
    if part.exists() {
        fs::remove_dir_all(&part)?;
    }
    for (i, sub) in missing.iter().enumerate() {
        run.detail(format!("extracting {sub} ({} of {})", i + 1, missing.len()));
        run.check()?;
        let dest = part.join(sub);
        let extracting = || format!("extracting romfs/{sub}");
        match nup.find(romfs, sub) {
            Some(Node::Dir(dir)) => {
                nup.extract(dir, &dest).with_context(extracting)?;
            }
            Some(Node::File(file)) => {
                fs::create_dir_all(dest.parent().context("no parent")?)?;
                fs::write(&dest, nup.read(file).with_context(extracting)?)?;
            }
            None => bail!("the 1.1.1 update has no romfs/{sub}"),
        }
    }
    for sub in missing {
        let dest = out.join(sub);
        fs::create_dir_all(dest.parent().context("no parent")?)?;
        fs::rename(part.join(sub), dest)?;
    }
    fs::remove_dir_all(&part)?;
    Ok(out)
}

/// A cache folder for what's built from the dump at `path`, named so a
/// different dump gets its own: the `.nup`'s uuid, or a folder's path.
pub fn cached(path: &Path, what: &str) -> Result<PathBuf> {
    let id = match path.is_dir() {
        true => format!("{:08x}", murmur3(path.to_string_lossy().as_bytes())),
        false => open(path)?
            .uuid()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    };
    Ok(cache()?.join(what).join(id))
}

/// Every file in the dump's romfs whose path contains `pattern`, read
/// straight from the `.nup`, so files outside [`SUBTREES`] are reachable.
pub fn search(path: &Path, pattern: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut found = Vec::new();
    each(path, pattern, |rel, data| {
        found.push((rel, data));
        Ok(())
    })?;
    Ok(found)
}

/// [`search`], handing each file to `f` as it's read instead of keeping
/// them all.
pub fn each(
    path: &Path,
    pattern: &str,
    mut f: impl FnMut(String, Vec<u8>) -> Result<()>,
) -> Result<()> {
    if path.is_dir() {
        for file in files_with_suffix(path, "")? {
            let rel = file
                .strip_prefix(path)?
                .to_string_lossy()
                .replace('\\', "/");
            if rel.contains(pattern) {
                f(rel, fs::read(&file)?)?;
            }
        }
        return Ok(());
    }
    let nup = open(path)?;
    walk(&nup, romfs(&nup)?, "", pattern, &mut f)
}

fn walk(
    nup: &Nup,
    dir: nup_format::Dir,
    at: &str,
    pattern: &str,
    f: &mut impl FnMut(String, Vec<u8>) -> Result<()>,
) -> Result<()> {
    for node in nup.children(dir) {
        let path = format!("{at}{}", nup.name(node));
        match node {
            Node::Dir(d) => walk(nup, d, &format!("{path}/"), pattern, f)?,
            Node::File(file) if path.contains(pattern) => f(path, nup.read(file)?)?,
            Node::File(_) => {}
        }
    }
    Ok(())
}

/// Where tests find the dev dump.
#[cfg(test)]
pub(crate) const DEV_DUMP: &str = "../../dump/oot.nup";

/// The dev dump, or `None` without one so tests that need it pass vacuously.
#[cfg(test)]
pub(crate) fn dev_dump() -> Option<Romfs> {
    let path = Path::new(DEV_DUMP);
    path.exists()
        .then(|| prepare(path, &Run::silent()).unwrap())
}

/// Whether tests run without the dev dump, and so without the game's
/// names. A test that reads a name returns early then, as the dump-backed
/// tests do: the names never live in the repo.
#[cfg(test)]
pub(crate) fn no_names() -> bool {
    !Path::new(DEV_DUMP).exists()
}

/// Every actor pack holding `file`, sorted by name.
#[cfg(test)]
pub(crate) fn packs_holding(romfs: &Romfs, file: &str) -> Vec<String> {
    let dir = romfs.root().join("Pack/Actor");
    let mut found: Vec<String> = ounce_formats::files_with_suffix(&dir, ".pack.zs")
        .unwrap()
        .iter()
        .filter_map(|p| {
            p.file_name()?
                .to_str()?
                .strip_suffix(".pack.zs")
                .map(String::from)
        })
        .filter(|pack| {
            ounce_formats::ActorPack::load(romfs, pack).is_ok_and(|p| p.raw(file).is_some())
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use nup_format::fixture::{Package, build};

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-game-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A dump with a base and an update, in a fresh folder.
    fn dump(name: &str, version: u32) -> (PathBuf, PathBuf) {
        let mut update: Vec<(String, &[u8])> = SUBTREES
            .iter()
            .map(|s| (format!("romfs/{s}/new.bin"), &b"update"[..]))
            .chain(files().map(|f| (format!("romfs/{f}"), &b"file"[..])))
            .collect();
        update.push(("romfs/Movie/skip.bin".into(), b"not read"));
        let update: Vec<(&str, &[u8])> = update.iter().map(|(p, d)| (p.as_str(), *d)).collect();
        let base: &[(&str, &[u8])] = &[
            ("romfs/Banc/new.bin", b"base"),
            ("romfs/Banc/old.Product.100.bin", b"base"),
        ];
        let bytes = build(&[
            Package {
                kind: 0,
                version: 0,
                title_id: TITLE,
                files: base,
            },
            Package {
                kind: 1,
                version,
                title_id: TITLE | 0x800,
                files: &update,
            },
        ]);
        let dir = scratch(name);
        let path = dir.join("oot.nup");
        fs::write(&path, bytes).unwrap();
        (path, dir.join("cache"))
    }

    #[test]
    fn extracts_the_update_subtrees_only() {
        let (path, cache) = dump("update", UPDATE);
        let out = extract(&path, &cache, &Run::silent()).unwrap();
        for sub in SUBTREES {
            assert_eq!(fs::read(out.join(sub).join("new.bin")).unwrap(), b"update");
        }
        for file in files() {
            assert_eq!(fs::read(out.join(file)).unwrap(), b"file");
        }
        assert!(!out.join("Banc/old.Product.100.bin").exists());
        assert!(!out.join("Movie").exists());
    }

    #[test]
    fn refuses_other_versions() {
        let (path, cache) = dump("version", UPDATE + 1);
        let err = extract(&path, &cache, &Run::silent())
            .unwrap_err()
            .to_string();
        assert!(err.contains("1.1.1"), "{err}");
        assert!(prepare(&path, &Run::silent()).is_err());
    }

    #[test]
    fn reuses_a_finished_cache_and_rebuilds_a_partial_one() {
        let (path, cache) = dump("cache", UPDATE);
        let out = extract(&path, &cache, &Run::silent()).unwrap();
        fs::write(out.join("Banc/new.bin"), b"edited").unwrap();
        assert_eq!(extract(&path, &cache, &Run::silent()).unwrap(), out);
        assert_eq!(fs::read(out.join("Banc/new.bin")).unwrap(), b"edited");

        fs::remove_dir_all(&out).unwrap();
        let part = out.with_extension("part");
        fs::create_dir_all(&part).unwrap();
        fs::write(part.join("junk"), b"").unwrap();
        extract(&path, &cache, &Run::silent()).unwrap();
        assert!(!part.exists() && !out.join("junk").exists());
        assert_eq!(fs::read(out.join("Banc/new.bin")).unwrap(), b"update");
    }

    #[test]
    fn fills_in_a_subtree_an_older_cache_lacks() {
        let (path, cache) = dump("stale", UPDATE);
        let out = extract(&path, &cache, &Run::silent()).unwrap();
        fs::write(out.join("Banc/new.bin"), b"edited").unwrap();
        fs::remove_dir_all(out.join("Logic")).unwrap();
        assert_eq!(extract(&path, &cache, &Run::silent()).unwrap(), out);
        assert_eq!(fs::read(out.join("Logic/new.bin")).unwrap(), b"update");
        assert_eq!(fs::read(out.join("Banc/new.bin")).unwrap(), b"edited");
    }
}
