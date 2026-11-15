//! Sweeping one spot: set it up, trigger it, see it through (with the
//! judge when it doesn't resolve by itself), and give a verdict.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ounce_formats::{BymlDoc, Romfs};
use serde::Serialize;

use super::judge::{Answer, Judge, Look, TURNS};
use super::melody;
use super::plan::{Halt, Planned, Trigger, preset};
use crate::dev::{Dev, EVENT_PLAYING, NOT_TAKEN, Place, Request};
use crate::gamedata::GameData;
use crate::gerudo;
use crate::inventory;
use crate::known::{Name, field, flag};
use crate::live;
use crate::nav::act::Used;
use crate::nav::drive::Drive;
use crate::nav::{self, Level};
use crate::play::Input;
use crate::progressive;
use crate::savefile::Change;
use crate::shops;
use crate::standins;
use crate::steps::SAVED;
use crate::warp::Placement;

/// How long the harness presses through an event by itself before the
/// judge is asked.
const ALONE: Duration = Duration::from_secs(12);
/// Link's rupees, and what a spot that sells fills them to: past any
/// wallet's size, which the game lets stand, so the dearest offering, 500,
/// is met without a wallet upgrade.
const RUPEES: (&Name, &str) = (flag::PLAYER_STATUS_CURRENT_RUPEE, "999");
/// How long an event may run on after the judge's turns before it counts
/// as a stall.
const FINISH: Duration = Duration::from_secs(90);
/// How long a spot with no event may go before it counts as not started.
const IDLE: Duration = Duration::from_secs(3);
/// How long the game gets to calm down before a spot starts.
const CALM: Duration = Duration::from_secs(15);
const BEAT: Duration = Duration::from_millis(1000);
/// Between a song's notes: a lesson takes them at this pace.
const NOTE: Duration = Duration::from_millis(600);
/// How long an event takes to bring the ocarina out before it takes notes.
const OCARINA: Duration = Duration::from_millis(3000);
/// How long an event its sensor starts gets to begin once Link steps in.
const STEP_IN: Duration = Duration::from_secs(5);
/// How many beats of A `finish` presses between songs.
const ENCORE: usize = 5;
/// Legs of each length in the square spiral [`search`] walks.
const SPIRAL: usize = 8;
/// How long each step pushes the stick: about 0.75 m on the Mac. A 100 ms
/// push moves Link 0.15 m, most of it spent getting going.
const STEP_MS: u64 = 250;
/// How long a rolling giver may take to pass his stopping spot: a lap of
/// the rolling Goron's track is about 19 s (Mac, 2026-11-15).
const LAP: Duration = Duration::from_secs(45);
/// How often his passing is read: he crosses the spot in well under a
/// second.
const PASSING: Duration = Duration::from_millis(50);
/// The shop cursor: the shelf spot browsing points at, -1 before it.
const CURSOR: &Name = flag::SHOP_SELECT_ITEM_INDEX;
/// What the shelf spot under the cursor sells, by actor name.
const GOODS: &Name = flag::SHOP_SELECT_ITEM_NAME;
/// The stick pushes that move the shop cursor.
const MOVES: [&str; 4] = ["LRight", "LDown", "LLeft", "LUp"];
/// The price of the goods under the shop cursor, a GameData Int with no
/// name, set as the cursor moves onto them.
const PRICE: u32 = 0xf0363f80;
/// A push that moves the cursor one spot, and the wait for it to land.
const NUDGE_MS: u64 = 120;
const NUDGE: Duration = Duration::from_millis(500);
/// Beats of A on the goods, through the price line and the get window,
/// before the buy has had its chance. The Goron keeper says two more lines
/// than the Kokiri one, a warning and a haggle, and 8 ran out before his
/// get window (Mac, 2026-11-14). The loop ends as the get lands, so spare beats cost nothing.
const BUY: usize = 16;
/// How a spot went.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "verdict", content = "why", rename_all = "snake_case")]
pub enum Verdict {
    /// The dealt item landed and the event ended.
    Pass,
    /// Another item landed: its value's name.
    WrongItem(String),
    /// The game gave its stand-in for an item Link can't use yet, as it
    /// does in an unrandomized game: what it gave.
    Substituted(String),
    /// The event ended and no item landed.
    Nothing,
    /// An event played and nothing came of it, after the judge's turns.
    Stall,
    /// The game quit or stopped answering.
    Crash(String),
    /// The judge named one.
    Bug(String),
    /// The harness couldn't set the spot up. Its fault, not the game's.
    SetupFailed(String),
    /// Out of scope, or no trigger known.
    NotSwept(String),
}

impl Verdict {
    /// Whether the game needs a restart before the next spot.
    pub fn broke(&self) -> bool {
        matches!(self, Verdict::Crash(_) | Verdict::Stall | Verdict::Bug(_))
    }
}

/// A spot's verdict and its evidence.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Swept {
    pub verdict: Option<Verdict>,
    /// The judge's answers, in order.
    pub judge: Vec<String>,
    pub shots: Vec<PathBuf>,
    /// What changed in GameData, one line each.
    pub changes: Vec<String>,
    /// How the walk to the holder and its use went, for a spot opened or
    /// talked to.
    pub used: Option<Used>,
}

/// What a spot is swept with.
pub struct Sweeper<'a, I, J> {
    pub dev: &'a Dev,
    pub romfs: &'a Romfs,
    /// The game dump, for each map's navmesh.
    pub game: &'a Path,
    pub gd: &'a GameData,
    pub input: &'a mut I,
    pub judge: &'a mut J,
    pub places: &'a BTreeSet<Place>,
    /// Where this spot's shots go.
    pub dir: &'a Path,
}

