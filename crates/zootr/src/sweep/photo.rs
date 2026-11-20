//! `zootr photo`: a photo of each check as it stands, untouched, for the
//! ledger's drawer. Link warps beside the holder (a gift's or shop's
//! giver), faces it, and Hob says whether it's in view. The next side is
//! tried until one is, and the side that worked is kept for future sweeps.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

use super::clock::NOON;
use super::plan::{Planned, Steps, Trigger, preset};
use super::spot::{arrival, gone, presets, quiet, teleport};
use crate::dev::{Dev, Place, Request};
use crate::gamedata::GameData;
use crate::known::actor;
use crate::ledger::{self, Why};
use crate::live;
use crate::look::look_at;
use crate::nav::drive::Drive;
use crate::nav::{Level, kinds};
use crate::play::Input;
use crate::vision::Vision;
use crate::warp::{self, Placement, Side};

/// The hour a Gold Skulltula is shot at: they come out at night.
const NIGHT: u32 = 22;
/// How long the right stick raises the camera over a child afloat, in ms,
/// to look down into the water.
const LOOK_DOWN: u64 = 800;
/// The run's folder of every try's frame, `HASH-SIDE-STEP.png`.
const TRIES: &str = "tries";
/// A frame Hob scores this well ends a check's tries. Stopping at the first
/// such frame halved the tries of the vanilla pass's first 541 checks
/// (4.5 to 2.3 each); a later 5 was given up on 99 of them.
const GOOD: u32 = 4;
/// How far under an air landing a floor is looked for, in m.
const DROP: f32 = 12.0;
/// A holder this far over Link is looked up at, in m.
const HIGH: f32 = 2.0;
/// How long the right stick tilts the camera up at a high holder, in ms.
const LOOK_UP: u64 = 800;
/// How much farther each step back stands from the holder.
const STEP: f32 = 3.0;
/// Steps back tried after the first ring of sides.
const STEPS: u32 = 1;
/// How long the camera gets to swing behind Link after ZL.
const SETTLE: Duration = Duration::from_millis(800);

/// How the camera swings off Link's back before a shot, so he isn't
/// standing in front of the holder: the right stick held for a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    /// Around Link, in ms of the stick held left (right when negative).
    pub orbit: i64,
    /// Up over him, in ms of the stick held down (up when negative).
    pub raise: i64,
}

impl Swing {
    /// Each push it makes: the right stick's direction and for how long.
    fn pushes(self) -> impl Iterator<Item = (&'static str, u64)> {
        let push = |ms: i64, plus, minus| match ms {
            0 => None,
            ms if ms > 0 => Some((plus, ms.unsigned_abs())),
            ms => Some((minus, ms.unsigned_abs())),
        };
        [
            push(self.orbit, "RLeft", "RRight"),
            push(self.raise, "RDown", "RUp"),
        ]
        .into_iter()
        .flatten()
    }
}
/// How far below a landing the floor may be for it to count as floored.
const UNDERFOOT: f32 = 1.0;
/// The run's photo lines, one per check.
pub const LINES: &str = "photos.jsonl";

/// Where a check is photographed from.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    /// Its own placement, by hash.
    Placed(u64),
    /// The giver standing here: a gift's or shop's, as the sweep's planner
    /// found them.
    At([f32; 3]),
}

/// What a check's holder needs to be there, untouched: a `NoBake` chest
/// raised, a keeper out. The sweep's plan of it, without the use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prep {
    /// GameData values held through the load, as `NAME=VALUE`.
    pub ready: Vec<String>,
    /// Quest steps run through the quest system before the teleport.
    pub steps: Steps,
}

impl Prep {
    /// The prep of `p`: its presets, and the values held back for its foes
    /// (no foe is beaten here).
    fn of(p: &Planned) -> Self {
        let mut ready = presets(p);
        let mut steps = vec![];
        if let Trigger::Open {
            latch, steps: s, ..
        } = &p.trigger
        {
            ready.extend(latch.iter().cloned());
            steps.clone_from(s);
        }
        Self { ready, steps }
    }
}

/// One check to photograph.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub hash: u64,
    pub child: Option<bool>,
    /// Its map and spot, or none to stand beside.
    pub at: Option<(String, Spot)>,
    /// What Hob looks for: `chest`, `person`, `heart container`.
    pub what: String,
    pub prep: Prep,
}

