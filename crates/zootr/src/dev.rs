//! The dev panel's commands for the running game. A teleport, an age
//! switch, the time of day, a quest step, an item to give, an event to
//! start and a warp beside an actor are left in the hook's mailbox, which runs them on Link's next
//! update (see `zootr_hook::Mailbox`). The state and set are plain reads and writes
//! through [`live`].

use std::collections::BTreeSet;
use std::fmt;
use std::mem::offset_of;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};
use zootr_hook::{Ask, GUARD, MAILBOX, Mailbox, NAME, Outcome, SWITCHES, switches};

use crate::checks::flat;
use crate::config::TestSetup;
use crate::gamedata::{GameData, Info};
use crate::known::{Name, flag, scene};
use crate::live::{self, Game, Memory};
use crate::{inventory, patch, starting, steps, world};

/// The arrival event a teleport plays. Some areas may want their own.
const ARRIVE: &Name = scene::SY_BANC_CHANGE_ARRIVE;
pub(crate) const CHILD: &str = "_Child";
pub(crate) const ADULT: &str = "_Adult";
/// Link's yaw (GameData, unnamed).
const LINK_YAW: u32 = 0x1ea93bd8;
/// Link's live hearts, in quarters (`PlayerStatus.Life` is only the save's
/// copy).
const HEARTS: u32 = 0x31ab5580;
/// Where an age switch lands when the map has no version for the other
/// age: the Temple of Time, where Prelude of Light arrives.
const FALLBACK: (&str, &Name) = ("Market", flag::LIGHT_PRELUDE);
/// How long Link has to pick a request up.
const WAIT: Duration = Duration::from_secs(3);
/// GameData names the state reads.
const MAP: &Name = flag::SEQUENCE_CURRENT_BANC;
const IS_CHILD: &Name = flag::SEQUENCE_IS_CHILDHOOD;
// Not a word of the dump's name tables.
const LOADING: &str = "Sequence_IsLoadingBanc";
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
const TICK: Duration = Duration::from_millis(100);

/// A map and an arrival point in it, written `MAP/SPOT`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Place {
    pub map: String,
    pub spot: String,
}

impl Place {
    pub fn parse(text: &str) -> Result<Self> {
        let (map, spot) = text
            .split_once('/')
            .filter(|(m, s)| !m.is_empty() && !s.is_empty())
            .with_context(|| format!("{text} isn't MAP/SPOT"))?;
        Ok(Self {
            map: map.into(),
            spot: spot.into(),
        })
    }

    /// Whether the map is the child one, the adult one, or either.
    fn child(&self) -> Option<bool> {
        crate::warp::era(&self.map)
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.map, self.spot)
    }
}

/// Every place a scene exit arrives at, both ages.
pub fn places(romfs: &Romfs) -> Result<BTreeSet<Place>> {
    Ok(world::exits(romfs)?
        .into_iter()
        .filter(|e| !e.to_start.is_empty())
        .map(|e| Place {
            map: e.to,
            spot: e.to_start,
        })
        .collect())
}

/// Refuses `place` unless a scene exit arrives there. The game crashes on a
/// map it doesn't have, and an unknown spot leaves a black screen.
pub fn known(place: &Place, places: &BTreeSet<Place>) -> Result<()> {
    if places.contains(place) {
        return Ok(());
    }
    let close = (places.iter())
        .filter(|p| p.map.starts_with(&place.map) || place.map.starts_with(&p.map))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    match close.is_empty() {
        true => bail!("{place} isn't an arrival point. `zootr probe places` lists them"),
        false => bail!(
            "{place} isn't an arrival point. Close: {}",
            close.join(", ")
        ),
    }
}

