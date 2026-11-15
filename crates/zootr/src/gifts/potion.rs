//! The potion shop's bottle tutorial gives a bottle, lets a bug out, and
//! ends once Link catches the bug in that bottle. Until then the shopkeeper
//! only talks about the bug. A seed that deals the check another item may
//! leave Link no bottle to catch it with, so the tutorial ends with the gift.

use anyhow::{Result, bail};
use ounce_formats::{EventKind, Flowchart, Link};

use super::{Call, calls_to, get, said};
use crate::flows::SET_BOOL;
use crate::flows::{self, GAME_DATA, param};
use crate::known::{Name, actor, flag, flow};

pub(super) const FLOW: &Name = actor::NPC_HYLIA_020_SHOP_CHILD;
/// The call that gives the bottle.
pub(super) const CALL: &[Call] = &[get(flow::GET_IN_EVENT_NOT_TURN_FIXED_CAMERA_WARP_LINK)];
/// Set once the bug is caught, the tutorial's end.
pub(super) const CLEARED: &Name = flag::MARKET_ITEM_BOTTLE_TUTORIAL_EVENT_CLEARED;

/// Send the gift's call to the event that sets `CLEARED`, which goes on to
/// the tutorial's last step.
pub(super) fn cleared_with_the_gift(flow: &mut Flowchart) -> Result<()> {
    let sets = flows::running(flow, SET_BOOL);
    let clears: Vec<u16> = sets
        .into_iter()
        .filter(|&i| match &flow.events[i as usize].kind {
            EventKind::Action { params, .. } => param(params, GAME_DATA) == Some(CLEARED),
            _ => false,
        })
        .collect();
    let [clear] = clears[..] else {
        bail!("{FLOW} sets {CLEARED} {} times, not once", clears.len());
    };
    let gives = calls_to(flow, CALL)?;
    if gives.is_empty() {
        bail!("{FLOW} never calls {}", said(CALL));
    }
    for i in gives {
        flow.link(Link::Next(i as u16), Some(clear))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::{GIFTS, give_all};
    use super::*;
    use crate::known::flag;

    fn next(flow: &Flowchart, i: usize) -> Option<u16> {
        match &flow.events[i].kind {
            EventKind::SubFlow { next, .. } => *next,
            _ => unreachable!("calls are sub-flows"),
        }
    }

    #[test]
    fn a_dealt_bottle_ends_the_tutorial_and_a_vanilla_one_is_left_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gift = GIFTS.iter().find(|g| g.flow == FLOW).unwrap();
        let flows = crate::events::Flows::load(&romfs).unwrap();
        let give = |item: &str| {
            let drops = HashMap::from([(gift.hash(), crate::checks::Drop::from(item.to_owned()))]);
            let mut rewrites = flows::Rewrites::default();
            give_all(&romfs, &flows, &mut rewrites, &drops).unwrap();
            rewrites.get(&flows::rel(FLOW)).cloned()
        };
        assert!(give(gift.item).is_none());
        let flow = Flowchart::parse(&give("Item_Nut").unwrap())
            .unwrap()
            .unwrap();
        // The dealt gift's call is the plain one, so find it by its item.
        let gives: Vec<usize> = (0..flow.events.len())
            .filter(|&i| match &flow.events[i].kind {
                EventKind::SubFlow { params, .. } => {
                    param(params, flag::ACTOR_NAME) == Some("Item_Nut")
                }
                _ => false,
            })
            .collect();
        assert!(!gives.is_empty());
        for i in gives {
            let to = next(&flow, i).expect("the gift goes on");
            let EventKind::Action { params, .. } = &flow.events[to as usize].kind else {
                panic!("#{i} goes to #{to}, not an action");
            };
            assert_eq!(param(params, GAME_DATA), Some(&**CLEARED), "#{i}");
        }
    }
}
