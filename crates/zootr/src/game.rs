//! The game dump: a `.nup`, extracted into a cache on first use, or an
//! already extracted romfs folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow, bail};
use nup_format::nso::Nso;
use nup_format::{Kind, Node, Nup};
use ounce_formats::{Romfs, files_with_suffix, murmur3};

use crate::plain::Plain;
use crate::platform::{Folder, user_dir};
use crate::progress::{Run, Step};

/// Ocarina of Time's base title. Its update is `TITLE | 0x800`.
pub const TITLE: u64 = 0x0400_4860_24DE_C000;
/// The 1.1.1 update, the only one zootr reads. Its romfs names its
/// resource size table `Product.111`.
pub const UPDATE: u32 = 131072;
/// The base package, the game as it came out: 1.0.0. Only its `main` is
/// read.
pub const BASE: u32 = 65536;
/// The pause menu's table: each page's slots, their items, and the ages
/// that use them. Equipment for both ages edits it, and the trade slot's
/// tests check it.
pub const POUCH_TABLE: &str = "UI/Pouch/PouchTable/PouchTable.game__ui__PouchTable.bgyml";

/// The single files zootr reads, under the romfs root: what the custom
/// models are made from, the pause menu's table, the effect file the
/// ice traps' look fits to child Link, and the animation archives items
/// for both ages lends each Link.
pub fn files() -> impl Iterator<Item = &'static str> {
    crate::medallions::FILES
        .into_iter()
        .chain(crate::key_tints::FILES)
        .chain(crate::bombchu_bags::FILES)
        .chain(crate::magic::meter::FILES)
        .chain(crate::foreign::FILES)
        .chain([POUCH_TABLE])
        .chain(crate::ice_traps::FILES)
        .chain(crate::ageless_items::FILES)
        .chain(crate::marks::FILES)
}

/// What zootr reads, under the romfs root, besides [`files`].
pub const SUBTREES: [&str; 12] = [
    "Banc",                         // actor placement: chests, pots, freestanding items
    "GameData",                     // save flag table
    "Pack",                         // actor packs and the zstd dictionaries
    "System/Resource",              // RESTBL
    "Mals",                         // message text
    "Event/EventFlow",              // cutscenes and NPC gifts
    "Logic",                        // logic graphs: what makes a chest appear or unlock
    "RSDB",                         // per-actor tables, for the actors shopsanity adds
    "Sequence",                     // menu graphs: the flags a menu reads, like the tapestry's
    crate::adult_jabu::FOLIAGE,     // the fountain's moss and plants, stripped under adult Jabu
    crate::carpenters::SCHEDULES,   // NPC schedules: the carpenters' rescue past the card
    crate::carpenters::CAPTURE_DIR, // map pictures: the valley bridge's
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

/// Why a dump can't be read, each with one sentence a player can act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BadDump {
    Missing,
    NotAPackage,
    NotTheGame,
    NoUpdate,
}

impl BadDump {
    pub const fn words(self) -> &'static str {
        match self {
            Self::Missing => "There's no file at that path. Check it, or choose the file again",
            Self::NotAPackage => {
                "That file isn't a game package, or it's damaged. Choose the .nup of your copy of the game"
            }
            Self::NotTheGame => {
                "That isn't Ocarina of Time. Choose the .nup of your copy of the game, or a folder of its extracted romfs"
            }
            Self::NoUpdate => "That dump doesn't include the 1.1.1 update. Choose one that does",
        }
    }

    /// `raw`, the developer's account, under this kind's words.
    fn with(self, raw: anyhow::Error) -> anyhow::Error {
        raw.context(Plain(self.words().into()))
    }
}

