//! Talon's cucco game gives its milk through a bottle: it asks for an empty
//! one before the game starts and again before the prize, and it fills that
//! bottle with its own calls. A seed that deals the check another item has
//! no bottle to fill or ask for, so the game runs without one and every
//! milk call becomes the plain hand-over, which the third already is.

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{EventKind, Flowchart};

use super::{Call, HAND_OVER, call, calls_to, get, said};
use crate::known::{Name, field, flow, item};

pub(super) const FLOW: &Name = flow::MINIGAME_FIND_OUT_CUCCO;
/// The plain hand-over, and the game's one call to it.
pub(super) const CALL: &[Call] = &[HAND_OVER];
/// What the game gives, the gift's vanilla item.
pub(super) const MILK: &Name = item::BOTTLE_MILK;
/// Whether Link holds an empty bottle. Case 0 is yes.
const EMPTY: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_QUERY_IS_ANY_BOTTLE_EMPTY,
);
/// The calls that fill a bottle with the milk.
const FILLS: [Call; 2] = [
    call(
        flow::NPC_COMMON,
        flow::NPC_COMMON_HAND_OVER_NPC_TO_PLAYER_EMPTY_BOTTLE,
    ),
    get(flow::GET_IN_EVENT_IN_BOTTLE),
];

/// Answer every question about an empty bottle with yes, and make each call
/// that fills one the plain hand-over, keeping where it goes next.
pub(super) fn open(flow: &mut Flowchart) -> Result<()> {
    let [plain] = calls_to(flow, CALL)?[..] else {
        bail!("{FLOW} hands over by {} not once", said(CALL));
    };
    for i in calls_to(flow, &FILLS)? {
        let mut kind = flow.events[plain].kind.clone();
        if let (EventKind::SubFlow { next: to, .. }, EventKind::SubFlow { next, .. }) =
            (&mut kind, &flow.events[i].kind)
        {
            *to = *next;
        }
        flow.events[i].kind = kind;
    }
    let asks: Vec<usize> = (0..flow.events.len())
        .filter(|&i| match &flow.events[i].kind {
            EventKind::Switch { actor, query, .. } => {
                let a = flow.actor_of(*actor);
                a.name == EMPTY.0 && a.queries[*query as usize] == EMPTY.1
            }
            _ => false,
        })
        .collect();
    ensure!(!asks.is_empty(), "{FLOW} never asks for an empty bottle");
    for i in asks {
        let EventKind::Switch { cases, .. } = &mut flow.events[i].kind else {
            unreachable!("asks are switches");
        };
        let yes = cases.iter().find(|c| c.0 == 0).context("no yes case")?.1;
        cases.iter_mut().for_each(|c| c.1 = yes);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::{GIFTS, ITEM_PARAMS, give_all, rename_calls};
    use super::*;
    use crate::flows::{self, param};
    use crate::known::actor;

    const HEART: &Name = actor::OBJ_HEART_PIECE;

    fn nexts(flow: &Flowchart, calls: &[usize]) -> Vec<Option<u16>> {
        let next = |i: &usize| match &flow.events[*i].kind {
            EventKind::SubFlow { next, .. } => *next,
            _ => unreachable!(),
        };
        calls.iter().map(next).collect()
    }

    #[test]
    fn a_moved_milk_runs_without_a_bottle_and_every_milk_call_is_the_plain_one() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, FLOW).unwrap().unwrap();
        let fills = calls_to(&flow, &FILLS).unwrap();
        assert_eq!(fills.len(), 2);
        let after = nexts(&flow, &fills);
        open(&mut flow).unwrap();
        rename_calls(&mut flow, CALL, MILK, HEART).unwrap();
        assert!(calls_to(&flow, &FILLS).unwrap().is_empty());
        assert_eq!(nexts(&flow, &fills), after, "each goes on as before");
        let plain = calls_to(&flow, CALL).unwrap();
        assert_eq!(plain.len(), 3);
        for i in plain {
            let EventKind::SubFlow { params, .. } = &flow.events[i].kind else {
                unreachable!();
            };
            let named: Vec<_> = ITEM_PARAMS
                .iter()
                .filter_map(|k| param(params, k))
                .collect();
            assert_eq!(named, [HEART], "#{i}");
        }
        let mut asks = 0;
        for e in &flow.events {
            if let EventKind::Switch {
                actor,
                query,
                cases,
                ..
            } = &e.kind
            {
                let a = flow.actor_of(*actor);
                if a.name == EMPTY.0 && a.queries[*query as usize] == EMPTY.1 {
                    assert!(cases.iter().all(|c| c.1 == cases[0].1), "{cases:?}");
                    asks += 1;
                }
            }
        }
        assert_eq!(asks, 3);
    }

    #[test]
    fn a_dealt_check_is_written_open_and_a_vanilla_one_is_left_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gift = GIFTS.iter().find(|g| g.flow == FLOW).unwrap();
        let flows = crate::events::Flows::load(&romfs).unwrap();
        let give = |item: &str| {
            let drops = HashMap::from([(gift.hash(), crate::checks::Drop::from(item.to_owned()))]);
            let mut rewrites = flows::Rewrites::default();
            give_all(&romfs, &flows, &mut rewrites, &drops, false).unwrap();
            rewrites.get(&flows::rel(FLOW)).cloned()
        };
        assert!(give(MILK).is_none());
        let flow = Flowchart::parse(&give(HEART).unwrap()).unwrap().unwrap();
        assert!(!flows::strings(&flow).contains(&&**MILK));
        assert!(calls_to(&flow, &FILLS).unwrap().is_empty());
    }
}
