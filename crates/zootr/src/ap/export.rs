//! The world graph, pool and options as the `znx` world loads them, with
//! abilities inlined, eras resolved per place and start flags folded in.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;
use serde_json::{Value, json};

use super::rule::{Rules, any};
use super::seed::{Seed, base, open, variants};
use super::{OPTIONS, ids::Ids, names};
use crate::checks::ItemCheck;
use crate::fill;
use crate::narrowing::allowed;
use crate::rules::{Need, vocab};
use crate::settings::{Kind, NewGame, schema};
use crate::world::Era;

/// Items that aren't progression but are worth more than filler.
const USEFUL: &[&str] = &[
    "Ice Arrows",
    "Hylian Shield",
    "Piece of Heart",
    "Progressive Deku Stick Bag",
    "Progressive Deku Nut Bag",
];

/// The event that beats the seed.
pub const VICTORY: &str = "victory";

/// Every file of the export, by name, as JSON text. `ids` is the last
/// export's `ids.json`, if any.
pub fn export(romfs: &Romfs, ids: Option<&str>) -> Result<BTreeMap<&'static str, String>> {
    let base = base();
    let seed = Seed::build(romfs, &base, &open())?;
    let mut ids: Ids = ids
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    let names = Namer::new(romfs, &seed)?;

    // What each exposed option adds to the start, which may only be flags.
    let mut flag_settings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, value) in variants()? {
        let mut s = base.clone();
        s.insert(id.clone(), value.clone());
        let start = fill::start(&NewGame::from_settings(&s, &[])?);
        ensure!(
            start.items == seed.start.items,
            "{id} changes what Link starts with"
        );
        for f in start.flags.difference(&seed.start.flags) {
            if let Value::Bool(true) = value {
                flag_settings.entry(f.clone()).or_default().push(id.clone());
            } else {
                bail!("{id} = {value} sets a start flag, which only a toggle may");
            }
        }
    }

    let w = &seed.world;
    // What gives each item outside the pool: a source, or a check the deal
    // leaves holding its vanilla item, each an event.
    let mut givers: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for s in &w.sources {
        givers.entry(&s.item).or_default().push(s.name.clone());
    }
    for (hash, item) in &seed.dealt.kept {
        givers.entry(item).or_default().push(vanilla(*hash));
    }
    let mut rules = Rules {
        seed: &seed,
        names: &names,
        flag_settings: &flag_settings,
        givers: &givers,
        used: BTreeSet::new(),
    };
    // What Link reaches holding everything. The rest never happens.
    let reach = seed.everything();
    let era = |i: usize| w.places[i].1;
    // Where something is: one spot per place, any of its needs there.
    let at = |rules: &mut Rules, spots: Vec<(usize, &Need)>| {
        let mut by_place: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
        for (place, need) in spots.into_iter().filter(|(p, _)| reach.places.contains(p)) {
            by_place
                .entry(place)
                .or_default()
                .push(rules.rule(need, era(place)));
        }
        let spots = by_place
            .into_iter()
            .map(|(place, needs)| (place, any(needs)));
        let spots = spots.filter(|(_, rule)| *rule != Value::Bool(false));
        let spots = spots.map(|(place, rule)| json!({ "place": names.place(place), "rule": rule }));
        spots.collect::<Vec<_>>()
    };

    let mut places: Vec<String> = reach.places.iter().map(|i| names.place(*i)).collect();
    places.extend([Namer::hub(Era::Child), Namer::hub(Era::Adult)]);
    ensure!(
        places.iter().collect::<BTreeSet<_>>().len() == places.len(),
        "two places share a name"
    );

    // One exit per pair of places, any of the edges between them.
    let mut pairs: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    let edges = w.edges.iter().filter(|e| {
        reach.places.contains(&e.to) && e.from.is_none_or(|f| reach.places.contains(&f))
    });
    for e in edges {
        let (from, era) = match e.from {
            Some(f) => (names.place(f), era(f)),
            None => (Namer::hub(era(e.to)), era(e.to)),
        };
        let rule = rules.rule(&e.need, era);
        pairs
            .entry((from, names.place(e.to)))
            .or_default()
            .push(rule);
    }
    for &i in &reach.places {
        let pair = (names.place(i), Namer::hub(era(i)));
        pairs.entry(pair).or_default().push(true.into());
    }
    let mut exits = Vec::new();
    for ((from, to), rules) in pairs {
        let rule = any(rules);
        if rule != Value::Bool(false) {
            exits.push(json!({ "from": from, "to": to, "rule": rule }));
        }
    }

    let mut by_hash: BTreeMap<u64, Vec<(usize, &Need)>> = BTreeMap::new();
    for l in &w.locations {
        by_hash.entry(l.hash).or_default().push((l.place, &l.need));
    }
    let mut checks = Vec::new();
    let mut events = Vec::new();
    let pooled: BTreeSet<&str> = seed.dealt.pool.iter().map(String::as_str).collect();
    for (hash, spots) in by_hash {
        let at = at(&mut rules, spots);
        if seed.dealt.kept.contains_key(&hash) {
            if reach.checks.contains(&hash) {
                events.push(json!({ "name": vanilla(hash), "at": at }));
            }
            continue;
        }
        let check = (seed.dealt.checks.iter().find(|c| c.hash == hash))
            .with_context(|| format!("no check {hash}"))?;
        let mut forbid = BTreeSet::new();
        for actor in pooled
            .iter()
            .filter(|a| !allowed(check, a, seed.dealt.new_game.progressive))
        {
            forbid.insert(names.item(actor)?);
        }
        checks.push(json!({
            "id": ids.location(hash),
            "hash": hash.to_string(),
            "name": names.location(hash)?,
            "at": at,
            "unprovable": !reach.checks.contains(&hash),
            "forbid": forbid,
        }));
    }

    for (_, s) in (w.sources.iter().enumerate()).filter(|(i, _)| reach.sources.contains(i)) {
        events.push(json!({ "name": s.name, "at": at(&mut rules, vec![(s.place, &s.need)]) }));
    }
    let goal = at(
        &mut rules,
        w.goal().iter().map(|(p, need)| (*p, need)).collect(),
    );
    events.push(json!({ "name": VICTORY, "at": goal }));

    let mut counts: BTreeMap<&str, (u32, &str)> = BTreeMap::new();
    for actor in seed.dealt.pool.iter().map(|a| names::same(a)) {
        let name = names.item(actor)?;
        let row = counts.entry(name).or_insert((0, actor));
        ensure!(row.1 == actor, "{name} names two items");
        row.0 += 1;
    }
    let mut items = Vec::new();
    let mut pool = Vec::new();
    for (name, (count, actor)) in &counts {
        let class = match () {
            _ if rules.used.contains(*name) => "progression",
            _ if USEFUL.contains(name) => "useful",
            _ => "filler",
        };
        items.push(json!({ "id": ids.item(actor), "name": name, "class": class }));
        pool.push(json!({ "item": name, "count": count }));
    }
    let event_names: BTreeSet<&str> = w.sources.iter().map(|s| s.name.as_str()).collect();
    ensure!(
        event_names.len() == w.sources.len(),
        "two sources share a name"
    );
    // A source that gives a pool item shares its name, so AP counts both.
    for s in &w.sources {
        if let Some((_, actor)) = counts.get(s.name.as_str()) {
            ensure!(
                *actor == s.item,
                "{} names both an item and another's event",
                s.name
            );
        }
    }

    let world = json!({
        "start": names.place(w.start()),
        "places": places,
        "exits": exits,
        "checks": checks,
        "events": events,
        "goal": VICTORY,
    });
    let golden = super::golden::golden(romfs, &seed, &names, &ids)?;
    let mut out = BTreeMap::new();
    out.insert("world.json", pretty(&world)?);
    out.insert("items.json", pretty(&Value::Array(items))?);
    out.insert("pool.json", pretty(&Value::Array(pool))?);
    out.insert("options.json", pretty(&options())?);
    out.insert("golden.json", pretty(&golden)?);
    out.insert("ids.json", pretty(&serde_json::to_value(&ids)?)?);
    Ok(out)
}

