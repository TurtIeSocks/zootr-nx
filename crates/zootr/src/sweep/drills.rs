//! The harness ladder's drills: small play tasks with a known answer, each
//! proving one skill the sweep leans on. They run on one seed that shuffles
//! nothing, so every holder keeps its own item (a key chest its stand-in, as
//! the mix starts with every key) and the sweep's verdict is the answer. Each drill's holder failed in a sweep night first.

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use serde_json::json;

use super::judge::Judge;
use super::night::{self, Night, Round};
use super::plan::Planned;
use crate::dev;
use crate::play::Input;
use crate::settings::{self, Settings};

/// What a drill proves the harness can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Skill {
    /// Walk through a door or a shop's doorway.
    Door,
    /// Walk where the navmesh has no route.
    NoRoute,
    /// Climb a ladder or a vine wall.
    Climb,
    /// Swim across water.
    Swim,
    /// Open a chest.
    Chest,
    /// Break a box with a running roll.
    Box,
    /// Lift a rock and throw it.
    Lift,
    /// Talk to someone and get their gift.
    Talk,
    /// Buy from a Business Scrub.
    Scrub,
    /// Buy from a shop's shelf.
    Shop,
    /// Play a song back.
    Song,
}

impl Skill {
    /// The skills every walk leans on. A walk that gave up can't say which
    /// one it lacked.
    pub const WALKING: [Skill; 4] = [Skill::Door, Skill::NoRoute, Skill::Climb, Skill::Swim];

    /// Its name as rows and the command line spell it, like `no-route`.
    pub fn label(self) -> String {
        self.to_possible_value()
            .expect("every skill has a name")
            .get_name()
            .to_owned()
    }
}

/// One play task with a known answer.
#[derive(Debug)]
pub struct Drill {
    /// Short and readable, like `chest facing a wall`.
    pub name: &'static str,
    pub skill: Skill,
    /// The placement hash of the holder to reach and use, as the sweep
    /// plans it.
    pub holder: u64,
}

const fn drill(name: &'static str, skill: Skill, holder: u64) -> Drill {
    Drill {
        name,
        skill,
        holder,
    }
}

/// Every drill, from the sweep's rows: each holder came to nothing there
/// for a reason the harness, not the seed, is to blame for. The Lost Woods
/// scrub passed and stays as a control.
pub const DRILLS: &[Drill] = &[
    drill("poe collector's bottle", Skill::Door, 7975754981590731986),
    drill("potion shop shelf", Skill::Door, 16594423016797951568),
    drill(
        "glass chest, training ground",
        Skill::NoRoute,
        5329807728555596183,
    ),
    drill("gerudo valley chest", Skill::NoRoute, 13612880783193811229),
    drill("man on the roof", Skill::Climb, 9780435211847824593),
    drill("water temple red rupee", Skill::Swim, 11682974908842171598),
    drill("ruto's letter, a dive", Skill::Swim, 474136620438465634),
    drill(
        "chest facing a wall, well",
        Skill::Chest,
        8397646530660039355,
    ),
    drill(
        "chest that rises, ice cavern",
        Skill::Chest,
        6859289214373847284,
    ),
    drill(
        "chest that rises, water temple",
        Skill::Chest,
        3693930347660122883,
    ),
    drill("hyrule field box", Skill::Box, 4706627579492125828),
    drill("colossus rock", Skill::Lift, 12414690125675705208),
    drill(
        "lake scientist, no choice",
        Skill::Talk,
        10888561858771484232,
    ),
    drill(
        "carpet merchant, a choice",
        Skill::Talk,
        17038515111946271548,
    ),
    drill(
        "kokiri forest stick scrub",
        Skill::Scrub,
        182031956235860994,
    ),
    drill("lost woods stick scrub", Skill::Scrub, 5406759280757349546),
    drill("bombchu shop slot", Skill::Shop, 7981518132387036590),
    drill("kakariko cow", Skill::Song, 3581309213670406493),
];

/// The ladder's seed. Shuffling nothing, its number changes nothing.
const SEED: u64 = 1;

/// The file each drill run's row goes to, in the ladder's folder.
pub const ROWS: &str = "drills.jsonl";

/// The ladder's settings: the sweep's mix with every holder kind on, with
/// nothing shuffled, so each kind is planned and keeps its own item.
pub fn settings() -> Result<Settings> {
    let (_, mut settings) = night::mix(night::pinned("everything")?);
    settings.insert(settings::SHUFFLE_ITEMS.into(), false.into());
    Ok(settings)
}

