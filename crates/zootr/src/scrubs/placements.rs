//! What a seed writes for scrubs outside the flow: the placement fields that
//! point each spot at its entry, and the seller lines the sales show.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;

use super::spots::{Branch, Scrub, edits, plain};
use super::{ACTOR, ATTACKED, ENTRY, HEART, ITEM, LIKE, NEUTRAL, PRICED, TALK, TEXT};
use crate::checks::Drop;
use crate::rsdb::Events;
use crate::text::Lines;

/// The entry point `s` runs when talked to, while it sells `item` plain.
pub(super) fn sells(s: &Scrub, item: &str) -> String {
    format!("{ACTOR}_Sells_{}_{item}", s.hash())
}

/// The event `s` runs when talked to while it sells `item`, which runs the
/// entry of the same name. `None` keeps its vanilla talk.
fn talk(s: &Scrub, item: &str) -> Option<String> {
    match (plain(item), s.branch) {
        (_, Branch::Secret) => None,
        (true, _) => Some(sells(s, item)),
        (_, Branch::Shop) => Some(ENTRY.clone()),
        _ => None,
    }
}

/// The label of the line selling `item`.
fn line(item: &str) -> String {
    format!("{ACTOR}_Check_{item}")
}

/// The `SellerMessageId` of a sale of `item`.
pub(super) fn seller(item: &str) -> String {
    format!("{}:{}", *TEXT, line(item))
}

/// The seller lines the edited scrubs need, one per item.
pub fn lines(drops: &HashMap<u64, Drop>) -> Lines<'_> {
    let mut new: Vec<(String, &str)> = Vec::new();
    for (_, item) in edits(drops) {
        if new.iter().all(|(_, i)| *i != item) {
            new.push((line(item), item));
        }
    }
    Lines {
        file: &TEXT,
        like: (PRICED, HEART),
        fallback: NEUTRAL,
        new,
    }
}

/// Placement fields, as `HASH.PATH=VALUE`: each edited scrub's item, and
/// the entry a repeat spot or a plain sale runs.
///
/// A plain sale leaves the placement's own item alone, since the talk reads
/// it before anything else and the game doesn't know these items as stock.
pub fn fields(drops: &HashMap<u64, Drop>) -> Vec<String> {
    let mut out = Vec::new();
    for (s, item) in edits(drops).filter(|(s, _)| s.branch != Branch::Secret) {
        let entry = talk(s, item);
        let stock = if plain(item) { s.item } else { item };
        for h in s.placements {
            out.push(format!("{h}.Dynamic.{ITEM}={stock}"));
            if let Some(entry) = &entry {
                out.push(format!("{h}.Dynamic.{TALK}={entry}"));
                if s.attacked {
                    out.push(format!("{h}.Dynamic.{ATTACKED}={entry}"));
                }
            }
        }
    }
    out
}

/// Event rows for every talk the edited scrubs run, without which the game
/// starts nothing when one is talked to.
pub fn events(drops: &HashMap<u64, Drop>, events: &mut Events) -> Result<()> {
    let talks: BTreeSet<String> = edits(drops).filter_map(|(s, i)| talk(s, i)).collect();
    talks.iter().try_for_each(|t| events.copy(t, LIKE))
}
