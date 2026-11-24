//! A night of rounds on a target with a bridge: shuffle a mix, install it, start
//! the game, sweep every spot, append the results, and go again.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;
use serde_json::json;

use super::judge::Judge;
use super::plan::{Planned, Trigger, plan};
use super::resume::{Resume, Yielding};
use super::spot::{Sweeper, slug};
use super::verdict::Verdict;
use crate::config::Target;
use crate::dev::{self, Dev};
use crate::gamedata::GameData;
use crate::generate::{self, Extras};
use crate::known::{Name, actor};
use crate::live::{self, Game};
use crate::play::{Input, Start, boot};
use crate::progress::Run;
use crate::savefile;
use crate::settings::{self, Settings};
use crate::starting;

mod known;

pub(crate) use known::holder_kind;
use known::order;
pub(super) use known::{Known, at_known, learn};

const LOOK: Duration = Duration::from_secs(2);
/// How long an event may hold off a give.
const CALM: Duration = Duration::from_secs(60);
/// Heart containers given at each start, for 20 hearts: the harness stands
/// still beside enemies while it presses through a spot and the judge
/// thinks. The health switch refills them, and 20 outlast any one hit.
const CONTAINERS: usize = 17;
/// Given at each start, for adult spots: the saved file is a child's with
/// the Kokiri Sword only, and adult Link swung nothing at a pot (Mac,
/// 2026-11-14).
const ADULT_SWORD: &Name = actor::WEAPON_SWORD_MASTER;
/// The dev switches held at each start: hearts and magic never run out.
/// Ammo and rupees stay real, so a pickup still shows in their counts.
const SWITCHES: dev::Switches = dev::Switches {
    health: true,
    magic: true,
    resources: false,
};
/// Spots in a row the harness may fail to set up before the game restarts:
/// a menu left open refuses every request after it.
const FAILS: usize = 2;
/// Each night folder's rows, one JSON line a spot.
pub(super) const RESULTS: &str = "results.jsonl";

/// What every round's seed has: the test seed rules, and no enemies in
/// the harness's way.
const BASE: [&str; 4] = [
    settings::SKIP_ALL,
    settings::QUIET_ENEMIES,
    "open-forest",
    "kakariko-gate",
];

/// What every round's seed starts with, by the test seed rules.
const START: [&str; 5] = [
    "kokiri-sword",
    "deku-shield",
    "slingshot",
    "bomb-bag",
    "fairy-ocarina",
];

/// The settings mixes, one a round in turn, so a failure names its mix.
pub const MIXES: [(&str, &[&str]); 4] = [
    ("chests", &[]),
    (
        "songs and gifts",
        &[
            settings::SONGS,
            settings::NPC_GIFTS,
            settings::DUNGEON_REWARDS,
        ],
    ),
    ("scrubs and shops", &[settings::SCRUBS, settings::SHOPS]),
    (
        "everything",
        &[
            settings::SONGS,
            settings::NPC_GIFTS,
            settings::DUNGEON_REWARDS,
            settings::SCRUBS,
            settings::SHOPS,
            settings::MAPS_COMPASSES,
            settings::FREESTANDING,
            settings::SKULLTULAS,
            settings::BOSS_HEARTS,
            settings::POTS,
            settings::CRATES,
            settings::ROCKS,
            settings::GRASS,
            settings::HIVES,
            settings::COWS,
            settings::MERCHANTS,
            "keys=start",
            "fortress-keys=start",
        ],
    ),
];

/// The settings of mix `i`. An entry is a toggle's id, or `ID=VALUE` for a
/// choice, as `--setting` takes it.
pub fn mix(i: usize) -> (&'static str, Settings) {
    let (name, extra) = MIXES[i % MIXES.len()];
    let set = |entry: &&str| match entry.split_once('=') {
        Some((id, value)) => (id.into(), value.into()),
        None => (entry.to_string(), true.into()),
    };
    let mut settings: Settings = BASE.iter().chain(extra).map(set).collect();
    let start = starting::Start::of(&START);
    let start = serde_json::to_value(start).expect("a map");
    settings.insert(starting::SETTING.into(), start);
    (name, settings)
}