/// The update's romfs. The base isn't read: the update's romfs is complete,
/// and the base's files carry older versions in their names.
fn romfs(nup: &Nup) -> Result<nup_format::Dir> {
    let found = nup
        .packages()
        .iter()
        .find(|p| p.kind == Kind::Update && p.title_id == TITLE | 0x800 && p.version == UPDATE);
    if let Some(d) = found.and_then(|p| p.romfs) {
        return Ok(d);
    }
    let ours = nup.packages().iter().any(|p| p.title_id & !0x800 == TITLE);
    let kind = if ours {
        BadDump::NoUpdate
    } else {
        BadDump::NotTheGame
    };
    Err(kind.with(anyhow!(
        "this dump has no 1.1.1 update of Ocarina of Time (title {:016X}, version {UPDATE})",
        TITLE | 0x800
    )))
}

fn open(path: &Path) -> Result<Nup> {
    Nup::open(path).map_err(|e| {
        let kind = match &e {
            _ if !path.exists() => Some(BadDump::Missing),
            nup_format::Error::Io(io) if io.kind() != std::io::ErrorKind::UnexpectedEof => None,
            _ => Some(BadDump::NotAPackage),
        };
        let raw = anyhow::Error::from(e).context(format!("opening {}", path.display()));
        match kind {
            Some(kind) => kind.with(raw),
            None => raw,
        }
    })
}

/// A build of the game's code: the `main` in the base package (1.0.0) or
/// in the update (1.1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Release {
    #[value(name = "1.0.0")]
    V100,
    #[value(name = "1.1.1")]
    V111,
}

impl Release {
    pub const fn name(self) -> &'static str {
        match self {
            Self::V100 => "1.0.0",
            Self::V111 => "1.1.1",
        }
    }

    /// The package that holds it: title and version.
    const fn package(self) -> (u64, u32) {
        match self {
            Self::V100 => (TITLE, BASE),
            Self::V111 => (TITLE | 0x800, UPDATE),
        }
    }
}

/// `release`'s `exefs/main` as the `.nup` at `path` holds it: an NSO file.
pub fn main_nso(path: &Path, release: Release) -> Result<Vec<u8>> {
    let nup = open(path)?;
    let (title, version) = release.package();
    let name = release.name();
    let exefs = (nup.packages().iter())
        .find(|p| p.title_id == title && p.version == version)
        .and_then(|p| p.exefs)
        .with_context(|| {
            format!("this dump has no {name} exefs (title {title:016X}, version {version})")
        })?;
    match nup.find(exefs, "main") {
        Some(Node::File(main)) => Ok(nup.read(main)?),
        _ => bail!("{name}'s exefs has no main"),
    }
}

/// `release`'s `main` as it lies in memory, and its NSO header.
pub fn main_image(path: &Path, release: Release) -> Result<(Nso, Vec<u8>)> {
    let file = main_nso(path, release)?;
    let nso = Nso::parse(&file)?;
    let image =
        (nso.image(&file)).with_context(|| format!("decoding {}'s main", release.name()))?;
    Ok((nso, image))
}

