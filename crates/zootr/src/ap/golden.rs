//! Answers the `znx` world's own logic has to match: for each exposed
//! option and seeded handfuls of the pool, which checks Rust logic reaches
//! and whether it beats the seed.

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

/// The golden rows of case `n`: `seed`, built with `settings` set.
pub fn rows(
    n: usize,
    settings: &Settings,
    seed: &Seed,
    names: &Namer,
    ids: &Ids,
) -> Result<Vec<Value>> {
    let mut out = Vec::new();
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
        out.push(json!({
            "settings": exposed(settings)?,
            "items": items,
            "reachable": reachable,
            "goal": goal,
        }));
    }
    Ok(out)
}
