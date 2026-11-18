//! Dungeon entrance shuffle: which dungeon each dungeon's front door leads
//! to. A door keeps its place, its leaving scene and its rule, and takes
//! the destination of the door it stands in for. Doors are coupled: a
//! dungeon's front exit leads back out through the door that leads in.
//! Spirit Temple's hands and Ice Cavern's backdoor keep their targets.

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, Romfs};

use crate::checks::{self, actors, actors_mut, bancs, str_field};
use crate::known::{Name, field, map};
use crate::rng::Rng;
use crate::settings::DungeonEntrances;
use crate::world::node_of;

const DOOR_ACTOR: &Name = field::FIELD_AND_DUNGEON_ENTRANCE_EVENT_REQUEST_ACTOR;
const DYNAMIC: &str = "Dynamic";

/// The fields that say where a door leads. A moved door takes all of them
/// from the door it stands in for, and drops any that one lacks. The init
/// event is the destination's first-arrival scene, so it moves too.
const BLOCK: [&Name; 7] = [
    field::NEXT_BANC_PARAM_NAME_CHILD,
    field::NEXT_BANC_PARAM_NAME_ADULT,
    field::NEXT_START_POS_NAME,
    field::NEXT_EVENT_NAME,
    field::IS_ALTERNATIVE_GAME_DATA_BOOL,
    field::INIT_EVENT_NAME,
    field::INIT_EVENT_FLAG,
];

/// A dungeon and its front door.
pub struct Dungeon {
    /// As `--door` and the spoiler name it.
    pub id: &'static str,
    pub label: &'static str,
    /// Where its door stands.
    pub door: &'static str,
    pub node: &'static Name,
    /// Where the front door lands inside.
    inside: &'static Name,
    /// Where the front exit lands outside.
    outside: &'static Name,
    /// Its boss's room, which logic lets either era reach once doors move.
    pub boss: Option<&'static Name>,
}

/// Ganon's Castle last: only `on-ganon` shuffles it.
pub const DUNGEONS: [Dungeon; 12] = [
    Dungeon {
        id: "deku-tree",
        label: "Deku Tree",
        door: "Kokiri Forest",
        node: map::INSIDE_THE_DEKU_TREE,
        inside: field::ENTRANCE,
        outside: field::FRONT_DEKU_TREE,
        boss: Some(map::GOHMA_STAGE),
    },
    Dungeon {
        id: "dodongos-cavern",
        label: "Dodongo's Cavern",
        door: "Death Mountain Trail",
        node: map::DODONGO_CAVERN,
        inside: field::DODONGO_CAVERN_MOUNTAIN_TRAIL_SIDE,
        outside: field::MOUNTAIN_TRAIL_DODONGO_CAVERN_SIDE,
        boss: Some(map::DODONGO_STAGE),
    },
    Dungeon {
        id: "jabu-jabu",
        label: "Jabu-Jabu's Belly",
        door: "Zora's Fountain",
        node: map::INSIDE_JABU_JABU_BELLY,
        inside: field::INSIDE_JABU_JABU_BELLY_FROM_ZORA_FOUNTAIN,
        outside: field::ZORA_FOUNTAIN_FROM_INSIDE_JABU_JABU_BELLY,
        boss: Some(map::BARINADE_STAGE),
    },
    Dungeon {
        id: "forest-temple",
        label: "Forest Temple",
        door: "Sacred Forest Meadow",
        node: map::FOREST_TEMPLE,
        inside: map::FOREST_TEMPLE,
        outside: field::OUTSIDE_OF_FOREST_TEMPLE,
        boss: Some(map::PHANTOM_GANON_STAGE),
    },
    Dungeon {
        id: "fire-temple",
        label: "Fire Temple",
        door: "Death Mountain Crater",
        node: map::FIRE_TEMPLE,
        inside: map::FIRE_TEMPLE,
        outside: field::OUTSIDE_OF_FIRE_TEMPLE,
        boss: Some(map::VOLVAGIA_STAGE),
    },
    Dungeon {
        id: "water-temple",
        label: "Water Temple",
        door: "Lake Hylia",
        node: map::WATER_TEMPLE,
        inside: field::WATER_TEMPLE_FROM_LAKE_HYLIA,
        outside: field::OUTSIDE_OF_WATER_TEMPLE,
        boss: Some(map::MORPHA_STAGE),
    },
    Dungeon {
        id: "shadow-temple",
        label: "Shadow Temple",
        door: "Graveyard",
        node: map::SHADOW_TEMPLE,
        inside: field::ENTRANCE,
        outside: field::GRAVE_YARD_SHADOW_TEMPLE_SIDE,
        boss: Some(map::BONGO_BONGO_STAGE),
    },
    Dungeon {
        id: "spirit-temple",
        label: "Spirit Temple",
        door: "Desert Colossus",
        node: map::SPIRIT_TEMPLE,
        inside: field::SPIRIT_TEMPLE_ENTRANCE,
        outside: field::DESERT_COLOSSUS_FROM_SPIRIT_TEMPLE_ENTRANCE,
        boss: Some(map::TWINROVA_STAGE),
    },
    Dungeon {
        id: "bottom-of-the-well",
        label: "Bottom of the Well",
        door: "Kakariko Village",
        node: map::BOTTOM_OF_THE_WELL,
        inside: field::WELL_KAKARIKO_SIDE,
        outside: field::KAKARIKO_WELL_SIDE,
        boss: None,
    },
    Dungeon {
        id: "ice-cavern",
        label: "Ice Cavern",
        door: "Zora's Fountain",
        node: map::ICE_CAVERN,
        inside: field::ICE_CAVERN_FROM_ZORA_FOUNTAIN,
        outside: field::ZORA_FOUNTAIN_FROM_ICE_CAVERN,
        boss: None,
    },
    Dungeon {
        id: "gerudo-training-ground",
        label: "Gerudo Training Ground",
        door: "Gerudo's Fortress",
        node: map::GERUDO_TRAINING_GROUND,
        inside: field::GERUDO_TRAINING_GROUND_FROM_GERUDO_FORTRESS,
        outside: field::GERUDO_FORTRESS_FROM_GERUDO_TRAINING_GROUND,
        boss: None,
    },
    Dungeon {
        id: "ganons-castle",
        label: "Ganon's Castle",
        door: "Ganon's Castle grounds",
        node: map::INSIDE_GANON_CASTLE,
        inside: field::ENTRANCE,
        outside: field::GANON_CASTLE_ENTRANCE_FROM_INSIDE,
        boss: Some(map::GANONDORF_STAGE),
    },
];

