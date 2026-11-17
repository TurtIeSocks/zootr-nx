//! Checks an NPC hands over in an event flow rather than a chest.

mod cucco;
mod cutscene;
mod later;
pub(crate) mod levels;
mod lines;
mod potion;
pub(crate) use potion::DONE as BOTTLE_TUTORIAL_DONE;
mod rewrite;
mod rolling;
mod sales;
mod table;
mod zora;

pub use cutscene::straight;
pub(crate) use later::scene_marks;
pub use lines::lines;
pub use table::GIFTS;

use cutscene::{cut, stay};
use later::{grant_after, silence_calls};
use rewrite::*;
use sales::sell;

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{EventKind, Flowchart, Link, Params, Romfs, Value};

use crate::checks::{Drop, ItemCheck};
use crate::cutscenes::{PLAYED, transition};
use crate::events::Flows;
use crate::flows::{
    self, ACTOR, FITS_POUCH, GAME_DATA, GRANT, IN_POUCH, IS_BOOL, Rewrites, SET_BOOL, is_local,
    only, param, params_mut, running, string,
};
use crate::known::{Name, actor, field, flag, flow, item, scene};
use crate::settings::{DUNGEON_REWARDS, MERCHANTS, NPC_GIFTS, OPEN_DOOR, SONGS, TRADE_CHAIN};
use crate::{cows, dampe, grant, pickups, scrubs, shops, starting};

pub struct Gift {
    pub flow: &'static Name,
    /// Banc folder, for the spoiler.
    pub area: &'static Name,
    pub giver: &'static str,
    pub item: &'static Name,
    pub how: How,
    /// The setting that deals it, or none when it's always a check.
    pub setting: Option<&'static str>,
}

/// An edit to one flow.
type Edit = fn(&mut Flowchart) -> Result<()>;

pub enum How {
    /// Rename the item in the grant and the "You got" window.
    InPlace,
    /// Replace the cutscene with a grant, then `then` if any, then its
    /// save changes.
    Cut { then: Option<&'static Name> },
    /// Rename the item in each call to `call` that names it, as
    /// `Flowchart::Entry`, or `::Entry` for one in the same flow, with `|`
    /// between calls when one gift goes through several. `found` is the
    /// Bool set once it's given, if the game sets one.
    Calls {
        call: &'static [Call],
        found: Option<&'static Name>,
    },
    /// For a cutscene that can't hold the grant: each call to `call` naming
    /// the item is emptied, and `caller`, the flow that starts the cutscene,
    /// grants the new item with its window right after the transition. A
    /// cutscene skip that swapped the transition for the scene's played flag
    /// gets the grant right after that instead, the vanilla item included,
    /// since the scene no longer plays.
    Later {
        call: &'static [Call],
        caller: &'static Name,
        found: &'static Name,
    },
    /// A sale of one item. The call to `call` at `price` that sells `sold`
    /// sells the dealt item instead, and `gate` and `shop` edit the giver's
    /// flow and the shop flow around it, whichever the sale needs.
    /// No grant in the vanilla game: `trade_chain` adds one, of the dealt
    /// item, to the giver's flow and to each flow in `also`, where the same
    /// giver stands in another place.
    Added { also: &'static [&'static Name] },
    Sale {
        call: &'static [Call],
        sold: &'static Name,
        price: i32,
        found: Option<&'static Name>,
        gate: Option<Edit>,
        shop: Edit,
    },
}

/// Scenes with a grant that a cutscene skip can cut, and the flow that starts
/// each: the skip swaps the transition for the scene's played flag, and the
/// grant is made right after it. Impa's lullaby is cut by `cutscenes::IMPA`,
/// the rest by the rule in `cut`, which keeps Sheik's Bolero, Minuet and
/// Serenade: the sweep still starts those through their callers.
const SKIPPED_IN: [(&Name, &Name); 17] = [
    (scene::MELODY_GET_ZELDA, flow::DOOR_OF_TIME),
    (scene::GORON_0030, flow::GET_STONE_OF_FIRE),
    (scene::DEATH_MT_TRAIL_0025, ESCAPE),
    (scene::ZORA_FOUNTAIN_0030, ESCAPE),
    (scene::AWAKE_FOREST_GET, ESCAPE),
    (scene::AWAKE_FIRE_GET, ESCAPE),
    (scene::AWAKE_WATER_GET, ESCAPE),
    (scene::AWAKE_SHADOW_GET, ESCAPE),
    (scene::AWAKE_SPIRIT_GET, ESCAPE),
    (scene::SAGE_ROOM_0005, flow::DOOR_OF_TIME),
    (scene::MELODY_GET_FOREST, flow::SAGE_OF_FOREST),
    (scene::MELODY_GET_FIRE, flow::SAGE_OF_FIRE),
    (scene::MELODY_GET_WATER, flow::SAGE_OF_WATER),
    (scene::MELODY_GET_SHADOW, scene::SY_BANC_CHANGE_ARRIVE),
    (scene::MELODY_GET_SHINE, flow::ALL_SAGE),
    (scene::MELODY_GET_STORM, actor::NPC_HYLIA_009_ADULT),
    (scene::TIME_TEMPLE_0130, flow::ALL_SAGE),
];

