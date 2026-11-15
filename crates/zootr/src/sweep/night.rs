//! A night of rounds on the native build: shuffle a mix, install it, start
//! the game, sweep every spot, append the results, and go again.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;
use serde_json::json;

use super::judge::Judge;
use super::plan::{Planned, Trigger, plan};
use super::spot::{Sweeper, Verdict, slug};
use crate::config::Target;
use crate::dev::{self, Dev};
use crate::gamedata::GameData;
use crate::generate::{self, Extras};
use crate::known::{Name, actor};
use crate::live::Game;
use crate::play::{Boot, Input};
use crate::settings::{self, Settings};
use crate::warp::{self, Side};

/// How long the game gets to come up, its shaders prepared on a first start.
const BOOT: Duration = Duration::from_secs(600);
const LOOK: Duration = Duration::from_secs(2);
/// How long a game that can't say how far it has got gets before A goes
/// in: the Mac's title takes buttons after about 34 s, and one sent
/// sooner lands in the menus.
const TITLE: Duration = Duration::from_secs(42);
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

/// What every round's seed has: the test seed rules.
const BASE: [&str; 8] = [
    settings::SKIP_ALL,
    "open-forest",
    "kakariko-gate",
    "start-kokiri-sword",
    "start-deku-shield",
    "start-slingshot",
    "start-bomb-bag",
    "start-fairy-ocarina",
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
    (name, BASE.iter().chain(extra).map(set).collect())
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

/// Shuffle `seed` with `settings` into `mod_dir` and plan each spot, the
/// ones in a map without one of `places` skipped.
pub fn pool(
    romfs: &Romfs,
    mod_dir: &Path,
    seed: u64,
    settings: &Settings,
    places: &BTreeSet<dev::Place>,
) -> Result<Vec<Planned>> {
    let extras = Extras {
        dev_panel: true,
        ..Default::default()
    };
    let report = generate::generate(romfs, mod_dir, seed, settings, &extras)?;
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
}

impl<I: Input, J: Judge> Night<'_, I, J> {
    /// `rounds` rounds, or no end with 0.
    pub fn run(&mut self, rounds: usize) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let places = dev::places(self.romfs)?;
        let rows = std::fs::read_to_string(self.results()).unwrap_or_default();
        let first = next_round(&rows);
        for round in (first..).take_while(|&r| rounds == 0 || r < first + rounds) {
            if let Err(e) = self.round(round, &places) {
                eprintln!("round {round} ended early: {e:#}");
                self.note(round, &format!("{e:#}"))?;
            }
        }
        Ok(())
    }

    fn round(&mut self, round: usize, places: &BTreeSet<dev::Place>) -> Result<()> {
        let (mix, settings) = mix(self.mix.unwrap_or(round));
        let seed = generate::time_seed() % 1_000_000;
        eprintln!("round {round}: {mix}, seed {seed}");
        self.stop();
        let mut planned = pool(self.romfs, &self.target.mod_dir, seed, &settings, places)?;
        planned.retain(|p| self.only.as_deref().is_none_or(|text| picked(p, text)));
        let known = self.known();
        let skip = |p: &Planned| matches!(p.trigger, Trigger::Skip(_));
        let fresh = |p: &Planned| !skip(p) && !known.confirmed.contains(&p.spot.hash);
        let shown = |p: &Planned| known.kinds.contains(holder_kind(&p.spot.holder));
        planned.sort_by_key(|p| (skip(p), !fresh(p), shown(p), map(p)));
        let new = planned.iter().filter(|p| fresh(p)).count();
        let (new, old) = planned.split_at_mut(new);
        start_at(new, seed);
        start_at(old, seed);
        planned.truncate(self.limit.unwrap_or(usize::MAX));
        let shots = self.dir.join(format!("round-{round:03}-{seed}"));
        std::fs::create_dir_all(&shots)?;
        let mut dev = self.start()?;
        let mut fails = 0;
        let at = Round {
            round,
            seed,
            mix,
            places,
            shots: &shots,
        };
        for p in &planned {
            let first = at_known(p, &known);
            let mut verdict = self.spot(&dev, &at, &first, name(p))?;
            if stuck(&verdict) {
                dev = self.start()?;
                verdict = self.spot(&dev, &at, &first, format!("{} (after a restart)", name(p)))?;
            }
            if let Some(again) = other_age(p, &verdict, dev.state()?.child) {
                let again = Planned {
                    side: first.side,
                    ..again
                };
                let age = if again.child == Some(true) {
                    "child"
                } else {
                    "adult"
                };
                verdict = self.spot(&dev, &at, &again, format!("{} (as {age})", name(p)))?;
            }
            fails = match verdict {
                Verdict::SetupFailed(_) => fails + 1,
                _ => 0,
            };
            if verdict.broke() || fails >= FAILS {
                dev = self.start()?;
                fails = 0;
            }
        }
        Ok(())
    }

    /// Sweep one spot and record its row.
    fn spot(&mut self, dev: &Dev, at: &Round, p: &Planned, name: String) -> Result<Verdict> {
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
        .sweep(p, &name);
        let row = json!({
            "round": at.round, "seed": at.seed, "mix": at.mix, "name": name,
            "spot": p.spot, "trigger": kind(&p.trigger), "child": p.child,
            "secs": started.elapsed().as_secs_f32(), "swept": swept,
        });
        self.append(&row)?;
        let verdict = swept.verdict.unwrap_or(Verdict::Nothing);
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
    fn start(&mut self) -> Result<Dev> {
        self.stop();
        if let (Some(from), Some(to)) = (&self.pristine, &self.target.save_dir) {
            restore(from, to).context("restoring the pristine save")?;
        }
        self.input.launch()?;
        let start = Instant::now();
        loop {
            if start.elapsed() > BOOT {
                bail!("the game never reached play");
            }
            sleep(LOOK);
            let boot = match self.input.boot() {
                Ok(Some(boot)) => boot,
                Ok(None) => self.boot(start.elapsed()),
                Err(_) => continue,
            };
            match boot {
                Boot::Play => break,
                Boot::Menus => {
                    let _ = self.input.press("A");
                }
                Boot::Starting => {}
            }
        }
        let dev = Dev::new(Game::attach(self.target)?)?;
        dev.set_switches(SWITCHES)?;
        for _ in 0..CONTAINERS {
            self.give(&dev, actor::OBJ_HEART_CONTAINER)?;
        }
        self.give(&dev, ADULT_SWORD)?;
        Ok(dev)
    }

    /// How far a start has got, by the game's memory, for an input that
    /// can't tell: menus once the title takes buttons, until the dev hook
    /// answers from play.
    fn boot(&self, since: Duration) -> Boot {
        let state = Game::attach(self.target)
            .and_then(Dev::new)
            .and_then(|dev| dev.state());
        match state {
            Ok(s) if !s.map.is_empty() && !s.loading && !s.event => Boot::Play,
            _ if since > TITLE => Boot::Menus,
            _ => Boot::Starting,
        }
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

    fn append(&self, row: &serde_json::Value) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.results())?;
        writeln!(file, "{row}")?;
        Ok(())
    }

    fn results(&self) -> PathBuf {
        self.dir.join("results.jsonl")
    }

    /// What the rows of every night beside this one, oldest first, tell of
    /// each spot.
    fn known(&self) -> Known {
        let mut nights: Vec<PathBuf> = (self.dir.parent())
            .and_then(|all| std::fs::read_dir(all).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|night| night.path().join("results.jsonl"))
            .collect();
        nights.sort();
        let mut known = Known::default();
        for rows in nights
            .iter()
            .filter_map(|f| std::fs::read_to_string(f).ok())
        {
            learn(&mut known, &rows);
        }
        known
    }

    fn note(&self, round: usize, error: &str) -> Result<()> {
        self.append(&json!({"round": round, "error": error}))
    }
}

