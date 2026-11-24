//! AP ids, append-only: a check or item keeps its id for good, and an id
//! is never handed out twice.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Ids {
    /// By check hash.
    locations: BTreeMap<String, u64>,
    /// By our hash of the item's actor, so the file names no actor.
    items: BTreeMap<String, u64>,
}

/// The AP world's table, as the app ships it.
pub(super) const IDS: &str = include_str!("../../../../ap/znx/data/ids.json");

impl Ids {
    pub fn load() -> anyhow::Result<Self> {
        Ok(serde_json::from_str(IDS)?)
    }

    pub fn location(&mut self, hash: u64) -> u64 {
        take(&mut self.locations, hash.to_string())
    }

    pub fn item(&mut self, actor: &str) -> u64 {
        self.item_as(actor, false)
    }

    /// `actor`'s id, or with `plain` its second: a line's first level goes
    /// by its line's name with progressive items on and its own with them
    /// off (`names::levels`), and a name keeps one id. The Magic Meter's two
    /// are the other way round: a release shipped its plain name under the
    /// first id (`ap::released`).
    pub fn item_as(&mut self, actor: &str, plain: bool) -> u64 {
        take(&mut self.items, item_key(actor, plain))
    }

    pub fn location_id(&self, hash: u64) -> Option<u64> {
        self.locations.get(&hash.to_string()).copied()
    }

    pub fn item_id(&self, actor: &str) -> Option<u64> {
        self.id_as(actor, false)
    }

    fn id_as(&self, actor: &str, plain: bool) -> Option<u64> {
        self.items.get(&item_key(actor, plain)).copied()
    }

    /// Each of `actors` the table has an id for, by that id, both ids of
    /// one with two.
    pub fn actors<'a>(&self, actors: impl IntoIterator<Item = &'a str>) -> HashMap<u64, &'a str> {
        let ids = |a| {
            [false, true]
                .into_iter()
                .filter_map(move |p| self.id_as(a, p))
        };
        actors
            .into_iter()
            .flat_map(|a| ids(a).map(move |id| (id, a)))
            .collect()
    }
}

fn item_key(actor: &str, plain: bool) -> String {
    let key = crate::known::hash(actor);
    match plain {
        false => format!("{key:016x}"),
        true => format!("{key:016x}-plain"),
    }
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
    fn an_item_s_plain_id_is_its_second_and_both_find_it() {
        let mut ids = Ids::default();
        let (id, plain) = (
            ids.item("Item_FakeLine"),
            ids.item_as("Item_FakeLine", true),
        );
        assert_ne!(id, plain);
        let actors = ids.actors(["Item_FakeLine"]);
        assert_eq!(
            (actors[&id], actors[&plain]),
            ("Item_FakeLine", "Item_FakeLine")
        );
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
