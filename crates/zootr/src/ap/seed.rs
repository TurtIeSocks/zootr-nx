//! One seed's settings as the export resolves them, and the exposed
//! options' settings.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;
use serde_json::Value;

use super::OPTIONS;
use crate::checks::{self, ItemCheck};
use crate::fill;
use crate::logic::{Got, Play, World};
use crate::rules::{Book, Ctx, Have};
use crate::settings::{Kind, NewGame, Settings, schema};
use crate::shuffle::{self, Fixed};

/// What Link reaches holding everything. The rest never happens, so the
/// export leaves it out.
#[derive(PartialEq)]
pub(super) struct Everything {
    pub checks: HashSet<u64>,
    /// By index into the world's sources.
    pub sources: BTreeSet<usize>,
    /// By index into the world's places.
    pub places: BTreeSet<usize>,
}

impl Everything {
    #[cfg(test)]
    fn covers(&self, other: &Self) -> bool {
        self.checks.is_superset(&other.checks)
            && self.sources.is_superset(&other.sources)
            && self.places.is_superset(&other.places)
    }
}

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

    /// What Link reaches holding `have`, kept checks giving theirs.
    pub fn play(&self, have: &Have) -> Play {
        let kept = self.kept.iter().map(|(h, i)| (*h, i.as_str())).collect();
        self.world.play(have, &kept)
    }

    /// What Link reaches holding everything.
    pub fn everything(&self) -> Everything {
        let mut all = self.start.clone();
        self.pool.iter().for_each(|i| all.add(i));
        let play = self.play(&all);
        let sources = (play.rounds.iter().flatten())
            .filter_map(|g| match g {
                Got::Source(i) => Some(*i),
                Got::Check(_) => None,
            })
            .collect();
        let seen = self.world.seen(&play.have).into_iter().enumerate();
        Everything {
            checks: play.reached,
            sources,
            places: seen.filter(|(_, s)| *s).map(|(i, _)| i).collect(),
        }
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
        let base_all = base_seed.everything();
        let mut changed = Vec::new();
        for (id, value) in variants().unwrap() {
            let mut s = base();
            s.insert(id.clone(), value.clone());
            let seed = Seed::build(&romfs, &s, &open()).unwrap();
            let rules = !read.contains(&id) && seed.needs() != base_seed.needs();
            // The export leaves out what the base can't reach, so no value
            // may reach less.
            let less = !seed.everything().covers(&base_all);
            if seed.shape() != base_seed.shape() || rules || less {
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
}