/// The round after the last one `results` records, so a night restarted
/// after a deploy goes on through the mixes instead of starting over.
fn next_round(results: &str) -> usize {
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
struct Round<'a> {
    round: usize,
    seed: u64,
    mix: &'static str,
    places: &'a BTreeSet<dev::Place>,
    shots: &'a Path,
}

/// What earlier nights learned of each spot, by placement hash.
#[derive(Debug, Default)]
struct Known {
    /// The age a spot was last reached at: an item landed, the dealt one
    /// or not, on a try that named Link's age.
    ages: HashMap<u64, bool>,
    /// Spots that gave the dealt item, or its substitute, at least once. A
    /// round sweeps the others first, so the nights close in on the pool.
    confirmed: HashSet<u64>,
    /// The [`holder_kind`]s a confirmed spot has. One pot that gave
    /// its item shows pots drop theirs, so a round sweeps kinds no spot
    /// has shown yet first: crates before the 38th pot.
    kinds: HashSet<String>,
    /// The side Link last used a spot from when an item landed.
    sides: HashMap<u64, Side>,
    /// The sides a spot came to nothing from: a chest opened from behind,
    /// a keeper talked to from the wrong side of his counter.
    missed: HashMap<u64, Vec<Side>>,
}

/// What kind of holder `holder` is, its number off: `Pot_A_03` is a
/// `Pot_A`, `Kokiri Shop 2` a `Kokiri Shop`.
fn holder_kind(holder: &str) -> &str {
    holder.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_' || c == ' ')
}

