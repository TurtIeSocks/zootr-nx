//! The world graph, pool and options as the `znx` world loads them. Rules
//! go out as trees of `true`, `false`, `{items, count}`, `{event}`,
//! `{setting}`, `{all}` and `{any}`, with abilities inlined, eras resolved
//! per place and start flags folded in.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;
use serde_json::{Value, json};

use super::{OPTIONS, ids::Ids, names};
use crate::checks::{self, ItemCheck};
use crate::fill;
use crate::logic::World;
use crate::rules::{Book, Ctx, Have, Need, vocab};
use crate::settings::{Kind, NewGame, Settings, schema};
use crate::shuffle::{self, Fixed, allowed};
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

/// One seed's settings as the export sees them: what's dealt, what Link
/// starts with, and the graph.
pub(super) struct Seed {
    pub new_game: NewGame,
    pub checks: Vec<ItemCheck>,
    /// The checks the deal shuffles.
    pub free: BTreeSet<u64>,
    /// The item each other check keeps.
    pub kept: HashMap<u64, String>,
    pub pool: Vec<String>,
    pub start: Have,
    pub world: World,
}

impl Seed {
    /// `settings` resolved as a normal seed would, with `open` left open.
    pub fn build(romfs: &Romfs, settings: &Settings, open: &BTreeSet<String>) -> Result<Self> {
        let new_game = NewGame::from_settings(settings, &[])?;
        let checks = checks::dealt(romfs, &new_game)?;
        let fixed = shuffle::own_dungeon(&checks, 0, &Fixed::new(), &new_game)?;
        ensure!(fixed.is_empty(), "these settings pin items, which AP can't");
        let deal = shuffle::deal(&checks, &fixed, &new_game)?;
        let pool = deal.items.iter().map(|i| i.to_string()).collect();
        let free: BTreeSet<u64> = deal.free.iter().map(|c| c.hash).collect();
        let mut kept: HashMap<u64, String> = (deal.placed.iter())
            .map(|p| (p.check.hash, p.item.to_owned()))
            .collect();
        for c in checks.iter().filter(|c| !free.contains(&c.hash)) {
            kept.entry(c.hash).or_insert_with(|| c.item.clone());
        }
        let ctx = Ctx {
            open: open.clone(),
            ..Ctx::new(&new_game)
        };
        let world = World::build(romfs, &Book::load()?, &ctx, &checks)?;
        let start = fill::start(&new_game);
        Ok(Self {
            new_game,
            checks,
            free,
            kept,
            pool,
            start,
            world,
        })
    }

    /// The checks Link reaches holding `have`, kept checks giving theirs.
    pub fn play(&self, have: &Have) -> std::collections::HashSet<u64> {
        let kept = self.kept.iter().map(|(h, i)| (*h, i.as_str())).collect();
        self.world.play(have, &kept).reached
    }

    /// What every edge, check, source and the goal need.
    #[cfg(test)]
    pub fn needs(&self) -> String {
        let w = &self.world;
        let edges = w.edges.iter().map(|e| e.need.to_string());
        let locations = w.locations.iter().map(|l| l.need.to_string());
        let sources = w.sources.iter().map(|s| s.need.to_string());
        let goal = w.goal().iter().map(|(_, n)| n.to_string());
        let all: Vec<String> = edges.chain(locations).chain(sources).chain(goal).collect();
        all.join("\n")
    }

    /// Everything about the seed but what its rules need: the checks, the
    /// pool, the start, and the graph's shape.
    #[cfg(test)]
    pub fn shape(&self) -> String {
        let w = &self.world;
        let mut pool = self.pool.clone();
        pool.sort();
        let mut start: Vec<_> = self.start.items.iter().collect();
        start.sort();
        let mut edges: Vec<_> = w.edges.iter().map(|e| (e.from, e.to)).collect();
        edges.sort();
        let mut locations: Vec<_> = w.locations.iter().map(|l| (l.hash, l.place)).collect();
        locations.sort();
        let sources: Vec<_> = (w.sources.iter())
            .map(|s| (&s.name, s.place, &s.item))
            .collect();
        let checks: BTreeSet<u64> = self.checks.iter().map(|c| c.hash).collect();
        format!(
            "{checks:?}{:?}{pool:?}{start:?}{:?}{edges:?}{locations:?}{sources:?}",
            self.free, w.places
        )
    }
}

