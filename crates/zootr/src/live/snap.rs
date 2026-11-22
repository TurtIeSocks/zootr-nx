//! A search for the values that follow a state of the game, as a cheat
//! table finds them. A snap is taken in one state or the other, each
//! named by a label: the first saves memory, the first in the other state
//! keeps the 4-byte values that differ, and each after keeps those that
//! read what they read before in that state.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

use super::Memory;

/// How far a snap reads at once, so a big region never stalls the game.
const CHUNK: usize = 1 << 20;
/// The first snap: every region's bytes.
const FULL: &str = "full";
/// The values left: their addresses and what each state read there.
const LEFT: &str = "left";

/// A value still in the search: where it is, and what each state read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Value {
    pub at: u64,
    pub reads: [u32; 2],
}

/// What a snap did.
#[derive(Debug, PartialEq)]
pub enum Snapped {
    /// The first: this many bytes saved, for a snap in the other state.
    Saved(u64),
    /// The values left, and the two states' labels.
    Left([String; 2], Vec<Value>),
}

/// Snap the memory `mem` maps in `regions` (each a start and a size) in
/// the state `label`, into the search kept in `dir`.
pub fn snap(
    mem: &impl Memory,
    regions: &[(u64, usize)],
    dir: &Path,
    label: &str,
) -> Result<Snapped> {
    fs::create_dir_all(dir)?;
    let (full, left) = (dir.join(FULL), dir.join(LEFT));
    if left.exists() {
        let (labels, values) = load(&left)?;
        let Some(state) = labels.iter().position(|l| l == label) else {
            bail!("the search is between {} and {}", labels[0], labels[1])
        };
        let values = kept(mem, values, state)?;
        save(&left, &labels, &values)?;
        return Ok(Snapped::Left(labels, values));
    }
    if full.exists() {
        let mut file = BufReader::new(File::open(&full)?);
        let first = string(&mut file)?;
        ensure!(
            first != label,
            "the first snap was {label}: take one in the other state"
        );
        let labels = [first, label.to_string()];
        let values = changed(mem, &mut file)?;
        save(&left, &labels, &values)?;
        fs::remove_file(&full)?;
        return Ok(Snapped::Left(labels, values));
    }
    let mut file = BufWriter::new(File::create(&full)?);
    put_string(&mut file, label)?;
    let mut saved = 0;
    for &(start, size) in regions {
        // A region unmapped since the list was made is left out.
        let Ok(bytes) = read(mem, start, size) else {
            continue;
        };
        file.write_all(&start.to_le_bytes())?;
        file.write_all(&(bytes.len() as u64).to_le_bytes())?;
        file.write_all(&bytes)?;
        saved += bytes.len() as u64;
    }
    file.flush()?;
    Ok(Snapped::Saved(saved))
}

/// `size` bytes at `start`, read a chunk at a time.
fn read(mem: &impl Memory, start: u64, size: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(size);
    for from in (0..size).step_by(CHUNK) {
        bytes.extend(mem.read(start + from as u64, CHUNK.min(size - from))?);
    }
    Ok(bytes)
}

fn words(bytes: &[u8]) -> impl Iterator<Item = (usize, u32)> + '_ {
    (bytes.as_chunks::<4>().0.iter().enumerate()).map(|(i, w)| (4 * i, u32::from_le_bytes(*w)))
}

/// The values the saved regions in `file` read that `mem` now reads
/// differently.
fn changed(mem: &impl Memory, file: &mut impl Read) -> Result<Vec<Value>> {
    let mut values = vec![];
    let mut head = [0; 16];
    while read_exact_or_end(file, &mut head)? {
        let start = u64::from_le_bytes(head[..8].try_into()?);
        let size = u64::from_le_bytes(head[8..].try_into()?) as usize;
        for from in (0..size).step_by(CHUNK) {
            let len = CHUNK.min(size - from);
            let mut was = vec![0; len];
            file.read_exact(&mut was)?;
            let at = start + from as u64;
            let Ok(now) = mem.read(at, len) else { continue };
            for ((i, a), (_, b)) in words(&was).zip(words(&now)) {
                if a != b {
                    values.push(Value {
                        at: at + i as u64,
                        reads: [a, b],
                    });
                }
            }
        }
    }
    Ok(values)
}

