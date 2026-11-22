//! Cutting story scenes. A scene whose effects can all be copied out is never
//! played: its caller's transition to it becomes the set of its played flag,
//! then the scene's own lasting effects, in the scene's order. A grant isn't
//! copied, `gifts` grants it right after the flag. See
//! `docs/research/cutscenes.md`.

use std::collections::BTreeSet;

use anyhow::Result;
use ounce_formats::{Actor, EventKind, Flowchart, Link, Params, Value};

use crate::cutscenes::{self, Cutscene, lasting};
use crate::events::Flows;
use crate::flows::{self, ACTOR, GRANT, param};
use crate::known::{Name, flag, flow, scene};
use crate::{arrivals, gifts};

/// A scene's entry point, which every call into its sub-flows comes from.
const ENTRY: &Name = flag::ENTRY_POINT0;
/// Actions copied into a caller as they are.
const COPIED: [&Name; 7] = [
    flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
    flow::EVENT_TRIGGER_SET_GAME_DATA_STRING64,
    flow::EVENT_TRIGGER_COPY_GAME_DATA_STRING64,
    flow::EVENT_TRIGGER_SET_GAME_DATA_INT,
    flow::EVENT_TRIGGER_ADD_OR_SUB_GAME_DATA_INT,
    flow::EVENT_EXECUTE_PROGRESS_CHALLENGE_STEP,
    flow::EVENT_TRIGGER_SET_TIME,
];
fn is_copied(action: &str) -> bool {
    COPIED.iter().any(|c| **c == *action)
}
/// Parts of an action's name that make or end an actor, which only the scene
/// itself reaches.
const ACTOR_STATE: [&str; 5] = ["Death", "Appear", "Spawn", "Generate", "Pause"];
const TRANSITION: &Name = flow::EVENT_TRANSITION;
/// Sets up a "You got" window, which a cut has no use for.
const WINDOW: &Name = flow::EVENT_TRIGGER_SET_FOR_GET_WINDOW;
/// The song lesson, which a grant in the scene's place makes pointless.
const LESSON: &Name = scene::SY_OCARINA;
/// What a switch on Link's pick from a menu asks. The others read the game's
/// state, and the effects past them are copied whichever way they went.
const CHOICE: &str = "Choice";

const EXIT: &str =
    "an exit whose warp may be what puts Link where the banc change after it sends him";
const MOVIE: &str = "in a chain with a pre-rendered movie, which has to be swapped whole";
const LINK: &str = "Link started it, with a talk or a tune";
const HORSE: &str = "Epona's rides and jumps, which the timeline performs";
const MINIGAME: &str = "a minigame, which Link starts";
const BOSS: &str = "a boss fight's own scene, which carries its spawn";
const LISTENED: &str = "an actor or the map listens to it from inside";
const CHOICE_EFFECTS: &str = "its choices lead to different effects";
const CLOCK: &str = "its timeline sets the clock, which a flag can't";

