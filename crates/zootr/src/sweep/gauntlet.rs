//! `zootr gauntlet`: every place loaded and every item kind tried at every
//! holder kind, on seeds with every check kind shuffled. It fails, with a
//! frame, on a hung load (the black screen players hit entering an area,
//! #425, #469), a crash, a stall or a wrong item. No vision model: each
//! verdict comes from memory, the process and frame pixels. Specs:
//! `docs/superpowers/specs/2026-11-25-gauntlet-design.md`, and
//! `2026-11-27-gauntlet-v2-design.md` beside it for the loads.

pub mod grid;
pub mod hang;
pub mod loads;
pub mod report;
pub mod script;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde_json::json;

use self::grid::{Cell, HOLDERS, ITEMS, Run};
use self::hang::{Hang, Limits, Watched};
use self::loads::Load;
use self::report::row;
use super::judge::Judge;
use super::night::{self, Live, Night, Round, append};
use super::spot::slug;
use super::verdict::Verdict;
use crate::dev::{self, Place};
use crate::generate::{self, Extras};
use crate::play::Input;
use crate::settings::{self, NewGame, Settings};
use crate::{checks, narrowing};

/// What a gauntlet runs, and how it fails.
#[derive(Debug)]
pub struct Gauntlet {
    pub run: Run,
    /// Only these holder kinds, all with none.
    pub holders: Vec<String>,
    /// Only these item kinds, all with none.
    pub items: Vec<String>,
    /// Tour only the places whose `MAP/SPOT` holds one of these, all with
    /// none.
    pub places: Vec<String>,
    /// Stop at the first failure.
    pub fail_fast: bool,
    /// Loads deal only [`loads::QUICK`].
    pub quick: bool,
    /// How long a black screen or a load may last.
    pub limits: Limits,
    /// The first round's seed, each next round's one more. `None` for one
    /// from the clock.
    pub seed: Option<u64>,
    /// The turn the run holds ([`crate::turn::harness`]): once it's lost,
    /// the run stops before touching the game again. `None` in tests.
    pub turn: Option<std::sync::mpsc::Receiver<()>>,
}

impl Gauntlet {
    /// Whether the run has lost its game's turn.
    fn lost(&self) -> bool {
        self.turn.as_ref().is_some_and(crate::turn::lost)
    }
}

/// The file the report goes to, beside the rows.
pub const REPORT: &str = "report.md";

/// The settings every gauntlet seed has: the sweep's widest mix, with keys
/// shuffled one by one, ice traps, and the check kinds it leaves off.
pub fn settings() -> Result<Settings> {
    let (_, mut s) = night::mix(night::pinned("everything")?);
    s.insert(settings::KEYS.into(), "singles".into());
    s.insert(settings::ICE_TRAPS.into(), "some".into());
    for id in [
        // Keys one by one take the place of the key chests' rupees.
        settings::CHESTS,
        settings::PROGRESSIVE,
        settings::LOOSE_ITEMS,
        settings::SHUFFLE_MASKS,
        settings::TRADE_CHAIN,
        settings::RUNNING_RACE,
        settings::MASTER_SWORD,
    ] {
        s.insert(id.into(), true.into());
    }
    Ok(s)
}

/// Whether the run goes on after a place or cell.
#[derive(PartialEq)]
enum Go {
    On,
    Stop,
}