/// Where switching age from `map` lands: the other age's version of the
/// map at its first arrival point, else the Temple of Time.
pub fn other_age(map: &str, child: bool, places: &BTreeSet<Place>) -> Place {
    let (from, to) = if child {
        (CHILD, ADULT)
    } else {
        (ADULT, CHILD)
    };
    let other = map.strip_suffix(from).map(|base| format!("{base}{to}"));
    other
        .and_then(|other| places.iter().find(|p| p.map == other).cloned())
        .unwrap_or_else(|| Place {
            map: format!("{}{to}", FALLBACK.0),
            spot: FALLBACK.1.to_string(),
        })
}

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
    /// Start an event by name, with Link as its starter.
    Play(String),
    /// Move Link to a point of the current map.
    Warp([f32; 3]),
    /// Where Link stands.
    Where,
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
            "the dev panel's hook isn't in the game. Turn on the dev panel in Settings, reinstall the seed, then restart the game"
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

    /// Link's hearts now, in quarters.
    pub fn hearts(&self) -> Result<i32> {
        let bytes = self.value_of(HEARTS)?;
        let quarters = bytes.get(..4).context("Link's hearts are under 4 bytes")?;
        Ok(i32::from_le_bytes(quarters.try_into()?))
    }

    fn flag(&self, name: &str) -> Result<bool> {
        Ok(self.value(name)?[0] & 1 == 1)
    }

    pub fn state(&self) -> Result<State> {
        let map = self.value(MAP)?;
        let map = map.split(|&b| b == 0).next().unwrap_or_default();
        let slot = self.main + u64::from(patch::OFFSETS.events);
        let events = self.game.u64(self.game.u64(slot)?)?;
        Ok(State {
            map: String::from_utf8_lossy(map).into(),
            child: self.flag(IS_CHILD)?,
            loading: self.flag(LOADING)?,
            event: events == 0 || self.game.u64(events + PLAYING)? != 0,
            switches: self.switches()?,
        })
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

    /// Leave `request` for the hook and wait for its answer: what it did,
    /// or an error in the words of its refusal.
    pub fn ask(&self, request: &Request) -> Result<&'static str> {
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
            Request::Play(event) => (Ask::Play, 0, Some([&**event, "", ""])),
            Request::Warp(_) => (Ask::Warp, 0, None),
            Request::Where => (Ask::Where, 0, None),
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
        let outcome = Outcome::from_word(word).context("the hook gave an unknown answer")?;
        if let Some(why) = refusal(outcome) {
            return Err(anyhow!(why));
        }
        Ok(match (request, outcome) {
            (_, Outcome::Unanswered) => UNANSWERED,
            (Request::Time(_), _) => "set",
            (Request::Step(..) | Request::Give(..), _) => "ran",
            (Request::Warp(_), _) => "warped",
            (Request::Where, _) => "found",
            _ => "sent",
        })
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
        for job in plan_give(gd, actor, count)? {
            match job {
                Grant::Game(actor, equip) => {
                    self.ask(&Request::Give(actor, equip))?;
                    self.settle()?;
                }
                Grant::Stock(hash, count) => {
                    live::set(&self.game, self.global, hash, &count.to_string())?
                }
            }
        }
        Ok(())
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
        loop {
            let tried = self.ask(&Request::Warp(at)).and_then(|done| {
                std::thread::sleep(STAY);
                Ok((done, self.link()?))
            });
            match tried {
                Ok((done, here)) if flat(here, at) <= HELD => return Ok(done),
                Ok(_) if start.elapsed() < HOLD => {}
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
        wait_while(|| Ok(self.state()?.event), SETTLE, TICK)
            .context("the request's event never ended")
    }

    /// Run quest `challenge`'s `step` through the quest system, and wait
    /// for its event to end.
    pub fn step(&self, challenge: &str, step: &str) -> Result<()> {
        self.ask(&Request::Step(challenge.into(), step.into()))?;
        self.settle()
    }

    /// Apply a saved test setup: its values, its items, its quest steps,
    /// its hour, then one teleport, so the new map reads the rest, landed
    /// before the warp to `near`, the spot beside its `near` actor. Says what it did, and which goals
    /// already held before it changed anything. A line that can't be run
    /// refuses the whole setup first.
    pub fn apply(
        &self,
        gd: &GameData,
        setup: &TestSetup,
        near: Option<[f32; 3]>,
    ) -> Result<Applied> {
        let jobs = plan(setup, near, |challenge, step| {
            steps::check(gd, challenge, step)
        })?;
        let held = (self.goals(gd, setup).into_iter())
            .filter(|g| g.met)
            .map(|g| g.name)
            .collect();
        let mut done = Vec::new();
        for job in jobs {
            match job {
                Job::Set { hash, value, line } => {
                    live::set(&self.game, self.global, hash, &value)?;
                    done.push(format!("set {line}"));
                }
                Job::Give(actor) => {
                    self.give(gd, &actor, 1)?;
                    done.push(format!("gave {actor}"));
                }
                Job::Step(challenge, step) => {
                    self.step(&challenge, &step)?;
                    done.push(format!("ran quest step {challenge}={step}"));
                }
                Job::Hour(hour) => {
                    self.ask(&Request::Time(hour))?;
                    done.push(format!("set the hour to {hour}"));
                }
                Job::Go(place) => {
                    let went = self.go(&place)?;
                    self.land(&place.map)?;
                    done.push(format!("{went} to {place}"));
                }
                Job::Near(actor, at) => {
                    let warped = self.warp(at)?;
                    done.push(format!("{warped} beside {actor}"));
                }
            }
        }
        Ok(Applied { done, held })
    }

    /// A test setup's goals as the game holds them now.
    pub fn goals(&self, gd: &GameData, setup: &TestSetup) -> Vec<Goal> {
        let read = |hash| live::read(&self.game, gd, self.global, hash);
        setup.goals.iter().map(|g| Goal::check(g, read)).collect()
    }
}

/// One thing giving an item does.
#[derive(Debug, PartialEq)]
enum Grant {
    /// The game's grant of an actor, equipped or not.
    Game(String, bool),
    /// The stock written over the count the grant left, for more than one.
    Stock(u32, i32),
}

/// What giving `count` of `actor` does, in order: the game's grant of it and
/// of what comes with it, then its stock if there's more than one. A grant
/// adds one, so the stock is set after it.
fn plan_give(gd: &GameData, actor: &str, count: i32) -> Result<Vec<Grant>> {
    ensure!(count >= 1, "a count is 1 or more");
    let stock = inventory::stock(gd, actor)?;
    // Swords and shields have neither, and the starting items are known to
    // grant.
    ensure!(
        stock.is_some()
            || inventory::has_flag(gd, actor)
            || starting::ITEMS.iter().any(|i| i.actor == actor),
        "{actor} isn't an item Link can be given. `zootr items` lists them"
    );
    let mut jobs: Vec<_> = (starting::granted(actor).into_iter())
        .map(|(actor, equip)| Grant::Game(actor.into(), equip))
        .collect();
    if count > 1 {
        let stock =
            stock.with_context(|| format!("{actor} has no count, only one can be given"))?;
        jobs.push(Grant::Stock(stock, count));
    }
    Ok(jobs)
}

/// One thing Apply does.
#[derive(Debug, PartialEq)]
enum Job {
    Set {
        hash: u32,
        value: String,
        /// The `NAME=VALUE` line, for the report.
        line: String,
    },
    Give(String),
    Step(String, String),
    Hour(u32),
    Go(Place),
    Near(String, [f32; 3]),
}

/// Everything Apply does for `setup`, in order: values, items, quest steps,
/// the hour, the teleport, then the warp. Each line is read here, and each quest step
/// asked of `known`, so a bad one refuses the setup before anything runs.
fn plan(
    setup: &TestSetup,
    near: Option<[f32; 3]>,
    known: impl Fn(&str, &str) -> Result<()>,
) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
    for line in &setup.defaults {
        let (name, value) = name_value(line)?;
        jobs.push(Job::Set {
            hash: live::hash(name)?,
            value: value.into(),
            line: line.clone(),
        });
    }
    jobs.extend(setup.give.iter().cloned().map(Job::Give));
    for line in &setup.progress {
        let (challenge, step) = steps::parse(line)?;
        known(&challenge, &step).with_context(|| format!("quest step {line}"))?;
        jobs.push(Job::Step(challenge, step));
    }
    if let Some(hour) = setup.hour {
        ensure!(hour < 24, "an hour is 0 to 23");
        jobs.push(Job::Hour(hour));
    }
    if let Some(spawn) = &setup.spawn {
        jobs.push(Job::Go(Place::parse(spawn)?));
    }
    if let Some(actor) = &setup.near {
        let at = near.with_context(|| format!("no spot beside {actor} to warp to"))?;
        jobs.push(Job::Near(actor.clone(), at));
    }
    Ok(jobs)
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

/// What applying a test setup did.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Applied {
    pub done: Vec<String>,
    /// Goals that held before Apply, which prove nothing.
    pub held: Vec<String>,
}

