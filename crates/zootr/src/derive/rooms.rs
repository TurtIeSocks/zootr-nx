//! A dungeon's rooms from its navmesh: each face takes the room of the
//! nearest placement that names one (`DefaultDungeonRoomIndex`), by walking
//! distance, so a room never reaches across a wall or through a door. The
//! arcs between rooms become each room file's links.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::{Romfs, murmur3};

use crate::entrances::Dungeon;
use crate::known::{field, flag};
use crate::nav::Level;
use crate::nav::graph::{Cross, Via, dist};
use crate::nav::limits::limits;
use crate::nav::shape::{Solid, parts_of};
use crate::rules::Book;
use crate::rules::kinds::{Kind, Sets};
use crate::rules::rooms::{self, folder};
use crate::sweep::spot::slug;
use crate::warp::Placement;

mod label;
mod locks;
mod reach;

use label::{label, path_of};
use locks::{Lock, flag, locks_in, room_in_name};
use reach::{REACH_MAX, skulltula_reach};

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
    /// Where it was seen with the least metric: the place to test it.
    at: ([f32; 3], [f32; 3]),
}

impl Seen<'_> {
    fn add(&mut self, metric: f32, by: Option<u64>, at: ([f32; 3], [f32; 3])) {
        if self.count == 0 || metric < self.least {
            self.at = at;
        }
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

/// A check as a room file writes it.
struct CheckOut {
    hash: u64,
    /// `None` names it by its room: a gold skulltula nothing else names.
    name: Option<String>,
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
    /// Each jump link, at its narrowest: where a playtest settles it.
    pub jumps: Vec<Jump>,
}

/// A jump link at its narrowest gap.
pub struct Jump {
    pub from: u32,
    pub to: u32,
    pub width: f32,
    pub at: ([f32; 3], [f32; 3]),
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
    let jump = limits().level_jump();
    let shutters = crate::gates::trace_node(romfs, SHUTTER)?;
    let flagmap = crate::flagmap::Graph::build(romfs)?;
    let mut checks = crate::checks::all(romfs)?;
    // Gold skulltulas are pickups, which `all` leaves to their setting.
    checks.extend(crate::pickups::SKULLTULAS.checks(romfs)?);
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
    let mut jumps = vec![];
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
            for (name, mut setters) in found {
                // No graph sets it: a placement may, playing an event.
                if setters.is_empty() {
                    let placed = flagmap.played_by(&name).into_iter();
                    setters.extend(placed.filter(|h| level.placement(*h).is_ok()));
                }
                let setter_room = (setters.iter())
                    .filter_map(|h| room_at(level.placement(*h).ok()?))
                    .next();
                let named = (setters.iter())
                    .filter_map(|h| {
                        Some(format!(
                            "{} {h}",
                            super::words(&level.placement(*h).ok()?.name)
                        ))
                    })
                    .collect();
                let lock = Lock {
                    room: setter_room.or_else(|| room_in_name(&name)),
                    name: name.clone(),
                    setters,
                    named,
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
                s.add(metric, by, path);
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
        // Floor Link shoots from: its room's main part, out of water too
        // deep to stand in. The rest is out of bounds or swum.
        let stands = |f: usize| {
            room[f].and_then(main) == Some(labels.part[f])
                && graph.above(graph.centers[f]).is_none()
        };
        // Each check: what stands between it and its room's main part.
        for check in checks.iter().filter(|c| c.room.is_some()) {
            let Ok(holder) = level.placement(check.hash) else {
                continue;
            };
            let Some(mut r) = check.room else { continue };
            let at = holder.pose.at;
            let skulltula = crate::pickups::SKULLTULAS.holds(check);
            let mut walled = None;
            if !floored.contains(&r) && main(r).is_none() {
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
                .filter(|&f| room[f] == Some(r) && Some(labels.part[f]) == main(r))
                .min_by(|&a, &b| dist(graph.centers[a], at).total_cmp(&dist(graph.centers[b], at)));
            let blocking = nearest
                .map(|f| on(at, graph.centers[f]))
                .unwrap_or_default();
            let mut uses: Vec<&str> = blocking.iter().map(|(_, k)| k.uses.as_str()).collect();
            let mut why = match &blocking[..] {
                [] => "in reach from its room".to_owned(),
                _ => (blocking.iter())
                    .map(|(h, k)| format!("{} {h} in front: {}", k.name, k.uses))
                    .collect::<Vec<_>>()
                    .join("; "),
            };
            let mut guess = blocking.iter().any(|(_, k)| k.guess);
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
            if let Some(w) = walled {
                why = format!("on a wall of room {w}, which has no floor; {why}");
                guess = true;
            }
            if nearest.is_none() {
                why = "its room has no floor to reach it from".into();
                guess = true;
            }
            let name = (book.check(check.hash).map(|c| c.name.clone()))
                .or_else(|| named.get(&check.hash).cloned())
                .or_else(|| match check.holder.contains("TBox") {
                    true => Some(format!(
                        "{} Chest",
                        super::words(check.item.trim_start_matches("Item_"))
                    )),
                    false => reach.is_none().then(|| super::words(&check.holder)),
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
            if via == rooms::Via::Jump {
                let (from, width, at) = (room, s.least, s.at);
                jumps.push(Jump {
                    from,
                    to,
                    width,
                    at,
                });
            }
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
        let unnamed = here.iter().filter(|c| c.name.is_none()).count();
        let mut nth = 0;
        for c in here {
            let name = c.name.unwrap_or_else(|| {
                nth += 1;
                match unnamed {
                    1 => format!("{words} Gold Skulltula"),
                    _ => format!("{words} Gold Skulltula {nth}"),
                }
            });
            let fields = [
                ("hash", c.hash.to_string()),
                ("name", q(&name)),
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
        jumps,
    })
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
        rooms::Via::Jump => format!(
            "level gap {}: within a running jump ({:.1} m)",
            span("m"),
            limits().level_jump()
        ),
        _ if s.by.is_empty() => via.word().to_owned(),
        _ => format!("{} {}", via.word(), by()),
    };
    let mut guess = false;
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
