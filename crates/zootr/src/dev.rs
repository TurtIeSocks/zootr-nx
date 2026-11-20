//! The dev panel's commands for the running game. A teleport, an age
//! switch, the time of day, a quest step, an item to give, an event to
//! start and a warp beside an actor are left in the hook's mailbox, which runs them on Link's next
//! update (see `zootr_hook::Mailbox`). The state and set are plain reads and writes
//! through [`live`].

use std::collections::BTreeSet;
use std::mem::offset_of;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use zootr_hook::{Ask, GUARD, MAILBOX, Mailbox, NAME, Notice, Outcome, SWITCHES, switches, traps};

use crate::ammo;
use crate::checks::{Swap, flat};
use crate::gamedata::GameData;
use crate::known::{Name, flag, scene};
use crate::live::{self, Game, Memory};
use crate::patch;

mod place;
mod setup;
mod slay;

pub use place::{Place, known, other_age, places};
pub use setup::{Applied, Goal, Value, value};
use setup::{Grant, grants, plan_give};
pub(crate) use slay::distance;
pub use slay::{Fate, Foe, Lines, Slain, Target};

/// The arrival event a teleport plays. Some areas may want their own.
const ARRIVE: &Name = scene::SY_BANC_CHANGE_ARRIVE;
pub(crate) const CHILD: &str = "_Child";
pub(crate) const ADULT: &str = "_Adult";
/// Link's yaw (GameData, unnamed).
const LINK_YAW: u32 = 0x1ea93bd8;
/// The Iron Boots on (a Bool): Link sinks, and the HUD shows them. A
/// member of `PlayerStatus` no name table names, found by putting them on
/// from the menu (Mac, 2026-11-17).
const BOOTS_ON: &str = "#a6719419";
/// Link's live hearts, in quarters (`PlayerStatus.Life` is only the save's
/// copy).
pub(crate) const HEARTS: u32 = switches::HEARTS;
/// How long Link has to pick a request up.
const WAIT: Duration = Duration::from_secs(3);
/// GameData names the state reads.
const MAP: &Name = flag::SEQUENCE_CURRENT_BANC;
const IS_CHILD: &Name = flag::SEQUENCE_IS_CHILDHOOD;
const LOADING: &Name = flag::SEQUENCE_IS_LOADING_BANC;
/// Set as the title's menu opens.
const TITLE: &Name = flag::UI_DEBUG_IS_OPENED_TITLE;
/// The event manager's playing event.
const PLAYING: u64 = 104;
/// How long a quest step's or grant's event gets to finish, and how often to look. The
/// first look waits a little, since the event starts after the request.
const SETTLE: Duration = Duration::from_secs(10);
/// How long a teleport gets to land and its arrival to end.
const LAND: Duration = Duration::from_secs(30);
/// How long a warp gets to hold, how long it's given before looking, and
/// how near the spot Link must stand, measured flat.
const HOLD: Duration = Duration::from_secs(15);
const STAY: Duration = Duration::from_millis(500);
const HELD: f32 = 1.0;
/// How far off the spot Link may settle, a step down a slope or off a
/// ledge's lip, and still count as there when he settles in the same place
/// twice: the walk after a warp reaches its goal from it.
const SETTLED: f32 = 3.0;
const TICK: Duration = Duration::from_millis(100);

/// What the game is doing, as far as a teleport cares, and the dev
/// switches it holds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename = "DevState")]
pub struct State {
    pub map: String,
    pub child: bool,
    pub loading: bool,
    /// An event is playing.
    pub event: bool,
    pub switches: Switches,
}

/// The dev switches: what the hook keeps at its cap on Link's update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename = "DevSwitches")]
pub struct Switches {
    pub health: bool,
    pub magic: bool,
    /// Every ammo kind and rupees.
    pub resources: bool,
}

impl Switches {
    const BITS: [u32; 3] = [switches::HEALTH, switches::MAGIC, switches::RESOURCES];