/// The place in [`MIXES`] of the mix whose name starts with `name`.
pub fn pinned(name: &str) -> Result<usize> {
    let names = || MIXES.iter().map(|(n, _)| *n);
    names().position(|n| n.starts_with(name)).with_context(|| {
        format!(
            "no mix named {name}: {}",
            names().collect::<Vec<_>>().join(", ")
        )
    })
}

/// GameData's play time in seconds, a UInt known only by its hash.
const PLAY_TIME: u32 = 0xe573f564;
/// The most play a start adds to the save it loads: 8 s measured (86 over a
/// pristine 78), against 52 s for a save that had one sweep spot played on
/// it (the other machine, 2026-11-17).
const START_PLAY: u32 = 30;

/// The most play time any save under `dir` holds, `None` with no save.
fn played(dir: &Path) -> Result<Option<u32>> {
    let mut most = None;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let time = if path.is_dir() {
            played(&path)?
        } else if path.file_name().is_some_and(|n| n == "progress.sav") {
            let save = savefile::load(&path)?;
            save.get(PLAY_TIME).and_then(|e| save.scalar(e))
        } else {
            None
        };
        most = most.max(time);
    }
    Ok(most)
}

/// Shuffle `seed` with `settings` into `mod_dir` and plan each spot, the
/// ones in a map without one of `places` skipped.
pub fn pool(
    romfs: &Romfs,
    mod_dir: &Path,
    seed: u64,
    settings: &Settings,
    places: &BTreeSet<dev::Place>,
) -> Result<Vec<Planned>> {
    let report = generate::generate(
        romfs,
        mod_dir,
        seed,
        settings,
        &Extras::default(),
        &Run::silent(),
    )?;
    let mut planned = plan(romfs, &report.spots)?;
    skip_unreachable(&mut planned, places);
    Ok(planned)
}

/// One night.
pub struct Night<'a, I, J> {
    pub romfs: &'a Romfs,
    /// The game dump, for each map's navmesh.
    pub game: &'a Path,
    pub gd: &'a GameData,
    pub target: &'a Target,
    pub input: I,
    pub judge: J,
    /// The night's folder: `results.jsonl` and a folder of shots a round.
    pub dir: PathBuf,
    /// A save folder copied over the target's before each start, so every
    /// start is the same game.
    pub pristine: Option<PathBuf>,
    /// Spots a round at most, for a short run. `None` for all.
    pub limit: Option<usize>,
    /// The mix every round sweeps, by its place in [`MIXES`]. `None` for
    /// each in turn.
    pub mix: Option<usize>,
    /// Only the spots [`picked`] by this text. `None` for all.
    pub only: Option<String>,
    /// The seed every round shuffles. `None` for a new one each round.
    pub seed: Option<u64>,
    /// The turn queue to step aside for between spots. `None` sweeps on
    /// regardless.
    pub yielding: Option<Yielding>,
    /// A photo pass's `photos.jsonl`: the side each holder was seen from,
    /// tried first.
    pub sides: Option<PathBuf>,
}

/// How a round ended.
#[derive(PartialEq)]
enum Ended {
    Swept,
    /// Stepped aside for someone waiting for the game.
    Yielded,
}