/// Scenes the rule would cut that stay, and why. These need a judgement the
/// flows can't give: Link started them, the actor or the map listens from
/// inside, a movie covers them, or Link is carried to another map.
pub(crate) const KEPT: &[(&Name, &str)] = &[
    (scene::DEKU_0010, EXIT),
    (scene::DESERT_COLOSSUS_0040, EXIT),
    (scene::DODONGO_CAVE_0030, EXIT),
    (scene::FIRE_TEMPLE_0030, EXIT),
    (scene::FOREST_TEMPLE_0020, EXIT),
    (scene::GRAVEYARD_0030, EXIT),
    (scene::INSIDE_JABU_JABU_0060, EXIT),
    (scene::LAKE_HRL_0020, EXIT),
    (scene::SHADOW_TEMPLE_0020, EXIT),
    (scene::SPIRIT_TEMPLE_0040, EXIT),
    (scene::WATER_TEMPLE_0040, EXIT),
    (scene::INSIDE_GANON_CASTLE_0170, MOVIE),
    (scene::KAKARIKO_0025, MOVIE),
    (scene::KOKIRI_0080, MOVIE),
    (scene::KOKIRI_0130, MOVIE),
    (scene::KOKIRI_0140, MOVIE),
    (scene::KOKIRI_0162, MOVIE),
    (scene::LOST_WOOD_0025, MOVIE),
    (scene::MELODY_GET_SUN_03, MOVIE),
    (scene::TIME_TEMPLE_0090, MOVIE),
    (scene::DEATH_MT_TRAIL_0040, LINK),
    (scene::GORON_0028, LINK),
    (scene::WATER_TEMPLE_0020, LINK),
    (scene::WATER_TEMPLE_0021, LINK),
    (scene::WATER_TEMPLE_0022, LINK),
    (scene::GERUDO_VALLEY_0010, HORSE),
    (scene::GERUDO_VALLEY_0020, HORSE),
    (scene::HRL_FIELD_0070, HORSE),
    (scene::HRL_FIELD_0071, HORSE),
    (scene::HRL_FIELD_0072, HORSE),
    (scene::HRL_FIELD_0073, HORSE),
    (scene::LON_LON_0020, MINIGAME),
    (scene::LON_LON_0040, MINIGAME),
    (scene::ZORA_0020, MINIGAME),
    (scene::DODONGO_CAVE_BOSS_OUT, BOSS),
    (scene::FINAL_BATTLE_0030, BOSS),
    (scene::FINAL_BATTLE_BOSS_IN, BOSS),
    (scene::GERUDO_0010, LISTENED),
    (scene::GERUDO_0023, LISTENED),
    (scene::GRAVEYARD_0010, LISTENED),
    (scene::SACRED_FOREST_0000, LISTENED),
    (scene::SHADOW_TEMPLE_0010, LISTENED),
    (scene::TIME_TEMPLE_0065, LISTENED),
    (scene::GERUDO_0025, CHOICE_EFFECTS),
    (scene::KAKARIKO_0030, CLOCK),
    (scene::KAKARIKO_0040, CLOCK),
];

/// One lasting effect of a scene, as it runs in a caller.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Copy {
    Action {
        actor: Actor,
        action: String,
        params: Params,
    },
    Call {
        flowchart: String,
        entry_point: String,
        params: Params,
    },
}

/// What cutting a scene takes: its effects, in order.
#[derive(Debug, PartialEq)]
pub(crate) struct Plan {
    pub copies: Vec<Copy>,
}

pub(crate) enum Verdict {
    Cut(Plan),
    Kept(String),
}

/// What one event of a scene does that a cut has to deal with.
enum Fx {
    Nothing,
    Copy(Copy),
    /// A grant of a gift's item, which `gifts` makes in the caller.
    Grant,
    Block(String),
}

fn has_argument(params: &Params) -> bool {
    params.iter().any(|(_, v)| matches!(v, Value::Argument(_)))
}

/// An effect no cut can copy: a lasting one past `COPIED`, an actor made
/// or ended, another scene.
fn blocks(actor: &str, action: &str) -> bool {
    action == TRANSITION
        || ACTOR_STATE.iter().any(|s| action.contains(s))
        || (lasting(actor, action) && !is_copied(action) && action != WINDOW)
}

/// What a call into another flow does that a cut has to deal with.
fn lasts(actor: &str, action: &str) -> bool {
    blocks(actor, action) || (lasting(actor, action) && action != WINDOW) || is_copied(action)
}