    /// The word the hook reads.
    pub fn word(self) -> u32 {
        let on = [self.health, self.magic, self.resources];
        (Self::BITS.iter().zip(on))
            .filter(|(_, on)| *on)
            .fold(0, |word, (bit, _)| word | bit)
    }

    pub fn from_word(word: u32) -> Self {
        let [health, magic, resources] = Self::BITS.map(|bit| word & bit != 0);
        Self {
            health,
            magic,
            resources,
        }
    }
}

/// A request for the hook.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Teleport(Place),
    /// Switch age and land here.
    Age(Place),
    Time(u32),
    /// Move a quest to one of its steps.
    Step(String, String),
    /// Grant an item actor, equipped or not, as the game does.
    Give(String, bool),
    /// Grant the level of the actor's progressive line Link needs next, or
    /// with a swap, its `then` until Link owns what fires it.
    GiveLevel(String, Option<Swap>),
    /// Start an event by name, with Link as its starter.
    Play(String),
    /// Move Link to a point of the current map.
    Warp([f32; 3]),
    /// Where Link stands.
    Where,
    /// A received item's banner, with the popup for the item actor of a
    /// [`Notice::Item`]. Its words are GameData: see [`Dev::notice`].
    Notice(String, Notice),
}

/// Why the hook refused a request while an event plays.
pub const EVENT_PLAYING: &str = "an event is playing";
/// What a request the event manager answered 0 to did.
const UNANSWERED: &str = "sent, but the event manager answered 0";
/// How [`Dev::go_as`] fails when the game ignores the teleport, as it
/// does right after a save loads.
pub const NOT_TAKEN: &str = "the game didn't take the teleport";
/// How [`Dev::land`] fails: the game took the teleport and never moved,
/// as it does after some scenes a `Play` request started.
pub const NEVER_LANDED: &str = "the teleport never landed";

/// The words for a refusal, or `None` when the hook did it.
fn refusal(outcome: Outcome) -> Option<String> {
    Some(match outcome {
        Outcome::Done | Outcome::Unanswered => return None,
        Outcome::Soon => format!("another teleport in the last {GUARD} s"),
        Outcome::NoGame => "the game isn't up yet".into(),
        Outcome::Event => EVENT_PLAYING.into(),
        Outcome::Loading => "a map is loading".into(),
        Outcome::NotBuilt => "the request didn't take its arguments".into(),
        Outcome::NoAge => "the game has no age flag yet".into(),
        Outcome::NoClock => "there's no clock here".into(),
        Outcome::NoController => "Link's character controller isn't where the warp looks".into(),
        Outcome::Unknown => {
            "the installed seed's hook is older than this request: install a seed built since"
                .into()
        }
    })
}

/// The request's three names, each NUL padded to the mailbox's size.
fn names(texts: [&str; 3]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(3 * NAME);
    for (i, name) in texts.into_iter().enumerate() {
        ensure!(name.len() < NAME, "{name} is over {} bytes", NAME - 1);
        out.extend(name.bytes());
        out.resize((i + 1) * NAME, 0);
    }
    Ok(out)
}

/// The running game, with our hook in it.
pub struct Dev<M = Game> {
    game: M,
    global: u64,
    main: u64,
}

impl Dev<Game> {
    pub fn new(game: Game) -> Result<Self> {
        let global = game.global();
        Self::with(game, global)
    }
}

impl<M: Memory> Dev<M> {
    /// The game in `game`, whose GameData global is at `global`.
    pub fn with(game: M, global: u64) -> Result<Self> {
        // The GameData global sits at a fixed place in `main`.
        let main = global - u64::from(patch::OFFSETS.gamedata);
        let at = patch::OFFSETS.link_update;
        let want = patch::site(at).context("Link's update has no hook site")?;
        ensure!(
            game.read(main + u64::from(at), want.len())? == want,
            "the dev panel's hook isn't in the game. Reinstall the seed, then restart the game"
        );
        Ok(Self { game, global, main })
    }

