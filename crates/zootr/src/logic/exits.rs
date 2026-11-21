//! The exits between places: the data's, each with its rule, the rule
//! files' added ones, and the dungeon doors once they shuffle.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};

use super::{Edge, Scan, eras_of};
use crate::entrances::{self, DUNGEONS, Role};
use crate::keys;
use crate::rules::{Book, Ctx, Need};
use crate::world::{self, Era};

/// Every exit's edge, for `ctx`'s seed. `at` finds a place's index.
pub(super) fn edges(
    book: &Book,
    ctx: &Ctx,
    scan: &Scan,
    alternatives: &BTreeMap<String, String>,
    exits: &BTreeSet<world::Exit>,
    at: &impl Fn(&str, Era) -> Result<usize>,
) -> Result<Vec<Edge>> {
    let mut edges = Vec::new();
    // Exits the data has, each with its rule if the files give one.
    let rule_of = |from: &str, to: &str| book.exits.iter().find(|r| r.from == from && r.to == to);
    let mut ruled = HashSet::new();
    if let Some(stale) = book
        .alternatives
        .iter()
        .find(|a| !alternatives.contains_key(&a.place))
    {
        bail!("{} has an alternative rule but no alternative", stale.place);
    }
    let role = |e: &world::Exit| entrances::role(&e.from, [e.to.as_str()], &e.to_start);
    // Where each dungeon's front exit leads in vanilla, per era.
    let out_to: HashMap<(usize, Era), &str> = (exits.iter())
        .filter_map(|e| match role(e)? {
            Role::OutOf(d) => Some(((d, e.era), e.to.as_str())),
            Role::Into(_) => None,
        })
        .collect();
    for e in exits.iter() {
        // A moved door leads into the dungeon behind it, and that
        // dungeon's front exit back out of it. Each keeps the rule of
        // the vanilla exit it stands in, and a door adds what the dungeon
        // behind it needs inside, which its own door's rule had covered.
        let (to, inside) = match (&ctx.doors, role(e)) {
            (Some(doors), Some(Role::Into(door))) => {
                let node = &**DUNGEONS[doors.behind(door)].node;
                (node, Some(book.inside(node, ctx)?))
            }
            (Some(doors), Some(Role::OutOf(dungeon))) => {
                match out_to.get(&(doors.door_of(dungeon), e.era)) {
                    Some(to) => (*to, None),
                    None => continue,
                }
            }
            _ => (e.to.as_str(), None),
        };
        let mut need = match e.presence.as_str() {
            // Flags only ever get set, so an exit open while one is unset
            // stays open.
            "-" => Need::Always,
            flag if flag.starts_with('!') => Need::Always,
            flag => Need::Flag(flag.into()),
        };
        if let Some(rule) = rule_of(&e.from, &e.to) {
            ruled.insert((rule.from.as_str(), rule.to.as_str()));
            need = Need::All(vec![need, book.need(&rule.expr, ctx)?]);
        }
        if let Some(inside) = inside {
            need = Need::All(vec![need, inside]);
        }
        // A dungeon's copy for an era its exit's map doesn't have, like
        // child Link leaving Ganon's Castle for the adult Market.
        let to_name = to;
        let (Ok(from), Ok(to)) = (at(&e.from, e.era), at(to_name, e.era)) else {
            continue;
        };
        // Once its rule holds the game loads the _Alternative map instead.
        if alternatives.contains_key(to_name) {
            edges.push(Edge {
                from: Some(from),
                to: at(&format!("{to_name}_Alternative"), e.era)?,
                need: Need::All(vec![need.clone(), book.alternative(to_name, ctx)?]),
            });
        }
        edges.push(Edge {
            from: Some(from),
            to,
            need,
        });
    }
    for rule in &book.exits {
        if ruled.contains(&(rule.from.as_str(), rule.to.as_str())) {
            ensure!(
                !rule.added,
                "{} to {} is in the data, so not added",
                rule.from,
                rule.to
            );
            continue;
        }
        ensure!(rule.added, "no exit from {} to {}", rule.from, rule.to);
        let mut need = book.need(&rule.expr, ctx)?;
        if let Some(keys) = keys::boss_door_need(&rule.to, ctx.keys) {
            need = Need::All(vec![need, keys]);
        }
        let to_eras = eras_of(&scan.places, &rule.to);
        for era in eras_of(&scan.places, &rule.from) {
            let to_era = match to_eras.contains(&era) {
                true => era,
                false => *to_eras
                    .first()
                    .with_context(|| format!("no place {}", rule.to))?,
            };
            edges.push(Edge {
                from: Some(at(&rule.from, era)?),
                to: at(&rule.to, to_era)?,
                need: need.clone(),
            });
        }
    }
    Ok(edges)
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::HashMap;

    use ounce_formats::Romfs;

    use super::super::World;
    use crate::checks;
    use crate::entrances::Doors;
    use crate::known::{Name, actor, map};
    use crate::rules::{Ctx, Have};
    use crate::settings::DungeonEntrances;
    use crate::world::Era;

    /// A world whose dungeon doors shuffle, with `doors` rolled.
    pub(in crate::logic) fn shuffled(romfs: &Romfs, doors: Doors, on: &[&str]) -> World {
        let new_game = crate::settings::NewGame {
            dungeon_entrances: DungeonEntrances::On,
            doors,
            on: on.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let checks = checks::dealt(romfs, &new_game).unwrap();
        let book = crate::rules::Book::load().unwrap();
        World::build(romfs, &book, &Ctx::new(&new_game), &checks).unwrap()
    }

    /// Each need on an edge from `from` to `to`, written out.
    fn needs(world: &World, from: (&Name, Era), to: (&Name, Era)) -> Vec<String> {
        let place = |(node, era): (&Name, Era)| (node.to_string(), era);
        let (from, to) = (place(from), place(to));
        (world.edges.iter())
            .filter(|e| e.from.is_some_and(|f| world.places[f] == from))
            .filter(|e| world.places[e.to] == to)
            .map(|e| e.need.to_string())
            .collect()
    }

    #[test]
    fn a_moved_door_keeps_its_rule_adds_the_inside_and_leads_back_out() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = shuffled(&romfs, Doors::default(), &[]);
        let moved = shuffled(&romfs, Doors::swapped("deku-tree", "fire-temple"), &[]);
        let kokiri = (map::KOKIRI_FOREST_CHILD, Era::Child);
        let deku = (map::INSIDE_THE_DEKU_TREE, Era::Child);
        let fire = (map::FIRE_TEMPLE, Era::Child);
        let rule = needs(&vanilla, kokiri, deku);
        assert_eq!(rule.len(), 1);
        // The Deku Tree's door rule, then the Fire Temple's heat.
        let [fire_door] = &needs(&moved, kokiri, fire)[..] else {
            panic!("one door into the Fire Temple");
        };
        assert!(fire_door.starts_with(&rule[0]), "{fire_door}");
        assert!(
            fire_door.contains(actor::ARMOR_ADULT_GORON.get()),
            "{fire_door}"
        );
        assert!(needs(&moved, kokiri, deku).is_empty());
        assert!(!needs(&moved, fire, kokiri).is_empty());
        let crater = (map::DEATH_MOUNTAIN_CRATER_ADULT, Era::Adult);
        let adult_deku = (map::INSIDE_THE_DEKU_TREE, Era::Adult);
        assert!(!needs(&moved, crater, adult_deku).is_empty());
        assert!(!needs(&moved, adult_deku, crater).is_empty());
    }

    #[test]
    fn the_water_temple_behind_jabu_jabus_door_cant_be_beaten() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let beaten = |doors: Doors| {
            let new_game = crate::settings::NewGame {
                dungeon_entrances: DungeonEntrances::On,
                doors,
                ..Default::default()
            };
            let checks = checks::dealt(&romfs, &new_game).unwrap();
            let book = crate::rules::Book::load().unwrap();
            let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
            // Every item the checks hold, as fill gets them: the rewards
            // stay with their bosses.
            let pool = checks.iter().map(|c| c.item.as_str());
            let all = crate::fill::everything(&crate::fill::start(&new_game), pool);
            world.beaten(&world.play(&all, &HashMap::new()).have)
        };
        assert!(beaten(Doors::default()));
        assert!(!beaten(Doors::swapped("jabu-jabu", "water-temple")));
    }

    #[test]
    fn a_dungeon_behind_a_moved_door_needs_what_it_needs_inside() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let doors = Doors::swapped("deku-tree", "water-temple");
        let world = shuffled(&romfs, doors, &[crate::presets::ADULT_DEKU_TREE]);
        let place =
            |node: &Name, era| (world.places.iter()).position(|p| *p == (node.to_string(), era));
        let into = |era| {
            let (from, to) = (
                place(map::KOKIRI_FOREST_CHILD, era),
                place(map::WATER_TEMPLE, era),
            );
            let from = from.or(place(map::KOKIRI_FOREST_ADULT, era));
            (world.edges.iter())
                .filter(move |e| e.from == from && Some(e.to) == to)
                .collect::<Vec<_>>()
        };
        let mut have = Have::default();
        for word in ["item:hookshot", "item:zora-tunic"] {
            have.add(&crate::rules::vocab::id(word).unwrap());
        }
        let open = |have: &Have, era| into(era).iter().any(|e| e.need.met(have, era));
        assert!(!open(&have, Era::Adult), "no Iron Boots");
        have.add(&crate::rules::vocab::id("item:iron-boots").unwrap());
        assert!(open(&have, Era::Adult));
        assert!(!open(&have, Era::Child), "a child wears neither");
    }
}