/// The flow every dungeon's boss warp leaves through.
const ESCAPE: &Name = scene::SY_BANC_CHANGE_ESCAPE_DUNGEON;

/// A call into an event flow, `Flowchart::Entry`, or `::Entry` for one in the
/// same flow.
#[derive(Clone, Copy)]
pub struct Call(Option<&'static Name>, &'static Name);

impl fmt::Display for Call {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Some(flow) => write!(f, "{flow}::{}", self.1),
            None => write!(f, "::{}", self.1),
        }
    }
}

/// `calls` joined by `|`, as a message or a hash says them.
fn said(calls: &[Call]) -> String {
    let said: Vec<_> = calls.iter().map(Call::to_string).collect();
    said.join("|")
}

const fn call(flow: &'static Name, entry: &'static Name) -> Call {
    Call(Some(flow), entry)
}

const fn local(entry: &'static Name) -> Call {
    Call(None, entry)
}

/// A call into the getting flow.
const fn get(entry: &'static Name) -> Call {
    call(scene::SY_GET_IN_EVENT, entry)
}

/// The getting sub-flows every NPC gift goes through.
const GET: Call = get(flow::GET_IN_EVENT);

const GET_QUIETLY: Call = get(flow::GET_IN_EVENT_NOT_TURN_NOT_NPC);

const HAND_OVER: Call = call(
    flow::NPC_COMMON,
    flow::NPC_COMMON_HAND_OVER_NPC_TO_PLAYER_NO_DISPLAY,
);

const HEART: &Name = actor::OBJ_HEART_PIECE;

/// The fishing pond's two prize paths, a plain win and a new record.
const FISHING: [Call; 2] = [HAND_OVER, get(flow::GET_IN_EVENT_NOT_TURN)];

/// How three Great Fairies hand over their gift.
const FAIRY: Call = get(flow::GET_IN_EVENT_DEMO_NOT_LOOKAT_OBJECT_ENTRY);

/// Biggoron, who hands over the sword three days after the claim check.
const BIGGORON: &Name = actor::NPC_GORON_BIGGORON_ADULT;

/// Medigoron, who sells the Giant's Knife once.
const MEDIGORON: &Name = actor::NPC_GORON_MEDIGORON_ADULT;

/// The call that starts his sale.
const MEDIGORON_SALE: Call = call(
    flow::SIMPLE_SHOP,
    flow::SIMPLE_SHOP_ROOT_NPC_GORON_MEDIGORON_ADULT,
);

/// The bean seller, whose first sale is a check.
const BEAN_SELLER: &Name = actor::NPC_HYLIA_047_CHILD;

/// The carpet merchant in the Haunted Wasteland, whose first sale is a
/// check.
const CARPET_MERCHANT: &Name = actor::NPC_HYLIA_044_ADULT;

/// The plain sale only he calls.
const CARPET_SALE: Call = call(flow::SIMPLE_SHOP, flow::SIMPLE_SHOP_ROOT);

/// What he sells: ten Bombchus, shown by the shop's stand-in.
const CARPET_BOMBCHUS: &Name = actor::SHOP_BOMBCHU_BUNDLE_01;

/// The Bool every plain sale sets. Only he calls it, so it is his own.
const CARPET_SOLD: &Name = flag::NPC_HYLIA_044_BUY;

/// The flow every simple sale goes through.
const SIMPLE_SHOP: &Name = flow::SIMPLE_SHOP;

/// The Bool Medigoron's sale sets, so that a sale of anything else still
/// happens once.
const MEDIGORON_SOLD: &str = "Randomizer_Medigoron";

/// The query for how long since a wait began. Case 0 is "long enough".
const ELAPSED: &Name = flow::EVENT_QUERY_CHECK_ELAPSED_TIME;

/// The flow every Great Fairy fountain's ocarina melody runs through.
const FOUNTAIN: &Name = flow::TRI_FORCE_POINT;

const FOUNTAIN_ENTRY: &Name = flow::SUB_PLAY_OCARINA_SEQUENCE_FAIRY_FOUNTAIN;

/// The Bool the game checks before any fountain but Din's opens.
const DIN_FIRE: &Name = flag::IS_GET_ITEM_DIN_FIRE;

/// Stands for the played flag of the gift's own scene, `HavePlayedEvent.`
/// then its flow, which the game's name tables don't list.
const SCENE_PLAYED: &Name = flag::HAVE_PLAYED_EVENT;

/// The plain grant with a get window, by item name.
const POUCH_GRANT: Call = call(
    flow::GET_POUCH_CONTENT,
    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
);

/// Rauru's scene, which the open Door of Time skips.
const RAURU: &Name = scene::SAGE_ROOM_0005;