/// The event a check the deal leaves vanilla stands for.
fn vanilla(hash: u64) -> String {
    format!("vanilla-{hash}")
}

/// Each shuffled check once.
fn unique_free(seed: &Seed) -> Vec<ItemCheck> {
    let mut seen = BTreeSet::new();
    let free = seed
        .dealt
        .checks
        .iter()
        .filter(|c| seed.dealt.free.contains(&c.hash));
    free.filter(|c| seen.insert(c.hash)).cloned().collect()
}

fn pretty(value: &Value) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)? + "\n")
}

/// Each exposed option as AP builds it.
fn options() -> Value {
    let defs = schema();
    let rows = OPTIONS
        .iter()
        .filter_map(|id| defs.iter().find(|d| d.id == *id));
    let rows = rows.map(|d| match &d.kind {
        Kind::Toggle { default } => json!({
            "id": d.id, "label": d.label, "description": d.description,
            "kind": "toggle", "default": default,
        }),
        Kind::Choice { default, options } => json!({
            "id": d.id, "label": d.label, "description": d.description,
            "kind": "choice", "default": default,
            "options": options.iter().map(|o| json!({ "id": o.id, "label": o.label })).collect::<Vec<_>>(),
        }),
    });
    Value::Array(rows.collect())
}

/// Names for the export: places, items and locations, all ours.
pub(super) struct Namer {
    places: Vec<String>,
    progressive: bool,
    locations: BTreeMap<u64, String>,
}

