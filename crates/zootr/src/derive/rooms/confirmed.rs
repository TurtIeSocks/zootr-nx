//! The maintainer's confirmed entries that name a spot (puzzle links
//! spec): each found on the level's floor, checked, and turned into
//! what derive writes. A spot link replaces the derived arcs between the
//! same rooms (or parts of a room) by its `via`, or adds one; `needs =
//! "never"` only removes. A `[[floor]]` gives floor no room had to its
//! file's room. A flag with `at` replaces the derived flag of its hash.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};

use anyhow::{Result, bail, ensure};

use crate::nav::graph::{Cross, Graph, Snap};
use crate::rules::rooms::{Flag, FloorClaim, Point, Room, SpotLink, Via};

use super::Segment;
use super::label::Labels;
use super::locks::Lock;

/// How far under a spot its floor may be, in m (room reach rule 8).
const BELOW: f32 = 0.3;
/// A spot written to a decimal may sit this far under its floor, in m.
const ROUNDING: f32 = 0.05;

/// The face under spot `p`.
fn floor_at(graph: &Graph, p: Point) -> Option<Snap> {
    let [x, y, z] = p.0;
    graph.nearest_floor([x, y + ROUNDING, z], BELOW + ROUNDING, |_| true)
}

/// Whether a spot link costs nothing to take.
pub(super) fn free(link: &SpotLink) -> bool {
    link.needs.trim() == crate::derive::ALWAYS
}

fn warn(warnings: &mut Vec<String>, room: u32, hash: u32, setter: u32) {
    let w = format!("flag #{hash:08x}: confirmed in room {room}, but a setter is in room {setter}");
    if !warnings.contains(&w) {
        warnings.push(w);
    }
}

fn never(link: &SpotLink) -> bool {
    link.needs.trim() == "never"
}

/// A confirmed entry of room `room`, and whether any map found its spot
/// on the floor and, for a removal, a derived arc it removed.
struct Entry<'b, T> {
    room: u32,
    it: &'b T,
    found: Cell<bool>,
    /// Set by `Here::takes_room_arc` and `takes_parts_arc` when the
    /// entry's link stands in for a derived arc: `finish` needs it for a
    /// removal.
    hit: Cell<bool>,
}

impl<'b, T> Entry<'b, T> {
    fn of(room: u32, it: &'b T) -> Self {
        Self {
            room,
            it,
            found: Cell::new(false),
            hit: Cell::new(false),
        }
    }
}

impl Entry<'_, SpotLink> {
    /// The link as an error names it.
    fn what(&self) -> String {
        let l = self.it;
        format!(
            "confirmed room {}: link from {} to {}",
            self.room, l.from, l.to
        )
    }
}

/// One dungeon's confirmed spot entries.
pub(super) struct Confirmed<'b> {
    links: Vec<Entry<'b, SpotLink>>,
    flags: Vec<Entry<'b, Flag>>,
    floors: Vec<Entry<'b, FloorClaim>>,
}

/// A spot link as one map has it: its two ends' (room, part), and the
/// path between them.
pub(super) struct Placed<'b> {
    pub link: &'b SpotLink,
    pub from: (u32, usize),
    pub to: (u32, usize),
    pub at: Segment,
}

/// One map's spot links, and the derived arcs they replace.
pub(super) struct Here<'c, 'b> {
    of: &'c Confirmed<'b>,
    pub links: Vec<Placed<'b>>,
    rooms: HashMap<(u32, u32, Via), usize>,
    parts: HashMap<(usize, usize, Via), usize>,
}

impl Here<'_, '_> {
    fn replaced(&self, i: Option<&usize>) -> bool {
        i.inspect(|&&i| self.of.links[i].hit.set(true)).is_some()
    }

    /// Whether a confirmed link stands in for a derived arc from room `a`
    /// to room `b` by `via`, recording on the link that it took one.
    pub fn takes_room_arc(&self, a: u32, b: u32, via: Via) -> bool {
        self.replaced(self.rooms.get(&(a, b, via)))
    }

    /// Whether a confirmed link stands in for a derived arc from part `a`
    /// of a room to its part `b` by `via`, recording on the link that it
    /// took one.
    pub fn takes_parts_arc(&self, a: usize, b: usize, via: Via) -> bool {
        self.replaced(self.parts.get(&(a, b, via)))
    }
}