/// The flow chests and NPCs hand items out through.
use crate::grant::POUCH;

/// The "You got" window.
const WINDOW: &Name = flow::DISPLAY_GET_BY_ACTOR_NAME;

/// An NPC's reward: the grant and its window.
const REWARD: &Name = flow::GET_POUCH_CONTENT_BY_ACTOR_NAME;

pub(super) const NAME: &Name = flag::ACTOR_NAME;

/// What a Bool about an item's name starts with.
const IS_GET: &str = "IsGet.";

/// The song lesson, which needs an ocarina.
const LESSON: &Name = scene::SY_OCARINA;

const ENTRY: &Name = flag::ENTRY_POINT0;

impl Gift {
    /// A stable id that reads like a Banc hash, for plando and history.
    pub fn hash(&self) -> u64 {
        match self.how {
            How::Calls { call, .. } | How::Later { call, .. } | How::Sale { call, .. } => {
                crate::known::hash(&format!("{}{}{}", self.flow, said(call), self.item))
            }
            _ => crate::known::hash(self.flow),
        }
    }

    /// The GameData Bool set once it's given: a cutscene's played flag, an
    /// NPC's own, or one the seed adds. Biggoron sets none.
    pub fn found(&self) -> Option<String> {
        if let Some((.., own)) = OWN.iter().find(|(flow, ..)| *flow == self.flow) {
            return Some((*own).into());
        }
        if let Some(added) = self.added() {
            return Some(added);
        }
        let flag = |found: &Name| match found == SCENE_PLAYED {
            true => format!("{PLAYED}.{}", self.flow),
            false => found.to_string(),
        };
        match self.how {
            How::Calls { found, .. } | How::Sale { found, .. } => found.map(flag),
            How::Later { found, .. } => Some(flag(found)),
            How::Added { .. } => None,
            _ => Some(flag(SCENE_PLAYED)),
        }
    }

    /// The Bool the seed adds and sets right after its grant, for a gift
    /// the game marks with nothing of its own.
    fn added(&self) -> Option<String> {
        let own = OWN.iter().any(|(flow, ..)| *flow == self.flow);
        let none = matches!(self.how, How::Calls { found: None, .. });
        (none && !own).then(|| format!("Rando_Gift_{:016x}", self.hash()))
    }

    /// Whether a new game with `on` deals it: its setting is on, and it's
    /// not Rauru's with the Door of Time open.
    pub fn dealt(&self, on: &BTreeSet<String>) -> bool {
        self.setting().is_none_or(|s| on.contains(s))
            && !(self.flow == RAURU && on.contains(OPEN_DOOR))
    }

    /// The flow that starts its scene, when the grant can move there: a
    /// `Later` gift's caller, or the one a skip swaps a song's scene in.
    pub(crate) fn caller(&self) -> Option<&'static str> {
        match self.how {
            How::Later { caller, .. } => Some(caller.get()),
            _ => SKIPPED_IN
                .iter()
                .find(|s| s.0 == self.flow)
                .map(|s| s.1.get()),
        }
    }

    /// Whether a cutscene skip already swapped its scene's transition in
    /// `rewrites`, so the scene never plays and its own grant is lost.
    fn skipped(&self, rewrites: &Rewrites) -> Result<bool> {
        let Some(caller) = self.caller() else {
            return Ok(false);
        };
        let Some(data) = rewrites.get(&flows::rel(caller)) else {
            return Ok(false);
        };
        let flow = Flowchart::parse(data)?.with_context(|| format!("{caller} is a timeline"))?;
        Ok(scene_marks(&flow, self.flow).0.is_empty())
    }

    /// The items its scene grants: the gift's own, and the one a cut
    /// `then` hands over after it.
    pub(crate) fn items(&self) -> Vec<&'static str> {
        let then = match self.how {
            How::Cut { then } => then,
            _ => None,
        };
        std::iter::once(self.item)
            .chain(then)
            .map(|n| n.get())
            .collect()
    }

    /// The setting that deals it, if only one does.
    pub fn setting(&self) -> Option<&'static str> {
        self.setting
    }

    /// Whether it's a sale, which keeps selling what it sold.
    pub fn is_sale(&self) -> bool {
        matches!(self.how, How::Sale { .. })
    }
}

/// The gifts a scene hands over.
pub(crate) fn of_scene(scene: &str) -> Vec<&'static Gift> {
    GIFTS.iter().filter(|g| g.flow == scene).collect()
}

/// A check an event hands out, rather than a placed actor.
pub(crate) fn check(file: String, area: &str, hash: u64, holder: String, item: &str) -> ItemCheck {
    ItemCheck {
        file,
        area: area.into(),
        hash,
        holder,
        item: item.to_string(),
        get_event: None,
        substitute: None,
        presence: None,
    }
}

/// The item the seed puts at `hash`, else `vanilla`.
pub(crate) fn item_at<'a>(drops: &'a HashMap<u64, Drop>, hash: u64, vanilla: &'a str) -> &'a str {
    drops.get(&hash).map_or(vanilla, |d| d.item.as_str())
}

