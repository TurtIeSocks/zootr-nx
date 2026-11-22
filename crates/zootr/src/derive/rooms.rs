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
use crate::nav::Level;
use crate::nav::graph::{Arc, Cross, Graph, Via, dist};
use crate::nav::limits::limits;
use crate::nav::shape::{Solid, parts_of};
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

/// Each face's room, `None` for a face no named placement reaches. A room
/// spreads across anything but a door or a path `blocked` says an obstacle
/// stands on, so rooms meet at their doors, shutters and webs.
pub fn label(
    graph: &Graph,
    placed: &[Placement],
    rooms: &HashMap<u64, u32>,
    blocked: impl Fn([f32; 3], [f32; 3]) -> bool,
) -> Vec<Option<u32>> {
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
    best.into_iter().map(|b| b.map(|(_, r)| r)).collect()
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

/// One dungeon's room files, by path under `logic/`, and what derive
/// couldn't place.
pub struct Derived {
    pub files: Vec<(String, String)>,
    pub faces: usize,
    /// Faces no named placement reaches.
    pub unplaced: usize,
    /// Rooms the game numbers that got no floor.
    pub floorless: Vec<u32>,
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

/// The room files of `dungeon`, from its navmesh, its placements and
/// `kinds`.
pub fn derive(romfs: &Romfs, game: &Path, dungeon: &Dungeon, kinds: &[Kind]) -> Result<Derived> {
    let kind_of: HashMap<u32, &Kind> = kinds.iter().map(|k| (k.hash, k)).collect();
    let jump = limits().child.edge_jump.as_ref().map_or(0.0, |j| j.value);
    let mut seen: BTreeMap<(u32, u32, rooms::Via), Seen> = BTreeMap::new();
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
            let (Some(h), Some(k)) = (p.hash, kind_of.get(&murmur3(p.name.as_bytes()))) else {
                continue;
            };
            if k.sets == Sets::Opens {
                obstacles.push((h, k, parts_of(romfs, p)?));
            }
        }
        let on_path = |a: [f32; 3], b: [f32; 3]| {
            (obstacles.iter()).any(|(_, _, solids)| solids.iter().any(|s| meets(s, a, b)))
        };
        let labels = label(&graph, &level.placed, &rooms_of, on_path);
        faces += labels.len();
        unplaced += labels.iter().filter(|l| l.is_none()).count();
        floored.extend(labels.iter().flatten().copied());
        let mut note = |a: usize,
                        b: usize,
                        via: rooms::Via,
                        metric: f32,
                        by: Option<u64>,
                        path: ([f32; 3], [f32; 3])| {
            let (Some(ra), Some(rb)) = (labels[a], labels[b]) else {
                return;
            };
            if ra == rb {
                return;
            }
            let s = seen.entry((ra, rb, via)).or_default();
            s.add(metric, by);
            for (h, k, solids) in &obstacles {
                if solids.iter().any(|solid| meets(solid, path.0, path.1)) {
                    s.on.insert(*h, k);
                }
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
        let from_here = (room, 0, rooms::Via::Walk)..=(room, u32::MAX, rooms::Via::Sink);
        for (&(_, to, via), s) in seen.range(from_here) {
            let (needs, why, guess) = link(via, s);
            write!(
                out,
                "\n[[link]]\nto = {to}\nneeds = {}\nvia = {}\nwhy = {}\n",
                q(&needs),
                q(via.word()),
                q(&why)
            )?;
            if guess {
                out += "guess = true\n";
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
    })
}

/// A link's needs, why and whether it's a guess, from what was seen.
fn link(via: rooms::Via, s: &Seen) -> (String, String, bool) {
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
    let mut needs: Vec<&str> = vec![];
    for (h, k) in &s.on {
        if k.uses == "always" {
            // Opened by a flag, which the flags step finds.
            write!(why, "; {} {h} opens on a flag, not found yet", k.name).unwrap();
            guess = true;
        } else {
            write!(why, "; {} {h}: {}", k.name, k.uses).unwrap();
            guess |= k.guess;
            if !needs.contains(&k.uses.as_str()) {
                needs.push(&k.uses);
            }
        }
    }
    let needs = match &needs[..] {
        [] => "always".to_owned(),
        [one] => (*one).to_owned(),
        many => many
            .iter()
            .map(|n| format!("({n})"))
            .collect::<Vec<_>>()
            .join(" and "),
    };
    (needs, why, guess)
}

/// Writes `dungeon`'s baseline room files under the logic folder `dir`,
/// first clearing the old ones, so a renamed room leaves no stale file.
pub fn write(romfs: &Romfs, game: &Path, dungeon: &Dungeon, dir: &Path) -> Result<Derived> {
    let book = crate::rules::Book::load()?;
    let derived = derive(romfs, game, dungeon, &book.kinds)?;
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
        let labels = label(&g, &placed, &rooms, |_, _| false);
        assert_eq!(labels, [Some(1), Some(1), Some(2), Some(2), None]);
    }

    /// An obstacle at x 2 holds room 1 back, so the floor past it is room 2's.
    #[test]
    fn a_room_stops_at_an_obstacle() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let at_two = |a: [f32; 3], b: [f32; 3]| (a[0] - 2.0) * (b[0] - 2.0) < 0.0;
        let labels = label(&g, &placed, &rooms, at_two);
        assert_eq!(labels, [Some(1), Some(2), Some(2), Some(2), None]);
    }

    /// The floor between two obstacles joins the nearest room past them.
    #[test]
    fn the_floor_between_two_obstacles_joins_a_room() {
        let (m, placed, rooms) = row();
        let g = Graph::build(&m, &placed, graph::step_up());
        let between =
            |a: [f32; 3], b: [f32; 3]| [2.0, 6.0].iter().any(|x| (a[0] - x) * (b[0] - x) < 0.0);
        let labels = label(&g, &placed, &rooms, between);
        assert!(labels[1].is_some() && labels[2].is_some(), "{labels:?}");
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
        let fresh = derive(&romfs, game, deku, &book.kinds).unwrap();
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
        assert!(
            fresh.unplaced * 10 < fresh.faces,
            "{} of {}",
            fresh.unplaced,
            fresh.faces
        );
    }
}
