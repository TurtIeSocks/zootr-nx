//! The potion shop's bottle tutorial gives a bottle, lets a bug out, and
//! ends once Link catches the bug in that bottle. Until then the shopkeeper
//! only talks about the bug. A seed that deals the check another item may
//! leave Link no bottle to catch it with, so the tutorial ends with the gift.
//! The catch also puts the bottled bug on the shelf, a shop slot of its own,
//! so the gift does that too.

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
/// Set as the bug is caught: the shelf slot selling a bottled bug shows.
const ON_SHELF: &Name = flag::MARKET_ITEM_BOTTLE_TUTORIAL_EVENT_SHOP_BOTTLE_INSECT_APPEARED;
/// What the tutorial leaves set: begun, ended, and the bug's bottle out on
/// the shelf. Begun and not ended, the shop door stays shut from inside and
/// the keeper talks only about the bug. Setting these skips the gift, so
/// only a harness does it, never a seed.
pub(crate) const DONE: [&Name; 3] = [
    flag::MARKET_ITEM_BOTTLE_TUTORIAL_EVENT_STARTED,
    CLEARED,
    ON_SHELF,
];

/// Send the gift's call to a copy of the event that sets `ON_SHELF`, which
/// goes on to the one setting `CLEARED`, then the tutorial's last step.
pub(super) fn cleared_with_the_gift(flow: &mut Flowchart) -> Result<()> {
    let clear = sets(flow, CLEARED)?;
    let mut on_shelf = flow.events[sets(flow, ON_SHELF)? as usize].kind.clone();
    let EventKind::Action { next, .. } = &mut on_shelf else {
        unreachable!("a setter is an action");
    };
    *next = Some(clear);
    let on_shelf = flow.push(on_shelf);
    let gives = calls_to(flow, CALL)?;
    if gives.is_empty() {
        bail!("{FLOW} never calls {}", said(CALL));
    }
    for i in gives {
        flow.link(Link::Next(i as u16), Some(on_shelf))?;
    }
    Ok(())
}

/// The one event in `flow` that sets `name`.
fn sets(flow: &Flowchart, name: &Name) -> Result<u16> {
    let found: Vec<u16> = (flows::running(flow, SET_BOOL).into_iter())
        .filter(|&i| match &flow.events[i as usize].kind {
            EventKind::Action { params, .. } => param(params, GAME_DATA) == Some(name),
            _ => false,
        })
        .collect();
    let [one] = found[..] else {
        bail!("{FLOW} sets {name} {} times, not once", found.len());
    };
    Ok(one)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::{GIFTS, give_all};
    use super::*;
    use crate::known::{flag, item};

    fn next(flow: &Flowchart, i: usize) -> Option<u16> {
        match &flow.events[i].kind {
            EventKind::SubFlow { next, .. } => *next,
            _ => unreachable!("calls are sub-flows"),
        }
    }

    #[test]
    fn a_dealt_bottle_shelves_the_bug_and_ends_the_tutorial_and_a_vanilla_one_is_left_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gift = GIFTS.iter().find(|g| g.flow == FLOW).unwrap();
        let flows = crate::events::Flows::load(&romfs).unwrap();
        let give = |item: &str| {
            let drops = HashMap::from([(gift.hash(), crate::checks::Drop::from(item.to_owned()))]);
            let mut rewrites = flows::Rewrites::default();
            give_all(&romfs, &flows, &mut rewrites, &drops, false, &Default::default()).unwrap();
            rewrites.get(&flows::rel(FLOW)).cloned()
        };
        assert!(give(gift.item).is_none());
        let flow = Flowchart::parse(&give(item::DEKU_NUT).unwrap())
            .unwrap()
            .unwrap();
        // The dealt gift's call is the plain one, so find it by its item.
        let gives: Vec<usize> = (0..flow.events.len())
            .filter(|&i| match &flow.events[i].kind {
                EventKind::SubFlow { params, .. } => {
                    param(params, flag::ACTOR_NAME) == Some(&**item::DEKU_NUT)
                }
                _ => false,
            })
            .collect();
        assert!(!gives.is_empty());
        // It puts the bottled bug on the shelf, then ends the tutorial.
        let set = |at: u16| match &flow.events[at as usize].kind {
            EventKind::Action { params, next, .. } => (param(params, GAME_DATA), *next),
            _ => panic!("#{at} isn't an action"),
        };
        for i in gives {
            let to = next(&flow, i).expect("the gift goes on");
            let (shelf, then) = set(to);
            assert_eq!(shelf, Some(&**ON_SHELF), "#{i}");
            assert_eq!(set(then.unwrap()).0, Some(&**CLEARED), "#{i}");
        }
    }
}
