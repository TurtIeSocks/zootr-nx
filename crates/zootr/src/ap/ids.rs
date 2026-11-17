//! AP ids, append-only: a check or item keeps its id for good, and an id
//! is never handed out twice.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Ids {
    /// By check hash.
    locations: BTreeMap<String, u64>,
    /// By our hash of the item's actor, so the file names no actor.
    items: BTreeMap<String, u64>,
}

impl Ids {
    pub fn location(&mut self, hash: u64) -> u64 {
        take(&mut self.locations, hash.to_string())
    }

    pub fn item(&mut self, actor: &str) -> u64 {
        take(&mut self.items, item_key(actor))
    }

    pub fn location_id(&self, hash: u64) -> Option<u64> {
        self.locations.get(&hash.to_string()).copied()
    }
}

fn item_key(actor: &str) -> String {
    format!("{:016x}", crate::known::hash(actor))
}

/// `key`'s id, or the next one after every id the table has given.
fn take(table: &mut BTreeMap<String, u64>, key: String) -> u64 {
    let next = table.values().max().map_or(1, |m| m + 1);
    *table.entry(key).or_insert(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_stays_and_a_new_key_takes_the_next() {
        let mut ids = Ids::default();
        assert_eq!((ids.location(30), ids.location(10)), (1, 2));
        assert_eq!(ids.location(30), 1);
        let mut again: Ids = serde_json::from_str(&serde_json::to_string(&ids).unwrap()).unwrap();
        assert_eq!((again.location(10), again.location(20)), (2, 3));
    }

    #[test]
    fn a_retired_key_keeps_its_id_from_the_next() {
        let mut ids = Ids::default();
        ids.location(1);
        ids.location(2);
        ids.locations.retain(|k, _| k == "2");
        // The table still holds 2's id, so a new key takes 3, never 1.
        assert_eq!(ids.location(5), 3);
    }
}
