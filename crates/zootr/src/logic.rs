//! The world graph: places (a Banc node in one era), the exits between
//! them, and what each check and vanilla source needs, resolved for one
//! seed's settings from the rule files, the logic graphs and the obstacles.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;

use crate::checks::{self, ItemCheck, actors, bancs, hash_of, str_field};
use crate::entrances::DUNGEONS;
use crate::known::{Name, flag, item, map};
use crate::rules::{Book, Ctx, Have, Need};
use crate::tools::Tools;
use crate::world::{self, Era, node_of};
use crate::{cows, gates, gifts, keys, pickups, scrubs, shops};

mod exits;
mod goal;

pub type Place = (String, Era);

/// Where a new file starts at `age`: the child forest, or adult in the
/// Temple of Time, which sits in the adult market.
pub fn start_of(age: Era) -> Place {
    let node = match age {
        Era::Child => map::KOKIRI_FOREST_CHILD,
        Era::Adult => map::MARKET_ADULT,
    };
    (node.to_string(), age)
}

/// What every Business Scrub wants before it sells.
const STUN_SCRUB: &str = "stun-scrub";

/// Warp songs land on a start point named after the song, which is also the
/// melody item's suffix.
pub const WARP_SONGS: &[&Name] = &[
    flag::FOREST_MINUET,
    flag::FIRE_BOLERO,
    flag::WATER_SERENADE,
    flag::SHADOW_NOCTURNE,
    flag::SPIRIT_REQUIEM,
    flag::LIGHT_PRELUDE,
];

pub struct Edge {
    /// `None` for a warp, usable from any place in the destination's era.
    pub from: Option<usize>,
    pub to: usize,
    pub need: Need,
}

/// A check at one place. A dungeon check is at both eras' copies.
pub struct Location {
    pub hash: u64,
    pub place: usize,
    pub need: Need,
}

/// An item Link gets outside the shuffle, or a flag he sets in a room.
pub struct Source {
    pub name: String,
    /// Reaching any one of them is enough.
    pub places: Vec<usize>,
    /// The item actor, or the flag as `#hash`.
    pub item: String,
    pub gives: Gives,
    pub need: Need,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gives {
    Item,
    /// A room file's `[[flag]]`.
    Flag,
}

pub struct World {
    pub places: Vec<Place>,
    pub edges: Vec<Edge>,
    pub locations: Vec<Location>,
    pub sources: Vec<Source>,
    /// Obstacle counts by tool, per node.
    pub obstacles: BTreeMap<String, BTreeMap<String, usize>>,
    /// Each exit into `GOAL`: its place and what it needs.
    goal: Vec<(usize, Need)>,
    /// Where the new file starts.
    start: Place,
}

/// Something collected in a playthrough.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Got {
    Check(u64),
    Source(usize),
}

/// What a playthrough collected.
pub struct Play {
    pub have: Have,
    /// What each round collected, everything reachable with what came before.
    pub rounds: Vec<Vec<Got>>,
    /// Every check reached.
    pub reached: HashSet<u64>,
}

/// Reads the Banc files once: places, obstacles, warp pads, and which nodes
/// each placement hash sits in.
struct Scan {
    places: BTreeSet<Place>,
    obstacles: BTreeMap<String, BTreeMap<String, usize>>,
    /// Each warp pad: its node, its song and its era.
    warps: BTreeSet<(String, String, Era)>,
    nodes_of: HashMap<u64, BTreeSet<String>>,
}

fn scan(romfs: &Romfs, tools: &mut Tools) -> Result<Scan> {
    let mut s = Scan {
        places: BTreeSet::new(),
        obstacles: BTreeMap::new(),
        warps: BTreeSet::new(),
        nodes_of: HashMap::new(),
    };
    for (file, doc) in bancs(romfs)? {
        let doc = doc?;
        let (node, eras) = node_of(&file);
        s.places.extend(eras.iter().map(|&e| (node.clone(), e)));
        for actor in actors(&doc.root)? {
            if let Some(hash) = hash_of(actor) {
                s.nodes_of.entry(hash).or_default().insert(node.clone());
            }
            if let Some(tool) = tools.of(actor)? {
                *s.obstacles
                    .entry(node.clone())
                    .or_default()
                    .entry(tool)
                    .or_default() += 1;
            }
            let gyaml = str_field(actor, checks::GYAML).unwrap_or_default();
            let name = str_field(actor, "Name").unwrap_or_default();
            if gyaml == flag::START_POS && WARP_SONGS.iter().any(|s| *s == name) {
                // Each pad is placed twice in some maps; one edge is enough.
                s.warps
                    .extend(eras.iter().map(|&e| (node.clone(), name.to_owned(), e)));
            }
        }
    }
    Ok(s)
}