/// The item's IsGet flags and stock, and the get-item message, to tell
/// when it lands.
struct Watch {
    /// The names `item` lands as, [`owned_as`].
    items: Vec<String>,
    /// The `IsGet` flags, taken away before the spot so they can change.
    flags: Vec<u32>,
    stock: Option<u32>,
    /// [`MESSAGE`], emptied before the spot.
    message: u32,
}

/// The item a get-item window last named. A chest's rupee leaves its
/// `IsGet` alone once Link has had one, so this is the sure sign.
const MESSAGE: &Name = flag::UI_MESSAGE_TARGET_ACTOR_NAME;

/// Whether `named` is the game's own stand-in for `item`: arrows come as a
/// rupee until Link has a bow, and a new file has none.
fn swapped(item: &str, named: &str) -> bool {
    item.contains("ArrowBundle") && named.contains("Rupee")
}

/// The item `changes` say a get-item window named.
fn named(changes: &[Change]) -> Option<String> {
    let c = changes.iter().find(|c| c.name == MESSAGE)?;
    let to = c.change.rsplit(" -> ").next()?.trim_matches('"');
    (!to.is_empty()).then(|| to.to_owned())
}

/// A rupee lying on the floor, which Link can pick up beside any spot. A
/// chest's rupee names its `TBox_` drop in the get window instead.
const FLOOR_RUPEE: &str = "Obj_Rupee_";

/// The first get outside `wanted` that `changes` show: an `IsGet` flag
/// turned on, else the item a get window named. A flag turned off is a load
/// or a reset, and a floor rupee a pickup beside the spot, never its get.
fn got_other(changes: &[Change], wanted: &[String]) -> Option<String> {
    let flag = changes.iter().find(|c| {
        c.name
            .strip_prefix("IsGet.")
            .is_some_and(|item| !item.starts_with(FLOOR_RUPEE))
            && !wanted.contains(&c.name)
            && c.change.ends_with("-> true")
    });
    let named = named(changes).filter(|n| !n.starts_with(FLOOR_RUPEE));
    (flag.map(|c| c.name.clone())).or(named.map(|n| format!("{MESSAGE} {n}")))
}

