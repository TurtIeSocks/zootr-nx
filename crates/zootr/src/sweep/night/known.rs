//! What earlier nights taught of each spot, and the round's order it sets:
//! the age and side a spot gave from, which spots gave at all, and which
//! keep coming to nothing.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::map;
use crate::sweep::plan::{Planned, Trigger};
use crate::warp::{self, Side};

/// What earlier nights learned of each spot, by placement hash.
#[derive(Debug, Default)]
pub(in crate::sweep) struct Known {
    /// The age a spot was last reached at: an item landed, the dealt one
    /// or not, on a try that named Link's age.
    ages: HashMap<u64, bool>,
    /// Spots that gave the dealt item, or its substitute, at least once. A
    /// round sweeps the others first, so the nights close in on the pool.
    confirmed: HashSet<u64>,
    /// The [`holder_kind`]s a confirmed spot has. One pot that gave
    /// its item shows pots drop theirs, so a round sweeps kinds no spot
    /// has shown yet first: crates before the 38th pot.
    kinds: HashSet<String>,
    /// The side Link last used a spot from when an item landed.
    sides: HashMap<u64, Side>,
    /// The side a photo pass saw each holder from (`--sides`), tried
    /// first where no item landed yet.
    pub(in crate::sweep) seen: HashMap<u64, Side>,
    /// When a spot last came to nothing from each side, as the row it did
    /// in: a chest opened from behind, a keeper talked to from the wrong
    /// side of his counter.
    missed: HashMap<u64, HashMap<Side, usize>>,
    /// How many rows it has learnt from, so a later miss counts as later.
    rows: usize,
    /// How often a spot was swept without giving its item, from any side.
    tries: HashMap<u64, usize>,
}

/// What kind of holder `holder` is, its number off: `Pot_A_03` is a
/// `Pot_A`, `Kokiri Shop 2` a `Kokiri Shop`.
pub(crate) fn holder_kind(holder: &str) -> &str {
    holder.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_' || c == ' ')
}

/// Add what `rows` tell of each spot to `known`. A later row's age wins.
pub(in crate::sweep) fn learn(known: &mut Known, rows: &str) {
    for row in rows
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    {
        known.rows += 1;
        let verdict = row["swept"]["verdict"]["verdict"].as_str();
        let reached = matches!(verdict, Some("pass" | "wrong_item" | "substituted"));
        let Some(hash) = row["spot"]["hash"].as_str().and_then(|h| h.parse().ok()) else {
            continue;
        };
        if verdict.is_some_and(|v| !matches!(v, "pass" | "substituted")) {
            *known.tries.entry(hash).or_default() += 1;
        }
        if let (true, Some(child)) = (reached, row["child"].as_bool()) {
            known.ages.insert(hash, child);
        }
        if matches!(verdict, Some("pass" | "substituted")) {
            known.confirmed.insert(hash);
            let holder = row["spot"]["holder"].as_str().unwrap_or_default();
            known.kinds.insert(holder_kind(holder).to_owned());
        }
        let side = serde_json::from_value::<Side>(row["swept"]["used"]["side"].clone());
        match (side, reached) {
            (Ok(side), true) => _ = known.sides.insert(hash, side),
            // A side a swing broke a neighbour from is as good as none.
            (Ok(side), false) if matches!(verdict, Some("nothing" | "neighbour")) => {
                known
                    .missed
                    .entry(hash)
                    .or_default()
                    .insert(side, known.rows);
            }
            _ => {}
        }
    }
}

/// `p` at the age it was last reached at, when its map names none. Logic
/// lists most dungeon checks for both ages, and the wrong one costs a try.
/// And from the side it gave an item from, else a side it never came to
/// nothing from, its planned one first. Once every side has missed, the one
/// that missed longest ago comes round next, so a fix to `use` gets its turn
/// at each: the Well's chest missed from its front 27 times and its back
/// once, and the back is where it opens. Counting misses instead starved a
/// side that missed often before the fix: the Spirit Trial's hammer chest
/// missed 50 times from its front while its flames were up, so the sweep
/// kept trying the other sides once the hammer put them out, and the front
/// is where it opens.
pub(in crate::sweep) fn at_known(p: &Planned, known: &Known) -> Planned {
    let hash = p.spot.hash;
    let missed = |s: &Side| (known.missed.get(&hash)).map_or(0, |m| m.get(s).copied().unwrap_or(0));
    let first = known.seen.get(&hash).copied().unwrap_or(p.side);
    let least = warp::sides_from(first).min_by_key(missed);
    Planned {
        child: p.child.or_else(|| known.ages.get(&hash).copied()),
        side: (known.sides.get(&hash).copied())
            .or(least)
            .unwrap_or(p.side),
        ..p.clone()
    }
}