/// What the world reads from the game: the same for every seed with the
/// same checks, so a seed that rolls new doors reads it once.
pub struct Data<'a> {
    tools: Tools<'a>,
    scan: Scan,
    alternatives: BTreeMap<String, String>,
    exits: BTreeSet<world::Exit>,
    gates: BTreeMap<u64, String>,
    shop_slots: Vec<shops::Slot>,
    cows: Vec<cows::Cow>,
}

impl<'a> Data<'a> {
    pub fn read(romfs: &'a Romfs, checks: &[ItemCheck]) -> Result<Self> {
        let mut tools = Tools::new(romfs);
        let scan = scan(romfs, &mut tools)?;
        Ok(Self {
            tools,
            scan,
            alternatives: world::alternatives(romfs)?,
            exits: world::exits(romfs)?,
            gates: gates::gates(romfs)?,
            shop_slots: match checks.iter().any(shops::is_shop) {
                true => shops::slots(romfs)?,
                false => Vec::new(),
            },
            cows: match checks.iter().any(cows::is_cow) {
                true => cows::find(romfs)?,
                false => Vec::new(),
            },
        })
    }
}

/// The eras a node's places come in.
fn eras_of(places: &BTreeSet<Place>, node: &str) -> Vec<Era> {
    places.iter().filter(|p| p.0 == node).map(|p| p.1).collect()
}

impl World {
    pub fn build(romfs: &Romfs, book: &Book, ctx: &Ctx, checks: &[ItemCheck]) -> Result<Self> {
        Self::from_data(&mut Data::read(romfs, checks)?, book, ctx, checks)
    }