impl<'b> Confirmed<'b> {
    /// The spot entries of the rooms of the dungeon at `node`, each flag's
    /// hash one GameData has (`in_game`).
    pub fn of(rooms: &'b [Room], node: &str, in_game: impl Fn(u32) -> bool) -> Result<Self> {
        let mut out = Confirmed {
            links: vec![],
            flags: vec![],
            floors: vec![],
        };
        for r in rooms.iter().filter(|r| r.node == node) {
            let room = r.file.room;
            out.links
                .extend(r.spots.link.iter().map(|l| Entry::of(room, l)));
            out.floors
                .extend(r.spots.floor.iter().map(|f| Entry::of(room, f)));
            for f in &r.spots.flag {
                ensure!(
                    in_game(f.hash),
                    "confirmed room {room}: flag #{:08x} isn't in GameData",
                    f.hash
                );
                out.flags.push(Entry::of(room, f));
            }
        }
        Ok(out)
    }

    /// Gives each `[[floor]]`'s floor to its room: the faces in no room
    /// walk-joined to its spot.
    pub fn claim(&self, graph: &Graph, labels: &mut Labels) -> Result<()> {
        for e in &self.floors {
            let Some(snap) = floor_at(graph, e.it.at) else {
                continue;
            };
            e.found.set(true);
            if let Some(r) = labels.room[snap.face] {
                bail!(
                    "confirmed room {}: floor at {} is room {r}'s already: [[floor]] claims floor in no room",
                    e.room,
                    e.it.at
                );
            }
            let mut queue = VecDeque::from([snap.face]);
            labels.room[snap.face] = Some(e.room);
            while let Some(f) = queue.pop_front() {
                for arc in &graph.arcs[f] {
                    if matches!(arc.cross, Cross::Edge(_)) && labels.room[arc.to].is_none() {
                        labels.room[arc.to] = Some(e.room);
                        queue.push_back(arc.to);
                    }
                }
            }
        }
        Ok(())
    }