/// How many tries a spot comes to nothing in before it goes first only
/// in one round of [`STUBBORN_EVERY`].
const STUBBORN: usize = 20;

/// How often a stubborn spot goes first, in rounds. 21 spots past 50 tries
/// took 21 of every round's 40 (2026-11-17), so the 356 confirmed ones
/// came round for a fresh seed's item half as often.
const STUBBORN_EVERY: u64 = 4;

/// The round's order. Spots no try has confirmed go first, the least
/// tried of them first, so a spot that never gives can't take every round
/// from the rest. One that came to nothing [`STUBBORN`] times goes after
/// the confirmed ones, but for one round in [`STUBBORN_EVERY`], so a fix
/// still reaches it. Then kinds no spot has shown before shown ones, then
/// by map from one `seed` picks, so a round cut short by `--limit` reaches
/// a different part of the game each time. The skips go last.
pub(super) fn order(planned: &mut [Planned], known: &Known, seed: u64) {
    let due = seed.is_multiple_of(STUBBORN_EVERY);
    let maps: Vec<String> = (planned.iter())
        .filter(|p| !matches!(p.trigger, Trigger::Skip(_)))
        .map(map)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let start = seed as usize % maps.len().max(1);
    let rank = |p: &Planned| {
        let i = maps.binary_search(&map(p)).unwrap_or_default();
        (i + maps.len() - start) % maps.len().max(1)
    };
    planned.sort_by_cached_key(|p| {
        let hash = p.spot.hash;
        let skip = matches!(p.trigger, Trigger::Skip(_));
        let tries = known.tries.get(&hash).copied().unwrap_or_default();
        let confirmed = known.confirmed.contains(&hash);
        let resting = !confirmed && !due && tries >= STUBBORN;
        (
            skip,
            resting,
            confirmed,
            tries,
            known.kinds.contains(holder_kind(&p.spot.holder)),
            rank(p),
        )
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::known::{actor, flag};
    use crate::sweep::night::tests::open;

    #[test]
    fn earlier_nights_give_each_spot_its_age_and_whether_it_gave_its_item() {
        let row = |hash: u64, child: Option<bool>, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()}, "child": child,
                "swept": {"verdict": {"verdict": verdict}}})
            .to_string()
        };
        let rows = [
            row(1, Some(false), "pass"),
            row(1, Some(true), "wrong_item"),
            row(2, Some(true), "nothing"),
            row(3, None, "pass"),
            row(4, Some(false), "substituted"),
            json!({"round": 9, "error": "x"}).to_string(),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        assert_eq!(known.ages, HashMap::from([(1, true), (4, false)]));
        assert_eq!(known.confirmed, HashSet::from([1, 3, 4]));
        let spot = |hash| Planned {
            spot: crate::generate::Spot {
                hash,
                ..Default::default()
            },
            ..open(flag::SPIRIT_TEMPLE)
        };
        assert_eq!(at_known(&spot(4), &known).child, Some(false));
        assert_eq!(at_known(&spot(2), &known).child, None, "never reached");
        let named = Planned {
            child: Some(true),
            side: Side::Front,
            ..spot(4)
        };
        assert_eq!(
            at_known(&named, &known).child,
            Some(true),
            "its map's age wins"
        );
    }

    #[test]
    fn a_kind_of_holder_is_shown_once_one_of_its_spots_gave() {
        needs_names!();
        let row = |hash: u64, holder: &str, verdict: &str| {
            json!({"spot": {"hash": hash.to_string(), "holder": holder},
                "swept": {"verdict": {"verdict": verdict}}})
            .to_string()
        };
        let rows = [
            row(1, actor::OBJ_POT_A_03, "pass"),
            row(2, actor::OBJ_BROKEN_BOX_A_01, "nothing"),
            row(3, "Kokiri Shop 2", "substituted"),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        let kinds = HashSet::from([actor::OBJ_POT_A.to_string(), "Kokiri Shop".to_owned()]);
        assert_eq!(known.kinds, kinds);
        assert_eq!(
            holder_kind(actor::OBJ_T_BOX_A_02_NO_BAKE),
            actor::OBJ_T_BOX_A_02_NO_BAKE
        );
    }

    #[test]
    fn a_spot_goes_again_from_the_side_it_gave_from_else_the_one_missed_longest_ago() {
        let row = |hash: u64, side: &str, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()},
                "swept": {"verdict": {"verdict": verdict}, "used": {"side": side}}})
            .to_string()
        };
        let rows = [
            row(1, "front", "nothing"),
            row(1, "left", "pass"),
            row(2, "front", "nothing"),
            row(2, "left", "nothing"),
            row(2, "front", "stall"),
            row(3, "front", "nothing"),
            row(3, "left", "nothing"),
            row(3, "right", "nothing"),
            row(3, "back", "nothing"),
            row(5, "back", "neighbour"),
            row(6, "back", "neighbour"),
            row(6, "front", "nothing"),
        ];
        let worn = [
            "front", "front", "front", "left", "left", "right", "right", "back",
        ];
        let rows = rows.into_iter().chain(worn.map(|s| row(7, s, "nothing")));
        let mut known = Known::default();
        learn(&mut known, &rows.collect::<Vec<_>>().join("\n"));
        let planned = |hash, side| Planned {
            spot: crate::generate::Spot {
                hash,
                ..Default::default()
            },
            side,
            ..open(flag::SPIRIT_TEMPLE)
        };
        let side = |hash| at_known(&planned(hash, Side::Front), &known).side;
        assert_eq!(side(1), Side::Left, "the side it gave from");
        assert_eq!(side(2), Side::Right, "the next side not come to nothing");
        assert_eq!(side(3), Side::Front, "round again once all have");
        assert_eq!(side(4), Side::Front, "never swept");
        assert_eq!(side(7), Side::Front, "every side missed: the longest ago");
        let back = |hash| at_known(&planned(hash, Side::Back), &known).side;
        assert_eq!(back(4), Side::Back, "never swept, from the planned side");
        assert_eq!(back(5), Side::Front, "a neighbour broke from the back");
        assert_eq!(back(6), Side::Left, "and the front came to nothing");
        assert_eq!(back(1), Side::Left, "the side it gave from still wins");
        assert_eq!(back(3), Side::Front, "round again from the longest ago");
        known.seen = [(1, Side::Back), (4, Side::Right)].into();
        let side = |hash| at_known(&planned(hash, Side::Front), &known).side;
        assert_eq!(side(4), Side::Right, "never swept: where the photo saw it");
        assert_eq!(
            side(1),
            Side::Left,
            "the side it gave from beats the photo's"
        );
    }

    #[test]
    fn a_round_starts_at_a_map_its_seed_picks() {
        let skip = Planned {
            trigger: Trigger::Skip("none".into()),
            ..open("")
        };
        let order_of = |seed| {
            let mut planned = [open("A"), open("A"), open("B"), open("C"), skip.clone()];
            order(&mut planned, &Known::default(), seed);
            planned.iter().map(map).collect::<Vec<_>>()
        };
        assert_eq!(order_of(0), ["A", "A", "B", "C", ""]);
        assert_eq!(order_of(1), ["B", "C", "A", "A", ""]);
        assert_eq!(order_of(5), ["C", "A", "A", "B", ""], "the skips stay last");
    }

    #[test]
    fn the_least_tried_spots_go_first_and_confirmed_ones_last() {
        let at = |map: &str, hash: u64| {
            let mut p = open(map);
            p.spot.hash = hash;
            p
        };
        let row = |hash: u64, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()}, "swept": {"verdict": {"verdict": verdict}}})
                .to_string()
        };
        let rows = [
            row(1, "nothing"),
            row(1, "nothing"),
            row(2, "setup_failed"),
            row(3, "pass"),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        let mut planned = [at("A", 1), at("A", 2), at("A", 3), at("B", 4)];
        order(&mut planned, &known, 0);
        let hashes: Vec<u64> = planned.iter().map(|p| p.spot.hash).collect();
        assert_eq!(
            hashes,
            [4, 2, 1, 3],
            "never tried, then tried once, twice, then confirmed"
        );
    }

    #[test]
    fn a_stubborn_spot_goes_first_one_round_in_four_and_last_otherwise() {
        let at = |hash: u64| {
            let mut p = open("A");
            p.spot.hash = hash;
            p
        };
        let row = |hash: u64, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()}, "swept": {"verdict": {"verdict": verdict}}})
                .to_string()
        };
        let rows = std::iter::repeat_n(row(1, "nothing"), STUBBORN)
            .chain(std::iter::repeat_n(row(2, "nothing"), STUBBORN - 1))
            .chain([row(3, "pass")]);
        let mut known = Known::default();
        learn(&mut known, &rows.collect::<Vec<_>>().join("\n"));
        let order_of = |seed| {
            let mut planned = [at(1), at(2), at(3)];
            order(&mut planned, &known, seed);
            planned.map(|p| p.spot.hash)
        };
        assert_eq!(order_of(4), [2, 1, 3], "its round: by tries");
        assert_eq!(order_of(5), [2, 3, 1], "else after the confirmed");
    }
}
