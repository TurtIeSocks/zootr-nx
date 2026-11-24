//! How the sweep triggers each spot of a seed: the plan, with no game I/O.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use super::wield::{self, Wield};
use crate::cows::{self, Cow};
use crate::cutscenes::Catalogue;
use crate::events::Flows;
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::gifts::GIFTS;
use crate::known::{Name, actor};
use crate::nav::kinds::{self, Act};
use crate::warp::{self, Pose, Side};
use crate::{pickups, rsdb, scrubs, shops, steps};
use landing::{around, away_from, clear_landing, neighbours, off_line};
use quest::{quests, step_of, steps};
use room::{drain, noted, water};

mod events;
mod gated;
mod landing;
mod quest;
mod room;
mod shop;

/// What `slot` holds, `load`ed into it the first time it's asked for.
fn lazy<T>(slot: &mut Option<T>, load: impl FnOnce() -> Result<T>) -> Result<&T> {
    let held: &T = match slot {
        Some(held) => held,
        None => slot.insert(load()?),
    };
    Ok(held)
}

/// A map's name, its poses, the pose of the placement looked for, and the
/// map's age.
type PlacedIn<'a> = (&'a String, &'a HashMap<u64, Pose>, &'a Pose, Option<bool>);

/// Where in `maps` the placement `hash` is: in a map of age `first`, else
/// of the other age, else of none.
fn placed_in(maps: &Maps, hash: u64, first: bool) -> Option<PlacedIn<'_>> {
    [Some(first), Some(!first), None]
        .into_iter()
        .find_map(|era| {
            let mut of_era = maps.iter().filter(|(map, _)| warp::era(map) == era);
            of_era.find_map(|(map, poses)| Some((map, poses, poses.get(&hash)?, era)))
        })
}

/// What `by` holds for `map` of `area`.
fn in_map<'a, T>(by: &'a HashMap<String, Maps<Vec<T>>>, area: &str, map: &str) -> &'a [T] {
    (by.get(area).and_then(|m| m.get(map))).map_or(&[], Vec::as_slice)
}

/// `(challenge, step)` quest steps, written as their save copies before
/// the teleport (`sweep::spot::presets`).
pub type Steps = Vec<(String, String)>;

/// GameData values set before the teleport, so the holder is there and
/// answers: the flag that raises a chest risen on a condition, like enemies
/// beaten, or the one that makes a Business Scrub a seller. `NAME` sets a
/// Bool true, `NAME=VALUE` sets any value (see [`preset`]).
pub type Ready = Vec<String>;

/// The GameData name and value a [`Ready`] entry sets.
pub fn preset(entry: &str) -> (&str, &str) {
    entry.split_once('=').unwrap_or((entry, "true"))
}

