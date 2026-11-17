//! Biggoron's trade chain. Every seed cuts its three countdowns (the
//! mushroom, the frog and the eye drops): the trade slot makes holding
//! several trade items normal, and a countdown punishes that. With
//! `trade-chain` on, each trade is a gift (`gifts::GIFTS`), and [`deal`]
//! and [`by_pouch`] make the chain work in any order. See
//! `docs/superpowers/specs/2026-11-17-trade-checks-design.md`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::ainb::Patch;
use ounce_formats::{Byml, EventKind, Flowchart, Link, Romfs, Value, murmur3};

use crate::checks::Drop;
use crate::flows::{self, IS_BOOL, Rewrites, SET_BOOL};
use crate::gamedata::{self, op};
use crate::gifts::{GIFTS, How, item_at};
use crate::graphs::{logic, node};
use crate::grant::Models;
use crate::known::{Name, actor, field, flag, flow, item, map};

/// The flow holding the chain's shared entries, the countdowns among them.
const CHAIN: &Name = flow::SUB_EVENT_GET_SWORD_BIG_GORON;
/// The entries Grog, King Zora and the scientist call to start a countdown.
const TIMERS: [&Name; 3] = [
    flow::BIG_GORON_TIMER_STEP04,
    flow::BIG_GORON_TIMER_STEP09,
    flow::BIG_GORON_TIMER_STEP10,
];
/// Whether a countdown runs, read from the quest step and the item held,
/// not the timer. Warping, sleeping and changing age read it, and when
/// it's true Navi warns and the trade item is taken.
const COUNTING: &Name = flag::SUB_EVENT_GET_SWORD_BIG_GORON_TIMER_COUNTING_STEP;

/// Each countdown entry runs only the short wait after its timer setting,
/// then ends: no setting, no timer on screen, no start.
pub fn cut_timers(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, CHAIN, |flow| {
        for timer in TIMERS {
            let entry = (flow.entry_points.iter())
                .position(|e| e.name == **timer)
                .with_context(|| format!("no entry {timer}"))?;
            let wait = wait_after(flow, flow.entry_points[entry].main)
                .with_context(|| format!("{timer} doesn't wait after its setting"))?;
            flow.link(Link::Entry(entry), Some(wait))?;
            flow.link(Link::Next(wait), None)?;
        }
        Ok(())
    })
}

/// The wait right after event `setting`, reached from nowhere else.
fn wait_after(flow: &Flowchart, setting: Option<u16>) -> Option<u16> {
    let next = |e: u16| match &flow.events[e as usize].kind {
        EventKind::Action { next, .. } => *next,
        _ => None,
    };
    let wait = next(setting?)?;
    let is_wait = match &flow.events[wait as usize].kind {
        EventKind::Action { actor, action, .. } => {
            flow.actor_of(*actor).actions[*action as usize] == **flow::EVENT_WAIT
        }
        _ => false,
    };
    (is_wait && flow.links_to(wait).len() == 1).then_some(wait)
}

/// No countdown ever runs, so none can cost Link his trade item.
pub fn never_counting(root: &mut Byml) -> Result<()> {
    gamedata::never(root, COUNTING)
}

/// Anju, who hands over the Pocket Egg, then Cojiro once Talon is awake.
const ANJU: &Name = actor::NPC_HYLIA_049_ADULT;
/// The call every trade giver but four hands its item over through.
const HAND_OVER: (&Name, &Name) = (
    flow::NPC_COMMON,
    flow::NPC_COMMON_HAND_OVER_NPC_TO_PLAYER_NO_DISPLAY,
);
/// Talon in Kakariko, then at the ranch once Epona is won. Showing either
/// one the Pocket Cucco gives the Cojiro check, once.
const TALONS: [(&Name, &Name); 2] = [
    (
        actor::NPC_HYLIA_TALON_ADULT_KAKARIKO,
        flow::NPC_HYLIA_TALON_ADULT_SHOW_POCKET_CUCCO,
    ),
    (
        actor::NPC_HYLIA_TALON_ADULT,
        flow::NPC_HYLIA_TALON_ADULT_SHOW_POCKET_CUCCO_LON_LON,
    ),
];
/// Set once either Talon has given his check. A new game adds it.
pub(crate) const TALON_GAVE: &str = "Randomizer_TalonGave";
/// Grog and Fado's spot in the Lost Woods, and who stands there.
const STUMP: &Name = map::LOST_WOOD_ADULT_PREFAB_B5E7;
const GROG: &Name = map::NPC_HYLIA_017_ADULT_7D7E;
const FADO: &Name = map::NPC_KOKIRI_FADO_ADULT_7409;
const SKULL_KID: &Name = map::ENEMY_SKULL_KID_5A60;
/// A system actor the graph always creates and never deletes, so its
/// alive output is always true and its dead one always false.
const ALWAYS: &Name = map::SUPPLY_ITEM_RESET_SYSTEM_ACTOR_56CD;
/// Grog's Presence, negated, and Fado's: a step past Grog's.
const FADO_WAITS: &Name = flag::SUB_EVENT_GET_SWORD_BIG_GORON_IS_AFTER_STEP04_EXP;