/// The items whose `IsGet` tells `item` landed: itself, a chest item's
/// field twin, which its get sets instead (`TBox_Rupee_Red` sets
/// `IsGet.Obj_Rupee_Red`), and every level of its upgrade line, as the
/// hook gives the next level Link lacks (`Item_BombBag_Small` lands as
/// `Item_BombBag_Medium` once he has a bag).
fn twins(item: &str) -> Vec<String> {
    let twin = item.strip_prefix("TBox_").map(|rest| format!("Obj_{rest}"));
    let names = std::iter::once(item.to_owned()).chain(twin);
    let names = names.chain(progressive::levels(item).into_iter().map(str::to_owned));
    let mut out = Vec::new();
    for name in names {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// The pouch row fields naming the gear a pickup is owned as.
const GEAR: [&Name; 2] = [field::ADULT_EQUIPMENT, field::CHILD_EQUIPMENT];

/// [`twins`], and the gear its pouch row says `item` is owned as:
/// `Obj_Armor_Goron` sets `IsGet.Armor_Adult_Goron`.
fn owned_as(romfs: &Romfs, item: &str) -> Result<Vec<String>> {
    let mut out = twins(item);
    let table = standins::POUCH_ACTOR_INFO;
    let doc = BymlDoc::parse(&romfs.read(table)?).with_context(|| table.to_owned())?;
    let Ok(row) = standins::row(&doc, item) else {
        return Ok(out);
    };
    for path in GEAR
        .iter()
        .filter_map(|k| row.get(k.get())?.as_string().ok())
    {
        let file = path.rsplit('/').next().unwrap_or(path.as_str());
        let actor = file.split('.').next().unwrap_or(file).to_owned();
        if !out.contains(&actor) {
            out.push(actor);
        }
    }
    Ok(out)
}

impl Watch {
    fn new(gd: &GameData, romfs: &Romfs, item: &str) -> Result<Self> {
        let items = owned_as(romfs, item)?;
        let flags = (items.iter())
            .filter(|t| inventory::has_flag(gd, t))
            .map(|t| live::hash(&format!("IsGet.{t}")))
            .collect::<Result<_>>()?;
        let stock = inventory::stock(gd, item)?;
        let message = live::hash(MESSAGE)?;
        Ok(Self {
            items,
            flags,
            stock,
            message,
        })
    }

    fn hashes(&self) -> impl Iterator<Item = u32> + '_ {
        let message = std::iter::once(self.message);
        self.flags.iter().copied().chain(self.stock).chain(message)
    }

    fn names(&self, gd: &GameData) -> Vec<String> {
        self.hashes().map(|h| gd.name(h)).collect()
    }

    fn read(&self, dev: &Dev, gd: &GameData) -> Result<Vec<String>> {
        let game = dev.game();
        (self.hashes())
            .map(|h| live::read(game, gd, dev.global(), h))
            .collect()
    }
}

/// How many times a request refused for an event is asked again.
const AGAIN: usize = 3;

/// The GameData values a spot writes before its teleport and holds through
/// the load: its trigger's ready values, then each quest step it needs, and
/// the guards stood down where they watch, as the step's save copy
/// `Step_<challenge>`. Gates read that copy (a keeper's presence, the
/// guards' `IsAfter` expressions). The copy is one write held through the
/// load, where a step run through the quest system is an event of its own.
fn presets(p: &Planned) -> Vec<String> {
    let (ready, quests) = match &p.trigger {
        Trigger::Open { ready, steps, .. }
        | Trigger::Talk { ready, steps, .. }
        | Trigger::Play { ready, steps, .. } => (ready, steps),
        Trigger::Skip(_) => return vec![],
    };
    let guards = gerudo::guard_step(&p.spot.area)
        .filter(|(quest, _)| !quests.iter().any(|(named, _)| named == quest));
    let saved = (quests.iter().chain(&guards)).map(|(c, s)| format!("{SAVED}{c}={s}"));
    ready.iter().cloned().chain(saved).collect()
}

/// A halted giver as the placement Link talks to: his own, standing at
/// `at`, where his gift puts him, rather than where his track starts.
fn stopped_at(level: &Level, halt: &Halt, at: [f32; 3]) -> Result<Placement> {
    let mut him = (level.placed.iter())
        .find(|p| p.name == halt.actor)
        .with_context(|| format!("{} isn't placed in {}", halt.actor, level.map))?
        .clone();
    him.pose.at = at;
    Ok(him)
}

/// `ask`, and again after `calm` while it's refused for an event: Navi or
/// an enemy's scene can start between one request and the next.
fn calmly<T>(
    mut ask: impl FnMut() -> Result<T>,
    mut calm: impl FnMut() -> Result<()>,
) -> Result<T> {
    for _ in 0..AGAIN {
        match ask() {
            Err(e) if format!("{e:#}").contains(EVENT_PLAYING) => calm()?,
            done => return done,
        }
    }
    ask()
}

/// Whether `e` means the game is gone rather than a request refused.
pub fn gone(e: &anyhow::Error) -> bool {
    let text = format!("{e:#}");
    [
        "didn't answer",
        "closed the bridge",
        "no game is listening",
        "Broken pipe",
    ]
    .iter()
    .any(|t| text.contains(t))
}

impl<I: Input, J: Judge> Sweeper<'_, I, J> {
    /// Sweep `p`, named `name` in the report.
    pub fn sweep(&mut self, p: &Planned, name: &str) -> Swept {
        let mut swept = Swept::default();
        let verdict = match &p.trigger {
            Trigger::Skip(why) => Verdict::NotSwept(why.clone()),
            _ => self
                .run(p, name, &mut swept)
                .unwrap_or_else(|e| match gone(&e) {
                    true => Verdict::Crash(format!("{e:#}")),
                    false => Verdict::SetupFailed(format!("{e:#}")),
                }),
        };
        if !matches!(
            verdict,
            Verdict::Pass | Verdict::NotSwept(_) | Verdict::Crash(_)
        ) {
            self.shot(p, name, "end", &mut swept);
        }
        swept.verdict = Some(verdict);
        swept
    }

    fn run(&mut self, p: &Planned, name: &str, swept: &mut Swept) -> Result<Verdict> {
        let watch = Watch::new(self.gd, self.romfs, &p.spot.item)?;
        self.calm()?;
        if let Trigger::Open { steps, .. } = &p.trigger {
            for (challenge, step) in steps {
                self.calmly(|dev| dev.step(challenge, step))?;
            }
        }
        // Before the teleport, and held through its load, which clears
        // them. Once more after, naming any the load still cleared.
        self.ready(p)?;
        self.setup(p)?;
        let cleared = self.ready(p)?;
        if let Trigger::Open {
            first: Some(event), ..
        } = &p.trigger
        {
            self.play(event)?;
            self.calm()?;
        }
        if pays(p) {
            live::set(
                self.dev.game(),
                self.dev.global(),
                live::hash(RUPEES.0)?,
                RUPEES.1,
            )?;
        }
        // Link may own it from an earlier spot or round: take it away.
        for &flag in &watch.flags {
            live::set(self.dev.game(), self.dev.global(), flag, "false")?;
        }
        live::set(self.dev.game(), self.dev.global(), watch.message, "")?;
        let was = live::snapshot(self.dev.game(), self.dev.global())?;
        let before = watch.read(self.dev, self.gd)?;
        let landed = |s: &Self| Ok::<_, anyhow::Error>(watch.read(s.dev, s.gd)? != before);
        self.trigger(p, name, swept)?;
        if matches!(p.trigger, Trigger::Open { broken: true, .. }) {
            self.fetch(&landed)?;
        }
        let mut verdict = match shops::is_holder(&p.spot.holder) {
            true => self.buy(&p.spot.item, &landed, swept)?,
            false => self.alone(&landed)?,
        };
        if verdict.is_none() {
            verdict = self.judged(p, name, &landed, swept)?;
        }
        let now = live::snapshot(self.dev.game(), self.dev.global())?;
        let changes = live::changes(self.gd, &was, &now, |_| true);
        let cleared = cleared
            .iter()
            .map(|f| format!("{f} cleared by the map load"));
        swept.changes = cleared
            .chain(changes.iter().map(|c| format!("{} {}", c.name, c.change)))
            .collect();
        let (wanted, items) = (watch.names(self.gd), &watch.items);
        let named = named(&changes);
        let swap = named.clone().filter(|n| swapped(&p.spot.item, n));
        let ours = changes
            .iter()
            .any(|c| c.name != MESSAGE && wanted.contains(&c.name))
            || named.as_ref().is_some_and(|n| items.contains(n));
        let other = got_other(&changes, &wanted);
        Ok(match (verdict, ours, other) {
            (Some(v @ (Verdict::Bug(_) | Verdict::Stall)), _, _) => v,
            (_, true, _) => Verdict::Pass,
            _ if swap.is_some() => Verdict::Substituted(swap.unwrap_or_default()),
            (_, false, Some(c)) => Verdict::WrongItem(c),
            _ => Verdict::Nothing,
        })
    }

    /// [`calmly`] with this spot's game.
    fn calmly<T>(&mut self, ask: impl Fn(&Dev) -> Result<T>) -> Result<T> {
        let dev = self.dev;
        calmly(|| ask(dev), || self.calm())
    }

    /// Wait for no event and no load, pressing A through any text left.
    fn calm(&mut self) -> Result<()> {
        let start = Instant::now();
        loop {
            let s = self.dev.state()?;
            if !s.event && !s.loading {
                return Ok(());
            }
            if start.elapsed() > CALM {
                bail!("an event from before the spot never ended");
            }
            if s.event {
                self.input.press("A")?;
            }
            sleep(BEAT);
        }
    }

    /// Sets the spot's [`presets`], and names those that weren't set.
    /// A map load clears a scrub's seller flag, and the scrub reads it once,
    /// as it spawns: the Shadow Temple scrub set after the load still shot
    /// Link, and one set all through the load sold (Mac, 2026-11-13).
    fn ready(&self, p: &Planned) -> Result<Vec<String>> {
        let (game, global) = (self.dev.game(), self.dev.global());
        let mut cleared = vec![];
        for entry in presets(p) {
            let (name, value) = preset(&entry);
            let hash = live::hash(name)?;
            if live::read(game, self.gd, global, hash)? != value {
                cleared.push(entry.clone());
            }
            live::set(game, global, hash, value)?;
        }
        Ok(cleared)
    }

    /// The song a cow wants or the item a giver takes, then the map. Quest
    /// steps are [`presets`].
    fn setup(&mut self, p: &Planned) -> Result<()> {
        let map = match &p.trigger {
            Trigger::Open { map, .. } | Trigger::Talk { map, .. } => Some(map),
            Trigger::Play { map, .. } => map.as_ref(),
            Trigger::Skip(_) => return Ok(()),
        };
        // The ocarina plays only the songs in Link's songbook: a cow didn't
        // know Epona's Song until the game granted it (Mac, 2026-11-14). A
        // given item is the game's own grant for the same reason.
        let give = match &p.trigger {
            Trigger::Play { melody, .. } => melody,
            Trigger::Talk { give, .. } => give,
            _ => &None,
        };
        if let Some(item) = give {
            let gd = self.gd;
            self.calmly(|dev| dev.give(gd, item, 1))?;
        }
        let Some(map) = map else {
            return Ok(());
        };
        let here = self.dev.state()?;
        if here.map == *map && p.child.is_none_or(|c| c == here.child) {
            return Ok(());
        }
        let place = (self.places.iter())
            .find(|pl| pl.map == *map)
            .with_context(|| format!("no arrival point in {map}"))?;
        self.go(place, p.child)?;
        let dev = self.dev;
        dev.land_holding(map, || self.ready(p).map(drop))
    }

    /// Where a shot of the try `name` tagged `tag` goes: `NAME-TAG.png`
    /// beside the spot's other shots, so a second try keeps the first's.
    /// Spots can share a name (a field's pots are all "Blue Rupee"), so the
    /// placement's hash keeps each one's frames apart.
    fn frame(&self, p: &Planned, name: &str, tag: impl std::fmt::Display) -> PathBuf {
        self.dir
            .join(format!("{}-{}-{tag}.png", slug(name), p.spot.hash))
    }

    /// Save a shot of the game as [`Self::frame`], when the game gives one.
    fn shot(&mut self, p: &Planned, name: &str, tag: &str, swept: &mut Swept) {
        let shot = self.frame(p, name, tag);
        if self.input.shot(&shot).is_ok() {
            swept.shots.push(shot);
        }
    }

    /// Open, talk or play: whatever starts the spot's event. A holder is
    /// used as `zootr dev use` does, from a warp near it.
    fn trigger(&mut self, p: &Planned, name: &str, swept: &mut Swept) -> Result<()> {
        let (at, face, here, halt) = match &p.trigger {
            Trigger::Open { at, face, .. } => (*at, *face, false, None),
            Trigger::Talk {
                at,
                face,
                counter,
                halt,
                ..
            } => (*at, *face, *counter, *halt),
            Trigger::Play {
                event, melody, at, ..
            } => {
                if !self.step_in(*at)? {
                    self.play(event)?;
                }
                if let Some(song) = melody {
                    sleep(OCARINA);
                    self.perform(song)?;
                }
                return Ok(());
            }
            Trigger::Skip(_) => return Ok(()),
        };
        if let Some(halt) = &halt {
            self.halt(halt)?;
        }
        let level = Level::read(self.dev, self.romfs, self.game)?;
        let graph = level.graph();
        let stopped: Placement;
        let holder = match &halt {
            Some(halt) => {
                stopped = stopped_at(&level, halt, face)?;
                &stopped
            }
            None => level.standing_at(face)?,
        };
        let at = match here {
            true => at,
            false => nav::landing(&graph, holder, at, p.side),
        };
        self.calmly(|dev| dev.warp(at))?;
        let shot = self.frame(p, name, "use");
        let mut drive = Drive::new(self.dev, &mut *self.input);
        drive.side = p.side;
        drive.here = here;
        let used = drive.touch(&graph, holder, shot)?;
        swept.shots.extend(used.shot.clone());
        swept.used = Some(used);
        if let Trigger::Talk {
            then: Some(button), ..
        } = &p.trigger
        {
            self.after_talk(button)?;
        }
        Ok(())
    }

    /// Stop a giver who rolls a track: wait for him to pass the one spot
    /// he can be stopped at, then set what a bomb hit there sets.
    fn halt(&self, halt: &Halt) -> Result<()> {
        let (game, global) = (self.dev.game(), self.dev.global());
        let passing = live::hash(halt.passing)?;
        let until = Instant::now() + LAP;
        while live::read(game, self.gd, global, passing)? != "true" {
            if Instant::now() > until {
                bail!("{} didn't pass his stopping spot in {LAP:?}", halt.actor);
            }
            sleep(PASSING);
        }
        live::set(game, global, live::hash(halt.hit)?, "true")
    }

    /// Talk, A through it, then press `button`: what the giver waits for
    /// once the talk ends. `use` can stop short of a giver up on a step,
    /// "A Talk" showing, so A opens the talk when it hasn't started.
    fn after_talk(&mut self, button: &str) -> Result<()> {
        if !self.dev.state()?.event {
            self.input.press("A")?;
            sleep(BEAT);
        }
        self.finish("A", None)?;
        self.input.press(button)
    }

    /// [`search`] for what a broken pot dropped until that lands or a get
    /// event starts. `use` broke it with the sword and stepped onto its
    /// spot, where the drop falls, but one can bounce off (a rock's in the
    /// Colossus).
    fn fetch(&mut self, landed: &impl Fn(&Self) -> Result<bool>) -> Result<()> {
        self.input.press("ZL")?;
        sleep(BEAT / 2);
        for push in search() {
            if self.dev.state()?.event || landed(self)? {
                break;
            }
            self.input.stick(push, STEP_MS)?;
        }
        Ok(())
    }

    /// Press A through the event alone, for [`ALONE`]. A verdict of pass
    /// when it lands and the event ends, `None` for the judge to go on.
    fn alone(&mut self, landed: &impl Fn(&Self) -> Result<bool>) -> Result<Option<Verdict>> {
        let start = Instant::now();
        while start.elapsed() < ALONE {
            let s = self.dev.state()?;
            if !s.event && !s.loading && landed(self)? {
                return Ok(Some(Verdict::Pass));
            }
            if !s.event && start.elapsed() > IDLE {
                return Ok(None);
            }
            if s.event {
                self.input.press("A")?;
            }
            sleep(BEAT);
        }
        Ok(None)
    }

    /// The judge's turns. `None` when it resolved.
    fn judged(
        &mut self,
        p: &Planned,
        name: &str,
        landed: &impl Fn(&Self) -> Result<bool>,
        swept: &mut Swept,
    ) -> Result<Option<Verdict>> {
        let mut tried: Vec<String> = Vec::new();
        for turn in 0..TURNS {
            let s = self.dev.state()?;
            let shot = self.frame(p, name, turn);
            self.input.shot(&shot)?;
            swept.shots.push(shot.clone());
            let look = Look {
                spot: name,
                expected: &p.spot.item_name,
                map: &s.map,
                event: s.event,
                tried: &tried,
                shot: &shot,
            };
            let answer = self.judge.look(&look)?;
            swept.judge.push(answer.line());
            match &answer {
                Answer::Press { button, .. } => self.input.press(button)?,
                Answer::Stick { direction, ms, .. } => self.input.stick(direction, *ms)?,
                Answer::Wait { .. } => sleep(BEAT * 2),
                Answer::Play { .. } => self.perform(song(p))?,
                Answer::Bug { why } => return Ok(Some(Verdict::Bug(why.clone()))),
                Answer::Unreadable(_) => {}
            }
            tried.push(answer.line());
            sleep(BEAT * 2);
            let s = self.dev.state()?;
            if !s.event && !s.loading && landed(self)? {
                let how = tried.join(", then ");
                self.judge
                    .learn(&format!("{name} ({}): {how} got it", s.map))?;
                return Ok(None);
            }
        }
        Ok(self.finish("A", Some(song(p)))?.then_some(Verdict::Stall))
    }

    /// Teleport to `place`, asking again while the game doesn't take it:
    /// right after a restart's save loads it ignores the request, and Hob's
    /// spots waited 30 s for a teleport that never left Link's house
    /// (2026-11-14).
    fn go(&mut self, place: &Place, child: Option<bool>) -> Result<()> {
        let start = Instant::now();
        loop {
            match self.calmly(|dev| dev.go_as(place, child)) {
                Err(e) if format!("{e:#}").contains(NOT_TAKEN) && start.elapsed() < CALM => {
                    sleep(BEAT)
                }
                done => return done.map(drop),
            }
        }
    }

    /// Start the event `event`, asking again until it plays: right after a
    /// map loads the game ignores the request, and a cow's event never
    /// started (Mac, 2026-11-14). An event that ends before a look still
    /// counts as started.
    fn play(&mut self, event: &str) -> Result<()> {
        let start = Instant::now();
        loop {
            self.calm()?;
            self.dev.ask(&Request::Play(event.to_owned()))?;
            sleep(BEAT);
            if self.dev.state()?.event || start.elapsed() > CALM {
                return Ok(());
            }
        }
    }

    /// Warp Link to `at`, where his step starts the event, and say whether
    /// it did within `STEP_IN`.
    fn step_in(&mut self, at: Option<[f32; 3]>) -> Result<bool> {
        let Some(at) = at else {
            return Ok(false);
        };
        self.calmly(|dev| dev.warp(at))?;
        let start = Instant::now();
        while start.elapsed() < STEP_IN {
            if self.dev.state()?.event {
                return Ok(true);
            }
            sleep(BEAT);
        }
        Ok(false)
    }

    /// Play the song `item` teaches, note by note: nothing for an item
    /// that isn't a song.
    fn perform(&mut self, item: &str) -> Result<()> {
        for note in melody::notes(item).into_iter().flatten() {
            self.input.press(note)?;
            sleep(NOTE);
        }
        Ok(())
    }

    /// Press `button` through an event still playing after the judge's
    /// turns, for [`FINISH`]: a Great Fairy's scene grants partway and runs
    /// on for over a minute. Every [`ENCORE`] beats, play `song` instead: a
    /// lesson's "Play a note" prompt outlasts the judge's turns, and A is
    /// a note. Whether it still plays after.
    fn finish(&mut self, button: &str, song: Option<&str>) -> Result<bool> {
        let start = Instant::now();
        for beat in 1.. {
            if !self.dev.state()?.event {
                break;
            }
            if start.elapsed() > FINISH {
                return Ok(true);
            }
            let encore = song.filter(|s| melody::notes(s).is_some());
            match encore {
                Some(song) if beat % ENCORE == 0 => self.perform(song)?,
                _ => self.input.press(button)?,
            }
            sleep(BEAT);
        }
        Ok(false)
    }

    /// Buy `item` off a shop shelf: A through the keeper's talk into
    /// browsing ("I'd like to browse" is its first line), move the cursor
    /// onto the goods, A through the price line and the get window, then B
    /// out, which backs out of browsing, says goodbye and ends the talk.
    /// The judge bought Hob's Token but left Link browsing, where A buys
    /// again, a stall (2026-11-14). `None` for the judge to go on when
    /// browsing never starts.
    fn buy(
        &mut self,
        item: &str,
        landed: &impl Fn(&Self) -> Result<bool>,
        swept: &mut Swept,
    ) -> Result<Option<Verdict>> {
        let (game, global) = (self.dev.game(), self.dev.global());
        live::set(game, global, live::hash(CURSOR)?, "-1")?;
        let start = Instant::now();
        while self.cursor()? < 0 {
            if start.elapsed() > ALONE {
                return Ok(None);
            }
            self.input.press("A")?;
            sleep(BEAT);
        }
        if self.shelve(item, &mut swept.judge)? {
            // Exactly the price: a buy that leaves more than the wallet
            // holds is refused as a full bag, rupees the most often (Mac,
            // 2026-11-14).
            let price = live::read(self.dev.game(), self.gd, self.dev.global(), PRICE)?;
            live::set(
                self.dev.game(),
                self.dev.global(),
                live::hash(RUPEES.0)?,
                &price,
            )?;
            for _ in 0..BUY {
                self.input.press("A")?;
                sleep(BEAT);
                if landed(self)? {
                    break;
                }
            }
        }
        Ok(Some(match self.finish("B", None)? {
            true => Verdict::Stall,
            false => Verdict::Nothing,
        }))
    }

    /// Move the shop cursor onto goods that shelve as `item`, trying each
    /// push from each shelf spot seen and reached until one does, and say
    /// whether it
    /// found them. Each spot's goods go to `seen`. The shelves aren't a
    /// grid one push a spot: Kokiri's cursor went 0, 1, 4, 5 pushed right
    /// (Mac, 2026-11-14), so where a push goes is learned, not assumed.
    fn shelve(&mut self, item: &str, seen: &mut Vec<String>) -> Result<bool> {
        let mut pushes = BTreeMap::new();
        let mut spots = BTreeSet::new();
        loop {
            let at = self.cursor()?;
            if spots.insert(at) {
                let goods = self.goods()?;
                seen.push(format!("shelf spot {at}: {goods}"));
                if shelves_as(item, &goods) {
                    return Ok(true);
                }
            }
            let untried = (spots.iter())
                .flat_map(|&spot| (0..MOVES.len()).map(move |m| (spot, m)))
                .filter(|push| !pushes.contains_key(push))
                .find_map(|(from, m)| Some((from, m, route(&pushes, at, from)?)));
            let Some((from, m, path)) = untried else {
                return Ok(false);
            };
            for step in path {
                self.nudge(step)?;
            }
            if self.cursor()? != from {
                return Ok(false);
            }
            self.nudge(m)?;
            pushes.insert((from, m), self.cursor()?);
        }
    }

    fn nudge(&mut self, m: usize) -> Result<()> {
        self.input.stick(MOVES[m], NUDGE_MS)?;
        sleep(NUDGE);
        Ok(())
    }

    fn cursor(&self) -> Result<i64> {
        let value = self.value(CURSOR)?;
        value
            .parse()
            .with_context(|| format!("{CURSOR} reads {value}"))
    }

    fn goods(&self) -> Result<String> {
        let value = self.value(GOODS)?;
        Ok(value.trim_matches('"').trim_end_matches("\\0").to_owned())
    }

    fn value(&self, name: &str) -> Result<String> {
        let hash = live::hash(name)?;
        live::read(self.dev.game(), self.gd, self.dev.global(), hash)
    }
}