/// How a spot is made to give its item.
#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// Warp near the holder, then use it as `zootr dev use` does.
    Open {
        map: String,
        at: [f32; 3],
        /// Where the holder stands, to find its placement by.
        face: [f32; 3],
        ready: Ready,
        /// Quest steps run through the quest system before the teleport:
        /// a boss's heart container reads the live quest, not its save
        /// copy (Mac, 2026-11-14: Morpha stayed with only the copy past
        /// the boss).
        steps: Steps,
        /// The event to play once landed, before the holder is used: the
        /// one that drains its water.
        first: Option<String>,
        /// Break the holder, then take what it drops: a pot.
        broken: bool,
        /// The items of the other breakables within a sword's swing of a
        /// broken holder, which its swing can break too.
        near: Vec<String>,
        /// What Link wields first: Blue Fire on the red ice the holder is
        /// frozen in, the hammer on the pillar pinning its switch down.
        wield: Option<Wield>,
        /// Every side to use a broken holder from, best first, for when the
        /// planned one has no floor: the crater box's left lies over lava.
        sides: Vec<Side>,
        /// The enemy placements beaten for real once landed, when beating
        /// them is all the holder's gate waits on.
        foes: Vec<u64>,
        /// The values that raise the holder, held back from `ready` while
        /// there are `foes`: written only when a kill doesn't take.
        latch: Ready,
    },
    /// Warp near, use the holder, the judge steers the talk.
    Talk {
        map: String,
        at: [f32; 3],
        face: [f32; 3],
        steps: Steps,
        ready: Ready,
        /// The button pressed once the talk ends, when the giver waits for
        /// one: d-pad Left takes the ocarina out for the Windmill Man.
        then: Option<&'static str>,
        /// An item the game gives Link before the talk: what the giver
        /// takes from him.
        give: Option<&'static str>,
        /// Talked to across a counter from `at` as it is: the customer
        /// spot the keeper's placement links, or the side across from his
        /// shelves when that isn't his front. The Goron Shop's floor is off
        /// the navmesh, so a landing found on it stood Link behind the
        /// counter, inside the keeper (maintainer, 2026-11-14).
        counter: bool,
        /// Stopped first, when the giver rolls a track and talks only
        /// once stopped.
        halt: Option<Halt>,
    },
    /// Play this event with Link as starter, after these quest steps and
    /// with these values set.
    Play {
        map: Option<String>,
        event: String,
        steps: Steps,
        ready: Ready,
        /// The song Link plays once the event takes the ocarina out, when
        /// it waits for one: a cow's Epona's Song.
        melody: Option<&'static str>,
        /// Where Link steps in so the event's request actor starts it, as
        /// the game does: a sensor area of that actor. A scene that walks
        /// Link to marks the actor holds can't find them with Link as its
        /// starter, as a request by name makes him.
        at: Option<[f32; 3]>,
    },
    /// Not swept tonight, and why.
    Skip(String),
}

impl Trigger {
    /// The quest steps the spot needs: none for a skip.
    pub fn steps(&self) -> &[(String, String)] {
        match self {
            Trigger::Open { steps, .. }
            | Trigger::Talk { steps, .. }
            | Trigger::Play { steps, .. } => steps,
            Trigger::Skip(_) => &[],
        }
    }

    /// The values set before the teleport: none for a skip.
    pub fn ready(&self) -> &[String] {
        match self {
            Trigger::Open { ready, .. }
            | Trigger::Talk { ready, .. }
            | Trigger::Play { ready, .. } => ready,
            Trigger::Skip(_) => &[],
        }
    }

    /// The map the spot is swept in: none for a skip, or an event played
    /// wherever Link stands.
    pub fn map(&self) -> Option<&str> {
        match self {
            Trigger::Open { map, .. } | Trigger::Talk { map, .. } => Some(map),
            Trigger::Play { map, .. } => map.as_deref(),
            Trigger::Skip(_) => None,
        }
    }
}

/// A spot with its trigger.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub spot: Spot,
    pub trigger: Trigger,
    /// The age the trigger needs: `None` for either.
    pub child: Option<bool>,
    /// The side of its holder Link lands on and uses it from first.
    pub side: Side,
}

#[cfg(test)]
impl Planned {
    /// A spot with `trigger`, for any age, used from the front.
    pub(crate) fn with(trigger: Trigger) -> Self {
        Self {
            spot: Default::default(),
            trigger,
            child: None,
            side: Side::Front,
        }
    }

    /// A chest spot in `map`, for any age.
    pub(crate) fn open(map: &str) -> Self {
        Self::with(Trigger::Open {
            map: map.into(),
            at: Default::default(),
            face: Default::default(),
            ready: vec![],
            steps: vec![],
            first: None,
            broken: false,
            near: vec![],
            wield: None,
            sides: vec![],
            foes: vec![],
            latch: vec![],
        })
    }
}

