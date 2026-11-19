//! One GameData value by its hash: found once through the type tables'
//! maps, then read or written where it sits.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Result, bail};
use ounce_formats::murmur3;

use super::Shape::Inline;
use super::{Memory, Row, TYPES, items, manager, show, sized, table, word};
use crate::gamedata::GameData;
use crate::value;

/// A value's row of [`TYPES`] and its index in that table, or `None` for a
/// hash the game doesn't hold.
type Place = Option<(&'static Row, u32)>;

/// Where each value was found, so a table's map is read once per value
/// rather than at every read. Only while the manager stays put: a new one
/// starts over.
#[derive(Default)]
pub struct Places(Mutex<(u64, HashMap<u32, Place>)>);

impl Places {
    /// The places found under `manager`.
    fn under(&self, manager: u64) -> std::sync::MutexGuard<'_, (u64, HashMap<u32, Place>)> {
        let mut held = self.0.lock().unwrap();
        if held.0 != manager {
            *held = (manager, HashMap::new());
        }
        held
    }

    fn get(&self, manager: u64, hash: u32) -> Option<Place> {
        self.under(manager).1.get(&hash).copied()
    }

    fn put(&self, manager: u64, hash: u32, place: Place) {
        self.under(manager).1.insert(hash, place);
    }
}

/// A table's row of [`TYPES`] and where `hash`'s entry sits in it, in any
/// table the game keeps.
fn find(mem: &impl Memory, global: u64, hash: u32) -> Result<Option<(&'static Row, u64)>> {
    let manager = manager(mem, global)?;
    let Some((row, i)) = place(mem, manager, hash)? else {
        return Ok(None);
    };
    let entries = mem.u64(table(manager, row) + 0x10)?;
    Ok(Some((row, entries + u64::from(i) * row.2 as u64)))
}

/// `hash`'s place, from [`Memory::places`] once it's been found.
fn place(mem: &impl Memory, manager: u64, hash: u32) -> Result<Place> {
    let Some(places) = mem.places() else {
        return scan(mem, manager, hash);
    };
    if let Some(place) = places.get(manager, hash) {
        return Ok(place);
    }
    let place = scan(mem, manager, hash)?;
    places.put(manager, hash, place);
    Ok(place)
}

/// `hash`'s place, read from each table's map in turn.
fn scan(mem: &impl Memory, manager: u64, hash: u32) -> Result<Place> {
    for row in TYPES.iter() {
        let table = table(manager, row);
        let count = mem.u32(table + 8)?;
        let cap = mem.u32(table + 0x24)? as usize;
        let map = mem.read(mem.u64(table + 0x18)?, sized(row.1, cap * 8)?)?;
        let found = (map.as_chunks::<8>().0.iter()).find(|p| word(&p[..], 0) == hash);
        if let Some(i) = found.map(|p| word(&p[..], 4))
            && i < count
        {
            return Ok(Some((row, i)));
        }
    }
    Ok(None)
}

/// Whether each Bool in `hashes` is true, in one read once their places
/// are known: one request over a bridge. A hash the game doesn't hold as a
/// Bool reads false.
pub fn bools(mem: &impl Memory, global: u64, hashes: &[u32]) -> Result<Vec<bool>> {
    let manager = manager(mem, global)?;
    let bool_row = &TYPES[0];
    let (stride, Inline(at, _)) = (bool_row.2, bool_row.3) else {
        unreachable!("Bool is inline")
    };
    let mut wanted = Vec::new();
    for (n, &hash) in hashes.iter().enumerate() {
        if let Some((row, i)) = place(mem, manager, hash)?
            && std::ptr::eq(row, bool_row)
        {
            wanted.push((n, i));
        }
    }
    let mut out = vec![false; hashes.len()];
    if wanted.is_empty() {
        return Ok(out);
    }
    let entries = mem.u64(table(manager, bool_row) + 0x10)?;
    let ranges: Vec<_> = (wanted.iter())
        .map(|&(_, i)| (entries + u64::from(i) * stride as u64 + at as u64, 1))
        .collect();
    for ((n, _), b) in wanted.iter().zip(mem.read_many(&ranges)?) {
        out[*n] = b.first().is_some_and(|&b| b != 0);
    }
    Ok(out)
}