impl<I: Input, J: Judge> Night<'_, I, J> {
    /// Run `g` and write its report. Returns the exit code
    /// [`report::render`] gives.
    pub fn gauntlet(&mut self, g: &Gauntlet) -> Result<i32> {
        // Settling presses A through arrival text, and cells press buttons.
        self.input.keys().context("the gauntlet presses buttons")?;
        std::fs::create_dir_all(&self.dir)?;
        let places = dev::places(self.romfs)?;
        let settings = settings()?;
        let cells = grid::filtered(g.run.cells(), &g.holders, &g.items)?;
        let refused = refusals(self.romfs, &settings, &cells)?;
        let mut rounds = grid::rounds(&cells);
        // The tour runs on round 0's seed: a seed is needed even with no cells.
        if g.run.tours() && rounds.is_empty() {
            rounds.push(vec![]);
        }
        let seed = g.seed.unwrap_or_else(|| generate::time_seed() % 1_000_000);
        eprintln!(
            "gauntlet {:?}: {} cells in {} rounds, seed {seed}",
            g.run,
            cells.len(),
            rounds.len()
        );
        let mut stopped = false;
        for (r, cells) in rounds.iter().enumerate() {
            let round = (r, seed.wrapping_add(r as u64));
            let ran = self.gauntlet_round(g, round, split(cells, &refused), &settings, &places);
            if self.ended(g, r, ran)? {
                stopped = true;
                break;
            }
        }
        let kinds = loads::kinds(g.run.loads(g.quick), &g.items);
        if !stopped && !kinds.is_empty() {
            let holders = loads::holders(self.romfs, &settings)?;
            let rounds = loads::rounds(&holders, &kinds);
            eprintln!("loads: {} holders, {} rounds", holders.len(), rounds.len());
            for (r, round) in rounds.iter().enumerate() {
                let at = (r, seed.wrapping_add(LOAD_SEEDS + r as u64));
                let ran = self.load_round(g, at, round, &settings, &places);
                if self.ended(g, r, ran)? {
                    break;
                }
            }
        }
        self.stop();
        let rows = match std::fs::read_to_string(self.results()) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            rows => rows?,
        };
        let (md, code) = report::render(&rows, &format!("{:?}, seed {seed}", g.run));
        std::fs::write(self.dir.join(REPORT), md)?;
        Ok(code)
    }

    /// Round `r` on `seed`: its refused cells listed, the `placed` ones
    /// placed, the tour first in round 0 when the run tours, then each cell.
    fn gauntlet_round(
        &mut self,
        g: &Gauntlet,
        (r, seed): (usize, u64),
        (placed, refused): (Vec<Cell>, Vec<&(Cell, String)>),
        settings: &Settings,
        places: &BTreeSet<Place>,
    ) -> Result<Go> {
        let rows = self.results();
        for (c, why) in &refused {
            let row = json!({"gauntlet": row::REFUSED, "cell": c.name(), "round": r, "why": why});
            append(&rows, &row)?;
        }
        let extras = plando(&pins(&placed));
        eprintln!(
            "round {r}: seed {seed}, {} cells, {} refused",
            placed.len(),
            refused.len()
        );
        // A seed installed without the hold replaces another session's.
        if self.turn_lost(g)? {
            return Ok(Go::Stop);
        }
        self.stop();
        let planned = night::pool(self.romfs, self.target, seed, (settings, &extras), places)?;
        let shots = self.dir.join(format!("round-{r:02}-{seed}"));
        std::fs::create_dir_all(&shots)?;
        let mut live = self.start()?;
        if r == 0 && g.run.tours() {
            if self.tour(g, &mut live, places, &shots)? == Go::Stop {
                return Ok(Go::Stop);
            }
            // The cells start clean, on a new game of their own.
            if !placed.is_empty() {
                live = self.start()?;
            }
        }
        let at = Round {
            round: r,
            seed,
            mix: "gauntlet",
            places,
            shots: &shots,
            rows: &rows,
        };
        let known = self.known();
        for c in &placed {
            if self.turn_lost(g)? {
                return Ok(Go::Stop);
            }
            let name = c.name();
            let Some(spot) = planned.iter().find(|p| p.spot.hash == c.holder) else {
                let why = format!("the seed plans no spot at holder {}", c.holder);
                append(
                    &rows,
                    &json!({"gauntlet": row::UNPLANNED, "cell": name, "round": r, "why": why}),
                )?;
                continue;
            };
            let tag = json!({
                "gauntlet": row::CELL, "cell": name,
                "holder_kind": HOLDERS[c.holder_kind].name, "item_kind": ITEMS[c.item_kind].name,
            });
            let first = night::at_known(spot, &known);
            let verdict = self.spot(&live.dev, &at, &first, &name, &tag)?;
            if self.after_cell(g, &mut live, &verdict, (c, &name), &shots)? == Go::Stop {
                return Ok(Go::Stop);
            }
        }
        Ok(Go::On)
    }

    /// Look for a hang after a cell that didn't give its item, record it,
    /// and restart the game when the cell broke it.
    fn after_cell(
        &mut self,
        g: &Gauntlet,
        live: &mut Live,
        verdict: &Verdict,
        (c, name): (&Cell, &str),
        shots: &Path,
    ) -> Result<Go> {
        let trap = ITEMS[c.item_kind].name == grid::ICE_TRAP;
        let outcome = report::outcome(&serde_json::to_value(verdict)?, trap);
        let hang = match outcome {
            report::Outcome::Pass | report::Outcome::NoTrigger(_) => None,
            _ => {
                let frame = shots.join(format!("{}-hang.png", slug(name)));
                hang::probe(&live.dev, &mut self.input, g.limits, &frame).map(|h| (h, frame))
            }
        };
        if let Some((hang, frame)) = &hang {
            dump(&mut self.input);
            let frame = frame.exists().then_some(frame);
            let row = json!({"gauntlet": row::HANG, "name": name, "hang": hang, "frame": frame});
            append(&self.results(), &row)?;
        }
        let failed = hang.is_some() || matches!(outcome, report::Outcome::Fail(_));
        if failed && g.fail_fast {
            return Ok(Go::Stop);
        }
        if hang.is_some() || verdict.broke() || night::stuck(verdict) {
            *live = self.start()?;
        }
        Ok(Go::On)
    }

    /// Whether a round's end, `ran`, ends the run: a stop, or an error
    /// under `--fail-fast`. An error is recorded either way.
    fn ended(&self, g: &Gauntlet, r: usize, ran: Result<Go>) -> Result<bool> {
        match ran {
            Ok(go) => Ok(go == Go::Stop),
            Err(e) => {
                eprintln!("round {r} ended early: {e:#}");
                let error = json!({"gauntlet": row::ERROR, "round": r, "error": format!("{e:#}")});
                append(&self.results(), &error)?;
                Ok(g.fail_fast)
            }
        }
    }

    /// Load round `r` on `seed`: `round`'s items pinned at their holders,
    /// then each map that holds one loaded once and watched.
    fn load_round(
        &mut self,
        g: &Gauntlet,
        (r, seed): (usize, u64),
        round: &[Load],
        settings: &Settings,
        places: &BTreeSet<Place>,
    ) -> Result<Go> {
        let rows = self.results();
        let pins = load_pins(round);
        let mut placed = vec![];
        for (pin, why) in pins.iter().zip(refused(self.romfs, settings, &pins)?) {
            match why {
                Some(why) => append(&rows, &load_row(row::REFUSED, pin, r, &why))?,
                None => placed.push(*pin),
            }
        }
        eprintln!("load round {r}: seed {seed}, {} pinned", placed.len());
        if self.turn_lost(g)? {
            return Ok(Go::Stop);
        }
        self.stop();
        let extras = plando(&placed);
        let planned = night::pool(self.romfs, self.target, seed, (settings, &extras), places)?;
        let mut maps: BTreeMap<&str, Vec<Pin>> = BTreeMap::new();
        for pin in &placed {
            let spot = planned.iter().find(|p| p.spot.hash == pin.0);
            match spot.and_then(|p| p.trigger.map()) {
                Some(map) => maps.entry(map).or_default().push(*pin),
                None => {
                    let why = "the seed plans no map for its holder";
                    append(&rows, &load_row(row::UNPLANNED, pin, r, why))?;
                }
            }
        }
        let shots = self.dir.join(format!("loads-{r:02}-{seed}"));
        std::fs::create_dir_all(&shots)?;
        let mut live = self.start()?;
        let picked =
            |map: &&str| g.places.is_empty() || g.places.iter().any(|p| map.contains(p.as_str()));
        let kept: Vec<_> = maps.iter().filter(|(map, _)| picked(map)).collect();
        for (i, (map, pins)) in kept.iter().enumerate() {
            if self.turn_lost(g)? {
                return Ok(Go::Stop);
            }
            let name = format!("{map} (round {r})");
            // Night's pool plans only spots in maps `places` has.
            let place = (places.iter().find(|p| p.map == **map))
                .with_context(|| format!("no place to teleport to in {map}"))?;
            let frame = shots.join(format!("{}.png", slug(map)));
            let watched = self.land(&live, place, g.limits, &frame);
            self.log(i, kept.len(), &name, &watched);
            let pins: Vec<_> = (pins.iter())
                .map(|&(holder, kind)| json!({"holder": holder.to_string(), "item_kind": ITEMS[kind].name}))
                .collect();
            let row = json!({
                "gauntlet": row::LOAD, "name": name, "round": r, "pins": pins,
                "secs": watched.secs, "hang": watched.hang, "frame": watched.frame,
            });
            append(&rows, &row)?;
            if self.hung(g, &mut live, &watched)? == Go::Stop {
                return Ok(Go::Stop);
            }
        }
        Ok(Go::On)
    }

    /// Teleport to `place` and watch the load, `frame` its evidence.
    fn land(&mut self, live: &Live, place: &Place, limits: Limits, frame: &Path) -> Watched {
        let started = std::time::Instant::now();
        match live.dev.go_as(place, None) {
            Ok(_) => hang::settle(&live.dev, &mut self.input, &place.map, limits, frame),
            Err(e) => Watched {
                secs: started.elapsed().as_secs_f32(),
                frame: None,
                hang: Some(Hang::Refused {
                    why: format!("{e:#}"),
                }),
            },
        }
    }

    /// One landing's line on the console, `i` of `of`.
    fn log(&self, i: usize, of: usize, name: &str, watched: &Watched) {
        let mark = if watched.hang.is_some() { "HANG" } else { "ok" };
        eprintln!("[{}/{of}] {name}: {mark} {:.1}s", i + 1, watched.secs);
    }

    /// After a landing that hung: the game's threads logged, then a stop
    /// under `--fail-fast`, else a new game.
    fn hung(&mut self, g: &Gauntlet, live: &mut Live, watched: &Watched) -> Result<Go> {
        if watched.hang.is_none() {
            return Ok(Go::On);
        }
        dump(&mut self.input);
        if g.fail_fast {
            return Ok(Go::Stop);
        }
        *live = self.start()?;
        Ok(Go::On)
    }

    /// Whether `g` has lost its game's turn, recorded as an error when so.
    fn turn_lost(&self, g: &Gauntlet) -> Result<bool> {
        if !g.lost() {
            return Ok(false);
        }
        let error = "the game's turn was lost: stopped before touching it again";
        eprintln!("{error}");
        append(
            &self.results(),
            &json!({"gauntlet": row::ERROR, "error": error}),
        )?;
        Ok(true)
    }

    /// Teleport to every place in turn and watch each load: child places,
    /// then adult, then those of no age, so Link changes age once.
    fn tour(
        &mut self,
        g: &Gauntlet,
        live: &mut Live,
        places: &BTreeSet<Place>,
        shots: &Path,
    ) -> Result<Go> {
        let order = tour_order(places, &g.places);
        for (i, place) in order.iter().enumerate() {
            if self.turn_lost(g)? {
                return Ok(Go::Stop);
            }
            let name = format!("{}/{}", place.map, place.spot);
            let frame = shots.join(format!("place-{}.png", slug(&name)));
            let watched = self.land(live, place, g.limits, &frame);
            self.log(i, order.len(), &name, &watched);
            let row = json!({
                "gauntlet": row::PLACE, "name": name, "secs": watched.secs,
                "hang": watched.hang, "frame": watched.frame,
            });
            append(&self.results(), &row)?;
            if self.hung(g, live, &watched)? == Go::Stop {
                return Ok(Go::Stop);
            }
        }
        Ok(Go::On)
    }
}

