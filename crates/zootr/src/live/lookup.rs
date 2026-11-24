//! One GameData value by its hash: found once through the type tables'
//! maps, then read or written where it sits.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{Result, bail};
use ounce_formats::murmur3;

use super::Shape::Inline;
use super::{
    COUNT, MAP, MAP_CAP, Memory, Row, TYPES, entries_pointer, items, manager, quad, show, sized,
    table, word,
};
use crate::gamedata::GameData;
use crate::value;

/// A value's row of [`TYPES`] and its index in that table, or `None` for a
/// hash the game doesn't hold.
type Place = Option<(&'static Row, u32)>;

/// Where values were found: each one's place in its table, each table's
/// entries, and the pointers to the manager. All of it is checked as it's
/// used, never trusted.
#[derive(Default)]
pub struct Places(Mutex<Kept>);

#[derive(Default)]
struct Kept {
    /// The manager the places and entries were found under.
    manager: u64,
    places: HashMap<u32, Place>,
    /// Each table's entries, by its row.
    entries: HashMap<u64, u64>,
    /// The global, the word it holds, and the manager that word points at.
    chain: Option<(u64, u64, u64)>,
}

impl Places {
    /// What was found under `manager`.
    fn under(&self, manager: u64) -> std::sync::MutexGuard<'_, Kept> {
        let mut held = self.0.lock().unwrap();
        if held.manager != manager {
            held.manager = manager;
            held.places.clear();
            held.entries.clear();
        }
        held
    }

    fn get(&self, manager: u64, hash: u32) -> Option<Place> {
        self.under(manager).places.get(&hash).copied()
    }

    fn put(&self, manager: u64, hash: u32, place: Place) {
        self.under(manager).places.insert(hash, place);
    }

    /// `global`'s word and the manager, as last read.
    fn chain(&self, global: u64) -> Option<(u64, u64)> {
        let held = self.0.lock().unwrap();
        (held.chain).and_then(|(g, word, manager)| (g == global).then_some((word, manager)))
    }

    /// Forgets the pointers, once they've moved.
    fn forget(&self) {
        let mut held = self.0.lock().unwrap();
        held.chain = None;
        held.entries.clear();
    }
}

/// The manager's pointer `global` holds, and the manager, from `places`
/// when kept. Whether they were read just now.
fn chain(mem: &impl Memory, places: &Places, global: u64) -> Result<(u64, u64, bool)> {
    if let Some((word, manager)) = places.chain(global) {
        return Ok((word, manager, false));
    }
    let word = mem.u64(global)?;
    let manager = mem.u64(word)?;
    places.0.lock().unwrap().chain = Some((global, word, manager));
    Ok((word, manager, true))
}

/// `row`'s entries under `manager`, from `places` when kept. Whether they
/// were read just now.
fn entries(mem: &impl Memory, places: &Places, manager: u64, row: &Row) -> Result<(u64, bool)> {
    if let Some(&at) = places.under(manager).entries.get(&row.0) {
        return Ok((at, false));
    }
    let at = mem.u64(entries_pointer(manager, row))?;
    places.under(manager).entries.insert(row.0, at);
    Ok((at, true))
}

