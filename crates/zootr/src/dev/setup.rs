//! Saved starts: applying a test setup to the game, planning the jobs it
//! makes, the goals it checks and the values and grants it reads.

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use super::{Dev, Place, Request};
use crate::config::TestSetup;
use crate::gamedata::{GameData, Info};
use crate::live::{self, Game, Memory};
use crate::{inventory, starting, steps};

impl<M: Memory> Dev<M> {
    /// Apply a saved test setup: its values, its items, its quest steps,
    /// its hour, then one teleport, so the new map reads the rest, landed
    /// before the warp to `near`, the spot beside its `near` actor. Says what it did, and which goals
    /// already held before it changed anything. A line that can't be run
    /// refuses the whole setup first.
    pub fn apply(
        &self,
        gd: &GameData,
        setup: &TestSetup,
        near: Option<[f32; 3]>,
    ) -> Result<Applied> {
        let jobs = plan(setup, near, |challenge, step| {
            steps::check(gd, challenge, step)
        })?;
        let held = (self.goals(gd, setup).into_iter())
            .filter(|g| g.met)
            .map(|g| g.name)
            .collect();
        let mut done = Vec::new();
        for job in jobs {
            match job {
                Job::Set { hash, value, line } => {
                    live::set(&self.game, self.global, hash, &value)?;
                    done.push(format!("set {line}"));
                }
                Job::Give(actor) => {
                    self.give(gd, &actor, 1)?;
                    done.push(format!("gave {actor}"));
                }
                Job::Step(challenge, step) => {
                    self.step(&challenge, &step)?;
                    done.push(format!("ran quest step {challenge}={step}"));
                }
                Job::Hour(hour) => {
                    self.ask(&Request::Time(hour))?;
                    done.push(format!("set the hour to {hour}"));
                }
                Job::Go(place) => {
                    let went = self.go(&place)?;
                    self.land(&place.map)?;
                    done.push(format!("{went} to {place}"));
                }
                Job::Near(actor, at) => {
                    let warped = self.warp(at)?;
                    done.push(format!("{warped} beside {actor}"));
                }
            }
        }
        Ok(Applied { done, held })
    }

    /// A test setup's goals as the game holds them now.
    pub fn goals(&self, gd: &GameData, setup: &TestSetup) -> Vec<Goal> {
        let read = |hash| live::read(&self.game, gd, self.global, hash);
        setup.goals.iter().map(|g| Goal::check(g, read)).collect()
    }
}

/// One thing giving an item does.
#[derive(Debug, PartialEq)]
pub(super) enum Grant {
    /// The game's grant of an actor, equipped or not.
    Game(String, bool),
    /// The stock written over the count the grant left, for more than one.
    Stock(u32, i32),
}

/// What giving `count` of `actor` does, in order: the game's grant of it and
/// of what comes with it, then its stock if there's more than one. A grant
/// adds one, so the stock is set after it.
pub(super) fn plan_give(gd: &GameData, actor: &str, count: i32) -> Result<Vec<Grant>> {
    ensure!(count >= 1, "a count is 1 or more");
    let stock = inventory::stock(gd, actor)?;
    // Swords and shields have neither, and the starting items are known to
    // grant.
    ensure!(
        stock.is_some()
            || inventory::has_flag(gd, actor)
            || starting::ITEMS.iter().any(|i| i.actor == actor),
        "{actor} isn't an item Link can be given. `zootr items` lists them"
    );
    let mut jobs: Vec<_> = (starting::granted(actor).into_iter())
        .map(|(actor, equip)| Grant::Game(actor.into(), equip))
        .collect();
    if count > 1 {
        let stock =
            stock.with_context(|| format!("{actor} has no count, only one can be given"))?;
        jobs.push(Grant::Stock(stock, count));
    }
    Ok(jobs)
}

/// One thing Apply does.
#[derive(Debug, PartialEq)]
pub(super) enum Job {
    Set {
        hash: u32,
        value: String,
        /// The `NAME=VALUE` line, for the report.
        line: String,
    },
    Give(String),
    Step(String, String),
    Hour(u32),
    Go(Place),
    Near(String, [f32; 3]),
}

