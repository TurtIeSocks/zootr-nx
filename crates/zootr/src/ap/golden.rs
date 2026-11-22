//! Answers the `znx` world's own logic has to match: for each exposed
//! option and seeded handfuls of the pool, which checks Rust logic reaches.

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::OPTIONS;
use super::export::Namer;
use super::ids::Ids;
use super::seed::Seed;
use crate::rng::Rng;
use crate::settings::{Kind, Settings, schema};

/// Shares of the pool each case holds.
const SHARES: [usize; 4] = [0, 1, 2, 3];
const OF: usize = 4;

/// Every exposed option's value in `settings`, its default if unset.
fn exposed(settings: &Settings) -> Result<serde_json::Map<String, Value>> {
    let defs = schema();
    let mut out = serde_json::Map::new();
    for id in OPTIONS {
        let def = defs.iter().find(|d| d.id == *id);
        let default = match &def.with_context(|| format!("no setting {id}"))?.kind {
            Kind::Toggle { default } => Value::Bool(*default),
            Kind::Choice { default, .. } => Value::String(default.clone()),
            Kind::Inventory { default } => serde_json::to_value(default)?,
        };
        out.insert((*id).into(), settings.get(*id).cloned().unwrap_or(default));
    }
    Ok(out)
}

/// Each of `seeds` built with its `settings` set.
pub fn golden(settings: &[Settings], seeds: &[Seed], names: &Namer, ids: &Ids) -> Result<Value> {
    let mut out = Vec::new();
    for (n, (settings, seed)) in settings.iter().zip(seeds).enumerate() {
        for share in SHARES {
            let mut pool = seed.dealt.pool.clone();
            Rng::new((n * OF + share) as u64).shuffle(&mut pool);
            pool.truncate(pool.len() * share / OF);
            let mut have = seed.start.clone();
            pool.iter().for_each(|i| have.add(i));
            let reached = seed.play(&have).reached;
            let mut reachable = Vec::new();
            for hash in reached.into_iter().filter(|h| seed.dealt.free.contains(h)) {
                let id = ids.location_id(hash);
                reachable.push(id.with_context(|| format!("check {hash} has no id"))?);
            }
            reachable.sort();
            let items = pool
                .iter()
                .map(|a| names.item(a))
                .collect::<Result<Vec<_>>>()?;
            out.push(json!({
                "settings": exposed(settings)?,
                "items": items,
                "reachable": reachable,
            }));
        }
    }
    Ok(Value::Array(out))
}
