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

use super::clock::NOON;
use super::plan::preset;
use super::spot::{arrival, gone, quiet, teleport};
use crate::dev::{Dev, Place, Request};
use crate::gamedata::GameData;
use crate::known::actor;
use crate::live;
use crate::look::look_at;
use crate::nav::Level;
use crate::nav::drive::Drive;
use crate::play::Input;
use crate::vision::Vision;
use crate::warp::{self, Placement, Side};

mod subject;
use subject::by_hand;
pub use subject::*;

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

/// Each try's side and steps back: every side at the usual distance, then
/// each again a step farther, for when nothing close shows the holder.
fn tries() -> Vec<(Side, u32)> {
    (0..=STEPS)
        .flat_map(|step| warp::sides_from(Side::Front).map(move |side| (side, step)))
        .collect()
}

/// How far round Link the pass beats enemies before a shot.
const CLEAR: f32 = 20.0;
/// How close a foe that never woke must be for a second try once the
/// camera has swung, in metres.
const NEAR: f32 = 8.0;
/// How many times a clear looks again for foes a death spawned.
const ROUNDS: usize = 3;

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
    /// The adult puts on the Zora Tunic and Iron Boots after each arrival,
    /// to sink to an underwater holder. Going to a check leaves his gear
    /// alone: the menu presses stick on the Mac.
    pub dress: bool,
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
            dress: true,
        })
    }

    /// Photograph each of `subjects` not in the run's lines yet, stopping
    /// early once `stop` says so. A check whose setup fails gets no line,
    /// so the next run tries it again.
    pub fn run(&mut self, subjects: &[Subject], stop: impl Fn() -> bool) -> Result<()> {
        self.endure()?;
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

    /// Turn infinite health on: nothing in the seed may wear Link down.
    fn endure(&self) -> Result<()> {
        let switches = self.dev.switches()?;
        self.dev.set_switches(crate::dev::Switches {
            health: true,
            ..switches
        })
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
        self.endure()?;
        self.dress = false;
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
        if self.dress && child == Some(false) {
            crate::gear::wear(dev, &self.gd, &mut *self.input, true)?;
        }
        self.here = Some(want);
        Ok(())
    }

    /// Beat each enemy near Link that a quiet seed keeps only for a gate,
    /// so none knocks him down, breaks a pot or stands in the frame. Its
    /// death raises its gate's chest, as the chest's own photo does.
    ///
    /// Looks again after each round, for what a death spawns (a
    /// Floormaster's three little hands). One that never woke is skipped
    /// for the rest of the map, as each try costs 10 s, unless `wake` and
    /// it's within [`NEAR`]: one that rises as Link arrives (a Stalfos) is
    /// up by the time the camera has swung.
    fn clear(&mut self, wake: bool) -> Result<()> {
        let map = self.dev.state()?.map;
        let mut tried = HashSet::new();
        for _ in 0..ROUNDS {
            let asleep: HashSet<&str> = (self.asleep.iter())
                .filter(|(m, _)| *m == map)
                .map(|(_, n)| n.as_str())
                .collect();
            let names = targets(&self.dev.foes(CLEAR)?.0, &asleep, &tried, wake);
            if names.is_empty() {
                break;
            }
            for name in names {
                tried.insert(name.clone());
                let target = crate::dev::Target::Name(name.clone());
                match self.dev.slay(self.romfs, &target, CLEAR) {
                    Ok(slain) if slain.0.iter().any(|s| s.fate == crate::dev::Fate::Died) => {
                        self.asleep.remove(&(map.clone(), name));
                    }
                    Ok(_) => {
                        self.asleep.insert((map.clone(), name));
                    }
                    Err(e) => eprintln!("  {name}: {e:#}"),
                }
            }
        }
        quiet(self.dev, self.input)
    }

    /// Warp Link to `at`, beat the enemies round him, turn him to `face`,
    /// put the camera behind him, then swing it off his back.
    fn stand(&mut self, at: [f32; 3], face: [f32; 3]) -> Result<()> {
        quiet(self.dev, self.input)?;
        self.dev.warp(at)?;
        // The adult sinks in his boots. The child can't wear them (and a
        // Link sent to a check by hand keeps his own gear), and a held A
        // didn't dive him on the native build (other machine,
        // 2026-11-19): he stays afloat and the camera looks down into the
        // water at the holder, clear enough to tell what's there.
        let mut afloat = false;
        if crate::nav::sink::floated(self.dev.link()?, at) {
            let adult = (self.here.as_ref()).is_some_and(|(_, child, ..)| *child == Some(false));
            afloat = !(adult && self.dress);
            if !afloat {
                crate::nav::sink::land(self.dev)?;
            }
        }
        self.clear(false)?;
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
        self.clear(true)?;
        sleep(SETTLE);
        Ok(())
    }
}

/// The foes in `foes` for [`Pass::clear`] to slay this round: each quiet
/// kind not `tried` yet this clear, and not `asleep` in the map unless
/// `wake` and it's within [`NEAR`].
fn targets(
    foes: &[crate::dev::Foe],
    asleep: &HashSet<&str>,
    tried: &HashSet<String>,
    wake: bool,
) -> BTreeSet<String> {
    (foes.iter())
        .filter(|f| foe(&f.name) && !tried.contains(&f.name))
        .filter(|f| !asleep.contains(f.name.as_str()) || (wake && f.distance <= NEAR))
        .map(|f| f.name.clone())
        .collect()
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
    fn a_clear_slays_what_a_death_spawns_and_wakes_only_near_sleepers() {
        let f = |name: &str, distance| crate::dev::Foe {
            name: name.into(),
            distance,
            left: 1,
            most: 1,
        };
        let (near, far) = ("Enemy_FakeStalfos", "Enemy_FakeKeese");
        let foes = [f(near, 3.0), f(far, 15.0), f("Enemy_FakeHand_Little", 2.0)];
        let asleep: HashSet<&str> = [near, far].into();
        let tried: HashSet<String> = HashSet::new();
        let names = |wake| {
            targets(&foes, &asleep, &tried, wake)
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert_eq!(names(false), ["Enemy_FakeHand_Little"]);
        assert_eq!(names(true), ["Enemy_FakeHand_Little", near]);
        let tried: HashSet<String> = ["Enemy_FakeHand_Little".to_string()].into();
        assert!(targets(&foes, &asleep, &tried, false).is_empty());
    }

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
}
