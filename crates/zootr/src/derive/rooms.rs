//! A dungeon's rooms from its navmesh: each face takes the room of the
//! nearest placement that names one (`DefaultDungeonRoomIndex`), by walking
//! distance, so a room never reaches across a wall or through a door. The
//! arcs between rooms become each room file's links.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::{Romfs, murmur3};

use crate::checks::ItemCheck;
use crate::entrances::Dungeon;
use crate::known::{Name, actor, field, flag};
use crate::nav::Level;
use crate::nav::graph::{Cross, Hop, Via, dist};
use crate::nav::limits::limits;
use crate::nav::shape::{Solid, parts_of};
use crate::rules::Book;
use crate::rules::kinds::{Kind, Sets};
use crate::rules::rooms::{self, folder};
use crate::warp::Placement;
use crate::world::NO_START_POS;

mod confirmed;
pub mod explain;
mod hand;
mod label;
mod locks;
mod reach;
mod render;
mod seen;

use hand::{Taken, TakenBy, in_front, is_chest};
use label::{label, path_of};
use locks::{Lock, locks_in, room_in_name};
use reach::{REACH_MAX, loose_reach, skulltula_reach};
use render::{CheckOut, Traced, room_files};

/// A straight path between two points, or a link's two ends.
pub type Segment = ([f32; 3], [f32; 3]);

/// Each dungeon's room names, by its folder and room number.
static NAMES: LazyLock<BTreeMap<String, BTreeMap<u32, String>>> = LazyLock::new(|| {
    let text = include_str!("../../logic/room-names.toml");
    let raw: BTreeMap<String, BTreeMap<String, String>> =
        toml::from_str(text).expect("logic/room-names.toml");
    (raw.into_iter())
        .map(|(d, rooms)| {
            let rooms = rooms
                .into_iter()
                .map(|(n, name)| (n.parse().expect("a room number"), name));
            (d, rooms.collect())
        })
        .collect()
});

/// The file in `logic/` the names come from, for the built-in files test.
#[cfg(test)]
pub(crate) const NAMES_FILE: &str = "room-names.toml";

/// How far from the floor an exit's trigger box may sit and still take
/// its room, in metres.
const NEAR: f32 = 6.0;

/// How far from the floor a holder whose placement names no room may sit
/// and still take the nearest floor's room, in metres: the Forest Temple's
/// Boss Key chest is about 7 m from Wallmaster Hall's.
const HOLDER_NEAR: f32 = 10.0;

/// The logic node that drives a shutter's lattice.
const SHUTTER: &str = "ActorLogicShutter";

/// How far round an obstacle's collision a path still meets it, in m.
const ON: f32 = 0.5;
/// How near an obstacle stands to a locked one to share its locks, in m: a
/// Deku shutter's leaves stand 0.4 m out from its frame.
const WITH: f32 = 1.0;

/// Whether the straight path from `a` to `b` passes through `solid`.
fn meets(solid: &Solid, a: [f32; 3], b: [f32; 3]) -> bool {
    // Exactly, not by samples: a Deku shutter is 0.11 m thick, and the
    // collision floors' small triangles make short paths that step over it.
    let Some((t0, t1)) = solid.over([a[0], a[2]], [b[0], b[2]]) else {
        return false;
    };
    let y = |t: f32| a[1] + (b[1] - a[1]) * t;
    let (lo, hi) = (y(t0).min(y(t1)), y(t0).max(y(t1)));
    lo <= solid.high + ON && hi >= solid.low - ON
}

fn via_of(via: Via) -> (rooms::Via, Option<u64>) {
    match via {
        Via::Walk => (rooms::Via::Walk, None),
        Via::Door(h) => (rooms::Via::Door, h),
        Via::Drop => (rooms::Via::Drop, None),
        Via::Step => (rooms::Via::Step, None),
        Via::Climb(h) => (rooms::Via::Climb, h),
        Via::Crawl(h) => (rooms::Via::Crawl, h),
        Via::Swim(h) => (rooms::Via::Swim, h),
    }
}

/// One dungeon's room files, by path under `logic/`, and what derive
/// couldn't place.
pub struct Derived {
    pub files: Vec<(String, String)>,
    pub faces: usize,
    /// Faces no named placement reaches.
    pub unplaced: usize,
    /// Rooms the game numbers that got no floor.
    pub floorless: Vec<u32>,
    /// Checks in no room, by hash.
    pub roomless: Vec<u64>,
    /// Exits in no room, by name in words.
    pub exitless: Vec<String>,
    /// Each jump link, at its narrowest: where a playtest settles it.
    pub jumps: Vec<Jump>,
    /// What a confirmed entry and the level disagree on, short of an error.
    pub warnings: Vec<String>,
}