/// A GameData value as the dev panel's Set shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename = "DevValue")]
pub struct Value {
    pub info: Info,
    /// The game is up and its memory could be read.
    pub game: bool,
    /// What it holds now, as the Live tab shows it. `None` without the
    /// game, or for a value it doesn't keep.
    pub live: Option<String>,
}

/// What `name` is and what it holds in `game`, read from memory without
/// the hook.
pub fn value(gd: &GameData, name: &str, game: Option<&Game>) -> Result<Value> {
    let info = gd.info(name)?;
    let live = game.and_then(|g| g.get(gd, live::hash(name).ok()?).ok());
    Ok(Value {
        info,
        game: game.is_some(),
        live,
    })
}

/// One of a test setup's goals, read from the game.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Goal {
    pub name: String,
    pub want: String,
    /// The value it holds, as the Live tab shows it.
    pub now: Option<String>,
    pub met: bool,
    /// Why it couldn't be read. The other goals still are.
    pub error: Option<String>,
}

impl Goal {
    pub(crate) fn check(line: &str, read: impl FnOnce(u32) -> Result<String>) -> Self {
        let parsed = name_value(line);
        let (name, want) = parsed.as_ref().map_or((line.trim(), ""), |p| *p);
        let now = parsed.and_then(|(name, _)| read(live::hash(name)?));
        Goal {
            name: name.into(),
            want: want.into(),
            met: now.as_deref().is_ok_and(|now| now == want),
            error: now.as_ref().err().map(|e| format!("{e:#}")),
            now: now.ok(),
        }
    }
}