/// Where a value held inline keeps its current value: its type, address
/// and width. A scalar's saved copy sits right after. See [`encode`] for
/// what's writable.
pub fn locate(mem: &impl Memory, global: u64, hash: u32) -> Result<(&'static str, u64, usize)> {
    if let Some(((_, kind, _, Inline(off, len)), entry)) = find(mem, global, hash)? {
        return Ok((kind, entry + *off as u64, *len));
    }
    bail!("{hash:#010x} isn't a value the game holds inline")
}

/// The most array elements [`read`] spells out.
const SHOWN: usize = 64;

/// The current value of any entry the game holds, as the Live tab shows it:
/// an array's elements in brackets, a binary's size, a struct's members one
/// per line, `?` for one the game doesn't hold.
pub fn read(mem: &impl Memory, gd: &GameData, global: u64, hash: u32) -> Result<String> {
    if gd.kind(hash) == Some("Struct") {
        let lines: Vec<String> = gd
            .members(hash)?
            .into_iter()
            .map(|(member, value)| {
                let now = read(mem, gd, global, value).unwrap_or_else(|_| "?".into());
                format!("{} = {now}", gd.bare(member))
            })
            .collect();
        return Ok(lines.join("\n"));
    }
    let Some(((_, kind, stride, shape), entry)) = find(mem, global, hash)? else {
        bail!("{hash:#010x} isn't a value the game holds");
    };
    let items = items(mem, kind, &mem.read(entry, *stride)?, *shape)?;
    Ok(match *kind {
        "Binary" => format!("{} bytes", items[0].len()),
        k if k.ends_with("Array") => {
            let more = items.len().saturating_sub(SHOWN);
            let mut shown: Vec<_> = items.iter().take(SHOWN).map(|b| show(gd, k, b)).collect();
            if more > 0 {
                shown.push(format!("... {more} more"));
            }
            format!("[{}]", shown.join(", "))
        }
        k => show(gd, k, &items[0]),
    })
}

/// Writes the current value, and for a scalar the copy after it, which
/// is the one the game saves from. Data pages only.
pub fn set(mem: &impl Memory, global: u64, hash: u32, value: &str) -> Result<()> {
    let (kind, at, width) = locate(mem, global, hash)?;
    let bytes = value::encode(kind, value, width)?;
    let copies = if kind.starts_with("String") { 1 } else { 2 };
    mem.write(at, &bytes.repeat(copies))
}