/// One check per gift.
pub fn checks() -> Vec<ItemCheck> {
    GIFTS
        .iter()
        .map(|g| check(flows::rel(g.flow), g.area, g.hash(), g.giver.into(), g.item))
        .collect()
}

pub fn of(check: &ItemCheck) -> Option<&'static Gift> {
    GIFTS.iter().find(|g| g.hash() == check.hash)
}

/// Whether `check` is a gift that gives ammo or a line's level by what Link
/// owns.
pub(crate) fn by_level(check: &ItemCheck) -> bool {
    of(check).is_some_and(levels::chained)
}

/// The minigame chests that are checks, by placement hash: the Treasure
/// Chest Game's heart piece, then Dampe's two. Every other chest whose event
/// starts `MiniGame_` stays vanilla. Gifts in all but where they're placed,
/// since a chest of the game can't turn into a substitute one.
pub(crate) const PRIZE_CHESTS: [u64; 3] = [
    16881481538337649070,
    dampe::HOOKSHOT_CHEST,
    dampe::HEART_CHEST,
];

pub(crate) fn is_prize(check: &ItemCheck) -> bool {
    PRIZE_CHESTS.contains(&check.hash)
}

/// Saria's and Zelda's, every business scrub's, shop slot's, cow's and
/// minigame prize chest's.
pub fn is_gift(check: &ItemCheck) -> bool {
    of(check).is_some()
        || is_prize(check)
        || scrubs::of(check).is_some()
        || shops::is_shop(check)
        || pickups::is_pickup(check)
        || cows::is_cow(check)
}

/// Whether `holder` is a gift's: a giver's name, a scrub's, a shop slot's,
/// a cow's or a thief's.
pub fn is_giver(holder: &str) -> bool {
    GIFTS.iter().any(|g| g.giver == holder)
        || holder.starts_with(scrubs::HOLDER)
        || shops::is_holder(holder)
        || cows::is_holder(holder)
        || holder == pickups::THIEF
}

/// Windows in another flow that show a gift's item before it's given, as
/// the gift's flow, then the window's flow and call. Ruto shows the Zora
/// Sapphire in Jabu-Jabu, and the fountain scene grants it.
const FORETOLD: [(&Name, &Name, &[Call]); 1] = [(
    scene::ZORA_FOUNTAIN_0030,
    scene::INSIDE_JABU_JABU_0030,
    &[call(
        flow::GET_POUCH_CONTENT,
        flow::DISPLAY_RUTO_GET_BY_ACTOR_NAME,
    )],
)];

/// Lessons in another flow that teach a gift's song again, as the gift's
/// flow, then the lesson's. The lesson flow is emptied when the item changes.
const TAUGHT: [(&Name, &Name); 1] = [(scene::MELODY_GET_SUN_03, scene::MELODY_GET_SUN)];

/// The Bools the gifts' edits add, each set once its gift is given: the
/// gift's flow, its setting, the Bool.
const OWN: [(&Name, &str, &str); 3] = [
    (MEDIGORON, MERCHANTS, MEDIGORON_SOLD),
    (flow::SAGE_OF_WATER, NPC_GIFTS, zora::GIVEN),
    (
        actor::NPC_HYLIA_TALON_ADULT_KAKARIKO,
        TRADE_CHAIN,
        crate::trade_chain::TALON_GAVE,
    ),
];

/// The Bools the gifts' edits set, which a new game adds to its save data.
pub fn flags(on: &BTreeSet<String>) -> Vec<String> {
    let own = (OWN.iter())
        .filter(|(_, setting, _)| on.contains(*setting))
        .map(|(.., flag)| (*flag).into());
    let added = GIFTS.iter().filter(|g| g.dealt(on)).filter_map(Gift::added);
    own.chain(added).collect()
}

/// Flows a dealt gift would leave stuck, each with the edit that frees it.
const UNSTUCK: [(&Name, Edit); 3] = [
    (cucco::FLOW, cucco::open),
    (potion::FLOW, potion::cleared_with_the_gift),
    (
        actor::NPC_HYLIA_048_BOTTLE_SHOP,
        sales::rewards_into_a_full_wallet,
    ),
];