    /// The world from game data read once, for `ctx`'s seed.
    pub fn from_data(
        data: &mut Data,
        book: &Book,
        ctx: &Ctx,
        checks: &[ItemCheck],
    ) -> Result<Self> {
        let Data {
            tools,
            scan,
            alternatives,
            exits,
            gates,
            shop_slots,
            cows,
        } = data;
        let places: Vec<Place> = scan.places.iter().cloned().collect();
        let index: HashMap<Place, usize> = places
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i))
            .collect();
        let at = |node: &str, era: Era| -> Result<usize> {
            index
                .get(&(node.to_owned(), era))
                .copied()
                .with_context(|| format!("no place {node} {era}"))
        };
        let resolve = |text: &str| book.resolve(text, ctx);

        let mut edges = Vec::new();
        for (node, song, era) in &scan.warps {
            edges.push(Edge {
                from: None,
                to: at(node, *era)?,
                need: resolve(&format!(
                    "ocarina and {}{song}",
                    item::MELODY_SPIRIT_REQUIEM.stem()
                ))?,
            });
        }

        edges.extend(exits::edges(book, ctx, scan, alternatives, exits, &at)?);

        // What each place's obstacles need. A map for one era drops the
        // obstacles only the other era clears.
        let mut by_tool = HashMap::new();
        let mut obstacles_at = |place: &Place| -> Result<Need> {
            let Some(tools) = scan.obstacles.get(&place.0) else {
                return Ok(Need::Always);
            };
            let one_era = eras_of(&scan.places, &place.0).len() == 1;
            let mut needs = Vec::new();
            for tool in tools.keys() {
                if !by_tool.contains_key(tool) {
                    by_tool.insert(tool.clone(), resolve(tool)?);
                }
                let need = &by_tool[tool];
                if !(one_era && !need.possible(place.1)) {
                    needs.push(need.clone());
                }
            }
            Ok(Need::All(needs))
        };

        // A boss room counts for either era once doors move, since its
        // dungeon may stand behind the other era's door. Gohma's always does:
        // adult Link reaches her only through a door that asks for the adult
        // Deku Tree, and both eras' rules inside are audited. The blue warp
        // keeps Link's age (`entrances.md`, "Blue warps keep Link's age").
        let boss_rooms: HashSet<&str> = DUNGEONS
            .iter()
            .filter(|d| ctx.doors.is_some() || d.node == map::INSIDE_THE_DEKU_TREE)
            .filter_map(|d| d.boss)
            .map(|b| &**b)
            .collect();
        let pinned = |node: &str, era: Era| match boss_rooms.contains(node) {
            true => eras_of(&scan.places, node),
            false => vec![era],
        };
        let pocket = gifts::pocket(&ctx.on);
        let mut locations = Vec::new();
        let mut seen = HashSet::new();
        for check in checks {
            if !seen.insert((check.hash, check.file.as_str())) {
                continue;
            }
            let rule = book.check(check.hash);
            let mut where_: Vec<Place> = Vec::new();
            if pocket == Some(check.hash) {
                where_.push(start_of(ctx.age));
            } else if let Some((node, era)) = rule.and_then(|r| r.place.clone()) {
                where_.extend(pinned(&node, era).into_iter().map(|e| (node.clone(), e)));
            } else if check.file.starts_with("Banc/") {
                let (node, eras) = node_of(&check.file);
                where_.extend(eras.into_iter().map(|e| (node.clone(), e)));
            } else {
                let scrub = scrubs::of(check).map(|s| s.placements.to_vec());
                let shop = || {
                    let slot = shop_slots.iter().find(|s| s.hash() == check.hash);
                    slot.map(|s| s.placements.clone())
                };
                let cow = || {
                    let cow = cows.iter().find(|c| c.hash() == check.hash);
                    cow.map(|c| c.placements.iter().map(|p| p.hash).collect())
                };
                for hash in scrub.or_else(shop).or_else(cow).into_iter().flatten() {
                    for node in scan.nodes_of.get(&hash).into_iter().flatten() {
                        for era in eras_of(&scan.places, node) {
                            where_.push((node.clone(), era));
                        }
                    }
                }
            }
            if where_.is_empty() {
                bail!("check {} ({}) has no place", check.hash, check.holder);
            }
            let presence = match &check.presence {
                Some(p) if !p.negated => Need::Flag(p.flag.clone()),
                _ => Need::Always,
            };
            // What Link pays there: a shop's price or a Great Fairy's offering.
            let rupees = match shops::is_shop(check) {
                true => Some(shops::price(ctx.seed, check.hash)),
                false => shops::offering(check.hash),
            };
            let cost = rupees.map_or(Need::Always, |r| shops::wallet(r, ctx.progressive));
            for place in where_ {
                let need = match rule {
                    Some(r) => book.need(&r.needs, ctx)?,
                    None => {
                        let obstacles = obstacles_at(&place)?;
                        match gates.get(&check.hash) {
                            Some(gate) => {
                                let gate = resolve(&switches(gate, tools)?)?;
                                Need::All(vec![gate, obstacles])
                            }
                            None => obstacles,
                        }
                    }
                };
                let kind = match scrubs::of(check) {
                    Some(_) => Some(STUN_SCRUB),
                    None => pickups::kind_of(check).and_then(|k| k.needs),
                };
                let need = match kind {
                    Some(word) => Need::All(vec![need, resolve(word)?]),
                    None => need,
                };
                let keys = [
                    keys::dungeon_need(check, ctx.keys),
                    keys::fortress_need(&check.item, ctx),
                ];
                let need = match keys.into_iter().flatten().collect::<Vec<_>>() {
                    keys if keys.is_empty() => need,
                    keys => Need::All([vec![need], keys].concat()),
                };
                locations.push(Location {
                    hash: check.hash,
                    place: at(&place.0, place.1)?,
                    need: Need::All(vec![presence.clone(), cost.clone(), need]),
                });
            }
        }
        for (hash, entry) in book.checks() {
            ensure!(
                entry.optional
                    || seen.iter().any(|(h, _)| *h == hash)
                    || gifts::by_hash(hash).is_some(),
                "a rule names check {hash}, which no check has"
            );
        }
        // A source a dealt check stands for, at its place with its item, or
        // a shop check takes the place of, leaves logic: the check puts the
        // item in the pool instead. A sale still sells what it sold.
        let stood_for: HashSet<(&str, &str)> = checks
            .iter()
            .filter(|c| !gifts::of(c).is_some_and(|g| g.is_sale()))
            .filter_map(|c| {
                let (place, _) = book.check(c.hash)?.place.as_ref()?;
                Some((place.as_str(), c.item.as_str()))
            })
            .collect();
        let sold = |item: &str| checks.iter().any(|c| shops::is_shop(c) && c.item == item);
        let mut sources = Vec::new();
        for s in book.sources.iter().filter(|s| !sold(&s.item)) {
            if stood_for.contains(&(s.place.as_str(), s.item.as_str())) {
                continue;
            }
            let places = (pinned(&s.place, s.era).into_iter())
                .map(|era| at(&s.place, era).with_context(|| format!("source {}", s.name)))
                .collect::<Result<_>>()?;
            sources.push(Source {
                name: s.name.clone(),
                places,
                item: s.item.clone(),
                gives: Gives::Item,
                need: match keys::fortress_need(&s.item, ctx) {
                    Some(keys) => Need::All(vec![book.need(&s.expr, ctx)?, keys]),
                    None => book.need(&s.expr, ctx)?,
                },
            });
        }
        // A room's flag, set once Link reaches its dungeon and meets its
        // needs. Room places join the graph with derive (build step 4).
        for room in &book.rooms {
            for flag in &room.file.flag {
                let places = (eras_of(&scan.places, &room.node).into_iter())
                    .map(|era| at(&room.node, era))
                    .collect::<Result<_>>()?;
                sources.push(Source {
                    name: flag.name.clone(),
                    places,
                    item: flag.word(),
                    gives: Gives::Flag,
                    need: book.need(&flag.expr, ctx)?,
                });
            }
        }
        let goal = goal::exits(book, ctx, scan, &at)?;
        Ok(Self {
            places,
            edges,
            locations,
            sources,
            obstacles: scan.obstacles.clone(),
            goal,
            start: start_of(ctx.age),
        })
    }

    pub fn start(&self) -> usize {
        self.places
            .iter()
            .position(|p| *p == self.start)
            .unwrap_or(0)
    }

    /// Every item actor any rule here mentions.
    pub fn items(&self) -> BTreeSet<&str> {
        let mut out = BTreeSet::new();
        let edges = self.edges.iter().map(|e| &e.need);
        let checks = self.locations.iter().map(|l| &l.need);
        let sources = self.sources.iter().map(|s| &s.need);
        edges
            .chain(checks)
            .chain(sources)
            .for_each(|n| n.items(&mut out));
        out
    }

    /// Grow `seen` to every place reachable with `have`.
    fn grow(&self, seen: &mut [bool], have: &Have) {
        let mut grew = true;
        while grew {
            grew = false;
            let mut era_seen = [false; 2];
            for (i, s) in seen.iter().enumerate() {
                if *s {
                    era_seen[self.places[i].1 as usize] = true;
                }
            }
            for e in &self.edges {
                if seen[e.to] {
                    continue;
                }
                let (from, era) = match e.from {
                    Some(f) => (seen[f], self.places[f].1),
                    None => {
                        let era = self.places[e.to].1;
                        (era_seen[era as usize], era)
                    }
                };
                if from && e.need.met(have, era) {
                    seen[e.to] = true;
                    grew = true;
                }
            }
        }
    }

    /// Collect everything reachable from `START`, starting with `start`,
    /// with `items` at the checks that hold them, round by round.
    pub fn play(&self, start: &Have, items: &HashMap<u64, &str>) -> Play {
        let mut have = start.clone();
        let mut seen = vec![false; self.places.len()];
        seen[self.start()] = true;
        let (mut reached, mut sourced) = (HashSet::new(), vec![false; self.sources.len()]);
        let mut rounds = Vec::new();
        loop {
            self.grow(&mut seen, &have);
            let mut round = Vec::new();
            for l in &self.locations {
                let era = self.places[l.place].1;
                if seen[l.place] && !reached.contains(&l.hash) && l.need.met(&have, era) {
                    reached.insert(l.hash);
                    round.push(Got::Check(l.hash));
                }
            }
            for (i, s) in self.sources.iter().enumerate() {
                let here = |&p: &usize| seen[p] && s.need.met(&have, self.places[p].1);
                if !sourced[i] && s.places.iter().any(here) {
                    sourced[i] = true;
                    round.push(Got::Source(i));
                }
            }
            if round.is_empty() {
                break;
            }
            for got in &round {
                match got {
                    Got::Check(h) => items.get(h).into_iter().for_each(|i| have.add(i)),
                    Got::Source(i) => {
                        let s = &self.sources[*i];
                        match s.gives {
                            Gives::Item => have.add(&s.item),
                            Gives::Flag => drop(have.flags.insert(s.item.clone())),
                        }
                    }
                }
            }
            rounds.push(round);
        }
        Play {
            have,
            rounds,
            reached,
        }
    }

    /// Which places are reachable with `have`, without collecting anything.
    pub(crate) fn seen(&self, have: &Have) -> Vec<bool> {
        let mut seen = vec![false; self.places.len()];
        seen[self.start()] = true;
        self.grow(&mut seen, have);
        seen
    }

    /// Every place reachable with `have`, without collecting anything.
    pub fn reach(&self, have: &Have) -> BTreeSet<&Place> {
        let reached = self.places.iter().zip(self.seen(have)).filter(|(_, s)| *s);
        reached.map(|(p, _)| p).collect()
    }

    /// Whether only a child (`Some(true)`) or only an adult can get the
    /// check at `hash`: the eras of its copies whose rule anything could
    /// meet there. `None` when both can, or logic can't tell.
    pub fn only_child(&self, hash: u64) -> Option<bool> {
        let eras = self.eras(hash, Need::possible)?;
        match eras.len() {
            1 => eras.first().map(|e| *e == Era::Child),
            _ => None,
        }
    }

    /// Whether logic has the check at `hash` but no item or flag gets
    /// Link to it at either age, like a child's copy that needs the Iron
    /// Boots.
    pub fn never(&self, hash: u64) -> bool {
        self.eras(hash, Need::ever)
            .is_some_and(|eras| eras.is_empty())
    }

    /// The eras whose copy of the check at `hash` has a rule `could`
    /// meet. `None` when logic has no copy of it.
    fn eras(&self, hash: u64, could: fn(&Need, Era) -> bool) -> Option<BTreeSet<Era>> {
        let mut copies = (self.locations.iter())
            .filter(|l| l.hash == hash)
            .map(|l| (l, self.places[l.place].1))
            .peekable();
        copies.peek()?;
        Some(
            copies
                .filter(|(l, era)| could(&l.need, *era))
                .map(|(_, era)| era)
                .collect(),
        )
    }
}

