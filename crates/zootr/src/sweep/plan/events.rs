//! Planning gifts and scenes: how each giver is talked to, played to or
//! won from, with the tables of what the sweep sets up first.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use anyhow::Result;

use super::{Halt, NPC, Planner, Ready, Trigger, anchor, steps};
use crate::cutscenes::Catalogue;
use crate::gifts::{self, Gift, How};
use crate::known::{Name, actor, field, flag, flow, item, scene};
use crate::settings::DUNGEON_REWARDS;
use crate::warp::{self, Pose, Side};
use crate::{flows, gates, rsdb};

const SHEIK: &str = "Sheik";

/// Gifts the sweep leaves alone, by the start of their flow, and what kind
/// each is.
static OUT_OF_SCOPE: LazyLock<[(&str, &str); 10]> = LazyLock::new(|| {
    [
        (flow::MINI_GAME_FISHING, "fishing"),
        (flow::MINI_GAME_BOMBCHU_BOWLING, "bowling"),
        (flow::MINIGAME_HORSEBACK_ARCHERY, "archery"),
        ("MiniGame_", "minigame"),
        ("Minigame_", "minigame"),
        (
            actor::NPC_SKULL_KID_CHILD.trim_end_matches("_Child"),
            "minigame",
        ),
        (
            actor::NPC_HYLIA_DAMPE_001.trim_end_matches("_001"),
            "the dig",
        ),
        (flow::GATHERING_TOKEN_GOLD, "token reward"),
        ("SubEvent_", "quest chain"),
        ("Dm_", "cutscene"),
    ]
});

/// Givers who teach only once Link takes his ocarina out after their
/// talk, by the gift's caller or flow, with the giver and the button that
/// does it: the Windmill Man's Song of Storms (Mac, 2026-11-14). His flow
/// isn't an event, so asked by name nothing plays, and he stands in the
/// adult village only. Malon's song, asked by name, was refused, and
/// beside her at the ranch her talk loops until Link draws it (Mac,
/// 2026-11-17).
const DRAWN: [(&Name, &Name, &str); 2] = [
    (
        actor::NPC_HYLIA_009_ADULT,
        actor::NPC_HYLIA_009_ADULT,
        "Left",
    ),
    (
        scene::MELODY_GET_EPONA,
        actor::NPC_HYLIA_MALON_CHILD,
        "Left",
    ),
];

/// Gifts a challenge wins, as `(flow, event, values)`: the values a won
/// challenge leaves, set before the event the game plays as it ends. The
/// Lake Scientist hands over his gift once Link has dived 9 deep, as long as
/// he hasn't yet (`HasDived` is the gift given, Mac, 2026-11-14).
static WON: LazyLock<[(&Name, &Name, [String; 2]); 1]> = LazyLock::new(|| {
    [(
        actor::NPC_HYLIA_045_CHILD,
        actor::NPC_HYLIA_045_CHILD_DIVING_CHALLENGE,
        [
            format!("{}=false", flag::NPC_HYLIA_045_HAS_DIVED),
            format!("{}=9", flag::NPC_HYLIA_045_DEPTH_OF_DIVING),
        ],
    )]
});

