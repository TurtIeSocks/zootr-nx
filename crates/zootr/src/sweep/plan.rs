//! How the sweep triggers each spot of a seed: the plan, with no game I/O.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use super::ice;
use crate::cows::{self, Cow};
use crate::cutscenes::Catalogue;
use crate::events::{Effect, Flows};
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::gifts::{GIFTS, Gift};
use crate::known::{Name, actor, field, flag, flow};
use crate::nav::kinds;
use crate::warp::{self, Pose, Side};
use crate::{flags, gates, pickups, rsdb, scrubs, shops, steps};
use landing::{away_from, clear_landing, neighbours};

mod events;
mod landing;
mod shop;

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
        /// The red ice the holder is frozen in, melted with Blue Fire first.
        ice: Option<[f32; 3]>,
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

// Not a word of the dump's name tables: only the start of chests' names.
/// A chest actor's name before its model and number.
fn chest() -> &'static str {
    actor::OBJ_T_BOX_A_01.trim_end_matches("_A_01")
}
const NPC: &str = "Npc_";
/// Quest steps a gift's flow needs first, as `(flow, challenge, step)`,
/// where a test preset already records them.
const STEPS: [(&Name, &Name, &str); 1] = [(
    actor::NPC_GORON_BIGGORON_ADULT,
    flow::SUB_EVENT_GET_SWORD_BIG_GORON,
    "Step11",
)];

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
        }
    }
}

