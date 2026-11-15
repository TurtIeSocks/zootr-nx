//! Names a player can find a check by, from what's placed around it. Banc
//! files cover a whole area, so the room comes from nearby actors, never
//! from the file.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_4, TAU};

use anyhow::Result;
use ounce_formats::{Byml, Romfs};

use crate::checks::{GYAML, actors, banc_files, flat, hash_of, load_banc, str_field, vec3};
use crate::generate::Spot;
use crate::known::{Name, field};

/// House models and the house each one is, in OoTR's words.
const HOUSES: [(&str, &str); 1] = [("Land_Kokiri_House_Inside_E", "Mido's House")];
/// How far from its house model a house's chests sit.
const HOUSE_REACH: f32 = 6.0;
/// Only dungeons place these, for the map screen.
const DUNGEON: &Name = field::DUNGEON_ROOM_CENTER_ANCHOR;
const ENTRANCE: &Name = field::ENTRANCE_POS;
const ROOM: &Name = field::DEFAULT_DUNGEON_ROOM_INDEX;
/// Height off the entrance that reads as another floor.
const FLOOR: f32 = 8.0;
/// Clockwise from north, which is -z.
const COMPASS: [&str; 8] = [
    "North",
    "Northeast",
    "East",
    "Southeast",
    "South",
    "Southwest",
    "West",
    "Northwest",
];

/// Sets each placed spot's `place`. Same names in an area get numbers, in
/// room order.
pub fn fill(romfs: &Romfs, spots: &mut [Spot]) -> Result<()> {
    let wanted: HashSet<String> = spots.iter().map(|s| s.area.clone()).collect();
    let areas = read(romfs, &wanted)?;
    let mut found = Vec::new();
    for (i, spot) in spots.iter_mut().enumerate() {
        let Some(area) = areas.get(&spot.area) else {
            continue;
        };
        let Some(chest) = area.chests.get(&spot.hash) else {
            continue;
        };
        spot.place = area.name(chest.pos, &spot.chest_name);
        found.push((i, *chest));
    }
    number(spots, found);
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Chest {
    pos: [f32; 3],
    room: i32,
}

struct House {
    name: &'static str,
    pos: [f32; 3],
    rot_y: f32,
}

#[derive(Default)]
struct Area {
    chests: HashMap<u64, Chest>,
    houses: Vec<House>,
    entrance: Option<f32>,
    dungeon: bool,
    /// Sum of every actor's x and z, for the center.
    sum: [f32; 2],
    count: usize,
}

fn read(romfs: &Romfs, wanted: &HashSet<String>) -> Result<HashMap<String, Area>> {
    let mut areas: HashMap<String, Area> = HashMap::new();
    for file in banc_files(romfs)? {
        let Some(name) = file.split('/').nth(1).filter(|a| wanted.contains(*a)) else {
            continue;
        };
        let area = areas.entry(name.to_owned()).or_default();
        for actor in actors(&load_banc(romfs, &file)?.root)? {
            area.add(actor);
        }
    }
    Ok(areas)
}

impl Area {
    fn add(&mut self, actor: &Byml) {
        let Some(pos) = vec3(actor, "Translate") else {
            return;
        };
        self.sum[0] += pos[0];
        self.sum[1] += pos[2];
        self.count += 1;
        let gyaml = str_field(actor, GYAML).unwrap_or_default();
        self.dungeon |= gyaml == DUNGEON;
        if str_field(actor, "Name") == Some(ENTRANCE) {
            self.entrance.get_or_insert(pos[1]);
        }
        if let Some((_, name)) = HOUSES.iter().find(|(m, _)| gyaml.starts_with(m)) {
            let rot_y = vec3(actor, "Rotate").map_or(0.0, |r| r[1]);
            self.houses.push(House { name, pos, rot_y });
        }
        if gyaml.starts_with("Obj_TBox")
            && let Some(hash) = hash_of(actor)
        {
            let room = actor
                .as_map()
                .ok()
                .and_then(|m| m.get("Dynamic")?.as_map().ok()?.get(&**ROOM)?.as_int().ok())
                .unwrap_or_default();
            self.chests.entry(hash).or_insert(Chest { pos, room });
        }
    }

    fn name(&self, pos: [f32; 3], content: &str) -> String {
        if let Some(house) = self.house(pos) {
            return house;
        }
        let at = match self.dungeon {
            true => self.entrance.and_then(|y| floor(pos[1] - y)),
            false => Some(self.compass(pos)),
        };
        match at {
            Some(at) => format!("{at} {content}"),
            None => content.to_owned(),
        }
    }

    /// The house a chest is in, and which corner, as seen from the door.
    fn house(&self, pos: [f32; 3]) -> Option<String> {
        let house = self
            .houses
            .iter()
            .find(|h| flat(h.pos, pos) < HOUSE_REACH)?;
        let local = |p: [f32; 3]| local(house, p);
        let inside: Vec<f32> = (self.chests.values())
            .filter(|c| flat(house.pos, c.pos) < HOUSE_REACH)
            .map(|c| local(c.pos)[1])
            .collect();
        let middle = inside.iter().sum::<f32>() / inside.len() as f32;
        let [x, z] = local(pos);
        let depth = if z > middle { "Front" } else { "Back" };
        let side = if x < 0.0 { "Left" } else { "Right" };
        Some(format!("{} {depth} {side}", house.name))
    }

    fn compass(&self, pos: [f32; 3]) -> &'static str {
        let n = self.count.max(1) as f32;
        let (dx, dz) = (pos[0] - self.sum[0] / n, pos[2] - self.sum[1] / n);
        COMPASS[(dx.atan2(-dz).rem_euclid(TAU) / FRAC_PI_4).round() as usize % 8]
    }
}

