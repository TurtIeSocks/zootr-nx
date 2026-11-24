//! How far above its origin each pool item's model reaches down on a
//! shelf, in the pose it plays there: its lowest vertex once the pose has
//! scaled, turned and lifted the skeleton root. `zootr probe floors`
//! writes the table ([`table`]) from the dump: rerun it when the pool or
//! the poses change. A model whose vertices it can't read takes its
//! `ActorInfo` floor, less its own pose's lift, plus the shelf pose's. A
//! model lying flat in the token's pose stands up in the Deku Shield's
//! instead.

mod table;

use crate::known::{Name, hash};
use table::FLOORS;

/// `item`'s floor on a shelf, if the table has it.
pub fn floor(item: &str) -> Option<f32> {
    row(item).map(|r| r.1)
}

/// Whether `item`'s model lies flat, so stands up on a shelf.
pub fn stands(item: &str) -> bool {
    row(item).is_some_and(|r| r.2)
}

fn row(item: &str) -> Option<&'static (Name, f32, bool)> {
    let at = FLOORS
        .binary_search_by_key(&hash(item), |(n, ..)| n.0)
        .ok()?;
    Some(&FLOORS[at])
}
