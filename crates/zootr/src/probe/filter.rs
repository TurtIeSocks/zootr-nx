//! The placement filter the probes share: `probe actors` lists what it
//! matches, `probe tools` and `probe hurts` read it.

use std::collections::HashSet;

use anyhow::Result;
use ounce_formats::{Byml, Romfs};

use super::point;
use crate::checks::{GYAML, actors, banc_files, flat, hash_of, load_banc, str_field, vec3};

#[derive(clap::Args, Default)]
pub struct Filter {
    /// Actor names containing this.
    #[arg(long)]
    pub(super) gyaml: Option<String>,
    /// Banc paths containing this, like an area or `_Child_`.
    #[arg(long)]
    pub(super) file: Option<String>,
    /// One placement hash.
    #[arg(long)]
    pub(super) hash: Option<u64>,
    /// Only actors within `--within` of X,Y,Z, measured flat.
    #[arg(long, value_parser = point, allow_hyphen_values = true)]
    pub(super) near: Option<[f32; 3]>,
    #[arg(long, default_value_t = 10.0)]
    pub(super) within: f32,
    /// Each placement once, though child and adult files repeat it.
    #[arg(long)]
    pub(super) dedupe: bool,
    /// Every field, not only the position.
    #[arg(long)]
    pub(super) fields: bool,
    /// Placements and unique hashes per actor name, instead of each one.
    #[arg(long)]
    pub(super) count: bool,
}

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
