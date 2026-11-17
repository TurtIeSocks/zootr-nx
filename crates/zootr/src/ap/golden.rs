//! Answers the `znx` world's own logic has to match: for each exposed
//! option and seeded handfuls of the pool, which checks Rust logic reaches.

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde_json::{Value, json};

use super::export::Namer;
use super::ids::Ids;
use super::seed::{Seed, base, variants};
use crate::rng::Rng;

/// Shares of the pool each case holds.
const SHARES: [usize; 4] = [0, 1, 2, 3];
const OF: usize = 4;

pub fn golden(romfs: &Romfs, base_seed: &Seed, names: &Namer, ids: &Ids) -> Result<Value> {
    let mut cases = vec![(None, base())];
    for (id, value) in variants()? {
        let mut s = base();
        s.insert(id.clone(), value.clone());
        cases.push((Some((id, value)), s));
    }
    let mut out = Vec::new();
    for (n, (changed, settings)) in cases.into_iter().enumerate() {
        let seed = Seed::build(romfs, &settings, &Default::default())?;
        for share in SHARES {
            let mut pool = base_seed.pool.clone();
            Rng::new((n * OF + share) as u64).shuffle(&mut pool);
            pool.truncate(pool.len() * share / OF);
            let mut have = seed.start.clone();
            pool.iter().for_each(|i| have.add(i));
            let reached = seed.play(&have).reached;
            let mut reachable = Vec::new();
            for hash in reached.into_iter().filter(|h| seed.free.contains(h)) {
                let id = ids.location_id(hash);
                reachable.push(id.with_context(|| format!("check {hash} has no id"))?);
            }
            reachable.sort();
            let items = pool
                .iter()
                .map(|a| names.item(a))
                .collect::<Result<Vec<_>>>()?;
            let settings = changed.iter().map(|(id, v)| (id.clone(), v.clone()));
            out.push(json!({
                "settings": settings.collect::<serde_json::Map<_, _>>(),
                "items": items,
                "reachable": reachable,
            }));
        }
    }
    Ok(Value::Array(out))
}