/// The dungeon `id` names, by its index in [`DUNGEONS`].
fn index(id: &str) -> Result<usize> {
    DUNGEONS.iter().position(|d| d.id == id).with_context(|| {
        let ids: Vec<&str> = DUNGEONS.iter().map(|d| d.id).collect();
        format!("no dungeon {id}: one of {}", ids.join(", "))
    })
}

/// Which dungeon is behind each door. Both are named by their index in
/// [`DUNGEONS`], a door by the dungeon behind it in the vanilla game.
#[derive(Clone, Debug, PartialEq)]
pub struct Doors([usize; DUNGEONS.len()]);

impl Default for Doors {
    fn default() -> Self {
        Self(std::array::from_fn(|i| i))
    }
}

impl Doors {
    /// The doors `mode` shuffles, with each of `pins` (`DOOR=DUNGEON`)
    /// holding its dungeon and the rest rolled.
    pub fn roll(mode: DungeonEntrances, pins: &[String], rng: &mut Rng) -> Result<Self> {
        let pool: Vec<usize> = match mode {
            DungeonEntrances::Off => vec![],
            DungeonEntrances::On => (0..DUNGEONS.len() - 1).collect(),
            DungeonEntrances::OnGanon => (0..DUNGEONS.len()).collect(),
        };
        let (mut doors, mut dungeons) = (pool.clone(), pool.clone());
        let mut out = Self::default();
        for pin in pins {
            let (door, dungeon) = pin
                .split_once('=')
                .with_context(|| format!("{pin} isn't DOOR=DUNGEON"))?;
            let (door, dungeon) = (index(door)?, index(dungeon)?);
            let (Some(d), Some(x)) = (
                doors.iter().position(|&d| d == door),
                dungeons.iter().position(|&x| x == dungeon),
            ) else {
                bail!("{pin}: both must be shuffled, and each pinned once");
            };
            doors.remove(d);
            dungeons.remove(x);
            out.0[door] = dungeon;
        }
        rng.shuffle(&mut dungeons);
        for (door, dungeon) in doors.into_iter().zip(dungeons) {
            out.0[door] = dungeon;
        }
        Ok(out)
    }

    /// The dungeon behind `door`.
    pub fn behind(&self, door: usize) -> usize {
        self.0[door]
    }