/// Write each gift whose item changed, and Zelda's always, so her lesson
/// never plays without an ocarina. Biggoron never makes Link wait, sword
/// moved or not. Every Great Fairy opens in any order.
pub fn give_all(
    romfs: &Romfs,
    flows: &Flows,
    rewrites: &mut Rewrites,
    drops: &HashMap<u64, Drop>,
    progressive: bool,
    on: &BTreeSet<String>,
) -> Result<()> {
    let mut set_bool = None;
    // Chained once every gift is renamed: a chain names other levels, maybe
    // another gift's own item in the same flow.
    let mut by_level = BTreeSet::new();
    // The events each flow's gifts took so far, so a flow-mate dealt one's
    // item never renames or chains the other's.
    let mut taken: HashMap<&str, BTreeSet<u16>> = HashMap::new();
    for gift in &GIFTS {
        let item = item_at(drops, gift.hash(), gift.item);
        let mine = taken.entry(gift.flow.get()).or_default();
        if let Some(line) = picked(gift.item).filter(|_| item != gift.item) {
            flows::edit(romfs, rewrites, gift.flow, |f| {
                one_of(f, line, gift.item, mine)
            })?;
        }
        for (_, free) in UNSTUCK
            .iter()
            .filter(|u| u.0 == gift.flow && item != gift.item)
        {
            flows::edit(romfs, rewrites, gift.flow, free)?;
        }
        let lost = gift.skipped(rewrites)?;
        let given = item != gift.item || matches!(gift.how, How::Cut { .. }) || lost;
        let took = match (given, &gift.how) {
            (true, _) => give(romfs, flows, rewrites, gift, item, mine)
                .with_context(|| format!("{} giving {item}", gift.giver))?,
            (false, How::Calls { call, .. }) => {
                let flow = flows::current(romfs, rewrites, gift.flow)?;
                naming(&flow, call, item, mine)?
            }
            _ => Vec::new(),
        };
        mine.extend(&took);
        if let Some(flag) = gift.added().filter(|_| gift.dealt(on)) {
            let set_bool = match &set_bool {
                Some(s) => s,
                None => set_bool.insert(crate::chest_levels::setter(romfs)?),
            };
            flows::edit(romfs, rewrites, gift.flow, |f| {
                (took.iter()).try_for_each(|&at| {
                    flows::set_after(f, set_bool, at, &flag, Value::Bool(vec![true]))
                })
            })?;
        }
        if levels::needs(gift, item, progressive) {
            // The grant moved to the caller when its scene can't play.
            let later = given && matches!(gift.how, How::Later { .. });
            let at = gift.caller().filter(|_| lost || later);
            let only = (at.is_none() && !took.is_empty()).then_some(took);
            by_level.insert((at.unwrap_or(gift.flow), item, only));
            if let How::Added { also } = gift.how {
                by_level.extend(also.iter().map(|flow| (flow.get(), item, None)));
            }
        }
        if item == gift.item {
            continue;
        }
        if gift.flow == zora::FLOW {
            zora::deal(romfs, rewrites, item)?;
        }
        for (_, flow, call) in FORETOLD.iter().filter(|f| f.0 == gift.flow) {
            flows::edit(romfs, rewrites, flow, |f| {
                rename_calls(f, call, gift.item, item)
            })?;
        }
        for (_, lesson) in TAUGHT.iter().filter(|t| t.0 == gift.flow) {
            flows::edit(romfs, rewrites, lesson, empty)?;
        }
    }
    lines::retalk(romfs, rewrites, drops)?;
    if !by_level.is_empty() {
        let model = levels::model(flows)?;
        for (flow, item, only) in by_level {
            flows::edit(romfs, rewrites, flow, |f| {
                levels::by_level(f, &model, item, only.as_deref())
            })
            .with_context(|| format!("{flow} giving {item} by level"))?;
        }
    }
    flows::edit(romfs, rewrites, BIGGORON, waited)?;
    flows::edit(romfs, rewrites, FOUNTAIN, ungated)
}

/// Rewrite `gift` to give `item`, leaving alone the events of its flow
/// another gift has `taken`. Returns the events of its flow that give it.
fn give(
    romfs: &Romfs,
    flows: &Flows,
    rewrites: &mut Rewrites,
    gift: &Gift,
    item: &str,
    taken: &BTreeSet<u16>,
) -> Result<Vec<u16>> {
    let mut back = None;
    let mut took = Vec::new();
    let lost = gift.skipped(rewrites)?;
    // A scene that never plays has nothing to cut or silence.
    if !lost {
        flows::edit(romfs, rewrites, gift.flow, |flow| match gift.how {
            How::InPlace => in_place(flow, item),
            How::Cut { then } => cut(flow, flows, gift.item, item, then).map(|(b, given)| {
                back = b;
                took = vec![given];
            }),
            How::Calls { call, .. } => {
                rename_untaken(flow, call, gift.item, item, taken).map(|t| took = t)
            }
            How::Later { call, .. } => silence_calls(flow, call, gift.item),
            How::Added { .. } => Ok(()),
            How::Sale {
                call,
                sold,
                price,
                gate,
                ..
            } => {
                sell(flow, call, sold, price, item).and_then(|()| gate.map_or(Ok(()), |g| g(flow)))
            }
        })?;
        if let How::Sale { shop, .. } = gift.how {
            flows::edit(romfs, rewrites, SIMPLE_SHOP, shop)?;
        }
    }
    if let Some(caller) = gift
        .caller()
        .filter(|_| lost || matches!(gift.how, How::Later { .. }))
    {
        flows::edit(romfs, rewrites, caller, |f| {
            grant_after(f, flows, gift.flow, item)
        })?;
    }
    if let Some((callee, entry)) = back {
        flows::edit(romfs, rewrites, &callee, |f| stay(f, &entry))?;
    }
    Ok(took)
}

