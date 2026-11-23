//! Masks Link keeps, as Ship of Harkinian's randomizer has them: the
//! Happy Mask Shop lends a mask without taking back the one Link holds,
//! and a buyer pays for a mask without taking it. The pouch and the
//! Equipment page's mask slot already hold all eight.

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Flowchart, Romfs};

use crate::flows::{self, param};
use crate::flows::Rewrites;
use crate::known::{actor, field, flow};

/// What a borrow and a sale run, every seed.
pub fn keep(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, flow::DISPLAY_SHOP, |shop| {
        skip_action(shop, flow::EVENT_TRIGGER_REMOVE_OWNED_MASK)?;
        skip_exchange(shop)
    })?;
    flows::edit(romfs, rewrites, flow::NPC_COMMON, |common| {
        let sell = (common.entry_points.iter())
            .find(|e| e.name == **flow::SELL_MASK)
            .context("no SellMask")?;
        let takes = (flows::reachable_from(common, sell.main).into_iter())
            .filter(|&e| is_action(common, e, flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT));
        let takes: Vec<_> = takes.collect();
        ensure!(takes.len() == 1, "SellMask takes {} times", takes.len());
        skip(common, takes[0])
    })
}

/// Whether event `e` runs `action`.
fn is_action(flow: &Flowchart, e: u16, action: &str) -> bool {
    match &flow.events[e as usize].kind {
        EventKind::Action { actor, action: a, .. } => {
            flow.actor_of(*actor).actions[*a as usize] == action
        }
        _ => false,
    }
}

/// Run the one event that runs `action` as though it weren't there.
fn skip_action(flow: &mut Flowchart, action: &str) -> Result<()> {
    let at: Vec<u16> = (0..flow.events.len() as u16)
        .filter(|&e| is_action(flow, e, action))
        .collect();
    ensure!(at.len() == 1, "{action} runs {} times", at.len());
    skip(flow, at[0])
}

/// Point everything that runs action `e` at what follows it.
fn skip(flow: &mut Flowchart, e: u16) -> Result<()> {
    let EventKind::Action { next, .. } = flow.events[e as usize].kind else {
        unreachable!("only actions are skipped");
    };
    for link in flow.links_to(e) {
        flow.link(link, next)?;
    }
    Ok(())
}

/// The shop asks "You already have an item checked out. Did you want to
/// exchange it?" when Link holds a mask. Whatever runs that line goes
/// where its Yes goes, so a borrow just goes on.
fn skip_exchange(shop: &mut Flowchart) -> Result<()> {
    let line = format!(
        "{}/{}:{}",
        field::EVENT_FLOW_MSG,
        actor::NPC_HYLIA_023_SHOP_CHILD,
        field::TALK_30
    );
    let asks: Vec<u16> = (0..shop.events.len() as u16)
        .filter(|&e| match &shop.events[e as usize].kind {
            EventKind::Action { params, .. } => param(params, field::MESSAGE_ID) == Some(&line),
            _ => false,
        })
        .collect();
    ensure!(asks.len() == 2, "the exchange line runs {} times", asks.len());
    for ask in asks {
        let EventKind::Action { next: Some(choice), .. } = shop.events[ask as usize].kind else {
            anyhow::bail!("nothing follows the exchange line");
        };
        let EventKind::Switch { cases, .. } = &shop.events[choice as usize].kind else {
            anyhow::bail!("no choice follows the exchange line");
        };
        let yes = (cases.iter().find(|&&(answer, _)| answer == 0)).map(|&(_, e)| e);
        let yes = yes.context("the exchange has no Yes")?;
        for link in shop.links_to(ask) {
            shop.link(link, Some(yes))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept() -> Option<(Flowchart, Flowchart)> {
        let romfs = crate::game::dev_dump()?;
        let mut rewrites = Rewrites::default();
        keep(&romfs, &mut rewrites).unwrap();
        let read = |name| flows::current(&romfs, &rewrites, name).unwrap();
        Some((read(flow::DISPLAY_SHOP), read(flow::NPC_COMMON)))
    }

    fn runs(flow: &Flowchart, action: &str) -> bool {
        (0..flow.events.len() as u16)
            .any(|e| is_action(flow, e, action) && !flow.links_to(e).is_empty())
    }

    #[test]
    fn a_borrow_takes_nothing_back_and_never_asks_to_exchange() {
        let Some((shop, _)) = kept() else {
            return;
        };
        assert!(!runs(&shop, flow::EVENT_TRIGGER_REMOVE_OWNED_MASK));
        let line = format!(
            "{}/{}:{}",
            field::EVENT_FLOW_MSG,
            actor::NPC_HYLIA_023_SHOP_CHILD,
            field::TALK_30
        );
        let asked = (0..shop.events.len() as u16).filter(|&e| match &shop.events[e as usize].kind {
            EventKind::Action { params, .. } => param(params, field::MESSAGE_ID) == Some(&line),
            _ => false,
        });
        assert!(asked.into_iter().all(|e| shop.links_to(e).is_empty()));
    }

    #[test]
    fn a_sale_leaves_the_mask_with_link() {
        let Some((_, common)) = kept() else {
            return;
        };
        let sell = common.entry_points.iter().find(|e| e.name == **flow::SELL_MASK).unwrap();
        let reached = flows::reachable_from(&common, sell.main);
        let add = flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT;
        assert!(!reached.iter().any(|&e| is_action(&common, e, add)));
        assert!(!reached.is_empty());
    }

    #[test]
    fn rental_starts_with_the_lend() {
        let Some((shop, _)) = kept() else {
            return;
        };
        let lends = |e: u16| is_action(&shop, e, flow::EVENT_TRIGGER_BORROW_MASK_AT_SHOP);
        assert!(shop.entry_points.iter().any(|e| e.main.is_some_and(lends)));
    }
}
