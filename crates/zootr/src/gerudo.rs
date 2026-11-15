//! The Gerudo membership card and the fortress guards. The guards stop
//! watching for Link once the Spirit sage's challenge is past its first
//! step, which the game moves only when the fourth carpenter is freed. Its
//! guard graphs never read the card or the saved step, so a card from a
//! chest, or a save edited to match, leaves them on watch.
//!
//! Every seed checks at each map change: once Link holds the card while the
//! quest is still at its first step, the quest moves on, as freeing the
//! carpenters would. That covers every way of getting it, a shop, a scrub,
//! a gift granted in its own flow, a start item or the dev panel. A seed
//! that moves the card also checks right after every grant the game runs
//! by a variable item, so a card found in the fortress works at once.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{EventKind, Flowchart, Link, Params, Romfs, Value};

use crate::checks::Drop;
use crate::flows::{self, GRANT, IS_BOOL, Model, Rewrites, running, set, string, switch};
use crate::gifts::GIFTS;
use crate::known::{Name, field, flag, flow, item, scene};
use crate::steps;

/// What the Gerudo give.
pub const CARD: &Name = item::TOKEN_GERUDO;
/// The quest the guards read.
const CHALLENGE: &Name = flow::SAGE_OF_SPIRIT;
/// The step past which the guards stand down.
const STAND_DOWN: &str = "Step02";
/// The Banc folders the guards watch.
const GUARDED: [&Name; 2] = [flag::GERUDO_FORTRESS, flag::GERUDO_TRAINING_GROUND];
const IS_EXP: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_QUERY_IS_GAME_DATA_BOOL_EXP,
);
/// True while the quest is past its first step.
const PAST_FIRST: &Name = flag::SAGE_OF_SPIRIT_IS_AFTER_STEP01_EXP;
/// The flows that grant an item they are handed by name: the shared get
/// flow every chest and gift goes through, the cutscene one, and the plain
/// hand-over.
const GRANTERS: [&Name; 3] = [
    flow::GET_POUCH_CONTENT,
    scene::SY_GET_IN_EVENT,
    flow::NPC_COMMON,
];

/// The quest step that stands the guards down, for a spot in `area` they
/// watch: what holding the card does in a seed.
pub fn guard_step(area: &str) -> Option<(String, String)> {
    GUARDED
        .iter()
        .any(|g| **g == *area)
        .then(|| (CHALLENGE.to_string(), STAND_DOWN.into()))
}

/// The shapes the hook copies from the quest's own flow.
struct Models {
    progress: Model,
    is_bool: Model,
    is_exp: Model,
}

impl Models {
    fn load(romfs: &Romfs) -> Result<Self> {
        let flow = steps::template(romfs)?;
        let call = |what: (&Name, &Name)| -> Result<Model> {
            let at = switch(&flow, what)
                .with_context(|| format!("{} never asks {what:?}", flow.name))?;
            flows::model(&flow, what, at)
        };
        Ok(Self {
            progress: steps::progress(&flow)?,
            is_bool: call(IS_BOOL)?,
            is_exp: call(IS_EXP)?,
        })
    }

    /// The progress to `step`, with no banner.
    fn progress(&self, challenge: &str, step: &str) -> Result<Params> {
        steps::moving(&self.progress, string(challenge), string(step))
    }

    /// `query` on the GameData entry `name`.
    fn asking(model: &Model, name: &str) -> Result<Params> {
        let mut params = model.params.clone();
        set(&mut params, flows::GAME_DATA, string(name))?;
        Ok(params)
    }
}

/// Whether the seed holds the card anywhere but its vanilla check.
fn moved(drops: &HashMap<u64, Drop>) -> bool {
    let vanilla = GIFTS.iter().find(|g| g.item == CARD).map(|g| g.hash());
    drops.iter().any(|(hash, d)| {
        let swapped = d.swap.is_some_and(|s| s.then == CARD);
        (d.item == CARD || swapped) && Some(*hash) != vanilla
    })
}

/// Every event in `flow` that grants an item named by a variable.
fn grants(flow: &Flowchart) -> Vec<u16> {
    let named = |e: &u16| match &flow.events[*e as usize].kind {
        EventKind::Action { params, .. } => params
            .iter()
            .any(|(k, v)| k == flows::ACTOR && matches!(v, Value::Argument(_))),
        _ => false,
    };
    running(flow, GRANT).into_iter().filter(named).collect()
}

