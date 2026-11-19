//! Sweeping one spot: set it up, trigger it, see it through (with the
//! judge when it doesn't resolve by itself), and give a verdict.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;

use super::judge::{Answer, Judge, Look, TURNS};
use super::melody;
use super::plan::{Halt, Planned, Trigger, preset};
use super::verdict::{Kill, Swept, Verdict};
use super::watch::{MESSAGE, Watch, got_other, named, swapped, wielded};
use super::wield::{self, Wield};
use crate::dev::{Dev, EVENT_PLAYING, Fate, NOT_TAKEN, Place, Request, Target};
use crate::gamedata::GameData;
use crate::gear;
use crate::gerudo;
use crate::known::{Name, field, flag, map};
use crate::live;
use crate::nav::act::{Touch, Used};
use crate::nav::drive::Drive;
use crate::nav::graph::dist;
use crate::nav::{self, Level};
use crate::play::Input;
use crate::shops;
use crate::steps::SAVED;
use crate::warp::Placement;

mod drop;

use drop::neighbour;

/// How long the harness presses through an event by itself before the
/// judge is asked.
pub(super) const ALONE: Duration = Duration::from_secs(12);
/// Link's rupees, and what a spot that sells fills them to: past any
/// wallet's size, which the game lets stand, so the dearest offering, 500,
/// is met without a wallet upgrade.
pub(super) const RUPEES: (&Name, &str) = (flag::PLAYER_STATUS_CURRENT_RUPEE, "999");
/// How long an event may run on after the judge's turns before it counts
/// as a stall.
const FINISH: Duration = Duration::from_secs(90);
/// How long a spot with no event may go before it counts as not started.
const IDLE: Duration = Duration::from_secs(3);
/// How long the game gets to calm down before a spot starts.
const CALM: Duration = Duration::from_secs(15);
pub(super) const BEAT: Duration = Duration::from_millis(1000);
/// Between a song's notes: a lesson takes them at this pace.
const NOTE: Duration = Duration::from_millis(600);
/// How long an event takes to bring the ocarina out before it takes notes.
const OCARINA: Duration = Duration::from_millis(3000);
/// How long an event its sensor starts gets to begin once Link steps in.
const STEP_IN: Duration = Duration::from_secs(5);
/// How many beats of A `finish` presses between songs.
const ENCORE: usize = 5;
/// How far from the holder a spot's foes are looked for, and how long
/// their latch gets to set once they're beaten.
const FOES_WITHIN: f32 = 60.0;
const LATCH: Duration = Duration::from_secs(10);
/// How long a rolling giver may take to pass his stopping spot: a lap of
/// the rolling Goron's track is about 19 s (Mac, 2026-11-15).
const LAP: Duration = Duration::from_secs(45);
/// How often his passing is read: he crosses the spot in well under a
/// second.
const PASSING: Duration = Duration::from_millis(50);
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

/// How many times a request refused for an event is asked again.
const AGAIN: usize = 3;

/// The GameData values a spot writes before its teleport and holds through
/// the load: its trigger's ready values, then each quest step it needs, as
/// the step's save copy `Step_<challenge>`. Gates read that copy (a
/// keeper's presence). The copy is one write held through the load, where
/// a step run through the quest system is an event of its own.
/// The quest steps a spot needs.
fn steps(p: &Planned) -> &[(String, String)] {
    match &p.trigger {
        Trigger::Open { steps, .. } | Trigger::Talk { steps, .. } | Trigger::Play { steps, .. } => {
            steps
        }
        Trigger::Skip(_) => &[],
    }
}