/// Whether a shelf shows `item` as `goods`: a chest's rupee shelves as the
/// rupee itself (`TBox_Rupee_Red` as `Obj_Rupee_Red`), so the name past
/// its kind decides.
fn shelves_as(item: &str, goods: &str) -> bool {
    fn bare(name: &str) -> &str {
        name.split_once('_').map_or(name, |(_, rest)| rest)
    }
    bare(item) == bare(goods)
}

/// The pushes that take the shop cursor from `from` to `to`, over the
/// ones seen so far (`pushes` maps a spot and push to where it went).
fn route(pushes: &BTreeMap<(i64, usize), i64>, from: i64, to: i64) -> Option<Vec<usize>> {
    let mut paths = BTreeMap::from([(from, vec![])]);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        if at == to {
            return paths.remove(&to);
        }
        for (&(_, m), &next) in pushes.range((at, 0)..(at + 1, 0)) {
            if !paths.contains_key(&next) {
                let mut path = paths[&at].clone();
                path.push(m);
                paths.insert(next, path);
                queue.push_back(next);
            }
        }
    }
    None
}

/// The stick pushes Link takes looking for a drop, the camera behind him:
/// a square spiral out from where he stands. Its rows are a step apart, so
/// it passes over a drop that bounced aside or behind.
fn search() -> impl Iterator<Item = &'static str> {
    const TURNS: [&str; 4] = ["LUp", "LRight", "LDown", "LLeft"];
    (0..2 * SPIRAL).flat_map(|leg| std::iter::repeat_n(TURNS[leg % 4], leg / 2 + 1))
}