    /// The spot links whose `from` stands on this map's floor, checked.
    pub fn locate<'c>(&'c self, graph: &Graph, labels: &Labels) -> Result<Here<'c, 'b>> {
        let mut here = Here {
            of: self,
            links: vec![],
            rooms: HashMap::new(),
            parts: HashMap::new(),
        };
        for (i, e) in self.links.iter().enumerate() {
            let l = e.it;
            let Some(from) = floor_at(graph, l.from) else {
                continue;
            };
            let Some(to) = floor_at(graph, l.to) else {
                bail!("{}: `to` has no floor within {BELOW} m under it", e.what());
            };
            let (ra, rb) = (labels.room[from.face], labels.room[to.face]);
            ensure!(
                ra == Some(e.room),
                "{}: `from` is in room {ra:?}, not this file's",
                e.what()
            );
            let Some(rb) = rb else {
                bail!(
                    "{}: `to` is in no room: claim its floor with a [[floor]]",
                    e.what()
                );
            };
            let (pa, pb) = (labels.part[from.face], labels.part[to.face]);
            let twice = match e.room == rb {
                true => {
                    ensure!(
                        pa != pb,
                        "{}: both spots are one part of the room",
                        e.what()
                    );
                    here.parts.insert((pa, pb, l.via), i).is_some()
                }
                false => here.rooms.insert((e.room, rb, l.via), i).is_some(),
            };
            ensure!(
                !twice,
                "{}: a second link for those ends by {}",
                e.what(),
                l.via.word()
            );
            e.found.set(true);
            if !never(l) {
                here.links.push(Placed {
                    link: l,
                    from: (e.room, pa),
                    to: (rb, pb),
                    at: (from.at, to.at),
                });
            }
        }
        Ok(here)
    }

    /// Checks each flag with `at` whose spot is on this map's floor.
    pub fn place_flags(&self, graph: &Graph, labels: &Labels) -> Result<()> {
        for e in &self.flags {
            let Some(at) = e.it.at else { continue };
            let Some(snap) = floor_at(graph, at) else {
                continue;
            };
            let r = labels.room[snap.face];
            ensure!(
                r == Some(e.room),
                "confirmed room {}: flag {} at {at} is in room {r:?}, not this file's",
                e.room,
                e.it.word()
            );
            e.found.set(true);
        }
        Ok(())
    }

    /// Each flag with `at`, by hash, with its room.
    pub fn flags(&self) -> HashMap<u32, (u32, &'b Flag)> {
        (self.flags.iter())
            .map(|e| (e.it.hash, (e.room, e.it)))
            .collect()
    }

    /// Warns of each flag with `at` that a placement in another room sets:
    /// `setter_rooms` gives the rooms of a flag's known setters.
    pub fn writers(&self, setter_rooms: impl Fn(u32) -> Vec<u32>, warnings: &mut Vec<String>) {
        for e in &self.flags {
            for r in setter_rooms(e.it.hash).into_iter().filter(|&r| r != e.room) {
                warn(warnings, e.room, e.it.hash, r);
            }
        }
    }

    /// Each lock derive traced on a flag with `at` takes the entry's room.
    pub fn place_locks<'l>(
        &self,
        locks: impl Iterator<Item = &'l mut Lock>,
        warnings: &mut Vec<String>,
    ) {
        let placed = self.flags();
        for lock in locks {
            let Some(&(r, _)) = placed.get(&lock.hash()) else {
                continue;
            };
            if let Some(was) = lock.room.filter(|&was| was != r) {
                warn(warnings, r, lock.hash(), was);
            }
            lock.room = Some(r);
        }
    }

    /// After every map: each spot found on some floor, and each removal
    /// removed a derived arc.
    pub fn finish(&self) -> Result<()> {
        for e in &self.links {
            let l = e.it;
            ensure!(
                e.found.get(),
                "{}: `from` has no floor within {BELOW} m under it in any map",
                e.what()
            );
            ensure!(
                !never(l) || e.hit.get(),
                "{}: needs \"never\", but derive has no arc there by {} to remove",
                e.what(),
                l.via.word()
            );
        }
        for e in &self.flags {
            ensure!(
                e.found.get(),
                "confirmed room {}: flag {}: `at` has no floor within {BELOW} m under it in any map",
                e.room,
                e.it.word()
            );
        }
        for e in &self.floors {
            ensure!(
                e.found.get(),
                "confirmed room {}: [[floor]] at {} has no floor within {BELOW} m under it in any map",
                e.room,
                e.it.at
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph;
    use crate::rules::rooms::{RoomFile, Spots};

    /// Five floors in a row along x, 2 m each: rooms 1 and 2, then one
    /// in no room. Room 1 is two parts (a web between them).
    fn row() -> (ounce_formats::NavMesh, Labels) {
        let m = mesh(&[
            quad(0.0, 0.0, 2.0, 2.0, 0.0),
            quad(2.0, 0.0, 4.0, 2.0, 0.0),
            quad(4.0, 0.0, 6.0, 2.0, 0.0),
            quad(6.0, 0.0, 8.0, 2.0, 0.0),
            quad(8.0, 0.0, 10.0, 2.0, 0.0),
        ]);
        let labels = Labels {
            room: vec![Some(1), Some(1), Some(2), Some(2), None],
            part: vec![0, 1, 2, 2, 3],
        };
        (m, labels)
    }

    /// Room `n` of a fake temple, its spot entries split out as the
    /// loader does.
    fn room(n: u32, entries: &str) -> Room {
        let text = format!("room = {n}\nname = \"Fake Room\"\n{entries}");
        let mut file: RoomFile = toml::from_str(&text).unwrap();
        let (flag, rest) = std::mem::take(&mut file.flag)
            .into_iter()
            .partition(|f| f.at.is_some());
        file.flag = rest;
        let spots = Spots {
            link: std::mem::take(&mut file.spot_link),
            flag,
            floor: std::mem::take(&mut file.floor),
        };
        Room {
            node: "FakeTemple".into(),
            file,
            live: false,
            spots,
        }
    }

    fn link(from: &str, to: &str, needs: &str, via: &str) -> String {
        format!(
            "[[link]]\nfrom = \"{from}\"\nto = \"{to}\"\nneeds = \"{needs}\"\nvia = \"{via}\"\nwhy = \"fake\"\n"
        )
    }

    fn err(r: Result<impl Sized>) -> String {
        match r {
            Ok(_) => panic!("no error"),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn a_spot_link_replaces_the_arcs_between_its_rooms_or_parts_by_its_via() {
        let (m, labels) = row();
        let g = Graph::build(&m, &[], graph::step_up());
        let rooms = [room(
            1,
            &[
                link("1,0,1", "7,0,1", "always", "fall"),
                link("1,0,1", "3,0.2,1", "flag:#1a2b3c4d", "climb"),
            ]
            .concat(),
        )];
        let c = Confirmed::of(&rooms, "FakeTemple", |_| true).unwrap();
        let here = c.locate(&g, &labels).unwrap();
        assert_eq!(here.links.len(), 2);
        assert_eq!((here.links[0].from, here.links[0].to), ((1, 0), (2, 2)));
        assert!(here.takes_room_arc(1, 2, Via::Fall));
        assert!(!here.takes_room_arc(1, 2, Via::Walk), "another via stays");
        assert!(here.takes_parts_arc(0, 1, Via::Climb));
        assert!(!here.takes_parts_arc(1, 0, Via::Climb), "one way");
        c.finish().unwrap();
    }

    #[test]
    fn never_removes_an_arc_and_must_find_one() {
        let (m, labels) = row();
        let g = Graph::build(&m, &[], graph::step_up());
        let rooms = [room(1, &link("1,0,1", "5,0,1", "never", "jump"))];
        let c = Confirmed::of(&rooms, "FakeTemple", |_| true).unwrap();
        let here = c.locate(&g, &labels).unwrap();
        assert!(here.links.is_empty(), "a removal adds nothing");
        assert!(err(c.finish()).contains("no arc there by jump"));
        assert!(here.takes_room_arc(1, 2, Via::Jump));
        c.finish().unwrap();
    }

    #[test]
    fn each_spot_stands_on_floor_in_its_files_room() {
        let (m, labels) = row();
        let g = Graph::build(&m, &[], graph::step_up());
        let locate = |r: Room| {
            let rooms = [r];
            let c = Confirmed::of(&rooms, "FakeTemple", |_| true).unwrap();
            c.locate(&g, &labels).map(|_| ()).and_then(|_| c.finish())
        };
        let wrong = locate(room(2, &link("1,0,1", "5,0,1", "always", "walk")));
        assert!(err(wrong).contains("not this file's"));
        let roomless = locate(room(1, &link("1,0,1", "9,0,1", "always", "walk")));
        assert!(err(roomless).contains("claim its floor"));
        let high = locate(room(1, &link("1,0,1", "5,1,1", "always", "walk")));
        assert!(err(high).contains("no floor within 0.3 m"));
        let nowhere = locate(room(1, &link("1,5,1", "5,0,1", "always", "walk")));
        assert!(err(nowhere).contains("in any map"));
        let one_part = locate(room(2, &link("5,0,1", "7,0,1", "always", "walk")));
        assert!(err(one_part).contains("one part"));
    }

    #[test]
    fn a_floor_claim_takes_only_floor_in_no_room() {
        let (m, mut labels) = row();
        let g = Graph::build(&m, &[], graph::step_up());
        let claim = "[[floor]]\nat = \"9,0,1\"\nwhy = \"fake\"\n";
        let rooms = [room(2, claim)];
        let c = Confirmed::of(&rooms, "FakeTemple", |_| true).unwrap();
        c.claim(&g, &mut labels).unwrap();
        assert_eq!(labels.room, [Some(1), Some(1), Some(2), Some(2), Some(2)]);
        c.finish().unwrap();
        let (_, mut labels) = row();
        let taken = [room(2, &claim.replace("9,0,1", "1,0,1"))];
        let c = Confirmed::of(&taken, "FakeTemple", |_| true).unwrap();
        assert!(err(c.claim(&g, &mut labels)).contains("room 1's already"));
    }

    #[test]
    fn a_placed_flag_is_in_game_data_and_its_files_room() {
        let (m, labels) = row();
        let g = Graph::build(&m, &[], graph::step_up());
        let flag = "[[flag]]\nhash = 0x1a2b3c4d\nname = \"Fake Room Cleared\"\nat = \"5,0,1\"\nneeds = \"always\"\nwhy = \"fake fight\"\n";
        let rooms = [room(2, flag)];
        assert!(err(Confirmed::of(&rooms, "FakeTemple", |_| false)).contains("isn't in GameData"));
        let c = Confirmed::of(&rooms, "FakeTemple", |_| true).unwrap();
        c.place_flags(&g, &labels).unwrap();
        c.finish().unwrap();
        assert_eq!(c.flags()[&0x1a2b3c4d].0, 2);
        let mut warnings = vec![];
        c.writers(|_| vec![2], &mut warnings);
        assert!(warnings.is_empty(), "set in its own room");
        c.writers(|_| vec![1, 1], &mut warnings);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("a setter is in room 1"));
        let wrong = [room(1, flag)];
        let c = Confirmed::of(&wrong, "FakeTemple", |_| true).unwrap();
        assert!(err(c.place_flags(&g, &labels)).contains("not this file's"));
    }
}
