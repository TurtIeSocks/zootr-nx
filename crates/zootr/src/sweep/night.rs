//! A night of rounds on the native build: shuffle a mix, install it, start
//! the game, sweep every spot, append the results, and go again.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde_json::json;

use super::judge::Judge;
use super::plan::{Planned, Trigger, plan};
use super::spot::{Sweeper, slug};
use super::verdict::Verdict;
use crate::config::Target;
use crate::dev::{self, Dev};
use crate::gamedata::GameData;
use crate::generate::{self, Extras};
use crate::known::{Name, actor};
use crate::live::Game;
use crate::play::{Input, Start, boot};
use crate::progress::Run;
use crate::settings::{self, Settings};
use crate::warp::{self, Side};

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
    let report = generate::generate(romfs, mod_dir, seed, settings, &extras, &Run::silent())?;
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
        order(&mut planned, &known, seed);
        planned.truncate(self.limit.unwrap_or(usize::MAX));
        let shots = self.dir.join(format!("round-{round:03}-{seed}"));
        std::fs::create_dir_all(&shots)?;
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
            let first = at_known(p, &known);
            self.attempt(&mut live, &at, p, &first, name(p), &json!({}))?;
        }
        Ok(())
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
        name: String,
        tag: &serde_json::Value,
    ) -> Result<()> {
        let mut verdict = self.spot(&live.dev, at, first, name.clone(), tag)?;
        if stuck(&verdict) {
            *live = self.start()?;
            let again = format!("{name} (after a restart)");
            verdict = self.spot(&live.dev, at, first, again, tag)?;
        }
        if let Some(again) = other_age(p, &verdict, live.dev.state()?.child) {
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
            verdict = self.spot(&live.dev, at, &again, named, tag)?;
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
        name: String,
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
        .sweep(p, &name);
        let mut row = json!({
            "round": at.round, "seed": at.seed, "mix": at.mix, "name": name,
            "spot": p.spot, "trigger": kind(&p.trigger), "child": p.child,
            "secs": started.elapsed().as_secs_f32(), "swept": swept,
        });
        if let (Some(row), Some(tag)) = (row.as_object_mut(), tag.as_object()) {
            row.extend(tag.clone());
        }
        append(at.rows, &row)?;
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
    pub(super) fn start(&mut self) -> Result<Live> {
        self.stop();
        let save = match &self.pristine {
            Some(p) => Some(boot::prepare(&Start::Save(p.clone()), "sweep")?),
            None => None,
        };
        boot::boot(&mut self.input, self.target, save.as_deref(), boot::LIMIT)?;
        let dev = Dev::new(Game::attach(self.target)?)?;
        dev.set_switches(SWITCHES)?;
        for _ in 0..CONTAINERS {
            self.give(&dev, actor::OBJ_HEART_CONTAINER)?;
        }
        self.give(&dev, ADULT_SWORD)?;
        Ok(Live { dev, fails: 0 })
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

/// What earlier nights learned of each spot, by placement hash.
#[derive(Debug, Default)]
pub(super) struct Known {
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
    /// When a spot last came to nothing from each side, as the row it did
    /// in: a chest opened from behind, a keeper talked to from the wrong
    /// side of his counter.
    missed: HashMap<u64, HashMap<Side, usize>>,
    /// How many rows it has learnt from, so a later miss counts as later.
    rows: usize,
    /// How often a spot was swept without giving its item, from any side.
    tries: HashMap<u64, usize>,
}

/// What kind of holder `holder` is, its number off: `Pot_A_03` is a
/// `Pot_A`, `Kokiri Shop 2` a `Kokiri Shop`.
pub(crate) fn holder_kind(holder: &str) -> &str {
    holder.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_' || c == ' ')
}

/// Add what `rows` tell of each spot to `known`. A later row's age wins.
pub(super) fn learn(known: &mut Known, rows: &str) {
    for row in rows
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    {
        known.rows += 1;
        let verdict = row["swept"]["verdict"]["verdict"].as_str();
        let reached = matches!(verdict, Some("pass" | "wrong_item" | "substituted"));
        let Some(hash) = row["spot"]["hash"].as_str().and_then(|h| h.parse().ok()) else {
            continue;
        };
        if verdict.is_some_and(|v| !matches!(v, "pass" | "substituted")) {
            *known.tries.entry(hash).or_default() += 1;
        }
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
                known
                    .missed
                    .entry(hash)
                    .or_default()
                    .insert(side, known.rows);
            }
            _ => {}
        }
    }
}

/// `p` at the age it was last reached at, when its map names none. Logic
/// lists most dungeon checks for both ages, and the wrong one costs a try.
/// And from the side it gave an item from, else a side it never came to
/// nothing from, its planned one first. Once every side has missed, the one
/// that missed longest ago comes round next, so a fix to `use` gets its turn
/// at each: the Well's chest missed from its front 27 times and its back
/// once, and the back is where it opens. Counting misses instead starved a
/// side that missed often before the fix: the Spirit Trial's hammer chest
/// missed 50 times from its front while its flames were up, so the sweep
/// kept trying the other sides once the hammer put them out, and the front
/// is where it opens.
pub(super) fn at_known(p: &Planned, known: &Known) -> Planned {
    let hash = p.spot.hash;
    let missed = |s: &Side| (known.missed.get(&hash)).map_or(0, |m| m.get(s).copied().unwrap_or(0));
    let least = warp::sides_from(p.side).min_by_key(missed);
    Planned {
        child: p.child.or_else(|| known.ages.get(&hash).copied()),
        side: (known.sides.get(&hash).copied())
            .or(least)
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
/// How many tries a spot comes to nothing in before it goes first only
/// in one round of [`STUBBORN_EVERY`].
const STUBBORN: usize = 20;

/// How often a stubborn spot goes first, in rounds. 21 spots past 50 tries
/// took 21 of every round's 40 (2026-11-17), so the 356 confirmed ones
/// came round for a fresh seed's item half as often.
const STUBBORN_EVERY: u64 = 4;

/// The round's order. Spots no try has confirmed go first, the least
/// tried of them first, so a spot that never gives can't take every round
/// from the rest. One that came to nothing [`STUBBORN`] times goes after
/// the confirmed ones, but for one round in [`STUBBORN_EVERY`], so a fix
/// still reaches it. Then kinds no spot has shown before shown ones, then
/// by map from one `seed` picks, so a round cut short by `--limit` reaches
/// a different part of the game each time. The skips go last.
fn order(planned: &mut [Planned], known: &Known, seed: u64) {
    let due = seed % STUBBORN_EVERY == 0;
    let maps: Vec<String> = (planned.iter())
        .filter(|p| !matches!(p.trigger, Trigger::Skip(_)))
        .map(map)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let start = seed as usize % maps.len().max(1);
    let rank = |p: &Planned| {
        let i = maps.binary_search(&map(p)).unwrap_or_default();
        (i + maps.len() - start) % maps.len().max(1)
    };
    planned.sort_by_cached_key(|p| {
        let hash = p.spot.hash;
        let skip = matches!(p.trigger, Trigger::Skip(_));
        let tries = known.tries.get(&hash).copied().unwrap_or_default();
        let confirmed = known.confirmed.contains(&hash);
        let resting = !confirmed && !due && tries >= STUBBORN;
        (
            skip,
            resting,
            confirmed,
            tries,
            known.kinds.contains(holder_kind(&p.spot.holder)),
            rank(p),
        )
    });
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
                wield: None,
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
    fn a_spot_goes_again_from_the_side_it_gave_from_else_the_one_missed_longest_ago() {
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
        let worn = [
            "front", "front", "front", "left", "left", "right", "right", "back",
        ];
        let rows = rows.into_iter().chain(worn.map(|s| row(7, s, "nothing")));
        let mut known = Known::default();
        learn(&mut known, &rows.collect::<Vec<_>>().join("\n"));
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
        assert_eq!(side(7), Side::Front, "every side missed: the longest ago");
        let back = |hash| at_known(&planned(hash, Side::Back), &known).side;
        assert_eq!(back(4), Side::Back, "never swept, from the planned side");
        assert_eq!(back(5), Side::Front, "a neighbour broke from the back");
        assert_eq!(back(6), Side::Left, "and the front came to nothing");
        assert_eq!(back(1), Side::Left, "the side it gave from still wins");
        assert_eq!(back(3), Side::Front, "round again from the longest ago");
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
        let order_of = |seed| {
            let mut planned = [open("A"), open("A"), open("B"), open("C"), skip.clone()];
            order(&mut planned, &Known::default(), seed);
            planned.iter().map(map).collect::<Vec<_>>()
        };
        assert_eq!(order_of(0), ["A", "A", "B", "C", ""]);
        assert_eq!(order_of(1), ["B", "C", "A", "A", ""]);
        assert_eq!(order_of(5), ["C", "A", "A", "B", ""], "the skips stay last");
    }

    #[test]
    fn the_least_tried_spots_go_first_and_confirmed_ones_last() {
        let at = |map: &str, hash: u64| {
            let mut p = open(map);
            p.spot.hash = hash;
            p
        };
        let row = |hash: u64, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()}, "swept": {"verdict": {"verdict": verdict}}})
                .to_string()
        };
        let rows = [
            row(1, "nothing"),
            row(1, "nothing"),
            row(2, "setup_failed"),
            row(3, "pass"),
        ];
        let mut known = Known::default();
        learn(&mut known, &rows.join("\n"));
        let mut planned = [at("A", 1), at("A", 2), at("A", 3), at("B", 4)];
        order(&mut planned, &known, 0);
        let hashes: Vec<u64> = planned.iter().map(|p| p.spot.hash).collect();
        assert_eq!(
            hashes,
            [4, 2, 1, 3],
            "never tried, then tried once, twice, then confirmed"
        );
    }

    #[test]
    fn a_stubborn_spot_goes_first_one_round_in_four_and_last_otherwise() {
        let at = |hash: u64| {
            let mut p = open("A");
            p.spot.hash = hash;
            p
        };
        let row = |hash: u64, verdict: &str| {
            json!({"spot": {"hash": hash.to_string()}, "swept": {"verdict": {"verdict": verdict}}})
                .to_string()
        };
        let rows = std::iter::repeat_n(row(1, "nothing"), STUBBORN)
            .chain(std::iter::repeat_n(row(2, "nothing"), STUBBORN - 1))
            .chain([row(3, "pass")]);
        let mut known = Known::default();
        learn(&mut known, &rows.collect::<Vec<_>>().join("\n"));
        let order_of = |seed| {
            let mut planned = [at(1), at(2), at(3)];
            order(&mut planned, &known, seed);
            planned.map(|p| p.spot.hash)
        };
        assert_eq!(order_of(4), [2, 1, 3], "its round: by tries");
        assert_eq!(order_of(5), [2, 3, 1], "else after the confirmed");
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
                wield: None,
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
