//! The world graph, pool and options as the `znx` world loads them, with
//! abilities inlined, eras resolved per place and start flags folded in.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Romfs, par_map};
use serde_json::{Value, json};

use super::rule::{Rules, all, any};
use super::seed::{Everything, Seed, base, domains, open, values, variants};
use super::when::{Counts, Variant, steps, word};
use super::{OPTIONS, described, ids::Ids, names, offers};
use crate::checks::ItemCheck;
use crate::fill;
use crate::narrowing::allowed;
use crate::rules::{Need, vocab};
use crate::settings::{Kind, NewGame, Settings, schema};
use crate::starter_bags;
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
    let base_seed = Seed::build(romfs, &base, &open())?;
    let variants = variants()?;
    let domains = domains()?;
    // Each seed builds on its own, so they build at once.
    let built: Vec<Seed> = par_map(&variants, |v| Seed::build(romfs, &v.settings(), &open()))
        .into_iter()
        .collect::<Result<_>>()?;
    // The base first, then each variant's seed.
    let seeds: Vec<&Seed> = std::iter::once(&base_seed).chain(&built).collect();
    // The same, each with its settings set rather than left as words.
    let settings: Vec<Settings> = std::iter::once(base.clone())
        .chain(variants.iter().map(Variant::settings))
        .collect();
    let set: Vec<Seed> = par_map(&settings, |s| Seed::build(romfs, s, &Default::default()))
        .into_iter()
        .collect::<Result<_>>()?;
    let mut ids: Ids = ids
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    let names = Namer::new(romfs, &seeds)?;
    let w = &base_seed.world;
    for (seed, v) in built.iter().zip(&variants) {
        ensure!(
            seed.world.places == w.places && edges(&seed.world) == edges(w),
            "{:?} changes the world's places or edges",
            v.overlay
        );
    }

    // What each option adds to the start, as the settings that set it.
    let mut flag_settings: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut item_settings: BTreeMap<String, Vec<(Value, u32)>> = BTreeMap::new();
    for overlay in values()? {
        let mut s = base.clone();
        s.extend(overlay.clone());
        let start = fill::start(&NewGame::from_settings(&s, &[])?);
        let words = overlay
            .iter()
            .map(|(id, v)| json!({ "setting": word(id, v) }));
        let rule = all(words.collect());
        for f in start.flags.difference(&base_seed.start.flags) {
            flag_settings
                .entry(f.clone())
                .or_default()
                .push(rule.clone());
        }
        for (actor, n) in &base_seed.start.items {
            let now = start.items.get(actor).copied().unwrap_or(0);
            ensure!(now >= *n, "{overlay:?} takes {actor} from the start");
        }
        for (actor, n) in &start.items {
            let had = base_seed.start.items.get(actor).copied().unwrap_or(0);
            if *n > had {
                let given = item_settings.entry(actor.clone()).or_default();
                given.push((rule.clone(), n - had));
            }
        }
    }

    // What gives each item outside a pool: a source, or a check a deal
    // leaves holding its vanilla item, each an event, in any variant.
    let mut givers: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut pooled = BTreeSet::new();
    for seed in &seeds {
        for s in &seed.world.sources {
            givers.entry(&s.item).or_default().push(s.name.clone());
        }
        for (hash, item) in &seed.dealt.kept {
            givers.entry(item).or_default().push(vanilla(*hash));
        }
        pooled.extend(seed.dealt.pool.iter().cloned());
    }
    for g in givers.values_mut() {
        g.sort();
        g.dedup();
    }
    let mut rules = Rules {
        start: &base_seed.start,
        pooled: &pooled,
        names: &names,
        flag_settings: &flag_settings,
        item_settings: &item_settings,
        givers: &givers,
        used: BTreeSet::new(),
    };
    let reach = Everything::union(&set);
    let era = |i: usize| w.places[i].1;
    // Where something is: each place it's at, any of its needs there.
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
        spots.collect::<Vec<_>>()
    };
    // One spot per place, false ones left out.
    let spots = |at: Vec<(usize, Value)>| {
        let mut by_place: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
        for (place, rule) in at {
            by_place.entry(place).or_default().push(rule);
        }
        let spots = by_place
            .into_iter()
            .map(|(place, rules)| (place, any(rules)));
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

    // Each row as some seeds hold it, keyed so the seeds' copies meet.
    let mut checks: BTreeMap<u64, Row> = BTreeMap::new();
    let mut events: BTreeMap<String, Row> = BTreeMap::new();
    for (n, seed) in seeds.iter().enumerate() {
        let mut by_hash: BTreeMap<u64, Vec<(usize, &Need)>> = BTreeMap::new();
        for l in &seed.world.locations {
            by_hash.entry(l.hash).or_default().push((l.place, &l.need));
        }
        for (&hash, spots) in &by_hash {
            if seed.dealt.kept.contains_key(&hash) {
                if reach.checks.contains(&hash) {
                    let at = at(&mut rules, spots.clone());
                    let row = json!({ "name": vanilla(hash) });
                    Row::add(&mut events, vanilla(hash), (n, seeds.len()), row, at)?;
                }
                continue;
            }
            if !seed.dealt.free.contains(&hash) {
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
            let row = json!({
                "id": ids.location(hash),
                "hash": hash.to_string(),
                "name": names.location(hash)?,
                "unprovable": !reach.checks.contains(&hash),
                "forbid": forbid,
            });
            let at = at(&mut rules, spots.clone());
            Row::add(&mut checks, hash, (n, seeds.len()), row, at)?;
        }
        let sources = seed.world.sources.iter();
        for s in sources.filter(|s| reach.sources.contains(&s.name)) {
            let at = at(&mut rules, s.places.iter().map(|&p| (p, &s.need)).collect());
            let row = json!({ "name": s.name });
            Row::add(&mut events, s.name.clone(), (n, seeds.len()), row, at)?;
        }
    }
    let goal = at(
        &mut rules,
        w.goal().iter().map(|(p, need)| (*p, need)).collect(),
    );
    let goal = spots(goal);
    // The settings a row or spot holds in, or none when it always does.
    let when = |seeds: &[u32], what: &Value| -> Result<Option<Value>> {
        let counts = Counts {
            base: seeds[0],
            by_variant: &seeds[1..],
        };
        match steps(&domains, &variants, &counts).as_slice() {
            [(1, Value::Bool(true))] => Ok(None),
            [(1, rule)] => Ok(Some(rule.clone())),
            other => bail!("{what} isn't held once: {other:?}"),
        }
    };
    let rows = |rows: Vec<Row>| -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for row in rows {
            let mut value = row.value.clone();
            if let Some(w) = when(&row.seeds, &value)? {
                value["when"] = w;
            }
            // A spot only some seeds hold, as a check Met Zelda puts at the
            // start, holds in their settings.
            let mut at = Vec::new();
            for ((place, _), (rule, seeds)) in row.at {
                if seeds == row.seeds {
                    at.push((place, rule));
                    continue;
                }
                let rule = match when(&seeds, &rule)? {
                    Some(w) => all(vec![w, rule]),
                    None => rule,
                };
                at.push((place, rule));
            }
            value["at"] = spots(at).into();
            out.push(value);
        }
        Ok(out)
    };
    let checks = rows(checks.into_values().collect())?;
    let mut events = rows(events.into_values().collect())?;
    events.push(json!({ "name": VICTORY, "at": goal }));

    // Each pool item's count in each seed, by its AP name: the pool
    // before the trade, then what settings add, which the world trades in
    // for rupees itself (`starter_bags::swap_in`). Which rupee depends on
    // every other option, so no row could say it.
    // Keyed by name and whether the trade may take it: a chest's rupee,
    // not a pot's, though both share a name.
    type Count<'a> = BTreeMap<(&'a str, bool), (Vec<u32>, &'a str)>;
    let mut counts: Count = BTreeMap::new();
    let junk_actor = |a: &str| starter_bags::JUNK.iter().any(|j| j.get() == a);
    // In `starter_bags::added`'s order, the order the world trades them.
    let mut added: Vec<(&str, (Vec<u32>, &str))> = Vec::new();
    for (n, seed) in seeds.iter().enumerate() {
        for raw in &seed.dealt.before {
            let actor = names::same(raw);
            let name = names.item(actor)?;
            let key = (name, junk_actor(raw));
            let row = counts.entry(key).or_insert((vec![0; seeds.len()], actor));
            ensure!(row.1 == actor, "{name} names two items");
            row.0[n] += 1;
        }
        for actor in seed.dealt.added.iter().map(|a| names::same(a)) {
            let name = names.item(actor)?;
            let at = match added.iter().position(|(n, _)| *n == name) {
                Some(at) => at,
                None => {
                    added.push((name, (vec![0; seeds.len()], actor)));
                    added.len() - 1
                }
            };
            added[at].1.0[n] += 1;
        }
    }
    let mut items: Vec<Value> = Vec::new();
    let mut pool = Vec::new();
    let rows = counts.iter().map(|((n, junk), c)| (*n, c, *junk, false));
    let added_rows = added.iter().map(|(n, c)| (*n, c, false, true));
    for (name, (count, actor), is_junk, is_added) in rows.chain(added_rows) {
        let class = match () {
            _ if rules.used.contains(name) => "progression",
            _ if USEFUL.contains(&name) => "useful",
            _ => "filler",
        };
        if !items.iter().any(|i| i["name"] == name) {
            items.push(json!({ "id": ids.item(actor), "name": name, "class": class }));
        }
        let counts = Counts {
            base: count[0],
            by_variant: &count[1..],
        };
        for (count, rule) in steps(&domains, &variants, &counts) {
            let mut row = json!({ "item": name, "count": count });
            if rule != Value::Bool(true) {
                row["when"] = rule;
            }
            if is_added {
                row["added"] = true.into();
            }
            if is_junk {
                row["junk"] = true.into();
            }
            pool.push(row);
        }
    }
    let junk: Vec<&str> = (starter_bags::JUNK.iter())
        .map(|j| names.item(names::same(j.get())))
        .collect::<Result<_>>()?;
    // A source that gives a pool item shares its name, so AP counts both.
    for seed in &seeds {
        for s in &seed.world.sources {
            let added = added.iter().find(|(n, _)| *n == s.name).map(|(_, c)| c);
            let pooled = [false, true]
                .into_iter()
                .find_map(|j| counts.get(&(s.name.as_str(), j)));
            if let Some((_, actor)) = pooled.or(added) {
                ensure!(
                    *actor == s.item,
                    "{} names both an item and another's event",
                    s.name
                );
            }
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
    let golden = super::golden::golden(&settings, &set, &names, &ids)?;
    let mut out = BTreeMap::new();
    out.insert("world.json", pretty(&world)?);
    out.insert("items.json", pretty(&Value::Array(items))?);
    out.insert("pool.json", pretty(&json!({ "junk": junk, "rows": pool }))?);
    out.insert("options.json", pretty(&options())?);
    out.insert("golden.json", pretty(&golden)?);
    out.insert("ids.json", pretty(&serde_json::to_value(&ids)?)?);
    Ok(out)
}

/// A row of the export as the seeds hold it: how many of the seeds, in
/// order (the base first), hold it, the first one's copy less where it
/// is, and each spot (place and rule) with the seeds holding it.
struct Row {
    seeds: Vec<u32>,
    value: Value,
    at: BTreeMap<(usize, String), (Value, Vec<u32>)>,
}

impl Row {
    /// Notes that seed `n` of `seeds` holds `value` under `key`, at `at`.
    /// Every seed holding a row must write it the same way.
    fn add<K: Ord>(
        rows: &mut BTreeMap<K, Row>,
        key: K,
        (n, seeds): (usize, usize),
        value: Value,
        at: Vec<(usize, Value)>,
    ) -> Result<()> {
        let row = rows.entry(key).or_insert_with(|| Row {
            seeds: vec![0; seeds],
            value: value.clone(),
            at: BTreeMap::new(),
        });
        ensure!(
            row.value == value,
            "seeds write a row two ways:\n{}\n{}",
            row.value,
            value
        );
        row.seeds[n] += 1;
        for (place, rule) in at {
            let spot =
                (row.at.entry((place, rule.to_string()))).or_insert_with(|| (rule, vec![0; seeds]));
            spot.1[n] += 1;
        }
        Ok(())
    }
}

/// Each edge's ends and need.
fn edges(w: &crate::logic::World) -> Vec<(Option<usize>, usize, String)> {
    w.edges
        .iter()
        .map(|e| (e.from, e.to, e.need.to_string()))
        .collect()
}

/// The event a check the deal leaves vanilla stands for.
fn vanilla(hash: u64) -> String {
    format!("vanilla-{hash}")
}

/// Each check any of `seeds` shuffles, once.
fn unique_free(seeds: &[&Seed]) -> Vec<ItemCheck> {
    let mut seen = BTreeSet::new();
    let free = (seeds.iter()).flat_map(|s| {
        s.dealt
            .checks
            .iter()
            .filter(|c| s.dealt.free.contains(&c.hash))
    });
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
            "id": d.id, "label": d.label, "description": described(&d.id, d.description),
            "kind": "choice", "default": default,
            "options": (options.iter())
                .filter(|o| offers(&d.id, &o.id))
                .map(|o| json!({ "id": o.id, "label": o.label }))
                .collect::<Vec<_>>(),
        }),
        Kind::Inventory { default } => json!({
            "id": d.id, "label": d.label, "description": d.description,
            "kind": "inventory", "default": default,
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
    fn new(romfs: &Romfs, seeds: &[&Seed]) -> Result<Self> {
        let seed = seeds[0];
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
            locations: names::locations(romfs, &unique_free(seeds))?,
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
    use std::sync::OnceLock;

    use super::*;
    use crate::ap::when::holds;

    /// The export of the committed ids, made once for every test.
    fn exported(romfs: &Romfs) -> &'static BTreeMap<&'static str, String> {
        static EXPORT: OnceLock<BTreeMap<&'static str, String>> = OnceLock::new();
        EXPORT.get_or_init(|| {
            let ids = std::fs::read_to_string(super::super::data_dir().join("ids.json")).unwrap();
            export(romfs, Some(&ids)).unwrap()
        })
    }

    /// The export's rows kept for `settings` against a seed built with
    /// them: the same checks, events and pool, and one item a check.
    fn rows_match(romfs: &Romfs, settings: &Settings) -> std::result::Result<(), String> {
        let files = exported(romfs);
        let world: Value = serde_json::from_str(&files["world.json"]).unwrap();
        let pool: Value = serde_json::from_str(&files["pool.json"]).unwrap();
        // Every offered option's value, as the world reads them.
        let mut options = Settings::new();
        for d in domains().unwrap() {
            let value = settings.get(&d.id).unwrap_or(&d.values[0]).clone();
            options.insert(d.id, value);
        }
        let kept = |r: &&Value| r.get("when").is_none_or(|w| holds(w, &options));
        let rows = |key: &str| -> Vec<&Value> {
            world[key].as_array().unwrap().iter().filter(kept).collect()
        };
        let seed = Seed::build(romfs, settings, &Default::default()).unwrap();
        let mut want = BTreeSet::new();
        let mut events = BTreeSet::new();
        for l in &seed.world.locations {
            match () {
                _ if seed.dealt.free.contains(&l.hash) => want.insert(l.hash.to_string()),
                _ if seed.dealt.kept.contains_key(&l.hash) => events.insert(vanilla(l.hash)),
                _ => false,
            };
        }
        let got: BTreeSet<String> = rows("checks")
            .iter()
            .map(|c| c["hash"].as_str().unwrap().to_owned())
            .collect();
        let mut wrong = Vec::new();
        if got != want {
            let extra: Vec<_> = got.difference(&want).collect();
            let missing: Vec<_> = want.difference(&got).collect();
            wrong.push(format!("checks: extra {extra:?}, missing {missing:?}"));
        }
        // Every event the seed has is a row; the export leaves out what no
        // seed reaches.
        let names: BTreeSet<String> = rows("events")
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_owned())
            .collect();
        events.extend(seed.world.sources.iter().map(|s| s.name.clone()));
        let stray: Vec<_> = names
            .iter()
            .filter(|n| *n != VICTORY && !events.contains(*n))
            .collect();
        if !stray.is_empty() {
            wrong.push(format!("events the seed hasn't: {stray:?}"));
        }
        let mut want_pool: BTreeMap<String, u64> = BTreeMap::new();
        for a in &seed.dealt.pool {
            let name = names::item(names::same(a), seed.dealt.new_game.progressive).unwrap();
            *want_pool.entry(name.to_owned()).or_default() += 1;
        }
        let held: Vec<&Value> = pool["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(kept)
            .collect();
        let junk: Vec<&str> = (pool["junk"].as_array().unwrap().iter())
            .map(|j| j.as_str().unwrap())
            .collect();
        let got_pool = traded(&held, &junk);
        if got_pool != want_pool {
            let keys: BTreeSet<&String> = got_pool.keys().chain(want_pool.keys()).collect();
            let off: Vec<_> = (keys.into_iter())
                .filter(|k| got_pool.get(*k) != want_pool.get(*k))
                .map(|k| format!("{k}: {:?} != {:?}", got_pool.get(k), want_pool.get(k)))
                .collect();
            wrong.push(format!("pool (rows != seed): {}", off.join(", ")));
        }
        if got_pool.values().sum::<u64>() != got.len() as u64 {
            wrong.push(format!(
                "{} items for {} checks",
                got_pool.values().sum::<u64>(),
                got.len()
            ));
        }
        match wrong.is_empty() {
            true => Ok(()),
            false => Err(wrong.join("; ")),
        }
    }

    /// The pool `rows` hold, each added item traded in for the cheapest
    /// of `junk` left among the rows the trade may take, as the world does
    /// it.
    fn traded(rows: &[&Value], junk: &[&str]) -> BTreeMap<String, u64> {
        let mut pool: BTreeMap<String, u64> = BTreeMap::new();
        let mut takeable: BTreeMap<&str, u64> = BTreeMap::new();
        let mut added = Vec::new();
        for r in rows {
            let (item, count) = (r["item"].as_str().unwrap(), r["count"].as_u64().unwrap());
            match r.get("added").is_some() {
                true => added.extend((0..count).map(|_| item)),
                false => *pool.entry(item.to_owned()).or_default() += count,
            }
            if r.get("junk").is_some() {
                *takeable.entry(item).or_default() += count;
            }
        }
        for item in added {
            let rupee = junk
                .iter()
                .find(|j| takeable.get(**j).is_some_and(|n| *n > 0));
            if let Some(rupee) = rupee {
                *takeable.get_mut(*rupee).unwrap() -= 1;
                *pool.get_mut(*rupee).unwrap() -= 1;
                *pool.entry(item.to_owned()).or_default() += 1;
            }
        }
        pool.retain(|_, n| *n > 0);
        pool
    }

    /// Each case's seed fills on its own, so the cases run in parallel.
    fn assert_rows_match(romfs: &Romfs, cases: impl IntoIterator<Item = Settings>) {
        let cases: Vec<Settings> = cases.into_iter().collect();
        let wrong: Vec<String> = par_map(&cases, |settings| {
            rows_match(romfs, settings).err().map(|e| {
                let mine: Vec<_> = (settings.iter())
                    .filter(|(k, v)| base().get(*k) != Some(v))
                    .collect();
                format!("{mine:?}: {e}")
            })
        })
        .into_iter()
        .flatten()
        .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn each_variant_keeps_its_own_seeds_rows() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let variants = variants().unwrap();
        let cases = std::iter::once(base()).chain(variants.iter().map(Variant::settings));
        assert_rows_match(&romfs, cases);
    }

    /// Every option at its last value at once: the rows add up across
    /// options no variant combines.
    #[test]
    fn every_option_at_once_keeps_its_seeds_rows() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut settings = base();
        for d in domains().unwrap() {
            settings.insert(d.id, d.values.last().unwrap().clone());
        }
        assert_rows_match(&romfs, [settings]);
    }

    /// Each pair of options at each pair of other values. About 330 CPU
    /// seconds, so it runs only in CI (the dump job). Run it locally with
    /// `CI=1`.
    #[test]
    fn each_pair_of_options_keeps_its_seeds_rows() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let domains = domains().unwrap();
        let mut cases = Vec::new();
        for (i, a) in domains.iter().enumerate() {
            for b in &domains[i + 1..] {
                for va in &a.values[1..] {
                    for vb in &b.values[1..] {
                        let mut s = base();
                        s.insert(a.id.clone(), va.clone());
                        s.insert(b.id.clone(), vb.clone());
                        cases.push(s);
                    }
                }
            }
        }
        assert_rows_match(&romfs, cases);
    }

    #[test]
    fn the_committed_data_matches_a_fresh_export() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = super::super::data_dir();
        for (name, text) in exported(&romfs) {
            let committed = std::fs::read_to_string(dir.join(name)).unwrap_or_default();
            assert!(
                committed == *text,
                "{name} is stale: run `zootr ap export` and commit it"
            );
        }
    }

    #[test]
    fn the_export_spells_no_internal_id() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (name, text) in exported(&romfs) {
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