/// What event `e` of `flow` does, given the `items` the scene's gifts grant.
fn effect(flow: &Flowchart, flows: &Flows, items: &[&str], e: u16) -> Fx {
    match &flow.events[e as usize].kind {
        EventKind::Action {
            actor,
            action,
            params,
            ..
        } => {
            let actor = flow.actor_of(*actor);
            let action = actor.actions[*action as usize].as_str();
            let copy = || Copy::Action {
                actor: actor.clone(),
                action: action.to_owned(),
                params: params.clone(),
            };
            let name = param(params, ACTOR);
            if action.starts_with("EventPlayerWarp") {
                Fx::Nothing
            } else if actor.name == GRANT.0 && action == GRANT.1 {
                let num = params.iter().find(|(k, _)| k == crate::flows::NUM);
                let num = num.and_then(|(_, v)| match v {
                    Value::Int(v) => v.first().copied(),
                    _ => None,
                });
                match name {
                    Some(n) if items.contains(&n) => Fx::Grant,
                    Some(n) if num.is_none_or(|n| n > 0) => Fx::Block(format!("grants {n}")),
                    _ if has_argument(params) => Fx::Block("takes an argument".into()),
                    _ => Fx::Copy(copy()),
                }
            } else if is_copied(action) {
                // The grant sets its own `IsGet` flag.
                let got = param(params, flows::GAME_DATA)
                    .and_then(|n| n.strip_prefix("IsGet."))
                    .is_some_and(|i| items.contains(&i));
                if got {
                    Fx::Nothing
                } else if has_argument(params) {
                    Fx::Block(format!("{action} takes an argument"))
                } else {
                    Fx::Copy(copy())
                }
            } else if blocks(&actor.name, action) {
                Fx::Block(format!("{}.{action}", actor.name))
            } else {
                Fx::Nothing
            }
        }
        EventKind::SubFlow {
            flowchart,
            entry_point,
            params,
            ..
        } if !flows::is_local(flow, flowchart) => {
            let names = |k: &str| param(params, k).is_some_and(|n| items.contains(&n));
            if [
                flag::ACTOR_NAME,
                flow::REVEIVE_ITEM,
                flow::TAKEN_REWARD_ACTOR,
            ]
            .iter()
            .any(|k| names(k))
            {
                return Fx::Grant;
            }
            if flowchart == LESSON {
                return if items.is_empty() {
                    Fx::Block("teaches a song it doesn't grant".into())
                } else {
                    Fx::Nothing
                };
            }
            let fx = flows.effects_of_call(flowchart, entry_point, lasts);
            match fx.iter().find(|f| !is_copied(&f.action)) {
                Some(f) => Fx::Block(format!("calls {flowchart}::{entry_point}: {}", f.action)),
                None if fx.is_empty() => Fx::Nothing,
                None if has_argument(params) => Fx::Block("a call takes an argument".into()),
                None => Fx::Copy(Copy::Call {
                    flowchart: flowchart.clone(),
                    entry_point: entry_point.clone(),
                    params: params.clone(),
                }),
            }
        }
        EventKind::SubFlowSwitch {
            flowchart,
            entry_point,
            ..
        } if !flows::is_local(flow, flowchart) => {
            let fx = flows.effects_of_call(flowchart, entry_point, lasts);
            if fx.is_empty() {
                Fx::Nothing
            } else {
                Fx::Block(format!("calls {flowchart}::{entry_point}, which branches"))
            }
        }
        _ => Fx::Nothing,
    }
}

/// What a copy is, as text, for telling two apart.
fn sig(copy: &Copy) -> String {
    match copy {
        Copy::Action {
            actor,
            action,
            params,
        } => format!("{}.{action}({})", actor.name, flows::params(params)),
        Copy::Call {
            flowchart,
            entry_point,
            params,
        } => format!("{flowchart}::{entry_point}({})", flows::params(params)),
    }
}

/// The effects the scene copies, in event order, or why it can't. The
/// scene runs `flow` from `entry`, or from its main entry point when it
/// runs a flow of its own, whose other entry points are its variants.
pub(crate) fn plan(
    flow: &Flowchart,
    entry: Option<&str>,
    flows: &Flows,
    items: &[&str],
) -> std::result::Result<Plan, String> {
    let main = flow
        .entry_points
        .iter()
        .find(|e| e.name == entry.unwrap_or(ENTRY));
    let starts: Vec<Option<u16>> = match main {
        Some(m) => vec![m.main],
        None => flow.entry_points.iter().map(|e| e.main).collect(),
    };
    // An entry point picks one scene from a flow several share, like the
    // Fire Temple Gorons' rescues in `scene::FIRE_TEMPLE_0020`: its other entry
    // points are the others' scenes, not variants.
    let main = main.filter(|_| entry.is_none());
    let reach: BTreeSet<u16> = starts
        .iter()
        .flat_map(|s| flows::reachable_from(flow, *s))
        .collect();
    // What running from `start` copies, to compare the sides of a choice.
    let sigs = |start: u16| -> BTreeSet<String> {
        flows::reachable_from(flow, Some(start))
            .into_iter()
            .filter_map(|e| match effect(flow, flows, items, e) {
                Fx::Copy(c) => Some(sig(&c)),
                Fx::Grant => Some("grant".into()),
                _ => None,
            })
            .collect()
    };
    // Entry points the main one never runs are variants of the scene.
    for ep in flow.entry_points.iter().filter(|_| main.is_some()) {
        let others = flows::reachable_from(flow, ep.main);
        let acts = |e: &u16| !matches!(effect(flow, flows, items, *e), Fx::Nothing);
        if others.difference(&reach).any(acts) {
            return Err(format!(
                "its entry point {} has effects of its own",
                ep.name
            ));
        }
    }
    let mut copies: Vec<Copy> = Vec::new();
    for &e in &reach {
        if let EventKind::Switch {
            actor,
            query,
            cases,
            ..
        } = &flow.events[e as usize].kind
            && flow.actor_of(*actor).queries[*query as usize].contains(CHOICE)
        {
            let mut sides = cases.iter().map(|c| sigs(c.1));
            let first = sides.next();
            if sides.any(|s| Some(&s) != first.as_ref()) {
                return Err(format!(
                    "a choice at event {e} has different effects on each side"
                ));
            }
        }
        match effect(flow, flows, items, e) {
            Fx::Block(why) => return Err(why),
            Fx::Copy(c) if !copies.iter().any(|d| sig(d) == sig(&c)) => copies.push(c),
            _ => {}
        }
    }
    Ok(Plan { copies })
}