/// A jump link at its narrowest gap.
pub struct Jump {
    pub from: u32,
    pub to: u32,
    pub width: f32,
    pub at: Segment,
}

/// The maps of `dungeon`: its own, by either age, never its boss's.
fn maps(romfs: &Romfs, dungeon: &Dungeon) -> Result<BTreeSet<String>> {
    let node = dungeon.node.get();
    let files = crate::checks::banc_files(romfs)?;
    Ok((files.iter())
        .map(|rel| crate::warp::map_of(rel))
        .filter(|m| m.starts_with(node) && dungeon.boss.is_none_or(|b| !m.starts_with(b.get())))
        .map(str::to_owned)
        .collect())
}

/// `uses` joined: each kept once, more than one in brackets with `and`.
fn all_of(uses: &[&str]) -> String {
    let mut kept: Vec<&str> = vec![];
    for u in uses.iter().filter(|u| **u != super::ALWAYS) {
        if !kept.contains(u) {
            kept.push(u);
        }
    }
    match &kept[..] {
        [] => super::ALWAYS.to_owned(),
        [one] => (*one).to_owned(),
        many => many
            .iter()
            .map(|n| format!("({n})"))
            .collect::<Vec<_>>()
            .join(" and "),
    }
}

/// Each kind in `book` by its name's hash.
fn kinds_by_hash(book: &Book) -> HashMap<u32, &Kind> {
    book.kinds.iter().map(|k| (k.hash, k)).collect()
}

/// One map as derive reads it: its level, the rooms its placements
/// number, and the obstacles placed in it with their collision.
struct Floor<'k> {
    pub level: Level,
    pub rooms_of: HashMap<u64, u32>,
    pub obstacles: Vec<(u64, &'k Kind, Vec<Solid>)>,
}

impl<'k> Floor<'k> {
    /// `map` as a child: how deep he wades, his crawls. The spec's adult
    /// pass comes later, with age in `needs`.
    pub fn of(
        romfs: &Romfs,
        game: &Path,
        map: &str,
        kinds: &HashMap<u32, &'k Kind>,
    ) -> Result<Self> {
        let mut level = Level::of(romfs, game, map)?;
        level.child = true;
        let rooms_of = crate::warp::rooms_in(romfs, map)?;
        let mut obstacles = vec![];
        for p in &level.placed {
            if let (Some(h), Some(k)) = (p.hash, kinds.get(&murmur3(p.name.as_bytes())))
                && k.sets == Sets::Opens
            {
                obstacles.push((h, *k, parts_of(romfs, p)?));
            }
        }
        // No collision floor under a shutter or a web: a doorway joins its
        // rooms through the navmesh, where the obstacle stands on the way.
        let under: Vec<Solid> = obstacles.iter().flat_map(|(_, _, s)| s.clone()).collect();
        let level = level.with_floors(romfs, &under)?;
        Ok(Self {
            level,
            rooms_of,
            obstacles,
        })
    }

    /// The obstacles standing on the way from `a` to `b`.
    pub fn on(&self, a: [f32; 3], b: [f32; 3]) -> Vec<(u64, &'k Kind)> {
        (self.obstacles.iter())
            .filter(|(_, _, solids)| solids.iter().any(|s| meets(s, a, b)))
            .map(|(h, k, _)| (*h, *k))
            .collect()
    }

    /// Each face's room, labelled over `graph`.
    pub fn labels(&self, graph: &crate::nav::graph::Graph) -> label::Labels {
        let first = self.level.floors.unwrap_or(usize::MAX);
        label(graph, &self.level.placed, &self.rooms_of, first, |a, b| {
            !self.on(a, b).is_empty()
        })
    }
}

/// A loose pickup's name in words, by what it is.
const LOOSE_NAMES: [(&Name, &str); 5] = [
    (actor::OBJ_RUPEE_GREEN, "Green Rupee"),
    (actor::OBJ_RUPEE_BLUE, "Blue Rupee"),
    (actor::OBJ_RUPEE_RED, "Red Rupee"),
    (actor::OBJ_RUPEE_PURPLE, "Purple Rupee"),
    (actor::OBJ_HEART, "Recovery Heart"),
];