    pub fn game(&self) -> &M {
        &self.game
    }

    pub fn global(&self) -> u64 {
        self.global
    }

    fn value(&self, name: &str) -> Result<Vec<u8>> {
        self.value_of(live::hash(name)?)
    }

    fn value_of(&self, hash: u32) -> Result<Vec<u8>> {
        let (_, at, len) = live::locate(&self.game, self.global, hash)?;
        self.game.read(at, len)
    }

    /// Link's yaw in radians: he faces `(sin, 0, cos)` of it.
    pub fn yaw(&self) -> Result<f32> {
        let bytes = self.value_of(LINK_YAW)?;
        ensure!(bytes.len() == 4, "Link's yaw is {} bytes", bytes.len());
        Ok(floats::<1>(&bytes)[0])
    }

    /// Whether Link wears the Iron Boots.
    pub fn boots(&self) -> Result<bool> {
        self.flag(BOOTS_ON)
    }

    /// Link's hearts now, in quarters.
    pub fn hearts(&self) -> Result<i32> {
        let bytes = self.value_of(HEARTS)?;
        let quarters = bytes.get(..4).context("Link's hearts are under 4 bytes")?;
        Ok(i32::from_le_bytes(quarters.try_into()?))
    }

    fn flag(&self, name: &str) -> Result<bool> {
        Ok(self.value(name)?[0] & 1 == 1)
    }

    /// Whether the title's menu has opened, so it takes buttons.
    pub fn title_open(&self) -> Result<bool> {
        self.flag(TITLE)
    }

    pub fn state(&self) -> Result<State> {
        let map = self.value(MAP)?;
        let map = map.split(|&b| b == 0).next().unwrap_or_default();
        Ok(State {
            map: String::from_utf8_lossy(map).into(),
            child: self.flag(IS_CHILD)?,
            loading: self.flag(LOADING)?,
            event: self.event()?,
            switches: self.switches()?,
        })
    }

    /// Loading or in an event, so not in play: [`Dev::state`]'s two flags
    /// without the rest, for a look made often.
    pub fn busy(&self) -> Result<bool> {
        Ok(self.flag(LOADING)? || self.event()?)
    }

    /// An event plays, or the event manager isn't up yet.
    fn event(&self) -> Result<bool> {
        let slot = self.main + u64::from(patch::OFFSETS.events);
        let events = self.game.u64(self.game.u64(slot)?)?;
        Ok(events == 0 || self.game.u64(events + PLAYING)? != 0)
    }

    /// The dev switches the hook holds.
    pub fn switches(&self) -> Result<Switches> {
        let word = self.game.u32(self.main + u64::from(SWITCHES))?;
        Ok(Switches::from_word(word))
    }

    /// Turn the dev switches on or off. The hook reads them on Link's
    /// next update.
    pub fn set_switches(&self, switches: Switches) -> Result<()> {
        let word = switches.word().to_le_bytes();
        self.game.write(self.main + u64::from(SWITCHES), &word)
    }

    /// Drop Link's hearts to zero on his next update, as a last hit would.
    pub fn kill(&self) -> Result<()> {
        let at = self.main + u64::from(SWITCHES);
        let word = self.game.u32(at)? | switches::KILL;
        self.game.write(at, &word.to_le_bytes())
    }

    /// Spring an ice trap on Link: he freezes once he's free to. How many
    /// were waiting before it.
    pub fn freeze(&self) -> Result<u32> {
        let at = self.main + u64::from(traps::TRAPS);
        let waiting = self.game.u32(at)?;
        self.game.write(at, &(waiting + 1).to_le_bytes())?;
        Ok(waiting)
    }