/// A `NAME=VALUE` line, as `defaults` and `goals` write them.
fn name_value(line: &str) -> Result<(&str, &str)> {
    let (name, value) =
        (line.split_once('=')).with_context(|| format!("{line} isn't NAME=VALUE"))?;
    Ok((name.trim(), value.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, flow, item, map};

    #[test]
    fn a_goal_holds_when_the_game_shows_its_value() {
        let shows = |text: &'static str| move |_| Ok(text.to_string());
        let met = Goal::check(&format!(" IsGet.{} = true", item::HOOKSHOT), shows("true"));
        assert_eq!(met.name, flag::IS_GET_ITEM_HOOKSHOT);
        assert_eq!(met.want, "true");
        assert!(met.met && met.error.is_none());
        assert!(!Goal::check("Step=Step05", shows("Step03")).met);
        let missing = Goal::check("Nope=1", |_| Err(anyhow!("no entry")));
        assert_eq!(
            (missing.now, missing.error),
            (None, Some("no entry".into()))
        );
        let bad = Goal::check("Nope", shows("true"));
        assert!(!bad.met && bad.error.unwrap().contains("NAME=VALUE"));
    }

    fn place(text: &str) -> Place {
        Place::parse(text).unwrap()
    }

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
    fn a_place_is_map_slash_spot() {
        let p = place(&format!(
            "{}/{}",
            flag::KAKARIKO_VILLAGE_CHILD,
            field::KAKARIKO_WELL_SIDE
        ));
        assert_eq!(
            p.to_string(),
            format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_CHILD,
                field::KAKARIKO_WELL_SIDE
            )
        );
        assert_eq!(p.child(), Some(true));
        assert_eq!(
            place(&format!("{}/Entrance", flag::WATER_TEMPLE)).child(),
            None
        );
        for bad in ["Kakariko", "/Spot", "Map/"] {
            assert!(Place::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_teleport_goes_only_where_an_exit_arrives() {
        let places = BTreeSet::from([
            place(&format!(
                "{}/{}",
                map::KOKIRI_FOREST_CHILD,
                field::FRONT_DEKU_TREE
            )),
            place(&format!(
                "{}/{}",
                map::KOKIRI_FOREST_CHILD,
                field::KOKIRI_FOREST_FRONT_OF_LOST_WOOD
            )),
            place(&format!("{}/Entrance", flag::WATER_TEMPLE)),
        ]);
        assert!(known(&place(&format!("{}/Entrance", flag::WATER_TEMPLE)), &places).is_ok());
        let err = known(
            &place(&format!(
                "{}/{}",
                flag::KOKIRI_FOREST,
                field::FRONT_DEKU_TREE
            )),
            &places,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "{}/{} isn't an arrival point. Close: \
             {}/{}, {}/{}",
                flag::KOKIRI_FOREST,
                field::FRONT_DEKU_TREE,
                map::KOKIRI_FOREST_CHILD,
                field::FRONT_DEKU_TREE,
                map::KOKIRI_FOREST_CHILD,
                field::KOKIRI_FOREST_FRONT_OF_LOST_WOOD
            )
        );
        let err = known(&place("Nowhere/Spot"), &places).unwrap_err();
        assert!(err.to_string().contains("probe places"), "{err}");
    }

    #[test]
    fn switching_age_keeps_the_map_or_goes_to_the_temple() {
        let places = BTreeSet::from([
            place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_WELL_SIDE
            )),
            place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_VILLAGE_ENTRANCE
            )),
            place(&format!("{}/Entrance", flag::WATER_TEMPLE)),
        ]);
        assert_eq!(
            other_age(flag::KAKARIKO_VILLAGE_CHILD, true, &places),
            place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_VILLAGE_ENTRANCE
            ))
        );
        assert_eq!(
            other_age(flag::KAKARIKO_VILLAGE_ADULT, false, &places),
            place(&format!("{}/{}", flag::MARKET_CHILD, flag::LIGHT_PRELUDE))
        );
        assert_eq!(
            other_age(flag::WATER_TEMPLE, false, &places),
            place(&format!("{}/{}", flag::MARKET_CHILD, flag::LIGHT_PRELUDE))
        );
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

    fn gd() -> Option<GameData> {
        let romfs = crate::game::dev_dump()?;
        Some(GameData::load(&romfs).unwrap())
    }

    fn game(actor: &str, equip: bool) -> Grant {
        Grant::Game(actor.into(), equip)
    }

    #[test]
    fn giving_one_is_the_games_grant_and_giving_more_tops_up_the_stock() {
        let Some(gd) = gd() else { return };
        let stock = inventory::stock(&gd, item::BOOMERANG).unwrap().unwrap();
        assert_eq!(
            plan_give(&gd, item::BOOMERANG, 1).unwrap(),
            [game(item::BOOMERANG, false)]
        );
        assert_eq!(
            plan_give(&gd, item::BOOMERANG, 5).unwrap(),
            [game(item::BOOMERANG, false), Grant::Stock(stock, 5)]
        );
        assert!(plan_give(&gd, item::BOOMERANG, 0).is_err());
    }

    #[test]
    fn a_song_is_granted_rather_than_refused() {
        let Some(gd) = gd() else { return };
        assert_eq!(
            plan_give(&gd, item::MELODY_SUN_SONG, 1).unwrap(),
            [game(item::MELODY_SUN_SONG, false)]
        );
        let err = plan_give(&gd, item::MELODY_SUN_SONG, 2).unwrap_err();
        assert!(err.to_string().contains("no count"), "{err}");
        let err = plan_give(&gd, "Item_Nothing", 1).unwrap_err();
        assert!(err.to_string().contains("isn't an item"), "{err}");
    }

    #[test]
    fn gear_is_equipped_and_a_companion_comes_with_its_item() {
        let Some(gd) = gd() else { return };
        assert_eq!(
            plan_give(&gd, item::SWORD_KOKIRI, 1).unwrap(),
            [game(item::SWORD_KOKIRI, true)]
        );
        assert_eq!(
            plan_give(&gd, item::RANGE_SLING_SHOT, 1).unwrap(),
            [
                game(item::RANGE_SLING_SHOT, false),
                game(item::BULLET_BAG_SMALL, false)
            ]
        );
    }

    #[test]
    fn a_give_request_names_its_actor_and_flags_the_equip() {
        let bytes = names([item::SWORD_KOKIRI, "", ""]).unwrap();
        assert!(bytes.starts_with(b"Item_Sword_Kokiri\0"));
        assert!(bytes[NAME..].iter().all(|&b| b == 0));
    }

    fn full() -> TestSetup {
        TestSetup {
            spawn: Some(format!("{}/Outside", map::GERUDO_FORTRESS_ADULT)),
            give: vec![item::TOKEN_GERUDO.to_string()],
            defaults: vec!["A=1".into(), "B=true".into()],
            progress: vec![
                format!("{}=Step02", flow::SAGE_OF_SPIRIT),
                format!("{}=Step02", flow::GOTO_HYRULE_CASTLE),
            ],
            hour: Some(21),
            near: Some("Npc_Gerudo_Guard".into()),
            ..Default::default()
        }
    }

    const SPOT: [f32; 3] = [1.0, 2.0, 3.0];

    #[test]
    fn apply_runs_values_items_steps_the_hour_then_the_teleport() {
        let jobs = plan(&full(), Some(SPOT), |_, _| Ok(())).unwrap();
        let kinds: Vec<_> = jobs
            .iter()
            .map(|j| match j {
                Job::Set { .. } => "set",
                Job::Give(_) => "give",
                Job::Step(..) => "step",
                Job::Hour(_) => "hour",
                Job::Go(_) => "go",
                Job::Near(..) => "near",
            })
            .collect();
        assert_eq!(
            kinds,
            ["set", "set", "give", "step", "step", "hour", "go", "near"]
        );
        assert_eq!(
            jobs[3..5],
            [
                Job::Step(flow::SAGE_OF_SPIRIT.to_string(), "Step02".into()),
                Job::Step(flow::GOTO_HYRULE_CASTLE.to_string(), "Step02".into())
            ]
        );
        assert_eq!(jobs[7], Job::Near("Npc_Gerudo_Guard".into(), SPOT));
        assert!(
            plan(&TestSetup::default(), None, |_, _| Ok(()))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_bad_line_refuses_the_setup_before_anything_runs() {
        let bad = |setup: TestSetup| {
            let err = plan(&setup, Some(SPOT), |_, _| Ok(())).unwrap_err();
            err.to_string()
        };
        let progress = |line: &str| TestSetup {
            progress: vec![line.into()],
            ..full()
        };
        assert!(bad(progress(flow::SAGE_OF_SPIRIT)).contains("CHALLENGE=STEP"));
        let unknown = plan(&full(), Some(SPOT), |c, _| {
            ensure!(c != flow::GOTO_HYRULE_CASTLE, "no such quest");
            Ok(())
        });
        let err = format!("{:#}", unknown.unwrap_err());
        assert!(
            err.contains(&format!("{}=Step02", flow::GOTO_HYRULE_CASTLE))
                && err.contains("no such quest"),
            "{err}"
        );
        let late = TestSetup {
            defaults: vec!["no equals".into()],
            ..full()
        };
        assert!(bad(late).contains("NAME=VALUE"));
        let hour = TestSetup {
            hour: Some(24),
            ..full()
        };
        assert!(bad(hour).contains("0 to 23"));
        let lost = plan(&full(), None, |_, _| Ok(())).unwrap_err().to_string();
        assert!(lost.contains("no spot beside Npc_Gerudo_Guard"), "{lost}");
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
