use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, ensure};

use roead::sarc::{Sarc, SarcWriter};

use crate::{Dicts, Restbl, files_with_suffix, pack_file, par_map};

/// A read-only extracted romfs folder. A clone shares its dictionaries.
#[derive(Clone)]
pub struct Romfs {
    root: PathBuf,
    dicts: Arc<Dicts>,
}

impl Romfs {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let dicts = Arc::new(Dicts::load(&root)?);
        Ok(Self { root, dicts })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dicts(&self) -> &Dicts {
        &self.dicts
    }

    /// The romfs-relative path of a file under this root, with `/` separators.
    pub fn rel(&self, path: &Path) -> Result<String> {
        let rel = path.strip_prefix(&self.root)?;
        Ok(rel.to_string_lossy().replace('\\', "/"))
    }

    /// `rel` as this romfs names it: see [`stamped`].
    pub fn resolve<'a>(&self, rel: &'a str) -> Cow<'a, str> {
        stamped(&self.root, rel)
    }

    /// A file's contents, decompressed when it is zstd. A stamped name
    /// reads this romfs's file under its own stamp ([`stamped`]).
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        let rel = self.resolve(rel);
        let rel = rel.as_ref();
        let path = self.root.join(rel);
        let raw = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        self.dicts.decompress(rel, &raw)
    }

    /// The RESTBL file's romfs-relative path. The game ships exactly one.
    pub fn restbl_rel(&self) -> Result<String> {
        let dir = self.root.join("System/Resource");
        let found = files_with_suffix(&dir, ".rsizetable.zs")?;
        let [only] = found.as_slice() else {
            return Err(anyhow!(
                "expected one RESTBL in {}, found {}",
                dir.display(),
                found.len()
            ));
        };
        self.rel(only)
    }
}

/// `name` split around its version stamp, `.Product.` and digits: what
/// comes before and after it. `None` for a name without one.
fn split_stamp(name: &str) -> Option<(&str, &str)> {
    const STAMP: &str = ".Product.";
    let at = name.find(STAMP)?;
    let rest = &name[at + STAMP.len()..];
    let digits = rest.find(|c: char| !c.is_ascii_digit())?;
    (digits > 0).then(|| (&name[..at], &rest[digits..]))
}

/// Whether `name` is `want` but for its version stamp, which may be any.
fn same_but_stamp(name: &str, want: &str) -> bool {
    split_stamp(name).is_some_and(|parts| split_stamp(want) == Some(parts))
}

/// Of `names`, the one that is `want` but for its version stamp, the
/// highest version if there are more (100 above 90).
pub fn stamped_among<'a>(names: impl IntoIterator<Item = &'a str>, want: &str) -> Option<&'a str> {
    let version = |n: &str| -> u32 {
        let (head, tail) = split_stamp(n).unwrap_or_default();
        n[head.len() + ".Product.".len()..n.len() - tail.len()]
            .parse()
            .unwrap_or_default()
    };
    (names.into_iter())
        .filter(|n| same_but_stamp(n, want))
        .max_by_key(|n| version(n))
}

/// `rel` as the romfs at `root` names it. Each build stamps some files
/// with its version, and not all alike: 1.0.0's `Product.100` is
/// `Product.110` or `Product.111` in 1.1.1. A stamped name `root` lacks is
/// looked up in its folder under any stamp ([`stamped_among`]). Anything
/// else, or a name no stamp finds, is `rel` as it is.
pub fn stamped<'a>(root: &Path, rel: &'a str) -> Cow<'a, str> {
    if split_stamp(rel).is_none() || root.join(rel).exists() {
        return Cow::Borrowed(rel);
    }
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let entries = std::fs::read_dir(root.join(dir)).into_iter().flatten();
    let names: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    match stamped_among(names.iter().map(String::as_str), file) {
        Some(found) if dir.is_empty() => Cow::Owned(found.into()),
        Some(found) => Cow::Owned(format!("{dir}/{found}")),
        None => Cow::Borrowed(rel),
    }
}

/// How much a mod holds uncompressed before compressing it: enough files
/// to keep every core busy, few enough to keep memory down.
const BATCH_BYTES: usize = 16 << 20;

/// Marks a mod folder as generated, so regenerating may clear it.
const MARKER: &str = ".ounce-mod";