#[cfg(test)]
/// A flow before and after `f`, rendered, as the game would read it.
pub(super) fn edited(
    romfs: &Romfs,
    name: &str,
    f: impl FnOnce(&mut Flowchart) -> Result<()>,
) -> (String, String) {
    let mut flow = flows::load(romfs, name).unwrap().unwrap();
    let before = flows::render(&flow);
    f(&mut flow).unwrap();
    let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
    (before, flows::render(&after))
}

#[cfg(test)]
/// The lines of `after` that differ from `before` at the same place.
pub(super) fn changed<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
    before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::known::{field, flow, scene};

    /// Sheik's songs whose scene the rule keeps: each calls `AllSage`, which
    /// the event that plays it doesn't bind, so it plays with its own grant.
    pub(super) const UNBOUND: [&Name; 3] = [
        scene::MELODY_GET_FOREST,
        scene::MELODY_GET_FIRE,
        scene::MELODY_GET_WATER,
    ];

    /// Each event the entry point runs, rendered, following `next`.
    pub(super) fn chain(flow: &Flowchart) -> Vec<String> {
        let lines: Vec<String> = flows::render(flow).lines().map(str::to_owned).collect();
        let entry = flow.entry_points.iter().find(|e| e.name == ENTRY).unwrap();
        let mut out = Vec::new();
        let mut at = entry.main;
        while let Some(e) = at {
            let line = lines
                .iter()
                .find(|l| l.starts_with(&format!("#{e} ")))
                .unwrap();
            out.push(line.split_once(' ').unwrap().1.to_owned());
            at = match &flow.events[e as usize].kind {
                EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } => *next,
                _ => None,
            };
        }
        out
    }

    pub(super) fn strip(line: &str) -> &str {
        line.rsplit_once(" -> ").map_or(line, |(l, _)| l)
    }

    #[test]
    fn a_gift_may_take_what_a_gift_of_its_own_flow_gives() {
        needs_names!();
        let at = GIFTS
            .iter()
            .position(|g| g.flow == flow::MINIGAME_HORSEBACK_ARCHERY && g.item == HEART);
        let heart = &checks()[at.unwrap()];
        assert!(crate::shuffle::allowed(heart, item::QUIVER_MEDIUM, true));
        assert!(crate::shuffle::allowed(heart, item::QUIVER_MEDIUM, false));
    }

    #[test]
    pub(super) fn rauru_is_dealt_only_with_the_door_closed() {
        let rauru = GIFTS.iter().find(|g| g.flow == RAURU).unwrap();
        let on = |ids: &[&str]| ids.iter().map(|&i| i.to_owned()).collect();
        assert!(rauru.dealt(&on(&[DUNGEON_REWARDS])));
        assert!(!rauru.dealt(&on(&[DUNGEON_REWARDS, OPEN_DOOR])));
        assert!(!rauru.dealt(&on(&[])));
    }

    #[test]
    pub(super) fn no_placed_actor_shares_a_gift_hash() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let hashes: std::collections::BTreeSet<u64> = GIFTS.iter().map(Gift::hash).collect();
        assert_eq!(hashes.len(), GIFTS.len());
        let checks = crate::checks::scan(&romfs).unwrap();
        assert!(checks.iter().all(|c| !hashes.contains(&c.hash)));
    }

    #[test]
    pub(super) fn no_cutscene_skip_steps_over_a_gift() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = crate::cutscenes::Catalogue::load(&romfs).unwrap();
        let every = crate::cutscenes::Skips {
            intro: true,
            saria: true,
            owl: true,
            great_deku: true,
            arrivals: true,
            areas: crate::cutscenes::AREAS.iter().map(|a| a.0).collect(),
        };
        let picked = catalogue.picked(&every);
        assert!(!picked.is_empty());
        // Zelda's letter and Impa's lullaby are swapped for their played
        // flags, and their grants move to where the flags are set.
        for gift in &GIFTS {
            let moved = gift.caller().is_some();
            assert!(
                moved || picked.iter().all(|c| c.name != gift.flow),
                "{}",
                gift.flow
            );
        }
    }

    /// Every event `entry` can reach, in the order it runs them: each
    /// event, then what follows it, a fork's branches before its join.
    pub(super) fn reach(flow: &Flowchart, entry: &str) -> Vec<u16> {
        let entry = flow.entry_points.iter().find(|e| e.name == entry);
        let mut stack: Vec<u16> = entry.unwrap().main.into_iter().collect();
        let mut out = Vec::new();
        while let Some(i) = stack.pop() {
            if out.contains(&i) {
                continue;
            }
            out.push(i);
            let after: Vec<u16> = match &flow.events[i as usize].kind {
                EventKind::Action { next, .. }
                | EventKind::SubFlow { next, .. }
                | EventKind::Join { next } => next.iter().copied().collect(),
                EventKind::Switch { cases, .. } | EventKind::SubFlowSwitch { cases, .. } => {
                    cases.iter().map(|c| c.1).collect()
                }
                EventKind::Fork { join, branches } => {
                    branches.iter().chain([join]).copied().collect()
                }
                EventKind::Return { .. } => vec![],
            };
            stack.extend(after.into_iter().rev());
        }
        out
    }

    /// Each of `flow`'s events, rendered without its number or its next.
    pub(super) fn shown(flow: &Flowchart) -> Vec<String> {
        flows::render(flow)
            .lines()
            .filter(|l| l.starts_with('#'))
            .map(|l| strip(l.split_once(' ').unwrap().1).to_owned())
            .collect()
    }

    /// `reach`, rendered, one line per event without its next.
    pub(super) fn run(flow: &Flowchart, entry: &str) -> Vec<String> {
        let lines = shown(flow);
        reach(flow, entry)
            .into_iter()
            .map(|i| lines[i as usize].clone())
            .collect()
    }

    /// `flow`'s event `e`, rendered without its number or its next.
    pub(super) fn event(flow: &Flowchart, e: u16) -> String {
        let line = flows::render(flow)
            .lines()
            .find(|l| l.starts_with(&format!("#{e} ")))
            .unwrap()
            .to_owned();
        strip(line.split_once(' ').unwrap().1).to_owned()
    }

    pub(super) fn sets(scene: &str) -> String {
        format!(
            r#"{}="{PLAYED}.{scene}" Index=-1 Value=true"#,
            field::GAME_DATA_NAME
        )
    }

    pub(super) fn is_set(line: &str, scene: &str) -> bool {
        line.starts_with(&format!(
            "{}.{}",
            field::GAME_DATA_ACTOR,
            flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL
        )) && line.contains(&sets(scene))
    }

    /// The fade-in a moved grant opens with, a copy of the castle flow's own.
    pub(super) const FADE: &str =
        "UISystemActor.EventEventFadeIn(Length=1 IsWaitEnd=true IsNoPlayAnime=false Color=0)";

    /// The grant, then the get window's two lines.
    pub(super) fn granted(item: &str) -> Vec<String> {
        let talk = |id: &str| {
            format!(
                r#"{}.{}({}=true {}=true {}=0 {}="" {}=false {}=false {}=0 {}=0 {}=0 {}=0 {}=0 {}=0 {}=0 {}=false {}=false {}=false {}=false {}=false {}=true {}=4 {}="{id}" {}=0)"#,
                field::SYSTEM_TEXT_NPC,
                flow::EVENT_TALK,
                field::IS_WAIT_END,
                field::IS_CLOSE_DIALOG,
                field::END_DIALOG_OPTION,
                field::AS_NAME,
                field::IS_ANIME_DRIVEN,
                field::IS_PLAY_AS_ALL_SLOT,
                field::CHOICE_NUMBER,
                field::CHOICE_LABEL1,
                field::CHOICE_LABEL2,
                field::CHOICE_LABEL3,
                field::CHOICE_LABEL4,
                field::CHOICE_DEFAULT_NO,
                field::CHOICE_CANCEL_NO,
                field::IS_NOT_BECOME_SPEAKER,
                field::CHANGE_MSTXT_FOR_ACTOR,
                field::IS_ANONYMOUS,
                field::NOT_PLAY_LIP_SYNC_ANIM,
                field::IS_NOT_OPEN_IF_SKIPED_MOST_RECENT,
                field::IS_OVERRIDE_STYLE,
                field::OVERRIDE_STYLE,
                field::MESSAGE_ID,
                field::MAINTAIN_PERFORMANCE_STATE
            )
        };
        [granting(item)]
            .into_iter()
            .chain(grant::GET_LINES.iter().map(|l| talk(l)))
            .collect()
    }

    pub(super) const GRANT_PREFIX: &str = "PouchSystemActor.EventTriggerAddOrRemovePouchContent(";

    pub(super) fn granting(item: &str) -> String {
        format!(
            r#"{}.{}(Num=1 {}=false {}=true Actor="{item}" {}=false)"#,
            field::POUCH_SYSTEM_ACTOR,
            flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT,
            field::IS_EQUIP_D_IRECT,
            field::IS_SET_MESSAGE_TARGET_ACTOR_NAME,
            field::IS_GET_EVENT
        )
    }

    pub(super) fn skip_all() -> crate::cutscenes::Skips {
        crate::cutscenes::Skips {
            intro: true,
            saria: true,
            owl: true,
            great_deku: true,
            arrivals: true,
            areas: crate::cutscenes::AREAS.iter().map(|a| a.0).collect(),
        }
    }

    /// What `skips` and the gifts write, with `dealt` for `item` placed at
    /// the gift whose scene is `flow`.
    pub(super) fn seed(
        romfs: &Romfs,
        skips: &crate::cutscenes::Skips,
        dealt: Option<(&Name, &Name)>,
    ) -> (Rewrites, Vec<String>) {
        let (mut rewrites, flags) = crate::cutscenes::skip(romfs, skips).unwrap();
        let drops: HashMap<u64, Drop> = dealt
            .into_iter()
            .map(|(flow, item)| {
                let gift = GIFTS.iter().find(|g| g.flow == flow).unwrap();
                let drop = Drop {
                    item: item.to_string(),
                    swap: None,
                };
                (gift.hash(), drop)
            })
            .collect();
        let all = Flows::load(romfs).unwrap();
        give_all(romfs, &all, &mut rewrites, &drops, false, &Default::default()).unwrap();
        (rewrites, flags)
    }

    /// Each flow holding several gifts, with them in table order.
    fn shared() -> Vec<Vec<&'static Gift>> {
        let mut by_flow: Vec<Vec<&Gift>> = Vec::new();
        for gift in &GIFTS {
            match by_flow.iter_mut().find(|f| f[0].flow == gift.flow) {
                Some(flow) => flow.push(gift),
                None => by_flow.push(vec![gift]),
            }
        }
        by_flow.retain(|f| f.len() > 1);
        by_flow
    }

    /// `flow` as `give_all` writes it with each gift dealt its item.
    fn dealt_flow(romfs: &Romfs, all: &Flows, deal: &[(&Gift, String)]) -> String {
        let drops = (deal.iter())
            .map(|(g, item)| (g.hash(), Drop::from(item.clone())))
            .collect();
        let mut rewrites = Rewrites::default();
        give_all(romfs, all, &mut rewrites, &drops, false, &Default::default()).unwrap();
        flows::render(&written(&rewrites, deal[0].0.flow))
    }

    #[test]
    fn gifts_of_one_flow_hold_each_others_items() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let mut wrong = Vec::new();
        for gifts in shared() {
            // Each gift a flow-mate's item, where that's another item.
            let next = |i: usize| gifts[(i + 1) % gifts.len()].item.to_string();
            let held: Vec<(usize, &Gift)> = (gifts.iter().copied().enumerate())
                .filter(|&(i, g)| next(i) != **g.item)
                .collect();
            if held.is_empty() {
                continue;
            }
            // Made-up items clash with nothing, so they mark where each
            // gift's grant belongs.
            let fake = |i: usize| format!("Item_Fake{i:02}");
            let marked: Vec<_> = held.iter().map(|&(i, g)| (g, fake(i))).collect();
            let traded: Vec<_> = held.iter().map(|&(i, g)| (g, next(i))).collect();
            let mut want = dealt_flow(&romfs, &all, &marked);
            for &(i, _) in &held {
                want = want.replace(&fake(i), &next(i));
            }
            if dealt_flow(&romfs, &all, &traded) != want {
                wrong.push(gifts[0].flow.to_string());
            }
        }
        assert!(
            wrong.is_empty(),
            "a gift renamed a flow-mate's grant: {wrong:?}"
        );
    }

    #[test]
    fn each_dealt_gift_the_game_marks_with_nothing_sets_its_own_bool() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let on: BTreeSet<String> = [NPC_GIFTS, MERCHANTS, TRADE_CHAIN]
            .map(String::from)
            .into();
        let added: Vec<&Gift> = (GIFTS.iter())
            .filter(|g| g.dealt(&on) && g.added().is_some())
            .collect();
        assert!(added.len() > 10, "{} gifts", added.len());
        // As dealt, then each holding the next one's item, chained by level.
        let next = |i: usize| (added[(i + 1) % added.len()].hash(), added[i].item);
        let rotated = (0..added.len()).map(|i| (next(i).0, Drop::from(next(i).1.to_string())));
        for (drops, progressive) in [(HashMap::new(), false), (rotated.collect(), true)] {
            let mut rewrites = Rewrites::default();
            give_all(&romfs, &all, &mut rewrites, &drops, progressive, &on).unwrap();
            for gift in &added {
                let flag = gift.added().unwrap();
                let flow = flows::render(&written(&rewrites, gift.flow));
                assert!(flow.contains(&flag), "{} sets no {flag}", gift.giver);
            }
        }
        let mut rewrites = Rewrites::default();
        give_all(&romfs, &all, &mut rewrites, &HashMap::new(), false, &Default::default()).unwrap();
        let flag = added[0].added().unwrap();
        let untouched = rewrites.get(&flows::rel(added[0].flow));
        assert!(untouched.is_none_or(|b| !flows::render(&Flowchart::parse(b).unwrap().unwrap()).contains(&flag)));
    }

    pub(super) fn written(rewrites: &Rewrites, flow: &str) -> Flowchart {
        let bytes = &rewrites[&flows::rel(flow)];
        Flowchart::parse(bytes).unwrap().unwrap()
    }

    /// How many times `flow` grants `item` through the reward call.
    pub(super) fn grants(flow: &Flowchart, item: &str) -> usize {
        flows::render(flow).matches(&granting(item)).count()
    }
}
