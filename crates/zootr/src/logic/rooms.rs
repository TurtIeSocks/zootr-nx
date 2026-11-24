//! A live dungeon's rooms as places of their own. The room Link enters by
//! keeps the dungeon's place, so every door into the dungeon still lands
//! there; each other room is a place named after it, joined by the room
//! files' links.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result};

use crate::rules::rooms::{Room, spot};
use crate::world;

/// Each live dungeon's rooms, by its node.
#[derive(Default)]
pub(super) struct RoomPlaces {
    dungeons: HashMap<String, Rooms>,
    /// Each live check's dungeon node and room, by hash.
    checks: HashMap<u64, (String, u32)>,
}

struct Rooms {
    /// The room an exit from outside lands in: the dungeon's own place.
    entrance: u32,
    every: BTreeSet<u32>,
    /// The room each exit leaves from, and each spot is in, by key.
    exits: HashMap<u32, u32>,
    arrivals: HashMap<u32, u32>,
}

impl RoomPlaces {
    /// The places of the live rooms in `rooms`. `exits` finds each
    /// dungeon's entrance: the room holding a spot an outside exit lands on.
    pub fn new<'a>(
        rooms: impl IntoIterator<Item = &'a Room>,
        exits: &BTreeSet<world::Exit>,
    ) -> Result<Self> {
        let mut by_node: BTreeMap<&str, Vec<&Room>> = BTreeMap::new();
        for room in rooms.into_iter().filter(|r| r.live) {
            by_node.entry(&room.node).or_default().push(room);
        }
        let mut out = Self::default();
        for (node, rooms) in by_node {
            let mut exits_at = HashMap::new();
            let mut arrivals = HashMap::new();
            for r in &rooms {
                let n = r.file.room;
                exits_at.extend(r.file.exit.iter().map(|s| (s.hash, n)));
                arrivals.extend(r.file.arrival.iter().map(|s| (s.hash, n)));
                for c in &r.file.check {
                    out.checks.insert(c.hash, (node.to_owned(), n));
                }
            }
            let entrance = (exits.iter())
                .filter(|e| e.to == node && e.from != node)
                .find_map(|e| arrivals.get(&spot(&e.to, &e.to_start)).copied())
                .with_context(|| format!("no exit into {node} lands in one of its rooms"))?;
            let every = rooms.iter().map(|r| r.file.room).collect();
            let dungeon = Rooms {
                entrance,
                every,
                exits: exits_at,
                arrivals,
            };
            out.dungeons.insert(node.to_owned(), dungeon);
        }
        Ok(out)
    }

    /// The place node of `room` in the dungeon at `node`.
    pub fn node(&self, node: &str, room: u32) -> String {
        match self.dungeons.get(node) {
            Some(d) if d.entrance != room => format!("{node}#{room:02}"),
            _ => node.to_owned(),
        }
    }

    /// The place nodes the rooms add, with the dungeon node each stands in.
    pub fn added(&self) -> impl Iterator<Item = (String, &str)> {
        (self.dungeons.iter()).flat_map(move |(node, d)| {
            (d.every.iter())
                .filter(|r| **r != d.entrance)
                .map(move |r| (self.node(node, *r), node.as_str()))
        })
    }

    /// Where an exit from `from` to `to`'s spot `start` leaves from.
    pub fn leave(&self, from: &str, to: &str, start: &str) -> Result<String> {
        let Some(d) = self.dungeons.get(from) else {
            return Ok(from.to_owned());
        };
        let room = d.exits.get(&spot(to, start));
        let room = room.with_context(|| format!("the exit from {from} to {to} is in no room"))?;
        Ok(self.node(from, *room))
    }

    /// Where an exit to `to`'s spot `start` lands.
    pub fn arrive(&self, to: &str, start: &str) -> Result<String> {
        let Some(d) = self.dungeons.get(to) else {
            return Ok(to.to_owned());
        };
        let room = d.arrivals.get(&spot(to, start));
        let room = room.with_context(|| format!("{to}'s spot {start} is in no room"))?;
        Ok(self.node(to, *room))
    }

    /// The place node of a live check's room, and its dungeon's node.
    pub fn check(&self, hash: u64) -> Option<(String, &str)> {
        let (node, room) = self.checks.get(&hash)?;
        Some((self.node(node, *room), node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::rooms::RoomFile;
    use crate::world::Era;

    fn room(node: &str, text: &str) -> Room {
        let file: RoomFile = toml::from_str(text).unwrap();
        Room {
            node: node.into(),
            file,
            live: true,
            spots: Default::default(),
        }
    }

    fn exit(from: &str, to: &str, start: &str) -> world::Exit {
        world::Exit {
            from: from.into(),
            era: Era::Child,
            to: to.into(),
            to_start: start.into(),
            name: None,
            presence: None,
        }
    }

    #[test]
    fn a_dungeon_is_entered_in_one_room_and_left_from_another() {
        let hall = format!(
            "room = 3\nname = \"Hall\"\n[[arrival]]\nhash = {}\nname = \"in\"\n",
            spot("FakeDungeon", "Front")
        );
        let boss = format!(
            "room = 7\nname = \"Lair\"\n[[exit]]\nhash = {}\nname = \"boss\"\n\
             [[check]]\nhash = 1\nname = \"Fake Chest\"\nneeds = \"always\"\nwhy = \"\"\n",
            spot("FakeBoss", world::NO_START_POS)
        );
        let rooms = [room("FakeDungeon", &hall), room("FakeDungeon", &boss)];
        let exits = BTreeSet::from([exit("FakeField", "FakeDungeon", "Front")]);
        let places = RoomPlaces::new(&rooms, &exits).unwrap();
        // The hall keeps the dungeon's place, the lair is its own.
        assert_eq!(
            places.arrive("FakeDungeon", "Front").unwrap(),
            "FakeDungeon"
        );
        assert_eq!(
            places
                .leave("FakeDungeon", "FakeBoss", world::NO_START_POS)
                .unwrap(),
            "FakeDungeon#07"
        );
        assert_eq!(
            places.check(1),
            Some(("FakeDungeon#07".into(), "FakeDungeon"))
        );
        let added: Vec<_> = places.added().collect();
        assert_eq!(added, [("FakeDungeon#07".to_owned(), "FakeDungeon")]);
        // Outside a live dungeon nothing moves, and an exit in no room is an error.
        assert_eq!(
            places.leave("FakeField", "FakeDungeon", "Front").unwrap(),
            "FakeField"
        );
        assert!(places.leave("FakeDungeon", "FakeField", "Back").is_err());
    }

    #[test]
    fn a_live_deku_tree_leaves_for_gohma_from_her_room() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load()
            .unwrap()
            .with_live("inside-the-deku-tree");
        let new_game = crate::settings::NewGame::default();
        let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        let ctx = crate::rules::Ctx::new(&new_game);
        let world = super::super::World::build(&romfs, &book, &ctx, &checks).unwrap();
        let deku = crate::known::map::INSIDE_THE_DEKU_TREE.get();
        let dungeon = (crate::entrances::DUNGEONS.iter()).find(|d| d.node.get() == deku);
        let boss = dungeon.and_then(|d| d.boss).unwrap().get();
        let name = |i: usize| world.places[i].0.as_str();
        let into_boss: BTreeSet<&str> = (world.edges.iter())
            .filter(|e| name(e.to) == boss)
            .filter_map(|e| e.from.map(name))
            .collect();
        assert_eq!(into_boss, BTreeSet::from([format!("{deku}#10").as_str()]));
        // Lower Three Deku Sticks waits in the basement, room 4.
        let sticks = (world.locations.iter()).find(|l| l.hash == 12889104996486369065);
        assert_eq!(name(sticks.unwrap().place), format!("{deku}#04"));
    }

    #[test]
    fn a_live_rooms_gold_skulltula_takes_its_rule_not_the_blanket_one() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load()
            .unwrap()
            .with_live("inside-the-deku-tree");
        let new_game = crate::settings::NewGame {
            skulltulas: true,
            ..Default::default()
        };
        let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        let ctx = crate::rules::Ctx::new(&new_game);
        let world = super::super::World::build(&romfs, &book, &ctx, &checks).unwrap();
        // The basement vine one: a slingshot hits it, which the blanket
        // word (boomerang or hookshot) would leave out.
        let vine = (world.locations.iter()).find(|l| l.hash == 16101480770659370603);
        let need = format!("{:?}", vine.unwrap().need);
        let slingshot = crate::known::item::RANGE_SLING_SHOT.get();
        assert!(need.contains(slingshot), "{need}");
    }
}