/// Everything Apply does for `setup`, in order: values, items, quest steps,
/// the hour, the teleport, then the warp. Each line is read here, and each quest step
/// asked of `known`, so a bad one refuses the setup before anything runs.
pub(super) fn plan(
    setup: &TestSetup,
    near: Option<[f32; 3]>,
    known: impl Fn(&str, &str) -> Result<()>,
) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
    for line in &setup.defaults {
        let (name, value) = name_value(line)?;
        jobs.push(Job::Set {
            hash: live::hash(name)?,
            value: value.into(),
            line: line.clone(),
        });
    }
    jobs.extend(setup.give.iter().cloned().map(Job::Give));
    for line in &setup.progress {
        let (challenge, step) = steps::parse(line)?;
        known(&challenge, &step).with_context(|| format!("quest step {line}"))?;
        jobs.push(Job::Step(challenge, step));
    }
    if let Some(hour) = setup.hour {
        ensure!(hour < 24, "an hour is 0 to 23");
        jobs.push(Job::Hour(hour));
    }
    if let Some(spawn) = &setup.spawn {
        jobs.push(Job::Go(Place::parse(spawn)?));
    }
    if let Some(actor) = &setup.near {
        let at = near.with_context(|| format!("no spot beside {actor} to warp to"))?;
        jobs.push(Job::Near(actor.clone(), at));
    }
    Ok(jobs)
}

/// What applying a test setup did.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Applied {
    pub done: Vec<String>,
    /// Goals that held before Apply, which prove nothing.
    pub held: Vec<String>,
}

/// A GameData value as the dev panel's Set shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename = "DevValue")]
pub struct Value {
    pub info: Info,
    /// The game is up and its memory could be read.
    pub game: bool,
    /// What it holds now, as the Live tab shows it. `None` without the
    /// game, or for a value it doesn't keep.
    pub live: Option<String>,
    /// What a BoolExp works out, as infix: the game keeps no value for it,
    /// so it's set through the values it reads.
    pub expr: Option<String>,
}

/// What `name` is and what it holds in `game`, read from memory without
/// the hook.
pub fn value(gd: &GameData, name: &str, game: Option<&Game>) -> Result<Value> {
    let info = gd.info(name)?;
    let live = game.and_then(|g| g.get(gd, live::hash(name).ok()?).ok());
    Ok(Value {
        info,
        game: game.is_some(),
        live,
        expr: gd.expr_of(live::hash(name)?)?,
    })
}

/// One of a test setup's goals, read from the game.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Goal {
    pub name: String,
    pub want: String,
    /// The value it holds, as the Live tab shows it.
    pub now: Option<String>,
    pub met: bool,
    /// Why it couldn't be read. The other goals still are.
    pub error: Option<String>,
}

impl Goal {
    pub(crate) fn check(line: &str, read: impl FnOnce(u32) -> Result<String>) -> Self {
        let parsed = name_value(line);
        let (name, want) = parsed.as_ref().map_or((line.trim(), ""), |p| *p);
        let now = parsed.and_then(|(name, _)| read(live::hash(name)?));
        Goal {
            name: name.into(),
            want: want.into(),
            met: now.as_deref().is_ok_and(|now| now == want),
            error: now.as_ref().err().map(|e| format!("{e:#}")),
            now: now.ok(),
        }
    }
}

