//! `zootr probe actors`: the placements in the level files that match a
//! filter, one line each or counted per actor name.

use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use ounce_formats::Romfs;

use super::filter::{Filter, Hit, hits};
use crate::checks::{GYAML, flat, str_field, xyz};
use crate::items::show;

pub(super) fn print_actors(romfs: &Romfs, f: &Filter) -> Result<()> {
    let hits = hits(romfs, f)?;
    if f.count {
        let mut by: BTreeMap<&str, (usize, HashSet<u64>)> = BTreeMap::new();
        for h in &hits {
            let e = by.entry(&h.gyaml).or_default();
            e.0 += 1;
            e.1.insert(h.hash);
        }
        for (gyaml, (n, unique)) in by {
            println!("{n:>5} {:>5} {gyaml}", unique.len());
        }
    } else {
        print_hits(&hits, f)?;
    }
    eprintln!("{} placements", hits.len());
    Ok(())
}

/// One line per placement, for `print_actors` when it isn't counting.
fn print_hits(hits: &[Hit], f: &Filter) -> Result<()> {
    for h in hits {
        let mut line = format!("{} {} {} {}", h.file, h.gyaml, h.hash, xyz(h.pos));
        if let Some(p) = f.near {
            line += &format!(" d={:.1}", flat(p, h.pos));
        }
        if f.fields {
            let skip = [GYAML, "Hash", "Translate"];
            let map = h.actor.as_map()?;
            let mut rest: Vec<_> = map
                .iter()
                .filter(|(k, _)| !skip.contains(&k.as_str()))
                .collect();
            rest.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in rest {
                line += &format!(" {k}={}", show(v));
            }
        } else if let Some(name) = str_field(&h.actor, "Name") {
            line += &format!(" Name={name}");
        }
        println!("{line}");
    }
    Ok(())
}