/// Every check, gold skulltulas and loose rupees and hearts included, and
/// each one's name in words, but a gold skulltula's, which takes its
/// room's name.
fn checks_and_names(romfs: &Romfs) -> Result<(Vec<ItemCheck>, HashMap<u64, String>)> {
    let mut checks = crate::checks::all(romfs)?;
    // Gold skulltulas are pickups, which `all` leaves to their setting.
    let skulltulas = crate::pickups::SKULLTULAS.checks(romfs)?;
    let by_room: HashSet<u64> = skulltulas.iter().map(|c| c.hash).collect();
    checks.extend(skulltulas);
    // Named one at a time: an item Archipelago has no name for fails only its own.
    // Skulltulas take their room's name below, closer than Archipelago's.
    let named = (checks.iter())
        .filter(|c| !by_room.contains(&c.hash))
        .filter_map(|c| {
            crate::ap::names::locations(romfs, std::slice::from_ref(c))
                .ok()?
                .pop_first()
        })
        .collect::<HashMap<_, _>>();
    // Loose pickups, which no setting deals yet, still get a room rule.
    let loose = crate::pickups::LOOSE.checks(romfs)?;
    let mut named = named;
    for c in &loose {
        let name = LOOSE_NAMES.iter().find(|(a, _)| **a == *c.item);
        named.extend(name.map(|(_, n)| (c.hash, (*n).to_owned())));
    }
    checks.extend(loose);
    Ok((checks, named))
}