/// The chain's flows for a seed with `trade-chain` on, before the gifts
/// rename what each giver hands over. Anju gives only the egg, Talon gives
/// the Cojiro check and takes the Pocket Cucco, and Grog and Fado stand at
/// the stump whatever the quest step.
pub fn deal(romfs: &Romfs, rewrites: &mut Rewrites, drops: &HashMap<u64, Drop>) -> Result<()> {
    flows::edit(romfs, rewrites, ANJU, skip_cojiro)?;
    let talon = GIFTS
        .iter()
        .find(|g| matches!(g.how, How::Added { .. }) && g.item == item::COJIRO_CUCCO)
        .context("no Talon gift")?;
    let given = item_at(drops, talon.hash(), talon.item);
    let models = Models::load(romfs)?;
    for (flow, entry) in TALONS {
        flows::edit(romfs, rewrites, flow, |f| {
            give_once(f, &Models::of(&models.0, &models.1), entry, given)
        })?;
    }
    let rel = logic(STUMP);
    let patched = stump(romfs.read(&rel)?).with_context(|| rel.clone())?;
    rewrites.insert(rel, patched);
    Ok(())
}

/// Anju's Cojiro hand-over is skipped: what led to it goes on to what came
/// after it, so she still takes the Pocket Cucco if Link has it.
fn skip_cojiro(flow: &mut Flowchart) -> Result<()> {
    let calls: Vec<u16> = (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| match &e.kind {
            EventKind::SubFlow {
                flowchart,
                entry_point,
                params,
                ..
            } => {
                (flowchart.as_str(), entry_point.as_str()) == (HAND_OVER.0.get(), HAND_OVER.1.get())
                    && params.iter().any(|(_, v)| v.as_str() == Some(item::COJIRO_CUCCO))
            }
            _ => false,
        })
        .map(|(i, _)| i)
        .collect();
    let [call] = calls[..] else {
        anyhow::bail!("{} hands Cojiro over {} times", flow.name, calls.len());
    };
    let EventKind::SubFlow { next, .. } = flow.events[call as usize].kind else {
        unreachable!("filtered to calls");
    };
    for link in flow.links_to(call) {
        flow.link(link, next)?;
    }
    Ok(())
}

/// `entry` first asks whether [`TALON_GAVE`] is set. If not, it sets it,
/// takes the Pocket Cucco, grants `item` with its window, then runs on as
/// it did.
fn give_once(flow: &mut Flowchart, models: &Models, entry: &Name, item: &str) -> Result<()> {
    let at = (flow.entry_points.iter())
        .position(|e| e.name == **entry)
        .with_context(|| format!("no entry {entry}"))?;
    let was = flow.entry_points[at].main;
    let granted = models.push(flow, Value::String(vec![item.into()]), was)?;
    let took = taking(flow, granted, item::POCKET_CUCCO)?;
    let set = copied(flow, SET_BOOL, Some(took))?;
    let asked = copied(flow, IS_BOOL, None)?;
    if let EventKind::Switch { cases, .. } = &mut flow.events[asked as usize].kind {
        // Case 0 is set.
        *cases = was.into_iter().map(|w| (0, w)).chain([(1, set)]).collect();
    }
    flow.link(Link::Entry(at), Some(asked))
}