fn floor(above: f32) -> Option<&'static str> {
    match above {
        a if a > FLOOR => Some("Upper"),
        a if a < -FLOOR => Some("Lower"),
        _ => None,
    }
}

/// `p` in the house's frame: x to the right and z toward the door, for
/// someone walking in. Doors face +z before rotation.
fn local(house: &House, p: [f32; 3]) -> [f32; 2] {
    let (dx, dz) = (p[0] - house.pos[0], p[2] - house.pos[2]);
    let (sin, cos) = house.rot_y.sin_cos();
    [dx * cos - dz * sin, dx * sin + dz * cos]
}

/// Numbers spots that share an area and a place, by room, then position.
fn number(spots: &mut [Spot], mut found: Vec<(usize, Chest)>) {
    let key = |c: &Chest| (c.room, c.pos[2], c.pos[0]);
    found.sort_by(|(_, a), (_, b)| {
        let (a, b) = (key(a), key(b));
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, _) in found {
        let s = &spots[i];
        groups
            .entry((s.area.clone(), s.place.clone()))
            .or_default()
            .push(i);
    }
    for group in groups.values().filter(|g| g.len() > 1) {
        for (n, &i) in group.iter().enumerate() {
            spots[i].place += &format!(" {}", n + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::flag;

    fn area() -> Area {
        Area {
            count: 1,
            ..Area::default()
        }
    }

    fn spot(area: &str, place: &str) -> Spot {
        Spot {
            area: area.into(),
            place: place.into(),
            ..Spot::default()
        }
    }

    #[test]
    fn compass_points_from_the_center_with_north_at_minus_z() {
        let a = area();
        assert_eq!(a.compass([0.0, 0.0, -10.0]), "North");
        assert_eq!(a.compass([10.0, 0.0, 0.0]), "East");
        assert_eq!(a.compass([-10.0, 0.0, 10.0]), "Southwest");
    }

    #[test]
    fn dungeons_read_floors_against_the_entrance() {
        let a = Area {
            dungeon: true,
            entrance: Some(100.0),
            ..area()
        };
        assert_eq!(a.name([0.0, 120.0, 0.0], "Compass"), "Upper Compass");
        assert_eq!(a.name([0.0, 104.0, 0.0], "Map"), "Map");
        assert_eq!(a.name([0.0, 80.0, 0.0], "Deku Stick"), "Lower Deku Stick");
    }

    #[test]
    fn clashes_get_numbers_in_room_order() {
        let mut spots = [
            spot("Forest", "Small Key"),
            spot("Forest", "Small Key"),
            spot("Forest", "Map"),
            spot("Fire", "Small Key"),
        ];
        let at = |room| Chest {
            room,
            ..Chest::default()
        };
        number(
            &mut spots,
            vec![(0, at(9)), (1, at(2)), (2, at(1)), (3, at(1))],
        );
        let places: Vec<_> = spots.iter().map(|s| s.place.as_str()).collect();
        assert_eq!(places, ["Small Key 2", "Small Key 1", "Map", "Small Key"]);
    }

    #[test]
    fn mido_s_chests_are_named_by_corner_from_the_door() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let corners = [
            (4555665598794293658, "Mido's House Front Left"),
            (12786193094758345086, "Mido's House Back Left"),
            (10831981390482426294, "Mido's House Back Right"),
            (9725242744547055868, "Mido's House Front Right"),
        ];
        let mut spots: Vec<Spot> = corners
            .iter()
            .map(|&(hash, _)| Spot {
                hash,
                ..spot(flag::KOKIRI_FOREST, "")
            })
            .collect();
        fill(&romfs, &mut spots).unwrap();
        for (spot, (_, want)) in spots.iter().zip(corners) {
            assert_eq!(spot.place, want);
        }
    }
}