impl Planner<'_> {
    fn spot(&mut self, spot: &Spot) -> Result<Planned> {
        let gift = GIFTS.iter().find(|g| g.hash() == spot.hash);
        let mut side = Side::Front;
        let (trigger, child) = match gift {
            Some(gift) => self.gift(gift)?,
            None if cows::is_holder(&spot.holder) => self.cow(spot)?,
            None if spot.holder.starts_with(chest())
                || spot.holder.starts_with(scrubs::HOLDER)
                || pickups::within_reach(&spot.holder) =>
            {
                let (trigger, child, landing) = self.placed(spot)?;
                side = landing;
                (trigger, child)
            }
            None if shops::is_holder(&spot.holder) => self.shop(spot)?,
            None => {
                let why = format!("out of scope: {}", spot.holder);
                (Trigger::Skip(why), None)
            }
        };
        Ok(Planned {
            spot: spot.clone(),
            trigger,
            child,
            side,
        })
    }

    /// The poses of every map in Banc folder `area`.
    fn maps(&mut self, area: &str) -> Result<&Maps> {
        if !self.areas.contains_key(area) {
            let maps = warp::poses_in(self.romfs, area)?;
            self.areas.insert(area.to_owned(), maps);
            let ice = warp::spots_in(self.romfs, area, &ice::RED_ICE)?;
            self.ice.insert(area.to_owned(), ice);
        }
        Ok(&self.areas[area])
    }

    /// The GameData values and quest steps that raise the chest or boss
    /// heart container with placement hash `hash`, and the age it rises
    /// for when only one can open it.
    fn appear(&mut self, hash: u64) -> Result<Raise> {
        if self.appear.is_none() {
            let gated = [gates::trace(self.romfs)?, gates::hearts(self.romfs)?].concat();
            let gd = self.gamedata()?;
            let raise = |g: gates::Gate| {
                let steps = quests(gd, &g.expr);
                (g.hash, (g.expr.opening(), steps, g.expr.age()))
            };
            self.appear = Some(gated.into_iter().map(raise).collect());
        }
        Ok((self.appear.as_ref())
            .and_then(|a| a.get(&hash).cloned())
            .unwrap_or_default())
    }

    /// The actor delete flags play sets along with `ready`'s, through the
    /// flows of the NPCs that set them. A jailed Fire Temple Goron's talk
    /// sets his delete flag, which unlocks the chest behind him, and
    /// his rescue sets another. Without that one
    /// he still sits in front of the chest after a load.
    fn leaving(&mut self, ready: &[String]) -> Result<Ready> {
        if ready.is_empty() {
            return Ok(vec![]);
        }
        if self.flows.is_none() {
            self.flows = Some(Flows::load(self.romfs)?);
        }
        let flows = self.flows.as_ref().unwrap();
        let mut out: Ready = vec![];
        for flag in ready {
            let setters = (flows.0.iter()).filter(|(name, chart)| {
                name.starts_with(NPC) && flags::set_in(chart).any(|f| f == flag)
            });
            for (setter, _) in setters {
                let deletes = flows.effects(setter).unwrap_or_default();
                for gone in deletes.iter().filter_map(deleted) {
                    if !out.contains(&gone) && !ready.contains(&gone) {
                        out.push(gone);
                    }
                }
            }
        }
        Ok(out)
    }

    /// A chest, a scrub, a pot or a pickup lying out: beside the actor with
    /// the spot's hash, in the child map before the adult one, or the map of
    /// the only age its gate opens for. And the side to land on: a broken
    /// holder is struck from the one that keeps its neighbours behind Link.
    fn placed(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>, Side)> {
        let (raise, steps, age) = self.appear(spot.hash)?;
        let raise = [self.leaving(&raise)?, raise].concat();
        self.maps(&spot.area)?;
        let maps = &self.areas[&spot.area];
        let first = age.unwrap_or(true);
        let found = [Some(first), Some(!first), None]
            .into_iter()
            .find_map(|era| {
                let mut of_era = maps.iter().filter(|(map, _)| warp::era(map) == era);
                of_era.find_map(|(map, poses)| Some((map, poses, poses.get(&spot.hash)?, era)))
            });
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
                let ice = (self.ice[&spot.area].get(&map)).and_then(|i| ice::encasing(i, face));
                let broken = pickups::broken(&spot.holder);
                let found = match broken {
                    true => neighbours(spot.hash, face, poses, &self.breakables),
                    false => vec![],
                };
                let side = away_from(pose, &found.iter().map(|n| n.1).collect::<Vec<_>>());
                let near = found.into_iter().map(|n| n.0).collect();
                let trigger = match scrub {
                    false => Trigger::Open {
                        ready: [raise, noted(&spot.holder), water(&map)].concat(),
                        steps,
                        first: drain(&map),
                        map,
                        at,
                        face,
                        broken,
                        near,
                        ice,
                    },
                    true => Trigger::Talk {
                        ready: self.sellers(&map, spot.hash)?,
                        map,
                        at,
                        face,
                        steps: vec![],
                        then: None,
                        give: None,
                        counter: false,
                        halt: None,
                    },
                };
                (trigger, era, side)
            }
        })
    }

    /// GameData, with every quest's saved step named.
    fn gamedata(&mut self) -> Result<&GameData> {
        if self.gamedata.is_none() {
            let mut gamedata = GameData::load(self.romfs)?;
            steps::learn(self.romfs, &mut gamedata)?;
            self.gamedata = Some(gamedata);
        }
        Ok(self.gamedata.as_ref().unwrap())
    }

    /// A cow: the event Epona's Song runs at it, in the map of the
    /// placement its check is keyed by.
    fn cow(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>)> {
        if self.cows.is_none() {
            self.cows = Some(cows::find(self.romfs)?);
        }
        let all = self.cows.as_ref().unwrap();
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

/// The `(challenge, step)` a BoolExp rendered as `Step_<challenge> ==
/// <step>` asks for, and `None` for any other expression.
fn step_of(expr: &str) -> Option<(String, String)> {
    let (saved, step) = compared(expr, "==")?;
    Some((saved.strip_prefix(steps::SAVED)?.into(), step.into()))
}

/// The enum and value a BoolExp rendered as `NAME OP VALUE` compares.
fn compared<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let (name, value) = expr.split_once(&format!(" {op} "))?;
    let one = |s: &str| !s.is_empty() && !s.contains(' ');
    (one(name) && one(value)).then_some((name, value))
}

