//! Open Door of Time. The door only opens at the quest step that also sends
//! Impa and Zelda away, so it reads `OPENED`, a bool its graph already
//! holds. A new game starts as if Link had met Rauru: the sages' quest
//! under way and the Light Medallion in hand. So every pull of the sword is
//! the plain swap, and the door's quest never ends.
//!
//! Adult Link can put the sword back at once. The pedestal waited for the
//! Forest Temple and Sheik stood on it until the Prelude of Light. Her song
//! scene brings its own Sheik, as all her others do.

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::Patch;
use ounce_formats::{Actor, EventKind, Flowchart, Params, Romfs, Value};

use crate::flows::{self, IS_EXP, Rewrites};
use crate::graphs::{logic, node, relink};
use crate::known::{Name, field, flag, flow, map};
use crate::starting;

/// Set when the door opens. A new game starts with it.
pub const OPENED: &Name = flag::MARKET_CHILD_TIME_TEMPLE_DOOR_OPENED;
const DOOR: &Name = map::MARKET_COMMON_F252;
const PEDESTAL: &Name = map::MARKET_ADULT_PREFAB_89DA;
const SHEIK: &Name = map::MARKET_ADULT_PREFAB_7F43;
const CHECK_STEP: &Name = field::HOLD_CHALLENGE_CURRENT_STEP_CHECK_ORDER;
/// The step that opens the door.
const OPEN_STEP: &str = "Step06";
const BOOL: (&Name, &Name) = (
    field::HOLD_GAME_DATA_GET_BOOL_SIMPLE,
    field::GAME_DATA_NAME_BOOL,
);
const EXP: (&Name, &Name) = (
    field::HOLD_GAME_DATA_GET_BOOL_EXP_SIMPLE,
    field::GAME_DATA_NAME_BOOL_EXP,
);
const FOREST_DONE: &Name = flag::SAGE_OF_FOREST_IS_COMPLETED_EXP;
/// Sheik leaves once she's taught the Prelude.
const PRELUDE: &Name = flag::MARKET_ADULT_GET_LIGHT_PRELUDE;
/// True from the sages' first step, so from the start of a new game.
const STARTED: &Name = flag::ALL_SAGE_IS_AFTER_READY_EXP;
const SET_BOOL: &Name = flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL;
const PROGRESS: &Name = flow::EVENT_EXECUTE_PROGRESS_CHALLENGE_STEP;
/// Hides the quest progress banner.
const QUIET: &Name = field::IS_IGNORE_CHALLENGE_PROGRESS_UI;
/// The opening leaves this quest's step to the first swap to adult: while
/// it's under way the forest is silent, child Link included.
const SAGE_OF_FOREST: &Name = flow::SAGE_OF_FOREST;
/// The plain swap a sword pull runs once a game is past Rauru.
const CHILD_TO_ADULT: &Name = flow::MARKET_CHILD_FIELD_CHILD_TO_ADULT;
/// True once the forest sage's quest has reached Step01.
const FOREST_STARTED: &Name = flag::SAGE_OF_FOREST_IS_AFTER_STEP01_EXP;
/// The swap's one Bool: Link is no longer a child.
const SETS_ADULT: (&Name, &Name) = (field::GAME_DATA_ACTOR, SET_BOOL);
/// Runs once Link has met Rauru and woken in the temple.
const FIRST_ADULT: (&Name, &Name) = (flow::ALL_SAGE, field::MARKET_ADULT_ALL_SAGE_START);

/// Patches one graph file.
type Edit = fn(Vec<u8>) -> Result<Vec<u8>>;

/// The door opens on `OPENED` where it opened after the step.
fn door(data: Vec<u8>) -> Result<Vec<u8>> {
    let mut patch = Patch::new(data)?;
    let step = node(
        patch.graph(),
        (CHECK_STEP, field::CHALLENGE_STEP_NAME),
        OPEN_STEP,
    )?;
    let opened = node(patch.graph(), BOOL, OPENED)?;
    relink(&mut patch, (step, flag::IS_AFTER), (opened, "Bool"))?;
    Ok(patch.into_bytes())
}

/// The pedestal takes the sword back once the quest has started, where it
/// waited for the Forest Temple.
fn pedestal(data: Vec<u8>) -> Result<Vec<u8>> {
    let mut patch = Patch::new(data)?;
    let forest = node(patch.graph(), EXP, FOREST_DONE)?;
    patch.set_str(forest, EXP.1, STARTED)?;
    Ok(patch.into_bytes())
}