/// The verdict of a photo the maintainer took by hand.
pub const BY_HAND: &str = "by hand";

/// What the pass decides about a check before standing anywhere.
struct Setup {
    map: String,
    holder: Placement,
    /// Each landing, by side and steps back, in the order tried.
    tries: Vec<((Side, u32), [f32; 3])>,
    hour: u32,
    line: Line,
}

/// What `zootr photo` does with its checks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Hob photographs each.
    Pass,
    /// Take Link to the one check, at a side and steps back if given.
    Go(Option<(Side, u32)>),
    /// Take the current picture as the one check's photo.
    Here,
}

/// One check's photo, as the ledger's `photos/index.json` keeps it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub hash: String,
    pub map: Option<String>,
    pub age: Option<String>,
    pub time: Option<String>,
    /// The side Link stood on when Hob saw the holder.
    pub side: Option<Side>,
    /// How many steps back from that side.
    pub step: u32,
    /// `framed`, `best effort` (no side passed, the first frame kept),
    /// `no spot` or [`BY_HAND`].
    pub verdict: String,
    /// The placement photographed in the check's place: a gift's giver.
    pub giver: Option<String>,
    pub at: String,
    pub build: Option<String>,
    pub seed: Option<String>,
}

impl Line {
    /// The placement this line photographed: its giver, or the check.
    fn subject(&self) -> Option<u64> {
        self.giver.as_ref().unwrap_or(&self.hash).parse().ok()
    }
}

impl std::fmt::Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let age = match self.child {
            Some(true) => "child",
            Some(false) => "adult",
            None => "either",
        };
        let (map, spot) = match &self.at {
            Some((map, Spot::Placed(h))) if *h == self.hash => (map.as_str(), "itself".into()),
            Some((map, Spot::Placed(h))) => (map.as_str(), format!("placed {h}")),
            Some((map, Spot::At(at))) => (map.as_str(), format!("giver at {at:?}")),
            None => ("-", "no spot".into()),
        };
        write!(f, "{}\t{map}\t{age}\t{spot}", self.hash)
    }
}