// Not a word of the dump's name tables: only the start of chests' names.
/// A chest actor's name before its model and number.
fn chest() -> &'static str {
    actor::OBJ_T_BOX_A_01.trim_end_matches("_A_01")
}
const NPC: &str = "Npc_";
/// How the sweep stops a giver who rolls a track: the Bool true while he
/// passes the one spot he can be stopped at, and the Bool a bomb hit there
/// sets, which stops him where he is. A hit anywhere else only counts a
/// miss (Mac, 2026-11-15).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Halt {
    /// His actor, to find his placement by.
    pub actor: &'static Name,
    pub passing: &'static Name,
    pub hit: &'static Name,
}

/// Where the placement named `label` stands in `map`.
fn anchor(romfs: &Romfs, map: &str, label: &str) -> Result<[f32; 3]> {
    let placed = warp::placements_in(romfs, map)?;
    (placed.iter())
        .find(|p| p.label.as_deref() == Some(label))
        .map(|p| p.pose.at)
        .with_context(|| format!("nothing in {map} is named {label}"))
}

type Maps<T = HashMap<u64, Pose>> = BTreeMap<String, T>;
/// A gated holder's ready entries, the quest steps it waits on, and the
/// only age that opens it.
type Raise = (Ready, Steps, Option<bool>);

/// Plan every spot, in order.
pub fn plan(romfs: &Romfs, spots: &[Spot]) -> Result<Vec<Planned>> {
    let chests = (spots.iter())
        .filter(|s| s.holder.starts_with(chest()))
        .map(|s| s.hash)
        .collect();
    let mut planner = Planner::new(romfs, chests);
    planner.breakables = (spots.iter())
        .filter(|s| pickups::broken(&s.holder))
        .map(|s| (s.hash, s.item.clone()))
        .collect();
    spots.iter().map(|s| planner.spot(s)).collect()
}

/// Where each gift's giver stands, by check hash: its map and spot. A
/// gift no one stands for, as a song's or a boss's, has none.
pub fn givers(romfs: &Romfs) -> Result<HashMap<u64, (String, [f32; 3])>> {
    let mut planner = Planner::new(romfs, BTreeSet::new());
    let mut found = HashMap::new();
    for gift in GIFTS.iter() {
        if let Some((map, pose)) = planner.standing(gift)? {
            found.insert(gift.hash(), (map, pose.at));
        }
    }
    Ok(found)
}

/// What planning has read so far, each part when first asked for.
struct Planner<'a> {
    romfs: &'a Romfs,
    areas: HashMap<String, Maps>,
    catalogue: Option<Catalogue>,
    /// Every event the game can start by name, with the flow file and
    /// entry point it runs: both empty for a scene that runs its own flow,
    /// like Sheik's at the Colossus.
    events: Option<BTreeMap<String, rsdb::Runs>>,
    /// The GameData values that raise each chest a graph gates, by
    /// placement hash.
    appear: Option<HashMap<u64, Raise>>,
    /// The actors each gated chest's graph reads, by placement hash: a
    /// switch a pillar may pin down. Filled with `appear` by
    /// [`Planner::appear`], which [`Planner::wield`] needs run first.
    drivers: HashMap<u64, Vec<Name>>,
    /// The enemy placements each gated chest waits on, when beating them
    /// is all it waits on, by placement hash. Filled with `appear` by
    /// [`Planner::appear`].
    foes: HashMap<u64, Vec<u64>>,
    cows: Option<Vec<Cow>>,
    /// The sensor areas that start each request actor's event.
    sensors: Option<HashMap<u64, Vec<u64>>>,
    /// Every chest spot, by placement hash, to keep a landing clear of.
    chests: BTreeSet<u64>,
    /// Every pot, crate and rock spot's item, by placement hash, to find
    /// the neighbours a swing at one can break.
    breakables: BTreeMap<u64, String>,
    gamedata: Option<GameData>,
    flows: Option<Flows>,
    /// Where each red ice stands, by area, then map.
    ice: HashMap<String, Maps<Vec<[f32; 3]>>>,
    /// Where each hammer pillar stands, by area, then map.
    pillars: HashMap<String, Maps<Vec<[f32; 3]>>>,
    /// Where every pot, crate and rock stands, a check this seed or not, by
    /// area, then map: a swing breaks either kind.
    breaking: HashMap<String, Maps<Vec<[f32; 3]>>>,
}

