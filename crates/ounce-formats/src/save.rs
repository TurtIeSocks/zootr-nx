//! Save files (`slot_NN/progress.sav` and friends): GameData values keyed by
//! name hash, grouped by type. Same layout as Tears of the Kingdom's.
//!
//! After a 0x20 byte header comes a table of `(hash, value)` pairs. A pair
//! with hash 0 starts the group for type `value`. Scalars keep their value
//! in the pair; every other type keeps an offset to its data.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};

const MAGIC: u32 = 0x0102_0304;
const TABLE: usize = 0x20;

/// GameData table names, in group order.
const TYPES: [&str; 33] = [
    "Bool",
    "BoolArray",
    "Int",
    "IntArray",
    "Float",
    "FloatArray",
    "Enum",
    "EnumArray",
    "Vector2",
    "Vector2Array",
    "Vector3",
    "Vector3Array",
    "String16",
    "String16Array",
    "String32",
    "String32Array",
    "String64",
    "String64Array",
    "Binary",
    "BinaryArray",
    "UInt",
    "UIntArray",
    "Int64",
    "Int64Array",
    "UInt64",
    "UInt64Array",
    "WString16",
    "WString16Array",
    "WString32",
    "WString32Array",
    "WString64",
    "WString64Array",
    "Bool64bitKey",
];

const INLINE: [&str; 5] = ["Bool", "Int", "Float", "Enum", "UInt"];

/// Arrays of 4-byte values: a count, then the values.
const ARRAYS: [&str; 4] = ["IntArray", "FloatArray", "EnumArray", "UIntArray"];

#[derive(Clone, Copy)]
pub struct Entry {
    pub hash: u32,
    pub kind: &'static str,
    /// Offset of the pair's value field.
    pos: usize,
}

pub struct Save {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
    index: HashMap<u32, usize>,
    /// Where each non-scalar's data starts, sorted, then the file's end.
    starts: Vec<usize>,
}