/// The places whose name holds one of `only` (all with none), in tour
/// order: child maps, then adult, then maps of no age, each group in name
/// order.
fn tour_order(places: &BTreeSet<Place>, only: &[String]) -> Vec<Place> {
    let rank = |p: &Place| match p.child() {
        Some(true) => 0,
        Some(false) => 1,
        None => 2,
    };
    let kept = |p: &&Place| {
        let name = format!("{}/{}", p.map, p.spot);
        only.is_empty() || only.iter().any(|o| name.contains(o.as_str()))
    };
    let mut order: Vec<Place> = places.iter().filter(kept).cloned().collect();
    order.sort_by_key(|p| rank(p));
    order
}

/// `cells` split into those to place and those of `refused`, with why.
fn split<'a>(
    cells: &[Cell],
    refused: &'a [(Cell, String)],
) -> (Vec<Cell>, Vec<&'a (Cell, String)>) {
    let refused_here: Vec<_> = refused.iter().filter(|(c, _)| cells.contains(c)).collect();
    let placed = (cells.iter())
        .filter(|c| !refused_here.iter().any(|(o, _)| o == *c))
        .copied()
        .collect();
    (placed, refused_here)
}

/// The thread dump after a hang, or why there's none.
fn dump(input: &mut impl Input) {
    if let Err(e) = input.dump() {
        eprintln!("no thread dump: {e:#}");
    }
}