/// Add what `rows` tell of each spot to `known`. A later row's age wins.
fn learn(known: &mut Known, rows: &str) {
    for row in rows
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    {
        let verdict = row["swept"]["verdict"]["verdict"].as_str();
        let reached = matches!(verdict, Some("pass" | "wrong_item" | "substituted"));
        let Some(hash) = row["spot"]["hash"].as_str().and_then(|h| h.parse().ok()) else {
            continue;
        };
        if let (true, Some(child)) = (reached, row["child"].as_bool()) {
            known.ages.insert(hash, child);
        }
        if matches!(verdict, Some("pass" | "substituted")) {
            known.confirmed.insert(hash);
            let holder = row["spot"]["holder"].as_str().unwrap_or_default();
            known.kinds.insert(holder_kind(holder).to_owned());
        }
        let side = serde_json::from_value::<Side>(row["swept"]["used"]["side"].clone());
        match (side, reached) {
            (Ok(side), true) => _ = known.sides.insert(hash, side),
            // A side a swing broke a neighbour from is as good as none.
            (Ok(side), false) if matches!(verdict, Some("nothing" | "neighbour")) => {
                let missed = known.missed.entry(hash).or_default();
                if !missed.contains(&side) {
                    missed.push(side);
                }
            }
            _ => {}
        }
    }
}

/// `p` at the age it was last reached at, when its map names none. Logic
/// lists most dungeon checks for both ages, and the wrong one costs a try.
/// And from the side it gave an item from, else the first side it hasn't
/// come to nothing from yet, starting with its planned one, and round to
/// that again once every side has.
fn at_known(p: &Planned, known: &Known) -> Planned {
    let hash = p.spot.hash;
    let missed = known.missed.get(&hash);
    let untried = warp::sides_from(p.side).find(|s| missed.is_none_or(|m| !m.contains(s)));
    Planned {
        child: p.child.or_else(|| known.ages.get(&hash).copied()),
        side: (known.sides.get(&hash).copied())
            .or(untried)
            .unwrap_or(p.side),
        ..p.clone()
    }
}

