//! Same-length string swaps inside a model or texture file, where every
//! offset must stay put: a copy of one under a new name.
//!
//! Both formats find names through dictionaries: patricia trees keyed on
//! bits counted from each name's end. A swap that changes a name's last
//! bytes would leave its lookups missing, so every dictionary is rebuilt
//! from its renamed keys, in its own order.

use anyhow::{Result, ensure};

/// `data` with every `from` replaced by `to`, which must be as long, and
/// its dictionaries rebuilt.
pub fn rename(data: &[u8], from: &str, to: &str) -> Result<Vec<u8>> {
    let (from, to) = (from.as_bytes(), to.as_bytes());
    ensure!(
        from.len() == to.len() && !from.is_empty(),
        "a rename keeps the length"
    );
    let mut out = data.to_vec();
    let mut at = 0;
    while let Some(i) = out[at..].windows(from.len()).position(|w| w == from) {
        out[at + i..at + i + to.len()].copy_from_slice(to);
        at += i + to.len();
    }
    for dict in dicts(&out) {
        let keys: Vec<&[u8]> = dict.keys.iter().map(|k| &out[k.clone()]).collect();
        let nodes = tree(&keys)?;
        for (i, (bit, left, right)) in nodes.into_iter().enumerate() {
            let e = dict.at + 8 + 16 * i;
            out[e..e + 4].copy_from_slice(&bit.to_le_bytes());
            out[e + 4..e + 6].copy_from_slice(&left.to_le_bytes());
            out[e + 6..e + 8].copy_from_slice(&right.to_le_bytes());
        }
    }
    Ok(out)
}

/// A dictionary at `at`, and where each key's bytes are, root left out.
struct Dict {
    at: usize,
    keys: Vec<std::ops::Range<usize>>,
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().expect("4 bytes"))
}

/// The bytes of the length-prefixed, zero-ended name at `at`.
fn key(data: &[u8], at: usize) -> Option<std::ops::Range<usize>> {
    let len = u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize;
    let range = at + 2..at + 2 + len;
    (data.get(range.end) == Some(&0)).then_some(range)
}

/// Every dictionary: a magic (`_DIC`, or zero in newer models), a count,
/// then a root of bit -1 and an empty name, and an entry for each key.
fn dicts(data: &[u8]) -> Vec<Dict> {
    let mut found = Vec::new();
    for at in (0..data.len().saturating_sub(24)).step_by(4) {
        let magic = &data[at..at + 4];
        if (magic != b"_DIC" && magic != [0; 4]) || u32_at(data, at + 8) != u32::MAX {
            continue;
        }
        let count = u32_at(data, at + 4) as usize;
        let end = at + 8 + 16 * (count + 1);
        if !(1..4096).contains(&count) || end > data.len() {
            continue;
        }
        let name = |i: usize| {
            let e = at + 8 + 16 * i;
            u64::from_le_bytes(data[e + 8..e + 16].try_into().expect("8 bytes")) as usize
        };
        let root = name(0);
        if root != 0 && key(data, root).is_none_or(|k| !k.is_empty()) {
            continue;
        }
        let keys: Option<Vec<_>> = (1..=count)
            .map(|i| key(data, name(i)).filter(|k| !k.is_empty()))
            .collect();
        if let Some(keys) = keys {
            found.push(Dict { at, keys });
        }
    }
    found
}

/// Bit `b` of `key`, counted from its last byte's lowest bit.
fn bit(key: &[u8], b: i32) -> bool {
    let byte = (b >> 3) as usize;
    b >= 0 && byte < key.len() && key[key.len() - 1 - byte] >> (b & 7) & 1 == 1
}