/// The base settings: every exposed toggle off.
pub(super) fn base() -> Settings {
    let defs = schema();
    let mut s = Settings::new();
    for id in OPTIONS {
        if let Some(d) = defs.iter().find(|d| d.id == *id)
            && let Kind::Toggle { .. } = d.kind
        {
            s.insert((*id).into(), Value::Bool(false));
        }
    }
    s
}

/// The settings each exposed option can take besides the base's.
pub(super) fn variants() -> Result<Vec<(String, Value)>> {
    let defs = schema();
    let mut out = Vec::new();
    for id in OPTIONS {
        let def = defs.iter().find(|d| d.id == *id);
        match def
            .map(|d| &d.kind)
            .with_context(|| format!("no setting {id}"))?
        {
            Kind::Toggle { .. } => out.push(((*id).into(), Value::Bool(true))),
            Kind::Choice {
                default, options, ..
            } => {
                let others = options.iter().filter(|o| o.id != *default);
                out.extend(others.map(|o| ((*id).into(), Value::String(o.id.clone()))));
            }
        }
    }
    Ok(out)
}

/// The exposed ids that are toggles, which rules can read.
pub(super) fn open() -> BTreeSet<String> {
    base().into_keys().collect()
}

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
    for (hash, item) in &seed.kept {
        givers.entry(item).or_default().push(vanilla(*hash));
    }
    let mut rules = Rules {
        seed: &seed,
        names: &names,
        flag_settings: &flag_settings,
        givers: &givers,
        used: BTreeSet::new(),
    };
    let era = |i: usize| w.places[i].1;
    let at = |rules: &mut Rules, place: usize, need: &Need| {
        let rule = rules.rule(need, era(place));
        (rule != Value::Bool(false)).then(|| json!({ "place": names.place(place), "rule": rule }))
    };

    let mut places: Vec<String> = (0..w.places.len()).map(|i| names.place(i)).collect();
    places.extend([Namer::hub(Era::Child), Namer::hub(Era::Adult)]);
    ensure!(
        places.iter().collect::<BTreeSet<_>>().len() == places.len(),
        "two places share a name"
    );

    let mut exits = Vec::new();
    for e in &w.edges {
        let (from, era) = match e.from {
            Some(f) => (names.place(f), era(f)),
            None => (Namer::hub(era(e.to)), era(e.to)),
        };
        let rule = rules.rule(&e.need, era);
        if rule != Value::Bool(false) {
            exits.push(json!({ "from": from, "to": names.place(e.to), "rule": rule }));
        }
    }
    for i in 0..w.places.len() {
        let to = Namer::hub(era(i));
        exits.push(json!({ "from": names.place(i), "to": to, "rule": true }));
    }

    // Every check Link can reach once he holds everything.
    let mut all = seed.start.clone();
    seed.pool.iter().for_each(|i| all.add(i));
    let reached = seed.play(&all);

    let mut by_hash: BTreeMap<u64, Vec<Value>> = BTreeMap::new();
    for l in &w.locations {
        let spot = at(&mut rules, l.place, &l.need);
        by_hash.entry(l.hash).or_default().extend(spot);
    }
    let mut checks = Vec::new();
    let mut events = Vec::new();
    let pooled: BTreeSet<&str> = seed.pool.iter().map(String::as_str).collect();
    for (hash, at) in by_hash {
        if seed.kept.contains_key(&hash) {
            events.push(json!({ "name": vanilla(hash), "at": at }));
            continue;
        }
        let check = (seed.checks.iter().find(|c| c.hash == hash))
            .with_context(|| format!("no check {hash}"))?;
        let mut forbid = BTreeSet::new();
        for actor in pooled
            .iter()
            .filter(|a| !allowed(check, a, seed.new_game.progressive))
        {
            forbid.insert(names.item(actor)?);
        }
        checks.push(json!({
            "id": ids.location(hash),
            "hash": hash.to_string(),
            "name": names.location(hash)?,
            "at": at,
            "unprovable": !reached.contains(&hash),
            "forbid": forbid,
        }));
    }

    for s in &w.sources {
        events.push(json!({ "name": s.name, "at": at(&mut rules, s.place, &s.need).into_iter().collect::<Vec<_>>() }));
    }
    let goal: Vec<Value> = (w.goal().iter())
        .filter_map(|(p, need)| at(&mut rules, *p, need))
        .collect();
    events.push(json!({ "name": VICTORY, "at": goal }));

    let mut counts: BTreeMap<&str, (u32, &str)> = BTreeMap::new();
    for actor in &seed.pool {
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
    let free = seed.checks.iter().filter(|c| seed.free.contains(&c.hash));
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
            progressive: seed.new_game.progressive,
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

/// Turns a [`Need`] into a rule tree, noting each item name a rule uses.
struct Rules<'a> {
    seed: &'a Seed,
    names: &'a Namer,
    flag_settings: &'a BTreeMap<String, Vec<String>>,
    givers: &'a BTreeMap<&'a str, Vec<String>>,
    used: BTreeSet<String>,
}