/// Whether `name` has a skip of its own, which the cut leaves alone.
fn owned(name: &str) -> bool {
    cutscenes::owned(name)
}

/// What the rule does with `c`, needing `all` to read its flow and its
/// callers. The area's setting isn't asked.
pub(crate) fn judge(all: &Flows, c: &Cutscene) -> Verdict {
    let kept = |why: &str| Verdict::Kept(why.to_owned());
    if let Some((_, why)) = KEPT.iter().find(|k| c.name == *k.0) {
        return kept(why);
    }
    if owned(&c.name) {
        return kept("has a skip of its own");
    }
    if c.area.is_none() {
        return kept("is a system scene");
    }
    if !c.flag {
        return kept("has no played flag");
    }
    let gifts = gifts::of_scene(&c.name);
    if gifts.len() > 1 {
        return kept("hands over more than one check");
    }
    let items: Vec<&str> = gifts.iter().flat_map(|g| g.items()).collect();
    let copies = match all.resolve(&c.name).as_slice() {
        [] => Vec::new(),
        // A scene running another's flow runs it from the main entry point.
        [(flow, entry)] => match plan(
            &all.0[*flow],
            entry.or((*flow != c.name).then_some(ENTRY)),
            all,
            &items,
        ) {
            Ok(p) => p.copies,
            Err(why) => return Verdict::Kept(why),
        },
        _ => return kept("names an entry point in several flows"),
    };
    // A copied call runs under its caller's event, which binds only its own
    // flow and the ones its `SubFlowList` names. The Ice Cavern chest's event
    // binds no others, so a copied `AllSage` call stopped it starting.
    let foreign = copies.iter().find_map(|copy| match copy {
        Copy::Call { flowchart, .. } if c.flows.iter().any(|f| f != flowchart) => Some(flowchart),
        _ => None,
    });
    if let Some(flowchart) = foreign {
        return Verdict::Kept(format!(
            "calls {flowchart}, which its caller's event doesn't bind"
        ));
    }
    if c.flows.is_empty() {
        if !copies.is_empty() || !gifts.is_empty() {
            return kept("nothing starts it that its effects can be copied into");
        }
    } else {
        if c.map {
            return kept("the map starts it as well");
        }
        for f in &c.flows {
            let plays = all.0.get(f).map(|f| gifts::scene_marks(f, &c.name).0.len());
            if plays.is_none_or(|n| n == 0) {
                return Verdict::Kept(format!("{f} has no transition to it"));
            }
        }
    }
    Verdict::Cut(Plan { copies })
}

/// Swap each of `flow`'s transitions to `scene` for its played flag, then the
/// scene's `copies` after it, before whatever followed the transition.
pub(crate) fn swap(flow: &mut Flowchart, scene: &str, copies: &[Copy]) -> Result<()> {
    arrivals::swap(flow, scene, &format!("{}.{scene}", cutscenes::PLAYED))?;
    for at in gifts::scene_marks(flow, scene).1 {
        let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
            unreachable!("a swapped transition is an action");
        };
        let mut first = next;
        for copy in copies.iter().rev() {
            let kind = match copy {
                Copy::Action {
                    actor,
                    action,
                    params,
                } => {
                    let (actor, action) = flow.action(actor, action);
                    EventKind::Action {
                        next: first,
                        actor,
                        action,
                        params: params.clone(),
                    }
                }
                Copy::Call {
                    flowchart,
                    entry_point,
                    params,
                } => EventKind::SubFlow {
                    next: first,
                    params: params.clone(),
                    flowchart: flowchart.clone(),
                    entry_point: entry_point.clone(),
                },
            };
            first = Some(flow.push(kind));
        }
        flow.link(Link::Next(at), first)?;
    }
    Ok(())
}