/// `name` fit for a file name.
pub fn slug(name: &str) -> String {
    let s: String = (name.chars())
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.split('-')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Whether Link may have to pay at the spot: a scrub or a shop sells, and
/// each Great Fairy asks an offering, up to 500 rupees, before her gift.
/// The song to play when the judge sees a prompt for one: the trigger's own,
/// else the one the spot's vanilla item teaches.
fn song(p: &Planned) -> &str {
    match &p.trigger {
        Trigger::Play {
            melody: Some(song), ..
        } => song,
        _ => &p.spot.vanilla,
    }
}

fn pays(p: &Planned) -> bool {
    matches!(p.trigger, Trigger::Talk { .. } | Trigger::Play { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, flow, item, scene};
    use crate::sweep::plan::Steps;
    use crate::warp::Side;

    #[test]
    fn a_chest_rupee_shelves_as_the_rupee_and_a_token_as_itself() {
        assert!(shelves_as(actor::T_BOX_RUPEE_RED, actor::OBJ_RUPEE_RED));
        assert!(shelves_as(item::TOKEN_GOLD, item::TOKEN_GOLD));
        assert!(!shelves_as(actor::T_BOX_RUPEE_RED, actor::OBJ_RUPEE_PURPLE));
    }

    #[test]
    fn the_shop_cursor_is_routed_over_the_pushes_seen() {
        // Kokiri's shelf on the Mac: right from 0 went to 1, from 1 to 4.
        let pushes = BTreeMap::from([((0, 0), 1), ((1, 0), 4), ((4, 2), 1), ((1, 2), 0)]);
        assert_eq!(route(&pushes, 0, 4), Some(vec![0, 0]));
        assert_eq!(route(&pushes, 4, 0), Some(vec![2, 2]));
        assert_eq!(route(&pushes, 4, 4), Some(vec![]));
        assert_eq!(route(&pushes, 0, 6), None);
    }

    #[test]
    fn verdicts_serialize_with_their_reason() {
        let v = serde_json::to_value(Verdict::WrongItem("IsGet.Item_Bow".into())).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"verdict": "wrong_item", "why": "IsGet.Item_Bow"})
        );
        let v = serde_json::to_value(Verdict::Pass).unwrap();
        assert_eq!(v, serde_json::json!({"verdict": "pass"}));
        assert!(Verdict::Stall.broke() && !Verdict::Nothing.broke());
    }

    #[test]
    fn a_gone_game_is_told_from_a_refusal() {
        assert!(gone(&anyhow::anyhow!(
            "the game didn't answer mem_read in 30s"
        )));
        assert!(!gone(&anyhow::anyhow!("an event is playing")));
    }

    #[test]
    fn a_chest_item_is_watched_with_its_field_twin() {
        assert_eq!(
            twins(actor::T_BOX_RUPEE_RED),
            [actor::T_BOX_RUPEE_RED, actor::OBJ_RUPEE_RED]
        );
        assert_eq!(twins("Item_Bow"), ["Item_Bow"]);
    }

    #[test]
    fn an_upgrade_is_watched_at_every_level_of_its_line() {
        let names = twins(item::RANGE_BOW);
        assert_eq!(names[0], item::RANGE_BOW);
        for n in [
            actor::WEAPON_RANGE_BOW,
            item::QUIVER_MEDIUM,
            item::QUIVER_LARGE,
        ] {
            assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
        }
        assert_eq!(
            twins(item::SCALE_SILVER),
            [item::SCALE_SILVER, item::SCALE_GOLD]
        );
    }

    #[test]
    fn a_gear_pickup_is_watched_as_the_gear_it_is_owned_as() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let names = owned_as(&romfs, actor::OBJ_ARMOR_GORON).unwrap();
        assert_eq!(names, [actor::OBJ_ARMOR_GORON, actor::ARMOR_ADULT_GORON]);
        assert_eq!(owned_as(&romfs, "Item_Bow").unwrap(), ["Item_Bow"]);
    }

    #[test]
    fn a_get_message_names_its_item() {
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let got = [
            change(&format!("IsGet.{}", actor::OBJ_RUPEE_RED), "false -> true"),
            change(
                MESSAGE,
                &format!(
                    r#""{}" -> "{}""#,
                    actor::OBJ_HEART_CONTAINER,
                    actor::T_BOX_RUPEE_BLUE
                ),
            ),
        ];
        assert_eq!(named(&got).as_deref(), Some(actor::T_BOX_RUPEE_BLUE.get()));
        let cleared = [change(
            MESSAGE,
            &format!(r#""{}" -> """#, actor::T_BOX_RUPEE_BLUE),
        )];
        assert_eq!(named(&cleared), None);
        assert_eq!(named(&got[..1]), None);
    }

    #[test]
    fn only_a_flag_turned_on_is_another_get() {
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let wanted = [format!("IsGet.{}", item::HOVER_BOOT)];
        let lost = [change(
            &format!("IsGet.{}", actor::OBJ_HEART_CONTAINER),
            "true -> false",
        )];
        assert_eq!(got_other(&lost, &wanted), None);
        let got = [change("IsGet.Item_Bow", "false -> true")];
        assert_eq!(got_other(&got, &wanted).as_deref(), Some("IsGet.Item_Bow"));
        let ours = [change(
            &format!("IsGet.{}", item::HOVER_BOOT),
            "false -> true",
        )];
        assert_eq!(got_other(&ours, &wanted), None);
    }

    #[test]
    fn a_rupee_off_the_floor_is_not_another_get() {
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let wanted = [format!("IsGet.{}", item::NUT_BAG_MEDIUM)];
        let floor = [
            change(
                &format!("IsGet.{}", actor::OBJ_RUPEE_GREEN),
                "false -> true",
            ),
            change(MESSAGE, &format!(r#""" -> "{}""#, actor::OBJ_RUPEE_GREEN)),
        ];
        assert_eq!(got_other(&floor, &wanted), None);
        let chest = [
            change(
                &format!("IsGet.{}", actor::OBJ_RUPEE_GREEN),
                "false -> true",
            ),
            change(MESSAGE, &format!(r#""" -> "{}""#, actor::T_BOX_RUPEE_GREEN)),
        ];
        let got = got_other(&chest, &wanted);
        assert_eq!(
            got.as_deref(),
            Some(
                format!(
                    "{} {}",
                    flag::UI_MESSAGE_TARGET_ACTOR_NAME,
                    actor::T_BOX_RUPEE_GREEN
                )
                .as_str()
            )
        );
    }

    #[test]
    fn arrows_without_a_bow_come_as_a_rupee() {
        assert!(swapped(
            actor::T_BOX_ARROW_BUNDLE_MEDIUM,
            actor::T_BOX_RUPEE_RED
        ));
        assert!(!swapped(actor::T_BOX_ARROW_BUNDLE_MEDIUM, "Item_Bow"));
        assert!(!swapped(actor::T_BOX_RUPEE_BLUE, actor::T_BOX_RUPEE_RED));
    }

    #[test]
    fn a_request_refused_for_an_event_waits_and_goes_again() {
        let (mut asks, mut calms) = (0, 0);
        let got = calmly(
            || {
                asks += 1;
                match asks {
                    1 | 2 => Err(anyhow::anyhow!("refused: {EVENT_PLAYING}")),
                    _ => Ok(asks),
                }
            },
            || {
                calms += 1;
                Ok(())
            },
        );
        assert_eq!(got.unwrap(), 3);
        assert_eq!(calms, 2);
        let other = calmly(
            || Err::<(), _>(anyhow::anyhow!("a map is loading")),
            || Ok(()),
        );
        assert!(other.is_err(), "only an event is waited out");
    }

    #[test]
    fn the_search_passes_within_a_step_of_every_spot_around_the_break() {
        let mut at = (0i32, 0i32);
        let mut seen = std::collections::BTreeSet::from([at]);
        for push in search() {
            match push {
                "LUp" => at.1 += 1,
                "LDown" => at.1 -= 1,
                "LRight" => at.0 += 1,
                _ => at.0 -= 1,
            }
            seen.insert(at);
        }
        let reach = SPIRAL as i32 / 2 - 1;
        for x in -reach..=reach {
            for y in -reach..=reach {
                assert!(seen.contains(&(x, y)), "never near ({x}, {y})");
            }
        }
    }

    #[test]
    fn slugs_are_file_names() {
        assert_eq!(slug("Mido's House: Back Left"), "mido-s-house-back-left");
        // The other age's try names its frames apart from the first's.
        assert_ne!(
            slug("Ice Cavern Red Rupee"),
            slug("Ice Cavern Red Rupee (as child)")
        );
    }

    #[test]
    fn quest_steps_and_the_guards_are_written_as_their_save_copies() {
        let at = |area: &str, steps: Steps| Planned {
            spot: crate::generate::Spot {
                area: area.into(),
                ..Default::default()
            },
            trigger: Trigger::Talk {
                map: String::new(),
                at: [0.0; 3],
                face: [0.0; 3],
                steps,
                ready: vec!["Flag".into()],
                then: None,
                give: None,
                counter: false,
                halt: None,
            },
            child: None,
            side: Side::Front,
        };
        let biggoron = vec![(
            flow::SUB_EVENT_GET_SWORD_BIG_GORON.to_string(),
            "Step11".into(),
        )];
        assert_eq!(
            presets(&at(flag::DEATH_MOUNTAIN_TRAIL, biggoron)),
            ["Flag", "Step_SubEvent_GetSwordBigGoron=Step11"]
        );
        assert_eq!(
            presets(&at(flag::GERUDO_FORTRESS, vec![])),
            ["Flag", &format!("{}=Step02", flag::STEP_SAGE_OF_SPIRIT)]
        );
        // A step the spot names itself wins over the guards'.
        let named = vec![(flow::SAGE_OF_SPIRIT.to_string(), "Step04".into())];
        assert_eq!(
            presets(&at(flag::GERUDO_FORTRESS, named)),
            ["Flag", &format!("{}=Step04", flag::STEP_SAGE_OF_SPIRIT)]
        );
    }

    #[test]
    fn link_pays_at_a_talk_or_a_play_spot() {
        let with = |trigger| Planned {
            spot: Default::default(),
            trigger,
            child: None,
            side: Side::Front,
        };
        let play = Trigger::Play {
            map: None,
            event: scene::FAIRY_FOUNTAIN_FRORE.to_string(),
            steps: vec![],
            ready: vec![],
            melody: None,
            at: None,
        };
        assert!(pays(&with(play)));
        let open = Trigger::Open {
            map: String::new(),
            at: Default::default(),
            face: Default::default(),
            ready: vec![],
            steps: vec![],
            first: None,
            broken: false,
        };
        assert!(!pays(&with(open)));
    }
}