/// The checks to photograph, `area`'s alone or `hashes` alone when given,
/// grouped by map so each map loads once.
pub fn subjects(romfs: &Romfs, area: Option<&str>, hashes: &[u64]) -> Result<Vec<Subject>> {
    let (ledger, planned) = ledger::planned(romfs)?;
    let preps: HashMap<u64, Prep> = (planned.iter())
        .map(|p| (p.spot.hash, Prep::of(p)))
        .collect();
    let talks: HashMap<u64, (String, [f32; 3])> = (planned.into_iter())
        .filter_map(|p| match p.trigger {
            Trigger::Talk { map, face, .. } => Some((p.spot.hash, (map, face))),
            _ => None,
        })
        .collect();
    let givers = super::plan::givers(romfs)?;
    let rows = (ledger.rows.iter())
        .filter(|r| matches!(r.left_out, None | Some(Why::Eie { .. })))
        .filter(|r| area.is_none_or(|a| r.area == a))
        .filter(|r| hashes.is_empty() || hashes.contains(&r.hash));
    let mut poses = HashMap::new();
    let mut out = Vec::new();
    for r in rows {
        if !poses.contains_key(&r.area) {
            poses.insert(r.area.clone(), warp::poses_in(romfs, &r.area)?);
        }
        let child = first_age(r.logic.iter().map(|n| n.era.as_str()));
        let maps = (poses[&r.area].iter())
            .filter(|(_, placed)| placed.contains_key(&r.hash))
            .map(|(map, _)| map.as_str());
        let placed = map_for(maps, child).map(|m| (m.to_owned(), Spot::Placed(r.hash)));
        let at_giver = |(map, face): &(String, [f32; 3])| (map.clone(), Spot::At(*face));
        let talk = talks.get(&r.hash).map(at_giver);
        let at = match r.kind {
            "gift" => givers.get(&r.hash).map(at_giver).or(talk).or(placed),
            "shop" => talk.or(placed),
            _ => placed.or(talk),
        };
        let child = (at.as_ref().and_then(|(map, _)| warp::era(map))).or(child);
        let what = match &at {
            Some((_, Spot::At(_))) if !r.holder.contains("Cow") => "person".into(),
            _ => what(&r.holder, &r.name),
        };
        out.push(Subject {
            hash: r.hash,
            child,
            at,
            what,
            prep: preps.get(&r.hash).cloned().unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| {
        (&a.at.as_ref().map(|(m, _)| m), a.child).cmp(&(&b.at.as_ref().map(|(m, _)| m), b.child))
    });
    Ok(out)
}

/// The first age logic lets reach a check: child when both can.
fn first_age<'a>(eras: impl Iterator<Item = &'a str>) -> Option<bool> {
    let eras: HashSet<&str> = eras.collect();
    match (eras.contains("child"), eras.contains("adult")) {
        (true, _) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// The map of `maps` to shoot in as `child`: one of that age (child when
/// either), else one with no age, else any.
fn map_for<'a>(maps: impl Iterator<Item = &'a str>, child: Option<bool>) -> Option<&'a str> {
    let maps: Vec<&str> = maps.collect();
    let want = Some(child.unwrap_or(true));
    (maps.iter().find(|m| warp::era(m) == want))
        .or_else(|| maps.iter().find(|m| warp::era(m).is_none()))
        .or_else(|| maps.first())
        .copied()
}

/// Each try's side and steps back: every side at the usual distance, then
/// each again a step farther, for when nothing close shows the holder.
fn tries() -> Vec<(Side, u32)> {
    (0..=STEPS)
        .flat_map(|step| warp::sides_from(Side::Front).map(move |side| (side, step)))
        .collect()
}

/// How far round Link the pass beats enemies before a shot.
const CLEAR: f32 = 20.0;

/// Whether the pass beats the enemy named `name` before a shot: one a
/// quiet seed would hide, kept only for a chest's gate. Never a holder (a
/// Gold Skulltula, a scrub, a thief), whose death would spend its check.
fn foe(name: &str) -> bool {
    crate::enemies::quiet(name) && !nightly(name)
}

/// Whether `name` is a Gold Skulltula, which comes out only at night.
fn nightly(name: &str) -> bool {
    name.starts_with(&**actor::ENEMY_SKULLTULA_GOLD)
}

/// What Hob looks for in the frame of `holder`, the check named `name`:
/// its kind, or the item itself when it stands alone (a heart container).
fn what(holder: &str, name: &str) -> String {
    if nightly(holder) {
        "Gold Skulltula".into()
    } else if holder.contains("Cow") {
        "cow".into()
    } else {
        match kinds::word(holder) {
            Some("item") | None => name.to_lowercase(),
            Some(word) => word.into(),
        }
    }
}

/// The lines `dir` already holds, by check: a later line wins.
pub fn done(dir: &Path) -> HashMap<u64, Line> {
    read(&dir.join(LINES))
}

/// The lines of `file`, by check: a later line wins.
fn read(file: &Path) -> HashMap<u64, Line> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    (text.lines())
        .filter_map(|l| serde_json::from_str::<Line>(l).ok())
        .filter_map(|l| Some((l.hash.parse().ok()?, l)))
        .collect()
}

/// The side each holder in `file` was framed from, by check: a sweep
/// tries it first.
pub fn sides(file: &Path) -> HashMap<u64, Side> {
    (read(file).into_iter())
        .filter(|(_, l)| l.verdict == "framed")
        .filter_map(|(hash, l)| Some((hash, l.side?)))
        .collect()
}

/// A photo pass on the running game.
pub struct Pass<'a, I> {
    pub dev: &'a Dev,
    pub input: &'a mut I,
    /// Hob, for a pass. Going to a check or shooting by hand needs none.
    pub vision: Option<&'a Vision>,
    pub romfs: &'a Romfs,
    /// The game dump, for each map's navmesh.
    pub game: &'a Path,
    pub places: &'a BTreeSet<Place>,
    pub out: &'a Path,
    pub build: Option<String>,
    pub seed: Option<String>,
    pub swing: Swing,
    /// The seed brings Gold Skulltulas out by day: shoot them at noon.
    pub day_skulltulas: bool,
    levels: HashMap<String, Level>,
    /// Enemies of each map that never woke to a beating, not tried again.
    asleep: HashSet<(String, String)>,
    /// The line of each placement shot so far, for a shop's other slots.
    shot: HashMap<u64, Line>,
    /// The map, age, hour and held values Link is in.
    here: Option<(String, Option<bool>, u32, Vec<String>)>,
    /// GameData, for the gives that put gear on and the Gold Scale.
    gd: GameData,
}