/// A giver who hands over a gift at a sale, as `(flow, gift, values, item)`:
/// the gift's vanilla item, the values the last sale finds and the item it
/// takes.
type Sold = (&'static Name, &'static Name, [String; 1], &'static Name);

/// Givers who hand over their gift at a sale. The Poe collector gives his
/// bottle at the sale that reaches 500 points and a Gold Rupee at the one
/// that reaches 1000, 100 a Big Poe, so the last sale finds 400, then 900.
static SOLD: LazyLock<[Sold; 2]> = LazyLock::new(|| {
    let poes = |points| [format!("{}={points}", flag::GHOST_SHOP_COUPON_NUM)];
    [
        (
            actor::NPC_HYLIA_048_BOTTLE_SHOP,
            item::BOTTLE,
            poes(400),
            item::BOTTLE_BIG_POE,
        ),
        (
            actor::NPC_HYLIA_048_BOTTLE_SHOP,
            actor::OBJ_RUPEE_GOLD,
            poes(900),
            item::BOTTLE_BIG_POE,
        ),
    ]
});

/// Givers the sweep stops first, as `(flow, halt, values, link, giver)`:
/// the values that let him be stopped again and give again, and the `Name`s
/// of the anchors his gift stands Link and him at. Link warps near the
/// first and walks into reach of the second: once stopped, the rolling
/// Goron stands up there (Mac, 2026-11-15: talked to from 1.5 m, his bomb
/// bag given).
/// A halted giver: flow, halt, values, link spot, his spot.
type Halted = (
    &'static Name,
    Halt,
    [String; 3],
    &'static Name,
    &'static Name,
);
static HALTED: LazyLock<[Halted; 1]> = LazyLock::new(|| {
    [(
        actor::NPC_GORON_006_CHILD,
        Halt {
            actor: actor::NPC_GORON_006_CHILD,
            passing: flag::GORON_CITY_STOP_GORON006,
            hit: flag::GORON_CITY_BOMB_HITS_GORON006,
        },
        [
            format!("{}=false", flag::GORON_CITY_BOMB_HITS_GORON006),
            format!("{}=0", field::GORON_CITY_GORON006_BOMB_HITS_WRONG_AREA),
            format!("{}=false", flag::GORON_CITY_GET_BOMB_BAG_FROM_GORON006),
        ],
        field::GORON_CITY_GET_BOMB_BAG_LINK_POSITION,
        field::GORON_CITY_GET_BOMB_BAG_GORON_POSITION,
    )]
});

impl Planner<'_> {
    pub(super) fn gift(&mut self, gift: &Gift) -> Result<(Trigger, Option<bool>)> {
        let drawn = (DRAWN.iter()).find(|(by, ..)| gift.caller() == Some(by) || gift.flow == *by);
        if let Some(&(_, giver, button)) = drawn {
            return self.talk(gift, giver, Some(button));
        }
        if gift.item.starts_with("Item_Melody_") || gift.giver.contains("Great Fairy") {
            return self.play(gift);
        }
        let kind = (gift.setting() == Some(DUNGEON_REWARDS))
            .then_some("boss reward")
            .or_else(|| {
                let of = OUT_OF_SCOPE.iter().find(|(p, _)| gift.flow.starts_with(p));
                of.map(|(_, kind)| *kind)
            });
        if let Some(kind) = kind {
            return Ok((Trigger::Skip(format!("out of scope: {kind}")), None));
        }
        if let Some((_, event, values)) = WON.iter().find(|(flow, ..)| *flow == gift.flow) {
            let values: Vec<&str> = values.iter().map(String::as_str).collect();
            return self.won(gift, event, &values);
        }
        if !gift.flow.starts_with(NPC) {
            let why = format!("no trigger: {} isn't an NPC", gift.flow);
            return Ok((Trigger::Skip(why), None));
        }
        self.talk(gift, gift.flow, None)
    }

    /// Beside `giver`, where the Banc folder of the gift's area places it,
    /// pressing `then` once the talk ends.
    pub(super) fn talk(
        &mut self,
        gift: &Gift,
        giver: &str,
        then: Option<&'static str>,
    ) -> Result<(Trigger, Option<bool>)> {
        let Some((map, pose)) = self.giver(giver, gift.area)? else {
            let why = format!("no trigger: {giver} isn't placed in {}", gift.area);
            return Ok((Trigger::Skip(why), None));
        };
        let child = warp::era(&map);
        // A giver there only past a quest step, as Malon at the ranch.
        let placed = warp::placements_in(self.romfs, &map)?;
        let presence = placed
            .into_iter()
            .find(|p| p.name == giver)
            .and_then(|p| p.presence);
        let mut steps = steps(gift);
        steps.extend(self.bring(presence.as_ref())?.0);
        let sold = (SOLD.iter()).find(|(flow, item, ..)| *flow == giver && *item == gift.item);
        let mut ready: Ready = sold.map_or(vec![], |s| s.2.iter().map(|v| v.to_string()).collect());
        let halted = HALTED.iter().find(|(flow, ..)| *flow == giver);
        let (at, face, halt) = match halted {
            Some((_, halt, values, link, him)) => {
                ready.extend(values.iter().map(|v| v.to_string()));
                let anchor = |label| anchor(self.romfs, &map, label);
                (anchor(link)?, anchor(him)?, Some(*halt))
            }
            None => (warp::landing(&pose, Side::Front), pose.at, None),
        };
        let trigger = Trigger::Talk {
            at,
            face,
            steps,
            ready,
            then,
            give: sold.map(|s| s.3.get()),
            counter: false,
            halt,
            map,
        };
        Ok((trigger, child))
    }

    /// The challenge's end, played beside the giver with `values` set.
    pub(super) fn won(
        &mut self,
        gift: &Gift,
        event: &str,
        values: &[&str],
    ) -> Result<(Trigger, Option<bool>)> {
        let Some((map, pose)) = self.giver(gift.flow, gift.area)? else {
            let why = format!("no trigger: {} isn't placed in {}", gift.flow, gift.area);
            return Ok((Trigger::Skip(why), None));
        };
        let child = warp::era(&map);
        let trigger = Trigger::Play {
            map: Some(map),
            event: event.into(),
            steps: steps(gift),
            ready: values.iter().map(|v| v.to_string()).collect(),
            melody: None,
            at: Some(warp::landing(&pose, Side::Front)),
        };
        Ok((trigger, child))
    }

    /// The map that places `giver` in `area`, and where.
    pub(super) fn giver(&mut self, giver: &str, area: &str) -> Result<Option<(String, Pose)>> {
        let placed = warp::placements(self.romfs, giver)?;
        let maps = self.maps(area)?;
        Ok(placed.into_iter().find(|(map, _)| maps.contains_key(map)))
    }

    /// A song or a Great Fairy: the event that starts its scene.
    pub(super) fn play(&mut self, gift: &Gift) -> Result<(Trigger, Option<bool>)> {
        let Some(event) = self.event(gift)? else {
            let why = format!("no trigger: {} has no caller or event setting", gift.flow);
            return Ok((Trigger::Skip(why), None));
        };
        let maps = self.maps(gift.area)?;
        let has = |era| maps.keys().any(|m| warp::era(m) == Some(era));
        let child = match (gift.giver.starts_with(SHEIK), has(true), has(false)) {
            (true, _, _) => Some(false),
            (_, true, _) => Some(true),
            (_, false, true) => Some(false),
            _ => None,
        };
        let of_era = |era| maps.keys().find(|m| warp::era(m) == era);
        let map = of_era(child).or_else(|| of_era(None)).cloned();
        let at = match &map {
            Some(map) => self.step_in(gift.area, map, &event)?,
            None => None,
        };
        let trigger = Trigger::Play {
            map,
            event,
            steps: steps(gift),
            ready: vec![],
            melody: None,
            at,
        };
        Ok((trigger, child))
    }

    /// Where in `map` of `area` Link steps in for `event` to start itself:
    /// the sensor area of its request actor nearest that actor.
    pub(super) fn step_in(
        &mut self,
        area: &str,
        map: &str,
        event: &str,
    ) -> Result<Option<[f32; 3]>> {
        let Some((Some(hash), request)) = warp::requested(self.romfs, event)?.remove(map) else {
            return Ok(None);
        };
        if self.sensors.is_none() {
            self.sensors = Some(gates::sensors(self.romfs)?);
        }
        let areas = (self.sensors.as_ref().unwrap().get(&hash).cloned()).unwrap_or_default();
        let poses = &self.maps(area)?[map];
        let off = |at: &[f32; 3]| (at[0] - request.at[0]).hypot(at[2] - request.at[2]);
        let near = areas.iter().filter_map(|h| Some(poses.get(h)?.at));
        Ok(near.min_by(|a, b| off(a).total_cmp(&off(b))))
    }

    /// The event to play for `gift`: the flow its grant waits in, else the
    /// event that starts its scene in the flow a skip moves the grant to,
    /// else the one flow that starts only its scene, else the scene's own
    /// event.
    pub(super) fn event(&mut self, gift: &Gift) -> Result<Option<String>> {
        if let How::Later { caller, .. } = gift.how {
            return Ok(Some(caller.to_string()));
        }
        if let Some(event) = self.skipped_in(gift)? {
            return Ok(Some(event));
        }
        if self.catalogue.is_none() {
            self.catalogue = Some(Catalogue::load(self.romfs)?);
        }
        let all = &self.catalogue.as_ref().unwrap().0;
        let scene = all.iter().find(|c| c.name == gift.flow);
        let alone = |f: &&String| all.iter().filter(|c| c.flows.contains(*f)).count() == 1;
        let mut callers = scene.iter().flat_map(|c| &c.flows).filter(alone);
        if let (Some(only), None) = (callers.next(), callers.next()) {
            return Ok(Some(only.clone()));
        }
        Ok(self
            .events()?
            .contains_key(&**gift.flow)
            .then(|| gift.flow.to_string()))
    }

    /// The one event whose entry point, in the flow a cutscene skip moves
    /// `gift`'s grant to, starts its scene. Playing the scene alone grants
    /// what it always did: Sheik's Bolero, where the seed's grant was in
    /// the Fire sage's melody entry (the other machine, 2026-11-14).
    pub(super) fn skipped_in(&mut self, gift: &Gift) -> Result<Option<String>> {
        let Some(caller) = gift.caller() else {
            return Ok(None);
        };
        let Some(flow) = flows::load(self.romfs, caller)? else {
            return Ok(None);
        };
        let (plays, _) = gifts::scene_marks(&flow, gift.flow);
        let starts = |entry: &Option<String>| {
            entry.as_deref().is_some_and(|entry| {
                flows::reachable(&flow, entry)
                    .is_ok_and(|seen| plays.iter().any(|p| seen.contains(p)))
            })
        };
        let mut events = (self.events()?.iter())
            .filter(|(_, (f, entry))| f == caller && starts(entry))
            .map(|(name, _)| name);
        Ok(match (events.next(), events.next()) {
            (Some(only), None) => Some(only.clone()),
            _ => None,
        })
    }

    /// Every event the game can start by name, with its flow file and entry
    /// point.
    pub(super) fn events(&mut self) -> Result<&BTreeMap<String, rsdb::Runs>> {
        if self.events.is_none() {
            self.events = Some(rsdb::events(self.romfs)?);
        }
        Ok(self.events.as_ref().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::seed;
    use super::super::{Planned, preset};
    use super::*;
    use crate::known::{map, scene};
    use crate::nav::graph::dist;

    fn songs_and_fairies(planned: &[Planned]) -> Vec<&Planned> {
        let of = |p: &&Planned| {
            p.spot.vanilla.starts_with("Item_Melody_") || p.spot.holder.contains("Great Fairy")
        };
        planned.iter().filter(of).collect()
    }

    #[test]
    fn a_sheik_song_plays_the_event_its_skip_moves_the_grant_to() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let play = |giver: &str| {
            let p = planned.iter().find(|p| p.spot.holder == giver).unwrap();
            match &p.trigger {
                Trigger::Play { event, at, .. } => (event.clone(), *at),
                other => panic!("{other:?}"),
            }
        };
        let (event, at) = play("Sheik in the Crater");
        assert_eq!(event, field::SAGE_OF_FIRE_MELODY_GET_FIRE);
        // The sensor area of its request actor, where his step starts it.
        assert_eq!(
            at.map(|[x, _, z]| (x.round(), z.round())),
            Some((-5.0, -11.0))
        );
        let (event, at) = play("Sheik at the Colossus");
        assert_eq!(event, scene::MELODY_GET_SPILIT);
        assert_eq!(
            at.map(|[x, _, z]| (x.round(), z.round())),
            Some((-161.0, 10.0))
        );
        let (_, at) = play("Sheik in the Meadow");
        assert_eq!(
            at.map(|[x, _, z]| (x.round(), z.round())),
            Some((-2.0, -102.0))
        );
        // The scene alone grants the vanilla song where a skip moved the
        // grant to its caller. The Colossus scene has no caller.
        for p in planned
            .iter()
            .filter(|p| p.spot.holder.starts_with("Sheik"))
            .filter(|p| p.spot.holder != "Sheik at the Colossus")
        {
            if let Trigger::Play { event, .. } = &p.trigger {
                assert!(!event.starts_with("Dm_MelodyGet_"), "{event}");
            }
        }
    }

    #[test]
    fn the_poe_collector_is_met_with_his_last_big_poe() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        for (holder, points) in [
            ("Poe collector", "400"),
            ("Poe collector: 1000 points", "900"),
        ] {
            let p = (planned.iter()).find(|p| p.spot.holder == holder).unwrap();
            let Trigger::Talk {
                map, ready, give, ..
            } = &p.trigger
            else {
                panic!("{holder} {:?}", p.trigger);
            };
            assert_eq!(map, flag::MARKET_ADULT);
            let set: Vec<_> = ready.iter().map(|e| preset(e)).collect();
            assert_eq!(set, [(&**flag::GHOST_SHOP_COUPON_NUM, points)]);
            assert_eq!(*give, Some(&**item::BOTTLE_BIG_POE));
        }
    }

    #[test]
    fn the_rolling_goron_is_stopped_then_talked_to_from_his_gift_spot() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let p = (planned.iter())
            .find(|p| p.spot.holder == "Rolling Goron")
            .unwrap();
        let Trigger::Talk {
            map,
            at,
            face,
            ready,
            counter,
            halt,
            ..
        } = &p.trigger
        else {
            panic!("{:?}", p.trigger);
        };
        assert_eq!(map, map::GORON_CITY_CHILD);
        assert_eq!(halt.unwrap().hit, flag::GORON_CITY_BOMB_HITS_GORON006);
        assert!(!*counter, "Link walks into reach of him");
        // The anchors his bomb bag scene stands Link and him at.
        let near = |a: [f32; 3], b: [f32; 3]| dist(a, b) < 0.1;
        assert!(near(*at, [8.0, 109.6, -17.2]), "{at:?}");
        assert!(near(*face, [7.0, 109.6, -19.0]), "{face:?}");
        let set: Vec<_> = ready.iter().map(|e| preset(e)).collect();
        assert!(set.contains(&(flag::GORON_CITY_BOMB_HITS_GORON006, "false")));
    }

    #[test]
    fn the_lake_scientist_plays_his_dive_won_beside_him() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let p = (planned.iter())
            .find(|p| p.spot.holder == "Lake Scientist")
            .unwrap();
        let Trigger::Play {
            map,
            event,
            ready,
            at: Some([x, _, z]),
            ..
        } = &p.trigger
        else {
            panic!("{:?}", p.trigger);
        };
        assert_eq!(map.as_deref(), Some(&**map::HYRULE_FIELD_CHILD));
        assert_eq!(event, actor::NPC_HYLIA_045_CHILD_DIVING_CHALLENGE);
        assert_eq!(p.child, Some(true));
        // His placement is at (-174.5, 422.3): Link lands a step in front.
        assert!((x + 174.5).hypot(z - 422.3) < 2.0, "{x} {z}");
        let set: Vec<_> = ready.iter().map(|e| preset(e)).collect();
        assert_eq!(
            set,
            [
                (&**flag::NPC_HYLIA_045_HAS_DIVED, "false"),
                (&**flag::NPC_HYLIA_045_DEPTH_OF_DIVING, "9")
            ]
        );
    }

    #[test]
    fn malon_is_talked_to_at_the_ranch_then_the_ocarina_drawn() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let p = planned.iter().find(|p| p.spot.holder == "Malon").unwrap();
        let Trigger::Talk {
            at: [x, _, z],
            steps,
            then,
            ..
        } = &p.trigger
        else {
            panic!("{:?}", p.trigger);
        };
        assert_eq!(*then, Some("Left"));
        // She comes to the ranch once the castle quest is past meeting her.
        assert!(
            steps.iter().any(|(q, _)| q == &**flow::GOTO_HYRULE_CASTLE),
            "{steps:?}"
        );
        // Her placement is at (-91.9, 245.4): Link lands a step in front.
        assert!((x + 91.9).hypot(z - 245.4) < 2.5, "{x} {z}");
    }

    #[test]
    fn the_windmill_man_is_talked_to_as_adult_then_the_ocarina_drawn() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let storms = planned
            .iter()
            .find(|p| p.spot.vanilla == item::MELODY_STORM_SONG);
        let storms = storms.unwrap();
        match &storms.trigger {
            Trigger::Talk { map, then, .. } => {
                assert_eq!(map, flag::KAKARIKO_VILLAGE_ADULT);
                assert_eq!(*then, Some("Left"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(storms.child, Some(false));
    }

    #[test]
    fn every_song_and_fairy_plays_an_event_or_says_why_not() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let (mut plays, mut skips) = (0, 0);
        for p in songs_and_fairies(planned) {
            match &p.trigger {
                Trigger::Talk { then: Some(_), .. } => plays += 1,
                Trigger::Play { event, .. } => {
                    assert!(!event.is_empty(), "{}", p.spot.holder);
                    plays += 1;
                    println!("{} plays {event}", p.spot.holder);
                }
                Trigger::Skip(why) => {
                    assert!(!why.is_empty(), "{}", p.spot.holder);
                    skips += 1;
                    println!("{} is skipped: {why}", p.spot.holder);
                }
                other => panic!("{}: {other:?}", p.spot.holder),
            }
        }
        println!("{plays} songs and fairies play, {skips} are skipped");
        assert_eq!(plays + skips, 12 + 6);
    }
}