/// A GameData name's hash, or a hash as given: `0x612320f9`, or
/// `#612320f9` as `probe data` and the live log print it.
pub fn hash(name: &str) -> Result<u32> {
    Ok(
        match (name.strip_prefix("0x")).or_else(|| name.strip_prefix('#')) {
            Some(hex) => u32::from_str_radix(hex, 16)?,
            None => murmur3(name.as_bytes()),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Fake, entry, game, hash, ints};
    use super::*;

    #[test]
    fn a_hash_is_taken_as_printed() {
        let h = 0x612320f9;
        assert_eq!(super::super::hash("0x612320f9").unwrap(), h);
        assert_eq!(super::super::hash("#612320f9").unwrap(), h);
        assert_eq!(
            super::super::hash("IsGet.Item_X").unwrap(),
            murmur3(b"IsGet.Item_X")
        );
        assert!(super::super::hash("#nothex").is_err());
    }

    #[derive(Debug, PartialEq)]
    enum Read {
        One(usize),
        /// A `read_many`, by its range count.
        Many(usize),
    }
    use Read::*;

    /// [`Fake`] keeping [`Places`] as a [`Game`] does, with its reads.
    struct Kept(Fake, Places, std::cell::RefCell<Vec<Read>>);

    impl Kept {
        fn reads(&self) -> Vec<Read> {
            self.2.take()
        }
    }

    impl Memory for Kept {
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            self.2.borrow_mut().push(One(len));
            self.0.read(at, len)
        }

        fn write(&self, _: u64, _: &[u8]) -> Result<()> {
            bail!("read only")
        }

        fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            self.2.borrow_mut().push(Many(ranges.len()));
            ranges
                .iter()
                .map(|&(at, len)| self.0.read(at, len))
                .collect()
        }

        fn places(&self) -> Option<&Places> {
            Some(&self.1)
        }
    }

    fn kept(fake: Fake) -> Kept {
        Kept(fake, Places::default(), Default::default())
    }

    #[test]
    fn a_kept_place_reads_no_map_again() {
        let mem = kept(ints([5, 7]));
        let first = locate(&mem, 0x1000, hash(2, 1)).unwrap();
        mem.reads();
        let (_, at, _) = locate(&mem, 0x1000, hash(2, 1)).unwrap();
        assert_eq!(at, first.1);
        assert_eq!(mem.u32(at).unwrap(), 7);
        // The manager, the table's entries, then the value.
        assert_eq!(mem.reads(), [One(8), One(8), One(8), One(4)]);
    }

    #[test]
    fn bools_read_every_flag_in_one_request() {
        let on = |b: u8| entry(0, &[(0x16, &[b])]);
        let int = entry(2, &[(0x1c, &1u32.to_le_bytes())]);
        let mem = kept(game(&[(0, vec![on(1), on(0), on(1)]), (2, vec![int])], &[]));
        let hashes = [hash(0, 0), hash(0, 1), hash(0, 2), hash(2, 0), 0xdead];
        let want = [true, false, true, false, false];
        assert_eq!(bools(&mem, 0x1000, &hashes).unwrap(), want);
        mem.reads();
        assert_eq!(bools(&mem, 0x1000, &hashes).unwrap(), want);
        // The manager, the Bool table's entries, then the flags at once:
        // no map again, even for the hash the game doesn't hold.
        assert_eq!(mem.reads(), [One(8), One(8), One(8), Many(3)]);
    }

    #[test]
    fn a_new_manager_forgets_every_place() {
        let places = Places::default();
        places.put(1, 7, Some((&TYPES[0], 3)));
        assert!(places.get(1, 7).is_some());
        assert!(places.get(2, 7).is_none());
        assert!(places.get(1, 7).is_none());
    }

    #[test]
    fn locate_finds_a_scalar_through_its_map() {
        let mem = ints([5, 7]);
        let (kind, at, len) = locate(&mem, 0x1000, hash(2, 1)).unwrap();
        assert_eq!((kind, at, len), ("Int", 0x10000 + 0x28 + 0x1c, 4));
        assert_eq!(mem.u32(at).unwrap(), 7);
        assert!(locate(&mem, 0x1000, 0xdead).is_err());
    }

    #[test]
    fn read_shows_any_kind_the_way_the_live_tab_does() {
        needs_names!();
        let gd = GameData::default();
        let list = entry(
            3,
            &[
                (0x20, &3u64.to_le_bytes()),
                (0x28, &0x50000u64.to_le_bytes()),
            ],
        );
        let items: Vec<u8> = [1i32, -2, 3]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect();
        let blob = entry(
            18,
            &[
                (0x20, &0x60000u64.to_le_bytes()),
                (0x28, &0x300u64.to_le_bytes()),
            ],
        );
        let mem = game(
            &[
                (2, vec![entry(2, &[(0x1c, &(-5i32).to_le_bytes())])]),
                (3, vec![list]),
                (18, vec![blob]),
            ],
            &[(0x50000, items), (0x60000, vec![0; 0x300])],
        );
        let at = |slot| read(&mem, &gd, 0x1000, hash(slot, 0)).unwrap();
        assert_eq!(at(2), "-5");
        assert_eq!(at(3), "[1, -2, 3]");
        assert_eq!(at(18), "768 bytes");
        assert!(read(&mem, &gd, 0x1000, 0xdead).is_err());
    }
}
