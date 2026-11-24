//! RESTBL: the resource size table in `System/Resource/*.rsizetable.zs`.
//!
//! The game sizes each file's load buffer from this table, keyed by the
//! romfs path without its `.zs` suffix. A file that outgrows its entry fails
//! to load, so every write that grows a file must grow its entry too.
//!
//! Layout (little-endian): `"RESTBL"`, version, name length, CRC entry count,
//! name entry count; then `(crc32(path), size)` pairs sorted by CRC; then
//! `(path padded to name length, size)` entries for paths whose CRCs collide.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};

use crate::crc32;

const MAGIC: &[u8; 6] = b"RESTBL";
const HEADER_LEN: usize = 22;

pub struct Restbl {
    version: u32,
    name_len: usize,
    crcs: BTreeMap<u32, u32>,
    /// Kept in file order; the game searches these before the CRC table.
    names: Vec<(String, u32)>,
}

fn u32_at(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}

const fn round_up_32(n: usize) -> usize {
    (n + 31) & !31
}

impl Restbl {
    pub fn parse(data: &[u8]) -> Result<Self> {
        ensure!(
            data.len() >= HEADER_LEN && &data[..6] == MAGIC,
            "not a RESTBL"
        );
        let version = u32_at(data, 6);
        let name_len = u32_at(data, 10) as usize;
        let crc_count = u32_at(data, 14) as usize;
        let name_count = u32_at(data, 18) as usize;
        let names_at = HEADER_LEN + crc_count * 8;
        let expected = names_at + name_count * (name_len + 4);
        ensure!(
            data.len() == expected,
            "RESTBL is {} bytes, header implies {expected}",
            data.len()
        );

        let crcs = (0..crc_count)
            .map(|i| {
                let off = HEADER_LEN + i * 8;
                (u32_at(data, off), u32_at(data, off + 4))
            })
            .collect();
        let names = (0..name_count)
            .map(|i| {
                let off = names_at + i * (name_len + 4);
                let raw = &data[off..off + name_len];
                let end = raw.iter().position(|&b| b == 0).unwrap_or(name_len);
                let name = String::from_utf8_lossy(&raw[..end]).into_owned();
                (name, u32_at(data, off + name_len))
            })
            .collect();
        Ok(Self {
            version,
            name_len,
            crcs,
            names,
        })
    }

    pub fn to_binary(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            HEADER_LEN + self.crcs.len() * 8 + self.names.len() * (self.name_len + 4),
        );
        out.extend_from_slice(MAGIC);
        for n in [
            self.version,
            self.name_len as u32,
            self.crcs.len() as u32,
            self.names.len() as u32,
        ] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        for (crc, size) in &self.crcs {
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
        }
        for (name, size) in &self.names {
            let mut padded = name.as_bytes().to_vec();
            padded.resize(self.name_len, 0);
            out.extend_from_slice(&padded);
            out.extend_from_slice(&size.to_le_bytes());
        }
        out
    }

    /// The table key for a romfs-relative path: the path minus `.zs`.
    fn key(path: &str) -> &str {
        path.strip_suffix(".zs").unwrap_or(path)
    }

    pub fn get(&self, path: &str) -> Option<u32> {
        let key = Self::key(path);
        self.names
            .iter()
            .find(|(n, _)| n == key)
            .map(|(_, s)| *s)
            .or_else(|| self.crcs.get(&crc32(key.as_bytes())).copied())
    }

    fn slot(&mut self, path: &str) -> Option<&mut u32> {
        let key = Self::key(path);
        match self.names.iter().position(|(n, _)| n == key) {
            Some(i) => Some(&mut self.names[i].1),
            None => self.crcs.get_mut(&crc32(key.as_bytes())),
        }
    }

    /// Account for `path` changing from `old_size` to `new_size` decompressed
    /// bytes. The entry grows by the 32-byte-rounded growth, which keeps the
    /// per-type overhead the shipped value already carries. Shrinking leaves
    /// the entry alone: a larger buffer than needed is harmless.
    pub fn resize(&mut self, path: &str, old_size: usize, new_size: usize) -> Result<()> {
        let Some(entry) = self.slot(path) else {
            bail!("{path} has no RESTBL entry; add it instead");
        };
        *entry = grown(*entry, old_size, new_size)?;
        Ok(())
    }

    /// Like [`Self::resize`], but the entry also grows in proportion to the
    /// file: added byml nodes cost the game more than their bytes, and the
    /// Mac build refuses a file whose parse outgrows its entry.
    pub fn scale(&mut self, path: &str, old_size: usize, new_size: usize) -> Result<()> {
        let scaled = self.get(path).map_or(0, |e| e as usize * new_size / old_size.max(1));
        self.resize(path, old_size, new_size)?;
        self.raise(path, u32::try_from(scaled)?)
    }

    /// Raise `path`'s entry to at least `size`.
    pub fn raise(&mut self, path: &str, size: u32) -> Result<()> {
        let entry = self
            .slot(path)
            .with_context(|| format!("{path} has no RESTBL entry"))?;
        *entry = (*entry).max(size);
        Ok(())
    }

    /// The entry a new file of `new_size` bytes needs, sized from `like`,
    /// a file of the same type that is `like_size` bytes.
    pub fn like(&self, like: &str, like_size: usize, new_size: usize) -> Result<u32> {
        let entry = self
            .get(like)
            .with_context(|| format!("{like} has no RESTBL entry to size a new file by"))?;
        grown(entry, like_size, new_size)
    }

    /// Add an entry for a file the game doesn't ship. One whose CRC another
    /// path already holds goes to the names table, which the game searches
    /// first.
    pub fn add(&mut self, path: &str, size: u32) -> Result<()> {
        let key = Self::key(path);
        ensure!(
            !self.names.iter().any(|(n, _)| n == key),
            "{key} already has a RESTBL entry"
        );
        ensure!(
            key.len() < self.name_len,
            "{key} is too long for the names table"
        );
        let crc = crc32(key.as_bytes());
        match self.crcs.contains_key(&crc) {
            true => self.names.push((key.to_owned(), size)),
            false => {
                self.crcs.insert(crc, size);
            }
        }
        Ok(())
    }
}