/// Writes changed files into a LayeredFS mod folder (`<mod>/romfs/...`) and
/// keeps the mod's RESTBL in step with them.
///
/// Files go to a hidden sibling first and replace the mod only in `finish`,
/// so a failed build leaves the previous mod playable.
pub struct ModBuilder<'a> {
    romfs: &'a Romfs,
    mod_dir: PathBuf,
    staging: PathBuf,
    restbl_rel: String,
    restbl: Restbl,
    written: Vec<String>,
    /// Files to compress and write, a batch at a time, in order.
    pending: Vec<(String, Vec<u8>)>,
    pending_bytes: usize,
}

/// `dir` with `suffix`, hidden next to it.
fn sibling(dir: &Path, suffix: &str) -> Result<PathBuf> {
    let name = dir.file_name().context("mod folder has no name")?;
    Ok(dir.with_file_name(format!(".{}.{suffix}", name.to_string_lossy())))
}

/// Refuse to `verb` `dir` unless it is missing, empty or carries our
/// marker: a mistyped path must never touch someone's files.
fn ensure_ours(dir: &Path, verb: &str) -> Result<()> {
    if dir.exists() && !dir.join(MARKER).exists() {
        ensure!(
            std::fs::read_dir(dir)?.next().is_none(),
            "{} exists and wasn't made by this tool (no {MARKER}); refusing to {verb} it",
            dir.display()
        );
    }
    Ok(())
}

/// Delete `dir` if it exists, but only when [`ensure_ours`] allows it.
fn clear(dir: &Path) -> Result<()> {
    ensure_ours(dir, "clear")?;
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(dir).with_context(|| format!("clearing {}", dir.display()))
}

impl<'a> ModBuilder<'a> {
    /// `mod_dir` is the folder the game loads as a mod, holding `romfs/`.
    /// `finish` replaces it whole, so stale files from an earlier seed can't
    /// leak in.
    pub fn new(romfs: &'a Romfs, mod_dir: &Path) -> Result<Self> {
        ensure_ours(mod_dir, "replace")?;
        let staging = sibling(mod_dir, "part")?;
        clear(&staging)?;
        std::fs::create_dir_all(&staging)?;
        std::fs::write(staging.join(MARKER), b"")?;
        let restbl_rel = romfs.restbl_rel()?;
        let restbl = Restbl::parse(&romfs.read(&restbl_rel)?)?;
        Ok(Self {
            romfs,
            mod_dir: mod_dir.to_owned(),
            staging,
            restbl_rel,
            restbl,
            written: Vec::new(),
            pending: Vec::new(),
            pending_bytes: 0,
        })
    }

