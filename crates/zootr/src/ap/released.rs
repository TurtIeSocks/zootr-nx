//! What each id a release shipped means. An id never changes meaning:
//! rooms and the client hold ids, and players' YAMLs (`exclude_locations`,
//! `start_inventory`, plando) hold the names that go with them.

use std::collections::{BTreeMap, BTreeSet};

use super::ids::Ids;
use crate::pickups::variants;

use serde::Deserialize;

/// The last release's names, by id. Refreshed with each release: the
/// `world.json` and `items.json` of its tag, `git show vX.Y.Z:ap/znx/data/...`
/// (RELEASING.md).
const RELEASED: &str = include_str!("testdata/released.json");
const WORLD: &str = include_str!("../../../../ap/znx/data/world.json");
const ITEMS: &str = include_str!("../../../../ap/znx/data/items.json");

#[derive(Deserialize)]
struct Released {
    locations: BTreeMap<u64, String>,
    items: BTreeMap<u64, (String, String)>,
}

#[derive(Deserialize)]
struct Named {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct Item {
    id: u64,
    name: String,
    class: String,
}

/// How much a class asks of the fill: an item may start to matter more in
/// a later release, never less.
fn rank(class: &str) -> usize {
    match class {
        "filler" | "trap" => 0,
        "useful" => 1,
        "progression" => 2,
        _ => panic!("unknown item class {class}"),
    }
}

#[test]
fn a_released_id_keeps_its_name_and_class() {
    let released: Released = serde_json::from_str(RELEASED).unwrap();
    let world: serde_json::Value = serde_json::from_str(WORLD).unwrap();
    let checks: Vec<Named> = serde_json::from_value(world["checks"].clone()).unwrap();
    let locations: BTreeMap<u64, &str> = checks.iter().map(|c| (c.id, c.name.as_str())).collect();
    let items: Vec<Item> = serde_json::from_str(ITEMS).unwrap();
    let items: BTreeMap<u64, &Item> = items.iter().map(|i| (i.id, i)).collect();

    let ids = Ids::load().unwrap();
    let retired: BTreeSet<u64> = variants::EXCLUDED
        .iter()
        .copied()
        .chain(variants::LINKED.iter().map(|&(alternate, _)| alternate))
        .map(|hash| ids.location_id(hash).expect("retired locations keep their reserved IDs"))
        .collect();
    for id in &retired {
        assert!(!locations.contains_key(id), "retired location {id} is active");
    }

    let mut changed = Vec::new();
    for (id, name) in &released.locations {
        if !retired.contains(id) && locations.get(id) != Some(&name.as_str()) {
            changed.push(format!(
                "location {id}: {name} is now {:?}",
                locations.get(id)
            ));
        }
    }
    for (id, (name, class)) in &released.items {
        match items.get(id) {
            Some(now) if now.name == *name && rank(&now.class) >= rank(class) => {}
            now => changed.push(format!(
                "item {id}: {name} ({class}) is now {:?}",
                now.map(|i| (&i.name, &i.class))
            )),
        }
    }
    assert!(
        changed.is_empty(),
        "released ids changed:\n{}",
        changed.join("\n")
    );
}