    /// The door that leads into `dungeon`.
    pub fn door_of(&self, dungeon: usize) -> usize {
        self.0.iter().position(|&x| x == dungeon).unwrap_or(dungeon)
    }

    /// Each door that leads somewhere new, with the dungeon behind it.
    pub fn moved(&self) -> impl Iterator<Item = (&'static Dungeon, &'static Dungeon)> + '_ {
        (self.0.iter().enumerate())
            .filter(|(door, dungeon)| door != *dungeon)
            .map(|(door, &dungeon)| (&DUNGEONS[door], &DUNGEONS[dungeon]))
    }

    pub fn is_vanilla(&self) -> bool {
        self.moved().next().is_none()
    }

    /// Dungeons `a` and `b` swapped, and nothing else.
    #[cfg(test)]
    pub(crate) fn swapped(a: &str, b: &str) -> Self {
        let mut doors = Self::default();
        doors.0.swap(index(a).unwrap(), index(b).unwrap());
        doors
    }
}

/// What a door is to the shuffle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// The world side of a dungeon's front door.
    Into(usize),
    /// A dungeon's front exit.
    OutOf(usize),
}

/// The `Dynamic` of a door actor.
fn door_of_actor(actor: &Byml) -> Option<&Byml> {
    (str_field(actor, checks::GYAML) == Some(&**DOOR_ACTOR))
        .then(|| actor.as_map().ok()?.get(DYNAMIC))
        .flatten()
}

/// What the door with `dynamic`, in map `node`, is to the shuffle.
fn role_of_door(node: &str, dynamic: &Byml) -> Option<Role> {
    let get = |key: &Name| str_field(dynamic, key);
    let maps = [
        field::NEXT_BANC_PARAM_NAME_CHILD,
        field::NEXT_BANC_PARAM_NAME_ADULT,
    ];
    role(
        node,
        maps.iter().filter_map(|&k| get(k)),
        get(field::NEXT_START_POS_NAME)?,
    )
}

/// What a door in map `node`, into one of `to` at `start`, is to the
/// shuffle. Doors are told apart by where they land, so a dungeon's other
/// ways out (the Spirit Temple's hands, the Ice Cavern's backdoor) stay put.
pub fn role<'a>(node: &str, to: impl IntoIterator<Item = &'a str>, start: &str) -> Option<Role> {
    let to: Vec<&str> = to.into_iter().collect();
    DUNGEONS.iter().enumerate().find_map(|(i, d)| {
        if node == &**d.node {
            (start == &**d.outside).then_some(Role::OutOf(i))
        } else {
            (to.contains(&&**d.node) && start == &**d.inside).then_some(Role::Into(i))
        }
    })
}

/// A door's destination, a value or nothing for each of [`BLOCK`].
type Block = [Option<Byml>; BLOCK.len()];

fn block_of(dynamic: &Byml) -> Block {
    BLOCK.map(|key| dynamic.as_map().ok()?.get(&**key).cloned())
}

/// Each dungeon's front door's destination and its front exit's, read from
/// the data.
pub struct Blocks {
    into: Vec<Block>,
    out_of: Vec<Block>,
}

impl Blocks {
    pub fn read(romfs: &Romfs) -> Result<Self> {
        let mut into: Vec<Option<Block>> = vec![None; DUNGEONS.len()];
        let mut out_of = into.clone();
        for (file, doc) in bancs(romfs)? {
            let doc = doc?;
            let node = node_of(&file).0;
            for dynamic in actors(&doc.root)?.iter().filter_map(door_of_actor) {
                let (seen, i) = match role_of_door(&node, dynamic) {
                    Some(Role::Into(i)) => (&mut into, i),
                    Some(Role::OutOf(i)) => (&mut out_of, i),
                    None => continue,
                };
                let block = block_of(dynamic);
                // A door is placed in each era's copy of its map.
                ensure!(
                    seen[i].as_ref().is_none_or(|b| *b == block),
                    "{}'s copies lead to different places",
                    DUNGEONS[i].label
                );
                seen[i] = Some(block);
            }
        }
        let all = |seen: Vec<Option<Block>>, what: &str| -> Result<Vec<Block>> {
            (seen.into_iter().zip(&DUNGEONS))
                .map(|(b, d)| b.with_context(|| format!("no {what} for {}", d.label)))
                .collect()
        };
        Ok(Self {
            into: all(into, "front door")?,
            out_of: all(out_of, "front exit")?,
        })
    }
}