/// An item kind pinned at a holder: a cell's or a load's.
type Pin = (u64, usize);

/// The plando that pins each of `pins`' items at its holder. The rounds
/// keep one pin a holder.
fn plando(pins: &[Pin]) -> Extras {
    let place: crate::shuffle::Fixed = (pins.iter())
        .map(|&(holder, kind)| (holder, ITEMS[kind].item()))
        .collect();
    debug_assert_eq!(place.len(), pins.len(), "two pins at one holder");
    Extras {
        place,
        ..Extras::default()
    }
}

/// `cells` as pins.
fn pins(cells: &[Cell]) -> Vec<Pin> {
    cells.iter().map(|c| (c.holder, c.item_kind)).collect()
}

/// A load round as pins.
fn load_pins(round: &[Load]) -> Vec<Pin> {
    round.iter().map(|l| (l.holder, l.item_kind)).collect()
}

/// How far load rounds' seeds sit from the grid's, so neither repeats
/// the other's.
const LOAD_SEEDS: u64 = 1000;

/// A `kind` row for a load's `pin` in round `r`, saying `why`.
fn load_row(kind: &str, &(holder, item): &Pin, r: usize, why: &str) -> serde_json::Value {
    let name = format!("{holder} × {} (round {r})", ITEMS[item].name);
    json!({"gauntlet": kind, "load": true, "name": name, "round": r, "why": why})
}