/// The room files of `dungeon`, from its navmesh, its placements, its
/// logic graphs and `book`'s kinds and check names.
pub fn derive(
    romfs: &Romfs,
    game: &Path,
    dungeon: &Dungeon,
    book: &Book,
    flagmap: &crate::flagmap::Graph,
) -> Result<Derived> {
    let kind_of = kinds_by_hash(book);
    let kind_at = |p: &Placement| kind_of.get(&murmur3(p.name.as_bytes())).copied();
    let jump = limits().level_jump();
    let shutters = crate::gates::trace_node(romfs, SHUTTER)?;
    let chest_gates = crate::gates::trace(romfs)?;
    let chest_gate = |h: u64| chest_gates.iter().find(|g| g.hash == h);
    let (checks, named) = checks_and_names(romfs)?;
    let game_data = crate::gamedata::GameData::load(romfs)?;
    let in_game = |h: u32| game_data.entry(h).is_some();
    let confirmed = confirmed::Confirmed::of(&book.rooms, dungeon.node.get(), in_game)?;
    let mut warnings = vec![];
    let mut traced = Traced::default();
    let mut roomless = vec![];
    let exits = crate::world::exits(romfs)?;
    let mut exitless = vec![];
    let mut numbered = BTreeSet::new();
    let mut floored = BTreeSet::new();
    let (mut faces, mut unplaced) = (0, 0);
    for map in maps(romfs, dungeon)? {
        let floor = Floor::of(romfs, game, &map, &kind_of)?;
        let (level, rooms_of, obstacles) = (&floor.level, &floor.rooms_of, &floor.obstacles);
        let graph = level.graph();
        numbered.extend(rooms_of.values().copied());
        let on = |a: [f32; 3], b: [f32; 3]| floor.on(a, b);
        let mut labels = floor.labels(&graph);
        confirmed.claim(&graph, &mut labels)?;
        let here = confirmed.locate(&graph, &labels)?;
        confirmed.place_flags(&graph, &labels)?;
        let room = &labels.room;
        faces += room.len();
        unplaced += room.iter().filter(|l| l.is_none()).count();
        floored.extend(room.iter().flatten().copied());
        let room_at = |p: &Placement| {
            let hash = p.hash?;
            (rooms_of.get(&hash).copied()).or_else(|| room[graph.locate(p.pose.at)?.face])
        };
        // A placement off the floor (an exit's trigger box) takes the room
        // of the nearest floor within `near` of it.
        let room_within = |p: &Placement, near: f32| {
            room_at(p).or_else(|| {
                (0..room.len())
                    .filter(|&f| room[f].is_some())
                    .map(|f| (dist(graph.centers[f], p.pose.at), f))
                    .filter(|(d, _)| *d < near)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .and_then(|(_, f)| room[f])
            })
        };
        let room_near = |p: &Placement| room_within(p, NEAR);
        let setter_rooms = |h: u32| {
            let played = flagmap.played_by(&game_data.bare(h)).into_iter();
            played
                .filter_map(|p| room_at(level.placement(p).ok()?))
                .collect()
        };
        confirmed.writers(setter_rooms, &mut warnings);
        let name_of = |to: &str, start: &str| match start {
            NO_START_POS => super::words(to),
            _ => format!("{} {}", super::words(to), super::words(start)),
        };
        let exit_placed = |label: &str| {
            (level.placed.iter())
                .filter(|p| p.name == field::FIELD_AND_DUNGEON_ENTRANCE_EVENT_REQUEST_ACTOR.get())
                .filter(|p| p.label.as_deref() == Some(label))
                .find_map(room_near)
        };
        for e in exits.iter().filter(|e| e.from == map) {
            let name = name_of(&e.to, &e.to_start);
            match e.name.as_deref().and_then(exit_placed) {
                Some(r) => drop(
                    traced
                        .spots
                        .entry((r, "exit"))
                        .or_default()
                        .insert(rooms::spot(&e.to, &e.to_start), name),
                ),
                None => exitless.push(name),
            }
        }
        // A boss exit the rule files add leaves from the boss's own room.
        let boss = (level.placed.iter())
            .find(|p| p.name.starts_with("Enemy") && p.name.contains("DungeonBoss"))
            .and_then(room_near);
        let added = (book.exits.iter()).filter(|r| r.added && r.from == map);
        for rule in added {
            let name = name_of(&rule.to, NO_START_POS);
            match boss.filter(|_| dungeon.boss.is_some_and(|b| b.get() == rule.to)) {
                Some(r) => drop(
                    traced
                        .spots
                        .entry((r, "exit"))
                        .or_default()
                        .insert(rooms::spot(&rule.to, NO_START_POS), name),
                ),
                None => exitless.push(name),
            }
        }
        let starts = (level.placed.iter()).filter(|p| p.name == flag::START_POS.get());
        for p in starts {
            if let (Some(label), Some(r)) = (&p.label, room_near(p)) {
                let at = traced.spots.entry((r, "arrival")).or_default();
                at.insert(rooms::spot(&map, label), name_of(&map, label));
            }
        }
        // The flag `name` and what sets it: the placements a graph names
        // (`setters`), or else those playing an event. Beating enemies or
        // playing an event clears the setters' room.
        let lock_of = |name: &str, mut setters: BTreeSet<u64>| -> (Lock, Vec<&Kind>) {
            let played = setters.is_empty();
            if played {
                let placed = flagmap.played_by(name).into_iter();
                setters.extend(placed.filter(|h| level.placement(*h).is_ok()));
            }
            let placed = || setters.iter().filter_map(|h| level.placement(*h).ok());
            let foes = !setters.is_empty()
                && placed().count() == setters.len()
                && placed().all(|p| p.name.starts_with(crate::enemies::ENEMY));
            let setter_room = placed().filter_map(room_at).next();
            let named = (setters.iter())
                .filter_map(|h| {
                    Some(format!(
                        "{} {h}",
                        super::words(&level.placement(*h).ok()?.name)
                    ))
                })
                .collect();
            let kinds: Vec<&Kind> = placed().filter_map(kind_at).collect();
            let lock = Lock {
                room: setter_room.or_else(|| room_in_name(name)),
                name: name.to_owned(),
                setters,
                named,
                clears: played || foes,
            };
            (lock, kinds)
        };
        // Each shutter's flags, and the rooms that set them.
        for gate in shutters.iter().filter(|g| level.placement(g.hash).is_ok()) {
            let mut found = BTreeMap::new();
            locks_in(&gate.expr, &mut found);
            for (name, setters) in found {
                let (lock, kinds) = lock_of(&name, setters);
                traced.flags.entry(name).or_insert((lock.clone(), kinds));
                traced.locks.entry(gate.hash).or_default().push(lock);
            }
        }
        // A shutter's leaves wait on its flags, but its frame carries none:
        // an obstacle with no locks of its own shares a locked one's that
        // stands with it.
        for (h, _, _) in obstacles {
            let Ok(p) = level.placement(*h) else { continue };
            if traced.locks.contains_key(h) {
                continue;
            }
            let shared: Vec<Lock> = (traced.locks.iter())
                .filter(|(g, _)| {
                    level
                        .placement(**g)
                        .is_ok_and(|q| dist(q.pose.at, p.pose.at) < WITH)
                })
                .flat_map(|(_, l)| l.iter().cloned())
                .collect();
            if !shared.is_empty() {
                traced.locks.insert(*h, shared);
            }
        }
        // Where Link enters each room: (room, part) of each arrival spot,
        // and of the floor each arc from another room lands on.
        let mut entries: HashSet<(u32, usize)> = HashSet::new();
        // Each arc between two parts of one room, and what stands on it.
        let mut crossings: Vec<hand::Crossing> = vec![];
        let cross = |a: usize, b: usize, path: Segment, via: rooms::Via| {
            let (pa, pb) = (labels.part[a], labels.part[b]);
            let own = pa != pb && room[a].is_some() && room[a] == room[b];
            (own && !here.takes_parts_arc(pa, pb, via)).then(|| hand::Crossing {
                from: pa,
                to: pb,
                on: on(path.0, path.1),
                crawl: via == rooms::Via::Crawl,
                puzzle: None,
            })
        };
        let starts = (level.placed.iter()).filter(|p| p.name == flag::START_POS.get());
        for p in starts {
            if let Some(snap) = graph.locate(p.pose.at)
                && let Some(r) = room[snap.face]
            {
                entries.insert((r, labels.part[snap.face]));
            }
        }
        let mut note = |a: usize,
                        b: usize,
                        via: rooms::Via,
                        metric: f32,
                        by: Option<u64>,
                        path: Segment,
                        fall: f32| {
            let (Some(ra), Some(rb)) = (room[a], room[b]) else {
                return;
            };
            if ra != rb && !here.takes_room_arc(ra, rb, via) {
                entries.insert((rb, labels.part[b]));
                let s = traced.seen.entry((ra, rb, via)).or_default();
                s.add(metric, by, path, level.on_terrain(a) || level.on_terrain(b));
                s.fall = s.fall.max(fall);
                s.on.extend(on(path.0, path.1));
            }
        };
        for (from, arcs) in graph.arcs.iter().enumerate() {
            for arc in arcs {
                let path = path_of(&graph, from, arc);
                let (via, by, metric) = match arc.cross {
                    Cross::Edge(_) => (rooms::Via::Walk, None, 0.0),
                    Cross::Link { via, from: f, to } => {
                        let (v, by) = via_of(via);
                        (v, by, (f[1] - to[1]).abs())
                    }
                };
                note(from, arc.to, via, metric, by, path, 0.0);
                crossings.extend(cross(from, arc.to, path, via));
            }
        }
        for Hop {
            from: a,
            to: b,
            leave: from,
            land: to,
        } in graph.plunges()
        {
            note(
                a,
                b,
                rooms::Via::Drop,
                from[1] - to[1],
                None,
                (from, to),
                0.0,
            );
            crossings.extend(cross(a, b, (from, to), rooms::Via::Drop));
        }
        // Level jumps fall nothing; a leap falls from its edge to its floor.
        let gaps = graph.gaps(jump).into_iter().map(|g| (g, false));
        for (hop, leap) in gaps.chain(graph.leaps().into_iter().map(|g| (g, true))) {
            let Hop {
                from: a,
                to: b,
                leave: from,
                land: to,
            } = hop;
            let width = (from[0] - to[0]).hypot(from[2] - to[2]);
            let fall = if leap { from[1] - to[1] } else { 0.0 };
            note(a, b, rooms::Via::Jump, width, None, (from, to), fall);
            crossings.extend(cross(a, b, (from, to), rooms::Via::Jump));
        }
        // The confirmed links, where the arcs they replace were.
        for p in &here.links {
            let ((ra, pa), (rb, pb)) = (p.from, p.to);
            if ra == rb {
                crossings.push(hand::Crossing {
                    from: pa,
                    to: pb,
                    on: vec![],
                    crawl: false,
                    puzzle: Some(p.link),
                });
                continue;
            }
            entries.insert((rb, pb));
            let s = traced.seen.entry((ra, rb, p.link.via)).or_default();
            s.add(0.0, None, p.at, false);
            s.puzzle = Some(p.link);
        }
        // A shutter's two leaves stand a strip of floor apart: its flag is
        // paid on the link into the room, so the floor past the second leaf
        // is entered too.
        // A confirmed link that needs nothing, settled by a playtest, enters
        // the part it leads to too.
        let opens: Vec<(usize, usize)> = (crossings.iter())
            .filter(|c| match c.puzzle {
                Some(p) => confirmed::free(p) && !p.guess,
                None => {
                    !c.crawl
                        && !c.on.is_empty()
                        && c.on.iter().all(|(_, k)| k.uses == super::ALWAYS)
                }
            })
            .map(|c| (c.from, c.to))
            .collect();
        // Each room's main part: the parts Link enters it by (rule 13). A
        // room nothing enters (the first one, with no arrival placed in
        // this map) falls back on the part with the most of its floor.
        let mut sizes: HashMap<(u32, usize), usize> = HashMap::new();
        for (f, r) in room.iter().enumerate() {
            if let Some(r) = r {
                *sizes.entry((*r, labels.part[f])).or_default() += 1;
            }
        }
        let largest = |r: u32| {
            (sizes.iter())
                .filter(|((sr, _), _)| *sr == r)
                .max_by_key(|(k, n)| (**n, std::cmp::Reverse(k.1)))
                .map(|((_, p), _)| *p)
        };
        let entries = past_shutters(entries, &opens);
        let entered: HashSet<u32> = entries.iter().map(|(r, _)| *r).collect();
        let main = |r: u32, part: usize| match entered.contains(&r) {
            true => entries.contains(&(r, part)),
            false => largest(r) == Some(part),
        };
        let has_main = |r: u32| sizes.keys().any(|&(sr, p)| sr == r && main(r, p));
        // Floor Link shoots from: its room's main part, out of water too
        // deep to stand in. The rest is out of bounds or swum.
        let stands = |f: usize| {
            room[f].is_some_and(|r| main(r, labels.part[f])) && !graph.deep(graph.centers[f])
        };
        // Each check: what stands between it and its room's main part.
        for check in &checks {
            let Ok(holder) = level.placement(check.hash) else {
                continue;
            };
            // A holder whose placement names no room (the Forest Temple's
            // Boss Key chest) takes the nearest floor's, as a guess.
            let unnamed = check.room.is_none();
            let Some(mut r) = check.room.or_else(|| room_within(holder, HOLDER_NEAR)) else {
                roomless.push(check.hash);
                continue;
            };
            let at = holder.pose.at;
            let skulltula = crate::pickups::SKULLTULAS.holds(check);
            let mut walled = None;
            if !floored.contains(&r) && !has_main(r) {
                // A gold skulltula on a floorless room's wall is hit from
                // the floor nearest it, whichever room that is.
                let near = skulltula
                    .then(|| graph.nearest_floor(at, REACH_MAX, stands))
                    .flatten();
                let Some(near) = near.and_then(|n| room[n.face]) else {
                    roomless.push(check.hash);
                    continue;
                };
                walled = Some(r);
                r = near;
            }
            let nearest = (0..room.len())
                .filter(|&f| room[f] == Some(r) && main(r, labels.part[f]))
                .min_by(|&a, &b| dist(graph.centers[a], at).total_cmp(&dist(graph.centers[b], at)));
            let by_hand = (!skulltula && walled.is_none())
                .then(|| hand::holder(check))
                .flatten();
            let taken = match by_hand {
                Some(kind) => {
                    let taken = TakenBy {
                        graph: &graph,
                        room,
                        part: &labels.part,
                        crossings: &crossings,
                    };
                    taken.hand(kind, r, at, &|p| main(r, p), &on)
                }
                None => {
                    let blocking = nearest
                        .map(|f| on(at, graph.centers[f]))
                        .unwrap_or_default();
                    let why = match &blocking[..] {
                        [] => "in reach from its room".to_owned(),
                        _ => in_front(&blocking),
                    };
                    Taken {
                        guess: blocking.iter().any(|(_, k)| k.guess),
                        on: blocking,
                        child: false,
                        needs: vec![],
                        why,
                    }
                }
            };
            let Taken {
                on: blocking,
                child,
                needs,
                mut why,
                mut guess,
            } = taken;
            // A chest its logic graph raises or unlocks waits on the
            // graph's flags, set where their setters stand.
            let mut waits = vec![];
            if let Some(gate) = chest_gate(check.hash) {
                let mut found = BTreeMap::new();
                locks_in(&gate.expr, &mut found);
                if found.is_empty() && !gate.expr.atoms().is_empty() {
                    why.push_str("; a logic graph gates it that derive doesn't read");
                    guess = true;
                }
                for (name, setters) in found {
                    let (lock, kinds) = lock_of(&name, setters);
                    write!(why, "; its graph waits on flag #{:08x}", lock.hash()).unwrap();
                    guess |= lock.room.is_none();
                    waits.push(format!("flag:#{:08x}", lock.hash()));
                    traced.flags.entry(name).or_insert((lock, kinds));
                }
            }
            let mut uses: Vec<&str> = blocking.iter().map(|(_, k)| k.uses.as_str()).collect();
            uses.extend(needs.iter().map(String::as_str));
            uses.extend(waits.iter().map(String::as_str));
            if child {
                uses.push("child");
            }
            let reach =
                skulltula.then(|| skulltula_reach(&graph, |f| room[f] == Some(r) && stands(f), at));
            if let Some(reach) = &reach {
                uses.push(&reach.needs);
                why = match blocking.is_empty() {
                    true => reach.why.clone(),
                    false => format!("{why}; {}", reach.why),
                };
                guess |= reach.guess;
            }
            let loose = crate::pickups::LOOSE.holds(check);
            let high = loose
                .then(|| loose_reach(&graph, |f| room[f] == Some(r) && stands(f), at))
                .flatten();
            if let Some(high) = high {
                why = format!("{why}; {high}");
                guess = true;
            }
            if let Some(w) = walled {
                why = format!("on a wall of room {w}, which has no floor; {why}");
                guess = true;
            }
            if unnamed {
                why = format!("its placement names no room, so its nearest floor's; {why}");
                guess = true;
            }
            if nearest.is_none() && by_hand.is_none() {
                why = "its room has no floor to reach it from".into();
                guess = true;
            }
            let name = (book.check(check.hash).map(|c| c.name.clone()))
                .or_else(|| named.get(&check.hash).cloned())
                .or_else(|| match is_chest(&check.holder) {
                    true => Some(format!(
                        "{} Chest",
                        super::words(check.item.trim_start_matches("Item_"))
                    )),
                    false => reach.is_none().then(|| super::words(&check.holder)),
                });
            traced.checks.entry(r).or_default().push(CheckOut {
                hash: check.hash,
                name,
                needs: all_of(&uses),
                why,
                guess,
            });
        }
    }
    confirmed.finish()?;
    // A flag a confirmed entry places is set in its room, whatever derive
    // traced.
    let locks = traced.locks.values_mut().flatten();
    confirmed.place_locks(
        locks.chain(traced.flags.values_mut().map(|(l, _)| l)),
        &mut warnings,
    );
    let placed = confirmed.flags();
    let (files, jumps) = room_files(&folder(dungeon.node.get()), &numbered, traced, &placed);
    let floorless = numbered.difference(&floored).copied().collect();
    Ok(Derived {
        files,
        faces,
        unplaced,
        floorless,
        roomless,
        exitless,
        jumps,
        warnings,
    })
}