    /// Leave `request` for the hook and wait for its answer: what it did,
    /// or an error in the words of its refusal.
    pub fn ask(&self, request: &Request) -> Result<&'static str> {
        let mut outcome = self.send(request)?;
        if outcome == Outcome::Soon {
            // The hook keeps teleports `GUARD` s apart: wait it out once.
            std::thread::sleep(Duration::from_secs(GUARD));
            outcome = self.send(request)?;
        }
        if let Some(why) = refusal(outcome) {
            return Err(anyhow!(why));
        }
        Ok(match (request, outcome) {
            (_, Outcome::Unanswered) => UNANSWERED,
            (Request::Time(_), _) => "set",
            (Request::Step(..) | Request::Give(..) | Request::GiveLevel(..), _) => "ran",
            (Request::Warp(_), _) => "warped",
            (Request::Where, _) => "found",
            (Request::Notice(..), _) => "shown",
            _ => "sent",
        })
    }

    /// Hand `request` to the hook and wait for its answer.
    fn send(&self, request: &Request) -> Result<Outcome> {
        let mail = self.main + u64::from(MAILBOX);
        let field = |off: usize| mail + off as u64;
        let (ask, hour, texts) = match request {
            Request::Teleport(p) => (Ask::Teleport, 0, Some([&*p.map, &p.spot, ARRIVE])),
            Request::Age(p) => (Ask::Age, 0, Some([&*p.map, &p.spot, ARRIVE])),
            Request::Time(h) => {
                ensure!(*h < 24, "an hour is 0 to 23");
                (Ask::Time, *h, None)
            }
            Request::Step(challenge, step) => (Ask::Step, 0, Some([&**challenge, step, ""])),
            Request::Give(actor, equip) => (Ask::Give, u32::from(*equip), Some([&**actor, "", ""])),
            Request::GiveLevel(actor, swap) => {
                let (check, then) = swap.map_or(("", ""), |s| (s.check, ammo::WITHOUT.get()));
                (Ask::GiveLevel, 0, Some([&**actor, check, then]))
            }
            Request::Play(event) => (Ask::Play, 0, Some([&**event, "", ""])),
            Request::Warp(_) => (Ask::Warp, 0, None),
            Request::Where => (Ask::Where, 0, None),
            Request::Notice(actor, kind) => (Ask::Notice, *kind as u32, Some([&**actor, "", ""])),
        };
        if let Some(texts) = texts {
            self.game
                .write(field(offset_of!(Mailbox, map)), &names(texts)?)?;
        }
        if let Request::Warp(at) = request {
            let bytes = at.map(f32::to_le_bytes).concat();
            self.game.write(field(offset_of!(Mailbox, at)), &bytes)?;
        }
        let words = [ask as u32, hour].map(u32::to_le_bytes).concat();
        self.game.write(field(offset_of!(Mailbox, ask)), &words)?;
        let asked = self.game.u32(field(offset_of!(Mailbox, asked)))? + 1;
        self.game
            .write(field(offset_of!(Mailbox, asked)), &asked.to_le_bytes())?;
        let start = Instant::now();
        while self.game.u32(field(offset_of!(Mailbox, answered)))? != asked {
            if start.elapsed() > WAIT {
                // Should Link pick it up later, the hook refuses it.
                self.game.write(field(offset_of!(Mailbox, ask)), &[0; 4])?;
                bail!("Link didn't pick the request up. Is the game paused or in a menu?");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let word = self.game.u32(field(offset_of!(Mailbox, outcome)))?;
        Outcome::from_word(word).context("the hook gave an unknown answer")
    }

    /// Teleport to `place`, switching age first when it's the other age's.
    pub fn go(&self, place: &Place) -> Result<&'static str> {
        self.go_as(place, None)
    }

    /// Go to `place` as `child` when given, else as its map's age. A map
    /// that names no age (a dungeon) keeps Link's unless `child` says.
    /// A teleport the event manager answers 0 to leaves Link where he is,
    /// and an age switch's flag flipped (the hook keeps it), so the flag
    /// goes back and it fails as [`NOT_TAKEN`].
    pub fn go_as(&self, place: &Place, child: Option<bool>) -> Result<&'static str> {
        let was = self.flag(IS_CHILD)?;
        let request = match child.or(place.child()) {
            Some(child) if child != was => Request::Age(place.clone()),
            _ => Request::Teleport(place.clone()),
        };
        let done = self.ask(&request)?;
        if done == UNANSWERED {
            if matches!(request, Request::Age(_)) {
                live::set(
                    &self.game,
                    self.global,
                    live::hash(IS_CHILD)?,
                    &was.to_string(),
                )?;
            }
            bail!("{NOT_TAKEN} to {place}");
        }
        Ok(done)
    }

    /// Switch age, landing where [`other_age`] says.
    pub fn switch_age(&self, places: &BTreeSet<Place>) -> Result<(Place, &'static str)> {
        let State { map, child, .. } = self.state()?;
        let place = other_age(&map, child, places);
        let done = self.ask(&Request::Age(place.clone()))?;
        Ok((place, done))
    }

    /// Give Link `count` of an item, as `zootr give` does: the game's own
    /// grant once, then the count written over its stock.
    pub fn give(&self, gd: &GameData, actor: &str, count: i32) -> Result<()> {
        self.grant(plan_give(gd, actor, count)?, false).map(drop)
    }

    /// One of an item a seed deals, as a chest gives it: a progressive
    /// line's at the level Link needs next, ammo as a red rupee until Link
    /// owns what fires it. The deal vouches for the item, so unlike
    /// [`Dev::give`] it isn't checked against the pouch. Says what Link
    /// got: the level, or the swap's item.
    pub fn give_level(&self, actor: &str) -> Result<String> {
        let got = self.grant(grants(actor), true)?;
        Ok(got.unwrap_or_else(|| actor.to_owned()))
    }

    /// Run `jobs`, and say what the first game grant gave.
    fn grant(&self, jobs: Vec<Grant>, level: bool) -> Result<Option<String>> {
        let mut got = None;
        for job in jobs {
            match job {
                Grant::Game(actor, equip) => {
                    let leveled = level && !equip;
                    let request = match leveled {
                        true => Request::GiveLevel(actor.clone(), ammo::swap(&actor)),
                        false => Request::Give(actor.clone(), equip),
                    };
                    self.ask(&request)?;
                    let given = match leveled {
                        true => self.given()?,
                        false => None,
                    };
                    got.get_or_insert(given.unwrap_or(actor));
                    self.settle()?;
                }
                Grant::Stock(hash, count) => {
                    live::set(&self.game, self.global, hash, &count.to_string())?
                }
            }
        }
        Ok(got)
    }

    /// The item a level give gave, as the hook wrote it back, or `None`
    /// when it wrote nothing (a hook from before the write-back).
    fn given(&self) -> Result<Option<String>> {
        let at = self.main + u64::from(MAILBOX) + offset_of!(Mailbox, arrive) as u64;
        let raw = self.game.read(at, NAME)?;
        let name = raw.split(|&b| b == 0).next().unwrap_or_default();
        Ok((!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned()))
    }

    /// Show `shown`'s banner: write the GameData its words read, then ask
    /// the hook for it.
    pub fn notice(&self, shown: &crate::notice::Shown) -> Result<()> {
        for (name, value) in shown.gamedata() {
            live::set(&self.game, self.global, live::hash(name)?, &value)?;
        }
        self.ask(&Request::Notice(shown.item.clone(), shown.kind))
            .map(drop)
    }

    /// Wait for a teleport to land in `map` and its arrival event to end.
    /// The flags alone read quiet for a moment while the screen fades out,
    /// so the map has to be the new one too.
    pub fn land(&self, map: &str) -> Result<()> {
        self.land_holding(map, || Ok(()))
    }

    /// [`Dev::land`], running `hold` at every look until Link lands. A value
    /// the load resets, written again while it loads, is there when the
    /// map's actors spawn and read it.
    pub fn land_holding(&self, map: &str, mut hold: impl FnMut() -> Result<()>) -> Result<()> {
        std::thread::sleep(Duration::from_secs(1));
        let mut last = None;
        let busy = || {
            hold()?;
            let s = self.state()?;
            let busy = s.loading || s.event || s.map != map;
            last = Some(s);
            Ok(busy)
        };
        let landed = wait_while(busy, LAND, TICK);
        landed.with_context(|| match &last {
            Some(s) => format!("{NEVER_LANDED} in {map}: {}", busy_with(s, map)),
            None => format!("{NEVER_LANDED} in {map}"),
        })
    }

    /// Where Link stands.
    pub fn link(&self) -> Result<[f32; 3]> {
        self.ask(&Request::Where)?;
        let at = self.main + u64::from(MAILBOX) + offset_of!(Mailbox, link) as u64;
        Ok(floats(&self.game.read(at, 12)?))
    }

    /// Move Link to `at`, and again until he stays: right after some maps
    /// load, the game puts him back where he arrived.
    pub fn warp(&self, at: [f32; 3]) -> Result<&'static str> {
        let start = Instant::now();
        let mut before = None;
        loop {
            let tried = self.ask(&Request::Warp(at)).and_then(|done| {
                std::thread::sleep(STAY);
                Ok((done, self.link()?))
            });
            match tried {
                Ok((done, here)) if stayed(at, here, before) => return Ok(done),
                Ok((_, here)) if start.elapsed() < HOLD => before = Some(here),
                Err(_) if start.elapsed() < HOLD => std::thread::sleep(TICK),
                Ok((_, here)) => bail!("Link didn't stay at {at:?}, he stands at {here:?}"),
                Err(e) => return Err(e),
            }
        }
    }

    /// Wait for the event a quest step or a grant started to end, so the
    /// next request isn't refused for it.
    fn settle(&self) -> Result<()> {
        std::thread::sleep(TICK);
        wait_while(|| self.event(), SETTLE, TICK).context("the request's event never ended")
    }

    /// Run quest `challenge`'s `step` through the quest system, and wait
    /// for its event to end.
    pub fn step(&self, challenge: &str, step: &str) -> Result<()> {
        self.ask(&Request::Step(challenge.into(), step.into()))?;
        self.settle()
    }
}