/// The spot again as the other age, when it came to nothing in a map that
/// names no age. Logic lists most dungeon checks for both ages, so only a
/// try tells which one a room needs. `child` is Link's age when it ended.
fn other_age(p: &Planned, verdict: &Verdict, child: bool) -> Option<Planned> {
    let tried = matches!(verdict, Verdict::Nothing | Verdict::SetupFailed(_)) && !stuck(verdict);
    let m = map(p);
    (tried && p.child.is_none() && !m.is_empty() && crate::warp::era(&m).is_none()).then(|| {
        Planned {
            child: Some(!child),
            ..p.clone()
        }
    })
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

/// The map a spot is swept in, for sweeping a map's spots together.
/// Turn the spots, sorted by map, to start at a map `seed` picks, so a
/// round cut short by `--limit` reaches a different part of the game each
/// time. The skips stay last.
fn start_at(planned: &mut [Planned], seed: u64) {
    let live = (planned.iter())
        .position(|p| matches!(p.trigger, Trigger::Skip(_)))
        .unwrap_or(planned.len());
    let starts: Vec<usize> = (0..live)
        .filter(|&i| i == 0 || map(&planned[i]) != map(&planned[i - 1]))
        .collect();
    if !starts.is_empty() {
        planned[..live].rotate_left(starts[seed as usize % starts.len()]);
    }
}

/// Whether `--only text` keeps `p`: its name, holder or the seed's item
/// there holds each of `text`'s comma-separated parts.
fn picked(p: &Planned, text: &str) -> bool {
    let fields = [name(p), p.spot.holder.clone(), p.spot.item.clone()];
    text.split(',')
        .all(|part| fields.iter().any(|f| f.contains(part)))
}

fn map(p: &Planned) -> String {
    match &p.trigger {
        Trigger::Open { map, .. } | Trigger::Talk { map, .. } => map.clone(),
        Trigger::Play { map, .. } => map.clone().unwrap_or_default(),
        Trigger::Skip(_) => String::new(),
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

/// Copy the folder `from` over `to`, file by file.
/// Make the save folder `to` the pristine one at `from`. Below `to`'s top
/// level, whatever `from` lacks goes: the game keeps writing autosaves into
/// slots the pristine save never had, and Continue loads the newest. `to`'s
/// own neighbours of the save stay.
fn restore(from: &Path, to: &Path) -> Result<()> {
    mirror(from, to, false)
}

fn mirror(from: &Path, to: &Path, prune: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let mut kept = HashSet::new();
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            mirror(&entry.path(), &dest, true)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
        kept.insert(entry.file_name());
    }
    for entry in std::fs::read_dir(to)?.filter(|_| prune) {
        let entry = entry?;
        if kept.contains(&entry.file_name()) {
            continue;
        }
        match entry.file_type()?.is_dir() {
            true => std::fs::remove_dir_all(entry.path())?,
            false => std::fs::remove_file(entry.path())?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, item, map};

    /// A chest spot in `map`, for any age.
    fn open(map: &str) -> Planned {
        Planned {
            spot: Default::default(),
            trigger: Trigger::Open {
                map: map.into(),
                at: Default::default(),
                face: Default::default(),
                ready: vec![],
                steps: vec![],
                first: None,
                broken: false,
                near: vec![],
            },
            child: None,
            side: Side::Front,
        }
    }

    #[test]
    fn only_keeps_the_spots_whose_name_holder_or_item_holds_each_part() {
        needs_names!();
        let mut cow = open(flag::KAKARIKO_VILLAGE_CHILD);
        cow.spot.area_name = "Kakariko Village".into();
        cow.spot.chest_name = "Lon Lon Milk (Cow)".into();
        cow.spot.holder = "Cow".into();
        let mut pot = open(map::KOKIRI_FOREST_CHILD);
        pot.spot.holder = actor::OBJ_POT_A_01.to_string();
        assert!(picked(&cow, "Cow") && picked(&cow, "Kakariko"));
        assert!(picked(&pot, "Pot") && !picked(&pot, "Cow"));
        pot.spot.item = item::MELODY_SUN_SONG.to_string();
        assert!(picked(&pot, "Pot,Melody") && !picked(&pot, "Pot,Bow"));
    }

    #[test]
    fn earlier_nights_give_each_spot_its_age_and_whether_it_gave_its_item() {
        let row = |hash: u64, child: Option<bool>, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()}, "child": child,
                "swept": {"verdict": {"verdict": verdict}}})
            .to_string()
        };
        let rows = [
            row(1, Some(false), "pass"),
            row(1, Some(true), "wrong_item"),
            row(2, Some(true), "nothing"),
            row(3, None, "pass"),
            row(4, Some(false), "substituted"),
            json!({"round": 9, "error": "x"}).to_string(),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        assert_eq!(known.ages, HashMap::from([(1, true), (4, false)]));
        assert_eq!(known.confirmed, HashSet::from([1, 3, 4]));
        let spot = |hash| Planned {
            spot: crate::generate::Spot {
                hash,
                ..Default::default()
            },
            ..open(flag::SPIRIT_TEMPLE)
        };
        assert_eq!(at_known(&spot(4), &known).child, Some(false));
        assert_eq!(at_known(&spot(2), &known).child, None, "never reached");
        let named = Planned {
            child: Some(true),
            side: Side::Front,
            ..spot(4)
        };
        assert_eq!(
            at_known(&named, &known).child,
            Some(true),
            "its map's age wins"
        );
    }

    #[test]
    fn a_kind_of_holder_is_shown_once_one_of_its_spots_gave() {
        needs_names!();
        let row = |hash: u64, holder: &str, verdict: &str| {
            json!({"spot": {"hash": hash.to_string(), "holder": holder},
                "swept": {"verdict": {"verdict": verdict}}})
            .to_string()
        };
        let rows = [
            row(1, actor::OBJ_POT_A_03, "pass"),
            row(2, actor::OBJ_BROKEN_BOX_A_01, "nothing"),
            row(3, "Kokiri Shop 2", "substituted"),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        let kinds = HashSet::from([actor::OBJ_POT_A.to_string(), "Kokiri Shop".to_owned()]);
        assert_eq!(known.kinds, kinds);
        assert_eq!(
            holder_kind(actor::OBJ_T_BOX_A_02_NO_BAKE),
            actor::OBJ_T_BOX_A_02_NO_BAKE
        );
    }

    #[test]
    fn a_spot_goes_again_from_the_side_it_gave_from_else_one_not_tried() {
        let row = |hash: u64, side: &str, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()},
                "swept": {"verdict": {"verdict": verdict}, "used": {"side": side}}})
            .to_string()
        };
        let rows = [
            row(1, "front", "nothing"),
            row(1, "left", "pass"),
            row(2, "front", "nothing"),
            row(2, "left", "nothing"),
            row(2, "front", "stall"),
            row(3, "front", "nothing"),
            row(3, "left", "nothing"),
            row(3, "right", "nothing"),
            row(3, "back", "nothing"),
            row(5, "back", "neighbour"),
            row(6, "back", "neighbour"),
            row(6, "front", "nothing"),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        let planned = |hash, side| Planned {
            spot: crate::generate::Spot {
                hash,
                ..Default::default()
            },
            side,
            ..open(flag::SPIRIT_TEMPLE)
        };
        let side = |hash| at_known(&planned(hash, Side::Front), &known).side;
        assert_eq!(side(1), Side::Left, "the side it gave from");
        assert_eq!(side(2), Side::Right, "the next side not come to nothing");
        assert_eq!(side(3), Side::Front, "round again once all have");
        assert_eq!(side(4), Side::Front, "never swept");
        let back = |hash| at_known(&planned(hash, Side::Back), &known).side;
        assert_eq!(back(4), Side::Back, "never swept, from the planned side");
        assert_eq!(back(5), Side::Front, "a neighbour broke from the back");
        assert_eq!(back(6), Side::Left, "and the front came to nothing");
        assert_eq!(back(1), Side::Left, "the side it gave from still wins");
        assert_eq!(back(3), Side::Back, "round again to the planned side");
    }

    #[test]
    fn a_restore_drops_autosaves_the_pristine_save_never_had() {
        let root = std::env::temp_dir().join(format!("zootr-restore-{}", std::process::id()));
        let (pristine, saves) = (root.join("pristine"), root.join("data"));
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(pristine.join("runner/save/p/slot_00/progress.sav"), "start");
        write(saves.join("runner/save/p/slot_00/progress.sav"), "played");
        write(saves.join("runner/save/p/slot_03/progress.sav"), "autosave");
        write(saves.join("runner/save/p/stray.sav"), "stray");
        write(saves.join("containers/keep"), "the runner's own");
        restore(&pristine, &saves).unwrap();
        let read = |rel: &str| std::fs::read_to_string(saves.join(rel)).ok();
        assert_eq!(
            read("runner/save/p/slot_00/progress.sav").as_deref(),
            Some("start")
        );
        assert!(!saves.join("runner/save/p/slot_03").exists());
        assert_eq!(read("runner/save/p/stray.sav"), None);
        assert_eq!(read("containers/keep").as_deref(), Some("the runner's own"));
        std::fs::remove_dir_all(root).unwrap();
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
    fn a_round_starts_at_a_map_its_seed_picks() {
        let skip = Planned {
            trigger: Trigger::Skip("none".into()),
            ..open("")
        };
        let order = |seed| {
            let mut planned = [open("A"), open("A"), open("B"), open("C"), skip.clone()];
            start_at(&mut planned, seed);
            planned.iter().map(map).collect::<Vec<_>>()
        };
        assert_eq!(order(0), ["A", "A", "B", "C", ""]);
        assert_eq!(order(1), ["B", "C", "A", "A", ""]);
        assert_eq!(order(5), ["C", "A", "A", "B", ""], "the skips stay last");
    }

    #[test]
    fn a_dungeon_spot_that_came_to_nothing_goes_again_as_the_other_age() {
        needs_names!();
        let again = other_age(&open(flag::SPIRIT_TEMPLE), &Verdict::Nothing, true);
        assert_eq!(again.map(|p| p.child), Some(Some(false)));
        let failed = Verdict::SetupFailed("x".into());
        assert!(other_age(&open(flag::SPIRIT_TEMPLE), &failed, false).is_some());
        assert!(other_age(&open(flag::SPIRIT_TEMPLE), &Verdict::Pass, true).is_none());
        assert!(
            other_age(&open(map::KOKIRI_FOREST_CHILD), &Verdict::Nothing, true).is_none(),
            "its map names its age"
        );
        let tried = Planned {
            child: Some(false),
            side: Side::Front,
            ..open(flag::SPIRIT_TEMPLE)
        };
        assert!(
            other_age(&tried, &Verdict::Nothing, false).is_none(),
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
        let open = Planned {
            spot: Default::default(),
            trigger: Trigger::Open {
                map: flag::SPIRIT_TEMPLE.to_string(),
                at: Default::default(),
                face: Default::default(),
                ready: vec![],
                steps: vec![],
                first: None,
                broken: false,
                near: vec![],
            },
            child: None,
            side: Side::Front,
        };
        assert!(other_age(&open, &never, true).is_none());
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
            open(map::KOKIRI_FOREST_CHILD),
            open(map::BOSS_RUSH_INTERVAL_ROOM),
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
