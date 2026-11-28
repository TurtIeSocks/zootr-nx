//! The rock circle Kokiri pays for his cleared circle until he's paid. His
//! talk goes to the early talk, the one that pays, only while the castle
//! and forest stone quests aren't done. After either, a circle cleared
//! late never paid. His talk now asks first whether he's paid and whether
//! the circle is clear: unpaid with every rock broken runs his own reward
//! talk, then sets his paid Bool. Otherwise it runs on as it did.

use anyhow::{Context, Result};
use ounce_formats::{EventKind, Flowchart, Link};

use super::rewrite::{bool_switch, set_true};
use crate::flows::is_local;
use crate::known::{actor, field, flag};

/// The rock circle Kokiri's flow.
pub(super) const FLOW: &crate::known::Name = actor::NPC_KOKIRI_008_CHILD;

/// `flow`'s talk asks Bool `paid`, then whether the circle is clear, and
/// pays once through the reward talk while it's unpaid and clear.
pub(super) fn paid_once_cleared(flow: &mut Flowchart, paid: &str) -> Result<()> {
    let at = (flow.entry_points.iter())
        .position(|e| e.name == field::TALK.get())
        .with_context(|| format!("{} has no talk", flow.name))?;
    let first = flow.entry_points[at]
        .main
        .context("the talk runs nothing")?;
    let reward = field::LIFT_ROCK_ALL_BROKEN_TALK.get();
    let calls_reward = |e: &EventKind| {
        matches!(e, EventKind::SubFlow { flowchart, entry_point, .. }
            if is_local(flow, flowchart) && entry_point == reward)
    };
    let mut call = (flow.events.iter())
        .find(|e| calls_reward(&e.kind))
        .with_context(|| format!("{} never calls its reward talk", flow.name))?
        .kind
        .clone();
    let pay = set_true(flow, paid, None)?;
    if let EventKind::SubFlow { next, .. } = &mut call {
        *next = Some(pay);
    }
    let call = flow.push(call);
    let clear = flag::KOKIRI_FOREST_NPC_KOKIRI_008_CHILD_LIFT_ROCK_ALL_BROKEN.get();
    let clear = bool_switch(flow, clear, vec![(0, call), (1, first)])?;
    let clear = flow.push(clear);
    let ask = bool_switch(flow, paid, vec![(0, first), (1, clear)])?;
    let ask = flow.push(ask);
    flow.link(Link::Entry(at), Some(ask))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::{self, GAME_DATA};
    use crate::gifts::GIFTS;

    fn switch(flow: &Flowchart, at: u16) -> (String, u16, u16) {
        let EventKind::Switch { params, cases, .. } = &flow.events[at as usize].kind else {
            panic!("#{at} asks nothing");
        };
        let to = |case: u32| cases.iter().find(|c| c.0 == case).unwrap().1;
        let name = flows::param(params, GAME_DATA).unwrap().to_owned();
        (name, to(0), to(1))
    }

    /// Paid runs the talk as it was. Unpaid asks the circle: clear runs the
    /// reward talk and sets paid, not clear runs the talk as it was.
    #[test]
    fn a_cleared_circle_pays_once_after_the_story_moves_on() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let paid = (GIFTS.iter().find(|g| g.flow == FLOW))
            .and_then(|g| g.found())
            .unwrap();
        let mut flow = flows::load(&romfs, FLOW).unwrap().unwrap();
        let talk = |f: &Flowchart| {
            (f.entry_points.iter())
                .find(|e| e.name == field::TALK.get())
                .unwrap()
                .main
                .unwrap()
        };
        let was = talk(&flow);
        paid_once_cleared(&mut flow, &paid).unwrap();

        let (asked, is_paid, unpaid) = switch(&flow, talk(&flow));
        assert_eq!(asked, paid);
        assert_eq!(is_paid, was, "paid runs the talk as it was");
        let (asked, clear, not_clear) = switch(&flow, unpaid);
        let all_broken = flag::KOKIRI_FOREST_NPC_KOKIRI_008_CHILD_LIFT_ROCK_ALL_BROKEN;
        assert_eq!(asked, all_broken.get());
        assert_eq!(
            not_clear, was,
            "an uncleared circle runs the talk as it was"
        );
        let EventKind::SubFlow {
            entry_point, next, ..
        } = &flow.events[clear as usize].kind
        else {
            panic!("a clear circle runs no call");
        };
        assert_eq!(entry_point, field::LIFT_ROCK_ALL_BROKEN_TALK.get());
        let EventKind::Action { params, next, .. } = &flow.events[next.unwrap() as usize].kind
        else {
            panic!("the reward sets nothing after");
        };
        assert_eq!(flows::param(params, GAME_DATA), Some(paid.as_str()));
        assert_eq!(*next, None);
    }
}