/// Points each door in Banc `file` where `doors` says. Returns how many
/// moved.
pub fn rewrite(file: &str, root: &mut Byml, doors: &Doors, blocks: &Blocks) -> Result<usize> {
    if doors.is_vanilla() {
        return Ok(0);
    }
    let node = node_of(file).0;
    let mut moved = 0;
    for actor in actors_mut(root)? {
        let Some(role) = door_of_actor(actor).and_then(|d| role_of_door(&node, d)) else {
            continue;
        };
        let block = match role {
            Role::Into(door) if doors.behind(door) != door => &blocks.into[doors.behind(door)],
            Role::OutOf(dungeon) if doors.door_of(dungeon) != dungeon => {
                &blocks.out_of[doors.door_of(dungeon)]
            }
            _ => continue,
        };
        let dynamic = (actor.as_mut_map()?)
            .get_mut(DYNAMIC)
            .context("a door with no Dynamic")?
            .as_mut_map()?;
        for (&key, value) in BLOCK.iter().zip(block) {
            let key: &str = key;
            match value {
                Some(v) => dynamic.insert(key.into(), v.clone()),
                None => dynamic.remove(key),
            };
        }
        moved += 1;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::load_banc;

    fn id(i: usize) -> &'static str {
        DUNGEONS[i].id
    }

    #[test]
    fn a_roll_deals_each_dungeon_once_and_repeats_per_seed() {
        let roll = |seed| Doors::roll(DungeonEntrances::On, &[], &mut Rng::new(seed)).unwrap();
        let doors = roll(7);
        assert_eq!(doors, roll(7));
        assert_ne!(doors, roll(8));
        let mut dealt = doors.0;
        dealt.sort();
        assert_eq!(dealt, Doors::default().0);
        let ganon = DUNGEONS.len() - 1;
        assert_eq!(doors.behind(ganon), ganon, "on keeps Ganon's Castle");
        for door in 0..DUNGEONS.len() {
            assert_eq!(doors.door_of(doors.behind(door)), door);
        }
    }

    #[test]
    fn off_moves_nothing() {
        let doors = Doors::roll(DungeonEntrances::Off, &[], &mut Rng::new(1)).unwrap();
        assert!(doors.is_vanilla());
    }

    #[test]
    fn a_pin_holds_its_dungeon() {
        let pins = [
            format!("{}={}", id(0), id(4)),
            format!("{}={}", id(4), id(0)),
        ];
        for seed in 0..20 {
            let doors = Doors::roll(DungeonEntrances::On, &pins, &mut Rng::new(seed)).unwrap();
            assert_eq!((doors.behind(0), doors.behind(4)), (4, 0));
        }
    }

    #[test]
    fn a_pin_must_be_shuffled_and_pinned_once() {
        let roll = |mode, pins: &[String]| Doors::roll(mode, pins, &mut Rng::new(1));
        let ganon = format!("{}={}", id(0), id(DUNGEONS.len() - 1));
        assert!(roll(DungeonEntrances::On, std::slice::from_ref(&ganon)).is_err());
        assert!(roll(DungeonEntrances::OnGanon, &[ganon]).is_ok());
        assert!(roll(DungeonEntrances::Off, &[format!("{}={}", id(0), id(1))]).is_err());
        let twice = [
            format!("{}={}", id(0), id(1)),
            format!("{}={}", id(2), id(1)),
        ];
        assert!(roll(DungeonEntrances::On, &twice).is_err());
        assert!(roll(DungeonEntrances::On, &["nowhere=deku-tree".into()]).is_err());
    }

    /// Each Banc file of `node`'s maps.
    fn files(romfs: &Romfs, node: &str) -> Vec<String> {
        (checks::banc_files(romfs).unwrap().into_iter())
            .filter(|f| node_of(f).0 == node)
            .collect()
    }

    /// The doors in `node`'s maps after `doors` rewrites them: each one's
    /// [`BLOCK`] fields, `-` for one it lacks.
    fn doors_in(
        romfs: &Romfs,
        node: &str,
        doors: &Doors,
        blocks: &Blocks,
    ) -> Vec<[String; BLOCK.len()]> {
        let mut out = Vec::new();
        for file in files(romfs, node) {
            let mut doc = load_banc(romfs, &file).unwrap();
            rewrite(&file, &mut doc.root, doors, blocks).unwrap();
            for d in actors(&doc.root).unwrap().iter().filter_map(door_of_actor) {
                out.push(std::array::from_fn(|i| {
                    str_field(d, BLOCK[i]).unwrap_or("-").to_owned()
                }));
            }
        }
        out
    }

    #[test]
    fn every_dungeon_has_one_front_door_and_one_front_exit() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        Blocks::read(&romfs).unwrap();
    }

    #[test]
    fn a_swapped_door_leads_into_the_other_dungeon_and_back_out() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let blocks = Blocks::read(&romfs).unwrap();
        let doors = Doors::swapped("deku-tree", "fire-temple");
        let fire = map::FIRE_TEMPLE.to_string();
        let kokiri = doors_in(&romfs, map::KOKIRI_FOREST_CHILD, &doors, &blocks);
        assert!(
            kokiri
                .iter()
                .any(|d| d[0] == fire && d[1] == fire && d[2] == fire),
            "{kokiri:?}"
        );
        let out = doors_in(&romfs, map::FIRE_TEMPLE, &doors, &blocks);
        let back = [
            map::KOKIRI_FOREST_CHILD.to_string(),
            map::KOKIRI_FOREST_ADULT.to_string(),
            field::FRONT_DEKU_TREE.to_string(),
        ];
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][..3], back[..]);
        let crater = doors_in(&romfs, map::DEATH_MOUNTAIN_CRATER_ADULT, &doors, &blocks);
        let deku = map::INSIDE_THE_DEKU_TREE.to_string();
        assert!(
            crater
                .iter()
                .any(|d| d[1] == deku && d[2] == **field::ENTRANCE)
        );
        assert!(crater.iter().all(|d| d[1] != fire), "{crater:?}");
    }

    #[test]
    fn a_moved_door_brings_its_dungeons_first_arrival_scene() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let blocks = Blocks::read(&romfs).unwrap();
        let init = BLOCK
            .iter()
            .position(|f| **f == **field::INIT_EVENT_FLAG)
            .unwrap();
        let vanilla = doors_in(
            &romfs,
            map::DEATH_MOUNTAIN_CRATER_ADULT,
            &Doors::default(),
            &blocks,
        );
        let fire = vanilla.iter().find(|d| d[2] == **map::FIRE_TEMPLE).unwrap();
        assert_ne!(fire[init], "-");
        let kokiri = doors_in(
            &romfs,
            map::KOKIRI_FOREST_CHILD,
            &Doors::swapped("deku-tree", "fire-temple"),
            &blocks,
        );
        assert!(
            kokiri
                .iter()
                .any(|d| d[2] == **map::FIRE_TEMPLE && d[init] == fire[init]),
            "{kokiri:?}"
        );
    }

    #[test]
    fn a_moved_exit_drops_the_fields_its_new_door_lacks() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let blocks = Blocks::read(&romfs).unwrap();
        let mut doors = Doors::default();
        let well = index("bottom-of-the-well").unwrap();
        doors.0.swap(0, well);
        let alternative = |node: &str| -> Vec<bool> {
            let mut out = Vec::new();
            for file in files(&romfs, node) {
                let mut doc = load_banc(&romfs, &file).unwrap();
                rewrite(&file, &mut doc.root, &doors, &blocks).unwrap();
                for d in actors(&doc.root).unwrap().iter().filter_map(door_of_actor) {
                    out.push(str_field(d, field::IS_ALTERNATIVE_GAME_DATA_BOOL).is_some());
                }
            }
            out
        };
        // The Deku Tree's exit leads to Kakariko and takes its burning copy's
        // switch. The Well's leads to Kokiri Forest and loses it.
        assert_eq!(alternative(map::INSIDE_THE_DEKU_TREE), [true]);
        assert_eq!(alternative(map::BOTTOM_OF_THE_WELL), [false]);
    }

    #[test]
    fn hands_and_backdoors_keep_their_targets() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let blocks = Blocks::read(&romfs).unwrap();
        let (spirit, ice) = (
            index("spirit-temple").unwrap(),
            index("ice-cavern").unwrap(),
        );
        let mut doors = Doors::default();
        doors.0.swap(spirit, ice);
        for (node, outside) in [
            (map::SPIRIT_TEMPLE, map::DESERT_COLOSSUS_CHILD),
            (map::ICE_CAVERN, map::ZORA_FOUNTAIN_CHILD),
        ] {
            let exits = doors_in(&romfs, node, &doors, &blocks);
            let kept = exits.iter().filter(|d| d[0] == **outside).count();
            assert_eq!(kept, exits.len() - 1, "{}: {exits:?}", &**node);
        }
    }
}