/// The check: if Link holds the card and the quest is still at its first
/// step, move it on, then go to `after`. Returns the check's first event.
fn check(flow: &mut Flowchart, models: &Models, after: u16) -> Result<u16> {
    let (actor, action) = flow.action(&models.progress.actor, models.progress.name);
    let params = models.progress(CHALLENGE, STAND_DOWN)?;
    let progress = flow.push(EventKind::Action {
        next: Some(after),
        actor,
        action,
        params,
    });
    let (actor, query) = flow.query(&models.is_exp.actor, models.is_exp.name);
    let params = Models::asking(&models.is_exp, PAST_FIRST)?;
    // Case 0 is true.
    let early = flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, after), (1, progress)],
    });
    let (actor, query) = flow.query(&models.is_bool.actor, models.is_bool.name);
    let params = Models::asking(&models.is_bool, &format!("IsGet.{CARD}"))?;
    Ok(flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, early), (1, after)],
    }))
}

/// The check right after each variable grant in `flow`, then on as the
/// flow went.
fn hook(flow: &mut Flowchart, models: &Models) -> Result<()> {
    let sites = grants(flow);
    anyhow::ensure!(!sites.is_empty(), "{} grants nothing by name", flow.name);
    for site in sites {
        let EventKind::Action { next, .. } = flow.events[site as usize].kind else {
            unreachable!("grants finds actions");
        };
        let after = match next {
            Some(next) => next,
            None => {
                flows::ending(flow).with_context(|| format!("{} ends on a grant", flow.name))?
            }
        };
        let head = check(flow, models, after)?;
        flow.link(Link::Next(site), Some(head))?;
    }
    Ok(())
}

/// The check at the head of the map change flow, then the map change.
fn at_map_change(flow: &mut Flowchart, models: &Models) -> Result<()> {
    let (entry, main) = steps::entry(flow)?;
    let head = check(flow, models, main)?;
    flow.link(Link::Entry(entry), Some(head))
}

