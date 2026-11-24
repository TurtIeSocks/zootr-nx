//! The weeding Kokiri offers his weeding until he's paid for it. His talk
//! goes to the early talk, the one that asks for the weeding and pays for
//! it, only while the castle and forest stone quests aren't done. After
//! either, he never paid. His talk now asks first whether he's paid: until
//! then it goes to the early talk, and after it runs on as it did.

use anyhow::{Context, Result, bail};
use ounce_formats::{EventKind, Flowchart, Link};

use super::rewrite::bool_switch;
use crate::flows::is_local;
use crate::known::field;

/// `flow`'s talk asks Bool `paid` first, to the early talk while it's unset.
pub(super) fn offered_until_paid(flow: &mut Flowchart, paid: &str) -> Result<()> {
    let at = (flow.entry_points.iter())
        .position(|e| e.name == field::TALK.get())
        .with_context(|| format!("{} has no talk", flow.name))?;
    let first = flow.entry_points[at]
        .main
        .context("the talk runs nothing")?;
    let early = field::GAME_STARTED_TALK.get();
    let calls = |e: &EventKind| {
        matches!(e, EventKind::SubFlow { flowchart, entry_point, .. }
            if is_local(flow, flowchart) && entry_point == early)
    };
    let started: Vec<u16> = ((0u16..).zip(&flow.events))
        .filter(|(_, e)| calls(&e.kind))
        .map(|(i, _)| i)
        .collect();
    let [started] = started[..] else {
        bail!("{} doesn't call its early talk once", flow.name);
    };
    let ask = bool_switch(flow, paid, vec![(0, first), (1, started)])?;
    let ask = flow.push(ask);
    flow.link(Link::Entry(at), Some(ask))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::{self, GAME_DATA};
    use crate::gifts::{GIFTS, WEEDING};

    /// After the edit the talk asks the paid Bool: unset runs the early
    /// talk, set runs the castle and stone questions as before.
    #[test]
    fn the_talk_goes_to_the_weeding_until_it_is_paid() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let paid = (GIFTS.iter().find(|g| g.flow == WEEDING))
            .and_then(|g| g.found())
            .unwrap();
        let mut flow = flows::load(&romfs, WEEDING).unwrap().unwrap();
        let talk = |f: &Flowchart| {
            f.entry_points
                .iter()
                .find(|e| e.name == field::TALK.get())
                .unwrap()
                .main
                .unwrap()
        };
        let was = talk(&flow);
        offered_until_paid(&mut flow, &paid).unwrap();
        let EventKind::Switch { params, cases, .. } = &flow.events[talk(&flow) as usize].kind
        else {
            panic!("the talk doesn't ask first");
        };
        assert_eq!(flows::param(params, GAME_DATA), Some(paid.as_str()));
        let to = |case: u32| cases.iter().find(|c| c.0 == case).unwrap().1;
        assert_eq!(to(0), was, "paid runs the talk as it was");
        let EventKind::SubFlow { entry_point, .. } = &flow.events[to(1) as usize].kind else {
            panic!("unpaid runs no call");
        };
        assert_eq!(entry_point, field::GAME_STARTED_TALK.get());
    }
}