    /// A file's decompressed contents as this mod has them so far: the
    /// last written, else the game's. Two edits of one file (both Links'
    /// packs, the pause menu's table) build on each other this way, in
    /// whichever order they run.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        let rel = self.romfs.resolve(rel);
        let rel = rel.as_ref();
        if let Some((_, contents)) = self.pending.iter().rev().find(|(r, _)| r == rel) {
            return Ok(contents.clone());
        }
        if !self.written.iter().any(|w| w == rel) {
            return self.romfs.read(rel);
        }
        let path = self.staging.join("romfs").join(rel);
        let raw = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        self.romfs.dicts().decompress(rel, &raw)
    }

    /// Every file written so far, by its romfs path, in order.
    pub fn written(&self) -> &[String] {
        &self.written
    }

    /// Write a changed file, given its decompressed contents. Its RESTBL
    /// entry grows from what [`Self::read`] had. A stamped name is written
    /// under the game's own stamp ([`stamped`]).
    pub fn write(&mut self, rel: &str, contents: &[u8]) -> Result<()> {
        let rel = self.romfs.resolve(rel);
        let rel = rel.as_ref();
        let old_size = self.read(rel)?.len();
        self.restbl.resize(rel, old_size, contents.len())?;
        self.put(rel, contents)?;
        self.written.push(rel.to_owned());
        Ok(())
    }

    /// Write a changed file that took on parts of `like`, a shipped file of
    /// its type `like_size` bytes: its entry grows by its bytes, and to at
    /// least `like`'s at its new size, since each part costs the game more
    /// than its bytes.
    pub fn write_like(
        &mut self,
        rel: &str,
        contents: &[u8],
        like: &str,
        like_size: usize,
    ) -> Result<()> {
        self.write(rel, contents)?;
        self.floor(rel, like, like_size, contents.len())
    }

    /// Write a file the game doesn't ship, its RESTBL entry sized from
    /// `like`, a file of the same type the game ships or this mod adds.
    pub fn add(&mut self, rel: &str, contents: &[u8], like: &str) -> Result<()> {
        let like_size = self.read(like)?.len();
        self.add_entry(rel, like, like_size, contents.len())?;
        self.put(rel, contents)?;
        self.written.push(rel.to_owned());
        Ok(())
    }

    /// Account for a file inside a changed pack, which has its own RESTBL
    /// entry.
    pub fn resize(&mut self, path: &str, old_size: usize, new_size: usize) -> Result<()> {
        self.restbl.resize(path, old_size, new_size)
    }

    /// Account for a byml inside a changed pack that gained nodes: see
    /// [`Restbl::scale`].
    pub fn scale(&mut self, path: &str, old_size: usize, new_size: usize) -> Result<()> {
        self.restbl.scale(path, old_size, new_size)
    }

    /// Account for a file inside a changed pack that took on parts of
    /// `like`, a shipped file of its type `like_size` bytes: its entry
    /// grows by its bytes, and to at least `like`'s at its new size, since
    /// each part costs the game more than its bytes.
    pub fn resize_like(
        &mut self,
        path: &str,
        old_size: usize,
        new_size: usize,
        like: &str,
        like_size: usize,
    ) -> Result<()> {
        self.restbl.resize(path, old_size, new_size)?;
        self.floor(path, like, like_size, new_size)
    }

    /// Raise `path`'s entry to at least what `like`, `like_size` bytes,
    /// would need at `new_size`.
    fn floor(&mut self, path: &str, like: &str, like_size: usize, new_size: usize) -> Result<()> {
        let floor = self.restbl.like(like, like_size, new_size)?;
        self.restbl.raise(path, floor)
    }

    /// Rewrite `param` with `edit` in each actor pack of `packs`, and size
    /// its RESTBL entry. Every copy is edited alike, since whichever pack
    /// loads first serves it to all.
    pub fn edit_param(
        &mut self,
        packs: &[&str],
        param: &str,
        edit: impl Fn(&[u8]) -> Result<Vec<u8>>,
    ) -> Result<()> {
        let mut sizes = None;
        for pack in packs {
            let rel = pack_file(pack);
            let sarc = Sarc::new(self.read(&rel)?).with_context(|| format!("reading {rel}"))?;
            let old = (sarc.get_data(param)).with_context(|| format!("{rel} has no {param}"))?;
            let new = edit(old)?;
            sizes = Some((old.len(), new.len()));
            let packed = SarcWriter::from_sarc(&sarc)
                .with_file(param, new)
                .to_binary();
            self.write(&rel, &packed)?;
        }
        let (old, new) = sizes.context("no pack to edit")?;
        self.resize(param, old, new)
    }

    /// Whether `path` has a RESTBL entry: a shipped file the game sizes.
    pub fn has_entry(&self, path: &str) -> bool {
        self.restbl.get(path).is_some()
    }

    /// Give a new file inside a pack its RESTBL entry, sized from `like`, a
    /// shipped file of the same type that is `like_size` bytes.
    pub fn add_entry(
        &mut self,
        path: &str,
        like: &str,
        like_size: usize,
        new_size: usize,
    ) -> Result<()> {
        let size = self.restbl.like(like, like_size, new_size)?;
        self.restbl.add(path, size)
    }

    fn put(&mut self, rel: &str, contents: &[u8]) -> Result<()> {
        self.pending.push((rel.to_owned(), contents.to_vec()));
        self.pending_bytes += contents.len();
        match self.pending_bytes >= BATCH_BYTES {
            true => self.flush(),
            false => Ok(()),
        }
    }

    /// Compress every pending file at once, then write each in the order
    /// it came, so a file written twice keeps its last contents.
    fn flush(&mut self) -> Result<()> {
        let dicts = self.romfs.dicts();
        let pending = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;
        let compressed = par_map(&pending, |(rel, contents)| dicts.compress(rel, contents));
        for ((rel, _), bytes) in pending.iter().zip(compressed) {
            let path = self.staging.join("romfs").join(rel);
            std::fs::create_dir_all(path.parent().expect("romfs paths have a parent"))?;
            std::fs::write(&path, bytes?).with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(())
    }

    /// Write the RESTBL, swap the new mod in and return the romfs paths
    /// written, RESTBL included.
    pub fn finish(mut self) -> Result<Vec<String>> {
        let (restbl_rel, restbl) = (self.restbl_rel.clone(), self.restbl.to_binary());
        self.put(&restbl_rel, &restbl)?;
        self.flush()?;
        self.written.push(self.restbl_rel);
        swap_in(&self.staging, &self.mod_dir)?;
        Ok(self.written)
    }
}