impl<I: Input, J: Judge> Night<'_, I, J> {
    /// `rounds` rounds, or no end with 0.
    pub fn run(&mut self, rounds: usize) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let places = dev::places(self.romfs)?;
        let rows = std::fs::read_to_string(self.results()).unwrap_or_default();
        let mut resume = Resume::load(&self.dir);
        let first = resume
            .as_ref()
            .map_or_else(|| next_round(&rows), |r| r.round);
        if let Some(y) = &self.yielding {
            y.start();
        }
        for round in (first..).take_while(|&r| rounds == 0 || r < first + rounds) {
            match self.round(round, &places, resume.take()) {
                Ok(Ended::Yielded) => break,
                Ok(Ended::Swept) => Resume::clear(&self.dir),
                Err(e) => {
                    Resume::clear(&self.dir);
                    eprintln!("round {round} ended early: {e:#}");
                    // Logged, not raised: the queue still hears the sweep stopped.
                    if let Err(e) = self.note(round, &format!("{e:#}")) {
                        eprintln!("noting round {round}'s error: {e:#}");
                    }
                }
            }
        }
        // Two checks: `stop` borrows the night mutably.
        if self.yielding.is_some() {
            self.stop();
        }
        if let Some(y) = &self.yielding {
            y.done();
        }
        Ok(())
    }

    /// Round `round`, or the rest of it as `resume` left it.
    fn round(
        &mut self,
        round: usize,
        places: &BTreeSet<dev::Place>,
        resume: Option<Resume>,
    ) -> Result<Ended> {
        let (mix, settings) = mix(self.mix.unwrap_or(round));
        let seed = (resume.as_ref().map(|r| r.seed).or(self.seed))
            .unwrap_or_else(|| generate::time_seed() % 1_000_000);
        let mut resume = resume.unwrap_or(Resume {
            round,
            seed,
            done: Vec::new(),
        });
        match resume.done.len() {
            0 => eprintln!("round {round}: {mix}, seed {seed}"),
            n => eprintln!("round {round}: {mix}, seed {seed}, resumed after {n} spots"),
        }
        // Before quitting the game or installing over it: both are the
        // holder's if someone else has the game.
        if self.step_aside(&resume)? {
            return Ok(Ended::Yielded);
        }
        self.stop();
        let mut planned = pool(self.romfs, &self.target.mod_dir, seed, &settings, places)?;
        planned.retain(|p| self.only.as_deref().is_none_or(|text| picked(p, text)));
        let known = self.known();
        order(&mut planned, &known, seed);
        left(&mut planned, &resume, self.limit, name);
        let shots = self.dir.join(format!("round-{round:03}-{seed}"));
        std::fs::create_dir_all(&shots)?;
        if self.step_aside(&resume)? {
            return Ok(Ended::Yielded);
        }
        let mut live = self.start()?;
        let rows = self.results();
        let at = Round {
            round,
            seed,
            mix,
            places,
            shots: &shots,
            rows: &rows,
        };
        for p in &planned {
            if self.step_aside(&resume)? {
                return Ok(Ended::Yielded);
            }
            let first = at_known(p, &known);
            let named = name(p);
            self.attempt(&mut live, &at, p, &first, &named, &json!({}))?;
            resume.done.push(named);
        }
        Ok(Ended::Swept)
    }

    /// Whether the turn queue wants the game back: if so, keep the
    /// round's place in `resume.json`.
    fn step_aside(&self, resume: &Resume) -> Result<bool> {
        if !self.yielding.as_ref().is_some_and(Yielding::asked) {
            return Ok(false);
        }
        let done = resume.done.len();
        eprintln!("round {}: stepping aside after {done} spots", resume.round);
        resume.save(&self.dir)?;
        Ok(true)
    }

    /// Sweep `first`, the spot `p` as earlier nights left it: again after a
    /// restart when the game never took its teleport, and again as the
    /// other age when `p`'s map names none. Restarts the game after a spot
    /// that broke it, or after [`FAILS`] setups in a row failed. `tag` adds
    /// its fields to each row.
    pub(super) fn attempt(
        &mut self,
        live: &mut Live,
        at: &Round,
        p: &Planned,
        first: &Planned,
        name: &str,
        tag: &serde_json::Value,
    ) -> Result<()> {
        let mut verdict = self.spot(&live.dev, at, first, name, tag)?;
        if stuck(&verdict) {
            *live = self.start()?;
            let again = format!("{name} (after a restart)");
            verdict = self.spot(&live.dev, at, first, &again, tag)?;
        }
        // The age is read only for a second try: after a crash the game is
        // gone, and the restart below must still run.
        let child = || live.dev.state().ok().map(|s| s.child);
        if let Some(again) = other_age(p, &verdict, child) {
            let again = Planned {
                side: first.side,
                ..again
            };
            let age = if again.child == Some(true) {
                "child"
            } else {
                "adult"
            };
            let named = format!("{name} (as {age})");
            verdict = self.spot(&live.dev, at, &again, &named, tag)?;
        }
        live.fails = match verdict {
            Verdict::SetupFailed(_) => live.fails + 1,
            _ => 0,
        };
        if verdict.broke() || live.fails >= FAILS {
            *live = self.start()?;
        }
        Ok(())
    }

    /// Sweep one spot and record its row.
    fn spot(
        &mut self,
        dev: &Dev,
        at: &Round,
        p: &Planned,
        name: &str,
        tag: &serde_json::Value,
    ) -> Result<Verdict> {
        let started = Instant::now();
        let swept = Sweeper {
            dev,
            romfs: self.romfs,
            game: self.game,
            gd: self.gd,
            input: &mut self.input,
            judge: &mut self.judge,
            places: at.places,
            dir: at.shots,
        }
        .sweep(p, name);
        let mut row = json!({
            "round": at.round, "seed": at.seed, "mix": at.mix, "name": name,
            "spot": p.spot, "trigger": kind(&p.trigger), "child": p.child,
            "secs": started.elapsed().as_secs_f32(), "swept": swept,
        });
        if let (Some(row), Some(tag)) = (row.as_object_mut(), tag.as_object()) {
            row.extend(tag.clone());
        }
        append(at.rows, &row)?;
        let verdict = swept.verdict;
        eprintln!("  {name}: {}", serde_json::to_string(&verdict)?);
        Ok(verdict)
    }

    /// Quit the game if it runs.
    fn stop(&mut self) {
        if self.input.running() {
            let _ = self.input.quit();
            sleep(LOOK * 2);
        }
    }

    /// Start the game from the pristine save and play into it.
    pub(super) fn start(&mut self) -> Result<Live> {
        self.stop();
        let save = match &self.pristine {
            Some(p) => Some(boot::prepare(&Start::Save(p.clone()), "sweep")?),
            None => None,
        };
        boot::boot(&mut self.input, self.target, save.as_deref(), boot::LIMIT)?;
        let dev = Dev::new(Game::attach(self.target)?)?;
        if let Some(pristine) = &self.pristine {
            self.loaded(&dev, pristine)?;
        }
        dev.set_switches(SWITCHES)?;
        for _ in 0..CONTAINERS {
            self.give(&dev, actor::OBJ_HEART_CONTAINER)?;
        }
        self.give(&dev, ADULT_SWORD)?;
        Ok(Live { dev, fails: 0 })
    }

    /// Refuse a game that didn't load `pristine`: one played longer than
    /// any of its slots plus a start. A game launched without the data
    /// folder its target's save folder names reads another folder's save,
    /// which keeps every chest an earlier run opened (2026-11-17).
    fn loaded(&self, dev: &Dev, pristine: &Path) -> Result<()> {
        let Some(saved) = played(pristine)? else {
            return Ok(());
        };
        let now = live::read(dev.game(), self.gd, dev.global(), PLAY_TIME)?;
        let now: u32 = now.parse().with_context(|| format!("play time {now}"))?;
        ensure!(
            now <= saved + START_PLAY,
            "the game loaded a save played {now} s, the pristine one {saved} s: \
             is {} the save folder it reads?",
            (self.target.save_dir.as_deref()).map_or("nothing".into(), |d| d.display().to_string())
        );
        Ok(())
    }

    /// One `actor`, pressing A through any event that holds it off: a new
    /// file's opening talk begins a moment after play does.
    fn give(&mut self, dev: &Dev, actor: &str) -> Result<()> {
        let start = Instant::now();
        loop {
            match dev.give(self.gd, actor, 1) {
                Ok(()) => return Ok(()),
                Err(_) if start.elapsed() < CALM => {
                    self.input.press("A")?;
                    sleep(LOOK);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn results(&self) -> PathBuf {
        self.dir.join(RESULTS)
    }

    /// What the rows of every night beside this one, oldest first, tell of
    /// each spot.
    fn known(&self) -> Known {
        let mut nights: Vec<PathBuf> = (self.dir.parent())
            .and_then(|all| std::fs::read_dir(all).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|night| night.path().join(RESULTS))
            .collect();
        nights.sort();
        let mut known = Known::default();
        for rows in nights
            .iter()
            .filter_map(|f| std::fs::read_to_string(f).ok())
        {
            learn(&mut known, &rows);
        }
        if let Some(file) = &self.sides {
            known.seen = super::photo::sides(file);
        }
        known
    }

    fn note(&self, round: usize, error: &str) -> Result<()> {
        append(&self.results(), &json!({"round": round, "error": error}))
    }
}

/// Add `row` to the JSON lines file `rows`.
fn append(rows: &Path, row: &serde_json::Value) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(rows)?;
    writeln!(file, "{row}")?;
    Ok(())
}

/// The game as a round plays it, and the setups in a row that failed.
pub(super) struct Live {
    dev: Dev,
    fails: usize,
}

/// The round after the last one `results` records, so a night restarted
/// after a deploy goes on through the mixes instead of starting over.
pub(super) fn next_round(results: &str) -> usize {
    (results.lines())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()?["round"].as_u64())
        .max()
        .map_or(0, |r| r as usize + 1)
}

/// Whether a spot failed because the game took a teleport and never moved,
/// or never took it: only a restart gets it out, so the spot goes again
/// after one.
fn stuck(verdict: &Verdict) -> bool {
    let stuck = |why: &str| {
        [dev::NEVER_LANDED, dev::NOT_TAKEN]
            .iter()
            .any(|s| why.contains(s))
    };
    matches!(verdict, Verdict::SetupFailed(why) if stuck(why))
}

/// The round a spot is swept in, for its row.
pub(super) struct Round<'a> {
    pub round: usize,
    pub seed: u64,
    pub mix: &'static str,
    pub places: &'a BTreeSet<dev::Place>,
    pub shots: &'a Path,
    /// The JSON lines file each spot's row goes to.
    pub rows: &'a Path,
}

