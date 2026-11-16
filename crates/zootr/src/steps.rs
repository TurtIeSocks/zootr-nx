//! Quest steps a test start moves on. A quest ("challenge") holds a current
//! step that NPCs, gates and AI graphs read, and only the flow action
//! `EventExecuteProgressChallengeStep` moves it: writing the saved
//! `Step_<Challenge>` leaves the quest system on the old one.
//!
//! A new game replays the steps in its opening (see `starting::replay`). A
//! running game runs one through the dev panel: a seed built with it has a
//! branch at the head of the map change flow, which the hook starts with the
//! step's names as arguments.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{BymlDoc, EventKind, Flowchart, Link, Params, Romfs, Value};
use zootr_hook::{ARG_ACTOR, ARG_CHALLENGE, ARG_EQUIP, ARG_GIVE, ARG_STEP, ARG_STEP_NAME};

use crate::checks::str_field;
use crate::flows::{self, Model, Rewrites, running, set, switch};
use crate::gamedata::GameData;
use crate::known::{Name, field, flag, flow, scene};
use crate::rsdb;
use crate::starting;

/// The flow with the quest's own progress event, to copy.
pub(crate) const TEMPLATE: &Name = flow::SAGE_OF_SPIRIT;
pub(crate) const PROGRESS: (&Name, &Name) = (
    field::CHALLENGE_SYSTEM_ACTOR,
    flow::EVENT_EXECUTE_PROGRESS_CHALLENGE_STEP,
);
/// Hides the quest progress banner.
pub(crate) const QUIET: &Name = field::IS_IGNORE_CHALLENGE_PROGRESS_UI;
/// The flow every map change runs, which the hook starts, and the entry the
/// game starts when a request names none.
pub(crate) const BANC: &Name = scene::SY_BANC_CHANGE_GO;
const ENTRY: &Name = flag::ENTRY_POINT0;
const IS_BOOL: (&Name, &Name) = (field::GAME_DATA_ACTOR, flow::EVENT_QUERY_IS_BOOL);
/// The challenge's saved step, an Enum whose values are its steps.
pub const SAVED: &str = "Step_";

/// The quest flow, which holds the shape of each event the steps copy.
pub(crate) fn template(romfs: &Romfs) -> Result<Flowchart> {
    flows::load(romfs, TEMPLATE)?.context("the quest flow is a timeline")
}

/// The quest flow's own progress event.
pub(crate) fn progress(flow: &Flowchart) -> Result<Model> {
    let at = *running(flow, PROGRESS)
        .first()
        .context("the quest flow never progresses")?;
    flows::model(flow, PROGRESS, at)
}

/// `progress`'s params, moving `challenge` to `step`, with no banner.
pub(crate) fn moving(progress: &Model, challenge: Value, step: Value) -> Result<Params> {
    let mut params = progress.params.clone();
    set(&mut params, field::CHALLENGE_NAME, challenge)?;
    set(&mut params, field::STEP_NAME, step)?;
    set(&mut params, QUIET, Value::Bool(vec![true]))?;
    Ok(params)
}

/// A `Challenge=Step` line, as a test start writes it.
pub fn parse(line: &str) -> Result<(String, String)> {
    let (challenge, step) = (line.split_once('='))
        .map(|(c, s)| (c.trim(), s.trim()))
        .filter(|(c, s)| !c.is_empty() && !s.is_empty())
        .with_context(|| format!("{line} isn't CHALLENGE=STEP"))?;
    Ok((challenge.into(), step.into()))
}

/// Whether `challenge` is a quest in the game and `step` one of its steps.
pub fn check(gd: &GameData, challenge: &str, step: &str) -> Result<()> {
    let saved = format!("{SAVED}{challenge}");
    let steps = gd
        .enum_values(&saved)
        .with_context(|| format!("{challenge} isn't a quest: the game has no {saved}"))?;
    ensure!(
        steps.iter().any(|s| s == step),
        "{step} isn't a step of {challenge}. Its steps are {}",
        steps.join(", ")
    );
    Ok(())
}

/// Every quest `gd` knows a saved step for, with its steps in order. Needs
/// [`learn`] first, which names the saved steps.
pub fn quests(gd: &GameData) -> BTreeMap<String, Vec<String>> {
    (gd.names())
        .filter_map(|n| Some((n.strip_prefix(SAVED)?.to_owned(), gd.enum_values(n)?)))
        .collect()
}