/// Sheik leaves once the quest has started, where she waited to teach the
/// Prelude. Her graph already reads `STARTED`, unlinked.
fn sheik(data: Vec<u8>) -> Result<Vec<u8>> {
    let mut patch = Patch::new(data)?;
    let prelude = node(patch.graph(), BOOL, PRELUDE)?;
    let started = node(patch.graph(), EXP, STARTED)?;
    relink(&mut patch, (prelude, "Bool"), (started, "Bool"))?;
    Ok(patch.into_bytes())
}

/// The events of the first adult flow that set a bool or reach a quest
/// step, as (actor, action, params), with no progress banner.
fn first_adult(flow: &Flowchart) -> Result<Vec<(&Actor, &str, Params)>> {
    let mut events = Vec::new();
    for e in flows::reachable(flow, FIRST_ADULT.1)? {
        let EventKind::Action {
            actor,
            action,
            params,
            ..
        } = &flow.events[e as usize].kind
        else {
            continue;
        };
        let actor = flow.actor_of(*actor);
        let action = actor.actions[*action as usize].as_str();
        let mut params = params.clone();
        if action == PROGRESS {
            flows::set(&mut params, QUIET, Value::Bool(vec![true]))?;
        } else if action != SET_BOOL {
            continue;
        }
        events.push((actor, action, params));
    }
    Ok(events)
}

/// Run `forest` (the forest sage's step) as the swap to adult makes Link an
/// adult, while that quest hasn't reached it yet.
fn start_forest(
    flow: &mut Flowchart,
    sages: &Flowchart,
    forest: Vec<(&Actor, &str, Params)>,
) -> Result<()> {
    ensure!(
        !forest.is_empty(),
        "the first adult flow never starts the forest sage"
    );
    let adult = flows::only(flow, SETS_ADULT)?;
    let callers = flow.links_to(adult);
    let mut step = adult;
    for (actor, action, params) in forest.into_iter().rev() {
        let (actor, action) = flow.action(actor, action);
        step = flow.push(EventKind::Action {
            next: Some(step),
            actor,
            action,
            params,
        });
    }
    let asks = flows::switch(sages, IS_EXP).context("the first adult flow asks no BoolExp")?;
    let asks = flows::model(sages, IS_EXP, asks)?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    let mut params = asks.params;
    flows::set(
        &mut params,
        field::GAME_DATA_NAME,
        flows::string(FOREST_STARTED.get()),
    )?;
    // Case 0 is true.
    let ask = flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, adult), (1, step)],
    });
    for link in callers {
        flow.link(link, Some(ask))?;
    }
    Ok(())
}

/// Write the patched graphs, and replay the first adult flow at the end of
/// the opening, so the game moves the quests itself. Returns the GameData
/// edits a new game needs. Rauru's gift is `gifts::give_all`'s, Link's Pocket.
/// An `adult` start grows up right after the opening, so the forest sage's
/// step comes in the opening too, where it would wait for the first swap.
pub fn open(romfs: &Romfs, rewrites: &mut Rewrites, adult: bool) -> Result<Vec<String>> {
    let graphs: [(&Name, Edit); 3] = [(DOOR, door), (PEDESTAL, pedestal), (SHEIK, sheik)];
    for (name, patch) in graphs {
        let rel = logic(name);
        let patched = patch(romfs.read(&rel)?).with_context(|| rel.clone())?;
        rewrites.insert(rel, patched);
    }
    let (name, _) = FIRST_ADULT;
    let sages = flows::timeline(romfs, name)?;
    let (forest, rest): (Vec<_>, Vec<_>) = first_adult(&sages)?.into_iter().partition(|e| {
        e.1 == PROGRESS.get()
            && flows::param(&e.2, field::CHALLENGE_NAME) == Some(SAGE_OF_FOREST.get())
    });
    let opening = match adult {
        true => [rest, forest.clone()].concat(),
        false => rest,
    };
    flows::edit(romfs, rewrites, starting::OPENING, |flow| {
        starting::before_save(flow, opening)
    })?;
    flows::edit(romfs, rewrites, CHILD_TO_ADULT, |flow| {
        start_forest(flow, &sages, forest)
    })?;
    Ok(vec![format!("{OPENED}=true")])
}

#[cfg(test)]
mod tests {
    use ounce_formats::ainb::Graph;

    use super::*;
    use crate::game::dev_dump;
    use crate::graphs::link_from;
    use crate::known::{field, flag, flow};