/// A `NAME=VALUE` line, as `defaults` and `goals` write them.
fn name_value(line: &str) -> Result<(&str, &str)> {
    let (name, value) =
        (line.split_once('=')).with_context(|| format!("{line} isn't NAME=VALUE"))?;
    Ok((name.trim(), value.trim()))
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::*;
    use crate::config::TestSetup;
    use crate::known::{flag, flow, item, map};

    #[test]
    fn a_goal_holds_when_the_game_shows_its_value() {
        needs_names!();
        let shows = |text: &'static str| move |_| Ok(text.to_string());
        let met = Goal::check(&format!(" IsGet.{} = true", item::HOOKSHOT), shows("true"));
        assert_eq!(met.name, flag::IS_GET_ITEM_HOOKSHOT);
        assert_eq!(met.want, "true");
        assert!(met.met && met.error.is_none());
        assert!(!Goal::check("Step=Step05", shows("Step03")).met);
        let missing = Goal::check("Nope=1", |_| Err(anyhow!("no entry")));
        assert_eq!(
            (missing.now, missing.error),
            (None, Some("no entry".into()))
        );
        let bad = Goal::check("Nope", shows("true"));
        assert!(!bad.met && bad.error.unwrap().contains("NAME=VALUE"));
    }

    fn gd() -> Option<GameData> {
        let romfs = crate::game::dev_dump()?;
        Some(GameData::load(&romfs).unwrap())
    }

    fn game(actor: &str, equip: bool) -> Grant {
        Grant::Game(actor.into(), equip)
    }

    #[test]
    fn giving_one_is_the_games_grant_and_giving_more_tops_up_the_stock() {
        let Some(gd) = gd() else { return };
        let stock = inventory::stock(&gd, item::BOOMERANG).unwrap().unwrap();
        assert_eq!(
            plan_give(&gd, item::BOOMERANG, 1).unwrap(),
            [game(item::BOOMERANG, false)]
        );
        assert_eq!(
            plan_give(&gd, item::BOOMERANG, 5).unwrap(),
            [game(item::BOOMERANG, false), Grant::Stock(stock, 5)]
        );
        assert!(plan_give(&gd, item::BOOMERANG, 0).is_err());
    }

    #[test]
    fn a_song_is_granted_rather_than_refused() {
        let Some(gd) = gd() else { return };
        assert_eq!(
            plan_give(&gd, item::MELODY_SUN_SONG, 1).unwrap(),
            [game(item::MELODY_SUN_SONG, false)]
        );
        let err = plan_give(&gd, item::MELODY_SUN_SONG, 2).unwrap_err();
        assert!(err.to_string().contains("no count"), "{err}");
        let err = plan_give(&gd, "Item_A", 1).unwrap_err();
        assert!(err.to_string().contains("isn't an item"), "{err}");
    }

    #[test]
    fn gear_is_equipped_and_a_companion_comes_with_its_item() {
        let Some(gd) = gd() else { return };
        assert_eq!(
            plan_give(&gd, item::SWORD_KOKIRI, 1).unwrap(),
            [game(item::SWORD_KOKIRI, true)]
        );
        assert_eq!(
            plan_give(&gd, item::RANGE_SLING_SHOT, 1).unwrap(),
            [
                game(item::RANGE_SLING_SHOT, false),
                game(item::BULLET_BAG_SMALL, false)
            ]
        );
    }

    fn full() -> TestSetup {
        TestSetup {
            spawn: Some(format!("{}/Outside", map::GERUDO_FORTRESS_ADULT)),
            give: vec![item::TOKEN_GERUDO.to_string()],
            defaults: vec!["A=1".into(), "B=true".into()],
            progress: vec![
                format!("{}=Step02", flow::SAGE_OF_SPIRIT),
                format!("{}=Step02", flow::GOTO_HYRULE_CASTLE),
            ],
            hour: Some(21),
            near: Some("Npc_G".into()),
            ..Default::default()
        }
    }

    const SPOT: [f32; 3] = [1.0, 2.0, 3.0];

    #[test]
    fn apply_runs_values_items_steps_the_hour_then_the_teleport() {
        let jobs = plan(&full(), Some(SPOT), |_, _| Ok(())).unwrap();
        let kinds: Vec<_> = jobs
            .iter()
            .map(|j| match j {
                Job::Set { .. } => "set",
                Job::Give(_) => "give",
                Job::Step(..) => "step",
                Job::Hour(_) => "hour",
                Job::Go(_) => "go",
                Job::Near(..) => "near",
            })
            .collect();
        assert_eq!(
            kinds,
            ["set", "set", "give", "step", "step", "hour", "go", "near"]
        );
        assert_eq!(
            jobs[3..5],
            [
                Job::Step(flow::SAGE_OF_SPIRIT.to_string(), "Step02".into()),
                Job::Step(flow::GOTO_HYRULE_CASTLE.to_string(), "Step02".into())
            ]
        );
        assert_eq!(jobs[7], Job::Near("Npc_G".into(), SPOT));
        assert!(
            plan(&TestSetup::default(), None, |_, _| Ok(()))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_bad_line_refuses_the_setup_before_anything_runs() {
        needs_names!();
        let bad = |setup: TestSetup| {
            let err = plan(&setup, Some(SPOT), |_, _| Ok(())).unwrap_err();
            err.to_string()
        };
        let progress = |line: &str| TestSetup {
            progress: vec![line.into()],
            ..full()
        };
        assert!(bad(progress(flow::SAGE_OF_SPIRIT)).contains("CHALLENGE=STEP"));
        let unknown = plan(&full(), Some(SPOT), |c, _| {
            ensure!(c != flow::GOTO_HYRULE_CASTLE, "no such quest");
            Ok(())
        });
        let err = format!("{:#}", unknown.unwrap_err());
        assert!(
            err.contains(&format!("{}=Step02", flow::GOTO_HYRULE_CASTLE))
                && err.contains("no such quest"),
            "{err}"
        );
        let late = TestSetup {
            defaults: vec!["no equals".into()],
            ..full()
        };
        assert!(bad(late).contains("NAME=VALUE"));
        let hour = TestSetup {
            hour: Some(24),
            ..full()
        };
        assert!(bad(hour).contains("0 to 23"));
        let lost = plan(&full(), None, |_, _| Ok(())).unwrap_err().to_string();
        assert!(lost.contains("no spot beside Npc_G"), "{lost}");
    }
}