/// The first `N` little endian floats of `bytes`, which holds at least
/// that many.
fn floats<const N: usize>(bytes: &[u8]) -> [f32; N] {
    core::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

/// What still holds a teleport to `map` off, by the last look.
fn busy_with(s: &State, map: &str) -> String {
    let mut why = Vec::new();
    if s.loading {
        why.push("loading".to_owned());
    }
    if s.event {
        why.push("an event playing".to_owned());
    }
    if s.map != map {
        why.push(format!("Link in {}", s.map));
    }
    why.join(", ")
}

/// Poll `busy` every `tick` until it's false, or fail after `limit`.
fn wait_while(
    mut busy: impl FnMut() -> Result<bool>,
    limit: Duration,
    tick: Duration,
) -> Result<()> {
    let start = Instant::now();
    while busy()? {
        ensure!(start.elapsed() <= limit, "still busy after {limit:?}");
        std::thread::sleep(tick);
    }
    Ok(())
}

/// Whether Link, warped to `at`, stands there: on the spot, or settled
/// within [`SETTLED`] of it where the last try left him too.
fn stayed(at: [f32; 3], here: [f32; 3], before: Option<[f32; 3]>) -> bool {
    let settled = before.is_some_and(|b| flat(b, here) <= HELD);
    flat(here, at) <= HELD || (settled && flat(here, at) <= SETTLED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{flow, item, map};

    #[test]
    fn switches_are_the_hooks_bits() {
        let all = Switches {
            health: true,
            magic: true,
            resources: true,
        };
        assert_eq!(all.word(), 7);
        assert_eq!(Switches::default().word(), 0);
        let magic = Switches {
            magic: true,
            ..Switches::default()
        };
        assert_eq!(magic.word(), switches::MAGIC);
        for word in 0..8 {
            assert_eq!(Switches::from_word(word).word(), word);
        }
    }

    #[test]
    fn names_fill_their_slots_and_refuse_overflow() {
        let bytes = names(["Map", "Spot", ARRIVE]).unwrap();
        assert_eq!(bytes.len(), 3 * NAME);
        assert_eq!(&bytes[..4], b"Map\0");
        assert_eq!(&bytes[NAME..NAME + 5], b"Spot\0");
        assert!(bytes[2 * NAME..].starts_with(ARRIVE.as_bytes()));
        let long = "M".repeat(NAME);
        assert!(names([&long, "Spot", ARRIVE]).is_err());
        let step = names([flow::GOTO_HYRULE_CASTLE, "Step02", ""]).unwrap();
        assert_eq!(&step[NAME..NAME + 7], b"Step02\0");
        assert!(step[2 * NAME..].iter().all(|&b| b == 0));
    }

    #[test]
    fn only_refusals_have_words() {
        assert_eq!(refusal(Outcome::Done), None);
        assert_eq!(refusal(Outcome::Unanswered), None);
    }

    #[test]
    fn a_teleport_that_never_lands_says_what_held_it() {
        let state = |map: &str, loading, event| State {
            map: map.into(),
            child: true,
            loading,
            event,
            switches: Switches::default(),
        };
        let home = state(map::KOKIRI_FOREST_CHILD, false, false);
        assert_eq!(
            busy_with(&home, map::GERUDO_FORTRESS_ADULT),
            format!("Link in {}", map::KOKIRI_FOREST_CHILD)
        );
        let there = state(map::GERUDO_FORTRESS_ADULT, true, true);
        assert_eq!(
            busy_with(&there, map::GERUDO_FORTRESS_ADULT),
            "loading, an event playing"
        );
        assert_eq!(refusal(Outcome::Event).unwrap(), "an event is playing");
        assert_eq!(
            refusal(Outcome::Soon).unwrap(),
            "another teleport in the last 8 s"
        );
    }

    #[test]
    fn a_give_request_names_its_actor_and_flags_the_equip() {
        let bytes = names([item::SWORD_KOKIRI, "", ""]).unwrap();
        assert!(bytes.starts_with(format!("{}\0", item::SWORD_KOKIRI).as_bytes()));
        assert!(bytes[NAME..].iter().all(|&b| b == 0));
    }

    #[test]
    fn a_warp_settled_a_step_off_its_spot_twice_stayed() {
        let at = [0.0, 0.0, 0.0];
        let off = [2.8, -0.5, 0.0];
        assert!(!stayed(at, off, None), "once could be a slide still going");
        assert!(stayed(at, off, Some([2.6, -0.4, 0.3])));
        assert!(!stayed(at, off, Some([1.0, 0.0, 0.0])), "still sliding");
        let arrival = [40.0, 0.0, 0.0];
        assert!(
            !stayed(at, arrival, Some(arrival)),
            "put back where he arrived"
        );
    }

    #[test]
    fn a_warp_holds_by_the_flat_distance() {
        assert_eq!(flat([0.0, 9.0, 0.0], [3.0, 0.0, 4.0]), 5.0);
        assert!(flat([1.0, 2.0, 3.0], [1.5, 0.5, 3.0]) <= HELD);
    }

    #[test]
    fn waiting_ends_when_idle_and_fails_when_it_never_is() {
        let mut left = 3;
        let idle = || {
            left -= 1;
            Ok(left > 0)
        };
        wait_while(idle, Duration::from_secs(1), Duration::ZERO).unwrap();
        assert_eq!(left, 0);
        let err = wait_while(|| Ok(true), Duration::ZERO, Duration::from_millis(1));
        assert!(err.is_err());
        let err = wait_while(|| Err(anyhow!("gone")), Duration::ZERO, Duration::ZERO);
        assert_eq!(err.unwrap_err().to_string(), "gone");
    }
}