/// The spot again as the other age, when it came to nothing in a map that
/// names no age. Logic lists most dungeon checks for both ages, so only a
/// try tells which one a room needs. `child` reads Link's age when it
/// ended, only when a second try is due: none when the game can't say.
fn other_age(
    p: &Planned,
    verdict: &Verdict,
    child: impl FnOnce() -> Option<bool>,
) -> Option<Planned> {
    let tried = matches!(verdict, Verdict::Nothing | Verdict::SetupFailed(_)) && !stuck(verdict);
    let m = map(p);
    if !(tried && p.child.is_none() && !m.is_empty() && crate::warp::era(&m).is_none()) {
        return None;
    }
    Some(Planned {
        child: Some(!child()?),
        ..p.clone()
    })
}

/// Keep the round's spots `resume` didn't sweep yet, within `limit` spots
/// all told, its done ones counted.
fn left<T>(
    planned: &mut Vec<T>,
    resume: &Resume,
    limit: Option<usize>,
    name: impl Fn(&T) -> String,
) {
    resume.skip(planned, name);
    planned.truncate(limit.map_or(usize::MAX, |l| l.saturating_sub(resume.done.len())));
}

/// Skip the spots in a map with no arrival point, which a warp can't reach,
/// so a short run spends its spots on ones it can sweep.
fn skip_unreachable(planned: &mut [Planned], places: &BTreeSet<dev::Place>) {
    for p in planned {
        let m = map(p);
        if !m.is_empty() && !places.iter().any(|pl| pl.map == m) {
            p.trigger = Trigger::Skip(format!("no arrival point in {m}"));
        }
    }
}