/// Why a cell whose holder the settings don't make a check is refused.
const NOT_A_CHECK: &str = "isn't a check with these settings";

/// Each of `cells` the shuffle refuses with `settings`, and why: an EIE
/// narrowing, or a holder that isn't a check with them.
fn refusals(romfs: &Romfs, settings: &Settings, cells: &[Cell]) -> Result<Vec<(Cell, String)>> {
    let why = refused(romfs, settings, &pins(cells))?;
    Ok((cells.iter().zip(why))
        .filter_map(|(c, why)| Some((*c, why?)))
        .collect())
}

/// Why the shuffle refuses each of `pins`, `None` for one it takes.
fn refused(romfs: &Romfs, settings: &Settings, pins: &[Pin]) -> Result<Vec<Option<String>>> {
    let new_game = NewGame::from_settings(settings, &[])?;
    let dealt = checks::dealt(romfs, &new_game)?;
    let why = |&(holder, kind): &Pin| {
        let item = ITEMS[kind].item();
        match dealt.iter().find(|check| check.hash == holder) {
            None => Some(format!("holder {holder} {NOT_A_CHECK}")),
            Some(check) => narrowing::refusal(check, &item, new_game.progressive)
                .map(|n| format!("{n:?} (EIE debt row {})", n.number())),
        }
    };
    Ok(pins.iter().map(why).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tour_takes_child_places_then_adult_then_ageless() {
        let place = |text: &str| Place::parse(text).unwrap();
        let places: BTreeSet<Place> = [
            "FakeDungeon/FakeSpot",
            "FakeHill_Adult/FakeSpot",
            "FakeHill_Child/FakeSpot",
        ]
        .map(place)
        .into();
        let order: Vec<String> = (tour_order(&places, &[]).iter())
            .map(|p| p.map.clone())
            .collect();
        assert_eq!(order, ["FakeHill_Child", "FakeHill_Adult", "FakeDungeon"]);
        let only = tour_order(&places, &["Hill_A".into()]);
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].map, "FakeHill_Adult");
    }

    #[test]
    fn a_run_whose_renewals_stopped_has_lost_its_turn() {
        let g = |turn| Gauntlet {
            run: Run::Places,
            holders: vec![],
            items: vec![],
            places: vec![],
            fail_fast: false,
            quick: false,
            limits: Limits {
                black: std::time::Duration::from_secs(2),
                load: std::time::Duration::from_secs(45),
            },
            seed: None,
            turn,
        };
        assert!(!g(None).lost());
        let (keep, turn) = std::sync::mpsc::channel();
        let held = g(Some(turn));
        assert!(!held.lost());
        drop(keep);
        assert!(held.lost());
    }

    #[test]
    fn a_round_places_its_cells_but_the_refused_ones() {
        let cells = Run::Holders.cells();
        let refused = vec![
            (cells[1], "fake refusal".to_string()),
            (Run::Items.cells()[5], "x".into()),
        ];
        let (placed, refused_here) = split(&cells, &refused);
        assert_eq!(placed.len(), cells.len() - 1);
        assert!(!placed.contains(&cells[1]));
        assert_eq!(refused_here, [&refused[0]]);
        let extras = plando(&pins(&placed));
        assert_eq!(extras.place.len(), placed.len());
        assert_eq!(
            extras.place[&cells[0].holder],
            ITEMS[cells[0].item_kind].item()
        );
    }

    #[test]
    fn every_cell_holder_is_a_check_with_the_gauntlets_settings() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let settings = settings().unwrap();
        let cells = Run::Full.cells();
        let refused = refusals(&romfs, &settings, &cells).unwrap();
        let missing: Vec<_> = (refused.iter())
            .filter(|(_, why)| why.contains(NOT_A_CHECK))
            .map(|(c, _)| c.name())
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
        let items = refusals(&romfs, &settings, &Run::Items.cells()).unwrap();
        assert!(
            items.is_empty(),
            "a small chest takes every item: {items:?}"
        );
    }

    /// The full grid's first round and `items` cover every holder kind and
    /// every item kind; all 20 rounds generate too, in 5 minutes.
    #[test]
    fn the_first_round_and_the_items_round_generate() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let settings = settings().unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-gauntlet-{}", std::process::id()));
        for run in [Run::Holders, Run::Items] {
            let cells = run.cells();
            let refused = refusals(&romfs, &settings, &cells).unwrap();
            let (placed, _) = split(&cells, &refused);
            let out = dir.join(format!("{run:?}"));
            let report = night::roll(&romfs, 1, &settings, &plando(&pins(&placed)))(&out);
            report.unwrap_or_else(|e| panic!("{run:?}: {e:#}"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_load_round_generates() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let settings = settings().unwrap();
        let holders = loads::holders(&romfs, &settings).unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-gauntlet-loads-{}", std::process::id()));
        // The quick kinds pin no key, so each pinned key chest must leave
        // its stand-in to the keys. Round 0 stands for the rest: all
        // seven full rounds generate too (92 s together).
        let rounds = loads::rounds(&holders, &loads::kinds(&loads::QUICK, &[]));
        for (r, round) in rounds.iter().enumerate().take(1) {
            let pins = load_pins(round);
            let why = refused(&romfs, &settings, &pins).unwrap();
            let placed: Vec<Pin> = (pins.iter().zip(&why))
                .filter(|(_, why)| why.is_none())
                .map(|(p, _)| *p)
                .collect();
            let report =
                night::roll(&romfs, 1, &settings, &plando(&placed))(&dir.join(r.to_string()));
            report.unwrap_or_else(|e| panic!("round {r}: {e:#}"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
