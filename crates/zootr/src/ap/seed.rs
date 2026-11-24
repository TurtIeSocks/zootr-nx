//! One seed's settings as the export resolves them, and the exposed
//! options' settings.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde_json::Value;

use super::when::{Domain, Variant};
use super::{JOINT, OPTIONS, names, offers};
use crate::checks::{self, ItemCheck};
use crate::fill;
use crate::ice_traps;
use crate::logic::{Got, Play, World};
use crate::progressive::Lines;
use crate::rules::{Book, Ctx, Have};
use crate::settings::{
    BOMBCHUS, CHESTS, IceTraps, Kind, MAGIC_WITH_FIRST_ITEM, NewGame, RUNNING_RACE, SCRUBS,
    SHUFFLE_NUT_BAG, SHUFFLE_STICK_BAG, Settings, schema, value_word,
};
use crate::shuffle::{self, Fixed};
use crate::starting;

/// What Link reaches holding everything. The rest never happens, so the
/// export leaves it out.
#[derive(Default, PartialEq)]
pub(super) struct Everything {
    pub checks: HashSet<u64>,
    /// By name: the variants' worlds hold different sources.
    pub sources: BTreeSet<String>,
    /// By index into the world's places.
    pub places: BTreeSet<usize>,
}

impl Everything {
    /// What Link reaches holding everything in any of `seeds`, each built
    /// with its settings set: what an option lets Link into, like the dead
    /// tree under Adult Deku Tree, or Bombchu Bowling's prizes with NPC
    /// Gifts on and Bombchus free. The rest never happens.
    pub fn union(seeds: &[Seed]) -> Self {
        let mut reach = Everything::default();
        for other in seeds.iter().map(Seed::everything) {
            reach.checks.extend(other.checks);
            reach.sources.extend(other.sources);
            reach.places.extend(other.places);
        }
        reach
    }
}

/// One seed's settings as the export sees them: what's dealt, what Link
/// starts with, and the graph.
pub(super) struct Seed {
    pub dealt: Dealt,
    pub start: Have,
    pub world: World,
    /// The items of the deal's pool a trap may take (`ice_traps::junk`).
    pub trappable: BTreeSet<String>,
}

/// One seed's deal as the export sees it: the checks AP places, and the
/// items it places there.
pub(super) struct Dealt {
    pub new_game: NewGame,
    pub checks: Vec<ItemCheck>,
    /// The checks the deal shuffles.
    pub free: BTreeSet<u64>,
    /// The item each other check keeps.
    pub kept: HashMap<u64, String>,
    pub pool: Vec<String>,
    /// The pool before settings' added items took their rupees: `pool`
    /// with each traded one turned back. What the world sums before it
    /// trades them in, as `starter_bags::swap_in` does.
    pub before: Vec<String>,
    /// What settings add to the pool, in `starter_bags::added`'s order, at
    /// the level the pool holds each.
    pub added: Vec<String>,
}

impl Dealt {
    /// The actors AP ids can name for this slot: the pool's, and the trap
    /// the room may send when the seed has traps.
    pub fn actors(&self) -> Vec<&str> {
        let pool = self.pool.iter().map(|a| names::same(a));
        let trap = (self.new_game.ice_traps != IceTraps::Off).then_some(ice_traps::POOLED);
        pool.chain(trap).collect()
    }

    pub fn new(romfs: &Romfs, settings: &Settings) -> Result<Self> {
        let new_game = NewGame::from_settings(settings, &[])?;
        let checks = checks::dealt(romfs, &new_game)?;
        let fixed = Fixed::new();
        let deal = shuffle::deal(&checks, &fixed, &new_game)?;
        let pool: Vec<String> = deal.items.iter().map(|i| i.to_string()).collect();
        let mut before = pool.clone();
        for (item, rupee) in &deal.traded {
            let at = before.iter().position(|i| i == item);
            let at = at.with_context(|| format!("{item} was traded in but isn't dealt"))?;
            before[at] = rupee.to_string();
        }
        let added = deal.added.iter().map(|i| i.to_string()).collect();
        let free: BTreeSet<u64> = deal.free.iter().map(|c| c.hash).collect();
        let mut kept: HashMap<u64, String> = (deal.placed.iter())
            .map(|p| (p.check.hash, p.item.to_owned()))
            .collect();
        for c in checks.iter().filter(|c| !free.contains(&c.hash)) {
            kept.entry(c.hash).or_insert_with(|| c.item.clone());
        }
        Ok(Self {
            new_game,
            checks,
            free,
            kept,
            pool,
            before,
            added,
        })
    }
}

