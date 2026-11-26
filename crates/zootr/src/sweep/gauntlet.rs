//! `zootr gauntlet`: every place loaded and every item kind tried at every
//! holder kind, on seeds with every check kind shuffled. It fails, with a
//! frame, on a hung load (the black screen players hit entering an area,
//! #425, #469), a crash, a stall or a wrong item. No vision model: each
//! verdict comes from memory, the process and frame pixels. Spec:
//! `docs/superpowers/specs/2026-11-25-gauntlet-design.md`.

pub mod grid;
pub mod hang;
pub mod report;
pub mod script;

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use ounce_formats::Romfs;
use serde_json::json;

use self::grid::{Cell, HOLDERS, ITEMS, Run};
use self::hang::{Hang, Limits, Watched};
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
    /// Stop at the first failure.
    pub fail_fast: bool,
    pub limits: Limits,
    /// The first round's seed, each next round's one more. `None` for one
    /// from the clock.
    pub seed: Option<u64>,
}

/// The file the report goes to, beside the rows.
pub const REPORT: &str = "report.md";

/// The settings every gauntlet seed has: the sweep's widest mix, with keys
/// shuffled one by one, ice traps, and the check kinds it leaves off.
pub fn settings() -> Result<Settings> {
    let (_, mut s) = night::mix(night::pinned("everything")?);
    s.insert(settings::KEYS.into(), "singles".into());
    s.insert(settings::ICE_TRAPS.into(), "some".into());
    for on in [
        settings::SHUFFLE_ITEMS,
        // Keys one by one take the place of the key chests' rupees.
        settings::CHESTS,
        settings::PROGRESSIVE,
        settings::LOOSE_ITEMS,
        settings::SHUFFLE_MASKS,
        settings::TRADE_CHAIN,
        settings::RUNNING_RACE,
        settings::MASTER_SWORD,
    ] {
        s.insert(on.into(), true.into());
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
        std::fs::create_dir_all(&self.dir)?;
        let places = dev::places(self.romfs)?;
        let settings = settings()?;
        let cells = grid::filtered(g.run.cells(), &g.holders, &g.items)?;
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
        for (r, cells) in rounds.iter().enumerate() {
            let at = (r, seed + r as u64);
            let went = self.gauntlet_round(g, at, cells, &settings, &places);
            match went {
                Ok(Go::On) => {}
                Ok(Go::Stop) => break,
                Err(e) => {
                    eprintln!("round {r} ended early: {e:#}");
                    let error =
                        json!({"gauntlet": row::ERROR, "round": r, "error": format!("{e:#}")});
                    append(&self.results(), &error)?;
                    if g.fail_fast {
                        break;
                    }
                }
            }
        }
        self.stop();
        let rows = std::fs::read_to_string(self.results()).unwrap_or_default();
        let (md, code) = report::render(&rows, &format!("{:?}, seed {seed}", g.run));
        std::fs::write(self.dir.join(REPORT), md)?;
        Ok(code)
    }

    /// Round `r` on `seed`: its refused cells listed, the rest placed, the
    /// tour first in round 0 when the run tours, then each cell.
    fn gauntlet_round(
        &mut self,
        g: &Gauntlet,
        (r, seed): (usize, u64),
        cells: &[Cell],
        settings: &Settings,
        places: &BTreeSet<Place>,
    ) -> Result<Go> {
        let rows = self.results();
        let refused = refusals(self.romfs, settings, cells)?;
        for (c, why) in &refused {
            let row = json!({"gauntlet": row::REFUSED, "cell": c.name(), "round": r, "why": why});
            append(&rows, &row)?;
        }
        let placed: Vec<Cell> = (cells.iter())
            .filter(|c| !refused.iter().any(|(o, _)| o == *c))
            .copied()
            .collect();
        let extras = plando(&placed);
        eprintln!(
            "round {r}: seed {seed}, {} cells, {} refused",
            placed.len(),
            refused.len()
        );
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
            live = self.start()?;
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
            let name = c.name();
            let Some(p) = planned.iter().find(|p| p.spot.hash == c.holder) else {
                let why = format!("the seed plans no spot at holder {}", c.holder);
                append(
                    &rows,
                    &json!({"gauntlet": row::UNPLANNED, "cell": name, "why": why}),
                )?;
                continue;
            };
            let tag = json!({
                "gauntlet": row::CELL, "cell": name,
                "holder_kind": HOLDERS[c.holder_kind].name, "item_kind": ITEMS[c.item_kind].name,
            });
            let first = night::at_known(p, &known);
            let verdict = self.spot(&live.dev, &at, &first, &name, &tag)?;
            if self.after_cell(g, &mut live, &verdict, c, &shots)? == Go::Stop {
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
        c: &Cell,
        shots: &Path,
    ) -> Result<Go> {
        let name = c.name();
        let trap = ITEMS[c.item_kind].name == "ice-trap";
        let outcome = report::outcome(&serde_json::to_value(verdict)?, trap);
        let hang = match outcome {
            report::Outcome::Pass | report::Outcome::NoTrigger(_) => None,
            _ => {
                let frame = shots.join(format!("{}-hang.png", slug(&name)));
                hang::probe(&live.dev, &mut self.input, g.limits, &frame).map(|h| (h, frame))
            }
        };
        if let Some((hang, frame)) = &hang {
            let _ = self.input.dump();
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

    /// Teleport to every place in turn and watch each load: child places,
    /// then adult, then those of no age, so Link changes age once.
    fn tour(
        &mut self,
        g: &Gauntlet,
        live: &mut Live,
        places: &BTreeSet<Place>,
        shots: &Path,
    ) -> Result<Go> {
        let order = tour_order(places);
        for (i, place) in order.iter().enumerate() {
            let name = format!("{}/{}", place.map, place.spot);
            let frame = shots.join(format!("place-{}.png", slug(&name)));
            let started = std::time::Instant::now();
            let watched = match live.dev.go_as(place, None) {
                Ok(_) => hang::settle(&live.dev, &mut self.input, &place.map, g.limits, &frame),
                Err(e) => Watched {
                    secs: started.elapsed().as_secs_f32(),
                    frame: None,
                    hang: Some(Hang::Refused {
                        why: format!("{e:#}"),
                    }),
                },
            };
            let mark = if watched.hang.is_some() { "HANG" } else { "ok" };
            eprintln!(
                "[{}/{}] {name}: {mark} {:.1}s",
                i + 1,
                order.len(),
                watched.secs
            );
            let row = json!({
                "gauntlet": row::PLACE, "name": name, "secs": watched.secs,
                "hang": watched.hang, "frame": watched.frame,
            });
            append(&self.results(), &row)?;
            if watched.hang.is_some() {
                let _ = self.input.dump();
                if g.fail_fast {
                    return Ok(Go::Stop);
                }
                *live = self.start()?;
            }
        }
        Ok(Go::On)
    }
}

/// Places in tour order: child maps, then adult, then maps of no age, each
/// group in name order.
fn tour_order(places: &BTreeSet<Place>) -> Vec<Place> {
    let rank = |p: &Place| match p.child() {
        Some(true) => 0,
        Some(false) => 1,
        None => 2,
    };
    let mut order: Vec<Place> = places.iter().cloned().collect();
    order.sort_by_key(|p| rank(p));
    order
}

/// The plando that pins each of `cells`' items at its holder.
fn plando(cells: &[Cell]) -> Extras {
    Extras {
        place: (cells.iter())
            .map(|c| (c.holder, ITEMS[c.item_kind].item()))
            .collect(),
        ..Extras::default()
    }
}

/// Each of `cells` the shuffle refuses with `settings`, and why: an EIE
/// narrowing, or a holder that isn't a check with them.
fn refusals(romfs: &Romfs, settings: &Settings, cells: &[Cell]) -> Result<Vec<(Cell, String)>> {
    let new_game = NewGame::from_settings(settings, &[])?;
    let dealt = checks::dealt(romfs, &new_game)?;
    let mut out = vec![];
    for c in cells {
        let item = ITEMS[c.item_kind].item();
        let why = match dealt.iter().find(|d| d.hash == c.holder) {
            None => Some(format!(
                "holder {} isn't a check with these settings",
                c.holder
            )),
            Some(check) => narrowing::refusal(check, &item, new_game.progressive)
                .map(|n| format!("{n:?} (EIE debt row {})", n.number())),
        };
        if let Some(why) = why {
            out.push((*c, why));
        }
    }
    Ok(out)
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
        let order: Vec<String> = tour_order(&places).iter().map(|p| p.map.clone()).collect();
        assert_eq!(order, ["FakeHill_Child", "FakeHill_Adult", "FakeDungeon"]);
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
            .filter(|(_, why)| why.contains("isn't a check"))
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
            let placed: Vec<Cell> = (cells.iter())
                .filter(|c| !refused.iter().any(|(o, _)| o == *c))
                .copied()
                .collect();
            let report = night::roll(&romfs, 1, &settings, &plando(&placed))(&dir);
            report.unwrap_or_else(|e| panic!("{run:?}: {e:#}"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