/// A copy of `granted`, the grant just pushed, that takes one `item` away,
/// then runs `granted`.
fn taking(flow: &mut Flowchart, granted: u16, item: &str) -> Result<u16> {
    let mut kind = flow.events[granted as usize].kind.clone();
    let params = flows::params_mut(&mut kind);
    flows::set(params, flows::ACTOR, flows::string(item))?;
    flows::set(params, flows::NUM, Value::Int(vec![-1]))?;
    if let EventKind::Action { next, .. } = &mut kind {
        *next = Some(granted);
    }
    Ok(flow.push(kind))
}

/// A copy of the flow's first `what` event, about [`TALON_GAVE`] (set
/// true, for a set), going on to `next`.
fn copied(flow: &mut Flowchart, what: (&Name, &Name), next: Option<u16>) -> Result<u16> {
    let first = *flows::running(flow, what)
        .first()
        .with_context(|| format!("{} has no {}", flow.name, what.1))?;
    let mut kind = flow.events[first as usize].kind.clone();
    let params = flows::params_mut(&mut kind);
    flows::set(params, flows::GAME_DATA, flows::string(TALON_GAVE))?;
    if what == SET_BOOL {
        flows::set(params, flows::VALUE, Value::Bool(vec![true]))?;
    }
    match &mut kind {
        EventKind::Action { next: n, .. } => *n = next,
        EventKind::Switch { cases, .. } => cases.clear(),
        _ => unreachable!("an action or a query"),
    }
    Ok(flow.push(kind))
}

/// The stump's graph creates Grog and Fado whatever the step, so their
/// Presence picks between them, and never the Skull Kid who'd stand there
/// with them.
fn stump(data: Vec<u8>) -> Result<Vec<u8>> {
    let mut patch = Patch::new(data)?;
    let named = |class, name: &Name| node(patch.graph(), (class, field::INSTANCE_NAME), name);
    let always = named(field::ACTOR_LOGIC_SIMPLE_ACTOR, ALWAYS)?;
    let skull_kid = named(field::ACTOR_LOGIC_SIMPLE_ACTOR, SKULL_KID)?;
    let (grog, fado) = (
        named(field::ACTOR_LOGIC_NPC, GROG)?,
        named(field::ACTOR_LOGIC_NPC, FADO)?,
    );
    for npc in [grog, fado] {
        patch.relink(npc, field::LOGIC_CREATE, 0, always, field::LOGIC_ALIVE)?;
    }
    patch.relink(skull_kid, field::LOGIC_CREATE, 0, always, field::DEAD)?;
    Ok(patch.into_bytes())
}

/// Fado waits at the stump while Link holds the Odd Powder and not Cojiro,
/// Grog the rest of the time: each stands there while Link holds his item.
pub fn by_pouch(root: &mut Byml) -> Result<()> {
    let values = gamedata::program(root, FADO_WAITS)?;
    let template = (values.iter())
        .filter_map(|v| v.as_array().ok())
        .find(|i| i.len() == 2)
        .with_context(|| format!("{FADO_WAITS} reads nothing"))?
        .to_vec();
    let stock = |item: &Name| {
        i64::from(murmur3(
            format!("PouchData.{item}.{}", flag::STOCK_NUM).as_bytes(),
        ))
    };
    let ins = |code, operand| gamedata::instruction(&template, code, operand);
    *values = vec![
        ins(op::INT_EQUAL, Some(stock(item::COJIRO_CUCCO)))?,
        ins(op::INT_CONSTANT, Some(0))?,
        ins(op::INT_GREATER, Some(stock(item::ODD_POWDER)))?,
        ins(op::INT_CONSTANT, Some(0))?,
        ins(op::AND, None)?,
    ];
    Ok(())
}

#[cfg(test)]
mod tests {
    use ounce_formats::BymlDoc;

    use super::*;
    use crate::game::dev_dump;
    use crate::gamedata::GameData;