/// `new_game`'s upgrade lines, each a setting in `open` can lengthen at
/// its longest, so every value reads the rules the same way. A longer line
/// only lets a higher level stand in for a lower one.
fn lines(new_game: &NewGame, open: &BTreeSet<String>) -> Lines {
    let mut lines = Lines::of(new_game);
    lines.nut_bag |= open.contains(SHUFFLE_NUT_BAG);
    lines.stick_bag |= open.contains(SHUFFLE_STICK_BAG);
    lines.bombchu_bags |= open.contains(&value_word(BOMBCHUS, "bag"));
    lines.magic_meter |= open.contains(MAGIC_WITH_FIRST_ITEM);
    lines
}

impl Seed {
    /// `settings` resolved as a normal seed would, with `open` left open.
    pub fn build(romfs: &Romfs, settings: &Settings, open: &BTreeSet<String>) -> Result<Self> {
        let dealt = Dealt::new(romfs, settings)?;
        let (new_game, checks) = (&dealt.new_game, &dealt.checks);
        let ctx = Ctx {
            open: open.clone(),
            lines: lines(new_game, open),
            ..Ctx::new(new_game)
        };
        let world = World::build(romfs, &Book::load()?, &ctx, checks)?;
        let start = fill::start(new_game);
        let needed = world.items();
        let pool = dealt.pool.iter().map(String::as_str);
        let trappable = ice_traps::junk(romfs, pool, &needed)?;
        let trappable = trappable.into_iter().map(str::to_owned).collect();
        Ok(Self {
            dealt,
            start,
            world,
            trappable,
        })
    }

    /// How many traps the app's seed puts in the pool, as its own swap
    /// counts them.
    #[cfg(test)]
    pub fn traps(&self, romfs: &Romfs) -> usize {
        let mut items: Vec<&str> = self.dealt.pool.iter().map(String::as_str).collect();
        let (needed, how) = (self.world.items(), self.dealt.new_game.ice_traps);
        ice_traps::swap(romfs, &mut items, &needed, how, 0).unwrap();
        items.iter().filter(|i| ice_traps::is_trap(i)).count()
    }

    /// What Link reaches holding `have`, kept checks giving theirs.
    pub fn play(&self, have: &Have) -> Play {
        let kept = self
            .dealt
            .kept
            .iter()
            .map(|(h, i)| (*h, i.as_str()))
            .collect();
        self.world.play(have, &kept)
    }

