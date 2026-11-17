//! The sweep's plan for one round, without the game: how each spot would
//! be swept, and the ones it skips with why.

use std::collections::BTreeMap;

use anyhow::Result;
use ounce_formats::Romfs;

use crate::dev;
use crate::sweep::night::{self, holder_kind};
use crate::sweep::plan::Trigger;

/// Tally a round's spots by holder kind and how they'd be swept, and list
/// each skipped spot with its reason.
pub fn plan(romfs: &Romfs, mix: Option<&str>, seed: u64) -> Result<()> {
    let (name, settings) = night::mix(mix.map(night::pinned).transpose()?.unwrap_or(0));
    let dir = std::env::temp_dir().join(format!("zootr-plan-{}", std::process::id()));
    let planned = night::pool(romfs, &dir, seed, &settings, &dev::places(romfs)?);
    let _ = std::fs::remove_dir_all(&dir);
    let planned = planned?;
    let mut tally: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut skipped = Vec::new();
    for p in &planned {
        if let Trigger::Skip(why) = &p.trigger {
            skipped.push(format!("{}  {}: {why}", night::name(p), p.spot.holder));
        }
        let how = night::kind(&p.trigger);
        *tally.entry((holder_kind(&p.spot.holder), how)).or_default() += 1;
    }
    println!("mix {name}, seed {seed}: {} spots", planned.len());
    for ((holder, how), n) in tally {
        println!("{n:5}  {how:5}  {holder}");
    }
    println!("\nskipped:");
    skipped.iter().for_each(|s| println!("  {s}"));
    Ok(())
}
