//! Masks Link keeps, as Ship of Harkinian's randomizer has them: the
//! Happy Mask Shop lends a mask without taking back the one Link holds,
//! and a buyer pays for a mask without taking it. The pouch and the
//! Equipment page's mask slot already hold all eight. With
//! `shuffle-masks` the eight join the pool and the keeper lends none.

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Flowchart, Romfs};

use crate::flows::Rewrites;
use crate::flows::{self, param};
use crate::known::{Name, actor, field, flow, item};
use crate::settings::{NewGame, SHUFFLE_MASKS};

/// The eight masks, as the pool holds them.
pub const ALL: [&Name; 8] = [
    item::MASK_KEATON,
    item::MASK_SKULL,
    item::MASK_SPOOKY,
    item::MASK_BUNNY,
    item::MASK_GORON,
    item::MASK_ZORA,
    item::MASK_GERUDO,
    item::MASK_TRUTH,
];

/// The masks `new_game` adds to the pool.
pub fn pooled(new_game: &NewGame) -> Vec<&'static str> {
    let on = new_game.on.contains(SHUFFLE_MASKS);
    ALL.iter().filter(|_| on).map(|m| m.get()).collect()
}

/// What the mask shop and the buyers run, for `new_game`.
pub fn write(romfs: &Romfs, new_game: &NewGame, rewrites: &mut Rewrites) -> Result<()> {
    keep(romfs, rewrites)?;
    if new_game.on.contains(SHUFFLE_MASKS) {
        close_shop(romfs, rewrites)?;
    }
    Ok(())
}

/// What a borrow and a sale run, every seed.
fn keep(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, flow::DISPLAY_SHOP, |shop| {
        skip_action(shop, flow::EVENT_TRIGGER_REMOVE_OWNED_MASK)?;
        skip_exchange(shop)
    })?;
    flows::edit(romfs, rewrites, flow::NPC_COMMON, |common| {
        let takes = (flows::reachable_from(common, sell_start(common)?).into_iter())
            .filter(|&e| is_action(common, e, flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT));
        let takes: Vec<_> = takes.collect();
        ensure!(takes.len() == 1, "SellMask takes {} times", takes.len());
        flows::step_over(common, takes[0])
    })
}

/// Where a buyer's SellMask starts.
fn sell_start(common: &Flowchart) -> Result<Option<u16>> {
    let sell = (common.entry_points.iter()).find(|e| e.name == **flow::SELL_MASK);
    Ok(sell.context("no SellMask")?.main)
}

/// The keeper talks but never opens the shelf: each call into the shop
/// goes on to the goodbye that follows it, as Ship's missing keeper lends
/// nothing.
fn close_shop(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, actor::NPC_HYLIA_023_SHOP_CHILD, |keeper| {
        let calls = shop_calls(keeper);
        ensure!(!calls.is_empty(), "the keeper never opens the shop");
        for e in calls {
            flows::step_over(keeper, e)?;
        }
        Ok(())
    })
}

/// The events of `flow` that `pick` picks.
fn events_where(flow: &Flowchart, pick: impl Fn(&EventKind) -> bool) -> Vec<u16> {
    (0..flow.events.len() as u16)
        .filter(|&e| pick(&flow.events[e as usize].kind))
        .collect()
}

/// The keeper's events that open the shop's shelf.
fn shop_calls(keeper: &Flowchart) -> Vec<u16> {
    events_where(keeper, |kind| match kind {
        EventKind::SubFlow {
            flowchart,
            entry_point,
            ..
        } => flowchart == flow::DISPLAY_SHOP && entry_point == flow::ROOT_HAPPY_MASK_SHOP,
        _ => false,
    })
}

/// Whether event `e` runs `action`.
fn is_action(flow: &Flowchart, e: u16, action: &str) -> bool {
    match &flow.events[e as usize].kind {
        EventKind::Action {
            actor, action: a, ..
        } => flow.actor_of(*actor).actions[*a as usize] == action,
        _ => false,
    }
}

/// Run the one event that runs `action` as though it weren't there.
fn skip_action(flow: &mut Flowchart, action: &str) -> Result<()> {
    let at: Vec<u16> = (0..flow.events.len() as u16)
        .filter(|&e| is_action(flow, e, action))
        .collect();
    ensure!(at.len() == 1, "{action} runs {} times", at.len());
    flows::step_over(flow, at[0])
}

/// The events that ask "You already have an item checked out. Did you
/// want to exchange it?". The shop runs that line twice.
fn exchange_asks(shop: &Flowchart) -> Vec<u16> {
    let line = format!(
        "{}/{}:{}",
        field::EVENT_FLOW_MSG,
        actor::NPC_HYLIA_023_SHOP_CHILD,
        field::TALK_30
    );
    events_where(shop, |kind| match kind {
        EventKind::Action { params, .. } => param(params, field::MESSAGE_ID) == Some(&line),
        _ => false,
    })
}

/// The shop asks "You already have an item checked out. Did you want to
/// exchange it?" when Link holds a mask. Whatever runs that line goes
/// where its Yes goes, so a borrow just goes on.
fn skip_exchange(shop: &mut Flowchart) -> Result<()> {
    let asks = exchange_asks(shop);
    ensure!(
        asks.len() == 2,
        "the exchange line runs {} times",
        asks.len()
    );
    for ask in asks {
        let EventKind::Action {
            next: Some(choice), ..
        } = shop.events[ask as usize].kind
        else {
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

    /// The shop, the buyers' common flow and the keeper, as a seed with
    /// `on` writes them.
    fn written(on: &[&str]) -> Option<[Flowchart; 3]> {
        let romfs = crate::game::dev_dump()?;
        let mut new_game = NewGame::default();
        new_game.on.extend(on.iter().map(|s| s.to_string()));
        let mut rewrites = Rewrites::default();
        write(&romfs, &new_game, &mut rewrites).unwrap();
        let read = |name| flows::current(&romfs, &rewrites, name).unwrap();
        let keeper = read(actor::NPC_HYLIA_023_SHOP_CHILD);
        Some([read(flow::DISPLAY_SHOP), read(flow::NPC_COMMON), keeper])
    }

    fn kept() -> Option<(Flowchart, Flowchart)> {
        let [shop, common, _] = written(&[])?;
        Some((shop, common))
    }

    fn opens_shop(keeper: &Flowchart) -> bool {
        shop_calls(keeper)
            .iter()
            .any(|&e| !keeper.links_to(e).is_empty())
    }

    #[test]
    fn the_keeper_lends_only_with_masks_unshuffled() {
        let (Some([_, _, open]), Some([_, _, closed])) = (written(&[]), written(&[SHUFFLE_MASKS]))
        else {
            return;
        };
        assert!(opens_shop(&open));
        assert!(!opens_shop(&closed));
    }

    #[test]
    fn only_the_setting_pools_the_masks() {
        let mut new_game = NewGame::default();
        assert!(pooled(&new_game).is_empty());
        new_game.on.insert(SHUFFLE_MASKS.into());
        assert_eq!(pooled(&new_game).len(), 8);
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
        let asked = exchange_asks(&shop);
        assert_eq!(asked.len(), 2);
        assert!(asked.into_iter().all(|e| shop.links_to(e).is_empty()));
    }

    #[test]
    fn a_sale_leaves_the_mask_with_link() {
        let Some((_, common)) = kept() else {
            return;
        };
        let reached = flows::reachable_from(&common, sell_start(&common).unwrap());
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
