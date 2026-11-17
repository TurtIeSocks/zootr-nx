//! Planning shops and scrubs: where the keeper stands, where a customer
//! does, and what makes a scrub a seller.

use anyhow::Result;

use super::landing::across_from;
use super::{Planner, Ready, Steps, Trigger, quests, step_of};
use crate::gates::{Atom, Expr};
use crate::generate::Spot;
use crate::gifts;
use crate::known::{Name, actor, field, flag};
use crate::nav::graph::dist;
use crate::scrubs;
use crate::warp::{self, Pose, Side};
use crate::world::Presence;

/// Every shopkeeper's actor name has this, like a Goron shopkeeper's.
const KEEPER: &str = "_Shop_";
/// And starts with this: the Bombchu Shop's music emitter,
/// the market Bombchu shop's background music, stands nearer its shelf than its keeper.
const PERSON: &str = "Npc_";
/// The anchor a keeper links where his customers stand.
const CUSTOMER: &Name = field::DESTINATION_ANCHOR;
/// How far a [`CUSTOMER`] spot may be from its keeper, in m: A reached the
/// Goron keeper from 2.2 (live, 2026-11-14), his spot is 2.0 off. The
/// Kokiri keeper's link, 5.4 off, is something else.
const COUNTER: f32 = 2.5;
/// How far, flat, a keeper stands from the shelves behind them.
const KEEPER_REACH: f32 = 5.0;
/// The links a keeper names each slot he sells by.
const FOR_SALE: &str = "ForSale";
/// Set when Link has said the Hyrule Field scrub's password. Its flow asks
/// only once a field note is read, then quizzes him, and with this set it
/// goes straight to the sale. No other scrub's flow reads it.
const PASSWORD: &Name = flag::ENEMY_SCRUB_BUSINESS_SECRET_SUCCESS;
/// The Market Potion Shop's keeper. He starts the bottle tutorial as Link
/// comes near, and until it's done his talk is about the bug, never the
/// shop (live, 2026-11-15). The tutorial's own check is a gift, planned
/// elsewhere.
const POTION_KEEPER: &Name = actor::NPC_HYLIA_020_SHOP_CHILD;