/// `entry` grown by the 32-byte-rounded growth from `old_size` to
/// `new_size`, never shrunk.
fn grown(entry: u32, old_size: usize, new_size: usize) -> Result<u32> {
    let growth = round_up_32(new_size).saturating_sub(round_up_32(old_size));
    Ok(u32::try_from(entry as usize + growth)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Restbl {
        Restbl {
            version: 1,
            name_len: 160,
            crcs: BTreeMap::from([(crc32(b"A/b.byml"), 1000)]),
            names: vec![("C/d.bgyml".into(), 500)],
        }
    }

    #[test]
    fn round_trips() {
        let t = sample();
        let bin = t.to_binary();
        assert_eq!(Restbl::parse(&bin).unwrap().to_binary(), bin);
    }

    #[test]
    fn looks_up_by_name_and_by_crc_without_zs() {
        let t = sample();
        assert_eq!(t.get("A/b.byml.zs"), Some(1000));
        assert_eq!(t.get("C/d.bgyml"), Some(500));
        assert_eq!(t.get("E/f.byml"), None);
    }

    #[test]
    fn grows_by_rounded_growth_and_never_shrinks() {
        let mut t = sample();
        t.resize("A/b.byml.zs", 100, 140).unwrap(); // 128 -> 160
        assert_eq!(t.get("A/b.byml"), Some(1032));
        t.resize("A/b.byml.zs", 140, 10).unwrap();
        assert_eq!(t.get("A/b.byml"), Some(1032));
        assert!(t.resize("E/f.byml", 1, 2).is_err());
    }

    #[test]
    fn scales_with_its_file_and_grows_at_least_by_bytes() {
        let mut t = sample();
        t.scale("A/b.byml.zs", 100, 150).unwrap();
        assert_eq!(t.get("A/b.byml"), Some(1500));
        t.scale("A/b.byml.zs", 2000, 2040).unwrap(); // 2016 -> 2048 beats 1530
        assert_eq!(t.get("A/b.byml"), Some(1532));
        t.scale("A/b.byml.zs", 150, 10).unwrap();
        assert_eq!(t.get("A/b.byml"), Some(1532));
        assert!(t.scale("E/f.byml", 1, 2).is_err());
    }

    #[test]
    fn raising_an_entry_never_lowers_it() {
        let mut t = sample();
        t.raise("A/b.byml.zs", 1200).unwrap();
        t.raise("A/b.byml.zs", 900).unwrap();
        assert_eq!(t.get("A/b.byml"), Some(1200));
        assert!(t.raise("X/y.byml", 1).is_err());
    }

    #[test]
    fn adds_new_files_sized_like_another() {
        let mut t = sample();
        let size = t.like("A/b.byml", 100, 140).unwrap();
        assert_eq!(size, 1032);
        t.add("E/f.byml.zs", size).unwrap();
        assert_eq!(t.get("E/f.byml"), Some(1032));
        assert!(t.add("C/d.bgyml", 1).is_err());
        assert!(t.like("G/h.byml", 1, 2).is_err());
    }

    #[test]
    fn a_colliding_crc_goes_to_the_names_table() {
        let mut t = sample();
        // A path is its own collision: the CRC table already holds it.
        t.add("A/b.byml", 7).unwrap();
        assert_eq!(t.names.last(), Some(&("A/b.byml".into(), 7)));
        assert_eq!(t.get("A/b.byml"), Some(7));
        let bin = t.to_binary();
        assert_eq!(Restbl::parse(&bin).unwrap().get("A/b.byml"), Some(7));
    }
}