/// `values` whose read in state `state` holds now, each read once a
/// chunk at a time.
fn kept(mem: &impl Memory, values: Vec<Value>, state: usize) -> Result<Vec<Value>> {
    let mut out = Vec::with_capacity(values.len());
    let mut i = 0;
    while i < values.len() {
        let from = values[i].at;
        let end = (values[i..].iter())
            .take_while(|v| v.at + 4 <= from + CHUNK as u64)
            .count();
        let last = values[i + end - 1].at + 4;
        if let Ok(now) = mem.read(from, (last - from) as usize) {
            for v in &values[i..i + end] {
                let at = (v.at - from) as usize;
                let read = u32::from_le_bytes(now[at..at + 4].try_into()?);
                if read == v.reads[state] {
                    out.push(*v);
                }
            }
        }
        i += end;
    }
    Ok(out)
}

/// Fill `buf`, or say the file ended before it.
fn read_exact_or_end(file: &mut impl Read, buf: &mut [u8]) -> Result<bool> {
    match file.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn put_string(file: &mut impl Write, s: &str) -> Result<()> {
    file.write_all(&(s.len() as u32).to_le_bytes())?;
    Ok(file.write_all(s.as_bytes())?)
}

fn string(file: &mut impl Read) -> Result<String> {
    let mut len = [0; 4];
    file.read_exact(&mut len)?;
    let mut s = vec![0; u32::from_le_bytes(len) as usize];
    file.read_exact(&mut s)?;
    Ok(String::from_utf8(s)?)
}

fn save(path: &Path, labels: &[String; 2], values: &[Value]) -> Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    labels.iter().try_for_each(|l| put_string(&mut file, l))?;
    for v in values {
        file.write_all(&v.at.to_le_bytes())?;
        file.write_all(&v.reads[0].to_le_bytes())?;
        file.write_all(&v.reads[1].to_le_bytes())?;
    }
    Ok(file.flush()?)
}

fn load(path: &Path) -> Result<([String; 2], Vec<Value>)> {
    let mut file = BufReader::new(File::open(path).context("no search")?);
    let labels = [string(&mut file)?, string(&mut file)?];
    let mut values = vec![];
    let mut row = [0; 16];
    while read_exact_or_end(&mut file, &mut row)? {
        let at = u64::from_le_bytes(row[..8].try_into()?);
        let a = u32::from_le_bytes(row[8..12].try_into()?);
        let b = u32::from_le_bytes(row[12..].try_into()?);
        values.push(Value { at, reads: [a, b] });
    }
    Ok((labels, values))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fake;
    use super::*;

    const BASE: u64 = 0x1000;

    fn put(mem: &Fake, at: u64, v: u32) {
        mem.write(BASE + at, &v.to_le_bytes()).unwrap();
    }

    /// A fresh folder for test `name`'s search.
    fn search(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-snap-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn left(s: Snapped) -> Vec<u64> {
        match s {
            Snapped::Left(_, values) => values.iter().map(|v| v.at - BASE).collect(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_value_that_follows_the_state_is_all_that_is_left() {
        let dir = search("follows");
        let mem = Fake::new(vec![(BASE, vec![0; 3 * CHUNK])]);
        let regions = [(BASE, 3 * CHUNK)];
        let snap = |label| snap(&mem, &regions, &dir, label).unwrap();
        // The prompt's value at 0x10, a counter at 0x20, and a value past
        // the first chunk that follows the state too.
        put(&mem, 0x10, 16);
        put(&mem, CHUNK as u64 + 8, 1);
        assert_eq!(snap("on"), Snapped::Saved(3 * CHUNK as u64));
        put(&mem, 0x10, 0);
        put(&mem, 0x20, 1);
        put(&mem, CHUNK as u64 + 8, 0);
        assert_eq!(left(snap("off")), [0x10, 0x20, CHUNK as u64 + 8]);
        put(&mem, 0x10, 16);
        put(&mem, 0x20, 2);
        put(&mem, CHUNK as u64 + 8, 1);
        assert_eq!(left(snap("on")), [0x10, CHUNK as u64 + 8]);
        put(&mem, 0x10, 0);
        assert_eq!(left(snap("off")), [0x10]);
    }

    #[test]
    fn a_snap_names_one_of_the_two_states() {
        let dir = search("states");
        let mem = Fake::new(vec![(BASE, vec![0; 64])]);
        let snap = |label| snap(&mem, &[(BASE, 64)], &dir, label);
        snap("on").unwrap();
        assert!(snap("on").is_err(), "the same state twice");
        put(&mem, 0, 1);
        snap("off").unwrap();
        let err = snap("aiming").unwrap_err().to_string();
        assert_eq!(err, "the search is between on and off");
    }
}
