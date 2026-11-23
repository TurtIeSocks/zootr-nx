//! GameData's per-placement flags (Bool64bitKey): each placed actor's own
//! key, like a pot's "broken until its area resets". The manager keeps them
//! apart from the typed tables, as a count and then a pointer to entries
//! sorted by key. `docs/research/tracker-flags.md`.

/// Where the count sits in the manager, the entries' pointer 8 bytes on.
pub const KEYS: usize = 0xc58;
/// One entry: the u64 key, then its value at [`KEY_VALUE`].
pub const KEY_STRIDE: usize = 0x10;
pub const KEY_VALUE: usize = 0xc;

/// The index of `key`'s entry in `entries`, the sorted list laid end to
/// end.
pub fn find(entries: &[u8], key: u64) -> Option<usize> {
    let (entries, _) = entries.as_chunks::<KEY_STRIDE>();
    let key_of = |e: &[u8; KEY_STRIDE]| u64::from_le_bytes(e[..8].try_into().unwrap_or_default());
    entries.binary_search_by_key(&key, key_of).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: u64, value: u8) -> [u8; KEY_STRIDE] {
        let mut e = [0; KEY_STRIDE];
        e[..8].copy_from_slice(&key.to_le_bytes());
        e[KEY_VALUE] = value;
        e
    }

    #[test]
    fn finds_a_key_among_sorted_entries_and_only_one_that_is_there() {
        let table: Vec<u8> = [entry(3, 0), entry(70, 1), entry(u64::MAX, 0)]
            .concat();
        assert_eq!(find(&table, 70), Some(1));
        assert_eq!(find(&table, u64::MAX), Some(2));
        assert_eq!(find(&table, 4), None);
        assert_eq!(find(&[], 3), None);
    }
}
