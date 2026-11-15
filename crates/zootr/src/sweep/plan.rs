//! How the sweep triggers each spot of a seed: the plan, with no game I/O.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use crate::cows::{self, Cow};
use crate::cutscenes::Catalogue;
use crate::events::{Effect, Flows};
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::gifts::{self, GIFTS, Gift, How};
use crate::known::{Name, actor, field, flag, flow, item};
use crate::nav::graph::dist;
use crate::nav::kinds;
use crate::settings::DUNGEON_REWARDS;
use crate::warp::{self, Pose, Side};
use crate::world::Presence;
use crate::{flags, flows, gates, pickups, rsdb, scrubs, shops, steps};

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
        /// spot the keeper's placement links. The Goron Shop's floor is off
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
const CHEST: &str = "Obj_TBox";
/// Every shopkeeper's actor name has this, like `Npc_Goron_011_Shop_Adult`.
const KEEPER: &str = "_Shop_";
/// And starts with this: the Bombchu Shop's music emitter,
/// `BGM_Shop_Market_Bombchu`, stands nearer its shelf than its keeper.
const PERSON: &str = "Npc_";
/// The anchor a keeper links where his customers stand.
const CUSTOMER: &Name = field::DESTINATION_ANCHOR;
/// How far a [`CUSTOMER`] spot may be from its keeper, in m: A reached the
/// Goron keeper from 2.2 (live, 2026-11-14), his spot is 2.0 off. The
/// Kokiri keeper's link, 5.4 off, is something else.
const COUNTER: f32 = 2.5;
/// How far, flat, a keeper stands from the shelves behind them.
const KEEPER_REACH: f32 = 5.0;
/// How far in front of a chest or a scrub Link lands: close enough that A
/// opens or talks once he faces it. The first night's sweep landed 2 m off
/// and never opened a chest.
const REACH: f32 = 1.0;
/// How far, in degrees, a chest's landing may swing off its front, tried
/// in order, to keep clear of a chest beside it.
const SWINGS: [f32; 7] = [0.0, -15.0, 15.0, -30.0, 30.0, -45.0, 45.0];
/// How much farther than the holder every other chest must be from the
/// landing. Mido's back chests each have a front one 1.5 ahead.
const CLEAR: f32 = 0.2;
const NPC: &str = "Npc_";
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
        ("Npc_SkullKid", "minigame"),
        ("Npc_Hylia_Dampe", "the dig"),
        (flow::GATHERING_TOKEN_GOLD, "token reward"),
        ("SubEvent_", "quest chain"),
        ("Dm_", "cutscene"),
    ]
});

/// Quest steps a gift's flow needs first, as `(flow, challenge, step)`,
/// where a test preset already records them.
/// Givers who teach only once Link takes his ocarina out after their
/// talk, and the button that does it: the Windmill Man's Song of Storms
/// (Mac, 2026-11-14). His flow isn't an event, so asked by name nothing
/// plays, and he stands in the adult village only.
const DRAWN: [(&Name, &str); 1] = [(actor::NPC_HYLIA_009_ADULT, "Left")];

const STEPS: [(&Name, &Name, &str); 1] = [(
    actor::NPC_GORON_BIGGORON_ADULT,
    flow::SUB_EVENT_GET_SWORD_BIG_GORON,
    "Step11",
)];

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

/// Givers who hand over their gift at a sale, as `(flow, values, item)`:
/// the values the last sale finds and the item it takes. The Poe collector
/// gives his bottle at the sale that reaches 1000 coupons, 100 a Big Poe
/// (Mac, 2026-11-15).
static SOLD: LazyLock<[(&Name, [String; 1], &Name); 1]> = LazyLock::new(|| {
    [(
        actor::NPC_HYLIA_048_BOTTLE_SHOP,
        [format!("{}=900", flag::GHOST_SHOP_COUPON_NUM)],
        item::BOTTLE_BIG_POE,
    )]
});

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

/// Where the placement named `label` stands in `map`.
fn anchor(romfs: &Romfs, map: &str, label: &str) -> Result<[f32; 3]> {
    let placed = warp::placements_in(romfs, map)?;
    (placed.iter())
        .find(|p| p.label.as_deref() == Some(label))
        .map(|p| p.pose.at)
        .with_context(|| format!("nothing in {map} is named {label}"))
}