/// Check at every map change, and after the grant flows if `drops` moves
/// the card, so any way of getting it stands the guards down.
pub fn stand_down(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    drops: &HashMap<u64, Drop>,
) -> Result<()> {
    let models = Models::load(romfs)?;
    flows::edit(romfs, rewrites, steps::BANC, |flow| {
        at_map_change(flow, &models)
    })?;
    if !moved(drops) {
        return Ok(());
    }
    for name in GRANTERS {
        flows::edit(romfs, rewrites, name, |flow| hook(flow, &models))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::only;
    use crate::game::dev_dump;
    use crate::known::{field, flag, flow, item};
    use crate::steps::QUIET;

    const ELSEWHERE: u64 = 1;

    #[test]
    fn the_guard_step_applies_only_where_they_watch() {
        let step = (CHALLENGE.to_string(), STAND_DOWN.to_owned());
        assert_eq!(guard_step(flag::GERUDO_FORTRESS), Some(step.clone()));
        assert_eq!(guard_step(flag::GERUDO_TRAINING_GROUND), Some(step));
        assert_eq!(guard_step(flow::GERUDO_VALLEY), None);
    }

    fn card_at(hash: u64) -> HashMap<u64, Drop> {
        HashMap::from([(hash, Drop::from(CARD.to_string()))])
    }

    #[test]
    fn the_card_is_moved_only_off_its_own_check() {
        let own = GIFTS.iter().find(|g| g.item == CARD).unwrap().hash();
        assert!(moved(&card_at(ELSEWHERE)));
        assert!(!moved(&card_at(own)));
        assert!(!moved(&HashMap::new()));
        let other = HashMap::from([(ELSEWHERE, Drop::from(item::BOTTLE.to_string()))]);
        assert!(!moved(&other));
    }

    #[test]
    fn a_card_left_where_it_is_checks_only_at_map_changes() {
        let Some(romfs) = dev_dump() else { return };
        let own = GIFTS.iter().find(|g| g.item == CARD).unwrap().hash();
        let mut rewrites = Rewrites::new();
        stand_down(&romfs, &mut rewrites, &card_at(own)).unwrap();
        let only: Vec<_> = rewrites.keys().cloned().collect();
        assert_eq!(only, [flows::rel(steps::BANC)]);
    }

    #[test]
    fn every_map_change_checks_the_card_then_changes_the_map() {
        let Some(romfs) = dev_dump() else { return };
        let before = flows::load(&romfs, steps::BANC).unwrap().unwrap();
        let (_, was) = steps::entry(&before).unwrap();
        let mut rewrites = Rewrites::new();
        stand_down(&romfs, &mut rewrites, &HashMap::new()).unwrap();
        let flow = Flowchart::parse(&rewrites[&flows::rel(steps::BANC)])
            .unwrap()
            .unwrap();
        let (_, head) = steps::entry(&flow).unwrap();
        assert_chain(&flow, head, was);
        assert_eq!(Flowchart::parse(&flow.to_bytes()).unwrap().unwrap(), flow);
    }

    /// The check at `head` in `flow`: whether Link holds the card, then
    /// whether the quest is past its first step, then the progress, every
    /// way out going on to `after`.
    fn assert_chain(flow: &Flowchart, head: u16, after: u16) {
        let (held, early, progress, exits) = chain(flow, head);
        assert_eq!(held, format!("IsGet.{CARD}"));
        assert_eq!(early, PAST_FIRST);
        assert_eq!(flows::param(&progress, field::STEP_NAME), Some(STAND_DOWN));
        assert_eq!(
            flows::param(&progress, field::CHALLENGE_NAME),
            Some(&**CHALLENGE)
        );
        assert!(progress.contains(&(QUIET.to_string(), Value::Bool(vec![true]))));
        assert_eq!(exits, [after; 3], "no card, past it, and moved on");
    }

    /// The check at `head`: what it asks first and second, the progress,
    /// and where each way out goes.
    fn chain(flow: &Flowchart, head: u16) -> (String, String, Params, [u16; 3]) {
        let at = |e: u16| &flow.events[e as usize].kind;
        let EventKind::Switch {
            params: held,
            cases,
            ..
        } = at(head)
        else {
            panic!("no switch at the head")
        };
        let EventKind::Switch {
            params: early,
            cases: early_cases,
            ..
        } = at(cases[0].1)
        else {
            panic!("no second switch")
        };
        let EventKind::Action { params, next, .. } = at(early_cases[1].1) else {
            panic!("no progress")
        };
        let name = |p: &Params| flows::param(p, flows::GAME_DATA).unwrap().to_owned();
        let exits = [cases[1].1, early_cases[0].1, next.unwrap()];
        (name(held), name(early), params.clone(), exits)
    }

    #[test]
    fn a_moved_card_moves_the_quest_after_every_named_grant() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        stand_down(&romfs, &mut rewrites, &card_at(ELSEWHERE)).unwrap();
        for name in GRANTERS {
            let flow = Flowchart::parse(&rewrites[&flows::rel(name)])
                .unwrap()
                .unwrap();
            let sites = grants(&flow);
            assert!(!sites.is_empty(), "{name}");
            for site in sites {
                let EventKind::Action { next, .. } = &flow.events[site as usize].kind else {
                    panic!("not an action")
                };
                let (.., exits) = chain(&flow, next.unwrap());
                assert_chain(&flow, next.unwrap(), exits[0]);
            }
        }
    }

    #[test]
    fn a_hooked_flow_rejoins_where_it_was() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        stand_down(&romfs, &mut rewrites, &card_at(ELSEWHERE)).unwrap();
        let before = flows::load(&romfs, flow::GET_POUCH_CONTENT)
            .unwrap()
            .unwrap();
        let after = Flowchart::parse(&rewrites[&flows::rel(flow::GET_POUCH_CONTENT)])
            .unwrap()
            .unwrap();
        let site = only(&before, GRANT).unwrap();
        let EventKind::Action { next: was, .. } = &before.events[site as usize].kind else {
            unreachable!()
        };
        let EventKind::Action { next, .. } = &after.events[site as usize].kind else {
            unreachable!()
        };
        let EventKind::Switch { cases, .. } = &after.events[next.unwrap() as usize].kind else {
            panic!("no switch")
        };
        assert_eq!(Some(cases[1].1), *was, "no card goes straight on");
    }
}