/// The romfs folder for `path`: a folder as is, a `.nup` extracted into the
/// cache first.
pub fn folder(path: &Path, run: &Run) -> Result<PathBuf> {
    if path.is_dir() {
        if !path.join("Pack").is_dir() {
            let raw = anyhow!("{} has no Pack folder", path.display());
            return Err(BadDump::NotTheGame.with(raw));
        }
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
    f: impl FnMut(String, Vec<u8>) -> Result<()>,
) -> Result<()> {
    each_where(path, |p| p.contains(pattern), f)
}

/// [`each`] for the files whose path `keep` takes, so one walk of the
/// `.nup` reads many named files.
pub fn each_where(
    path: &Path,
    keep: impl Fn(&str) -> bool,
    mut f: impl FnMut(String, Vec<u8>) -> Result<()>,
) -> Result<()> {
    if path.is_dir() {
        for file in files_with_suffix(path, "")? {
            let rel = file
                .strip_prefix(path)?
                .to_string_lossy()
                .replace('\\', "/");
            if keep(&rel) {
                f(rel, fs::read(&file)?)?;
            }
        }
        return Ok(());
    }
    let nup = open(path)?;
    walk(&nup, romfs(&nup)?, "", &keep, &mut f)
}

fn walk(
    nup: &Nup,
    dir: nup_format::Dir,
    at: &str,
    keep: &impl Fn(&str) -> bool,
    f: &mut impl FnMut(String, Vec<u8>) -> Result<()>,
) -> Result<()> {
    for node in nup.children(dir) {
        let path = format!("{at}{}", nup.name(node));
        match node {
            Node::Dir(d) => walk(nup, d, &format!("{path}/"), keep, f)?,
            Node::File(file) if keep(&path) => f(path, nup.read(file)?)?,
            Node::File(_) => {}
        }
    }
    Ok(())
}

/// Where tests find the dev dump.
#[cfg(test)]
pub(crate) const DEV_DUMP: &str = "../../dump/oot.nup";

/// The dev dump, or `None` without one so tests that need it pass vacuously.
/// Opened once a process: each test gets a clone, which shares it.
#[cfg(test)]
pub(crate) fn dev_dump() -> Option<Romfs> {
    static DUMP: std::sync::OnceLock<Option<Romfs>> = std::sync::OnceLock::new();
    DUMP.get_or_init(|| {
        let path = Path::new(DEV_DUMP);
        path.exists()
            .then(|| prepare(path, &Run::silent()).unwrap())
    })
    .clone()
}

/// The file at `rel` a test's [`ModBuilder`](ounce_formats::ModBuilder)
/// wrote under `dir`, decompressed.
#[cfg(test)]
pub(crate) fn read_built(romfs: &Romfs, dir: &Path, rel: &str) -> Vec<u8> {
    let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
    romfs.dicts().decompress(rel, &raw).unwrap()
}

/// Whether tests run without the dev dump, and so without the game's
/// names. A test that reads a name returns early then, as the dump-backed
/// tests do: the names never live in the repo.
#[cfg(test)]
pub(crate) fn no_names() -> bool {
    !Path::new(DEV_DUMP).exists()
}

/// `release`'s `main` from the dev dump as it lies in memory, decoded
/// once a test run, or `None` without the dump.
#[cfg(test)]
pub(crate) fn dumped_main(release: Release) -> Option<&'static (Nso, Vec<u8>)> {
    use std::sync::OnceLock;
    static MAINS: [OnceLock<(Nso, Vec<u8>)>; 2] = [OnceLock::new(), OnceLock::new()];
    let path = Path::new(DEV_DUMP);
    let read = || main_image(path, release).unwrap();
    path.exists()
        .then(|| MAINS[release as usize].get_or_init(read))
}