impl<'a> Planner<'a> {
    fn new(romfs: &'a Romfs, chests: BTreeSet<u64>) -> Self {
        Planner {
            romfs,
            areas: HashMap::new(),
            catalogue: None,
            events: None,
            appear: None,
            cows: None,
            sensors: None,
            chests,
            breakables: BTreeMap::new(),
            gamedata: None,
            flows: None,
            ice: HashMap::new(),
            pillars: HashMap::new(),
            breaking: HashMap::new(),
            drivers: HashMap::new(),
            foes: HashMap::new(),
        }
    }
}

impl Planner<'_> {
    fn spot(&mut self, spot: &Spot) -> Result<Planned> {
        let gift = GIFTS.iter().find(|g| g.hash() == spot.hash);
        // Only a placed holder lands on a side of its own.
        let front = |(trigger, child): (Trigger, Option<bool>)| (trigger, child, Side::Front);
        let (trigger, child, side) = match gift {
            Some(gift) => front(self.gift(gift)?),
            None if cows::is_holder(&spot.holder) => front(self.cow(spot)?),
            None if spot.holder.starts_with(chest())
                || spot.holder.starts_with(scrubs::HOLDER)
                || pickups::within_reach(&spot.holder)
                // Ruto's bottle on the lake bed: a dive onto it takes it.
                || spot.holder == **actor::OBJ_BOTTLE_LETTER =>
            {
                self.placed(spot)?
            }
            None if shops::is_holder(&spot.holder) => front(self.shop(spot)?),
            None => {
                let why = format!("out of scope: {}", spot.holder);
                (Trigger::Skip(why), None, Side::Front)
            }
        };
        Ok(Planned {
            spot: spot.clone(),
            trigger,
            child: child.or(spot.child),
            side,
        })
    }

    /// The poses of every map in Banc folder `area`.
    fn maps(&mut self, area: &str) -> Result<&Maps> {
        if !self.areas.contains_key(area) {
            let maps = warp::poses_in(self.romfs, area)?;
            self.areas.insert(area.to_owned(), maps);
            let ice = warp::spots_in(self.romfs, area, &wield::RED_ICE)?;
            self.ice.insert(area.to_owned(), ice);
            let pillars = warp::spots_in(self.romfs, area, &[wield::PILLAR])?;
            self.pillars.insert(area.to_owned(), pillars);
            let breaking = warp::spots_where(self.romfs, area, |p| pickups::broken(&p.name))?;
            self.breaking.insert(area.to_owned(), breaking);
        }
        Ok(&self.areas[area])
    }

    /// A chest, a scrub, a pot or a pickup lying out: beside the actor with
    /// the spot's hash, in the child map before the adult one, or the map of
    /// the only age its gate opens for. And the side to land on: a broken
    /// holder is struck from the one that keeps its neighbours behind Link.
    fn placed(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>, Side)> {
        let (raise, steps, age) = self.appear(spot.hash)?;
        let raise = [self.leaving(&raise)?, raise].concat();
        let foes = self.foes.get(&spot.hash).cloned().unwrap_or_default();
        let (raise, latch) = if foes.is_empty() {
            (raise, vec![])
        } else {
            (vec![], raise)
        };
        // Not `maps`' own return: it would hold `self` across `wield`.
        self.maps(&spot.area)?;
        let found = placed_in(&self.areas[&spot.area], spot.hash, age.unwrap_or(true));
        let scrub = spot.holder.starts_with(scrubs::HOLDER);
        Ok(match found {
            None => {
                let kind = kinds::word(&spot.holder).unwrap_or(&spot.holder);
                let why = format!("{kind} not found in {}", spot.area);
                (Trigger::Skip(why), None, Side::Front)
            }
            Some((map, poses, pose, era)) => {
                let others: Vec<_> = (self.chests.iter())
                    .filter(|&&h| h != spot.hash)
                    .filter_map(|h| Some(poses.get(h)?.at))
                    .collect();
                let pose = &warp::facing(&spot.holder, pose);
                let (map, at) = (map.clone(), clear_landing(pose, &others));
                let face = pose.at;
                let wield = self.wield(spot, &map, face)?;
                let broken = pickups::broken(&spot.holder);
                let near = if broken {
                    neighbours(spot.hash, face, poses, &self.breakables)
                } else {
                    vec![]
                };
                // Every breakable near, not only this seed's: the crater
                // box's roll broke the Gold Skulltula box beside it, its
                // drop no check, 33 times (other machine, 2026-11-17).
                let breaking = in_map(&self.breaking, &spot.area, &map);
                let near_at = around(face, breaking);
                let act = kinds::kind_of(&spot.holder).and_then(|k| k.act);
                let sides = match act {
                    Some(Act::Break { by, .. }) if by == kinds::ROLL => off_line(pose, &near_at),
                    _ if broken => away_from(pose, &near_at),
                    _ => vec![],
                };
                let side = sides.first().copied().unwrap_or_default();
                let trigger = if scrub {
                    Trigger::Talk {
                        ready: self.sellers(&map, spot.hash)?,
                        map,
                        at,
                        face,
                        steps: vec![],
                        then: None,
                        give: None,
                        counter: false,
                        halt: None,
                    }
                } else {
                    Trigger::Open {
                        ready: [raise, noted(&spot.holder), water(&map)].concat(),
                        steps,
                        first: drain(&map),
                        map,
                        at,
                        face,
                        broken,
                        near,
                        wield,
                        sides,
                        foes,
                        latch,
                    }
                };
                (trigger, era, side)
            }
        })
    }

    /// GameData, with every quest's saved step named.
    fn gamedata(&mut self) -> Result<&GameData> {
        lazy(&mut self.gamedata, || {
            let mut gamedata = GameData::load(self.romfs)?;
            steps::learn(self.romfs, &mut gamedata)?;
            Ok(gamedata)
        })
    }

    /// A cow: the event Epona's Song runs at it, in the map of the
    /// placement its check is keyed by.
    fn cow(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>)> {
        let all = lazy(&mut self.cows, || cows::find(self.romfs))?;
        let Some(cow) = all.iter().find(|c| c.hash() == spot.hash) else {
            return Ok((Trigger::Skip(format!("no cow {}", spot.hash)), None));
        };
        let map = warp::map_of(&cow.check().file).to_owned();
        let child = warp::era(&map);
        let event = cow.event.clone();
        let trigger = Trigger::Play {
            map: Some(map),
            event,
            steps: vec![],
            ready: vec![],
            melody: Some(&**cows::MELODY),
            at: None,
        };
        Ok((trigger, child))
    }
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, field, flag, flow};
    use std::sync::OnceLock;

    use super::*;
    use crate::generate::{Extras, generate};
    use crate::settings::{self, Settings};

    /// A seed with every shuffle setting on: its spots, and each planned
    /// once for all the tests.
    pub(super) fn seed() -> Option<(Romfs, &'static [Spot], &'static [Planned])> {
        static SEED: OnceLock<(Vec<Spot>, Vec<Planned>)> = OnceLock::new();
        let romfs = crate::game::dev_dump()?;
        let (spots, planned) = SEED.get_or_init(|| {
            let ids = [
                settings::SCRUBS,
                settings::NPC_GIFTS,
                settings::DUNGEON_REWARDS,
                settings::SONGS,
                settings::MAPS_COMPASSES,
                settings::SHOPS,
                settings::FREESTANDING,
                settings::SKULLTULAS,
                settings::BOSS_HEARTS,
                settings::POTS,
                settings::CRATES,
                settings::ROCKS,
                settings::GRASS,
                settings::HIVES,
                settings::COWS,
                settings::SKIP_ALL,
                "open-forest",
                "kakariko-gate",
            ];
            let settings: Settings = ids.map(|id| (id.into(), true.into())).into();
            let dir = std::env::temp_dir().join(format!("zootr-sweep-plan-{}", std::process::id()));
            let report = generate(
                &romfs,
                &dir,
                7,
                &settings,
                &Extras::default(),
                &crate::progress::Run::silent(),
            )
            .unwrap();
            std::fs::remove_dir_all(dir).unwrap();
            let planned = plan(&romfs, &report.spots).unwrap();
            (report.spots, planned)
        });
        Some((romfs, spots, planned))
    }

    #[test]
    fn a_real_seed_plans_every_spot() {
        let Some((_, spots, planned)) = seed() else {
            return;
        };
        assert_eq!(planned.len(), spots.len());
        assert!(planned.iter().zip(spots).all(|(p, s)| p.spot == *s));
        let (mut open, mut talk, mut play, mut skip) = (0, 0, 0, 0);
        let mut why: BTreeMap<&str, usize> = BTreeMap::new();
        for p in planned {
            match &p.trigger {
                Trigger::Open { .. } => open += 1,
                Trigger::Talk { .. } => talk += 1,
                Trigger::Play { .. } => play += 1,
                Trigger::Skip(reason) => {
                    skip += 1;
                    *why.entry(reason).or_default() += 1;
                }
            }
        }
        println!("open {open}, talk {talk}, play {play}, skip {skip}");
        let mut why: Vec<_> = why.into_iter().collect();
        why.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        why.iter()
            .for_each(|(reason, n)| println!("  {n:>4} {reason}"));
        assert_eq!(open + talk + play + skip, spots.len());
        assert!(open > 0 && talk > 0 && play > 0 && skip > 0);
    }

    #[test]
    fn the_mirror_shield_chest_is_opened_by_an_adult_on_its_latch() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let chest = planned.iter().find(|p| p.spot.hash == 10012167258643363548);
        let Some(Trigger::Open { ready, map, .. }) = chest.map(|p| &p.trigger) else {
            panic!("{chest:?}");
        };
        assert_eq!(ready, &[flag::DESERT_COLOSSUS_TBOX_ON]);
        assert!(map.ends_with(crate::warp::ADULT), "{map}");
        assert_eq!(chest.unwrap().child, Some(false));
    }

    #[test]
    fn dampes_race_chests_rise_on_the_prize_the_race_left() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        // Both stand on one spot, and the prize enum picks which rises.
        for (hash, prize) in [
            (16041366920820397198, field::APPEAR_HOOK_SHOT),
            (5534919361313859554, field::APPEAR_HEART_PIECE),
        ] {
            let chest = planned.iter().find(|p| p.spot.hash == hash);
            let Some(Trigger::Open { ready, .. }) = chest.map(|p| &p.trigger) else {
                panic!("{chest:?}");
            };
            let state = format!("{}={prize}", flag::MINIGAME_DAMPE_RACE_TREASURE_BOX_STATE);
            assert!(ready.contains(&state), "{ready:?}");
            assert!(
                ready
                    .iter()
                    .any(|r| r == flag::MINI_GAME_DAMPE_RACE_IS_APPEAR_TREASURE_BOX)
            );
        }
    }

    #[test]
    fn a_preset_sets_a_bool_true_unless_it_names_a_value() {
        assert_eq!(preset("Flag"), ("Flag", "true"));
        assert_eq!(preset("Depth=9"), ("Depth", "9"));
    }

    #[test]
    fn every_cow_plays_its_own_song_event_in_its_map() {
        let Some((romfs, _, planned)) = seed() else {
            return;
        };
        let rows = Planner::new(&romfs, BTreeSet::new())
            .events()
            .unwrap()
            .clone();
        let mut events = BTreeSet::new();
        for p in planned.iter().filter(|p| cows::is_holder(&p.spot.holder)) {
            let Trigger::Play {
                map,
                event,
                steps,
                melody,
                ..
            } = &p.trigger
            else {
                panic!("{:?}", p.trigger);
            };
            assert!(steps.is_empty());
            assert_eq!(*melody, Some(&**cows::MELODY));
            let map = map.as_deref().unwrap();
            assert!(map.starts_with(&p.spot.area), "{map}");
            assert_eq!(p.child, warp::era(map));
            // A named cow's row is the game's, a plain cow's the seed's.
            assert!(
                rows.contains_key(event) || event.starts_with("GiveMilk_Rando_"),
                "{event}"
            );
            assert!(events.insert(event.clone()), "{event} twice");
        }
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn nearly_every_chest_opens_from_a_landing_inside_its_map() {
        let Some((romfs, _, planned)) = seed() else {
            return;
        };
        let chests: Vec<_> = planned
            .iter()
            .filter(|p| p.spot.holder.starts_with(chest()))
            .collect();
        let mut opens = 0;
        for p in &chests {
            let Trigger::Open { map, at, .. } = &p.trigger else {
                println!("{:?}: {:?}", p.spot.holder, p.trigger);
                continue;
            };
            opens += 1;
            let poses = warp::poses_in(&romfs, &p.spot.area).unwrap();
            let (lo, hi) =
                poses[map]
                    .values()
                    .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), pose| {
                        let min = |i: usize| lo[i].min(pose.at[i]);
                        let max = |i: usize| hi[i].max(pose.at[i]);
                        ([min(0), min(1), min(2)], [max(0), max(1), max(2)])
                    });
            let inside = (0..3).all(|i| at[i] >= lo[i] - 5.0 && at[i] <= hi[i] + 5.0);
            assert!(
                inside,
                "{} {map}: {at:?} outside {lo:?} {hi:?}",
                p.spot.holder
            );
            assert!(at.iter().all(|v| v.is_finite()) && at.iter().any(|&v| v != 0.0));
            assert_eq!(p.child, warp::era(map).or(p.spot.child), "{map}");
        }
        println!("{opens} of {} chests open", chests.len());
        assert!(
            opens * 100 >= chests.len() * 95,
            "{opens} of {}",
            chests.len()
        );
    }

    #[test]
    fn a_spot_of_an_unknown_holder_is_skipped() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let spot = Spot {
            hash: 1,
            area: flag::KOKIRI_FOREST.to_string(),
            holder: "Nobody".into(),
            ..Default::default()
        };
        let planned = plan(&romfs, &[spot]).unwrap();
        assert_eq!(
            planned[0].trigger,
            Trigger::Skip("out of scope: Nobody".into())
        );
        let chest = Spot {
            holder: actor::OBJ_T_BOX_A_01.to_string(),
            ..planned[0].spot.clone()
        };
        let planned = plan(&romfs, &[chest]).unwrap();
        assert_eq!(
            planned[0].trigger,
            Trigger::Skip(format!("chest not found in {}", flag::KOKIRI_FOREST))
        );
    }

    #[test]
    fn a_pot_is_broken_and_a_heart_piece_taken() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let at = |hash, holder: &str, area: &str| Spot {
            hash,
            area: area.into(),
            holder: holder.into(),
            ..Default::default()
        };
        let spots = [
            at(
                2108281022000564101,
                actor::OBJ_POT_A_01,
                flag::KOKIRI_FOREST,
            ),
            at(
                10331043360028932045,
                actor::OBJ_HEART_PIECE,
                flag::DEATH_MOUNTAIN_CRATER,
            ),
        ];
        let planned = plan(&romfs, &spots).unwrap();
        let broken = |p: &Planned| match p.trigger {
            Trigger::Open { broken, .. } => Some(broken),
            _ => None,
        };
        assert_eq!(broken(&planned[0]), Some(true), "{:?}", planned[0].trigger);
        assert_eq!(broken(&planned[1]), Some(false), "{:?}", planned[1].trigger);
    }

    #[test]
    fn a_pot_beside_pots_is_struck_from_a_side_that_leaves_them_unswung_at() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // Gerudo Valley's pot, with a pot 1.6 m off to its one side and
        // another 2.4 m off to its other: its sword swing broke both.
        let area = flow::GERUDO_VALLEY.to_string();
        let at = |hash, item: &str| Spot {
            hash,
            area: area.clone(),
            holder: actor::OBJ_POT_A_05.to_string(),
            item: item.into(),
            ..Default::default()
        };
        let spots = [
            at(3407692112799570510, "Item_A"),
            at(6896071498853277403, "Item_B"),
            at(15334540056586068351, "Item_C"),
        ];
        let planned = plan(&romfs, &spots).unwrap();
        let Trigger::Open { near, map, .. } = &planned[0].trigger else {
            panic!("{:?}", planned[0].trigger);
        };
        assert_eq!(near, &["Item_B", "Item_C"], "nearest first");
        let poses = &warp::poses_in(&romfs, &area).unwrap()[map];
        let [pot, left, right] = [0, 1, 2].map(|i| &poses[&spots[i].hash]);
        // From the side it lands on, neither lies past the holder from Link.
        let held = warp::facing(&spots[0].holder, pot);
        let link = warp::landing_at(&held, planned[0].side, 1.0);
        for n in [left, right] {
            let (to_pot, past) = ([pot.at[0] - link[0], pot.at[2] - link[2]], n.at);
            let ahead = to_pot[0] * (past[0] - pot.at[0]) + to_pot[1] * (past[2] - pot.at[2]);
            assert!(
                ahead <= 0.1,
                "{past:?} is in the swing from {:?}",
                planned[0].side
            );
        }
        // Planned alone, its neighbours no checks, they still break.
        let alone = plan(&romfs, &spots[..1]).unwrap();
        assert_eq!(alone[0].side, planned[0].side);
        assert!(matches!(&alone[0].trigger, Trigger::Open { near, .. } if near.is_empty()));
    }

    #[test]
    fn a_box_is_rolled_at_from_a_side_off_the_line_of_a_box_that_is_no_check() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // The crater's box, and 1.9 m off the box a Gold Skulltula hides in.
        let spot = Spot {
            hash: 4502491733150659917,
            area: flag::DEATH_MOUNTAIN_CRATER.to_string(),
            holder: actor::OBJ_BROKEN_BOX_A_01.to_string(),
            ..Default::default()
        };
        let planned = plan(&romfs, std::slice::from_ref(&spot)).unwrap();
        let Trigger::Open { map, sides, .. } = &planned[0].trigger else {
            panic!("{:?}", planned[0].trigger);
        };
        // Its left lies over lava: the right comes next, the box's line last.
        assert_eq!(sides, &[Side::Left, Side::Right, Side::Front, Side::Back]);
        let poses = &warp::poses_in(&romfs, &spot.area).unwrap();
        let held = warp::facing(&spot.holder, &poses[map][&spot.hash]);
        let link = warp::landing_at(&held, planned[0].side, 1.0);
        // Off the roll's line by more than a box: its run-up starts 1.4 m
        // out, so behind Link is as bad as past the box.
        let skulltula = [-2.9, 128.2, 48.7];
        let (dx, dz) = (held.at[0] - link[0], held.at[2] - link[2]);
        let (sx, sz) = (skulltula[0] - held.at[0], skulltula[2] - held.at[2]);
        let off = (dx * sz - dz * sx).abs() / dx.hypot(dz);
        assert!(
            off >= 1.0,
            "{off} m off the roll from {:?}",
            planned[0].side
        );
    }
}