/// Writes `dungeon`'s baseline room files under the logic folder `dir`,
/// first clearing the old ones, so a renamed room leaves no stale file.
pub fn write(romfs: &Romfs, game: &Path, dungeon: &Dungeon, dir: &Path) -> Result<Derived> {
    let book = Book::load_from(dir)?;
    let flagmap = crate::flagmap::Graph::build(romfs)?;
    let derived = derive(romfs, game, dungeon, &book, &flagmap)?;
    let folder = dir.join("baseline").join(folder(dungeon.node.get()));
    if folder.exists() {
        for entry in std::fs::read_dir(&folder)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with("room-") && name.ends_with(".toml") {
                std::fs::remove_file(&path)?;
            }
        }
    }
    for (rel, text) in &derived.files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().context("a room file's folder")?)?;
        std::fs::write(&path, text)?;
    }
    Ok(derived)
}

/// `entries` (room, part) grown across `opens`, each a pair of parts of
/// one room with only a flag shutter between: the parts Link enters.
fn past_shutters(
    mut entries: HashSet<(u32, usize)>,
    opens: &[(usize, usize)],
) -> HashSet<(u32, usize)> {
    loop {
        // Every room entered at `a`: a part may hold two rooms, and
        // picking one by the set's order made derive differ run to run.
        let more: Vec<(u32, usize)> = (opens.iter())
            .flat_map(|&(a, b)| {
                (entries.iter())
                    .filter(move |(_, p)| *p == a)
                    .map(move |&(r, _)| (r, b))
            })
            .filter(|e| !entries.contains(e))
            .collect();
        if more.is_empty() {
            return entries;
        }
        entries.extend(more);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_entered_between_a_shutters_leaves_is_entered_past_them() {
        // Room 2 entered on the strip (part 7) between its shutter's leaves,
        // the room itself past the second (part 8); part 9 is behind a web.
        let entries = HashSet::from([(2, 7)]);
        let found = past_shutters(entries, &[(7, 8), (3, 9)]);
        assert_eq!(found, HashSet::from([(2, 7), (2, 8)]));
    }

    #[test]
    fn a_path_meets_a_solid_it_passes_through() {
        let wall = Solid {
            name: "Obj_FakeShutter".into(),
            hash: Some(3),
            ring: vec![[1.9, 0.0], [2.1, 0.0], [2.1, 2.0], [1.9, 2.0]],
            low: 0.0,
            high: 3.0,
        };
        assert!(meets(&wall, [1.0, 0.0, 1.0], [3.0, 0.0, 1.0]));
        assert!(!meets(&wall, [3.0, 0.0, 1.0], [5.0, 0.0, 1.0]));
        // Over its top, past the margin.
        assert!(!meets(&wall, [1.0, 4.0, 1.0], [3.0, 4.0, 1.0]));
    }

    /// `id`'s committed room files against a fresh derive, which it returns.
    fn fresh(id: &str) -> Option<Derived> {
        let romfs = crate::game::dev_dump()?;
        let game = Path::new("../../dump/oot.nup");
        let dungeon = (crate::entrances::DUNGEONS.iter())
            .find(|d| d.id == id)
            .unwrap();
        let book = crate::rules::Book::load().unwrap();
        let flagmap = crate::flagmap::Graph::dev()?;
        let fresh = derive(&romfs, game, dungeon, &book, flagmap).unwrap();
        let folder = format!("baseline/{}/", folder(dungeon.node.get()));
        let committed: Vec<(String, String)> = (rooms::FILES.iter())
            .filter(|(path, _)| path.starts_with(&folder))
            .map(|&(p, t)| (p.to_owned(), t.to_owned()))
            .collect();
        assert!(
            fresh.files == committed,
            "{id} room files are stale: run `zootr logic derive {id}`"
        );
        Some(fresh)
    }

    /// One test a dungeon, so they derive side by side.
    macro_rules! fresh {
        ($($test:ident: $id:literal,)*) => {$(
            #[test]
            #[ignore = "derives a whole dungeon from the dump: run with `--ignored` when changing derive, and take this off once its dungeon is LIVE"]
            fn $test() {
                needs_names!();
                fresh($id);
            }
        )*};
    }

    fresh! {
        dodongos_cavern_rooms_match_a_fresh_derive: "dodongos-cavern",
        jabu_jabu_rooms_match_a_fresh_derive: "jabu-jabu",
        forest_temple_rooms_match_a_fresh_derive: "forest-temple",
        fire_temple_rooms_match_a_fresh_derive: "fire-temple",
        water_temple_rooms_match_a_fresh_derive: "water-temple",
        shadow_temple_rooms_match_a_fresh_derive: "shadow-temple",
        spirit_temple_rooms_match_a_fresh_derive: "spirit-temple",
        bottom_of_the_well_rooms_match_a_fresh_derive: "bottom-of-the-well",
        ice_cavern_rooms_match_a_fresh_derive: "ice-cavern",
        gerudo_training_ground_rooms_match_a_fresh_derive: "gerudo-training-ground",
        ganons_castle_rooms_match_a_fresh_derive: "ganons-castle",
    }

    /// The committed Deku Tree room files match a fresh derive.
    #[test]
    fn the_committed_deku_tree_rooms_match_a_fresh_derive() {
        needs_names!();
        if !crate::game::after_merge() {
            return;
        }
        let Some(fresh) = fresh("deku-tree") else {
            return;
        };
        // Every room the game numbers has floor but the Skulltula alcove,
        // whose Skulltulas hang on a wall, and every check has a room.
        // Its gold skulltula is hit from the floor nearest it.
        assert_eq!(fresh.floorless, [11]);
        assert!(fresh.roomless.is_empty(), "{:?}", fresh.roomless);
        assert!(fresh.exitless.is_empty(), "{:?}", fresh.exitless);
        let all: String = fresh.files.iter().map(|(_, t)| t.as_str()).collect();
        assert!(
            all.contains("needs = \"flag:#"),
            "a shutter waits on its flag"
        );
        // The navmesh also covers the tree's outer shell and pockets behind
        // walls, which the solid walls keep out of every room: about a third
        // of its faces (2,346, 2026-11-22).
        assert!(
            fresh.unplaced * 2 < fresh.faces,
            "{} of {}",
            fresh.unplaced,
            fresh.faces
        );
    }
}