    /// What Link reaches holding everything.
    pub fn everything(&self) -> Everything {
        let mut all = self.start.clone();
        self.dealt.pool.iter().for_each(|i| all.add(i));
        let play = self.play(&all);
        let sources = (play.rounds.iter().flatten())
            .filter_map(|g| match g {
                Got::Source(i) => Some(self.world.sources[*i].name.clone()),
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
}

/// Settings the base holds at their own value, whatever the app's default,
/// so a seed from an older world still opens: chests, Business Scrubs and
/// the running man's race are locations those worlds always held, and Link
/// starts with nothing.
pub(super) fn pinned() -> Settings {
    Settings::from([
        (CHESTS.into(), Value::Bool(true)),
        (SCRUBS.into(), Value::Bool(true)),
        (RUNNING_RACE.into(), Value::Bool(true)),
        (starting::SETTING.into(), Value::Object(Default::default())),
    ])
}

/// The base settings: the pinned ones, then every other exposed toggle off.
pub(super) fn base() -> Settings {
    let defs = schema();
    let mut s = pinned();
    for id in OPTIONS
        .iter()
        .filter(|id| !s.contains_key(**id))
        .collect::<Vec<_>>()
    {
        if let Some(d) = defs.iter().find(|d| d.id == *id)
            && let Kind::Toggle { .. } = d.kind
        {
            s.insert((*id).into(), Value::Bool(false));
        }
    }
    s
}

impl Variant {
    /// The base with this variant's settings over it.
    pub fn settings(&self) -> Settings {
        let mut s = base();
        s.extend(self.overlay.clone());
        s
    }
}

/// Each exposed option at each value besides the base's, alone over the
/// base: the toggle a setting `requires` comes along on. The settings a
/// start flag or item is read from.
pub(super) fn values() -> Result<Vec<Settings>> {
    let defs = schema();
    let mut out = Vec::new();
    for d in domains()? {
        let def = defs.iter().find(|x| x.id == d.id);
        let parent = def.and_then(|x| x.requires);
        for value in d.values.into_iter().skip(1) {
            let mut overlay = Settings::from([(d.id.clone(), value)]);
            if let Some(p) = parent {
                overlay.insert(p.into(), Value::Bool(true));
            }
            out.push(overlay);
        }
    }
    Ok(out)
}

/// Every exposed option and the values the world offers, the base's first.
pub(super) fn domains() -> Result<Vec<Domain>> {
    let defs = schema();
    let base = base();
    let mut out = Vec::new();
    for id in OPTIONS {
        let def = defs.iter().find(|d| d.id == *id);
        let values: Vec<Value> = match def
            .map(|d| &d.kind)
            .with_context(|| format!("no setting {id}"))?
        {
            Kind::Toggle { .. } => {
                let first = base.get(*id).and_then(Value::as_bool).unwrap_or(false);
                vec![first.into(), (!first).into()]
            }
            Kind::Choice {
                default, options, ..
            } => {
                let first = base.get(*id).and_then(Value::as_str).unwrap_or(default);
                let mut v = vec![Value::String(first.to_owned())];
                let others = (options.iter()).filter(|o| o.id != first && offers(id, &o.id));
                v.extend(others.map(|o| Value::String(o.id.clone())));
                v
            }
            Kind::Inventory { .. } => continue,
        };
        out.push(Domain {
            id: (*id).into(),
            values,
        });
    }
    Ok(out)
}

/// The seeds the export is built from, beside the base: each option's
/// other values alone, and each [`JOINT`] group's combinations whole. A
/// setting that `requires` another is varied with that one on, from the
/// seed with it on.
pub(super) fn variants() -> Result<Vec<Variant>> {
    let defs = schema();
    let domains = domains()?;
    let domain = |id: &str| domains.iter().find(|d| d.id == id);
    let mut out: Vec<Variant> = Vec::new();
    for group in JOINT {
        let members: Vec<&Domain> = group
            .iter()
            .map(|id| domain(id).with_context(|| format!("{id} in JOINT isn't offered")))
            .collect::<Result<_>>()?;
        let mut combos: Vec<Settings> = vec![Settings::new()];
        for d in &members {
            combos = (combos.into_iter())
                .flat_map(|c| {
                    d.values.iter().map(move |v| {
                        let mut c = c.clone();
                        c.insert(d.id.clone(), v.clone());
                        c
                    })
                })
                .collect();
        }
        // The combination of every base value is the base.
        for overlay in combos.into_iter().skip(1) {
            out.push(Variant {
                key: overlay.clone(),
                overlay,
                from: None,
            });
        }
    }
    let joint = |id: &str| JOINT.iter().any(|g| g.contains(&id));
    for d in domains.iter().filter(|d| !joint(&d.id)) {
        let parent = defs.iter().find(|x| x.id == d.id).and_then(|x| x.requires);
        for value in d.values.iter().skip(1) {
            let mut overlay = Settings::from([(d.id.clone(), value.clone())]);
            let mut key = overlay.clone();
            let from = match parent {
                None => None,
                Some(p) => {
                    key.insert(p.into(), Value::Bool(true));
                    let mut with_parent = Settings::from([(p.to_owned(), Value::Bool(true))]);
                    // A parent in a group comes with its group's base values.
                    if let Some(g) = JOINT.iter().find(|g| g.contains(&p)) {
                        for id in g.iter().filter(|id| **id != p) {
                            let first = domain(id).map(|x| x.values[0].clone());
                            with_parent.insert((*id).into(), first.unwrap_or_default());
                        }
                    }
                    overlay.extend(with_parent.clone());
                    let at = out.iter().position(|v| v.overlay == with_parent);
                    Some(at.with_context(|| format!("no seed with {p} on for {}", d.id))?)
                }
            };
            out.push(Variant { overlay, key, from });
        }
    }
    Ok(out)
}

/// The setting words of the exposed options, which rules can read: each
/// toggle's id and its `on` and `off`, each choice's offered options.
pub(super) fn open() -> BTreeSet<String> {
    let defs = schema();
    let mut out = BTreeSet::new();
    for def in OPTIONS
        .iter()
        .filter_map(|id| defs.iter().find(|d| d.id == *id))
    {
        let id = def.id.as_str();
        match &def.kind {
            Kind::Toggle { .. } => {
                out.insert(id.to_owned());
                out.extend(["on", "off"].map(|v| value_word(id, v)));
            }
            Kind::Choice { options, .. } => {
                let offered = options.iter().filter(|o| offers(id, &o.id));
                out.extend(offered.map(|o| value_word(id, &o.id)));
            }
            Kind::Inventory { .. } => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::settings_read;

    #[test]
    fn an_exposed_rule_setting_has_no_parent() {
        let defs = schema();
        for id in OPTIONS.iter().filter(|id| settings_read().contains(**id)) {
            let def = defs.iter().find(|d| d.id == *id).unwrap();
            assert_eq!(def.parent, None, "{id}");
        }
    }

    /// A file from a world before these options opens with what that
    /// world always held: chests, Business Scrubs and the race.
    #[test]
    fn an_older_file_keeps_chests_scrubs_and_the_race() {
        let base = base();
        for id in [CHESTS, SCRUBS, RUNNING_RACE] {
            assert_eq!(base.get(id), Some(&Value::Bool(true)), "{id}");
        }
        let scrubs = domains().unwrap().into_iter().find(|d| d.id == SCRUBS);
        assert_eq!(
            scrubs.unwrap().values,
            [Value::Bool(true), Value::Bool(false)]
        );
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