/// The quest steps that open `expr`: for each quest it reads past a step,
/// directly or through a BoolExp like the Spirit sage's "after step 9" one,
/// the step after it. A boss's heart container appears once its quest is
/// past the boss.
fn quests(gd: &GameData, expr: &gates::Expr) -> Steps {
    let past = |atom: &gates::Atom| match atom {
        gates::Atom::After { name, value } => Some((name.clone(), value.clone())),
        gates::Atom::Exp(exp) => {
            let rendered = gd.expr(exp).ok()??;
            let (name, value) = compared(&rendered, ">")?;
            Some((name.to_owned(), value.to_owned()))
        }
        _ => None,
    };
    let next = |(name, value): (String, String)| {
        let order = gd.enum_values(&name)?;
        let after = order.into_iter().skip_while(|s| *s != value).nth(1)?;
        Some((name.strip_prefix(steps::SAVED)?.to_owned(), after))
    };
    expr.quests()
        .into_iter()
        .filter_map(past)
        .filter_map(next)
        .collect()
}

fn steps(gift: &Gift) -> Steps {
    let of = STEPS.iter().filter(|(flow, ..)| *flow == gift.flow);
    of.map(|&(_, c, s)| (c.to_string(), s.into())).collect()
}

/// A map with water that play drains before its low spots can be
/// reached, and the event that drains it: the Well's first room stays
/// underwater until Zelda's Lullaby is played at its Triforce point. Its
/// flags set alone leave the water up, the event lowers it.
const DRAINS: [(&Name, &Name); 1] = [(
    flow::BOTTOM_OF_THE_WELL,
    field::BOTTOM_OF_THE_WELL_LEVEL_ROOM01_WATER_LEVEL_DOWN,
)];

/// The event that drains `map`'s water, if play drains any.
fn drain(map: &str) -> Option<String> {
    let mut of = DRAINS.iter().filter(|(m, _)| *m == map);
    of.next().map(|(_, event)| event.to_string())
}

/// Maps whose water level play changes, and the level that has their low
/// rooms dry. A new file starts the Water Temple at 2, its lower rooms
/// flooded and their chests under the surface. At 0, as Zelda's Lullaby at
/// the lowest tablet leaves it, they're dry (Mac, 2026-11-14). The map
/// reads the level as it loads, so it's held like a flag. The Forest
/// Temple's well stays full, its chest under the surface, until a switch
/// sets its Bool (the sweep swam above the chest on every side,
/// 2026-11-15).
static WATER: LazyLock<[(&Name, String); 2]> = LazyLock::new(|| {
    [
        (
            flag::WATER_TEMPLE,
            format!("{}=0", field::WATER_TEMPLE_ROOM00_WATER_LEVEL),
        ),
        (
            flag::FOREST_TEMPLE,
            field::FOREST_TEMPLE_ROOM07_CHANGE_WATER_LEVEL_ACTOR.to_string(),
        ),
    ]
});

/// The water level `map` is planned at, if play changes it.
fn water(map: &str) -> Vec<String> {
    let of = WATER.iter().filter(|(m, _)| *m == map);
    of.map(|(_, level)| level.to_string()).collect()
}

/// The actor delete flag `effect` sets, if it sets one: a name ending
/// `_Delete`, set true.
fn deleted(effect: &Effect) -> Option<String> {
    let set = effect.action == flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL
        && effect.params.contains("Value=true");
    let (_, rest) = effect
        .params
        .split_once(&format!("{}=\"", field::GAME_DATA_NAME))?;
    let (name, _) = rest.split_once('"')?;
    (set && name.ends_with("_Delete")).then(|| name.to_owned())
}

