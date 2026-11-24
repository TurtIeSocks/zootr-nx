//! The world graph: places (a Banc node in one era), the exits between
//! them, and what each check and vanilla source needs, resolved for one
//! seed's settings from the rule files, the logic graphs and the obstacles.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use crate::checks::{self, ItemCheck, actors, bancs, hash_of, str_field};
use crate::entrances::DUNGEONS;
use crate::known::{Name, flag, item, map};
use crate::rules::{Book, Ctx, Have, Need};
use crate::tools::Tools;
use crate::world::{self, Era, node_of};
use crate::{cows, gates, gifts, keys, shops};

mod exits;
mod goal;
mod locations;
mod play;
mod rooms;

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
    start: usize,
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
#[derive(Clone)]
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
#[derive(Clone)]
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
        let (shops, cows) = kinds(checks);
        Self::read_kinds(romfs, shops, cows)
    }

    /// The data for any checks, for many seeds to share through
    /// [`Data::only`].
    pub fn every(romfs: &'a Romfs) -> Result<Self> {
        Self::read_kinds(romfs, true, true)
    }

    /// This data as [`Data::read`] gives it for `checks`.
    pub fn only(&self, checks: &[ItemCheck]) -> Self {
        let mut out = self.clone();
        let (shops, cows) = kinds(checks);
        if !shops {
            out.shop_slots.clear();
        }
        if !cows {
            out.cows.clear();
        }
        out
    }

    fn read_kinds(romfs: &'a Romfs, shops: bool, cows: bool) -> Result<Self> {
        let mut tools = Tools::new(romfs);
        let scan = scan(romfs, &mut tools)?;
        Ok(Self {
            tools,
            scan,
            alternatives: world::alternatives(romfs)?,
            exits: world::exits(romfs)?,
            gates: gates::gates(romfs)?,
            shop_slots: match shops {
                true => shops::slots(romfs)?,
                false => Vec::new(),
            },
            cows: match cows {
                true => cows::find(romfs)?,
                false => Vec::new(),
            },
        })
    }
}

/// Whether `checks` hold a shop's and a cow's, whose data the world reads
/// only then.
fn kinds(checks: &[ItemCheck]) -> (bool, bool) {
    (
        checks.iter().any(shops::is_shop),
        checks.iter().any(cows::is_cow),
    )
}

/// The eras a node's places come in.
fn eras_of(places: &BTreeSet<Place>, node: &str) -> Vec<Era> {
    places.iter().filter(|p| p.0 == node).map(|p| p.1).collect()
}

/// Every era of `node`'s places, under `name`.
fn places_of(places: &BTreeSet<Place>, node: &str, name: &str) -> Vec<Place> {
    (eras_of(places, node).into_iter())
        .map(|e| (name.to_owned(), e))
        .collect()
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
        let rooms = rooms::RoomPlaces::new(&book.rooms, exits)?;
        let mut places: Vec<Place> = scan.places.iter().cloned().collect();
        for (room, node) in rooms.added() {
            places.extend(places_of(&scan.places, node, &room));
        }
        let b = Build {
            book,
            ctx,
            scan,
            index: (places.iter().enumerate())
                .map(|(i, p)| (p.clone(), i))
                .collect(),
            boss_rooms: boss_rooms(ctx),
            gates,
            shop_slots,
            cows,
            rooms,
        };
        let at = |node: &str, era: Era| b.at(node, era);
        let mut edges = b.warp_edges()?;
        edges.extend(exits::edges(
            book,
            ctx,
            scan,
            alternatives,
            exits,
            &b.rooms,
            &at,
        )?);
        edges.extend(b.room_edges()?);
        let locations = b.locations(checks, tools)?;
        let mut sources = b.sources(checks)?;
        sources.extend(b.room_flags()?);
        let goal = goal::exits(book, ctx, scan, &at)?;
        let (node, era) = start_of(ctx.age);
        let start = b.at(&node, era)?;
        Ok(Self {
            places,
            edges,
            locations,
            sources,
            obstacles: scan.obstacles.clone(),
            goal,
            start,
        })
    }

    pub fn start(&self) -> usize {
        self.start
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
}