impl<'a, I: Input> Pass<'a, I> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dev: &'a Dev,
        input: &'a mut I,
        vision: Option<&'a Vision>,
        romfs: &'a Romfs,
        game: &'a Path,
        places: &'a BTreeSet<Place>,
        out: &'a Path,
        (build, seed): (Option<String>, Option<String>),
    ) -> Result<Self> {
        let shot = (done(out).into_values())
            .filter(|l| l.verdict != "no spot")
            .filter_map(|l| Some((l.subject()?, l)))
            .collect();
        Ok(Self {
            dev,
            input,
            vision,
            romfs,
            game,
            places,
            out,
            build,
            seed,
            swing: Swing { orbit: 0, raise: 0 },
            levels: HashMap::new(),
            asleep: HashSet::new(),
            shot,
            here: None,
            gd: GameData::load(romfs)?,
            day_skulltulas: false,
        })
    }

    /// Photograph each of `subjects` not in the run's lines yet, stopping
    /// early once `stop` says so. A check whose setup fails gets no line,
    /// so the next run tries it again.
    pub fn run(&mut self, subjects: &[Subject], stop: impl Fn() -> bool) -> Result<()> {
        // Nothing left in the seed may wear Link down mid-run.
        let switches = self.dev.switches()?;
        self.dev.set_switches(crate::dev::Switches {
            health: true,
            ..switches
        })?;
        let done = done(self.out);
        let left: Vec<&Subject> = subjects
            .iter()
            .filter(|s| !done.contains_key(&s.hash))
            .collect();
        eprintln!(
            "{} checks, {} already shot",
            subjects.len(),
            subjects.len() - left.len()
        );
        for (i, s) in left.iter().enumerate() {
            if stop() {
                anyhow::bail!("the turn was lost");
            }
            match self.shoot(s) {
                Ok(line) => {
                    eprintln!("[{}/{}] {}: {}", i + 1, left.len(), s.hash, line.verdict);
                    self.append(&line)?;
                }
                Err(e) if gone(&e) => return Err(e),
                Err(e) => eprintln!("[{}/{}] {}: {e:#}", i + 1, left.len(), s.hash),
            }
        }
        Ok(())
    }

    fn append(&self, line: &Line) -> Result<()> {
        let path = self.out.join(LINES);
        let mut f = (std::fs::OpenOptions::new().create(true).append(true))
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        writeln!(f, "{}", serde_json::to_string(line)?)?;
        Ok(())
    }

    fn line(&self, s: &Subject, verdict: &str) -> Line {
        Line {
            hash: s.hash.to_string(),
            map: s.at.as_ref().map(|(m, _)| m.clone()),
            age: s.child.map(|c| if c { "child" } else { "adult" }.into()),
            verdict: verdict.into(),
            at: jiff::Timestamp::now().to_string(),
            build: self.build.clone(),
            seed: self.seed.clone(),
            ..Line::default()
        }
    }

    fn frame(&self, hash: u64) -> PathBuf {
        self.out.join(format!("{hash}.png"))
    }

    /// The navmesh and placements of `map`, loaded once.
    fn level(&mut self, map: &str) -> Result<&Level> {
        if !self.levels.contains_key(map) {
            let level = Level::of(self.romfs, self.game, map)?;
            self.levels.insert(map.to_owned(), level);
        }
        Ok(&self.levels[map])
    }

    /// What shooting `s` needs, or none when it has no spot to stand
    /// beside: its map, holder, landings in the order tried, and its line
    /// so far (hour, giver).
    fn setup(&mut self, s: &Subject) -> Result<Option<Setup>> {
        let Some((map, spot)) = &s.at else {
            return Ok(None);
        };
        let (holder, tries) = {
            let level = self.level(map)?;
            let holder: Placement = match spot {
                Spot::Placed(hash) => level.placement(*hash)?,
                Spot::At(face) => level.standing_at(*face)?,
            }
            .clone();
            let graph = level.graph();
            let pose = warp::facing(&holder.name, &holder.pose);
            // A landing in the air (a Skulltula over a door) drops to the
            // floor under it: warped there, Link fell onto the holder.
            let at = |&(side, step): &(Side, u32)| {
                let at = warp::landing_at(&pose, side, warp::AHEAD + step as f32 * STEP);
                match graph.floor(at, UNDERFOOT) {
                    Some(_) => at,
                    None => (graph.floor(at, DROP))
                        .filter(|f| f.height < at[1])
                        .map_or(at, |f| [at[0], f.height, at[2]]),
                }
            };
            let mut tries: Vec<_> = tries().into_iter().map(|t| (t, at(&t))).collect();
            // Each ring's floored landings first.
            tries.sort_by_key(|((_, step), at)| (*step, graph.floor(*at, UNDERFOOT).is_none()));
            (holder, tries)
        };
        let night = !self.day_skulltulas && nightly(&holder.name);
        let line = Line {
            time: Some(if night { "night" } else { "day" }.into()),
            giver: (holder.hash != Some(s.hash))
                .then(|| holder.hash.map(|h| h.to_string()))
                .flatten(),
            ..self.line(s, "best effort")
        };
        let hour = if night { NIGHT } else { NOON };
        Ok(Some(Setup {
            map: map.clone(),
            holder,
            tries,
            hour,
            line,
        }))
    }

    /// Photograph `s` from the side Hob scores best.
    fn shoot(&mut self, s: &Subject) -> Result<Line> {
        let Some(Setup {
            map,
            holder,
            tries,
            hour,
            line,
        }) = self.setup(s)?
        else {
            return Ok(self.line(s, "no spot"));
        };
        if let Some(earlier) = holder.hash.and_then(|h| self.shot.get(&h)) {
            std::fs::copy(self.frame(earlier.hash.parse()?), self.frame(s.hash))?;
            return Ok(Line {
                side: earlier.side,
                step: earlier.step,
                verdict: earlier.verdict.clone(),
                ..line
            });
        }
        self.arrive(&map, s.child, hour, &s.prep)?;
        let line = self.survey(s, &holder, tries, line)?;
        if let Some(h) = holder.hash.filter(|_| line.side.is_some()) {
            self.shot.insert(h, line.clone());
        }
        Ok(line)
    }

    /// Take Link to `s` as the pass would shoot it: its age, hour and prep,
    /// standing at `pick` (a side and steps back, where Hob's best frame
    /// was) or else the first landing tried. Says where he stands.
    pub fn go(&mut self, s: &Subject, pick: Option<(Side, u32)>) -> Result<String> {
        let Setup {
            map,
            holder,
            tries,
            hour,
            ..
        } = self
            .setup(s)?
            .context("this check has no spot to stand beside")?;
        let ((side, step), at) = landing(&tries, pick)?;
        self.arrive(&map, s.child, hour, &s.prep)?;
        self.stand(at, holder.pose.at)?;
        let side = format!("{side:?}").to_lowercase();
        Ok(format!("in {map}, {side} side, {step} steps back"))
    }

    /// Take the game's picture now as `s`'s photo, shot by hand: its line
    /// says [`BY_HAND`] and no side. Link isn't moved.
    pub fn here(&mut self, s: &Subject) -> Result<Line> {
        let line = by_hand(match self.setup(s)? {
            Some(setup) => setup.line,
            None => self.line(s, BY_HAND),
        });
        let frame = self.frame(s.hash);
        self.input.shot(&frame)?;
        self.append(&line)?;
        Ok(line)
    }

    /// Shoot `s` from each spot of the near ring, Hob scoring each frame,
    /// and keep the best as `HASH.png`. The far ring only when nothing near
    /// shows the holder. Each frame's answers go to `survey.jsonl`.
    fn survey(
        &mut self,
        s: &Subject,
        holder: &Placement,
        tries: Vec<((Side, u32), [f32; 3])>,
        mut line: Line,
    ) -> Result<Line> {
        let seen = format!(
            "Can you clearly see a {} in this picture? Say no if unsure or if it's only Link.",
            s.what
        );
        let score = format!(
            "From 1 to 5, how good a photo is this of the {}: 1 if it's hidden or tiny, 5 if it's big, clear and the room around it shows too? Answer with the number only?:",
            s.what
        );
        let questions = [seen.clone(), score.clone()];
        let vision = self.vision.context("a pass needs Hob's vision settings")?;
        let frame = self.frame(s.hash);
        // Every try's frame is kept, for the maintainer to pick from where
        // Hob's score passed a good view by.
        let tried = self.out.join(TRIES);
        std::fs::create_dir_all(&tried)?;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.out.join("survey.jsonl"))?;
        let mut best: Option<u32> = None;
        for ((side, step), at) in tries {
            if step > 0 && best.is_some_and(|p| p > 0) {
                break;
            }
            if self.stand(at, holder.pose.at).is_err() {
                continue;
            }
            let side_name = format!("{side:?}").to_lowercase();
            let trying = tried.join(format!("{}-{side_name}-{step}.png", s.hash));
            self.input.shot(&trying)?;
            let looked = look_at(vision, &trying, &questions)?;
            let yes = looked.answers[&seen].as_bool() == Some(true);
            let points = score_of(&looked.answers[&score]);
            let entry = serde_json::json!({
                "hash": s.hash.to_string(), "side": side_name, "step": step, "seen": yes, "score": points,
            });
            writeln!(log, "{entry}")?;
            let points = if yes { points.max(1) } else { 0 };
            if best.is_none_or(|p| points > p) {
                best = Some(points);
                std::fs::copy(&trying, &frame)?;
                (line.side, line.step) = (Some(side), step);
            }
            if points >= GOOD {
                break;
            }
        }
        line.verdict = match best {
            None => "no spot",
            Some(0) => "best effort",
            Some(_) => "framed",
        }
        .into();
        Ok(line)
    }

    /// Load `map` as `child` at `hour` with `prep` held, unless Link is
    /// there already.
    fn arrive(&mut self, map: &str, child: Option<bool>, hour: u32, prep: &Prep) -> Result<()> {
        let want = (map.to_owned(), child, hour, prep.ready.clone());
        if self.here.as_ref() == Some(&want) {
            return Ok(());
        }
        self.here = None;
        quiet(self.dev, self.input)?;
        for (challenge, step) in &prep.steps {
            self.dev.step(challenge, step)?;
        }
        // Before the teleport, which spawns who's out at that hour.
        self.dev.ask(&Request::Time(hour))?;
        let place =
            arrival(self.places, map).with_context(|| format!("no arrival point in {map}"))?;
        let dev = self.dev;
        let hold = || hold(dev, &prep.ready);
        hold()?;
        teleport(dev, self.input, place, child)?;
        dev.land_holding(map, hold)?;
        // The adult wears the Zora Tunic and Iron Boots all through a map,
        // so a warp into water sinks him to the holder rather than floating
        // him over it. On land they only slow a walk the pass never takes.
        if child == Some(false) {
            crate::gear::wear(dev, &self.gd, &mut *self.input, true)?;
        }
        self.here = Some(want);
        Ok(())
    }

    /// Beat each enemy near Link that a quiet seed keeps only for a gate,
    /// so none knocks him down, breaks a pot or stands in the frame. Its
    /// death raises its gate's chest, as the chest's own photo does.
    fn clear(&mut self) -> Result<()> {
        let map = self.dev.state()?.map;
        let names: BTreeSet<String> = (self.dev.foes(CLEAR)?.0.into_iter())
            .map(|f| f.name)
            .filter(|n| foe(n) && !self.asleep.contains(&(map.clone(), n.clone())))
            .collect();
        for name in names {
            let target = crate::dev::Target::Name(name.clone());
            match self.dev.slay(self.romfs, &target, CLEAR) {
                Ok(slain) => {
                    if !slain.0.iter().any(|s| s.fate == crate::dev::Fate::Died) {
                        self.asleep.insert((map.clone(), name));
                    }
                }
                Err(e) => eprintln!("  {name}: {e:#}"),
            }
        }
        quiet(self.dev, self.input)
    }

    /// Warp Link to `at`, beat the enemies round him, turn him to `face`,
    /// put the camera behind him, then swing it off his back.
    fn stand(&mut self, at: [f32; 3], face: [f32; 3]) -> Result<()> {
        quiet(self.dev, self.input)?;
        self.dev.warp(at)?;
        // The adult sinks in his boots. The child can't wear them, and a
        // held A didn't dive him on the native build (other machine,
        // 2026-11-19): he stays afloat and the camera looks down into the
        // water at the holder, clear enough to tell what's there.
        let mut afloat = false;
        if crate::nav::sink::floated(self.dev.link()?, at) {
            afloat = !(self.here.as_ref()).is_some_and(|(_, child, ..)| *child == Some(false));
            if !afloat {
                crate::nav::sink::land(self.dev)?;
            }
        }
        self.clear()?;
        Drive::new(self.dev, &mut *self.input).face(face)?;
        self.input.press("ZL")?;
        sleep(SETTLE);
        for (stick, ms) in self.swing.pushes() {
            self.input.hold(stick, ms)?;
        }
        if afloat {
            self.input.hold("RDown", LOOK_DOWN)?;
        } else if face[1] - at[1] > HIGH {
            self.input.hold("RUp", LOOK_UP)?;
        }
        sleep(SETTLE);
        Ok(())
    }
}