/// The nodes, root first, as (bit, left, right): each key inserted in
/// turn, splitting at the first bit where it parts from the key its
/// search ends on.
fn tree(keys: &[&[u8]]) -> Result<Vec<(i32, u16, u16)>> {
    let mut nodes: Vec<(i32, u16, u16)> = vec![(-1, 0, 0)];
    let name = |i: u16| {
        if i == 0 {
            &[][..]
        } else {
            keys[i as usize - 1]
        }
    };
    let next = |nodes: &[(i32, u16, u16)], q: u16, k: &[u8]| {
        let (b, left, right) = nodes[q as usize];
        if bit(k, b) { right } else { left }
    };
    for (i, k) in keys.iter().enumerate() {
        let idx = u16::try_from(i + 1)?;
        let (mut p, mut q) = (0u16, nodes[0].1);
        while nodes[p as usize].0 < nodes[q as usize].0 {
            (p, q) = (q, next(&nodes, q, k));
        }
        let other = name(q);
        let bits = 8 * k.len().max(other.len()) as i32;
        let split = (0..bits).find(|&b| bit(k, b) != bit(other, b));
        let Some(split) = split else {
            anyhow::bail!("a dictionary holds one name twice");
        };
        (p, q) = (0, nodes[0].1);
        while nodes[p as usize].0 < nodes[q as usize].0 && nodes[q as usize].0 < split {
            (p, q) = (q, next(&nodes, q, k));
        }
        nodes.push(match bit(k, split) {
            true => (split, q, idx),
            false => (split, idx, q),
        });
        let parent = &mut nodes[p as usize];
        match p != 0 && bit(k, parent.0) {
            true => parent.2 = idx,
            false => parent.1 = idx,
        }
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of one dictionary over `names`, zeroed nodes, names after.
    fn file(names: &[&str]) -> Vec<u8> {
        let mut data = b"_DIC".to_vec();
        data.extend((names.len() as u32).to_le_bytes());
        let strings = 8 + 16 * (names.len() + 1);
        let mut pool = Vec::new();
        for i in 0..=names.len() {
            let at = match i {
                0 => 0,
                _ => strings + pool.len(),
            };
            data.extend([0; 8]);
            data.extend((at as u64).to_le_bytes());
            if i > 0 {
                let n = names[i - 1];
                pool.extend((n.len() as u16).to_le_bytes());
                pool.extend(n.as_bytes());
                pool.extend([0, 0]);
            }
        }
        data[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        data.extend(pool);
        data
    }

    fn nodes(data: &[u8]) -> Vec<(i32, u16, u16)> {
        let count = u32_at(data, 4) as usize;
        (0..=count)
            .map(|i| {
                let e = 8 + 16 * i;
                let b = u32_at(data, e) as i32;
                let side = |at| u16::from_le_bytes([data[at], data[at + 1]]);
                (b, side(e + 4), side(e + 6))
            })
            .collect()
    }

    #[test]
    fn every_copy_is_renamed_in_place() {
        let out = rename(b"\x02Item_A.Item_A\0Mt_Item_A_Alb", "Item_A", "Item_B").unwrap();
        assert_eq!(out, b"\x02Item_B.Item_B\0Mt_Item_B_Alb");
    }

    #[test]
    fn a_rename_that_changes_the_length_is_refused() {
        assert!(rename(b"Item_A", "Item_A", "Item_A_B").is_err());
    }

    #[test]
    fn a_lone_name_splits_at_the_lowest_set_bit_of_its_last_byte() {
        // 't' is 0b0111_0100 and '3' is 0b0011_0011, as in a real model.
        let forest = rename(&file(&["Tree_Forest"]), "Forest", "Forest").unwrap();
        assert_eq!(nodes(&forest), [(-1, 1, 0), (2, 0, 1)]);
        let song = rename(&file(&["Tree_Forest"]), "Forest", "Song03").unwrap();
        assert_eq!(nodes(&song), [(-1, 1, 0), (0, 0, 1)]);
    }

    #[test]
    fn every_name_is_found_by_walking_its_bits() {
        let names = [
            "Mt_Item_Medal_Forest",
            "Mt_Medal_Glaze",
            "Mt_Medal_Glass",
            "B",
        ];
        let data = rename(&file(&names), "Forest", "Song07").unwrap();
        let tree = nodes(&data);
        let renamed = names.map(|n| n.replace("Forest", "Song07"));
        for (i, name) in renamed.iter().enumerate() {
            let (mut p, mut q) = (0, tree[0].1 as usize);
            while tree[p].0 < tree[q].0 {
                p = q;
                q = match bit(name.as_bytes(), tree[q].0) {
                    true => tree[q].2 as usize,
                    false => tree[q].1 as usize,
                };
            }
            assert_eq!(q, i + 1, "{name}");
        }
    }
}
