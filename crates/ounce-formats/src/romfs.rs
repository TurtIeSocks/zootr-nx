use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};

use roead::sarc::{Sarc, SarcWriter};

use crate::{Dicts, Restbl, files_with_suffix, pack_file, par_map};

/// A read-only extracted romfs folder.
pub struct Romfs {
    root: PathBuf,
    dicts: Dicts,
}

impl Romfs {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let dicts = Dicts::load(&root)?;
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

    /// A file's contents, decompressed when it is zstd.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
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

/// Delete `dir` if it exists, but only when empty or carrying our marker:
/// a mistyped path must never delete someone's files.
fn clear(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let ours = dir.join(MARKER).exists();
    if !ours && std::fs::read_dir(dir)?.next().is_some() {
        bail!(
            "{} exists and wasn't made by this tool (no {MARKER}); refusing to clear it",
            dir.display()
        );
    }
    std::fs::remove_dir_all(dir).with_context(|| format!("clearing {}", dir.display()))
}

impl<'a> ModBuilder<'a> {
    /// `mod_dir` is the folder the game loads as a mod, holding `romfs/`.
    /// `finish` replaces it whole, so stale files from an earlier seed can't
    /// leak in.
    pub fn new(romfs: &'a Romfs, mod_dir: &Path) -> Result<Self> {
        if mod_dir.exists() && !mod_dir.join(MARKER).exists() {
            ensure!(
                std::fs::read_dir(mod_dir)?.next().is_none(),
                "{} exists and wasn't made by this tool (no {MARKER}); refusing to replace it",
                mod_dir.display()
            );
        }
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

    /// Write a changed file, given its decompressed contents.
    pub fn write(&mut self, rel: &str, contents: &[u8]) -> Result<()> {
        let old_size = self.romfs.read(rel)?.len();
        self.restbl.resize(rel, old_size, contents.len())?;
        self.put(rel, contents)?;
        self.written.push(rel.to_owned());
        Ok(())
    }

    /// Write a file the game doesn't ship, its RESTBL entry sized from
    /// `like`, a shipped file of the same type.
    pub fn add(&mut self, rel: &str, contents: &[u8], like: &str) -> Result<()> {
        let like_size = self.romfs.read(like)?.len();
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
            let sarc =
                Sarc::new(self.romfs.read(&rel)?).with_context(|| format!("reading {rel}"))?;
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
    if mod_dir.exists() {
        std::fs::rename(mod_dir, &old)?;
    }
    std::fs::rename(staging, mod_dir)
        .with_context(|| format!("moving the mod into {}", mod_dir.display()))?;
    clear(&old)
}

/// Copy a written mod folder over `to`, staged and swapped in as
/// [`ModBuilder::finish`] does, so a failed copy leaves `to` as it was.
pub fn copy_mod(from: &Path, to: &Path) -> Result<()> {
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
}