/// Quest `challenge`'s saved step, by name.
pub fn saved(challenge: &Name) -> String {
    format!("{SAVED}{challenge}")
}

/// The edit that saves quest `challenge` at `step`.
pub fn at(challenge: &Name, step: impl std::fmt::Display) -> String {
    format!("{}={step}", saved(challenge))
}

/// Teach `gd` every quest's saved step name, which its data keeps only as
/// a hash: a BoolExp on one then reads as the saved step and its value.
pub fn learn(romfs: &Romfs, gd: &mut GameData) -> Result<()> {
    let table = BymlDoc::parse(&romfs.read(&rsdb::path(CHALLENGES))?)?;
    for row in table.root.as_array()? {
        let id = str_field(row, rsdb::ROW_ID).unwrap_or_default();
        let name = id.rsplit('/').next().and_then(|f| f.split('.').next());
        if let Some(challenge) = name {
            gd.learn(&format!("{SAVED}{challenge}"));
        }
    }
    Ok(())
}

/// The table of quests, a row each, its id the quest's file name under `Work/Challenge`.
const CHALLENGES: &str = "Challenge";

/// [`parse`] and [`check`] each of `lines`, in order.
pub fn parse_all(gd: &GameData, lines: &[String]) -> Result<Vec<(String, String)>> {
    lines
        .iter()
        .map(|l| {
            let (challenge, step) = parse(l)?;
            check(gd, &challenge, &step).with_context(|| format!("quest step {l}"))?;
            Ok((challenge, step))
        })
        .collect()
}

/// An argument's name as the flow reads it.
fn arg(name: &[u8]) -> String {
    String::from_utf8_lossy(&name[..name.len() - 1]).into_owned()
}

/// Put branches at the head of the map change flow's entry: when the hook
/// passes `ZootrGive`, grant the item `ZootrActor`, equipped if `ZootrEquip`,
/// and end. When it passes `ZootrStep`, run the quest step named by
/// `ZootrChallenge` and `ZootrStepName` and end. Otherwise the flow runs as
/// it did, since an argument the hook doesn't pass reads as false.
pub fn dev_branch(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    let template = template(romfs)?;
    let progress = progress(&template)?;
    let grant = starting::template(romfs)?;
    flows::edit(romfs, rewrites, BANC, |flow| {
        branch(flow, &progress, &grant)
    })
}

/// A switch on the bool argument `name`, to `yes` when it holds, else `no`.
fn when(flow: &mut Flowchart, asks: &Model, name: &[u8], yes: u16, no: u16) -> Result<u16> {
    let (actor, query) = flow.query(&asks.actor, asks.name);
    let mut params = asks.params.clone();
    set(&mut params, field::BOOL_VALUE, Value::Argument(arg(name)))?;
    // Case 0 is true.
    Ok(flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, yes), (1, no)],
    }))
}

/// The grant of the item the hook names, then the end. It's the starting
/// items' own grant event, so no "You got" window.
fn grant(flow: &mut Flowchart, template: &Flowchart, equip: bool) -> Result<u16> {
    let (actor, params) = starting::grant_event(template)?;
    let params = starting::granting(
        params,
        Value::Argument(arg(ARG_ACTOR)),
        Value::Bool(vec![equip]),
    )?;
    let (actor, action) = flow.action(actor, flows::GRANT.1);
    Ok(flow.push(EventKind::Action {
        next: None,
        actor,
        action,
        params,
    }))
}

/// The map change flow's entry the game starts, and its first event.
pub(crate) fn entry(flow: &Flowchart) -> Result<(usize, u16)> {
    let entry = (flow.entry_points.iter())
        .position(|e| e.name == ENTRY)
        .with_context(|| format!("{BANC} has no {ENTRY}"))?;
    let main = flow.entry_points[entry]
        .main
        .with_context(|| format!("{ENTRY} runs nothing"))?;
    Ok((entry, main))
}