/// `rule` with each `switch:<actor>` swapped for what the actor's pack
/// says hits it, or `always`.
fn switches(rule: &str, tools: &mut Tools) -> Result<String> {
    let spaced = rule.replace('(', " ( ").replace(')', " ) ");
    let mut out = Vec::new();
    for word in spaced.split_whitespace() {
        out.push(match word.strip_prefix("switch:") {
            Some(actor) => match tools.of_actor(actor)? {
                Some(tool) => format!("({tool})"),
                None => "always".into(),
            },
            None => word.to_owned(),
        });
    }
    Ok(out.join(" "))
}

#[cfg(test)]
pub(super) mod tests {
    use super::exits::tests::shuffled;
    use super::*;
    use crate::entrances::Doors;
    use crate::known::{actor, map};

    #[test]
    fn a_room_flag_is_held_once_its_room_is_reached() {
        let place: Place = (map::INSIDE_THE_DEKU_TREE.get().to_owned(), Era::Child);
        let flag = "#1a2b3c4d";
        let world = World {
            places: vec![place.clone()],
            edges: vec![],
            locations: vec![Location {
                hash: 1,
                place: 0,
                need: Need::Flag(flag.into()),
            }],
            sources: vec![Source {
                name: "Fake Room Unlocked".into(),
                places: vec![0],
                item: flag.into(),
                gives: Gives::Flag,
                need: Need::Always,
            }],
            obstacles: BTreeMap::new(),
            goal: vec![],
            start: place,
        };
        let play = world.play(&Have::default(), &HashMap::new());
        assert!(play.reached.contains(&1));
        assert!(play.have.flags.contains(flag) && play.have.items.is_empty());
        assert_eq!(play.rounds.len(), 2, "the flag first, then the check");
    }