    fn patched(romfs: &Romfs, name: &Name, patch: Edit) -> Graph {
        Graph::parse(&patch(romfs.read(&logic(name)).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn the_door_reads_the_bool() {
        let Some(romfs) = dev_dump() else { return };
        let g = patched(&romfs, DOOR, door);
        let opened = node(&g, BOOL, OPENED).unwrap();
        let (n, ..) = link_from(&g, opened, "Bool").unwrap();
        assert_eq!(g.nodes[n].name, field::OPERATE_OR_FIX_INPUT_TRUE);
    }

    #[test]
    fn the_pedestal_and_sheik_wait_for_nothing() {
        let Some(romfs) = dev_dump() else { return };
        let g = patched(&romfs, PEDESTAL, pedestal);
        assert!(node(&g, EXP, FOREST_DONE).is_err());
        let started = node(&g, EXP, STARTED).unwrap();
        let (n, ..) = link_from(&g, started, "Bool").unwrap();
        assert_eq!(g.nodes[n].name, field::INV_BOOL_MODULE);

        let g = patched(&romfs, SHEIK, sheik);
        let started = node(&g, EXP, STARTED).unwrap();
        let (n, input, _) = link_from(&g, started, "Bool").unwrap();
        assert!(g.nodes[n].name == field::ACTOR_LOGIC_NPC && input == field::LOGIC_DELETE);
    }

    #[test]
    fn a_new_game_starts_past_rauru() {
        let Some(romfs) = dev_dump() else { return };
        for adult in [false, true] {
            starts_past_rauru(&romfs, adult);
        }
    }

    /// The opening's quest steps, with the forest sage's only for `adult`.
    fn starts_past_rauru(romfs: &Romfs, adult: bool) {
        let mut rewrites = Rewrites::new();
        open(romfs, &mut rewrites, adult).unwrap();
        let flow = ounce_formats::Flowchart::parse(&rewrites[&flows::rel(starting::OPENING)])
            .unwrap()
            .unwrap();
        let steps: Vec<_> = flows::running(&flow, (field::CHALLENGE_SYSTEM_ACTOR, PROGRESS))
            .into_iter()
            .filter_map(|e| match &flow.events[e as usize].kind {
                EventKind::Action { params, .. } => Some((
                    flows::param(params, field::CHALLENGE_NAME)?,
                    flows::param(params, field::STEP_NAME)?,
                    params
                        .iter()
                        .any(|(k, v)| k == QUIET && *v == Value::Bool(vec![true])),
                )),
                _ => None,
            })
            .collect();
        for quest in [
            (flow::ALL_SAGE, "Step01"),
            (flag::SAGE_OF_LIGHT, "Complete"),
        ] {
            assert!(
                steps.contains(&(quest.0, quest.1, true)),
                "{quest:?} not in {steps:?}"
            );
        }
        assert_eq!(
            steps.iter().any(|s| s.0 == SAGE_OF_FOREST),
            adult,
            "{steps:?} moves the forest sage on, adult {adult}"
        );
    }

    #[test]
    fn the_swap_to_adult_starts_the_forest_sage_once() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        open(&romfs, &mut rewrites, false).unwrap();
        let flow = ounce_formats::Flowchart::parse(&rewrites[&flows::rel(CHILD_TO_ADULT)])
            .unwrap()
            .unwrap();
        let adult = flows::only(&flow, SETS_ADULT).unwrap();
        let ask = flows::switch(&flow, IS_EXP).unwrap();
        assert!(
            flows::reachable(&flow, CHILD_TO_ADULT)
                .unwrap()
                .contains(&ask)
        );
        let EventKind::Switch { params, cases, .. } = &flow.events[ask as usize].kind else {
            unreachable!("a switch runs the query")
        };
        assert_eq!(
            flows::param(params, field::GAME_DATA_NAME),
            Some(FOREST_STARTED.get())
        );
        let (started, step) = (cases[0], cases[1]);
        assert_eq!(started, (0, adult));
        let EventKind::Action { next, params, .. } = &flow.events[step.1 as usize].kind else {
            panic!("the step isn't an action")
        };
        assert_eq!(*next, Some(adult));
        assert_eq!(
            flows::param(params, field::CHALLENGE_NAME),
            Some(SAGE_OF_FOREST.get())
        );
        assert_eq!(flows::param(params, field::STEP_NAME), Some("Step01"));
    }
}
