//! A dungeon's rooms from its navmesh: each face takes the room of the
//! nearest placement that names one (`DefaultDungeonRoomIndex`), by walking
//! distance, so a room never reaches across a wall or through a door. The
//! arcs between rooms become each room file's links.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::{Romfs, murmur3};

use crate::entrances::Dungeon;
use crate::gates::{Atom, Expr as GateExpr};
use crate::known::{field, flag};
use crate::nav::Level;
use crate::nav::graph::{Arc, Cross, Graph, Via, dist};
use crate::nav::limits::limits;
use crate::nav::shape::{Solid, parts_of};
use crate::rules::Book;
use crate::rules::kinds::{Kind, Sets};
use crate::rules::rooms::{self, folder};
use crate::sweep::spot::slug;
use crate::warp::Placement;

/// The widest level gap a room spreads across, in m: a short hop over a
/// crack or a ledge's lip, well inside any jump. For labelling only, never a
/// reach claim (`limits.toml` holds those).
const GAP: f32 = 2.0;

/// A distance the heap can order.
#[derive(Clone, Copy)]
struct Far(f32);
impl PartialEq for Far {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Far {}
impl PartialOrd for Far {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Far {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// The path an arc takes: face to face across a shared edge (a shutter in
/// the doorway sits on it), or a link's own ends.
fn path_of(graph: &Graph, from: usize, arc: &Arc) -> ([f32; 3], [f32; 3]) {
    match arc.cross {
        Cross::Edge(_) => (graph.centers[from], graph.centers[arc.to]),
        Cross::Link { from: a, to: b, .. } => (a, b),
    }
}

/// Each face's room and part.
pub struct Labels {
    /// `None` for a face no named placement reaches.
    pub room: Vec<Option<u32>>,
    /// The faces joined with nothing in the way share a part: a chest
    /// behind a web is in another part of its room.
    pub part: Vec<usize>,
}

/// Each face's room. A room spreads across anything but a door or a path
/// `blocked` says an obstacle stands on, so rooms meet at their doors,
/// shutters and webs.
pub fn label(
    graph: &Graph,
    placed: &[Placement],
    rooms: &HashMap<u64, u32>,
    blocked: impl Fn([f32; 3], [f32; 3]) -> bool,
) -> Labels {
    let n = graph.centers.len();
    // Both ways: a ledge whose only arc is a drop off it is still its room's.
    // `near` leaves out what an obstacle stands on, `all` keeps it.
    let (mut near, mut all): (Vec<Vec<usize>>, Vec<Vec<usize>>) =
        (vec![vec![]; n], vec![vec![]; n]);
    let mut join = |a: usize, b: usize, open: bool| {
        for list in [&mut all].into_iter().chain(open.then_some(&mut near)) {
            list[a].push(b);
            list[b].push(a);
        }
    };
    for (from, arcs) in graph.arcs.iter().enumerate() {
        for arc in arcs {
            if !matches!(
                arc.cross,
                Cross::Link {
                    via: Via::Door(_),
                    ..
                }
            ) {
                let (a, b) = path_of(graph, from, arc);
                join(from, arc.to, !blocked(a, b));
            }
        }
    }
    for (a, b, from, to) in graph.gaps(GAP) {
        join(a, b, !blocked(from, to));
    }
    let mut best: Vec<Option<(f32, u32)>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    for p in placed {
        let Some(&room) = p.hash.and_then(|h| rooms.get(&h)) else {
            continue;
        };
        let Some(snap) = graph.locate(p.pose.at) else {
            continue;
        };
        let d = dist(snap.at, graph.centers[snap.face]);
        if best[snap.face].is_none_or(|(b, _)| d < b) {
            best[snap.face] = Some((d, room));
            heap.push(Reverse((Far(d), snap.face, room)));
        }
    }
    spread(graph, &near, &mut best, heap, &vec![false; n]);
    // The floor between two obstacles (Deku hangs shutters in pairs) joins
    // the nearest room past them; a face pass one placed keeps its room.
    let kept: Vec<bool> = best.iter().map(Option::is_some).collect();
    let heap = (0..n)
        .filter_map(|f| Some(Reverse((Far(0.0), f, best[f]?.1))))
        .collect();
    for b in best.iter_mut().flatten() {
        b.0 = 0.0;
    }
    spread(graph, &all, &mut best, heap, &kept);
    Labels {
        room: best.into_iter().map(|b| b.map(|(_, r)| r)).collect(),
        part: parts(&near),
    }
}

/// Each face's part: the faces `near` joins, numbered by their first face.
fn parts(near: &[Vec<usize>]) -> Vec<usize> {
    let mut part = vec![usize::MAX; near.len()];
    for start in 0..near.len() {
        if part[start] != usize::MAX {
            continue;
        }
        part[start] = start;
        let mut stack = vec![start];
        while let Some(f) = stack.pop() {
            for &to in &near[f] {
                if part[to] == usize::MAX {
                    part[to] = start;
                    stack.push(to);
                }
            }
        }
    }
    part
}

/// Spreads each room in `heap` over `near`, nearest first, never into a
/// face `kept` holds.
fn spread(
    graph: &Graph,
    near: &[Vec<usize>],
    best: &mut [Option<(f32, u32)>],
    mut heap: BinaryHeap<Reverse<(Far, usize, u32)>>,
    kept: &[bool],
) {
    while let Some(Reverse((Far(d), face, room))) = heap.pop() {
        if best[face].is_some_and(|(b, r)| b < d || (b == d && r != room)) {
            continue;
        }
        for &to in near[face].iter().filter(|&&to| !kept[to]) {
            let next = d + dist(graph.centers[face], graph.centers[to]);
            if best[to].is_none_or(|(b, _)| next < b) {
                best[to] = Some((next, room));
                heap.push(Reverse((Far(next), to, room)));
            }
        }
    }
}

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
/// The file in `logic/` the names come from, for the built-in files test.
#[cfg(test)]
pub(crate) const NAMES_FILE: &str = "room-names.toml";

/// How far from the floor an exit's trigger box may sit and still take
/// its room, in metres.
const NEAR: f32 = 6.0;

/// The logic node that drives a shutter's lattice.
const SHUTTER: &str = "ActorLogicShutter";

/// How far round an obstacle's collision a path still meets it, in m.
const ON: f32 = 0.5;
/// How far apart a path's samples are, in m.
const STEP: f32 = 0.25;

/// Whether the straight path from `a` to `b` passes through `solid`.
fn meets(solid: &Solid, a: [f32; 3], b: [f32; 3]) -> bool {
    let n = (dist(a, b) / STEP).ceil().max(1.0) as usize;
    (0..=n).any(|i| {
        let t = i as f32 / n as f32;
        let p: [f32; 3] = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
        (solid.low - ON..=solid.high + ON).contains(&p[1]) && solid.holds([p[0], p[2]])
    })
}

/// What a crossing from one room to another was seen as.
#[derive(Default)]
struct Seen<'k> {
    count: usize,
    /// Drop or step heights, gap widths.
    least: f32,
    most: f32,
    /// The placements it goes by (a ladder, a door, water).
    by: BTreeSet<u64>,
    /// The obstacle kinds standing on it, by placement.
    on: BTreeMap<u64, &'k Kind>,
}

impl Seen<'_> {
    fn add(&mut self, metric: f32, by: Option<u64>) {
        if self.count == 0 {
            (self.least, self.most) = (metric, metric);
        }
        self.count += 1;
        self.least = self.least.min(metric);
        self.most = self.most.max(metric);
        self.by.extend(by);
    }
}

fn via_of(via: Via) -> (rooms::Via, Option<u64>) {
    match via {
        Via::Walk => (rooms::Via::Walk, None),
        Via::Door(h) => (rooms::Via::Door, h),
        Via::Drop => (rooms::Via::Drop, None),
        Via::Step => (rooms::Via::Step, None),
        Via::Climb(h) => (rooms::Via::Climb, h),
        Via::Swim(h) => (rooms::Via::Swim, h),
    }
}

/// A flag a shutter waits on.
#[derive(Clone, Default)]
struct Lock {
    /// The game's name: code only, never written (the hook blocks it).
    name: String,
    /// The placements whose state sets it, when a graph sets it.
    setters: BTreeSet<u64>,
    /// Its room: its setters', or the room its name gives.
    room: Option<u32>,
}

impl Lock {
    fn hash(&self) -> u32 {
        murmur3(self.name.as_bytes())
    }