/// The landing at `pick` (a side and steps back) among `tries`, or the
/// first tried.
fn landing(
    tries: &[((Side, u32), [f32; 3])],
    pick: Option<(Side, u32)>,
) -> Result<((Side, u32), [f32; 3])> {
    (tries.iter().copied())
        .find(|(t, _)| pick.is_none_or(|p| *t == p))
        .context("no such side and step")
}

/// `line` as a photo shot by hand: [`BY_HAND`], with no side.
fn by_hand(line: Line) -> Line {
    Line {
        verdict: BY_HAND.into(),
        side: None,
        step: 0,
        ..line
    }
}

/// Write each `NAME=VALUE` of `entries`: a load clears some, so it's
/// written again all through one.
fn hold(dev: &Dev, entries: &[String]) -> Result<()> {
    for entry in entries {
        let (name, value) = preset(entry);
        live::set(dev.game(), dev.global(), live::hash(name)?, value)?;
    }
    Ok(())
}

/// Hob's 1 to 5 score, given as a number or in words, or 0 for none.
fn score_of(answer: &serde_json::Value) -> u32 {
    let text = answer.to_string();
    let digit = text.chars().find(char::is_ascii_digit);
    digit.and_then(|c| c.to_digit(10)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn going_stands_at_the_picked_side_else_the_first_tried() {
        let tries = [
            ((Side::Front, 0), [1.0, 0.0, 0.0]),
            ((Side::Left, 0), [2.0, 0.0, 0.0]),
            ((Side::Left, 1), [3.0, 0.0, 0.0]),
        ];
        assert_eq!(landing(&tries, None).unwrap().1, [1.0, 0.0, 0.0]);
        assert_eq!(
            landing(&tries, Some((Side::Left, 1))).unwrap().1,
            [3.0, 0.0, 0.0]
        );
        assert!(landing(&tries, Some((Side::Back, 0))).is_err());
    }

    #[test]
    fn a_hand_shot_says_so_and_has_no_side() {
        let line = Line {
            hash: "1".into(),
            map: Some("FakeHill_Child".into()),
            time: Some("night".into()),
            side: Some(Side::Left),
            step: 1,
            verdict: "best effort".into(),
            ..Line::default()
        };
        let hand = by_hand(line.clone());
        assert_eq!(
            (hand.verdict.as_str(), hand.side, hand.step),
            (BY_HAND, None, 0)
        );
        assert_eq!((hand.map, hand.time), (line.map, line.time));
    }

    #[test]
    fn a_score_reads_as_a_number_or_in_words() {
        use serde_json::json;
        let scores = [
            json!(4),
            json!("5"),
            json!(" 3."),
            json!(null),
            json!("none"),
        ];
        assert_eq!(scores.map(|s| score_of(&s)), [4, 5, 3, 0, 0]);
    }

    #[test]
    fn every_side_is_tried_then_each_a_step_back() {
        use Side::*;
        let t = tries();
        assert_eq!(t[..4], [(Front, 0), (Left, 0), (Right, 0), (Back, 0)]);
        assert_eq!(t[4..], [(Front, 1), (Left, 1), (Right, 1), (Back, 1)]);
    }

    #[test]
    fn the_first_age_is_child_when_both_reach_it() {
        assert_eq!(first_age(["adult", "child"].into_iter()), Some(true));
        assert_eq!(first_age(["adult"].into_iter()), Some(false));
        assert_eq!(first_age([].into_iter()), None);
    }

    #[test]
    fn the_map_is_the_ages_then_an_ageless_one() {
        let maps = ["FakeHill_Adult", "FakeHill_Child"];
        assert_eq!(
            map_for(maps.into_iter(), Some(false)),
            Some("FakeHill_Adult")
        );
        assert_eq!(map_for(maps.into_iter(), None), Some("FakeHill_Child"));
        assert_eq!(
            map_for(["FakeTemple"].into_iter(), Some(false)),
            Some("FakeTemple")
        );
        assert_eq!(map_for([].into_iter(), None), None);
    }

    #[test]
    fn a_run_resumes_from_its_lines_and_a_later_line_wins() {
        let dir = std::env::temp_dir().join(format!("zootr-photo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let line = |hash: &str, verdict: &str| Line {
            hash: hash.into(),
            verdict: verdict.into(),
            ..Line::default()
        };
        let text = [
            line("1", "best effort"),
            line("2", "framed"),
            line("1", "framed"),
        ]
        .map(|l| serde_json::to_string(&l).unwrap())
        .join("\n");
        std::fs::write(dir.join(LINES), text + "\nnot json\n").unwrap();
        let done = done(&dir);
        assert_eq!(done.len(), 2);
        assert_eq!(done[&1].verdict, "framed");
        let side = Line {
            hash: "3".into(),
            side: Some(Side::Left),
            verdict: "framed".into(),
            ..Line::default()
        };
        let text = serde_json::to_string(&side).unwrap();
        std::fs::write(dir.join("sides.jsonl"), text).unwrap();
        assert_eq!(sides(&dir.join("sides.jsonl")), [(3, Side::Left)].into());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_line_reads_as_the_ledgers_photo() {
        let line = Line {
            hash: "1".into(),
            side: Some(Side::Left),
            giver: Some("9".into()),
            ..Line::default()
        };
        let json = serde_json::to_value(&line).unwrap();
        assert_eq!(json["side"], "left");
        assert_eq!(line.subject(), Some(9));
    }

    #[test]
    fn a_swing_pushes_the_right_stick_each_way() {
        let pushes = |orbit, raise| Swing { orbit, raise }.pushes().collect::<Vec<_>>();
        assert_eq!(pushes(300, -100), [("RLeft", 300), ("RUp", 100)]);
        assert_eq!(pushes(-200, 0), [("RRight", 200)]);
    }

    #[test]
    fn the_pass_beats_only_enemies_no_check_holds() {
        assert!(foe("Enemy_FakeStalfos"));
        assert!(!foe(actor::ENEMY_SKULLTULA_GOLD));
        assert!(!foe(actor::ENEMY_SCRUB_BUSINESS));
        assert!(!foe(actor::OBJ_POT_A_02));
    }

    #[test]
    fn hob_looks_for_what_the_holder_is() {
        let gold = format!("{}_01", &**actor::ENEMY_SKULLTULA_GOLD);
        assert_eq!(what(&gold, "Gold Skulltula"), "Gold Skulltula");
        assert_eq!(what(actor::OBJ_T_BOX_A_01, "Small Key"), "chest");
        let heart = what(actor::OBJ_HEART_CONTAINER, "Heart container");
        assert_eq!(heart, "heart container");
    }

    #[test]
    fn a_shop_and_a_gift_are_shot_at_their_giver() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = subjects(&romfs, None, &[]).unwrap();
        let (ledger, _) = ledger::planned(&romfs).unwrap();
        let of_kind = |kind: &str| {
            let hashes: HashSet<u64> = (ledger.rows.iter())
                .filter(|r| r.kind == kind && r.left_out.is_none())
                .map(|r| r.hash)
                .collect();
            all.iter().filter(move |s| hashes.contains(&s.hash))
        };
        for kind in ["shop", "gift"] {
            let at = of_kind(kind)
                .filter(|s| matches!(s.at, Some((_, Spot::At(_)))))
                .count();
            assert!(at > 0, "no {kind} is shot at its giver");
        }
        let chests =
            of_kind("chest").filter(|s| matches!(s.at, Some((_, Spot::Placed(h))) if h == s.hash));
        assert!(chests.count() > 100);
    }
}
