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
    /// `framed`, `best effort` (no side passed, the first frame kept) or
    /// `no spot`.
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
    pub vision: &'a Vision,
    pub romfs: &'a Romfs,
    /// The game dump, for each map's navmesh.
    pub game: &'a Path,
    pub places: &'a BTreeSet<Place>,
    pub out: &'a Path,
    pub build: Option<String>,
    pub seed: Option<String>,
    pub swing: Swing,
    levels: HashMap<String, Level>,
    /// Enemies of each map that never woke to a beating, not tried again.
    asleep: HashSet<(String, String)>,
    /// The line of each placement shot so far, for a shop's other slots.
    shot: HashMap<u64, Line>,
    /// The map, age, hour and held values Link is in.
    here: Option<(String, Option<bool>, u32, Vec<String>)>,
}

impl<'a, I: Input> Pass<'a, I> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dev: &'a Dev,
        input: &'a mut I,
        vision: &'a Vision,
        romfs: &'a Romfs,
        game: &'a Path,
        places: &'a BTreeSet<Place>,
        out: &'a Path,
        (build, seed): (Option<String>, Option<String>),
    ) -> Self {
        let shot = (done(out).into_values())
            .filter(|l| l.verdict != "no spot")
            .filter_map(|l| Some((l.subject()?, l)))
            .collect();
        Self {
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
        }
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

    /// Photograph `s` from the side Hob scores best.
    fn shoot(&mut self, s: &Subject) -> Result<Line> {
        let Some((map, spot)) = &s.at else {
            return Ok(self.line(s, "no spot"));
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
            let at = |&(side, step): &(Side, u32)| {
                warp::landing_at(&pose, side, warp::AHEAD + step as f32 * STEP)
            };
            let mut tries: Vec<_> = tries().into_iter().map(|t| (t, at(&t))).collect();
            // Each ring's floored landings first.
            tries.sort_by_key(|((_, step), at)| (*step, graph.floor(*at, UNDERFOOT).is_none()));
            (holder, tries)
        };
        let night = nightly(&holder.name);
        let line = Line {
            time: Some(if night { "night" } else { "day" }.into()),
            giver: (holder.hash != Some(s.hash))
                .then(|| holder.hash.map(|h| h.to_string()))
                .flatten(),
            ..self.line(s, "best effort")
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
        self.arrive(map, s.child, if night { NIGHT } else { NOON }, &s.prep)?;
        let line = self.survey(s, &holder, tries, line)?;
        if let Some(h) = holder.hash.filter(|_| line.side.is_some()) {
            self.shot.insert(h, line.clone());
        }
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
        let (frame, trying) = (
            self.frame(s.hash),
            self.out.join(format!("{}-try.png", s.hash)),
        );
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
            self.input.shot(&trying)?;
            let looked = look_at(self.vision, &trying, &questions)?;
            let yes = looked.answers[&seen].as_bool() == Some(true);
            let points = score_of(&looked.answers[&score]);
            let side_name = format!("{side:?}").to_lowercase();
            let entry = serde_json::json!({
                "hash": s.hash.to_string(), "side": side_name, "step": step, "seen": yes, "score": points,
            });
            writeln!(log, "{entry}")?;
            let points = if yes { points.max(1) } else { 0 };
            if best.is_none_or(|p| points > p) {
                best = Some(points);
                std::fs::rename(&trying, &frame)?;
                (line.side, line.step) = (Some(side), step);
            }
        }
        let _ = std::fs::remove_file(&trying);
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
        self.clear()?;
        Drive::new(self.dev, &mut *self.input).face(face)?;
        self.input.press("ZL")?;
        sleep(SETTLE);
        for (stick, ms) in self.swing.pushes() {
            self.input.hold(stick, ms)?;
        }
        sleep(SETTLE);
        Ok(())
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