/// The drills `skill` and `name` pick: all of them with neither. `name`
/// matches any part of a drill's name.
pub fn chosen(skill: Option<Skill>, name: Option<&str>) -> Vec<&'static Drill> {
    let fits =
        |d: &&Drill| skill.is_none_or(|s| d.skill == s) && name.is_none_or(|n| d.name.contains(n));
    DRILLS.iter().filter(fits).collect()
}

/// Each drill with its planned spot, in drill order. A holder the seed
/// doesn't plan fails the lot, naming each: no drill can pass without one.
pub fn matched<'a>(drills: &[&'a Drill], planned: &[Planned]) -> Result<Vec<(&'a Drill, Planned)>> {
    let mut found = vec![];
    let mut missing = vec![];
    for d in drills {
        match planned.iter().find(|p| p.spot.hash == d.holder) {
            Some(p) => found.push((*d, p.clone())),
            None => missing.push(d.name),
        }
    }
    if !missing.is_empty() {
        bail!(
            "the ladder's seed plans no holder for: {}",
            missing.join(", ")
        );
    }
    Ok(found)
}

impl<I: Input, J: Judge> Night<'_, I, J> {
    /// Each of `drills`, `repeat` times over, through the sweep's own setup,
    /// walk, use and judge, from the side and age earlier runs found, as the
    /// sweep goes again. Each run's rows go to [`ROWS`], tagged with the
    /// drill and its skill. Stops before the next drill once `turn` says
    /// the game is no longer this run's.
    pub fn ladder(
        &mut self,
        drills: &[&Drill],
        repeat: usize,
        turn: &std::sync::mpsc::Receiver<()>,
    ) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let places = dev::places(self.romfs)?;
        let mod_dir = &self.target.mod_dir;
        let planned = night::pool(self.romfs, mod_dir, SEED, &settings()?, &places)?;
        let drills = matched(drills, &planned)?;
        let rows = self.dir.join(ROWS);
        let first = night::next_round(&std::fs::read_to_string(&rows).unwrap_or_default());
        let mut live = self.start()?;
        for run in first..first + repeat {
            let shots = self.dir.join(format!("run-{run:03}"));
            std::fs::create_dir_all(&shots)?;
            let at = Round {
                round: run,
                seed: SEED,
                mix: "drills",
                places: &places,
                shots: &shots,
                rows: &rows,
            };
            // What earlier runs taught: the side a drill gave its item from,
            // else the one it missed from longest ago, as the sweep goes again.
            let mut known = night::Known::default();
            night::learn(
                &mut known,
                &std::fs::read_to_string(&rows).unwrap_or_default(),
            );
            for (d, p) in &drills {
                if crate::turn::lost(turn) {
                    bail!("the turn was lost: stopping before {}", d.name);
                }
                eprintln!("run {run}: {}", d.name);
                let tag = json!({"drill": d.name, "skill": d.skill});
                let first = night::at_known(p, &known);
                self.attempt(&mut live, &at, p, &first, d.name, &tag)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{Extras, generate};
    use crate::sweep::plan::{Trigger, plan};

    #[test]
    fn every_drill_names_a_holder_the_ladders_seed_plans() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-drills-{}", std::process::id()));
        let report = generate(
            &romfs,
            &dir,
            SEED,
            &settings().unwrap(),
            &Extras::default(),
            &crate::progress::Run::silent(),
        );
        let report = report.unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let planned = plan(&romfs, &report.spots).unwrap();
        let all: Vec<&Drill> = DRILLS.iter().collect();
        for (d, p) in matched(&all, &planned).unwrap() {
            assert!(
                !matches!(p.trigger, Trigger::Skip(_)),
                "{}: {:?}",
                d.name,
                p.trigger
            );
        }
    }

    #[test]
    fn drills_are_picked_by_skill_and_by_part_of_their_name() {
        assert_eq!(chosen(None, None).len(), DRILLS.len());
        let scrubs = chosen(Some(Skill::Scrub), None);
        assert!(scrubs.len() == 2 && scrubs.iter().all(|d| d.skill == Skill::Scrub));
        let names: Vec<_> = chosen(None, Some("rises")).iter().map(|d| d.name).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(chosen(Some(Skill::Song), Some("box")).is_empty());
    }

    #[test]
    fn a_holder_the_seed_lacks_is_named() {
        let fake = drill("fake drill", Skill::Chest, 1);
        let err = matched(&[&fake], &[]).unwrap_err().to_string();
        assert!(err.contains("fake drill"), "{err}");
    }

    #[test]
    fn drill_names_are_unique() {
        let mut names: Vec<_> = DRILLS.iter().map(|d| d.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), DRILLS.len());
    }
}