impl Planner<'_> {
    /// A shop slot: talk to the keeper behind its shelf, and the judge
    /// browses to the item and buys it. The keeper is the shop NPC nearest
    /// the shelf in a map of its age: Kakariko's adult keepers stand in the
    /// alternative map, not the shelves'.
    pub(super) fn shop(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>)> {
        let maps = self.maps(&spot.area)?;
        let shelf = maps.iter().find_map(|(map, poses)| {
            Some((warp::era(map), poses.get(&spot.hash)?.at, poses.clone()))
        });
        let Some((era, shelf, slots)) = shelf else {
            let why = format!("shelf not found in {}", spot.area);
            return Ok((Trigger::Skip(why), None));
        };
        let of_era: Vec<String> = (maps.keys())
            .filter(|m| warp::era(m) == era)
            .cloned()
            .collect();
        let mut keepers = Vec::new();
        for map in of_era {
            let placed = warp::placements_in(self.romfs, &map)?;
            let shop = (placed.into_iter())
                .filter(|p| p.name.starts_with(PERSON) && p.name.contains(KEEPER));
            keepers.extend(shop.map(|p| (map.clone(), p.name, p.hash, p.pose, p.presence)));
        }
        let flat = |pose: &Pose| (pose.at[0] - shelf[0]).hypot(pose.at[2] - shelf[2]);
        let near = keepers
            .into_iter()
            .filter(|(_, _, _, pose, _)| flat(pose) < KEEPER_REACH);
        let Some((map, name, hash, pose, presence)) =
            near.min_by(|a, b| flat(&a.3).total_cmp(&flat(&b.3)))
        else {
            let why = format!("no shopkeeper by the shelf in {}", spot.area);
            return Ok((Trigger::Skip(why), None));
        };
        let (steps, mut ready) = self.bring(presence.as_ref())?;
        if name == **POTION_KEEPER {
            ready.extend(gifts::BOTTLE_TUTORIAL_DONE.map(|f| f.to_string()));
        }
        let (customer, sold) = match hash {
            Some(hash) => (
                self.customer(&map, hash, pose.at)?,
                warp::linked(self.romfs, &map, hash, Some(FOR_SALE))?,
            ),
            None => (None, vec![]),
        };
        // A keeper turned to face his shelves is talked to from across
        // them, a spot the sweep's own landing, by his front, never finds.
        // Landed there by the navmesh, Link stood at the counter's end and
        // A said nothing (Market Bazaar and Potion Shop, every round).
        let shelves: Vec<[f32; 3]> = (sold.iter())
            .filter_map(|h| Some(slots.get(h)?.at))
            .collect();
        let across = across_from(&pose, &shelves);
        let trigger = Trigger::Talk {
            at: customer.unwrap_or_else(|| warp::landing(&pose, across)),
            counter: customer.is_some() || across != Side::Front,
            map,
            face: pose.at,
            steps,
            ready,
            then: None,
            give: None,
            halt: None,
        };
        Ok((trigger, era))
    }

    /// Where a customer of the keeper placed as `hash` stands: the
    /// [`CUSTOMER`] his placement links within [`COUNTER`] of him at `at`.
    pub(super) fn customer(&self, map: &str, hash: u64, at: [f32; 3]) -> Result<Option<[f32; 3]>> {
        let links = warp::linked(self.romfs, map, hash, None)?;
        let placed = warp::placements_in(self.romfs, map)?;
        Ok((placed.into_iter())
            .filter(|p| p.name == CUSTOMER && p.hash.is_some_and(|h| links.contains(&h)))
            .map(|p| p.pose.at)
            .find(|spot| dist(*spot, at) < COUNTER))
    }

    /// What brings in a keeper there only by `presence`: the quest step
    /// its BoolExp asks for (the Bombchu Shop's keeper comes once the
    /// Goron's Ruby quest is complete), or the step after the one it asks
    /// a quest to be past (Malon at the ranch once the castle quest is past
    /// meeting her), or its Bool set true. Nothing for a keeper there while
    /// a flag is unset, or by any other expression.
    pub(super) fn bring(&mut self, presence: Option<&Presence>) -> Result<(Steps, Ready)> {
        let Some(p) = presence.filter(|p| !p.negated) else {
            return Ok(Default::default());
        };
        let gd = self.gamedata()?;
        Ok(match gd.expr(&p.flag)? {
            None => (vec![], vec![p.flag.clone()]),
            Some(expr) => {
                let past = quests(gd, &Expr::Atom(Atom::Exp(p.flag.clone())));
                (step_of(&expr).into_iter().chain(past).collect(), vec![])
            }
        })
    }

    /// The Bools that make the Business Scrub placed as `hash` a seller,
    /// with every other scrub in `map`. Until its flag is set a scrub is an
    /// enemy, beaten in play by sending its nut back: its brain graph reads
    /// the scrub's NPC flag for its own instance id. A neighbour left an
    /// enemy shoots Link mid-talk, as in the Hyrule Field grotto of three.
    pub(super) fn sellers(&self, map: &str, hash: u64) -> Result<Ready> {
        let placed = warp::placements_in(self.romfs, map)?;
        let scrubs = (placed.iter())
            .filter(|p| p.name == scrubs::ACTOR)
            .filter_map(|p| p.hash)
            .filter(|&h| h != hash);
        let flags = std::iter::once(hash).chain(scrubs);
        let mut ready: Ready = flags
            .map(|h| format!("{}.{h}", field::IS_NPC_SCRUB_BUSINESS))
            .collect();
        ready.push(PASSWORD.to_string());
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use super::super::plan;
    use super::super::tests::seed;
    use super::*;
    use crate::gamedata::GameData;
    use crate::known::{flow, map};
    use crate::{shops, steps};

    #[test]
    fn a_business_scrub_is_made_a_seller_before_its_talk() {
        let Some((romfs, _, planned)) = seed() else {
            return;
        };
        let gd = crate::gamedata::GameData::load(&romfs).unwrap();
        let mut scrubs = 0;
        for p in planned
            .iter()
            .filter(|p| p.spot.holder.starts_with(scrubs::HOLDER))
        {
            let Trigger::Talk { ready, .. } = &p.trigger else {
                continue;
            };
            scrubs += 1;
            assert_eq!(
                ready[0],
                format!("{}.{}", field::IS_NPC_SCRUB_BUSINESS, p.spot.hash)
            );
            assert!(ready.iter().any(|flag| flag == PASSWORD));
            for flag in ready {
                let hash = ounce_formats::murmur3(flag.as_bytes());
                assert_eq!(gd.kind(hash), Some("Bool"), "{flag}");
            }
        }
        assert!(scrubs > 10, "{scrubs} scrubs talk");
    }

    #[test]
    fn a_scrub_sharing_a_grotto_makes_its_neighbours_sellers_too() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        // Hyrule Field's grotto scrub selling bombs, beside the seeds and
        // nuts scrubs, which shoot Link mid-talk while they're enemies.
        let bombs = planned.iter().find(|p| p.spot.hash == 16672855544778411393);
        let Some(Trigger::Talk { ready, .. }) = bombs.map(|p| &p.trigger) else {
            panic!("{bombs:?}");
        };
        for neighbour in [12901354557574090016_u64, 17418042722045679657] {
            let flag = format!("{}.{neighbour}", field::IS_NPC_SCRUB_BUSINESS);
            assert!(ready.contains(&flag), "{ready:?}");
        }
    }

    #[test]
    fn a_shop_slot_is_bought_from_its_keeper() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let slots = shops::slots(&romfs).unwrap();
        let shops = [
            "Kokiri Shop",
            "Goron Shop",
            "Kakariko Bazaar",
            "Bombchu Shop",
        ];
        let spots: Vec<Spot> = shops
            .into_iter()
            .map(|shop| {
                let slot = slots.iter().find(|s| s.shop == shop).unwrap();
                Spot {
                    hash: slot.hash(),
                    area: slot.area.clone(),
                    holder: format!("{shop} 1"),
                    ..Default::default()
                }
            })
            .collect();
        let planned = plan(&romfs, &spots).unwrap();
        let talks: Vec<(&str, [f32; 3], &Steps)> = (planned.iter())
            .map(|p| match &p.trigger {
                Trigger::Talk {
                    map, face, steps, ..
                } => (map.as_str(), *face, steps),
                t => panic!("{t:?}"),
            })
            .collect();
        let maps: Vec<&str> = talks.iter().map(|(map, ..)| *map).collect();
        assert_eq!(
            maps,
            [
                map::KOKIRI_FOREST_CHILD,
                map::GORON_CITY_CHILD,
                map::KAKARIKO_VILLAGE_ADULT_ALTERNATIVE,
                flag::MARKET_CHILD
            ]
        );
        // Its keeper, not the music emitter standing nearer the shelf.
        let (_, bombchu, quest) = talks[3];
        assert!((bombchu[2] - 35.4).abs() < 0.1, "{bombchu:?}");
        // Who comes only once the Goron's Ruby is won.
        let ruby = (flow::GET_STONE_OF_FIRE.to_string(), "Complete".to_string());
        assert_eq!(quest, &vec![ruby]);
        let gd = GameData::load(&romfs).unwrap();
        steps::check(&gd, &quest[0].0, &quest[0].1).unwrap();
        assert!(talks[1].2.is_empty(), "the Goron keeper is always in");
        // The Goron keeper is talked to from the customer spot he links, in
        // front of his counter, and the Kakariko keeper, turned to his
        // shelves, from across them. The others from their front.
        let counters: Vec<(bool, [f32; 3])> = (planned.iter())
            .map(|p| match &p.trigger {
                Trigger::Talk { counter, at, .. } => (*counter, *at),
                t => panic!("{t:?}"),
            })
            .collect();
        let kept: Vec<bool> = counters.iter().map(|c| c.0).collect();
        assert_eq!(kept, [false, true, true, false], "{counters:?}");
        let (counter, at) = counters[1];
        assert!(counter);
        assert!(
            (at[0] - -17.1).abs() < 0.1 && (at[2] - -20.2).abs() < 0.1,
            "{at:?}"
        );
    }

    #[test]
    fn a_market_keeper_is_talked_to_across_the_counter() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let slots = shops::slots(&romfs).unwrap();
        let spots: Vec<Spot> = ["Market Bazaar", "Market Potion Shop"]
            .into_iter()
            .map(|shop| {
                let slot = slots.iter().find(|s| s.shop == shop).unwrap();
                Spot {
                    hash: slot.hash(),
                    area: slot.area.clone(),
                    holder: format!("{shop} 1"),
                    ..Default::default()
                }
            })
            .collect();
        let planned = plan(&romfs, &spots).unwrap();
        for (p, potion) in planned.iter().zip([false, true]) {
            let Trigger::Talk {
                at,
                face,
                ready,
                counter,
                ..
            } = &p.trigger
            else {
                panic!("{:?}", p.trigger);
            };
            // Both keepers stand at x 23 with their shelves behind them, so
            // a customer stands at a lower x (live, 2026-11-15).
            assert!(
                at[0] < face[0] - 0.5,
                "{}: {at:?} by {face:?}",
                p.spot.holder
            );
            // Landed as planned, not by the navmesh (live, 2026-11-16).
            assert!(counter, "{}", p.spot.holder);
            let tutorial = gifts::BOTTLE_TUTORIAL_DONE
                .iter()
                .all(|f| ready.contains(&f.to_string()));
            assert_eq!(tutorial, potion, "{}: {ready:?}", p.spot.holder);
        }
    }
}