    /// A flag only clearing its room sets (a fight) holds Link in; a flag
    /// a graph sets (a switch, torches) holds a door shut both ways.
    fn holds(&self, from: u32) -> bool {
        !self.setters.is_empty() || self.room == Some(from)
    }
}

/// The room a flag's name gives: `..._RoomB_...` is room 1.
fn room_in_name(name: &str) -> Option<u32> {
    let (_, rest) = name.split_once("_Room")?;
    let mut chars = rest.chars();
    let letter = chars.next().filter(char::is_ascii_uppercase)?;
    matches!(chars.next(), Some('_') | None).then(|| letter as u32 - 'A' as u32)
}

/// The flags `expr` waits on, with the placements that set each.
fn locks_in(expr: &GateExpr, out: &mut BTreeMap<String, BTreeSet<u64>>) {
    match expr {
        GateExpr::SetFlag(name, drives) => {
            let setters = drives
                .iter()
                .flat_map(GateExpr::atoms)
                .filter_map(|a| match a {
                    Atom::Actor { hash, .. } => *hash,
                    _ => None,
                });
            out.entry(name.clone()).or_default().extend(setters);
        }
        GateExpr::Atom(Atom::Flag(name)) => {
            out.entry(name.clone()).or_default();
        }
        GateExpr::And(v) | GateExpr::Or(v) => v.iter().for_each(|e| locks_in(e, out)),
        GateExpr::Not(e) => locks_in(e, out),
        GateExpr::Const(_) | GateExpr::Atom(_) => {}
    }
}

/// A check as a room file writes it.
struct CheckOut {
    hash: u64,
    name: String,
    needs: String,
    why: String,
    guess: bool,
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
    for u in uses.iter().filter(|u| **u != "always") {
        if !kept.contains(u) {
            kept.push(u);
        }
    }
    match &kept[..] {
        [] => "always".to_owned(),
        [one] => (*one).to_owned(),
        many => many
            .iter()
            .map(|n| format!("({n})"))
            .collect::<Vec<_>>()
            .join(" and "),
    }
}

/// The room files of `dungeon`, from its navmesh, its placements, its
/// logic graphs and `book`'s kinds and check names.
pub fn derive(romfs: &Romfs, game: &Path, dungeon: &Dungeon, book: &Book) -> Result<Derived> {
    let kind_of: HashMap<u32, &Kind> = book.kinds.iter().map(|k| (k.hash, k)).collect();
    let kind_at = |p: &Placement| kind_of.get(&murmur3(p.name.as_bytes())).copied();
    let jump = limits().child.edge_jump.as_ref().map_or(0.0, |j| j.value);
    let shutters = crate::gates::trace_node(romfs, SHUTTER)?;
    let checks = crate::checks::all(romfs)?;
    // Named one at a time: an item Archipelago has no name for fails only its own.
    let named: HashMap<u64, String> = (checks.iter())
        .filter_map(|c| {
            crate::ap::names::locations(romfs, std::slice::from_ref(c))
                .ok()?
                .pop_first()
        })
        .collect();
    let mut seen: BTreeMap<(u32, u32, rooms::Via), Seen> = BTreeMap::new();
    let mut locks: HashMap<u64, Vec<Lock>> = HashMap::new();
    let mut flags: BTreeMap<String, (Lock, Vec<&Kind>)> = BTreeMap::new();
    let mut checks_out: BTreeMap<u32, Vec<CheckOut>> = BTreeMap::new();
    let mut roomless = vec![];
    let exits = crate::world::exits(romfs)?;
    // Each room's exits and arrival spots, by key, with their names.
    let mut spots: BTreeMap<(u32, &str), BTreeMap<u32, String>> = BTreeMap::new();
    let mut exitless = vec![];
    let mut numbered = BTreeSet::new();
    let mut floored = BTreeSet::new();
    let (mut faces, mut unplaced) = (0, 0);
    for map in maps(romfs, dungeon)? {
        let level = Level::of(romfs, game, &map)?;
        let graph = level.graph();
        let rooms_of = crate::warp::rooms_in(romfs, &map)?;
        numbered.extend(rooms_of.values().copied());
        // The obstacles placed in this map: each one's kind and collision.
        let mut obstacles: Vec<(u64, &Kind, Vec<Solid>)> = vec![];
        for p in &level.placed {
            if let (Some(h), Some(k)) = (p.hash, kind_at(p))
                && k.sets == Sets::Opens
            {
                obstacles.push((h, k, parts_of(romfs, p)?));
            }
        }
        let on_path = |a: [f32; 3], b: [f32; 3]| {
            (obstacles.iter()).any(|(_, _, solids)| solids.iter().any(|s| meets(s, a, b)))
        };
        let on = |a: [f32; 3], b: [f32; 3]| -> Vec<(u64, &Kind)> {
            (obstacles.iter())
                .filter(|(_, _, solids)| solids.iter().any(|s| meets(s, a, b)))
                .map(|(h, k, _)| (*h, *k))
                .collect()
        };
        let labels = label(&graph, &level.placed, &rooms_of, on_path);
        let room = &labels.room;
        faces += room.len();
        unplaced += room.iter().filter(|l| l.is_none()).count();
        floored.extend(room.iter().flatten().copied());
        let room_at = |p: &Placement| {
            let hash = p.hash?;
            (rooms_of.get(&hash).copied()).or_else(|| room[graph.locate(p.pose.at)?.face])
        };
        // A placement off the floor (an exit's trigger box) takes the room
        // of the nearest floor around it.
        let room_near = |p: &Placement| {
            room_at(p).or_else(|| {
                (0..room.len())
                    .filter(|&f| room[f].is_some())
                    .map(|f| (dist(graph.centers[f], p.pose.at), f))
                    .filter(|(d, _)| *d < NEAR)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .and_then(|(_, f)| room[f])
            })
        };
        let name_of = |to: &str, start: &str| match start {
            "-" => super::words(to),
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
            match exit_placed(&e.name) {
                Some(r) => drop(
                    spots
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
            let name = name_of(&rule.to, "-");
            match boss.filter(|_| dungeon.boss.is_some_and(|b| b.get() == rule.to)) {
                Some(r) => drop(
                    spots
                        .entry((r, "exit"))
                        .or_default()
                        .insert(rooms::spot(&rule.to, "-"), name),
                ),
                None => exitless.push(name),
            }
        }
        let starts = (level.placed.iter()).filter(|p| p.name == flag::START_POS.get());
        for p in starts {
            if let (Some(label), Some(r)) = (&p.label, room_near(p)) {
                let at = spots.entry((r, "arrival")).or_default();
                at.insert(rooms::spot(&map, label), name_of(&map, label));
            }
        }
        // Each shutter's flags, and the rooms that set them.
        for gate in shutters.iter().filter(|g| level.placement(g.hash).is_ok()) {
            let mut found = BTreeMap::new();
            locks_in(&gate.expr, &mut found);
            for (name, setters) in found {
                let setter_room = (setters.iter())
                    .filter_map(|h| room_at(level.placement(*h).ok()?))
                    .next();
                let lock = Lock {
                    room: setter_room.or_else(|| room_in_name(&name)),
                    name: name.clone(),
                    setters,
                };
                let kinds: Vec<&Kind> = (lock.setters.iter())
                    .filter_map(|h| kind_at(level.placement(*h).ok()?))
                    .collect();
                flags.entry(name).or_insert((lock.clone(), kinds));
                locks.entry(gate.hash).or_default().push(lock);
            }
        }
        let mut note = |a: usize,
                        b: usize,
                        via: rooms::Via,
                        metric: f32,
                        by: Option<u64>,
                        path: ([f32; 3], [f32; 3])| {
            let (Some(ra), Some(rb)) = (room[a], room[b]) else {
                return;
            };
            if ra != rb {
                let s = seen.entry((ra, rb, via)).or_default();
                s.add(metric, by);
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
                note(from, arc.to, via, metric, by, path);
            }
        }
        for (a, b, from, to) in graph.plunges() {
            note(a, b, rooms::Via::Drop, from[1] - to[1], None, (from, to));
        }
        for (a, b, from, to) in graph.gaps(jump) {
            let width = (from[0] - to[0]).hypot(from[2] - to[2]);
            note(a, b, rooms::Via::Jump, width, None, (from, to));
        }
        // Each room's main part: the one with the most of its floor.
        let mut sizes: HashMap<(u32, usize), usize> = HashMap::new();
        for (f, r) in room.iter().enumerate() {
            if let Some(r) = r {
                *sizes.entry((*r, labels.part[f])).or_default() += 1;
            }
        }
        let main = |r: u32| {
            (sizes.iter())
                .filter(|((sr, _), _)| *sr == r)
                .max_by_key(|(k, n)| (**n, std::cmp::Reverse(k.1)))
                .map(|((_, p), _)| *p)
        };
        // Each check: what stands between it and its room's main part.
        for check in checks.iter().filter(|c| c.room.is_some()) {
            let Ok(holder) = level.placement(check.hash) else {
                continue;
            };
            let Some(r) = check.room else { continue };
            if !floored.contains(&r) && main(r).is_none() {
                roomless.push(check.hash);
                continue;
            }
            let at = holder.pose.at;
            let nearest = (0..room.len())
                .filter(|&f| room[f] == Some(r) && Some(labels.part[f]) == main(r))
                .min_by(|&a, &b| dist(graph.centers[a], at).total_cmp(&dist(graph.centers[b], at)));
            let blocking = nearest
                .map(|f| on(at, graph.centers[f]))
                .unwrap_or_default();
            let uses: Vec<&str> = blocking.iter().map(|(_, k)| k.uses.as_str()).collect();
            let mut why = match &blocking[..] {
                [] => "in reach from its room".to_owned(),
                _ => (blocking.iter())
                    .map(|(h, k)| format!("{} {h} in front: {}", k.name, k.uses))
                    .collect::<Vec<_>>()
                    .join("; "),
            };
            let mut guess = blocking.iter().any(|(_, k)| k.guess);
            if nearest.is_none() {
                why = "its room has no floor to reach it from".into();
                guess = true;
            }
            let name = (book.check(check.hash).map(|c| c.name.clone()))
                .or_else(|| named.get(&check.hash).cloned())
                .unwrap_or_else(|| match check.holder.contains("TBox") {
                    true => format!(
                        "{} Chest",
                        super::words(check.item.trim_start_matches("Item_"))
                    ),
                    false => super::words(&check.holder),
                });
            checks_out.entry(r).or_default().push(CheckOut {
                hash: check.hash,
                name,
                needs: all_of(&uses),
                why,
                guess,
            });
        }
    }
    let folder = folder(dungeon.node.get());
    let names = NAMES.get(&folder);
    let q = |s: &str| toml::Value::String(s.to_owned()).to_string();
    let mut files = vec![];
    for &room in &numbered {
        let name = names.and_then(|n| n.get(&room));
        let file = match name {
            Some(n) => format!("room-{room:02}-{}.toml", slug(n)),
            None => format!("room-{room:02}.toml"),
        };
        let words = name.cloned().unwrap_or_else(|| format!("Room {room:02}"));
        let mut out = format!(
            "# Generated by `zootr logic derive`. Don't edit: confirmed/ overrides it.\nroom = {room}\nname = {}\n",
            q(&words)
        );
        let mut entry = |table: &str, fields: &[(&str, String)], guess: bool| {
            write!(out, "\n[[{table}]]\n").unwrap();
            for (k, v) in fields {
                writeln!(out, "{k} = {v}").unwrap();
            }
            if guess {
                out += "guess = true\n";
            }
        };
        let from_here = (room, 0, rooms::Via::Walk)..=(room, u32::MAX, rooms::Via::Sink);
        for (&(_, to, via), s) in seen.range(from_here) {
            let (needs, why, guess) = link(via, room, s, &locks);
            let fields = [
                ("to", to.to_string()),
                ("needs", q(&needs)),
                ("via", q(via.word())),
                ("why", q(&why)),
            ];
            entry("link", &fields, guess);
        }
        let mut here = checks_out.remove(&room).unwrap_or_default();
        here.sort_by_key(|c| c.hash);
        for c in here {
            let fields = [
                ("hash", c.hash.to_string()),
                ("name", q(&c.name)),
                ("needs", q(&c.needs)),
                ("why", q(&c.why)),
            ];
            entry("check", &fields, c.guess);
        }
        let mut set_here: Vec<&(Lock, Vec<&Kind>)> = flags
            .values()
            .filter(|(l, _)| l.room == Some(room))
            .collect();
        set_here.sort_by_key(|(l, _)| l.hash());
        for (lock, kinds) in set_here {
            let (needs, why, guess) = flag(lock, kinds);
            let fields = [
                ("hash", format!("0x{:08x}", lock.hash())),
                ("name", q(&super::words(&lock.name))),
                ("needs", q(&needs)),
                ("why", q(&why)),
            ];
            entry("flag", &fields, guess);
        }
        for table in ["exit", "arrival"] {
            for (hash, name) in spots.get(&(room, table)).into_iter().flatten() {
                entry(
                    table,
                    &[("hash", format!("0x{hash:08x}")), ("name", q(name))],
                    false,
                );
            }
        }
        files.push((format!("baseline/{folder}/{file}"), out));
    }
    let floorless = numbered.difference(&floored).copied().collect();
    Ok(Derived {
        files,
        faces,
        unplaced,
        floorless,
        roomless,
        exitless,
    })
}

/// A flag's needs, why and whether it's a guess, from what sets it.
fn flag(lock: &Lock, kinds: &[&Kind]) -> (String, String, bool) {
    if lock.setters.is_empty() {
        let why = "set by clearing the room: what clears it isn't traced yet".to_owned();
        return ("always".into(), why, true);
    }
    let uses: Vec<&str> = kinds.iter().map(|k| k.uses.as_str()).collect();
    let mut seen = BTreeSet::new();
    let why = (kinds.iter())
        .filter(|k| seen.insert(k.hash))
        .map(|k| format!("{} sets it ({}): {}", k.name, lock.setters.len(), k.uses))
        .collect::<Vec<_>>()
        .join("; ");
    let guess = kinds.is_empty() || kinds.iter().any(|k| k.guess);
    (all_of(&uses), why, guess)
}

/// A link's needs, why and whether it's a guess, from what was seen
/// crossing from `from`.
fn link(
    via: rooms::Via,
    from: u32,
    s: &Seen,
    locks: &HashMap<u64, Vec<Lock>>,
) -> (String, String, bool) {
    let span = |unit: &str| {
        if (s.most - s.least).abs() < 0.05 {
            format!("{:.1} {unit}", s.least)
        } else {
            format!("{:.1} to {:.1} {unit}", s.least, s.most)
        }
    };
    let by = || {
        s.by.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut why = match via {
        rooms::Via::Walk => format!("walk, {} edges", s.count),
        rooms::Via::Drop if s.most > limits().drop() => format!("drop {} into water", span("m")),
        rooms::Via::Drop => format!("drop {}", span("m")),
        rooms::Via::Step => format!("step up {}", span("m")),
        rooms::Via::Jump => format!("level gap {}: level jump not measured", span("m")),
        _ if s.by.is_empty() => via.word().to_owned(),
        _ => format!("{} {}", via.word(), by()),
    };
    let mut guess = via == rooms::Via::Jump;
    let mut needs: Vec<String> = vec![];
    for (h, k) in &s.on {
        if k.uses != "always" {
            write!(why, "; {} {h}: {}", k.name, k.uses).unwrap();
            guess |= k.guess;
            needs.push(k.uses.clone());
            continue;
        }
        for lock in locks.get(h).into_iter().flatten().filter(|l| l.holds(from)) {
            write!(why, "; {} {h} shut until flag #{:08x}", k.name, lock.hash()).unwrap();
            guess |= lock.room.is_none();
            needs.push(format!("flag:#{:08x}", lock.hash()));
        }
    }
    let needs: Vec<&str> = needs.iter().map(String::as_str).collect();
    (all_of(&needs), why, guess)
}

/// Writes `dungeon`'s baseline room files under the logic folder `dir`,
/// first clearing the old ones, so a renamed room leaves no stale file.
pub fn write(romfs: &Romfs, game: &Path, dungeon: &Dungeon, dir: &Path) -> Result<Derived> {
    let book = Book::load()?;
    let derived = derive(romfs, game, dungeon, &book)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph;
    use crate::warp::Pose;

    fn tag(hash: u64, at: [f32; 3]) -> Placement {
        Placement {
            name: "Obj_FakeTag".into(),
            hash: Some(hash),
            label: None,
            pose: Pose {
                at,
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        }
    }

    /// Four floors in a row, x 0 to 8, a room tagged at each end, and one
    /// floor far off that nothing tags.
    fn row() -> (ounce_formats::NavMesh, Vec<Placement>, HashMap<u64, u32>) {
        let m = mesh(&[
            quad(0.0, 0.0, 2.0, 2.0, 0.0),
            quad(2.0, 0.0, 4.0, 2.0, 0.0),
            quad(4.0, 0.0, 6.0, 2.0, 0.0),
            quad(6.0, 0.0, 8.0, 2.0, 0.0),
            quad(50.0, 0.0, 52.0, 2.0, 0.0),
        ]);
        let placed = vec![tag(1, [1.0, 0.0, 1.0]), tag(2, [7.0, 0.0, 1.0])];
        (m, placed, HashMap::from([(1, 1), (2, 2)]))
    }

    #[test]
    fn each_floor_takes_the_room_of_the_nearest_tag_by_walking() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let labels = label(&g, &placed, &rooms, |_, _| false).room;
        assert_eq!(labels, [Some(1), Some(1), Some(2), Some(2), None]);
    }

    /// An obstacle at x 2 holds room 1 back, so the floor past it is room 2's.
    #[test]
    fn a_room_stops_at_an_obstacle() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let at_two = |a: [f32; 3], b: [f32; 3]| (a[0] - 2.0) * (b[0] - 2.0) < 0.0;
        let labels = label(&g, &placed, &rooms, at_two).room;
        assert_eq!(labels, [Some(1), Some(2), Some(2), Some(2), None]);
    }

    /// The floor between two obstacles joins the nearest room past them.
    #[test]
    fn the_floor_between_two_obstacles_joins_a_room() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let between =
            |a: [f32; 3], b: [f32; 3]| [2.0, 6.0].iter().any(|x| (a[0] - x) * (b[0] - x) < 0.0);
        let labels = label(&g, &placed, &rooms, between).room;
        assert!(labels[1].is_some() && labels[2].is_some(), "{labels:?}");
    }

    #[test]
    fn a_flag_names_its_room_by_letter() {
        assert_eq!(room_in_name("FakeTemple_RoomB_Cleared"), Some(1));
        assert_eq!(room_in_name("FakeTemple_RoomA"), Some(0));
        assert_eq!(room_in_name("FakeTemple_Roomy_Cleared"), None);
        assert_eq!(room_in_name("FakeTemple_Cleared"), None);
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

    /// The committed Deku Tree room files match a fresh derive.
    #[test]
    fn the_committed_deku_tree_rooms_match_a_fresh_derive() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        needs_names!();
        let game = Path::new("../../dump/oot.nup");
        let deku = (crate::entrances::DUNGEONS.iter())
            .find(|d| d.node == map::INSIDE_THE_DEKU_TREE)
            .unwrap();
        let book = crate::rules::Book::load().unwrap();
        let fresh = derive(&romfs, game, deku, &book).unwrap();
        let folder = format!("baseline/{}/", folder(deku.node.get()));
        let committed: Vec<(String, String)> = (rooms::FILES.iter())
            .filter(|(path, _)| path.starts_with(&folder))
            .map(|&(p, t)| (p.to_owned(), t.to_owned()))
            .collect();
        assert!(
            fresh.files == committed,
            "Deku Tree room files are stale: run `zootr logic derive deku-tree`"
        );
        // Every room the game numbers has floor but the Skulltula alcove,
        // whose Skulltulas hang on a wall, and nearly every face has a room.
        assert_eq!(fresh.floorless, [11]);
        assert!(fresh.roomless.is_empty(), "{:?}", fresh.roomless);
        assert!(fresh.exitless.is_empty(), "{:?}", fresh.exitless);
        let all: String = fresh.files.iter().map(|(_, t)| t.as_str()).collect();
        assert!(
            all.contains("needs = \"flag:#"),
            "a shutter waits on its flag"
        );
        assert!(
            fresh.unplaced * 10 < fresh.faces,
            "{} of {}",
            fresh.unplaced,
            fresh.faces
        );
    }
}