/// Whether `--only text` keeps `p`: its name, holder or the seed's item
/// there holds each of `text`'s comma-separated parts.
fn picked(p: &Planned, text: &str) -> bool {
    let fields = [name(p), p.spot.holder.clone(), p.spot.item.clone()];
    text.split(',')
        .all(|part| fields.iter().any(|f| f.contains(part)))
}

/// The map a spot is swept in, for sweeping a map's spots together.
fn map(p: &Planned) -> String {
    p.trigger.map().unwrap_or_default().to_owned()
}

/// How the sweep triggers a spot, or why it skips it.
pub(crate) fn how(t: &Trigger) -> String {
    match t {
        Trigger::Skip(why) => format!("skip: {why}"),
        t => kind(t).into(),
    }
}

pub(crate) fn kind(t: &Trigger) -> &'static str {
    match t {
        Trigger::Open { .. } => "open",
        Trigger::Talk { .. } => "talk",
        Trigger::Play { .. } => "play",
        Trigger::Skip(_) => "skip",
    }
}

/// The spot as the spoiler names it.
pub(crate) fn name(p: &Planned) -> String {
    let s = &p.spot;
    let what = [&s.place, &s.chest_name, &s.holder]
        .into_iter()
        .find(|t| !t.is_empty())
        .cloned()
        .unwrap_or_default();
    let name = format!("{} {what}", s.area_name).trim().to_owned();
    if slug(&name).is_empty() {
        format!("{:x}", s.hash)
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, item, map};

    #[test]
    fn every_mix_entry_is_a_setting_the_cli_takes() {
        let entries = MIXES
            .iter()
            .flat_map(|(_, extra)| BASE.iter().chain(*extra));
        for entry in entries {
            let arg = match entry.contains('=') {
                true => entry.to_string(),
                false => format!("{entry}=true"),
            };
            crate::cli::args::setting(&arg).unwrap();
        }
    }

    #[test]
    fn only_keeps_the_spots_whose_name_holder_or_item_holds_each_part() {
        needs_names!();
        let mut cow = Planned::open(flag::KAKARIKO_VILLAGE_CHILD);
        cow.spot.area_name = "Kakariko Village".into();
        cow.spot.chest_name = "Lon Lon Milk (Cow)".into();
        cow.spot.holder = "Cow".into();
        let mut pot = Planned::open(map::KOKIRI_FOREST_CHILD);
        pot.spot.holder = actor::OBJ_POT_A_01.to_string();
        assert!(picked(&cow, "Cow") && picked(&cow, "Kakariko"));
        assert!(picked(&pot, "Pot") && !picked(&pot, "Cow"));
        pot.spot.item = item::MELODY_SUN_SONG.to_string();
        assert!(picked(&pot, "Pot,Melody") && !picked(&pot, "Pot,Bow"));
    }

    #[test]
    fn every_mix_keeps_the_test_seed_rules() {
        for i in 0..MIXES.len() {
            let (_, s) = mix(i);
            assert!(BASE.iter().all(|id| s.contains_key(*id)), "mix {i}");
        }
        assert_eq!(mix(0).0, mix(MIXES.len()).0, "the mixes go round");
        assert!(mix(3).1.contains_key(settings::SCRUBS));
        assert_eq!(pinned("everything").unwrap(), 3);
        assert!(pinned("nothing").is_err());
    }

    #[test]
    fn a_dungeon_spot_that_came_to_nothing_goes_again_as_the_other_age() {
        needs_names!();
        let child = || Some(true);
        let again = other_age(
            &Planned::open(flag::SPIRIT_TEMPLE),
            &Verdict::Nothing,
            child,
        );
        assert_eq!(again.map(|p| p.child), Some(Some(false)));
        let failed = Verdict::SetupFailed("x".into());
        assert!(other_age(&Planned::open(flag::SPIRIT_TEMPLE), &failed, || Some(false)).is_some());
        assert!(other_age(&Planned::open(flag::SPIRIT_TEMPLE), &Verdict::Pass, child).is_none());
        assert!(
            other_age(
                &Planned::open(map::KOKIRI_FOREST_CHILD),
                &Verdict::Nothing,
                child
            )
            .is_none(),
            "its map names its age"
        );
        assert!(
            other_age(
                &Planned::open(flag::SPIRIT_TEMPLE),
                &Verdict::Nothing,
                || None
            )
            .is_none(),
            "the game can't say Link's age"
        );
        let tried = Planned {
            child: Some(false),
            ..Planned::open(flag::SPIRIT_TEMPLE)
        };
        assert!(
            other_age(&tried, &Verdict::Nothing, || Some(false)).is_none(),
            "once"
        );
    }

    #[test]
    fn a_teleport_that_never_lands_restarts_the_game_not_the_age() {
        let never = Verdict::SetupFailed(format!("{}: still busy after 30s", dev::NEVER_LANDED));
        assert!(stuck(&never));
        let ignored = format!("{} to {}/Start", dev::NOT_TAKEN, map::GERUDO_FORTRESS_ADULT);
        assert!(stuck(&Verdict::SetupFailed(ignored)));
        assert!(!stuck(&Verdict::SetupFailed("no arrival point".into())));
        let unread = || -> Option<bool> { panic!("no second try, so no age read") };
        assert!(other_age(&Planned::open(flag::SPIRIT_TEMPLE), &never, unread).is_none());
    }

    #[test]
    fn a_crash_restarts_the_game_without_reading_its_age() {
        needs_names!();
        let crash = Verdict::Crash(live::CLOSED.into());
        let unread = || -> Option<bool> { panic!("the game is gone") };
        assert!(other_age(&Planned::open(flag::SPIRIT_TEMPLE), &crash, unread).is_none());
        assert!(crash.broke());
    }

    #[test]
    fn a_resumed_round_sweeps_its_limit_all_told() {
        let resume = Resume {
            done: vec!["c".into(), "a".into()],
            ..Resume::default()
        };
        let mut planned = vec!["a", "b", "c", "d", "e"];
        left(&mut planned, &resume, Some(3), |p| p.to_string());
        assert_eq!(planned, ["b"]);
        let mut planned = vec!["a", "b", "c", "d"];
        left(&mut planned, &resume, None, |p| p.to_string());
        assert_eq!(planned, ["b", "d"]);
    }

    #[test]
    fn a_restarted_night_goes_on_from_its_last_round() {
        assert_eq!(next_round(""), 0);
        let rows = "{\"round\":0,\"name\":\"a\"}\n{\"round\":6,\"error\":\"x\"}\n\
                    {\"round\":2,\"name\":\"b\"}\nnot json\n";
        assert_eq!(next_round(rows), 7);
    }

    #[test]
    fn a_map_with_no_arrival_point_is_skipped() {
        let mut planned = [
            Planned::open(map::KOKIRI_FOREST_CHILD),
            Planned::open(map::BOSS_RUSH_INTERVAL_ROOM),
        ];
        let places =
            BTreeSet::from([
                dev::Place::parse(&format!("{}/Start", map::KOKIRI_FOREST_CHILD)).unwrap(),
            ]);
        skip_unreachable(&mut planned, &places);
        assert_eq!(kind(&planned[0].trigger), "open");
        assert_eq!(
            planned[1].trigger,
            Trigger::Skip(format!(
                "no arrival point in {}",
                map::BOSS_RUSH_INTERVAL_ROOM
            ))
        );
    }
}
