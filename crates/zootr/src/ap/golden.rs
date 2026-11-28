//! Answers the `znx` world's own logic has to match: for each exposed
//! option and seeded handfuls of the pool, which checks Rust logic reaches
//! and whether it beats the seed.
//!
//! Written small (`exported.golden` reads it back): one case a line, its
//! settings as they differ from each option's default, and each row's
//! items as ids, a repeated one as `[id, count]`.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::export::Namer;
use super::ids::Ids;
use super::seed::Seed;
use super::{OPTIONS, SETS};
use crate::rng::Rng;
use crate::settings::{Kind, Settings, schema};

/// Shares of the pool each case holds, from none to all of it: the goal
/// only shows once enough is in hand.
const SHARES: [usize; 5] = [0, 1, 2, 3, 4];
const OF: usize = 4;

/// Every AP option's value in `settings`, its default if unset: a set's
/// as the children it holds on.
fn exposed(settings: &Settings) -> Result<serde_json::Map<String, Value>> {
    let defs = schema();
    let value = |id: &str| -> Result<Value> {
        let def = defs.iter().find(|d| d.id == id);
        let default = match &def.with_context(|| format!("no setting {id}"))?.kind {
            Kind::Toggle { default } => Value::Bool(*default),
            Kind::Choice { default, .. } => Value::String(default.clone()),
            Kind::Inventory { default } => serde_json::to_value(default)?,
        };
        Ok(settings.get(id).cloned().unwrap_or(default))
    };
    let mut out = serde_json::Map::new();
    for id in OPTIONS {
        out.insert((*id).into(), value(id)?);
    }
    for set in SETS {
        let mut on = Vec::new();
        for d in set.members() {
            if value(&d.id)? == Value::Bool(true) {
                on.push(Value::String(d.id));
            }
        }
        out.insert(set.id.into(), Value::Array(on));
    }
    Ok(out)
}

/// Each AP option in `settings` off its default.
fn changed(settings: &Settings) -> Result<serde_json::Map<String, Value>> {
    let defaults = exposed(&Settings::new())?;
    let mut out = exposed(settings)?;
    out.retain(|id, v| defaults.get(id) != Some(v));
    Ok(out)
}

/// `items`, names from the export's items, as their ids: a repeated one as
/// `[id, count]`.
fn counted(items: &[&str], item_ids: &HashMap<&str, u64>) -> Result<Vec<Value>> {
    let mut counts = BTreeMap::new();
    for name in items {
        let id = item_ids
            .get(name)
            .with_context(|| format!("no id for item {name}"))?;
        *counts.entry(*id).or_insert(0u32) += 1;
    }
    let one = |(id, n): (u64, u32)| if n == 1 { json!(id) } else { json!([id, n]) };
    Ok(counts.into_iter().map(one).collect())
}

/// Golden case `n`: `seed`, built with `settings` set, and its rows.
pub fn case(
    n: usize,
    settings: &Settings,
    seed: &Seed,
    names: &Namer,
    ids: &Ids,
    item_ids: &HashMap<&str, u64>,
) -> Result<Value> {
    let mut rows = Vec::new();
    for share in SHARES {
        let mut pool = seed.dealt.pool.clone();
        Rng::new((n * OF + share) as u64).shuffle(&mut pool);
        pool.truncate(pool.len() * share / OF);
        let mut have = seed.start.clone();
        pool.iter().for_each(|i| have.add(i));
        let play = seed.play(&have);
        let goal = seed.world.beaten(&play.have);
        let reached = play.reached;
        let mut reachable = Vec::new();
        for hash in reached.into_iter().filter(|h| seed.dealt.free.contains(h)) {
            let id = ids.location_id(hash);
            reachable.push(id.with_context(|| format!("check {hash} has no id"))?);
        }
        reachable.sort();
        let items = pool
            .iter()
            .map(|a| names.item_in(a, Some(seed.dealt.new_game.progressive)))
            .collect::<Result<Vec<_>>>()?;
        rows.push(json!({
            "items": counted(&items, item_ids)?,
            "reachable": reachable,
            "goal": goal,
        }));
    }
    Ok(json!({ "settings": changed(settings)?, "rows": rows }))
}

/// `cases` as golden.json: a JSON array, one case a line.
pub fn write(cases: &[Value]) -> Result<String> {
    let lines = cases.iter().map(serde_json::to_string);
    let lines = lines.collect::<serde_json::Result<Vec<_>>>()?;
    Ok(format!("[\n{}\n]\n", lines.join(",\n")))
}
