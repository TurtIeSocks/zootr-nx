//! The export's pool: each item's count in each seed, the items with
//! their ids and classes, and `pool.json`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use serde_json::{Value, json};

use super::{Ctx, Namer, USEFUL};
use crate::ap::ids::Ids;
use crate::ap::names;
use crate::ap::rule;
use crate::ap::seed::Seed;
use crate::ap::when::{Counts, steps};
use crate::ice_traps;
use crate::settings::{IceTraps, PROGRESSIVE, Settings, choice_id};
use crate::starter_bags;

/// Each pool item's count in each seed, by its AP name: the pool before
/// the trade, then what settings add, which the world trades in for
/// rupees itself (`starter_bags::swap_in`). Which rupee depends on every
/// other option, so no row could say it.
pub(super) struct Counted<'s> {
    /// Keyed by name, whether the trade may take it (a chest's rupee, not
    /// a pot's, though both share a name) and whether a trap may.
    counts: BTreeMap<(&'s str, bool, bool), (Vec<u32>, &'s str)>,
    /// In `starter_bags::added`'s order, the order the world trades them.
    added: Vec<(&'s str, (Vec<u32>, &'s str))>,
}

impl<'s> Counted<'s> {
    /// The counts in `seeds`, built with `settings`.
    pub(super) fn new(seeds: &[&'s Seed], settings: &[Settings]) -> Result<Self> {
        let mut counted = Self {
            counts: BTreeMap::new(),
            added: Vec::new(),
        };
        let junk_actor = |a: &str| starter_bags::JUNK.iter().any(|j| j.get() == a);
        // A seed with progressive items on deals each line's first level, so
        // its pool reads from its twin with them off, which deals each check's
        // own item: the world names a line by it (`names::line`).
        let twin = |n: usize| -> Result<&'s Seed> {
            if !seeds[n].dealt.new_game.progressive {
                return Ok(seeds[n]);
            }
            let mut off = settings[n].clone();
            off.insert(PROGRESSIVE.into(), false.into());
            let at = settings.iter().position(|s| *s == off);
            Ok(seeds[at.with_context(|| format!("no seed {off:?}"))?])
        };
        for n in 0..seeds.len() {
            let seed = twin(n)?;
            for raw in &seed.dealt.before {
                let actor = names::same(raw);
                let name = names::item(actor, false)?;
                let key = (name, junk_actor(raw), seed.trappable.contains(raw));
                let row = (counted.counts.entry(key)).or_insert((vec![0; seeds.len()], actor));
                ensure!(row.1 == actor, "{name} names two items");
                row.0[n] += 1;
            }
            let added = &mut counted.added;
            for actor in seed.dealt.added.iter().map(|a| names::same(a)) {
                let name = names::item(actor, false)?;
                let at = match added.iter().position(|(n, _)| *n == name) {
                    Some(at) => at,
                    None => {
                        added.push((name, (vec![0; seeds.len()], actor)));
                        added.len() - 1
                    }
                };
                added[at].1.0[n] += 1;
            }
        }
        Ok(counted)
    }

    /// The items, each once with its id and class by what the rules
    /// `used`, and the pool's rows.
    pub(super) fn items_and_pool(
        &self,
        cx: &Ctx,
        used: &BTreeSet<String>,
        ids: &mut Ids,
    ) -> Result<(Vec<Value>, Vec<Value>)> {
        let mut items: Vec<Value> = Vec::new();
        let mut pool = Vec::new();
        let rows = (self.counts.iter()).map(|((n, junk, trap), c)| (*n, c, *junk, *trap, false));
        let added_rows = self.added.iter().map(|(n, c)| (*n, c, false, false, true));
        let mut firsts: BTreeMap<&str, &str> = BTreeMap::new();
        let class = |name: &str| match () {
            _ if used.contains(name) => "progression",
            _ if USEFUL.contains(&name) => "useful",
            _ => "filler",
        };
        for (name, (count, actor), is_junk, is_trappable, is_added) in rows.chain(added_rows) {
            if !items.iter().any(|i| i["name"] == name) {
                // A line's first level keeps its first id for the line's name,
                // unless the line goes by it.
                let line = names::line(actor)?;
                let plain = line.is_some_and(|(l, first)| first == *actor && l != name);
                items.push(
                    json!({ "id": ids.item_as(actor, plain), "name": name, "class": class(name) }),
                );
                if let Some((line, first)) = line {
                    firsts.insert(line, first);
                }
            }
            let counts = Counts {
                base: count[0],
                by_variant: &count[1..],
            };
            for (count, rule) in steps(cx.domains, cx.variants, &counts) {
                let mut row = json!({ "item": name, "count": count });
                if rule != Value::Bool(true) {
                    row["when"] = rule;
                }
                if is_added {
                    row["added"] = true.into();
                }
                if is_junk {
                    row["junk"] = true.into();
                }
                if is_trappable {
                    row["trappable"] = true.into();
                }
                pool.push(row);
            }
        }
        // With progressive items on, each line's levels go by its name, under
        // its first level's id.
        // The Magic Meter's goes by its first level's, already there.
        for (line, first) in firsts {
            if !items.iter().any(|i| i["name"] == line) {
                items.push(json!({ "id": ids.item(first), "name": line, "class": class(line) }));
            }
        }
        // The traps the world swaps in for its trappable items.
        let trap = ice_traps::POOLED;
        let name = cx.names.item(trap)?;
        items.push(json!({ "id": ids.item(trap), "name": name, "class": "trap" }));
        Ok((items, pool))
    }

    /// Fails unless a source that gives a pool item gives that item: it
    /// shares the item's name, so AP counts both.
    pub(super) fn check_sources(&self, seeds: &[&Seed]) -> Result<()> {
        for seed in seeds {
            for s in &seed.world.sources {
                let added = (self.added.iter())
                    .find(|(n, _)| *n == s.name)
                    .map(|(_, c)| c);
                let pooled = (self.counts.iter())
                    .find(|((n, ..), _)| *n == s.name)
                    .map(|(_, c)| c);
                if let Some((_, actor)) = pooled.or(added) {
                    ensure!(
                        *actor == s.item,
                        "{} names both an item and another's event",
                        s.name
                    );
                }
            }
        }
        Ok(())
    }
}

/// `pool.json`: the pool's rows, the junk the trade takes, the traps and
/// their shares, and each line's levels.
pub(super) fn pool_json(rows: Vec<Value>, levels: &[rule::Line], names: &Namer) -> Result<Value> {
    // Each line's levels by its name, each its own name then its events:
    // with progressive items on, the world deals each as the line and reads
    // its rules as `rule::lined` does.
    let levels: serde_json::Map<String, Value> = (levels.iter())
        .map(|(line, steps)| (line.clone(), json!(steps)))
        .collect();
    // A share of the trappable items by Ice Traps' value (`IceTraps::of`).
    let shares: serde_json::Map<String, Value> = (IceTraps::value_variants().iter())
        .map(|&v| (choice_id(v), json!(v.share())))
        .collect();
    let junk: Vec<&str> = (starter_bags::JUNK.iter())
        .map(|j| names.item(names::same(j.get())))
        .collect::<Result<_>>()?;
    let trap = names.item(ice_traps::POOLED)?;
    Ok(json!({ "junk": junk, "rows": rows, "trap": trap, "shares": shares, "levels": levels }))
}