/// A boss room counts for either era once doors move, since its
/// dungeon may stand behind the other era's door. Gohma's always does:
/// adult Link reaches her only through a door that asks for the adult
/// Deku Tree, and both eras' rules inside are audited. The blue warp
/// keeps Link's age (`entrances.md`, "Blue warps keep Link's age").
fn boss_rooms(ctx: &Ctx) -> HashSet<&'static str> {
    DUNGEONS
        .iter()
        .filter(|d| ctx.doors.is_some() || d.node == map::INSIDE_THE_DEKU_TREE)
        .filter_map(|d| d.boss)
        .map(|b| &**b)
        .collect()
}

/// What the world's parts read as it's built for one seed.
struct Build<'a> {
    book: &'a Book,
    ctx: &'a Ctx,
    scan: &'a Scan,
    rooms: rooms::RoomPlaces,
    /// Each place's index in the world's places.
    index: HashMap<Place, usize>,
    /// The boss rooms that count for either era ([`boss_rooms`]).
    boss_rooms: HashSet<&'static str>,
    gates: &'a BTreeMap<u64, String>,
    shop_slots: &'a [shops::Slot],
    cows: &'a [cows::Cow],
}

impl Build<'_> {
    fn at(&self, node: &str, era: Era) -> Result<usize> {
        (self.index.get(&(node.to_owned(), era)).copied())
            .with_context(|| format!("no place {node} {era}"))
    }

    fn resolve(&self, text: &str) -> Result<Need> {
        self.book.resolve(text, self.ctx)
    }

    /// The eras a thing in `node` at `era` counts for: a boss room's every
    /// era.
    fn pinned(&self, node: &str, era: Era) -> Vec<Era> {
        match self.boss_rooms.contains(node) {
            true => eras_of(&self.scan.places, node),
            false => vec![era],
        }
    }

    /// Each warp song's landing, from anywhere in its era.
    fn warp_edges(&self) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        for (node, song, era) in &self.scan.warps {
            edges.push(Edge {
                from: None,
                to: self.at(node, *era)?,
                need: self.resolve(&format!(
                    "ocarina and {}{song}",
                    item::MELODY_SPIRIT_REQUIEM.stem()
                ))?,
            });
        }
        Ok(edges)
    }

    /// A live dungeon's rooms, joined by their links.
    fn room_edges(&self) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        for room in self.book.rooms.iter().filter(|r| r.live) {
            let from = self.rooms.node(&room.node, room.file.room);
            for link in &room.file.link {
                let need = self.book.need(&link.expr, self.ctx)?;
                let to = self.rooms.node(&room.node, link.to);
                for era in eras_of(&self.scan.places, &room.node) {
                    edges.push(Edge {
                        from: Some(self.at(&from, era)?),
                        to: self.at(&to, era)?,
                        need: need.clone(),
                    });
                }
            }
        }
        Ok(edges)
    }

    /// The rules' sources, less those a dealt check stands for or a shop
    /// sells.
    fn sources(&self, checks: &[ItemCheck]) -> Result<Vec<Source>> {
        let (book, ctx) = (self.book, self.ctx);
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
            let places = (self.pinned(&s.place, s.era).into_iter())
                .map(|era| (self.at(&s.place, era)).with_context(|| format!("source {}", s.name)))
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
        Ok(sources)
    }

    /// A room's flag, set once Link reaches its room and meets its needs.
    fn room_flags(&self) -> Result<Vec<Source>> {
        let mut sources = Vec::new();
        for room in self.book.rooms.iter().filter(|r| r.live) {
            for flag in &room.file.flag {
                let node = self.rooms.node(&room.node, room.file.room);
                let places = (eras_of(&self.scan.places, &room.node).into_iter())
                    .map(|era| self.at(&node, era))
                    .collect::<Result<_>>()?;
                sources.push(Source {
                    name: flag.name.clone(),
                    places,
                    item: flag.word(),
                    gives: Gives::Flag,
                    need: self.book.need(&flag.expr, self.ctx)?,
                });
            }
        }
        Ok(sources)
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
    use crate::scrubs;

    #[test]
    fn a_dealt_check_takes_over_from_its_source() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let sources = |on: &[&str]| {
            let world = world(&romfs, on);
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
        // The trade chain's Biggoron check gives the Claim Check. The world
        // builds, so every trade check has a place.
        let traded = sources(&[crate::settings::NPC_GIFTS, crate::settings::TRADE_CHAIN]);
        assert!(on.iter().any(|n| n == "Claim Check"));
        assert!(!traded.iter().any(|n| n == "Claim Check"));
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
        let off = world(&romfs, &[]);
        assert_eq!(beaten_in(&off, "barinade"), [Era::Child]);
        let on = shuffled(&romfs, Doors::default(), &[]);
        assert_eq!(beaten_in(&on, "barinade"), [Era::Child, Era::Adult]);
    }

    /// The eras `boss` can be beaten in, sorted.
    fn beaten_in(world: &World, boss: &str) -> Vec<Era> {
        let beat = crate::rules::vocab::id(&format!("beat:{boss}")).unwrap();
        let source = world.sources.iter().find(|s| s.item == beat).unwrap();
        let mut eras: Vec<Era> = source.places.iter().map(|&p| world.places[p].1).collect();
        eras.sort();
        eras
    }

    #[test]
    fn gohma_counts_for_either_era() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let world = world(&romfs, &[]);
        assert_eq!(beaten_in(&world, "gohma"), [Era::Child, Era::Adult]);
    }

    #[test]
    fn burning_kakariko_waits_for_three_sages() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let world = world(&romfs, &[]);
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
            let settings =
                crate::settings::favorites::randomizer_and(crate::settings::Settings::from([pick]));
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
            let pocket = gifts::pocket(&[crate::settings::DOOR_OPEN.to_string()].into());
            let hash = pocket.into_iter().next().unwrap();
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
    pub(super) fn adult_edges<'a>(
        world: &'a World,
        from: &impl ToString,
        to: &impl ToString,
    ) -> Vec<&'a Edge> {
        let at =
            |node: String| (world.places.iter()).position(|p| p.0 == node && p.1 == Era::Adult);
        let (from, to) = (at(from.to_string()), at(to.to_string()));
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
        // The Deku Tree is live, so its door leaves from Gohma's room.
        let room = format!("{}#10", map::INSIDE_THE_DEKU_TREE);
        let doors = adult_edges(&world, &room, map::GOHMA_STAGE);
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
        let fast = crate::settings::FORTRESS_FAST;
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
    fn met_zelda_gives_her_three_checks_at_the_start_and_wakes_talon() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        use crate::settings::{MET_ZELDA, NPC_GIFTS, SONGS};
        let on = [MET_ZELDA, NPC_GIFTS, SONGS];
        let met = world(&romfs, &on);
        let pocket = gifts::pocket(&on.map(str::to_owned).into());
        assert_eq!(pocket.len(), 3);
        for hash in pocket {
            let at: Vec<_> = (met.locations.iter())
                .filter(|l| l.hash == hash)
                .map(|l| {
                    (
                        met.places[l.place].clone(),
                        l.need.met(&Have::default(), Era::Child),
                    )
                })
                .collect();
            assert_eq!(at, [(start_of(Era::Child), true)], "{hash}");
        }
        // Talon's cuccos and the courtyard need no egg under Met Zelda.
        const TALON: u64 = 14014730143884270499;
        let talon = |world: &World| {
            let l = world.locations.iter().find(|l| l.hash == TALON).unwrap();
            l.need.met(&Have::default(), Era::Child)
        };
        assert!(talon(&met));
        assert!(!talon(&world(&romfs, &[NPC_GIFTS, SONGS])));
    }

    #[test]
    fn gerudo_fortress_freed_gives_the_card_at_the_start() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        use crate::settings::{FORTRESS_FREED, NPC_GIFTS};
        let on = [FORTRESS_FREED, NPC_GIFTS];
        let freed = world(&romfs, &on);
        let card = gifts::pocket(&on.map(str::to_owned).into());
        let card = card.into_iter().next().unwrap();
        let at: Vec<_> = (freed.locations.iter())
            .filter(|l| l.hash == card)
            .map(|l| {
                (
                    freed.places[l.place].clone(),
                    l.need.met(&Have::default(), Era::Child),
                )
            })
            .collect();
        assert_eq!(at, [(start_of(Era::Child), true)]);
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