/// Chests whose own flow gives the item only once a note beside them is
/// read, by holder, with the flag the note sets. Unread, the Hyrule Field
/// note chest (the field's eleventh note chest) opens empty and Navi talks.
/// It gives only to a child, and its child map is planned first.
const NOTED: [(&Name, &Name); 1] = [(
    actor::OBJ_T_BOX_A_01_MEMO11,
    flag::HAVE_READ_HRL_FIELD_NOTE11,
)];

/// The flags a chest's note must have set before it gives its item.
fn noted(holder: &str) -> Vec<String> {
    let of = NOTED.iter().filter(|(h, _)| *h == holder);
    of.map(|(_, flag)| flag.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, field, flag, flow, map};
    use std::sync::OnceLock;

    use super::*;
    use crate::gamedata::GameData;
    use crate::generate::{Extras, generate};
    use crate::settings::{self, Settings};
    use crate::steps;

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
                settings::HIVES,
                settings::COWS,
                settings::SKIP_ALL,
                "open-forest",
                "kakariko-gate",
            ];
            let settings: Settings = ids.map(|id| (id.into(), true.into())).into();
            let dir = std::env::temp_dir().join(format!("zootr-sweep-plan-{}", std::process::id()));
            let report = generate(&romfs, &dir, 7, &settings, &Extras::default()).unwrap();
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
    fn a_chest_in_the_well_is_planned_with_its_water_drained() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        // The Well's Lower Blue Rupee 1, underwater until the lullaby.
        let lower = planned.iter().find(|p| p.spot.hash == 12988877782812797760);
        let Some(Trigger::Open { first, .. }) = lower.map(|p| &p.trigger) else {
            panic!("{lower:?}");
        };
        let drain = field::BOTTOM_OF_THE_WELL_LEVEL_ROOM01_WATER_LEVEL_DOWN;
        assert_eq!(first.as_deref(), Some(&**drain));
    }

    #[test]
    fn a_chest_that_rises_is_planned_with_the_flag_that_raises_it() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        // Dodongo's Cavern's Red Rupee chest, risen once the Lizalfos fall.
        let dodongo = planned.iter().find(|p| p.spot.hash == 7764016096998075927);
        let Some(Trigger::Open { ready, .. }) = dodongo.map(|p| &p.trigger) else {
            panic!("{dodongo:?}");
        };
        assert_eq!(ready, &[flag::GIMMICK_DEFEAT_ENEMY_OPEN_DODONGO_CAVE_01]);
        let plain = planned.iter().filter_map(|p| match &p.trigger {
            Trigger::Open { ready, .. } => Some(ready.is_empty()),
            _ => None,
        });
        assert!(
            plain.filter(|&p| p).count() > 50,
            "most chests just sit there"
        );
    }

    #[test]
    fn the_field_note_chest_is_opened_by_a_child_with_its_note_read() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let chest = planned.iter().find(|p| p.spot.hash == 2967378296224661060);
        let Some(Trigger::Open { ready, map, .. }) = chest.map(|p| &p.trigger) else {
            panic!("{chest:?}");
        };
        assert_eq!(ready, &[flag::HAVE_READ_HRL_FIELD_NOTE11]);
        assert_eq!(map, map::HYRULE_FIELD_CHILD);
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
        assert!(map.ends_with("_Adult"), "{map}");
        assert_eq!(chest.unwrap().child, Some(false));
    }

    #[test]
    fn the_fire_temple_boss_key_waits_for_its_goron_to_leave() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // The boss key chest's gate holds his talk's flag. His rescue takes
        // him out of the way.
        let mut planner = Planner::new(&romfs, BTreeSet::new());
        let gate = [flag::NPC_GORON_010_DELETE.to_string()];
        let leaving = planner.leaving(&gate).unwrap();
        assert_eq!(leaving, [flag::NPC_GORON_010_ADULT_FIRE_TEMPLE_DELETE]);
    }

    #[test]
    fn a_boss_heart_container_waits_for_its_quest_past_the_boss() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut planner = Planner::new(&romfs, BTreeSet::new());
        let mut raised = |hash| planner.appear(hash).unwrap();
        // Read live from the quest system: past Step07_2 is Step08.
        let water = raised(14426685265732353862);
        assert_eq!(
            water.1,
            [(flow::SAGE_OF_WATER.to_string(), "Step08".to_owned())]
        );
        // Through a BoolExp, the Spirit sage's "after step 9" one.
        let spirit = raised(4720088048159616469);
        assert_eq!(
            spirit.1,
            [(flow::SAGE_OF_SPIRIT.to_string(), "Step10".to_owned())]
        );
        // A plain flag.
        let fire = raised(9478330070473364248);
        assert_eq!(
            (fire.0, fire.1),
            (
                vec![flag::FIRE_TEMPLE_BOSS_GROUND_CHANGE.to_string()],
                vec![]
            )
        );
        for g in gates::hearts(&romfs).unwrap() {
            let (ready, steps, _) = raised(g.hash);
            assert!(
                !ready.is_empty() || !steps.is_empty(),
                "{} raises on nothing",
                g.area
            );
        }
    }

    #[test]
    fn a_water_temple_chest_is_planned_at_low_water_with_its_gate() {
        needs_names!();
        assert_eq!(
            water(flag::WATER_TEMPLE),
            [format!("{}=0", field::WATER_TEMPLE_ROOM00_WATER_LEVEL)]
        );
        assert!(water(flow::BOTTOM_OF_THE_WELL).is_empty());
        assert_eq!(
            water(flag::FOREST_TEMPLE),
            [field::FOREST_TEMPLE_ROOM07_CHANGE_WATER_LEVEL_ACTOR.to_string()]
        );
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut planner = Planner::new(&romfs, BTreeSet::new());
        let (ready, ..) = planner.appear(18099019250896853049).unwrap();
        assert_eq!(ready, [flag::WATER_TEMPLE_ROOM18_TRESURE]);
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
            assert_eq!(p.child, warp::era(map), "{map}");
        }
        println!("{opens} of {} chests open", chests.len());
        assert!(
            opens * 100 >= chests.len() * 95,
            "{opens} of {}",
            chests.len()
        );
    }

    #[test]
    fn quest_steps_are_steps_the_game_has() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        for (_, challenge, step) in STEPS {
            steps::check(&gd, challenge, step).unwrap();
        }
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
    fn holders_frozen_in_red_ice_melt_it_first() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let frozen: BTreeSet<u64> = (planned.iter())
            .filter(|p| matches!(p.trigger, Trigger::Open { ice: Some(_), .. }))
            .map(|p| p.spot.hash)
            .collect();
        // The Ice Cavern's two pots and its map chest.
        let cavern = [
            3105481948394149361,
            13545259034644162992,
            11498802739257120175,
        ];
        assert!(cavern.iter().all(|h| frozen.contains(h)), "{frozen:?}");
        assert!(frozen.len() < 10, "{frozen:?}");
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
        // A pot alone keeps the front.
        let alone = plan(&romfs, &spots[..1]).unwrap();
        assert_eq!(alone[0].side, Side::Front);
    }

    #[test]
    fn a_step_is_read_from_its_expression() {
        needs_names!();
        let ruby = Some((flow::GET_STONE_OF_FIRE.to_string(), "Complete".into()));
        assert_eq!(
            step_of(&format!(
                "{} == Complete",
                crate::steps::saved(flow::GET_STONE_OF_FIRE)
            )),
            ruby
        );
        assert_eq!(
            step_of(&format!(
                "{} > Step02",
                crate::steps::saved(flow::MEET_DEKU_TREE)
            )),
            None
        );
        assert_eq!(step_of("Step_A == B && Step_C == D"), None);
        assert_eq!(step_of("IsGet.Item_A"), None);
    }
}
