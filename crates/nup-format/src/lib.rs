//! Reader for NUP containers (`.nup`): unencrypted NX game dumps, with the
//! base game, its updates and its add-ons in one file.
//!
//! Written from the format's SPEC.md, v1. Read only, and the per-package meta
//! TLV block is not read (see the README).

use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

#[cfg(feature = "fixture")]
pub mod fixture;

const MAGIC: [u8; 4] = *b"NUP\x01";
const TOC_MAGIC: [u8; 4] = *b"TOC\x01";
const HEADER: usize = 0x80;
const TOC_HEAD: usize = 0x30;
const PACKAGE: usize = 0x40;
const DIR: usize = 0x18;
const FILE: usize = 0x50;
const ALIGN: u64 = 16;
const NONE: u32 = u32::MAX;
/// Deeper trees are refused, which also stops a TOC whose directories loop.
const MAX_DEPTH: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("not a NUP v1 container")]
    NotNup,
    #[error("corrupt container: {0}")]
    Corrupt(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("unsafe name {0:?}")]
    UnsafeName(String),
    #[error("{0}: content does not match its SHA-256")]
    Hash(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn corrupt(what: impl Into<String>) -> Error {
    Error::Corrupt(what.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dir(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct File(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    Dir(Dir),
    File(File),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Base,
    Update,
    Addon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub title_id: u64,
    pub kind: Kind,
    /// The NCM content version, opaque: 131072 is not necessarily 2.0.0.
    pub version: u32,
    /// An update in a container without a base.
    pub standalone: bool,
    pub root: Dir,
    pub exefs: Option<Dir>,
    pub romfs: Option<Dir>,
}

#[derive(Clone, Copy)]
struct Name {
    off: u32,
    len: u32,
}

struct DirRec {
    name: Name,
    first: u32,
    count: u32,
}

struct FileRec {
    name: Name,
    size: u64,
    offset: u64,
    stored: u64,
    sha256: [u8; 32],
    zstd: bool,
}

pub struct Nup {
    file: fs::File,
    uuid: [u8; 16],
    packages: Vec<Package>,
    dirs: Vec<DirRec>,
    files: Vec<FileRec>,
    children: Vec<u32>,
    pool: Vec<u8>,
}

/// `N` bytes at `at`. Callers check the slice is long enough first.
fn le<const N: usize>(b: &[u8], at: usize) -> [u8; N] {
    b[at..at + N]
        .try_into()
        .expect("length checked by the caller")
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(le(b, at))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(le(b, at))
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(le(b, at))
}

fn size(n: u64) -> Result<usize> {
    usize::try_from(n).map_err(|_| corrupt("size beyond this platform's address space"))
}

/// `count` records of `each` bytes, as a byte length.
fn span(count: u64, each: usize) -> Result<usize> {
    size(count)?
        .checked_mul(each)
        .ok_or_else(|| corrupt("record count overflows"))
}

/// Names are labels, not paths: anything that could leave the folder it is
/// extracted into is refused.
fn safe(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

#[cfg(unix)]
fn pread(file: &fs::File, buf: &mut [u8], at: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, at)
}

#[cfg(windows)]
fn pread(file: &fs::File, buf: &mut [u8], at: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, at)
}

/// A byte range of the container, read without moving a shared cursor.
struct Range<'a> {
    file: &'a fs::File,
    pos: u64,
    end: u64,
}

impl Read for Range<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = usize::try_from(self.end - self.pos).unwrap_or(usize::MAX);
        let want = buf.len().min(left);
        if want == 0 {
            return Ok(0);
        }
        let n = pread(self.file, &mut buf[..want], self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }
}

/// Hashes and counts what passes through to `inner`.
struct Hashing<W> {
    inner: W,
    hash: Sha256,
    len: u64,
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hash.update(&buf[..n]);
        self.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Nup {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len < HEADER as u64 {
            return Err(Error::NotNup);
        }
        let mut head = [0; HEADER];
        Range {
            file: &file,
            pos: 0,
            end: HEADER as u64,
        }
        .read_exact(&mut head)?;
        if head[..4] != MAGIC {
            return Err(Error::NotNup);
        }

        let header_size = u64::from(u16_at(&head, 4));
        let toc_offset = u64_at(&head, 0x08);
        let toc_size = u64_at(&head, 0x10);
        let blob_offset = u64_at(&head, 0x38);
        let package_count = u32_at(&head, 0x48);
        let flags = u32_at(&head, 0x4C);
        if header_size < HEADER as u64 || toc_offset != header_size {
            return Err(corrupt("TOC does not follow the header"));
        }
        if u64_at(&head, 0x40) != len {
            return Err(corrupt("file_size does not match the file"));
        }
        let toc_end = toc_offset
            .checked_add(toc_size)
            .ok_or_else(|| corrupt("TOC size overflows"))?;
        if !blob_offset.is_multiple_of(ALIGN) || blob_offset < toc_end || blob_offset > len {
            return Err(corrupt("blob region misplaced"));
        }
        if flags != 0 {
            return Err(Error::Unsupported(format!("superblock flags {flags:#x}")));
        }
        if package_count == 0 {
            return Err(corrupt("no packages"));
        }

        let mut toc = vec![0; size(toc_size)?];
        Range {
            file: &file,
            pos: toc_offset,
            end: toc_end,
        }
        .read_exact(&mut toc)?;
        if Sha256::digest(&toc)[..] != head[0x18..0x38] {
            return Err(corrupt("TOC SHA-256 mismatch"));
        }

        let mut nup = Self {
            file,
            uuid: le(&head, 0x50),
            packages: Vec::new(),
            dirs: Vec::new(),
            files: Vec::new(),
            children: Vec::new(),
            pool: Vec::new(),
        };
        nup.parse_toc(&toc, package_count, blob_offset, len)?;
        Ok(nup)
    }

    fn parse_toc(&mut self, toc: &[u8], package_count: u32, blob: u64, len: u64) -> Result<()> {
        if toc.len() < TOC_HEAD || toc[..4] != TOC_MAGIC {
            return Err(corrupt("bad TOC magic"));
        }
        if u32_at(toc, 4) != package_count {
            return Err(corrupt("package counts disagree"));
        }
        let dir_count = u64_at(toc, 0x08);
        let file_count = u64_at(toc, 0x10);
        let pool_off = size(u64_at(toc, 0x18))?;
        let pool_size = size(u64_at(toc, 0x20))?;
        if dir_count + file_count > u64::from(u32::MAX) {
            return Err(corrupt("too many nodes for 32-bit node ids"));
        }

        let dirs_at = TOC_HEAD + span(package_count.into(), PACKAGE)?;
        let files_at = dirs_at + span(dir_count, DIR)?;
        let children_at = files_at + span(file_count, FILE)?;
        if children_at > toc.len() {
            return Err(corrupt("TOC truncated"));
        }
        if pool_off
            .checked_add(pool_size)
            .is_none_or(|end| end > toc.len())
        {
            return Err(corrupt("string pool outside the TOC"));
        }
        self.pool = toc[pool_off..pool_off + pool_size].to_vec();
        let name = |b: &[u8], at: usize| Name {
            off: u32_at(b, at),
            len: u32_at(b, at + 4),
        };

        self.dirs = (0..size(dir_count)?)
            .map(|i| {
                let r = &toc[dirs_at + i * DIR..][..DIR];
                DirRec {
                    name: name(r, 0),
                    first: u32_at(r, 0x08),
                    count: u32_at(r, 0x0C),
                }
            })
            .collect();
        self.files = (0..size(file_count)?)
            .map(|i| {
                let r = &toc[files_at + i * FILE..][..FILE];
                let codec = r[0x40];
                let rec = FileRec {
                    name: name(r, 0),
                    size: u64_at(r, 0x08),
                    offset: u64_at(r, 0x10),
                    stored: u64_at(r, 0x18),
                    sha256: le(r, 0x20),
                    zstd: codec == 1,
                };
                match codec {
                    0 if rec.stored != rec.size => Err(corrupt("raw extent size differs")),
                    0 | 1 => Ok(rec),
                    c => Err(Error::Unsupported(format!("codec {c}"))),
                }
            })
            .collect::<Result<_>>()?;

        let slots: u64 = self.dirs.iter().map(|d| u64::from(d.count)).sum();
        let children_end = children_at + span(slots, 4)?;
        if children_end > toc.len() {
            return Err(corrupt("children outside the TOC"));
        }
        self.children = (0..size(slots)?)
            .map(|i| u32_at(toc, children_at + i * 4))
            .collect();

        self.packages = (0..package_count as usize)
            .map(|i| self.package(&toc[TOC_HEAD + i * PACKAGE..][..PACKAGE], len))
            .collect::<Result<_>>()?;
        self.check(blob, len)
    }

    fn package(&self, r: &[u8], len: u64) -> Result<Package> {
        let dir = |at: usize| match u32_at(r, at) {
            NONE => Ok(None),
            i if (i as usize) < self.dirs.len() => Ok(Some(Dir(i))),
            _ => Err(corrupt("package directory out of range")),
        };
        let kind = match r[0x18] {
            0 => Kind::Base,
            1 => Kind::Update,
            2 => Kind::Addon,
            k => return Err(Error::Unsupported(format!("package kind {k}"))),
        };
        let meta_end = u64_at(r, 0x20).checked_add(u32_at(r, 0x28).into());
        if meta_end.is_none_or(|end| end > len) {
            return Err(corrupt("meta block outside the file"));
        }
        Ok(Package {
            title_id: u64_at(r, 0),
            kind,
            version: u32_at(r, 0x14),
            standalone: r[0x19] & 1 != 0,
            root: dir(0x08)?.ok_or_else(|| corrupt("package without a root"))?,
            exefs: dir(0x0C)?,
            romfs: dir(0x10)?,
        })
    }

    /// Everything a later read trusts: names, child ranges and extents.
    fn check(&self, blob: u64, len: u64) -> Result<()> {
        let names = self
            .dirs
            .iter()
            .map(|d| d.name)
            .chain(self.files.iter().map(|f| f.name));
        for n in names {
            if u64::from(n.off) + u64::from(n.len) > self.pool.len() as u64 {
                return Err(corrupt("name outside the string pool"));
            }
            if std::str::from_utf8(self.raw_name(n)).is_err() {
                return Err(corrupt("name is not UTF-8"));
            }
        }
        for d in &self.dirs {
            if u64::from(d.first) + u64::from(d.count) > self.children.len() as u64 {
                return Err(corrupt("child range out of bounds"));
            }
        }
        let nodes = self.dirs.len() + self.files.len();
        for &id in &self.children {
            if id as usize >= nodes {
                return Err(corrupt("child id out of range"));
            }
            let name = self.name(self.node(id));
            if !safe(name) {
                return Err(Error::UnsafeName(name.into()));
            }
        }
        for f in &self.files {
            let fits = f.offset >= blob
                && f.offset.is_multiple_of(ALIGN)
                && f.offset.checked_add(f.stored).is_some_and(|end| end <= len);
            let ok = if f.stored == 0 { f.size == 0 } else { fits };
            if !ok {
                return Err(corrupt("file extent out of bounds"));
            }
        }
        Ok(())
    }

    fn raw_name(&self, n: Name) -> &[u8] {
        &self.pool[n.off as usize..][..n.len as usize]
    }

    fn node(&self, id: u32) -> Node {
        match id.checked_sub(self.dirs.len() as u32) {
            None => Node::Dir(Dir(id)),
            Some(i) => Node::File(File(i)),
        }
    }

    /// Derived from the packages' ids, versions and kinds: the same dump
    /// packed twice has the same uuid.
    pub fn uuid(&self) -> [u8; 16] {
        self.uuid
    }

    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    pub fn name(&self, node: Node) -> &str {
        let n = match node {
            Node::Dir(d) => self.dirs[d.0 as usize].name,
            Node::File(f) => self.files[f.0 as usize].name,
        };
        std::str::from_utf8(self.raw_name(n)).expect("checked at open")
    }

    pub fn children(&self, dir: Dir) -> impl Iterator<Item = Node> + '_ {
        let d = &self.dirs[dir.0 as usize];
        let slots = &self.children[d.first as usize..][..d.count as usize];
        slots.iter().map(|&id| self.node(id))
    }

    /// The node at a `/`-separated path under `dir`.
    pub fn find(&self, dir: Dir, path: &str) -> Option<Node> {
        let mut node = Node::Dir(dir);
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let Node::Dir(d) = node else { return None };
            node = self.children(d).find(|&n| self.name(n) == part)?;
        }
        Some(node)
    }

    /// A file's uncompressed size.
    pub fn size(&self, file: File) -> u64 {
        self.files[file.0 as usize].size
    }

    /// Decode a file into `out`, then check its size and SHA-256. On a
    /// mismatch `out` has already received the bad bytes.
    pub fn copy(&self, file: File, out: impl Write) -> Result<()> {
        let f = &self.files[file.0 as usize];
        let range = Range {
            file: &self.file,
            pos: f.offset,
            end: f.offset + f.stored,
        };
        let mut out = Hashing {
            inner: out,
            hash: Sha256::new(),
            len: 0,
        };
        if f.zstd {
            zstd::stream::copy_decode(range, &mut out)?;
        } else {
            io::copy(&mut BufReader::with_capacity(1 << 16, range), &mut out)?;
        }
        out.flush()?;
        if out.len != f.size || out.hash.finalize()[..] != f.sha256 {
            return Err(Error::Hash(self.name(Node::File(file)).into()));
        }
        Ok(())
    }

    pub fn read(&self, file: File) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.copy(file, &mut out)?;
        Ok(out)
    }

    /// Write the tree under `dir` into `out`, replacing files already there.
    /// Returns how many files were written.
    pub fn extract(&self, dir: Dir, out: &Path) -> Result<u64> {
        self.extract_at(dir, out, 0)
    }

    fn extract_at(&self, dir: Dir, out: &Path, depth: usize) -> Result<u64> {
        if depth > MAX_DEPTH {
            return Err(corrupt(format!("tree deeper than {MAX_DEPTH} levels")));
        }
        fs::create_dir_all(out)?;
        let mut written = 0;
        for node in self.children(dir) {
            let path = out.join(self.name(node));
            match node {
                Node::Dir(d) => written += self.extract_at(d, &path, depth + 1)?,
                Node::File(f) => {
                    self.copy(f, io::BufWriter::new(fs::File::create(&path)?))?;
                    written += 1;
                }
            }
        }
        Ok(written)
    }
}