impl Save {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let word = |pos: usize| -> Result<u32> {
            let b = bytes.get(pos..pos + 4).context("save ends early")?;
            Ok(u32::from_le_bytes(b.try_into()?))
        };
        ensure!(word(0)? == MAGIC, "not a save file");
        let end = word(8)? as usize;
        ensure!(
            end <= bytes.len() && (end - TABLE).is_multiple_of(8),
            "bad table end {end:#x}"
        );
        let (mut entries, mut index, mut kind) = (Vec::new(), HashMap::new(), None);
        for pos in (TABLE..end).step_by(8) {
            let (hash, value) = (word(pos)?, word(pos + 4)?);
            if hash == 0 {
                kind = Some(*TYPES.get(value as usize).context("unknown save type")?);
                continue;
            }
            let Some(kind) = kind else {
                bail!("value before the first type marker")
            };
            index.insert(hash, entries.len());
            entries.push(Entry {
                hash,
                kind,
                pos: pos + 4,
            });
        }
        let mut starts: Vec<usize> = entries
            .iter()
            .filter(|e| !INLINE.contains(&e.kind))
            .map(|e| word(e.pos).map(|w| w as usize))
            .collect::<Result<_>>()?;
        starts.push(bytes.len());
        starts.sort_unstable();
        starts.dedup();
        Ok(Self {
            bytes,
            entries,
            index,
            starts,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, hash: u32) -> Option<Entry> {
        self.index.get(&hash).map(|&i| self.entries[i])
    }

    fn word(&self, pos: usize) -> Option<u32> {
        let b = self.bytes.get(pos..pos + 4)?;
        Some(u32::from_le_bytes(b.try_into().ok()?))
    }

    /// A scalar's raw 32 bits: a bool, int, float, enum value hash or uint.
    pub fn scalar(&self, e: Entry) -> Option<u32> {
        INLINE.contains(&e.kind).then(|| self.word(e.pos)).flatten()
    }

    /// Any entry's raw bytes: a scalar's 4, or another type's data up to
    /// where the next one starts.
    pub fn data(&self, e: Entry) -> &[u8] {
        if INLINE.contains(&e.kind) {
            return &self.bytes[e.pos..e.pos + 4];
        }
        let at = self.word(e.pos).unwrap_or_default() as usize;
        let end = self.starts[self.starts.partition_point(|&s| s <= at)..]
            .first()
            .copied()
            .unwrap_or(self.bytes.len());
        self.bytes.get(at..end).unwrap_or_default()
    }

    /// A 4-byte-element array's raw values.
    pub fn array(&self, e: Entry) -> Option<Vec<u32>> {
        if !ARRAYS.contains(&e.kind) {
            return None;
        }
        let at = self.word(e.pos)? as usize;
        let count = self.word(at)? as usize;
        (0..count).map(|i| self.word(at + 4 + i * 4)).collect()
    }

    /// A fixed-width string or string array, NUL padded: String16, String32,
    /// String64 and their arrays.
    pub fn strings(&self, e: Entry) -> Option<Vec<String>> {
        let width: usize = e
            .kind
            .strip_prefix("String")?
            .trim_end_matches("Array")
            .parse()
            .ok()?;
        let at = self.word(e.pos)? as usize;
        let (count, first) = match e.kind.ends_with("Array") {
            true => (self.word(at)? as usize, at + 4),
            false => (1, at),
        };
        (0..count)
            .map(|i| {
                let b = self.bytes.get(first + i * width..first + (i + 1) * width)?;
                let end = b.iter().position(|&c| c == 0).unwrap_or(width);
                Some(String::from_utf8_lossy(&b[..end]).into_owned())
            })
            .collect()
    }

    /// Overwrite a scalar in place. The file never changes size.
    pub fn set_scalar(&mut self, hash: u32, raw: u32) -> Result<()> {
        let e = self.get(hash).context("not in this save")?;
        ensure!(INLINE.contains(&e.kind), "{} isn't a scalar", e.kind);
        self.bytes[e.pos..e.pos + 4].copy_from_slice(&raw.to_le_bytes());
        Ok(())
    }

    /// A save's bytes holding just these table pairs, for tests.
    #[cfg(any(test, feature = "fixture"))]
    pub fn fixture(pairs: &[(u32, u32)]) -> Vec<u8> {
        let mut b = vec![0u8; TABLE];
        b[..4].copy_from_slice(&MAGIC.to_le_bytes());
        let end = TABLE + pairs.len() * 8;
        b[8..12].copy_from_slice(&(end as u32).to_le_bytes());
        for (h, v) in pairs {
            b.extend(h.to_le_bytes());
            b.extend(v.to_le_bytes());
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut b = Save::fixture(&[(0, 0), (11, 1), (0, 3), (22, 0), (0, 32)]);
        // IntArray 22 points here: two values.
        let at = b.len() as u32;
        b[TABLE + 3 * 8 + 4..TABLE + 3 * 8 + 8].copy_from_slice(&at.to_le_bytes());
        for w in [2u32, 7, 9] {
            b.extend(w.to_le_bytes());
        }
        b
    }

    #[test]
    fn reads_and_patches() {
        let mut s = Save::parse(sample()).unwrap();
        let flag = s.get(11).unwrap();
        assert_eq!((flag.kind, s.scalar(flag)), ("Bool", Some(1)));
        assert_eq!(s.array(s.get(22).unwrap()), Some(vec![7, 9]));
        assert_eq!(s.data(flag), 1u32.to_le_bytes());
        let words: Vec<u8> = [2u32, 7, 9].iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(s.data(s.get(22).unwrap()), words);
        s.set_scalar(11, 0).unwrap();
        assert_eq!(s.scalar(flag), Some(0));
        assert!(s.set_scalar(22, 0).is_err());
        assert_eq!(s.bytes().len(), sample().len());
    }
}