impl Namer {
    fn new(romfs: &Romfs, seed: &Seed) -> Result<Self> {
        let places = (seed.world.places.iter())
            .map(|(node, era)| match vocab::place_word(node) {
                Some(w) if w.ends_with("-child") || w.ends_with("-adult") => w.to_owned(),
                Some(w) => format!("{w}-{era}"),
                None => format!("place-{:08x}-{era}", crate::known::hash(node) >> 32),
            })
            .collect();
        Ok(Self {
            places,
            progressive: seed.dealt.new_game.progressive,
            locations: names::locations(romfs, &unique_free(seed))?,
        })
    }

    pub fn place(&self, i: usize) -> String {
        self.places[i].clone()
    }

    /// Where a warp song leaves from: anywhere Link has been at that age.
    fn hub(era: Era) -> String {
        format!("anywhere-{era}")
    }

    pub fn item(&self, actor: &str) -> Result<&'static str> {
        names::item(actor, self.progressive)
    }

    pub fn location(&self, hash: u64) -> Result<&str> {
        let name = self.locations.get(&hash).map(String::as_str);
        name.with_context(|| format!("no name for check {hash}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_data_matches_a_fresh_export() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = super::super::data_dir();
        let ids = std::fs::read_to_string(dir.join("ids.json")).unwrap();
        for (name, text) in export(&romfs, Some(&ids)).unwrap() {
            let committed = std::fs::read_to_string(dir.join(name)).unwrap_or_default();
            assert!(
                committed == text,
                "{name} is stale: run `zootr ap export` and commit it"
            );
        }
    }

    #[test]
    fn the_export_spells_no_internal_id() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (name, text) in export(&romfs, None).unwrap() {
            let id = text
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .find(|w| {
                    w.split_once('_').is_some_and(|(a, b)| {
                        a.starts_with(|c: char| c.is_ascii_uppercase())
                            && b.starts_with(|c: char| c.is_ascii_uppercase())
                    })
                });
            assert_eq!(id, None, "{name}");
        }
    }
}