type Maps = BTreeMap<String, HashMap<u64, Pose>>;
/// A gated holder's ready entries, the quest steps it waits on, and the
/// only age that opens it.
type Raise = (Ready, Steps, Option<bool>);

/// Plan every spot, in order.
pub fn plan(romfs: &Romfs, spots: &[Spot]) -> Result<Vec<Planned>> {
    let chests = (spots.iter())
        .filter(|s| s.holder.starts_with(CHEST))
        .map(|s| s.hash)
        .collect();
    let mut planner = Planner::new(romfs, chests);
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
    gamedata: Option<GameData>,
    flows: Option<Flows>,
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
            gamedata: None,
            flows: None,
        }
    }
}

impl Planner<'_> {
    fn spot(&mut self, spot: &Spot) -> Result<Planned> {
        let gift = GIFTS.iter().find(|g| g.hash() == spot.hash);
        let (trigger, child) = match gift {
            Some(gift) => self.gift(gift)?,
            None if cows::is_holder(&spot.holder) => self.cow(spot)?,
            None if spot.holder.starts_with(CHEST)
                || spot.holder.starts_with(scrubs::HOLDER)
                || pickups::within_reach(&spot.holder) =>
            {
                self.placed(spot)?
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
            side: Side::Front,
        })
    }

    /// The poses of every map in Banc folder `area`.
    fn maps(&mut self, area: &str) -> Result<&Maps> {
        if !self.areas.contains_key(area) {
            let maps = warp::poses_in(self.romfs, area)?;
            self.areas.insert(area.to_owned(), maps);
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
    /// sets `Npc_Goron_010_Delete`, which unlocks the chest behind him, and
    /// his rescue `Npc_Goron_010_Adult_FireTemple_Delete`. Without that one
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
    /// the only age its gate opens for.
    fn placed(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>)> {
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
                (Trigger::Skip(why), None)
            }
            Some((map, poses, pose, era)) => {
                let others: Vec<_> = (self.chests.iter())
                    .filter(|&&h| h != spot.hash)
                    .filter_map(|h| Some(poses.get(h)?.at))
                    .collect();
                let pose = &warp::facing(&spot.holder, pose);
                let (map, at) = (map.clone(), clear_landing(pose, &others));
                let face = pose.at;
                let trigger = match scrub {
                    false => Trigger::Open {
                        ready: [raise, noted(&spot.holder), water(&map)].concat(),
                        steps,
                        first: drain(&map),
                        map,
                        at,
                        face,
                        broken: pickups::broken(&spot.holder),
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
                (trigger, era)
            }
        })
    }

    /// A shop slot: talk to the keeper behind its shelf, and the judge
    /// browses to the item and buys it. The keeper is the shop NPC nearest
    /// the shelf in a map of its age: Kakariko's adult keepers stand in the
    /// alternative map, not the shelves'.
    fn shop(&mut self, spot: &Spot) -> Result<(Trigger, Option<bool>)> {
        let maps = self.maps(&spot.area)?;
        let shelf = maps
            .iter()
            .find_map(|(map, poses)| Some((warp::era(map), poses.get(&spot.hash)?.at)));
        let Some((era, shelf)) = shelf else {
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
            keepers.extend(shop.map(|p| (map.clone(), p.hash, p.pose, p.presence)));
        }
        let flat = |pose: &Pose| (pose.at[0] - shelf[0]).hypot(pose.at[2] - shelf[2]);
        let near = keepers
            .into_iter()
            .filter(|(_, _, pose, _)| flat(pose) < KEEPER_REACH);
        let Some((map, hash, pose, presence)) =
            near.min_by(|a, b| flat(&a.2).total_cmp(&flat(&b.2)))
        else {
            let why = format!("no shopkeeper by the shelf in {}", spot.area);
            return Ok((Trigger::Skip(why), None));
        };
        let (steps, ready) = self.bring(presence.as_ref())?;
        let customer = match hash {
            Some(hash) => self.customer(&map, hash, pose.at)?,
            None => None,
        };
        let trigger = Trigger::Talk {
            at: customer.unwrap_or_else(|| warp::landing(&pose, Side::Front)),
            counter: customer.is_some(),
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
    fn customer(&self, map: &str, hash: u64, at: [f32; 3]) -> Result<Option<[f32; 3]>> {
        let links = warp::linked(self.romfs, map, hash)?;
        let placed = warp::placements_in(self.romfs, map)?;
        Ok((placed.into_iter())
            .filter(|p| p.name == CUSTOMER && p.hash.is_some_and(|h| links.contains(&h)))
            .map(|p| p.pose.at)
            .find(|spot| dist(*spot, at) < COUNTER))
    }

    /// What brings in a keeper there only by `presence`: the quest step
    /// its BoolExp asks for (the Bombchu Shop's keeper comes once the
    /// Goron's Ruby quest is complete), or its Bool set true. Nothing for
    /// a keeper there while a flag is unset, or by any other expression.
    fn bring(&mut self, presence: Option<&Presence>) -> Result<(Steps, Ready)> {
        let Some(p) = presence.filter(|p| !p.negated) else {
            return Ok(Default::default());
        };
        Ok(match self.gamedata()?.expr(&p.flag)? {
            None => (vec![], vec![p.flag.clone()]),
            Some(expr) => (step_of(&expr).into_iter().collect(), vec![]),
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

    /// The Bools that make the Business Scrub placed as `hash` a seller,
    /// with every other scrub in `map`. Until its flag is set a scrub is an
    /// enemy, beaten in play by sending its nut back: its brain graph reads
    /// `IsNPC_ScrubBusiness` for its own instance id. A neighbour left an
    /// enemy shoots Link mid-talk, as in the Hyrule Field grotto of three.
    fn sellers(&self, map: &str, hash: u64) -> Result<Ready> {
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

    fn gift(&mut self, gift: &Gift) -> Result<(Trigger, Option<bool>)> {
        let drawn = DRAWN.iter().find(|(giver, _)| gift.caller() == Some(giver));
        if let Some(&(giver, button)) = drawn {
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
    fn talk(
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
        let sold = SOLD.iter().find(|(flow, ..)| *flow == giver);
        let mut ready: Ready = sold.map_or(vec![], |s| s.1.iter().map(|v| v.to_string()).collect());
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
            steps: steps(gift),
            ready,
            then,
            give: sold.map(|s| s.2.get()),
            counter: false,
            halt,
            map,
        };
        Ok((trigger, child))
    }

    /// The challenge's end, played beside the giver with `values` set.
    fn won(
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
    fn giver(&mut self, giver: &str, area: &str) -> Result<Option<(String, Pose)>> {
        let placed = warp::placements(self.romfs, giver)?;
        let maps = self.maps(area)?;
        Ok(placed.into_iter().find(|(map, _)| maps.contains_key(map)))
    }

    /// A song or a Great Fairy: the event that starts its scene.
    fn play(&mut self, gift: &Gift) -> Result<(Trigger, Option<bool>)> {
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
    fn step_in(&mut self, area: &str, map: &str, event: &str) -> Result<Option<[f32; 3]>> {
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
    fn event(&mut self, gift: &Gift) -> Result<Option<String>> {
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
    /// `SageOfFire_MelodyGetFire` (the other machine, 2026-11-14).
    fn skipped_in(&mut self, gift: &Gift) -> Result<Option<String>> {
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
    fn events(&mut self) -> Result<&BTreeMap<String, rsdb::Runs>> {
        if self.events.is_none() {
            self.events = Some(rsdb::events(self.romfs)?);
        }
        Ok(self.events.as_ref().unwrap())
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
/// directly or through a BoolExp like `SageOfSpirit_IsAfter_Step09_01_Exp`,
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
/// reads the level as it loads, so it's held like a flag.
static WATER: LazyLock<[(&Name, String); 1]> = LazyLock::new(|| {
    [(
        flag::WATER_TEMPLE,
        format!("{}=0", field::WATER_TEMPLE_ROOM00_WATER_LEVEL),
    )]
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
/// note chest (`HrlField_Note_11_Treaurebox`) opens empty and Navi talks.
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

/// Set when Link has said the Hyrule Field scrub's password. Its flow asks
/// only once a field note is read, then quizzes him, and with this set it
/// goes straight to the sale. No other scrub's flow reads it.
const PASSWORD: &Name = flag::ENEMY_SCRUB_BUSINESS_SECRET_SUCCESS;

/// In front of `pose` at `REACH`, swung off its front as little as keeps
/// every one of `others` `CLEAR` farther away than the holder, so A opens
/// the holder and not its neighbour. Straight in front when no swing does.
fn clear_landing(pose: &Pose, others: &[[f32; 3]]) -> [f32; 3] {
    let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[2] - b[2]);
    let swung = |degrees: f32| {
        let [x, y, z] = pose.rotate;
        let rotate = [x, y + degrees.to_radians(), z];
        warp::landing_at(
            &Pose {
                at: pose.at,
                rotate,
            },
            Side::Front,
            REACH,
        )
    };
    (SWINGS.iter().map(|&d| swung(d)))
        .find(|&at| others.iter().all(|&o| flat(at, o) >= REACH + CLEAR))
        .unwrap_or_else(|| swung(0.0))
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, field, flag, flow, item, map, scene};
    use std::sync::OnceLock;

    use super::*;
    use crate::gamedata::GameData;
    use crate::generate::{Extras, generate};
    use crate::settings::{self, Settings};
    use crate::steps;

    /// A seed with every shuffle setting on: its spots, and each planned
    /// once for all the tests.
    fn seed() -> Option<(Romfs, &'static [Spot], &'static [Planned])> {
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

    fn songs_and_fairies(planned: &[Planned]) -> Vec<&Planned> {
        let of = |p: &&Planned| {
            p.spot.vanilla.starts_with("Item_Melody_") || p.spot.holder.contains("Great Fairy")
        };
        planned.iter().filter(of).collect()
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
        // Through a BoolExp, `SageOfSpirit_IsAfter_Step09_01_Exp`.
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
        assert_eq!(
            water(flag::WATER_TEMPLE),
            [format!("{}=0", field::WATER_TEMPLE_ROOM00_WATER_LEVEL)]
        );
        assert!(water(flow::BOTTOM_OF_THE_WELL).is_empty());
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
        let p = (planned.iter())
            .find(|p| p.spot.holder == "Poe collector")
            .unwrap();
        let Trigger::Talk {
            map, ready, give, ..
        } = &p.trigger
        else {
            panic!("{:?}", p.trigger);
        };
        assert_eq!(map, flag::MARKET_ADULT);
        let set: Vec<_> = ready.iter().map(|e| preset(e)).collect();
        assert_eq!(set, [(&**flag::GHOST_SHOP_COUPON_NUM, "900")]);
        assert_eq!(*give, Some(&**item::BOTTLE_BIG_POE));
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
    fn a_chest_lands_clear_of_the_chest_in_front_of_it() {
        let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[2] - b[2]);
        // Mido's Back Right, and the Front Right chest 1.5 ahead of it.
        let back_right = Pose {
            at: [-50.3, 112.3, -42.0],
            rotate: [0.0, -0.48513162, 0.0],
        };
        let front_right = [-49.3, 112.3, -40.9];
        let ahead = warp::landing_at(&back_right, Side::Front, REACH);
        assert!(
            flat(ahead, front_right) < REACH,
            "straight ahead is crowded"
        );
        let at = clear_landing(&back_right, &[front_right]);
        assert!((flat(at, back_right.at) - REACH).abs() < 1e-4);
        assert!(flat(at, front_right) >= REACH + CLEAR, "{at:?}");
        assert_eq!(clear_landing(&back_right, &[]), ahead, "alone, straight on");
    }

    #[test]
    fn nearly_every_chest_opens_from_a_landing_inside_its_map() {
        let Some((romfs, _, planned)) = seed() else {
            return;
        };
        let chests: Vec<_> = planned
            .iter()
            .filter(|p| p.spot.holder.starts_with(CHEST))
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
        // front of his counter. The others from their front.
        let counters: Vec<(bool, [f32; 3])> = (planned.iter())
            .map(|p| match &p.trigger {
                Trigger::Talk { counter, at, .. } => (*counter, *at),
                t => panic!("{t:?}"),
            })
            .collect();
        assert_eq!(counters.iter().filter(|c| c.0).count(), 1, "{counters:?}");
        let (counter, at) = counters[1];
        assert!(counter);
        assert!(
            (at[0] - -17.1).abs() < 0.1 && (at[2] - -20.2).abs() < 0.1,
            "{at:?}"
        );
    }

    #[test]
    fn a_step_is_read_from_its_expression() {
        let ruby = Some((flow::GET_STONE_OF_FIRE.to_string(), "Complete".into()));
        assert_eq!(step_of("Step_Get_StoneOfFire == Complete"), ruby);
        assert_eq!(step_of("Step_Meet_DekuTree > Step02"), None);
        assert_eq!(step_of("Step_A == B && Step_C == D"), None);
        assert_eq!(step_of("IsGet.Item_Bow"), None);
    }
}