    #[test]
    fn a_dealt_check_takes_over_from_its_source() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let sources = |on: &[&str]| {
            let new_game = crate::settings::NewGame {
                on: on.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            };
            let checks = checks::dealt(&romfs, &new_game).unwrap();
            let ctx = Ctx::new(&new_game);
            let world = World::build(&romfs, &book, &ctx, &checks).unwrap();
            world
                .sources
                .into_iter()
                .map(|s| s.name)
                .collect::<Vec<_>>()
        };
        let off = sources(&[]);
        let on = sources(&[crate::settings::NPC_GIFTS]);
        for name in ["Light Arrows", "Fire Arrows", "Ruto's Letter", "Weird Egg"] {
            assert!(off.iter().any(|n| n == name), "{name} without the gifts");
            assert!(!on.iter().any(|n| n == name), "{name} with the gifts");
        }
        // Dampe's race is a check whatever the settings, so the Hookshot is
        // never a source.
        assert!(!off.iter().chain(&on).any(|n| n == "Hookshot"));
        // The bean seller still sells beans after his first sale.
        let sold = sources(&[crate::settings::MERCHANTS]);
        for list in [&off, &on, &sold] {
            assert!(list.iter().any(|n| n == "Magic Beans"));
        }
        // The trade chain's Biggoron check gives the Claim Check, and
        // every trade check has a place.
        let traded = sources(&[crate::settings::NPC_GIFTS, crate::settings::TRADE_CHAIN]);
        assert!(on.iter().any(|n| n == "Claim Check"));
        assert!(!traded.iter().any(|n| n == "Claim Check"));
    }

    #[test]
    fn a_check_only_one_age_can_get_names_that_age() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame::default();
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        // The Water Temple's fourth small key chest, underwater: Iron Boots.
        assert_eq!(world.only_child(3693930347660122883), Some(false));
        assert_eq!(world.only_child(1), None, "no such check");
    }

    #[test]
    fn a_check_no_age_can_get_is_never() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame {
            crates: true,
            ..Default::default()
        };
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        // Hyrule Field's grotto pond crates: the child's copy needs the
        // Iron Boots, the adult's gets them.
        assert!(world.never(13505816572168486267));
        assert!(!world.never(13942975907881711341));
        assert!(!world.never(1), "no such check");
    }

    #[test]
    fn scrubs_sell_only_once_stunned() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame {
            scrubs: true,
            ..Default::default()
        };
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        let need = |hash| {
            let at = world.locations.iter().filter(|l| l.hash == hash);
            at.map(|l| l.need.to_string()).collect::<Vec<_>>()
        };
        let scrubs = scrubs::SCRUBS.map(|s| s.hash());
        for (hash, needs) in scrubs.iter().map(|&h| (h, need(h))) {
            assert!(!needs.is_empty(), "scrub {hash} has no location");
            assert!(
                needs.iter().all(|n| n.contains(item::SWORD_KOKIRI.get())),
                "{hash}: {needs:?}"
            );
        }
        // The Lost Woods rocks seal one grotto, not the scrubs in the open.
        let bags = item::BOMB_BAG_SMALL.stem();
        let bridge = need(5406759280757349546);
        assert!(bridge.iter().all(|n| !n.contains(bags)), "{bridge:?}");
        let grotto = need(2214083465643671502);
        assert!(grotto.iter().all(|n| n.contains(bags)), "{grotto:?}");
    }

    #[test]
    fn a_boss_counts_for_either_era_while_doors_shuffle() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let barinade = |world: &World| {
            let beat = crate::rules::vocab::id("beat:barinade").unwrap();
            let source = world.sources.iter().find(|s| s.item == beat).unwrap();
            let mut eras: Vec<Era> = source.places.iter().map(|&p| world.places[p].1).collect();
            eras.sort();
            eras
        };
        let new_game = crate::settings::NewGame::default();
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let book = crate::rules::Book::load().unwrap();
        let off = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        assert_eq!(barinade(&off), [Era::Child]);
        let on = shuffled(&romfs, Doors::default(), &[]);
        assert_eq!(barinade(&on), [Era::Child, Era::Adult]);
    }

    #[test]
    fn gohma_counts_for_either_era() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let world = world(&romfs, &[]);
        let beat = crate::rules::vocab::id("beat:gohma").unwrap();
        let source = world.sources.iter().find(|s| s.item == beat).unwrap();
        let mut eras: Vec<Era> = source.places.iter().map(|&p| world.places[p].1).collect();
        eras.sort();
        assert_eq!(eras, [Era::Child, Era::Adult]);
    }

    #[test]
    fn burning_kakariko_waits_for_three_sages() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame::default();
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        let burning = (
            map::KAKARIKO_VILLAGE_ADULT_ALTERNATIVE.to_string(),
            Era::Adult,
        );
        let beat = |words: &[&str]| {
            let mut have = Have::default();
            for w in words {
                have.add(&crate::rules::vocab::id(&format!("beat:{w}")).unwrap());
            }
            have
        };
        let edges = world.edges.iter().filter(|e| world.places[e.to] == burning);
        let (mut two, mut three) = (true, false);
        for e in edges {
            two &= !e
                .need
                .met(&beat(&["phantom-ganon", "volvagia"]), Era::Adult);
            three |= e
                .need
                .met(&beat(&["phantom-ganon", "volvagia", "morpha"]), Era::Adult);
        }
        assert!(two, "burning with two sages done");
        assert!(three, "never burning");
    }

    /// The vanilla world with `on` set, and a finder for its places.
    pub(super) fn world(romfs: &Romfs, on: &[&str]) -> World {
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame {
            on: on.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let checks = checks::dealt(romfs, &new_game).unwrap();
        World::build(romfs, &book, &Ctx::new(&new_game), &checks).unwrap()
    }

    #[test]
    fn an_adult_start_reaches_what_a_child_start_does() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let reached = |pick: (&str, serde_json::Value)| {
            let pick = (pick.0.to_string(), pick.1);
            let settings = crate::settings::Settings::from([pick]);
            let new_game = crate::settings::NewGame::from_settings(&settings, &[]).unwrap();
            let checks = checks::dealt(&romfs, &new_game).unwrap();
            let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
            let vanilla = checks.iter().map(|c| (c.hash, c.item.as_str())).collect();
            let play = world.play(&crate::fill::start(&new_game), &vanilla);
            (world.places[world.start()].clone(), play.reached)
        };
        let (start, adult) = reached((crate::settings::STARTING_AGE, "adult".into()));
        assert_eq!(start, start_of(Era::Adult));
        assert_eq!(start.0, map::MARKET_ADULT.get());
        // Adult forces open forest, so the child start gets it too.
        let (start, child) = reached((crate::presets::OPEN_FOREST, true.into()));
        assert_eq!(start, start_of(Era::Child));
        assert!(!adult.is_empty());
        assert_eq!(adult, child);
    }

    #[test]
    fn only_a_shuffled_master_sword_can_keep_adult_bosses_out_of_reach() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let pedestal = crate::known::hash(crate::known::flow::DOOR_OF_TIME.get());
        let reached = |on: &[&str], swap: Option<&str>| {
            let new_game = crate::settings::NewGame {
                on: on.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            };
            let checks = checks::dealt(&romfs, &new_game).unwrap();
            let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
            let mut placed: HashMap<u64, &str> =
                checks.iter().map(|c| (c.hash, c.item.as_str())).collect();
            if let Some(item) = swap {
                assert!(
                    placed.insert(pedestal, item).is_some(),
                    "the pedestal is dealt"
                );
            }
            world.play(&crate::fill::start(&new_game), &placed).reached
        };
        let (door, forest) = (crate::settings::DOOR_OPEN, crate::presets::OPEN_FOREST);
        let shuffle = crate::settings::MASTER_SWORD;
        let off = reached(&[door, forest], None);
        // Dealt its own sword, the pedestal adds itself and nothing more.
        assert_eq!(reached(&[door, forest, shuffle], None).len(), off.len() + 1);
        let green = Some(actor::OBJ_RUPEE_GREEN.get());
        let swordless = reached(&[door, forest, shuffle], green);
        assert!(
            swordless.len() < off.len(),
            "{} of {}",
            swordless.len(),
            off.len()
        );
    }

    #[test]
    fn links_pocket_sits_at_the_start_with_the_door_open() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rauru = |on: &[&str]| {
            let world = world(&romfs, on);
            let hash = gifts::pocket(&[crate::settings::DOOR_OPEN.to_string()].into()).unwrap();
            let at = |l: &Location| world.places[l.place].clone();
            (world.locations.iter())
                .filter(|l| l.hash == hash)
                .map(at)
                .collect::<Vec<_>>()
        };
        let rewards = crate::settings::DUNGEON_REWARDS;
        let open = rauru(&[rewards, crate::settings::DOOR_OPEN]);
        assert_eq!(open, [start_of(Era::Child)]);
        let closed = rauru(&[rewards]);
        assert_eq!(closed, [(map::MARKET_ADULT.to_string(), Era::Adult)]);
    }

    /// The edges from `from` to `to`, both as adult Link.
    pub(super) fn adult_edges<'a>(world: &'a World, from: &Name, to: &Name) -> Vec<&'a Edge> {
        let at =
            |node: &Name| (world.places.iter()).position(|p| p == &(node.to_string(), Era::Adult));
        let (from, to) = (at(from), at(to));
        (world.edges.iter())
            .filter(|e| e.from == from && Some(e.to) == to)
            .collect()
    }

    #[test]
    fn adult_link_walks_into_the_deku_tree_only_with_its_setting() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let way = |on: &[&str]| {
            let world = world(&romfs, on);
            let edges = adult_edges(&world, map::KOKIRI_FOREST_ADULT, map::INSIDE_THE_DEKU_TREE);
            edges
                .iter()
                .any(|e| e.need.met(&Have::default(), Era::Adult))
        };
        assert!(way(&[crate::presets::ADULT_DEKU_TREE]));
        assert!(!way(&[]));
    }

    #[test]
    fn adult_link_opens_gohmas_door_with_the_hylian_shield_and_bow() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let world = world(&romfs, &[]);
        let doors = adult_edges(&world, map::INSIDE_THE_DEKU_TREE, map::GOHMA_STAGE);
        let mut have = Have::default();
        for item in [
            "item:hylian-shield-chest",
            "item:dins-fire",
            "item:magic-meter",
            "item:master-sword",
        ] {
            have.add(&crate::rules::vocab::id(item).unwrap());
        }
        assert!(!doors.iter().any(|d| d.need.met(&have, Era::Adult)));
        have.add(&crate::rules::vocab::id("item:bow").unwrap());
        assert!(doors.iter().any(|d| d.need.met(&have, Era::Adult)));
    }

    #[test]
    fn gerudo_fortress_fast_gives_the_card_without_the_hookshot() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let fast = crate::presets::CARPENTERS_FAST;
        let card = crate::gerudo::CARD.get();
        let source = |on: &[&str]| {
            let world = world(&romfs, on);
            let s = world.sources.iter().find(|s| s.item == card).unwrap();
            s.need.met(&Have::default(), Era::Adult)
        };
        assert!(source(&[fast]));
        assert!(!source(&[]));
    }

    #[test]
    fn switches_take_their_tool() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut tools = Tools::new(&romfs);
        assert_eq!(
            switches(
                &format!("adult and switch:{}", actor::OBJ_STEP_SWITCH_A_01),
                &mut tools
            )
            .unwrap(),
            "adult and always"
        );
        let eye = switches(
            &format!("switch:{} or fire", actor::OBJ_EYE_SWITCH_A_02),
            &mut tools,
        )
        .unwrap();
        assert!(eye.starts_with("(slingshot or bow or boomerang"), "{eye}");
        assert!(eye.ends_with(") or fire"), "{eye}");
    }
}
