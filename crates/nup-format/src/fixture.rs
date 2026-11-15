//! Builds small containers for tests. Not a packer: no dedup, no meta, no
//! exefs, and names are taken as given so tests can build bad ones.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::{ALIGN, DIR, FILE, HEADER, MAGIC, NONE, PACKAGE, TOC_HEAD, TOC_MAGIC};

pub struct Package<'a> {
    /// 0 base, 1 update, 2 add-on. Other values build reserved kinds.
    pub kind: u8,
    pub version: u32,
    pub title_id: u64,
    /// Paths under the package root, e.g. `romfs/Banc/a.bin`.
    pub files: &'a [(&'a str, &'a [u8])],
}

#[derive(Default)]
struct Tree<'a> {
    dirs: BTreeMap<&'a str, Tree<'a>>,
    files: BTreeMap<&'a str, &'a [u8]>,
}

enum Slot {
    Dir(u32),
    File(u32),
}

#[derive(Default)]
struct Layout<'a> {
    /// Name, first child slot, child count.
    dirs: Vec<(&'a str, u32, u32)>,
    files: Vec<(&'a str, &'a [u8])>,
    slots: Vec<Slot>,
}

impl<'a> Layout<'a> {
    /// Lays out `tree` with its children in one contiguous, sorted range.
    fn place(&mut self, name: &'a str, tree: &Tree<'a>) -> u32 {
        enum Kid<'t, 'a> {
            Dir(&'t Tree<'a>),
            File(&'a [u8]),
        }
        let mut kids: Vec<(&'a str, Kid<'_, 'a>)> = tree
            .dirs
            .iter()
            .map(|(&n, t)| (n, Kid::Dir(t)))
            .chain(tree.files.iter().map(|(&n, &d)| (n, Kid::File(d))))
            .collect();
        kids.sort_by_key(|(n, _)| *n);

        let id = self.dirs.len() as u32;
        let first = self.slots.len() as u32;
        self.dirs.push((name, first, kids.len() as u32));
        self.slots.extend(kids.iter().map(|_| Slot::File(0)));
        for (i, (kid_name, kid)) in kids.into_iter().enumerate() {
            self.slots[first as usize + i] = match kid {
                Kid::Dir(t) => Slot::Dir(self.place(kid_name, t)),
                Kid::File(data) => {
                    self.files.push((kid_name, data));
                    Slot::File(self.files.len() as u32 - 1)
                }
            };
        }
        id
    }

    fn child(&self, dir: u32, name: &str) -> Option<u32> {
        let (_, first, count) = self.dirs[dir as usize];
        self.slots[first as usize..][..count as usize]
            .iter()
            .find_map(|s| match s {
                Slot::Dir(d) if self.dirs[*d as usize].0 == name => Some(*d),
                _ => None,
            })
    }
}

fn align(n: usize) -> usize {
    n.next_multiple_of(ALIGN as usize)
}

pub fn build(packages: &[Package]) -> Vec<u8> {
    let trees: Vec<Tree> = packages
        .iter()
        .map(|p| {
            let mut root = Tree::default();
            for &(path, data) in p.files {
                let mut parts: Vec<&str> = path.split('/').collect();
                let file = parts.pop().expect("split yields one part");
                let mut node = &mut root;
                for part in parts {
                    node = node.dirs.entry(part).or_default();
                }
                node.files.insert(file, data);
            }
            root
        })
        .collect();
    let mut layout = Layout::default();
    let roots: Vec<(u32, Option<u32>)> = trees
        .iter()
        .map(|t| {
            let root = layout.place("", t);
            (root, layout.child(root, "romfs"))
        })
        .collect();

    let mut pool = Vec::new();
    let mut name = |s: &str| {
        let off = pool.len() as u32;
        pool.extend_from_slice(s.as_bytes());
        (off, s.len() as u32)
    };
    let dir_names: Vec<_> = layout.dirs.iter().map(|d| name(d.0)).collect();
    let file_names: Vec<_> = layout.files.iter().map(|f| name(f.0)).collect();
    let stored: Vec<(Vec<u8>, u8)> = layout
        .files
        .iter()
        .map(|(_, data)| match zstd::bulk::compress(data, 3) {
            Ok(z) if z.len() < data.len() => (z, 1),
            _ => (data.to_vec(), 0),
        })
        .collect();

    let (p, d, f) = (packages.len(), layout.dirs.len(), layout.files.len());
    let pool_off = TOC_HEAD + p * PACKAGE + d * DIR + f * FILE + layout.slots.len() * 4;
    let toc_size = pool_off + pool.len();
    let blob = align(HEADER + toc_size);
    let mut offsets = Vec::new();
    let mut end = blob;
    for (bytes, _) in &stored {
        if bytes.is_empty() {
            offsets.push(0);
        } else {
            end = align(end);
            offsets.push(end);
            end += bytes.len();
        }
    }

    let mut toc = Vec::with_capacity(toc_size);
    toc.extend(TOC_MAGIC);
    toc.extend((p as u32).to_le_bytes());
    toc.extend((d as u64).to_le_bytes());
    toc.extend((f as u64).to_le_bytes());
    toc.extend((pool_off as u64).to_le_bytes());
    toc.extend((pool.len() as u64).to_le_bytes());
    toc.extend(0u64.to_le_bytes());
    for (pkg, (root, romfs)) in packages.iter().zip(&roots) {
        toc.extend(pkg.title_id.to_le_bytes());
        toc.extend(root.to_le_bytes());
        toc.extend(NONE.to_le_bytes());
        toc.extend(romfs.unwrap_or(NONE).to_le_bytes());
        toc.extend(pkg.version.to_le_bytes());
        toc.push(pkg.kind);
        toc.extend([0; 0x40 - 0x19]);
    }
    for ((_, first, count), (off, len)) in layout.dirs.iter().zip(&dir_names) {
        for v in [off, len, first, count] {
            toc.extend(v.to_le_bytes());
        }
        toc.extend(0u64.to_le_bytes());
    }
    for (i, ((_, data), (off, len))) in layout.files.iter().zip(&file_names).enumerate() {
        toc.extend(off.to_le_bytes());
        toc.extend(len.to_le_bytes());
        toc.extend((data.len() as u64).to_le_bytes());
        toc.extend((offsets[i] as u64).to_le_bytes());
        toc.extend((stored[i].0.len() as u64).to_le_bytes());
        toc.extend(Sha256::digest(data));
        toc.push(stored[i].1);
        toc.extend([0; 0x50 - 0x41]);
    }
    for slot in &layout.slots {
        let id = match *slot {
            Slot::Dir(i) => i,
            Slot::File(i) => d as u32 + i,
        };
        toc.extend(id.to_le_bytes());
    }
    toc.extend(&pool);
    assert_eq!(toc.len(), toc_size);

    let mut uuid = Sha256::new();
    for pkg in packages {
        uuid.update(pkg.title_id.to_le_bytes());
        uuid.update(pkg.version.to_le_bytes());
        uuid.update([pkg.kind]);
    }
    let mut out = Vec::with_capacity(end);
    out.extend(MAGIC);
    out.extend((HEADER as u16).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend((HEADER as u64).to_le_bytes());
    out.extend((toc_size as u64).to_le_bytes());
    out.extend(Sha256::digest(&toc));
    out.extend((blob as u64).to_le_bytes());
    out.extend((end as u64).to_le_bytes());
    out.extend((p as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(&uuid.finalize()[..16]);
    out.resize(HEADER, 0);
    out.extend(toc);
    for ((bytes, _), &at) in stored.iter().zip(&offsets) {
        if !bytes.is_empty() {
            out.resize(at, 0);
            out.extend(bytes);
        }
    }
    out.resize(end, 0);
    out
}

/// Where package `i`'s record starts, for tests that edit it.
pub fn package_record(i: usize) -> usize {
    HEADER + TOC_HEAD + i * PACKAGE
}

/// Where file `i`'s record starts, for tests that edit it.
pub fn file_record(nup: &[u8], i: usize) -> usize {
    let toc = &nup[HEADER..];
    let packages = u32::from_le_bytes(toc[4..8].try_into().unwrap()) as usize;
    let dirs = u64::from_le_bytes(toc[8..16].try_into().unwrap()) as usize;
    HEADER + TOC_HEAD + packages * PACKAGE + dirs * DIR + i * FILE
}

/// Recompute the TOC hash after a test edited the TOC.
pub fn rehash(nup: &mut [u8]) {
    let size = u64::from_le_bytes(nup[0x10..0x18].try_into().unwrap()) as usize;
    let hash = Sha256::digest(&nup[HEADER..HEADER + size]);
    nup[0x18..0x38].copy_from_slice(&hash);
}