fn branch(flow: &mut Flowchart, progress: &Model, template: &Flowchart) -> Result<()> {
    let (entry, main) = entry(flow)?;
    let asks = switch(flow, IS_BOOL).with_context(|| format!("{BANC} never asks {IS_BOOL:?}"))?;
    let asks = flows::model(flow, IS_BOOL, asks)?;
    let (actor, action) = flow.action(&progress.actor, progress.name);
    let params = moving(
        progress,
        Value::Argument(arg(ARG_CHALLENGE)),
        Value::Argument(arg(ARG_STEP_NAME)),
    )?;
    let run = flow.push(EventKind::Action {
        next: None,
        actor,
        action,
        params,
    });
    let step = when(flow, &asks, ARG_STEP, run, main)?;
    // Equipping is a second query, between two grants, so the only argument
    // a flow reads is the one the steps already prove: a query's `BoolValue`.
    let plain = grant(flow, template, false)?;
    let equipped = grant(flow, template, true)?;
    let give = when(flow, &asks, ARG_EQUIP, equipped, plain)?;
    let head = when(flow, &asks, ARG_GIVE, give, step)?;
    flow.link(Link::Entry(entry), Some(head))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{actor, field, flow};

    #[test]
    fn a_line_is_a_challenge_and_a_step() {
        assert_eq!(
            parse(&format!(" {} = Step02 ", flow::SAGE_OF_SPIRIT)).unwrap(),
            (flow::SAGE_OF_SPIRIT.to_string(), "Step02".into())
        );
        for bad in [
            &**flow::SAGE_OF_SPIRIT,
            "=Step02",
            format!("{}=", flow::SAGE_OF_SPIRIT).as_str(),
            "",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_learned_quest_is_listed_with_its_steps() {
        let Some(romfs) = dev_dump() else { return };
        let mut gd = GameData::load(&romfs).unwrap();
        assert!(quests(&gd).is_empty(), "nothing learned yet");
        learn(&romfs, &mut gd).unwrap();
        let quests = quests(&gd);
        let steps = &quests[&**flow::SAGE_OF_SPIRIT];
        assert!(steps.iter().any(|s| s == "Step02"), "{steps:?}");
        assert!(quests.contains_key(&**flow::GOTO_HYRULE_CASTLE));
    }

    #[test]
    fn a_quest_and_its_steps_must_exist() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        check(&gd, flow::SAGE_OF_SPIRIT, "Step02").unwrap();
        check(&gd, flow::GOTO_HYRULE_CASTLE, "Step02").unwrap();
        let err = check(&gd, "SageOfNothing", "Step02").unwrap_err();
        assert!(err.to_string().contains("isn't a quest"), "{err}");
        let err = check(&gd, flow::SAGE_OF_SPIRIT, "Step99").unwrap_err();
        let err = err.to_string();
        assert!(
            err.contains("Step99") && err.contains("Step02, Step03"),
            "{err}"
        );
        let lines = [
            format!("{}=Step02", flow::SAGE_OF_SPIRIT),
            format!("{}=Nope", flow::GOTO_HYRULE_CASTLE),
        ];
        let err = parse_all(&gd, &lines).unwrap_err();
        assert!(
            format!("{err:#}").contains(&format!("{}=Nope", flow::GOTO_HYRULE_CASTLE)),
            "{err:#}"
        );
    }

    /// A switch's params, and the events its cases go to.
    fn asked(flow: &Flowchart, at: u16) -> (&Params, [u16; 2]) {
        match &flow.events[at as usize].kind {
            EventKind::Switch { params, cases, .. } => match cases[..] {
                [(0, yes), (1, no)] => (params, [yes, no]),
                ref other => panic!("{other:?}"),
            },
            other => panic!("not a switch: {other:?}"),
        }
    }

    /// The argument a switch asks.
    fn asks(flow: &Flowchart, at: u16) -> (Value, [u16; 2]) {
        let (params, cases) = asked(flow, at);
        let held = params.iter().find(|(k, _)| k == field::BOOL_VALUE).unwrap();
        (held.1.clone(), cases)
    }

    fn param<'a>(flow: &'a Flowchart, at: u16, key: &str) -> &'a Value {
        let EventKind::Action { params, .. } = &flow.events[at as usize].kind else {
            panic!("not an action")
        };
        &params.iter().find(|(k, _)| k == key).unwrap().1
    }

    fn argument(name: &str) -> Value {
        Value::Argument(name.into())
    }

    /// The map change flow with the dev branches, and its first event before.
    fn branched(romfs: &Romfs) -> (Flowchart, Flowchart, u16) {
        let before = flows::load(romfs, BANC).unwrap().unwrap();
        let was = (before.entry_points.iter())
            .find(|e| e.name == ENTRY)
            .unwrap()
            .main
            .unwrap();
        let mut rewrites = Rewrites::new();
        dev_branch(romfs, &mut rewrites).unwrap();
        let data = &rewrites[&flows::rel(BANC)];
        let flow = Flowchart::parse(data).unwrap().unwrap();
        assert_eq!(Flowchart::parse(&flow.to_bytes()).unwrap().unwrap(), flow);
        (before, flow, was)
    }

    fn head(flow: &Flowchart) -> u16 {
        let entry = flow.entry_points.iter().find(|e| e.name == ENTRY).unwrap();
        entry.main.unwrap()
    }

    #[test]
    fn a_dev_branch_runs_the_step_or_the_old_flow() {
        let Some(romfs) = dev_dump() else { return };
        let (before, flow, was) = branched(&romfs);
        // Give is first, and ZootrGive false reaches the step.
        let (give, [_, next]) = asks(&flow, head(&flow));
        assert_eq!(give, argument("ZootrGive"));
        let (step, [run, rest]) = asks(&flow, next);
        assert_eq!(step, argument("ZootrStep"));
        assert_eq!(
            rest, was,
            "ZootrStep false reaches the original first event"
        );
        let EventKind::Action { next, .. } = &flow.events[run as usize].kind else {
            panic!("no step")
        };
        assert_eq!(*next, None);
        for (key, arg) in [
            (field::CHALLENGE_NAME, "ZootrChallenge"),
            (field::STEP_NAME, "ZootrStepName"),
        ] {
            assert_eq!(param(&flow, run, key), &argument(arg), "{key}");
        }
        assert_eq!(param(&flow, run, QUIET), &Value::Bool(vec![true]));
        let steps = running(&flow, PROGRESS);
        assert_eq!(steps, [run], "one step in the flow");
        // Nothing else changed: every old event is as it was.
        for (i, e) in before.events.iter().enumerate() {
            assert_eq!(flow.events[i], *e, "event {i}");
        }
    }

    #[test]
    fn a_dev_branch_grants_what_the_hook_names() {
        let Some(romfs) = dev_dump() else { return };
        let (_, flow, _) = branched(&romfs);
        let (_, [equip, _]) = asks(&flow, head(&flow));
        let (asked, [equipped, plain]) = asks(&flow, equip);
        assert_eq!(asked, argument("ZootrEquip"));
        let grants = running(&flow, flows::GRANT);
        assert_eq!(grants.len(), 2, "no other grant joined the flow");
        for (at, equips) in [(equipped, true), (plain, false)] {
            assert!(grants.contains(&at));
            let EventKind::Action { next, .. } = &flow.events[at as usize].kind else {
                unreachable!()
            };
            assert_eq!(*next, None, "a grant ends the event");
            assert_eq!(param(&flow, at, "Actor"), &argument("ZootrActor"));
            assert_eq!(
                param(&flow, at, field::IS_EQUIP_D_IRECT),
                &Value::Bool(vec![equips])
            );
            // One item and no get event, as the opening grants.
            assert_eq!(
                param(&flow, at, field::IS_GET_EVENT),
                &Value::Bool(vec![false])
            );
            assert_eq!(param(&flow, at, "Num"), &Value::Int(vec![1]));
        }
    }

    #[test]
    fn a_dev_branch_never_warps() {
        let Some(romfs) = dev_dump() else { return };
        let (_, flow, _) = branched(&romfs);
        let warp = (actor::PLAYER_EVENT, flow::EVENT_PLAYER_WARP_TO_TARGET_ACTOR);
        assert!(
            running(&flow, warp).is_empty(),
            "the hook moves Link itself"
        );
    }

    #[test]
    fn the_hook_and_the_flow_name_the_same_arguments() {
        assert_eq!(
            [arg(ARG_STEP), arg(ARG_CHALLENGE), arg(ARG_STEP_NAME)],
            ["ZootrStep", "ZootrChallenge", "ZootrStepName"]
        );
        assert_eq!(
            [arg(ARG_GIVE), arg(ARG_ACTOR), arg(ARG_EQUIP)],
            ["ZootrGive", "ZootrActor", "ZootrEquip"]
        );
    }
}