/// Replace `mod_dir` whole with `staging`, keeping it until the move lands.
fn swap_in(staging: &Path, mod_dir: &Path) -> Result<()> {
    let old = sibling(mod_dir, "old")?;
    clear(&old)?;
    let moved = mod_dir.exists();
    if moved {
        std::fs::rename(mod_dir, &old)?;
    }
    if let Err(e) = std::fs::rename(staging, mod_dir) {
        if moved {
            _ = std::fs::rename(&old, mod_dir);
        }
        return Err(e).with_context(|| format!("moving the mod into {}", mod_dir.display()));
    }
    clear(&old)
}

/// Copy a written mod folder over `to`, staged and swapped in as
/// [`ModBuilder::finish`] does, so a failed copy leaves `to` as it was.
pub fn copy_mod(from: &Path, to: &Path) -> Result<()> {
    ensure_ours(to, "replace")?;
    let staging = sibling(to, "part")?;
    clear(&staging)?;
    copy_dir(from, &staging)
        .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
    std::fs::write(staging.join(MARKER), b"")?;
    swap_in(&staging, to)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        match entry.file_type()?.is_dir() {
            true => copy_dir(&entry.path(), &dest)?,
            false => _ = std::fs::copy(entry.path(), &dest)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamped_name_finds_the_file_under_its_own_stamp() {
        let root = std::env::temp_dir().join(format!("ounce-stamp-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Mals")).unwrap();
        for f in [
            "USen.Product.100.sarc.zs",
            "USen.Product.90.sarc.zs",
            "USes.Product.100.sarc.zs",
        ] {
            std::fs::write(root.join("Mals").join(f), b"").unwrap();
        }
        std::fs::write(root.join("Top.Product.111.byml"), b"").unwrap();
        let s = |rel| stamped(&root, rel).into_owned();
        assert_eq!(
            s("Mals/USen.Product.110.sarc.zs"),
            "Mals/USen.Product.100.sarc.zs"
        );
        assert_eq!(
            s("Mals/USen.Product.100.sarc.zs"),
            "Mals/USen.Product.100.sarc.zs"
        );
        assert_eq!(s("Top.Product.100.byml"), "Top.Product.111.byml");
        // Nothing to find, or no stamp to look by: as it is.
        assert_eq!(
            s("Mals/JPja.Product.110.sarc.zs"),
            "Mals/JPja.Product.110.sarc.zs"
        );
        assert_eq!(
            s("Gone/USen.Product.110.sarc.zs"),
            "Gone/USen.Product.110.sarc.zs"
        );
        assert_eq!(s("Mals/USen.sarc.zs"), "Mals/USen.sarc.zs");
        assert!(same_but_stamp("a.Product.1.b", "a.Product.22.b"));
        assert!(!same_but_stamp("a.Product.1.b", "a.Product.1.c"));
        assert!(!same_but_stamp("a.Product..b", "a.Product..b"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clear_only_deletes_our_folders() {
        let base = std::env::temp_dir().join(format!("ounce-clear-{}", std::process::id()));
        let (theirs, ours) = (base.join("theirs"), base.join("ours"));
        for dir in [&theirs, &ours] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("save.dat"), b"x").unwrap();
        }
        std::fs::write(ours.join(MARKER), b"").unwrap();
        assert!(clear(&theirs).is_err());
        clear(&ours).unwrap();
        assert!(theirs.exists() && !ours.exists());
        assert_eq!(sibling(&ours, "part").unwrap(), base.join(".ours.part"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn copy_mod_leaves_a_foreign_folder_where_it_is() {
        let base = std::env::temp_dir().join(format!("ounce-copy-{}", std::process::id()));
        let (from, to) = (base.join("from"), base.join("theirs"));
        for dir in [&from, &to] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(from.join("a.bin"), b"new").unwrap();
        std::fs::write(to.join("save.dat"), b"x").unwrap();
        assert!(copy_mod(&from, &to).is_err());
        assert_eq!(std::fs::read(to.join("save.dat")).unwrap(), b"x");
        assert!(!sibling(&to, "old").unwrap().exists());
        assert!(!sibling(&to, "part").unwrap().exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_failed_swap_puts_the_old_mod_back() {
        let base = std::env::temp_dir().join(format!("ounce-swap-{}", std::process::id()));
        let mod_dir = base.join("mod");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(mod_dir.join(MARKER), b"").unwrap();
        std::fs::write(mod_dir.join("a.bin"), b"old").unwrap();
        assert!(swap_in(&base.join("missing"), &mod_dir).is_err());
        assert_eq!(std::fs::read(mod_dir.join("a.bin")).unwrap(), b"old");
        std::fs::remove_dir_all(base).unwrap();
    }
}