/// Where each hash's entry is, as far as `places` knows, and the kept
/// pointers it was found through: the words to read, and what each should
/// hold. `check` and `expect` are empty when every pointer was read just
/// now.
struct Spots {
    at: Vec<Option<(&'static Row, u64)>>,
    check: Vec<(u64, usize)>,
    expect: Vec<u64>,
}

fn spots(mem: &impl Memory, places: &Places, global: u64, hashes: &[u32]) -> Result<Spots> {
    let (word, manager, mut fresh) = chain(mem, places, global)?;
    let mut s = Spots {
        at: Vec::new(),
        check: vec![(global, 8), (word, 8)],
        expect: vec![word, manager],
    };
    for found in places_of(mem, manager, hashes)? {
        let at = match found {
            Some((row, i)) => {
                let (list, read) = entries(mem, places, manager, row)?;
                fresh &= read;
                let pointer = entries_pointer(manager, row);
                if !s.check.contains(&(pointer, 8)) {
                    s.check.push((pointer, 8));
                    s.expect.push(list);
                }
                Some((row, list + u64::from(i) * row.2 as u64))
            }
            None => None,
        };
        s.at.push(at);
    }
    if fresh {
        (s.check, s.expect) = Default::default();
    }
    Ok(s)
}

/// What [`read_checked`] read for one hash: its row, its entry's address,
/// and the bytes `pick` asked for there.
type Checked = Option<(&'static Row, u64, Option<Vec<u8>>)>;

/// For each of `hashes`, the range `pick` names off its row and entry,
/// read in one pass with the pointers it was found through, and `extra`
/// beside them. Should a pointer have moved, everything is found again
/// and read once more: a stale address never answers.
fn read_checked(
    mem: &impl Memory,
    global: u64,
    hashes: &[u32],
    pick: impl Fn(&'static Row, u64) -> Option<(u64, usize)>,
    extra: &[(u64, usize)],
) -> Result<(Vec<Checked>, Vec<Vec<u8>>)> {
    let local = Places::default();
    let places = mem.places().unwrap_or(&local);
    for _ in 0..2 {
        let s = spots(mem, places, global, hashes)?;
        let picks: Vec<_> = (s.at.iter())
            .map(|at| at.and_then(|(row, entry)| pick(row, entry)))
            .collect();
        let mut ranges = s.check.clone();
        ranges.extend(picks.iter().flatten());
        ranges.extend(extra);
        let mut got = mem.read_many(&ranges)?.into_iter();
        let held = (got.by_ref().take(s.check.len())).map(|b| (b.len() == 8).then(|| quad(&b, 0)));
        if !held.eq(s.expect.iter().map(|&e| Some(e))) {
            places.forget();
            continue;
        }
        let out = (s.at.iter().zip(&picks))
            .map(|(at, pick)| at.map(|(row, entry)| (row, entry, pick.and_then(|_| got.next()))))
            .collect();
        return Ok((out, got.collect()));
    }
    bail!("GameData moved while it was being read")
}

/// The bytes of `hash`'s current value, held inline, and `extra` read in
/// the same pass. See [`read_checked`].
pub fn value_with(
    mem: &impl Memory,
    global: u64,
    hash: u32,
    extra: &[(u64, usize)],
) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
    let inline = |row: &Row, entry| match row.3 {
        Inline(off, len) => Some((entry + off as u64, len)),
        _ => None,
    };
    let (mut got, extra) = read_checked(mem, global, &[hash], inline, extra)?;
    match got.remove(0) {
        Some((_, _, Some(bytes))) => Ok((bytes, extra)),
        _ => bail!("{hash:#010x} isn't a value the game holds inline"),
    }
}

/// The bytes of `hash`'s current value, held inline.
pub fn value(mem: &impl Memory, global: u64, hash: u32) -> Result<Vec<u8>> {
    Ok(value_with(mem, global, hash, &[])?.0)
}

fn find(mem: &impl Memory, global: u64, hash: u32) -> Result<Option<(&'static Row, u64)>> {
    let manager = manager(mem, global)?;
    let Some((row, i)) = place(mem, manager, hash)? else {
        return Ok(None);
    };
    let entries = mem.u64(entries_pointer(manager, row))?;
    Ok(Some((row, entries + u64::from(i) * row.2 as u64)))
}

/// `hash`'s place, from [`Memory::places`] once it's been found.
fn place(mem: &impl Memory, manager: u64, hash: u32) -> Result<Place> {
    Ok(places_of(mem, manager, &[hash])?[0])
}

/// Each of `hashes`' places: from [`Memory::places`] where found before,
/// the rest found together in one [`scan`].
fn places_of(mem: &impl Memory, manager: u64, hashes: &[u32]) -> Result<Vec<Place>> {
    let kept = mem.places();
    let mut out: Vec<_> = (hashes.iter())
        .map(|&h| kept.and_then(|k| k.get(manager, h)))
        .collect();
    let missing: Vec<u32> = (hashes.iter().zip(&out))
        .filter(|(_, o)| o.is_none())
        .map(|(&h, _)| h)
        .collect();
    if !missing.is_empty() {
        let found = scan(mem, manager, &missing)?;
        for (&hash, place) in hashes.iter().zip(&mut out) {
            if place.is_none() {
                let at = found.get(&hash).copied().flatten();
                if let Some(kept) = kept {
                    kept.put(manager, hash, at);
                }
                *place = Some(at);
            }
        }
    }
    Ok(out.into_iter().map(Option::unwrap).collect())
}

/// `hashes`' places, read from the tables' maps: every table's header in
/// one read, then every map in another, however many hashes there are.
/// A hash goes by the first table whose map lists it.
fn scan(mem: &impl Memory, manager: u64, hashes: &[u32]) -> Result<HashMap<u32, Place>> {
    let heads: Vec<_> = TYPES
        .iter()
        .map(|row| (table(manager, row), MAP_CAP + 4))
        .collect();
    let heads = mem.read_many(&heads)?;
    let mut maps = Vec::new();
    for (row, head) in TYPES.iter().zip(&heads) {
        let cap = word(head, MAP_CAP) as usize;
        if cap > 0 {
            maps.push((
                row,
                word(head, COUNT),
                (quad(head, MAP), sized(row.1, cap * 8)?),
            ));
        }
    }
    let ranges: Vec<_> = maps.iter().map(|m| m.2).collect();
    let wanted: HashSet<u32> = hashes.iter().copied().collect();
    let mut found = HashMap::new();
    for ((row, count, _), map) in maps.iter().zip(mem.read_many(&ranges)?) {
        let mut seen = HashSet::new();
        for p in map.as_chunks::<8>().0 {
            let (hash, i) = (word(&p[..], 0), word(&p[..], 4));
            if wanted.contains(&hash)
                && !found.contains_key(&hash)
                && seen.insert(hash)
                && i < *count
            {
                found.insert(hash, Some((*row, i)));
            }
        }
    }
    Ok(found)
}

/// Whether each Bool in `hashes` is true, in one read once their places
/// are known: one request over a bridge. A hash the game doesn't hold as a
/// Bool reads false.
pub fn bools(mem: &impl Memory, global: u64, hashes: &[u32]) -> Result<Vec<bool>> {
    let flag = |row: &'static Row, entry| match row.3 {
        Inline(at, _) if row.1 == "Bool" => Some((entry + at as u64, 1)),
        _ => None,
    };
    let (got, _) = read_checked(mem, global, hashes, flag, &[])?;
    Ok((got.into_iter())
        .map(|g| {
            g.and_then(|(_, _, b)| b)
                .is_some_and(|b| b.first().is_some_and(|&b| b != 0))
        })
        .collect())
}

/// Where a value held inline keeps its current value: its type, address
/// and width. A scalar's saved copy sits right after. See
/// [`value::encode`] for what's writable.
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
    let whole = |row: &Row, entry| Some((entry, row.2));
    let (mut got, _) = read_checked(mem, global, &[hash], whole, &[])?;
    let Some(((_, kind, _, shape), _, Some(entry))) = got.remove(0) else {
        bail!("{hash:#010x} isn't a value the game holds");
    };
    let items = items(mem, kind, &entry, *shape)?;
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
/// is the one the game saves from. Text has no copy: the next slot
/// follows it. Data pages only.
///
/// Its address is checked first (see [`read_checked`]): a write never
/// goes through a stale one.
pub fn set(mem: &impl Memory, global: u64, hash: u32, value: &str) -> Result<()> {
    let (mut got, _) = read_checked(mem, global, &[hash], |_, _| None, &[])?;
    let Some(((_, kind, _, Inline(off, width)), entry, _)) = got.remove(0) else {
        bail!("{hash:#010x} isn't a value the game holds inline")
    };
    let bytes = value::encode(kind, value, *width)?;
    let copies = if value::input(kind) == value::Input::Text {
        1
    } else {
        2
    };
    mem.write(entry + *off as u64, &bytes.repeat(copies))
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

        fn write(&self, at: u64, data: &[u8]) -> Result<()> {
            self.0.write(at, data)
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
        // The flags at once with the pointers to them (the global's word,
        // the manager, the Bool and Int tables' entries): no map again,
        // even for the hash the game doesn't hold.
        assert_eq!(mem.reads(), [Many(4 + 3)]);
    }

    #[test]
    fn a_cold_pass_reads_each_map_once_however_many_flags() {
        let on = |b: u8| entry(0, &[(0x16, &[b])]);
        let flags: Vec<_> = (0..40).map(|i| on((i % 2) as u8)).collect();
        let fresh = || kept(game(&[(0, flags.clone())], &[]));
        let reads = |n: u32| {
            let mem = fresh();
            let hashes: Vec<_> = (0..n).map(|i| hash(0, i)).collect();
            let got = bools(&mem, 0x1000, &hashes).unwrap();
            assert_eq!(got, (0..n).map(|i| i % 2 == 1).collect::<Vec<_>>());
            mem.reads().len()
        };
        assert_eq!(reads(1), reads(40));
    }

    /// Where [`movable`] moves its manager and entries to.
    const MANAGER: u64 = 0x40000;
    const ENTRIES: u64 = 0x50000;

    /// [`ints`]'s game, with room to move its manager and Int entries.
    fn movable() -> Kept {
        let int = |v: u32| entry(2, &[(0x1c, &v.to_le_bytes())]);
        let room = [
            (MANAGER, vec![0; super::super::KEYS + 0x10]),
            (ENTRIES, vec![0; 0x100]),
        ];
        kept(game(&[(2, vec![int(5), int(7)])], &room))
    }

    /// Moves the manager and its Int entries, as a reload might. Says
    /// where the old entries were.
    fn move_manager(mem: &Kept) -> u64 {
        let (fake, row) = (&mem.0, TYPES.iter().find(|t| t.0 == 2).unwrap());
        let pointer = entries_pointer(0, row) as usize;
        let mut manager = fake.read(0x3000, super::super::KEYS + 0x10).unwrap();
        let old = quad(&manager, pointer);
        fake.write(ENTRIES, &fake.read(old, 2 * row.2).unwrap())
            .unwrap();
        manager[pointer..pointer + 8].copy_from_slice(&ENTRIES.to_le_bytes());
        fake.write(MANAGER, &manager).unwrap();
        fake.write(0x2000, &MANAGER.to_le_bytes()).unwrap();
        old
    }

    #[test]
    fn a_warm_value_is_one_read() {
        let mem = movable();
        assert_eq!(value(&mem, 0x1000, hash(2, 1)).unwrap(), 7u32.to_le_bytes());
        mem.reads();
        assert_eq!(value(&mem, 0x1000, hash(2, 1)).unwrap(), 7u32.to_le_bytes());
        // The global's word, the manager, the Int entries, the value.
        assert_eq!(mem.reads(), [Many(4)]);
    }

    #[test]
    fn a_moved_manager_is_found_again() {
        let mem = movable();
        value(&mem, 0x1000, hash(2, 1)).unwrap();
        let old = move_manager(&mem);
        let stride = TYPES.iter().find(|t| t.0 == 2).unwrap().2 as u64;
        mem.0
            .write(old + stride + 0x1c, &99u32.to_le_bytes())
            .unwrap();
        assert_eq!(value(&mem, 0x1000, hash(2, 1)).unwrap(), 7u32.to_le_bytes());
        assert!(bools(&mem, 0x1000, &[hash(2, 1)]).is_ok());
    }

    #[test]
    fn a_write_after_a_move_lands_at_the_new_address() {
        let mem = movable();
        value(&mem, 0x1000, hash(2, 1)).unwrap();
        let old = move_manager(&mem);
        set(&mem, 0x1000, hash(2, 1), "42").unwrap();
        let stride = TYPES.iter().find(|t| t.0 == 2).unwrap().2 as u64;
        let at = |base: u64| mem.0.u32(base + stride + 0x1c).unwrap();
        assert_eq!((at(old), at(ENTRIES)), (7, 42));
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
    fn a_wide_string_write_leaves_the_next_slot_alone() {
        needs_names!();
        let next = [0xee; 0xa8 - 0x74];
        let mem = game(&[(26, vec![entry(26, &[(0x74, &next)])])], &[]);
        set(&mem, 0x1000, hash(26, 0), "Hi").unwrap();
        let (_, at, len) = locate(&mem, 0x1000, hash(26, 0)).unwrap();
        let mut want = vec![b'H', 0, b'i', 0];
        want.resize(len, 0);
        assert_eq!(mem.read(at, len).unwrap(), want);
        assert_eq!(mem.read(at + len as u64, next.len()).unwrap(), next);
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