    /// Every event an entry runs, by action name.
    fn runs(flow: &Flowchart, entry: &Name) -> Vec<String> {
        let reached = flows::reachable(flow, entry).unwrap();
        (reached.into_iter())
            .filter_map(|e| match &flow.events[e as usize].kind {
                EventKind::Action { actor, action, .. } => {
                    Some(flow.actor_of(*actor).actions[*action as usize].clone())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_countdown_starts() {
        let Some(romfs) = dev_dump() else { return };
        let before = flows::load(&romfs, CHAIN).unwrap().unwrap();
        let mut rewrites = Rewrites::new();
        cut_timers(&romfs, &mut rewrites).unwrap();
        let after = Flowchart::parse(&rewrites[&flows::rel(CHAIN)])
            .unwrap()
            .unwrap();
        for timer in TIMERS {
            let ran = runs(&before, timer);
            assert!(ran.iter().any(|a| a.contains("Timer")), "{timer}: {ran:?}");
            assert_eq!(
                runs(&after, timer),
                [flow::EVENT_WAIT.to_string()],
                "{timer}"
            );
        }
    }

    /// The chain's rewrites with `item` dealt at Talon.
    fn dealt(romfs: &Romfs, item: &str) -> Rewrites {
        let talon = GIFTS.iter().find(|g| matches!(g.how, How::Added { .. }));
        let drops = HashMap::from([(talon.unwrap().hash(), Drop::from(item.to_string()))]);
        let mut rewrites = Rewrites::new();
        deal(romfs, &mut rewrites, &drops).unwrap();
        rewrites
    }

    fn written(rewrites: &Rewrites, name: &str) -> Flowchart {
        Flowchart::parse(&rewrites[&flows::rel(name)])
            .unwrap()
            .unwrap()
    }

    /// Whether `flow` can reach a hand-over of Cojiro from any entry.
    fn hands_over_cojiro(flow: &Flowchart) -> bool {
        let rendered = flows::render(flow);
        let lines: Vec<&str> = (rendered.lines())
            .filter(|l| l.starts_with('#'))
            .collect();
        (flow.entry_points.iter()).any(|e| {
            let reached = flows::reachable(flow, &e.name).unwrap();
            reached.into_iter().any(|i| {
                let line = lines[i as usize];
                line.contains(HAND_OVER.1.get()) && line.contains(item::COJIRO_CUCCO.get())
            })
        })
    }

    #[test]
    fn anju_hands_over_only_the_egg() {
        let Some(romfs) = dev_dump() else { return };
        let before = flows::load(&romfs, ANJU).unwrap().unwrap();
        assert!(hands_over_cojiro(&before));
        let after = written(&dealt(&romfs, "Item_A"), ANJU);
        assert!(!hands_over_cojiro(&after));
    }

    /// Event `e`'s action and params, when it's an action or a query.
    fn said(flow: &Flowchart, e: u16) -> (String, String) {
        match &flow.events[e as usize].kind {
            EventKind::Action {
                actor,
                action,
                params,
                ..
            } => {
                let a = flow.actor_of(*actor);
                (a.actions[*action as usize].clone(), flows::params(params))
            }
            EventKind::Switch {
                actor,
                query,
                params,
                ..
            } => {
                let a = flow.actor_of(*actor);
                (a.queries[*query as usize].clone(), flows::params(params))
            }
            _ => panic!("event {e} is neither"),
        }
    }

    fn next(flow: &Flowchart, e: u16) -> Option<u16> {
        match flow.events[e as usize].kind {
            EventKind::Action { next, .. } => next,
            _ => panic!("event {e} isn't an action"),
        }
    }

    #[test]
    fn either_talon_gives_his_check_once_and_takes_the_cucco() {
        let Some(romfs) = dev_dump() else { return };
        let rewrites = dealt(&romfs, "Item_A");
        for (name, entry) in TALONS {
            let before = flows::load(&romfs, name).unwrap().unwrap();
            let was = before.entry_points.iter().find(|e| e.name == **entry);
            let was = was.unwrap().main.unwrap();
            let flow = written(&rewrites, name);
            let at = flow.entry_points.iter().find(|e| e.name == **entry);
            let asked = at.unwrap().main.unwrap();
            let (query, params) = said(&flow, asked);
            assert_eq!(query, IS_BOOL.1.get(), "{name}");
            assert!(params.contains(TALON_GAVE), "{name}: {params}");
            let EventKind::Switch { cases, .. } = &flow.events[asked as usize].kind else {
                panic!("{name} asks nothing");
            };
            assert!(cases.contains(&(0, was)), "{name}: set runs on as before");
            let set = cases.iter().find(|c| c.0 == 1).unwrap().1;
            let (action, params) = said(&flow, set);
            assert_eq!(action, SET_BOOL.1.get());
            assert!(params.contains(TALON_GAVE) && params.contains("Value=true"));
            let took = next(&flow, set).unwrap();
            let (action, params) = said(&flow, took);
            assert_eq!(action, flows::GRANT.1.get());
            assert!(params.contains(item::POCKET_CUCCO.get()) && params.contains("Num=-1"), "{params}");
            let granted = next(&flow, took).unwrap();
            let (action, params) = said(&flow, granted);
            assert_eq!(action, flows::GRANT.1.get());
            assert!(params.contains(r#"Actor="Item_A""#), "{params}");
            let mut at = next(&flow, granted);
            for _ in crate::grant::GET_LINES.iter() {
                at = next(&flow, at.unwrap());
            }
            assert_eq!(at, Some(was), "{name}: the window, then as before");
        }
    }

    #[test]
    fn the_stump_creates_grog_and_fado_and_never_its_skull_kid() {
        let Some(romfs) = dev_dump() else { return };
        let rewrites = dealt(&romfs, item::COJIRO_CUCCO);
        let graph = ounce_formats::ainb::Graph::parse(&rewrites[&logic(STUMP)]).unwrap();
        let named = |class, name: &Name| node(&graph, (class, field::INSTANCE_NAME), name).unwrap();
        let always = named(field::ACTOR_LOGIC_SIMPLE_ACTOR, ALWAYS);
        let create = |n: usize| {
            let input = graph.nodes[n].input(field::LOGIC_CREATE).unwrap();
            let [link] = &input.sources[..] else {
                panic!("node {n} creates by {} links", input.sources.len())
            };
            (link.node, link.output.clone())
        };
        let alive = (always, field::LOGIC_ALIVE.to_string());
        assert_eq!(create(named(field::ACTOR_LOGIC_NPC, GROG)), alive);
        assert_eq!(create(named(field::ACTOR_LOGIC_NPC, FADO)), alive);
        let dead = (always, field::DEAD.to_string());
        assert_eq!(create(named(field::ACTOR_LOGIC_SIMPLE_ACTOR, SKULL_KID)), dead);
        // The always-there actor is created on a constant and never deleted.
        let input = |name: &Name| graph.nodes[always].input(name).unwrap().sources.len();
        assert_eq!((input(field::LOGIC_CREATE), input(field::LOGIC_DELETE)), (0, 0));
    }

    #[test]
    fn fado_waits_while_link_holds_the_powder_and_not_cojiro() {
        let Some(romfs) = dev_dump() else { return };
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        let mut gd = GameData::load(&romfs).unwrap();
        let stock = |item: &Name| format!("PouchData.{item}.{}", flag::STOCK_NUM);
        let (cojiro, powder) = (stock(item::COJIRO_CUCCO), stock(item::ODD_POWDER));
        gd.learn(&cojiro);
        gd.learn(&powder);
        assert!(gd.expr(FADO_WAITS).unwrap().unwrap().contains("Step04"));
        by_pouch(&mut doc.root).unwrap();
        let hash = murmur3(FADO_WAITS.as_bytes());
        let table = &doc.root["Data"][flag::BOOL_EXP.get()];
        let entry = (table.as_array().unwrap().iter())
            .find(|e| e["Hash"].as_int::<i64>().is_ok_and(|h| h == i64::from(hash)))
            .unwrap();
        assert_eq!(
            gd.render(entry).unwrap(),
            format!("{cojiro} == 0 && {powder} > 0")
        );
    }

    #[test]
    fn nothing_reads_a_countdown_as_running() {
        let Some(romfs) = dev_dump() else { return };
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        let gd = GameData::load(&romfs).unwrap();
        let before = gd.expr(COUNTING).unwrap().unwrap();
        assert!(before.contains("== Step04"), "{before}");
        never_counting(&mut doc.root).unwrap();
        let hash = ounce_formats::murmur3(COUNTING.as_bytes());
        let table = &doc.root["Data"][flag::BOOL_EXP.get()];
        let entry = (table.as_array().unwrap().iter())
            .find(|e| {
                e["Hash"]
                    .as_int::<i64>()
                    .is_ok_and(|h| h == i64::from(hash))
            })
            .unwrap();
        assert_eq!(
            gd.render(entry).unwrap(),
            format!("({before}) && !({before})")
        );
    }
}