/// Every actor pack holding `file`, sorted by name.
#[cfg(test)]
pub(crate) fn packs_holding(romfs: &Romfs, file: &str) -> Vec<String> {
    let mut found = crate::flags::pack_actors(romfs).unwrap();
    found.retain(|pack| {
        ounce_formats::ActorPack::load(romfs, pack).is_ok_and(|p| p.raw(file).is_some())
    });
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

    /// The kind of dump problem `path` reads as.
    fn kind(path: &Path) -> Option<&'static str> {
        let e = folder(path, &Run::silent()).unwrap_err();
        let plain = crate::plain::split(&e).map(|(p, _)| p.to_owned())?;
        [
            BadDump::Missing,
            BadDump::NotAPackage,
            BadDump::NotTheGame,
            BadDump::NoUpdate,
        ]
        .into_iter()
        .map(BadDump::words)
        .find(|w| *w == plain)
    }

    #[test]
    fn says_why_a_dump_cant_be_read() {
        let dir = scratch("kinds");
        assert_eq!(kind(&dir.join("gone.nup")), Some(BadDump::Missing.words()));

        let junk = dir.join("junk.nup");
        fs::write(&junk, b"not a container at all, just some words").unwrap();
        assert_eq!(kind(&junk), Some(BadDump::NotAPackage.words()));
        fs::write(&junk, b"").unwrap();
        assert_eq!(kind(&junk), Some(BadDump::NotAPackage.words()));

        let other = dir.join("other.nup");
        let files: &[(&str, &[u8])] = &[("romfs/a.bin", b"a")];
        let package = |kind, title_id| Package {
            kind,
            version: UPDATE,
            title_id,
            files,
        };
        fs::write(&other, build(&[package(0, 0x0100_0000_0000_0000)])).unwrap();
        assert_eq!(kind(&other), Some(BadDump::NotTheGame.words()));

        let base = dir.join("base.nup");
        fs::write(&base, build(&[package(0, TITLE)])).unwrap();
        assert_eq!(kind(&base), Some(BadDump::NoUpdate.words()));
        let (newer, _) = dump("newer", UPDATE + 1);
        assert_eq!(kind(&newer), Some(BadDump::NoUpdate.words()));

        let romfs = dir.join("romfs");
        fs::create_dir_all(&romfs).unwrap();
        assert_eq!(kind(&romfs), Some(BadDump::NotTheGame.words()));
        fs::create_dir_all(romfs.join("Pack")).unwrap();
        assert!(folder(&romfs, &Run::silent()).is_ok());
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

    /// The GNU build ID note's header: name size 4, ID size 20, type 3,
    /// "GNU". The linker puts the ID the NSO header names after it.
    const NOTE: [u8; 16] = [4, 0, 0, 0, 20, 0, 0, 0, 3, 0, 0, 0, b'G', b'N', b'U', 0];

    #[test]
    fn each_release_names_its_build_id_and_carries_it() {
        let v100 = [
            0x33, 0xfa, 0x6c, 0x2c, 0x23, 0x84, 0x82, 0x82, 0xa7, 0x7e, 0x8b, 0x88, 0xa0, 0xa9,
            0x3f, 0x48, 0x87, 0xc5, 0x59, 0x20,
        ];
        let v111 = &crate::patch::BUILD_ID[..20];
        for (release, id) in [(Release::V100, &v100[..]), (Release::V111, v111)] {
            let Some((nso, image)) = dumped_main(release) else {
                return;
            };
            assert_eq!(nso.build_id[..20], *id, "{}", release.name());
            assert!(nso.build_id[20..].iter().all(|&b| b == 0));
            let note = image.windows(16).position(|w| w == NOTE).unwrap() + 16;
            assert_eq!(image[note..note + 20], *id, "{}", release.name());
        }
    }

    #[test]
    fn the_item_add_starts_with_a_pacibsp_in_each_release() {
        // 1.0.0's is sloperinaGZ's, from its per-build table.
        let adds = [
            (Release::V111, zootr_hook::OFFSETS.granted),
            (Release::V100, 0x113_78f0),
        ];
        for (release, at) in adds {
            let Some((_, image)) = dumped_main(release) else {
                return;
            };
            let word = u32::from_le_bytes(image[at as usize..][..4].try_into().unwrap());
            assert_eq!(word, 0xd503_237f, "{}", release.name());
        }
    }

    #[test]
    fn mod0_and_bss_lie_where_each_release_puts_them() {
        // 1.1.1's from our notes on `main`, 1.0.0's MOD0 from sloperinaGZ.
        for (release, at) in [(Release::V111, 0x3c4_705c), (Release::V100, 0x3be_705c)] {
            let Some((nso, image)) = dumped_main(release) else {
                return;
            };
            assert_eq!(nso.mod0(image).unwrap(), at, "{}", release.name());
        }
        let Some((nso, image)) = dumped_main(Release::V111) else {
            return;
        };
        // MOD0's fourth word: how far past it bss ends.
        let mod0 = nso.mod0(image).unwrap() as usize;
        let to_end = i32::from_le_bytes(image[mod0 + 12..][..4].try_into().unwrap());
        assert_eq!(mod0 as i64 + i64::from(to_end), 0x4e9_a4c8);
    }
}
