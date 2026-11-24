//! `zootr probe actors`: the placements in the level files that match a
//! filter, one line each or counted per actor name.

use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use ounce_formats::{Byml, Romfs};

use super::Filter;
use crate::checks::xyz;
use crate::checks::{GYAML, actors, banc_files, flat, hash_of, load_banc, str_field, vec3};
use crate::items::show;

pub(super) struct Hit {
    /// Without `Banc/` and the extension.
    pub(super) file: String,
    pub(super) gyaml: String,
    pub(super) hash: u64,
    pub(super) pos: [f32; 3],
    pub(super) actor: Byml,
}

pub(super) fn hits(romfs: &Romfs, f: &Filter) -> Result<Vec<Hit>> {
    let has = |want: &Option<String>, s: &str| want.as_ref().is_none_or(|w| s.contains(w.as_str()));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for rel in banc_files(romfs)? {
        if !has(&f.file, &rel) {
            continue;
        }
        for a in actors(&load_banc(romfs, &rel)?.root)? {
            let gyaml = str_field(a, GYAML).unwrap_or_default();
            let hash = hash_of(a).unwrap_or_default();
            let pos = vec3(a, "Translate").unwrap_or_default();
            let near = f.near.is_none_or(|p| flat(p, pos) <= f.within);
            if has(&f.gyaml, gyaml)
                && f.hash.is_none_or(|h| h == hash)
                && near
                && (!f.dedupe || seen.insert(hash))
            {
                out.push(Hit {
                    file: short(&rel).to_owned(),
                    gyaml: gyaml.to_owned(),
                    hash,
                    pos,
                    actor: a.clone(),
                });
            }
        }
    }
    Ok(out)
}

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

fn short(rel: &str) -> &str {
    let rel = rel.strip_prefix("Banc/").unwrap_or(rel);
    rel.strip_suffix(".bcett.byml.zs").unwrap_or(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    #[test]
    fn hundred_gold_skulltulas_once_deduped() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gold = |dedupe| Filter {
            gyaml: Some(actor::ENEMY_SKULLTULA_GOLD.to_string()),
            dedupe,
            ..Filter::default()
        };
        let exact = |hits: Vec<Hit>| {
            hits.iter()
                .filter(|h| h.gyaml == actor::ENEMY_SKULLTULA_GOLD)
                .count()
        };
        assert_eq!(exact(hits(&romfs, &gold(false)).unwrap()), 114);
        assert_eq!(exact(hits(&romfs, &gold(true)).unwrap()), 100);
    }
}
