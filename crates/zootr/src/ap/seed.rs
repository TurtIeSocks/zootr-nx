//! One seed's settings as the export resolves them, and the exposed
//! options' settings.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde_json::Value;

use super::ids::Ids;
use super::when::{Domain, Variant};
use super::{JOINT, exposed, names, offers};
use crate::checks::{self, ItemCheck};
use crate::fill;
use crate::ice_traps;
use crate::logic::{Data, Got, Play, World};
use crate::progressive::Lines;
use crate::rules::{Book, Ctx, Have};
use crate::settings::{
    BOMBCHUS, CHESTS, IceTraps, Kind, MAGIC_WITH_FIRST_ITEM, NewGame, RUNNING_RACE, SCRUBS,
    SHUFFLE_NUT_BAG, SHUFFLE_STICK_BAG, Settings, schema, value_word,
};
use crate::shuffle::{self, Fixed};
use crate::starting;

/// Every item the world names, by its AP ids.
pub(super) fn named_by_id(ids: &Ids) -> HashMap<u64, &'static str> {
    ids.actors(names::named().map(names::same))
}

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
    /// What Link reaches holding everything in any of the seeds `each` came
    /// from, each built with its settings set: what an option lets Link
    /// into, like the dead tree under Adult Deku Tree, or Bombchu Bowling's
    /// prizes with NPC Gifts on and Bombchus free. The rest never happens.
    pub fn union(each: impl IntoIterator<Item = Everything>) -> Self {
        let mut reach = Everything::default();
        for other in each {
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

    /// The actor Link gets for each AP id this slot's items go by: the
    /// pool's and the trap's. With progressive items on, a line goes by
    /// its key level's id (`names::line`) and gives this seed's first level
    /// of it, which the hook climbs from: the Nut Bag (20) with its shuffle
    /// on, the (30) without, Double Magic alone with magic from the first
    /// magic item.
    pub fn by_id(&self, ids: &Ids) -> Result<HashMap<u64, &str>> {
        let mut out = ids.actors(self.actors());
        if !self.new_game.progressive {
            return Ok(out);
        }
        for actor in self.actors() {
            if let Some((_, key)) = names::line(actor)?
                && let Some(id) = ids.item_id(key)
            {
                out.insert(id, actor);
            }
        }
        Ok(out)
    }

    /// [`Dealt::by_id`] over every item the world names: the room can
    /// send one this slot never deals (a host's `/send`, AP's start
    /// inventory), and Link gets it as itself.
    pub fn any_by_id(&self, ids: &Ids) -> Result<HashMap<u64, &str>> {
        let mut out: HashMap<u64, &str> = named_by_id(ids);
        out.extend(self.by_id(ids)?);
        Ok(out)
    }

    pub fn new(romfs: &Romfs, settings: &Settings) -> Result<Self> {
        Self::of(romfs, checks::all(romfs)?, settings)
    }

    /// [`Dealt::new`] from every check, as [`checks::all`] gives them.
    fn of(romfs: &Romfs, all: Vec<ItemCheck>, settings: &Settings) -> Result<Self> {
        // Every world has Saria's and Zelda's ocarinas as locations,
        // whatever NPC Gifts says.
        let new_game = NewGame {
            keep_gifts: false,
            ..NewGame::from_settings(settings, &[])?
        };
        let checks = checks::dealt_of(romfs, all, &new_game)?;
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
            kept.entry(c.hash)
                .or_insert_with(|| crate::keys::held(c).to_owned());
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
    if open.contains(&value_word(BOMBCHUS, "bag")) {
        lines.bombchus = crate::settings::Bombchus::Bag;
    }
    lines.magic_meter |= open.contains(MAGIC_WITH_FIRST_ITEM);
    lines
}

/// What every seed reads from the game, read once for the export's many.
pub struct Dump<'a> {
    romfs: &'a Romfs,
    /// Every check, before a seed's settings pick from them.
    all: Vec<ItemCheck>,
    data: Data<'a>,
    book: Book,
}

impl<'a> Dump<'a> {
    pub fn new(romfs: &'a Romfs) -> Result<Self> {
        Ok(Self {
            romfs,
            all: checks::all(romfs)?,
            data: Data::every(romfs)?,
            book: Book::load()?,
        })
    }
}

impl Seed {
    /// `settings` resolved as a normal seed would, with `open` left open.
    #[cfg(test)]
    pub fn build(romfs: &Romfs, settings: &Settings, open: &BTreeSet<String>) -> Result<Self> {
        Self::build_in(&Dump::new(romfs)?, settings, open)
    }

    /// [`Seed::build`] from game data read once.
    pub fn build_in(dump: &Dump, settings: &Settings, open: &BTreeSet<String>) -> Result<Self> {
        let romfs = dump.romfs;
        let dealt = Dealt::of(romfs, dump.all.clone(), settings)?;
        let (new_game, checks) = (&dealt.new_game, &dealt.checks);
        let ctx = Ctx {
            open: open.clone(),
            lines: lines(new_game, open),
            ..Ctx::new(new_game)
        };
        let mut data = dump.data.only(checks);
        let world = World::from_data(&mut data, &dump.book, &ctx, checks)?;
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
/// so a seed from an older world still opens: chests, Business Scrubs and the running
/// man's race are locations those worlds always held, and Link starts with
/// nothing.
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
    for id in exposed() {
        if let Some(d) = defs.iter().find(|d| d.id == id)
            && let Kind::Toggle { .. } = d.kind
        {
            s.entry(id).or_insert(Value::Bool(false));
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
        let parent = def.and_then(|x| x.requires).and_then(|r| r.toggle());
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
    for id in &exposed() {
        let def = defs.iter().find(|d| d.id == *id);
        let values: Vec<Value> = match def
            .map(|d| &d.kind)
            .with_context(|| format!("no setting {id}"))?
        {
            Kind::Toggle { .. } => {
                let first = base.get(id).and_then(Value::as_bool).unwrap_or(false);
                vec![first.into(), (!first).into()]
            }
            Kind::Choice {
                default, options, ..
            } => {
                let first = base.get(id).and_then(Value::as_str).unwrap_or(default);
                let mut v = vec![Value::String(first.to_owned())];
                let others = (options.iter()).filter(|o| o.id != first && offers(id, &o.id));
                v.extend(others.map(|o| Value::String(o.id.clone())));
                v
            }
            Kind::Inventory { .. } => continue,
        };
        out.push(Domain {
            id: id.clone(),
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
        let parent = (defs.iter().find(|x| x.id == d.id))
            .and_then(|x| x.requires)
            .and_then(|r| r.toggle());
        for value in d.values.iter().skip(1) {
            let mut overlay = Settings::from([(d.id.clone(), value.clone())]);
            let mut key = overlay.clone();
            let from = match parent {
                // The base holds it on already (Scrubs, pinned).
                Some(p) if base().get(p) == Some(&Value::Bool(true)) => None,
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
    for def in exposed()
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
    use crate::ap::OPTIONS;
    use crate::rules::settings_read;
    use crate::settings::SCRUB_PRICE;

    #[test]
    fn an_exposed_rule_setting_has_no_parent() {
        let defs = schema();
        for id in OPTIONS.iter().filter(|id| settings_read().contains(**id)) {
            let def = defs.iter().find(|d| d.id == *id).unwrap();
            assert_eq!(def.parent, None, "{id}");
        }
    }

    /// Scrub Price needs Scrubs, which the base pins on: its variants
    /// come from the base, not a Scrubs-on seed there's no need for.
    #[test]
    fn a_setting_whose_parent_the_base_holds_varies_from_the_base() {
        let variants = variants().unwrap();
        let price = (variants.iter()).filter(|v| v.overlay.contains_key(SCRUB_PRICE));
        assert!(price.clone().count() > 0);
        assert!(price.into_iter().all(|v| v.from.is_none()));
    }

    /// With progressive items on, a received line gives the seed's own
    /// first level of it, which the hook climbs from.
    #[test]
    fn a_line_s_id_gives_the_seed_s_first_level() {
        use crate::known::{Name, item};
        use crate::settings::{NPC_GIFTS, PROGRESSIVE};
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let ids = Ids::load().unwrap();
        let gives = |on: &[&str], key: &Name| {
            let mut settings = base();
            for id in [PROGRESSIVE, NPC_GIFTS].iter().chain(on) {
                settings.insert((*id).into(), true.into());
            }
            let dealt = Dealt::new(&romfs, &settings).unwrap();
            let id = ids.item_id(key.get()).unwrap();
            dealt.by_id(&ids).unwrap()[&id].to_owned()
        };
        let (nuts, meter) = (item::NUT_BAG_MEDIUM, crate::magic::meter::METER);
        assert_eq!(gives(&[], nuts), nuts.get());
        assert_eq!(gives(&[SHUFFLE_NUT_BAG], nuts), item::NUT_BAG_SMALL.get());
        assert_eq!(gives(&[], meter), meter.get());
        // Progressive items keep the meter in the line even with the
        // setting on, so the line's first level is the meter.
        assert_eq!(gives(&[MAGIC_WITH_FIRST_ITEM], meter), meter.get());
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