fn presets(p: &Planned) -> Vec<String> {
    let (ready, quests) = match &p.trigger {
        Trigger::Open { ready, steps, .. }
        | Trigger::Talk { ready, steps, .. }
        | Trigger::Play { ready, steps, .. } => (ready, steps),
        Trigger::Skip(_) => return vec![],
    };
    let saved = quests.iter().map(|(c, s)| format!("{SAVED}{c}={s}"));
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

/// How far from the holder a warp beside it can leave Link: farther, with
/// nothing started and no step taken, and the warp voided him out.
const VOIDED: f32 = 6.0;

/// Whether `use` gave up because its warp didn't hold: Link fell and came
/// back where he entered the map, tens of metres off.
fn voided(used: &Used) -> bool {
    matches!(used.result, Touch::Nothing)
        && used.went.legs == 0
        && used.went.left.is_some_and(|left| left > VOIDED)
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
        super::clock::set(self.dev, &p.trigger)?;
        let unset = self.ready(p)?;
        self.setup(p, !unset.is_empty())?;
        let cleared = self.ready(p)?;
        // The boots come off: a holder under water puts them on itself.
        self.calm()?;
        gear::wear(self.dev, self.gd, &mut *self.input, false)?;
        if let Trigger::Open {
            first: Some(event), ..
        } = &p.trigger
        {
            self.play(event)?;
            self.calm()?;
        }
        if let Trigger::Open {
            map,
            at,
            foes,
            latch,
            ..
        } = &p.trigger
            && !foes.is_empty()
            && let Some(verdict) = self.beat(p, map, *at, foes, latch, swept)?
        {
            return Ok(verdict);
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
        let mut changes = live::changes(self.gd, &was, &now, |_| true);
        // A shop may sell Blue Fire or a hammer, so only one wielded is ours.
        if let Trigger::Open { wield: Some(w), .. } = &p.trigger {
            changes.retain(|c| !wielded(c, w.item));
        }
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
        let windowed = !matches!(p.trigger, Trigger::Open { broken: true, .. });
        let other = got_other(&changes, &wanted, windowed);
        let next_door = match (&p.trigger, &other) {
            (Trigger::Open { near, .. }, Some(c)) => neighbour(self.romfs, near, c),
            _ => None,
        };
        Ok(match (verdict, ours, other) {
            (Some(v @ (Verdict::Bug(_) | Verdict::Stall)), _, _) => v,
            (_, true, _) => Verdict::Pass,
            _ if swap.is_some() => Verdict::Substituted(swap.unwrap_or_default()),
            _ if next_door.is_some() => Verdict::Neighbour(next_door.unwrap_or_default()),
            (_, false, Some(c)) => Verdict::WrongItem(c),
            _ => Verdict::Nothing,
        })
    }

    /// [`calmly`] with this spot's game.
    fn calmly<T>(&mut self, ask: impl Fn(&Dev) -> Result<T>) -> Result<T> {
        let dev = self.dev;
        calmly(|| ask(dev), || self.calm())
    }

    /// [`quiet`] with this spot's game.
    fn calm(&mut self) -> Result<()> {
        quiet(self.dev, &mut *self.input)
    }

    /// Sets the spot's [`presets`], and names those that weren't set.
    /// A map load clears a scrub's seller flag, and the scrub reads it once,
    /// as it spawns: the Shadow Temple scrub set after the load still shot
    /// Link, and one set all through the load sold (Mac, 2026-11-13).
    fn ready(&self, p: &Planned) -> Result<Vec<String>> {
        self.hold(&presets(p))
    }

    /// Sets each entry, and names those that weren't set.
    fn hold(&self, entries: &[String]) -> Result<Vec<String>> {
        let (game, global) = (self.dev.game(), self.dev.global());
        let mut cleared = vec![];
        for entry in entries {
            let (name, value) = preset(entry);
            let hash = live::hash(name)?;
            if live::read(game, self.gd, global, hash)? != value {
                cleared.push(entry.clone());
            }
            live::set(game, global, hash, value)?;
        }
        Ok(cleared)
    }

    /// Whether Link holds `item`.
    fn holds(&self, item: &str) -> Result<bool> {
        let hash = live::hash(&format!("IsGet.{item}"))?;
        let (game, global) = (self.dev.game(), self.dev.global());
        Ok(live::read(game, self.gd, global, hash)? == "true")
    }

    /// The song a cow wants, the item a giver takes or the card the guards
    /// want, then the map: loaded again when Link is already in it but
    /// `reload`, since a preset that wasn't set yet (a raised chest's flag,
    /// a scrub's seller flag) is read only as the map spawns. Quest
    /// steps are [`presets`].
    fn setup(&mut self, p: &Planned, reload: bool) -> Result<()> {
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
        // Held before the teleport, so the map change stands the guards
        // down as it would in a seed.
        if let Some(card) = gerudo::guard_card(&p.spot.area, steps(p))
            && !self.holds(card)?
        {
            let gd = self.gd;
            self.calmly(|dev| dev.give(gd, card, 1))?;
        }
        let Some(map) = map else {
            return Ok(());
        };
        let here = self.dev.state()?;
        if !reload && here.map == *map && p.child.is_none_or(|c| c == here.child) {
            return Ok(());
        }
        self.reload(p, map, &[])
    }

    /// Load `map` from its arrival point, holding the spot's [`presets`]
    /// and `more` through the load.
    fn reload(&mut self, p: &Planned, map: &str, more: &[String]) -> Result<()> {
        let place =
            arrival(self.places, map).with_context(|| format!("no arrival point in {map}"))?;
        self.go(place, p.child)?;
        let held = [presets(p), more.to_vec()].concat();
        let dev = self.dev;
        dev.land_holding(map, || self.hold(&held).map(drop))
    }

    /// Beat the spot's `foes` for real from `at` beside its holder, so
    /// their own deaths run, then wait for `latch`. When a kill doesn't
    /// take, `latch` is written and the map loaded again, as before real
    /// kills.
    fn beat(
        &mut self,
        p: &Planned,
        map: &str,
        at: [f32; 3],
        foes: &[u64],
        latch: &[String],
        swept: &mut Swept,
    ) -> Result<Option<Verdict>> {
        self.calmly(|dev| dev.warp(at))?;
        let mut died = true;
        for &foe in foes {
            match self
                .dev
                .slay(self.romfs, &Target::Placement(foe), FOES_WITHIN)
            {
                Ok(slain) => died &= slain.0.iter().all(|s| s.fate == Fate::Died),
                Err(e) if gone(&e) => return Err(e),
                Err(_) => died = false,
            }
        }
        self.calm()?;
        if !died {
            swept.kill = Some(Kill::LatchedNotKilled);
            self.reload(p, map, latch)?;
            return Ok(None);
        }
        let start = Instant::now();
        while start.elapsed() < LATCH {
            if self.latched(latch)? {
                swept.kill = Some(Kill::Killed);
                return Ok(None);
            }
            sleep(BEAT);
        }
        Ok(Some(Verdict::DiedNoLatch(latch.join(", "))))
    }

    /// Whether every entry already holds its value.
    fn latched(&self, entries: &[String]) -> Result<bool> {
        let (game, global) = (self.dev.game(), self.dev.global());
        for entry in entries {
            let (name, value) = preset(entry);
            if live::read(game, self.gd, global, live::hash(name)?)? != value {
                return Ok(false);
            }
        }
        Ok(true)
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
        let (at, face, here, halt, sides) = match &p.trigger {
            Trigger::Open {
                at, face, sides, ..
            } => (*at, *face, false, None, sides.as_slice()),
            Trigger::Talk {
                at,
                face,
                counter,
                halt,
                ..
            } => (*at, *face, *counter, *halt, &[][..]),
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
        // Up on a pillar: stand on it and take it from there.
        let aloft = nav::aloft(&graph, holder);
        let spots = match (aloft, here) {
            (true, _) => vec![holder.pose.at],
            (false, true) => vec![at],
            (false, false) => nav::landings(&graph, holder, at, p.side, sides),
        };
        let here = here || aloft;
        for (i, &at) in spots.iter().enumerate() {
            // Under water, Link uses it from the surface over it: a plane's,
            // or wherever a warp into water put him back up.
            let over = graph.above(at);
            self.calmly(|dev| dev.warp(over.unwrap_or(at)))?;
            let under = over.is_some() || nav::sink::floated(self.dev.link()?, at);
            if let Trigger::Open { wield: Some(w), .. } = &p.trigger {
                self.wield(w, p, name, swept)?;
            }
            let shot = self.frame(p, name, "use");
            let mut drive = Drive::new(self.dev, &mut *self.input);
            drive.side = p.side;
            drive.then = sides.to_vec();
            drive.here = here;
            let used = match under {
                true => nav::sink::touch_under(&mut drive, self.gd, &graph, holder, shot)?,
                false => drive.touch(&graph, holder, shot)?,
            };
            swept.shots.extend(used.shot.clone());
            let voided = voided(&used);
            swept.used = Some(used);
            if !voided || i + 1 == spots.len() {
                break;
            }
            eprintln!("    {name}: the warp beside it didn't hold, another side");
        }
        if let Trigger::Talk {
            then: Some(button), ..
        } = &p.trigger
        {
            self.after_talk(button)?;
        }
        Ok(())
    }

    /// Wield `w`: give its item, put it on ZR from the Items screen, face
    /// what it acts on and press ZR, then stand on the switch it frees.
    /// Out of reach of that, Link warps beside it first, and back after.
    fn wield(&mut self, w: &Wield, p: &Planned, name: &str, swept: &mut Swept) -> Result<()> {
        self.equip(w.item)?;
        let here = self.dev.link()?;
        let far = dist(here, w.on) > wield::REACH;
        if far {
            self.calmly(|dev| dev.warp(wield::beside(w.on, here)))?;
        }
        // A pour facing away empties the bottle and leaves the ice.
        Drive::new(self.dev, &mut *self.input).face(w.on)?;
        self.input.press("ZR")?;
        sleep(w.after);
        self.shot(p, name, "wield", swept);
        if let Some(switch) = w.press {
            self.calmly(|dev| dev.warp(switch))?;
            sleep(wield::PRESSED);
        }
        if far || w.press.is_some() {
            self.calmly(|dev| dev.warp(here))?;
        }
        Ok(())
    }

    /// Give `item` and press it in the menu, once no event plays.
    fn equip(&mut self, item: &'static Name) -> Result<()> {
        self.calm()?;
        gear::equip(self.dev, self.gd, &mut *self.input, item)
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
        teleport(self.dev, &mut *self.input, place, child)
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
    pub(super) fn finish(&mut self, button: &str, song: Option<&str>) -> Result<bool> {
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

/// Whether Link may have to pay at the spot: a scrub or a shop sells, and
/// each Great Fairy asks an offering, up to 500 rupees, before her gift.
fn pays(p: &Planned) -> bool {
    matches!(p.trigger, Trigger::Talk { .. } | Trigger::Play { .. })
}

/// Arrival points inside the trigger of a scene that plays as Link lands
/// and waits on its get window, so a teleport there never settles and the
/// item is given before the sweep looks: the adult meadow's by the temple,
/// where Sheik teaches his song.
const SCENE_ON_ARRIVAL: [(&Name, &Name); 1] = [(
    map::SACRED_FOREST_MEADOW_ADULT,
    field::OUTSIDE_OF_FOREST_TEMPLE,
)];

/// Where a teleport to `map` arrives: its first arrival point that starts
/// no scene, or its first.
/// Wait for no event and no load, pressing A through any text left.
pub(super) fn quiet<I: Input>(dev: &Dev, input: &mut I) -> Result<()> {
    let start = Instant::now();
    loop {
        let s = dev.state()?;
        if !s.event && !s.loading {
            return Ok(());
        }
        if start.elapsed() > CALM {
            bail!("an event from before the spot never ended");
        }
        if s.event {
            input.press("A")?;
        }
        sleep(BEAT);
    }
}

/// Teleport to `place` as `child`, asking again while the game ignores it
/// right after a load.
pub(super) fn teleport<I: Input>(
    dev: &Dev,
    input: &mut I,
    place: &Place,
    child: Option<bool>,
) -> Result<()> {
    let start = Instant::now();
    loop {
        match calmly(|| dev.go_as(place, child), || quiet(dev, &mut *input)) {
            Err(e) if format!("{e:#}").contains(NOT_TAKEN) && start.elapsed() < CALM => sleep(BEAT),
            done => return done.map(drop),
        }
    }
}

pub(super) fn arrival<'a>(places: &'a BTreeSet<Place>, map: &str) -> Option<&'a Place> {
    let quiet =
        |pl: &&Place| !(SCENE_ON_ARRIVAL.iter()).any(|(m, s)| **m == *pl.map && **s == *pl.spot);
    let mut of = places.iter().filter(|pl| pl.map == map);
    of.clone().find(quiet).or_else(|| of.next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, flow, scene};
    use crate::sweep::plan::Steps;
    use crate::warp::Side;

    #[test]
    fn a_use_that_never_left_a_far_landing_voided() {
        use crate::nav::went::{Done, Reason, Went};
        let used = |result, legs, left| Used {
            name: actor::ENEMY_SCRUB_BUSINESS.to_string(),
            hash: None,
            result,
            went: Went {
                result: Done::GaveUp,
                reason: Some(Reason::NoRoute),
                secs: 0.8,
                legs,
                left,
            },
            hurt: None,
            side: None,
            shot: None,
        };
        // The sticks scrub's sweep rows: no route, 87.7 m off, no step.
        assert!(voided(&used(Touch::Nothing, 0, Some(87.7))));
        assert!(!voided(&used(Touch::Nothing, 0, Some(1.5))), "beside it");
        assert!(!voided(&used(Touch::Nothing, 3, Some(87.7))), "it walked");
        assert!(!voided(&used(Touch::Event, 0, Some(87.7))), "it started");
        assert!(!voided(&used(Touch::Nothing, 0, None)), "left the map");
    }

    #[test]
    fn a_teleport_arrives_where_no_scene_starts() {
        needs_names!();
        let meadow = map::SACRED_FOREST_MEADOW_ADULT.to_string();
        let place = |map: &str, spot: &str| Place {
            map: map.to_string(),
            spot: spot.to_string(),
        };
        let temple = place(&meadow, field::OUTSIDE_OF_FOREST_TEMPLE.get());
        let woods = place(&meadow, "Zz_FakeEntrance");
        let both = BTreeSet::from([temple.clone(), woods.clone()]);
        assert_eq!(arrival(&both, &meadow), Some(&woods));
        let only = BTreeSet::from([temple.clone()]);
        assert_eq!(arrival(&only, &meadow), Some(&temple), "the only way in");
        assert_eq!(arrival(&both, "FakeMap"), None);
    }

    #[test]
    fn a_gone_game_is_told_from_a_refusal() {
        assert!(gone(&anyhow::anyhow!(
            "the game didn't answer mem_read in 30s"
        )));
        assert!(!gone(&anyhow::anyhow!("an event is playing")));
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
    fn slugs_are_file_names() {
        assert_eq!(slug("Mido's House: Back Left"), "mido-s-house-back-left");
        // The other age's try names its frames apart from the first's.
        assert_ne!(
            slug("Ice Cavern Red Rupee"),
            slug("Ice Cavern Red Rupee (as child)")
        );
    }

    #[test]
    fn quest_steps_are_written_as_their_save_copies() {
        needs_names!();
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
            [
                "Flag".to_owned(),
                crate::steps::at(flow::SUB_EVENT_GET_SWORD_BIG_GORON, "Step11")
            ]
        );
        // The guards never read the save copy: the card stands them down.
        assert_eq!(presets(&at(flag::GERUDO_FORTRESS, vec![])), ["Flag"]);
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
            near: vec![],
            wield: None,
            sides: vec![],
            foes: vec![],
            latch: vec![],
        };
        assert!(!pays(&with(open)));
    }
}
