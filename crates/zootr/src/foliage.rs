//! A map's foliage files (`Foliage/Placement/Scene/<area>/*.flnt`): the
//! moss, turf, stones and plants painted onto it, drawn in bulk rather than
//! placed as actors. Taking an actor away leaves the foliage painted on it
//! floating, so [`strip`] takes that out too.
//!
//! Two layouts, by the version in the first word:
//!
//! - 4: a 4 m grid. Cell keys (x, y, z bytes, 128 at the origin), each
//!   cell's first record, then 12-byte records leading with their spot in
//!   the cell in 1/256ths. Nothing follows the records.
//! - 8: the plant kinds' names, 256 bytes each, then 64-byte records leading
//!   with a world position, then data not yet decoded.

use anyhow::{Result, bail, ensure};

/// The header both layouts share. The count is at `+4`.
const HEAD: usize = 0x20;
const CELL: f32 = 4.0;
const CELL_RECORD: usize = 12;
const LIST_RECORD: usize = 64;
const NAME: usize = 0x100;
/// Where a list plant goes when it's stripped: far under the map. The data
/// after the records isn't decoded, so plants move rather than leave.
const SUNK: f32 = -10_000.0;

/// `file` without the foliage whose position `gone` picks, and how many
/// plants that was.
pub fn strip(file: &[u8], gone: impl FnMut([f32; 3]) -> bool) -> Result<(Vec<u8>, usize)> {
    ensure!(file.len() >= HEAD, "foliage file too short");
    match word(file, 0) {
        4 => strip_cells(file, gone),
        8 => sink_list(file, gone),
        v => bail!("foliage version {v}"),
    }
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn strip_cells(b: &[u8], mut gone: impl FnMut([f32; 3]) -> bool) -> Result<(Vec<u8>, usize)> {
    let n = word(b, 4) as usize;
    let starts = HEAD + 4 * n;
    let data = starts + 4 * (n + 1);
    ensure!(b.len() >= data, "foliage cells cut short");
    let total = word(b, starts + 4 * n) as usize;
    ensure!(
        b.len() == data + CELL_RECORD * total,
        "foliage cells: {} bytes, not {}",
        b.len(),
        data + CELL_RECORD * total
    );
    let (mut keys, mut firsts, mut records) = (Vec::new(), vec![0u32], Vec::new());
    let mut stripped = 0;
    for i in 0..n {
        let key = word(b, HEAD + 4 * i);
        let cell = [key & 0xff, (key >> 8) & 0xff, (key >> 16) & 0xff];
        let before = records.len();
        for j in word(b, starts + 4 * i)..word(b, starts + 4 * (i + 1)) {
            let r = &b[data + CELL_RECORD * j as usize..][..CELL_RECORD];
            let at =
                std::array::from_fn(|a| (cell[a] as f32 - 128.0 + f32::from(r[a]) / 256.0) * CELL);
            if gone(at) {
                stripped += 1;
            } else {
                records.extend_from_slice(r);
            }
        }
        let kept = (records.len() - before) / CELL_RECORD;
        if kept > 0 {
            keys.push(key);
            firsts.push(firsts.last().unwrap() + kept as u32);
        }
    }
    let mut out = b[..HEAD].to_vec();
    out[4..8].copy_from_slice(&(keys.len() as u32).to_le_bytes());
    keys.iter()
        .for_each(|k| out.extend_from_slice(&k.to_le_bytes()));
    firsts
        .iter()
        .for_each(|f| out.extend_from_slice(&f.to_le_bytes()));
    out.extend_from_slice(&records);
    Ok((out, stripped))
}

fn sink_list(b: &[u8], mut gone: impl FnMut([f32; 3]) -> bool) -> Result<(Vec<u8>, usize)> {
    let n = word(b, 4) as usize;
    let mut first = HEAD;
    while b.get(first + 1..first + 5) == Some(b"Work") {
        first += NAME;
    }
    ensure!(b.len() >= first + LIST_RECORD * n, "foliage list cut short");
    let mut out = b.to_vec();
    let mut stripped = 0;
    for i in 0..n {
        let r = first + LIST_RECORD * i;
        let f = |a: usize| f32::from_le_bytes(b[r + 4 * a..r + 4 * a + 4].try_into().unwrap());
        if gone([f(0), f(1), f(2)]) {
            out[r + 4..r + 8].copy_from_slice(&SUNK.to_le_bytes());
            stripped += 1;
        }
    }
    Ok((out, stripped))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(version: u32, n: usize) -> Vec<u8> {
        let mut b = vec![0; HEAD];
        b[0..4].copy_from_slice(&version.to_le_bytes());
        b[4..8].copy_from_slice(&(n as u32).to_le_bytes());
        b
    }

    /// Two cells: (0, 25, 0) with plants at x 0.5 and 2, and (1, 25, 0)
    /// with one at x 4.5.
    fn cells() -> Vec<u8> {
        let mut b = head(4, 2);
        for x in [128u32, 129] {
            b.extend_from_slice(&(x | 153 << 8 | 128 << 16).to_le_bytes());
        }
        for first in [0u32, 2, 3] {
            b.extend_from_slice(&first.to_le_bytes());
        }
        for local in [32u8, 128, 32] {
            b.extend_from_slice(&[local, 0, 0, 9, 9, 9, 9, 9, 9, 9, 9, 9]);
        }
        b
    }

    #[test]
    fn a_cell_plant_reads_its_spot_from_its_cell_and_its_bytes() {
        let mut seen = Vec::new();
        let (_, n) = strip(&cells(), |at| {
            seen.push(at);
            false
        })
        .unwrap();
        assert_eq!(n, 0);
        assert_eq!(
            seen,
            [[0.5, 100.0, 0.0], [2.0, 100.0, 0.0], [4.5, 100.0, 0.0]]
        );
    }

    #[test]
    fn stripping_cells_drops_records_and_empty_cells() {
        let (out, n) = strip(&cells(), |at| at[0] > 1.0).unwrap();
        assert_eq!(n, 2);
        // One cell, holding the plant at x 0.5.
        let mut want = head(4, 1);
        want.extend_from_slice(&(128u32 | 153 << 8 | 128 << 16).to_le_bytes());
        want.extend_from_slice(&0u32.to_le_bytes());
        want.extend_from_slice(&1u32.to_le_bytes());
        want.extend_from_slice(&[32, 0, 0, 9, 9, 9, 9, 9, 9, 9, 9, 9]);
        assert_eq!(out, want);
    }

    #[test]
    fn a_list_plant_sinks_and_the_rest_of_the_file_stays() {
        let mut b = head(8, 2);
        let mut name = vec![0; NAME];
        name[0] = 4;
        name[1..5].copy_from_slice(b"Work");
        b.extend_from_slice(&name);
        for x in [1.0f32, 50.0] {
            let mut r = vec![7; LIST_RECORD];
            for (a, v) in [x, 100.0, -5.0].iter().enumerate() {
                r[4 * a..4 * a + 4].copy_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(&r);
        }
        b.extend_from_slice(b"tail");
        let (out, n) = strip(&b, |at| at[0] > 10.0).unwrap();
        assert_eq!(n, 1);
        assert_eq!(out.len(), b.len());
        let second = HEAD + NAME + LIST_RECORD;
        assert_eq!(out[second + 4..second + 8], SUNK.to_le_bytes());
        assert_eq!(out[..second + 4], b[..second + 4]);
        assert_eq!(out[second + 8..], b[second + 8..]);
    }

    #[test]
    fn an_unknown_layout_or_a_short_file_fails() {
        assert!(strip(&head(5, 0), |_| true).is_err());
        let mut short = cells();
        short.pop();
        assert!(strip(&short, |_| true).is_err());
    }
}