impl Rules<'_> {
    fn rule(&mut self, need: &Need, era: Era) -> Value {
        match need {
            Need::Always => true.into(),
            Need::Never => false.into(),
            Need::Era(e) => (*e == era).into(),
            Need::Setting(id) => json!({ "setting": id }),
            Need::Flag(f) if self.seed.start.flags.contains(f) => true.into(),
            Need::Flag(f) => {
                let ids = self.flag_settings.get(f).into_iter().flatten();
                any(ids.map(|id| json!({ "setting": id })).collect())
            }
            Need::Items { actors, count } => {
                let have = &self.seed.start.items;
                let held: u32 = actors.iter().filter_map(|a| have.get(a)).sum();
                if held >= *count {
                    return true.into();
                }
                let mut names = BTreeSet::new();
                for a in actors {
                    if self.seed.pool.iter().any(|p| p == a)
                        && let Ok(n) = self.names.item(a)
                    {
                        names.insert(n.to_owned());
                    }
                    names.extend(self.givers.get(a.as_str()).into_iter().flatten().cloned());
                }
                if names.is_empty() {
                    return false.into();
                }
                self.used.extend(names.iter().cloned());
                json!({ "items": names, "count": count - held })
            }
            Need::All(needs) => all(needs.iter().map(|n| self.rule(n, era)).collect()),
            Need::Any(needs) => any(needs.iter().map(|n| self.rule(n, era)).collect()),
        }
    }
}

fn all(rules: Vec<Value>) -> Value {
    if rules.contains(&Value::Bool(false)) {
        return false.into();
    }
    let mut rest: Vec<Value> = rules
        .into_iter()
        .filter(|r| *r != Value::Bool(true))
        .collect();
    match rest.len() {
        0 => true.into(),
        1 => rest.remove(0),
        _ => json!({ "all": rest }),
    }
}

fn any(rules: Vec<Value>) -> Value {
    if rules.contains(&Value::Bool(true)) {
        return true.into();
    }
    let mut rest: Vec<Value> = rules
        .into_iter()
        .filter(|r| *r != Value::Bool(false))
        .collect();
    match rest.len() {
        0 => false.into(),
        1 => rest.remove(0),
        _ => json!({ "any": rest }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::settings_read;

    #[test]
    fn an_exposed_option_changes_only_rules() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let read = settings_read();
        let base_seed = Seed::build(&romfs, &base(), &open()).unwrap();
        let mut changed = Vec::new();
        for (id, value) in variants().unwrap() {
            let mut s = base();
            s.insert(id.clone(), value.clone());
            let seed = Seed::build(&romfs, &s, &open()).unwrap();
            let rules = !read.contains(&id) && seed.needs() != base_seed.needs();
            if seed.shape() != base_seed.shape() || rules {
                changed.push(format!("{id} = {value}"));
            }
        }
        assert_eq!(changed, Vec::<String>::new());
    }

    #[test]
    fn an_exposed_rule_setting_has_no_parent() {
        let defs = schema();
        for id in OPTIONS.iter().filter(|id| settings_read().contains(**id)) {
            let def = defs.iter().find(|d| d.id == *id).unwrap();
            assert_eq!(def.parent, None, "{id}");
        }
    }

    #[test]
    fn a_normal_seed_leaves_no_setting_open() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let seed = Seed::build(&romfs, &base(), &BTreeSet::new()).unwrap();
        assert!(!seed.needs().contains("setting:"));
    }

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

    #[test]
    fn all_and_any_fold_constants() {
        let item = json!({ "items": ["Hookshot"], "count": 1 });
        assert_eq!(all(vec![true.into(), item.clone()]), item);
        assert_eq!(all(vec![false.into(), item.clone()]), Value::Bool(false));
        assert_eq!(any(vec![false.into(), item.clone()]), item);
        assert_eq!(any(vec![true.into(), item.clone()]), Value::Bool(true));
        assert_eq!(all(vec![]), Value::Bool(true));
        assert_eq!(any(vec![]), Value::Bool(false));
    }
}
