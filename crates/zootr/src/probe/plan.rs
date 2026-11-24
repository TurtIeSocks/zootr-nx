//! The sweep's plan for one round, without the game: how each spot would
//! be swept, and the ones it skips with why.

use std::collections::BTreeMap;

use anyhow::Result;
use ounce_formats::Romfs;

use crate::dev;
use crate::settings::Settings;
use crate::sweep::night::{self, holder_kind};
use crate::sweep::plan::{Planned, Trigger};

/// The sweep's plan for one round of `settings`, rolled into a temp mod
/// folder of this process's own, which is removed after.
pub(super) fn round(romfs: &Romfs, seed: u64, settings: &Settings) -> Result<Vec<Planned>> {
    let dir = std::env::temp_dir().join(format!("zootr-plan-{}", std::process::id()));
    let planned = night::pool(romfs, &dir, seed, settings, &dev::places(romfs)?);
    let _ = std::fs::remove_dir_all(&dir);
    planned
}

/// Tally a round's spots by holder kind and how they'd be swept, and list
/// each skipped spot with its reason. With `only`, the whole plan of each
/// spot whose name holds it instead.
pub fn plan(romfs: &Romfs, mix: Option<&str>, seed: u64, only: Option<&str>) -> Result<()> {
    let (name, settings) = night::mix(mix.map(night::pinned).transpose()?.unwrap_or(0));
    let planned = round(romfs, seed, &settings)?;
    if let Some(only) = only {
        for p in planned.iter().filter(|p| night::name(p).contains(only)) {
            println!(
                "{}  {} (child {:?}, side {:?})",
                night::name(p),
                p.spot.holder,
                p.child,
                p.side
            );
            println!("  {:?}", p.trigger);
        }
        return Ok(());
    }
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
