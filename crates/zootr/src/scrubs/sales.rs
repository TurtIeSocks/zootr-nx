//! The edits to the shared scrub flow: what each edited spot sells, the
//! plain sale for items with no get animation, and the entries spots talk
//! through.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{EntryPoint, EventKind, Flowchart, Link, Romfs, Value};

use super::events::{calls, case, last, one, set_cases, set_string, with};
use super::placements::{seller, sells};
use super::spots::{Branch, Scrub, edits, plain};
use super::{
    COMPARE, ENTRY, FLOW, GRANT, HEAD, NAME, ONE_TIME, PAY, PLAIN_ONE_TIME, PRICE, PRICE_PARAM,
    ROOT, SECRET, SELECTED, SELLER, SET_STRING, SHOW, SHOW_FLOW, SOME_FLAG, TEMP, WAIT_PAID,
    WRITE_ITEM,
};
use crate::checks::Drop;
use crate::flows::{
    self, GAME_DATA, IN_POUCH, IS_BOOL, Rewrites, events, params_mut, reachable, string,
};
use crate::known::{field, flag, item};

/// Sell each edited scrub's item in the shared flow, every one at `price`.
/// Sales are picked by item, so one price for all keeps two scrubs selling
/// the same item from asking one price and taking another's.
pub fn write(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    drops: &HashMap<u64, Drop>,
    price: i32,
) -> Result<()> {
    let edits: Vec<_> = edits(drops).collect();
    if edits.is_empty() {
        return Ok(());
    }
    flows::edit(romfs, rewrites, FLOW, |flow| {
        if edits.iter().any(|(_, item)| plain(item)) {
            plain_sale(flow)?;
        }
        let mut sales: Vec<&str> = Vec::new();
        for &(s, item) in &edits {
            match s.branch {
                Branch::Secret => secret(flow, item, price)?,
                _ if sales.contains(&item) => {}
                _ => sales.push(item),
            }
        }
        sell(flow, &sales, price)?;
        if edits
            .iter()
            .any(|(s, item)| s.branch == Branch::Shop && !plain(item))
        {
            entry(flow)?;
        }
        let spots: Vec<_> = edits
            .iter()
            .filter(|(s, item)| s.branch != Branch::Secret && plain(item))
            .collect();
        spots_sell(flow, &spots)
    })
}

/// Hyrule Field's sale and the item it shows, swapped in place.
pub(super) fn secret(flow: &mut Flowchart, item: &str, price: i32) -> Result<()> {
    let talk = reachable(flow, SECRET)?;
    let only = |found: Vec<u16>, what| {
        one(
            found.into_iter().filter(|e| talk.contains(e)).collect(),
            what,
        )
    };
    let sale = only(calls(flow, ONE_TIME), "secret sales")?;
    let shown = only(
        with(flow, SET_STRING, field::GAME_DATA_NAME, SELECTED),
        "secret item shows",
    )?;
    if let EventKind::SubFlow { entry_point, .. } = &mut flow.events[sale as usize].kind
        && plain(item)
    {
        *entry_point = PLAIN_ONE_TIME.into();
    }
    let params = params_mut(&mut flow.events[sale as usize].kind);
    flows::set(params, flag::ACTOR_NAME, string(item))?;
    flows::set(params, PRICE_PARAM, Value::Int(vec![price]))?;
    flows::set(params, SELLER, string(&seller(item)))?;
    flows::set(
        params_mut(&mut flow.events[shown as usize].kind),
        "Value",
        string(item),
    )
}

/// Whether Link keeps `item`, so a second copy of its scrub mustn't sell it
/// again.
fn kept(item: &str) -> bool {
    !["Obj_", "TBox_", item::BOTTLE]
        .iter()
        .any(|p| item.starts_with(p))
}

/// Put a compare for each of `sales` in front of the vanilla chain. A match
/// sells the item at `price`, unless Link already keeps it.
fn sell(flow: &mut Flowchart, sales: &[&str], price: i32) -> Result<()> {
    if sales.is_empty() {
        return Ok(());
    }
    let compares = with(flow, COMPARE, field::GAME_DATA_NAME, TEMP);
    let head = one(
        with(flow, COMPARE, "Value", HEAD)
            .into_iter()
            .filter(|e| compares.contains(e))
            .collect(),
        "chain heads",
    )?;
    let mut miss = head;
    while compares.contains(&miss) {
        miss = case(flow, miss, 1)?;
    }
    let call = *calls(flow, ONE_TIME).first().context("no one-time sale")?;
    let gate = events(flow, IN_POUCH).first().context("no pouch check")?.0;
    let flag = one(with(flow, IS_BOOL, GAME_DATA, SOME_FLAG), "flag checks")?;
    let into_head = flow.links_to(head);
    let mut next = head;
    for &item in sales.iter().rev() {
        let mut sale = last(flow, call);
        let params = params_mut(&mut sale);
        flows::set(params, flag::ACTOR_NAME, string(item))?;
        flows::set(params, PRICE_PARAM, Value::Int(vec![price]))?;
        flows::set(params, SELLER, string(&seller(item)))?;
        if let (true, EventKind::SubFlow { entry_point, .. }) = (plain(item), &mut sale) {
            *entry_point = PLAIN_ONE_TIME.into();
        }
        let mut sale = flow.push(sale);
        if kept(item) {
            // Songs have no pouch count, only a flag that says Link has them.
            let (mut check, key, value) = match plain(item) {
                true => (
                    flow.events[flag as usize].kind.clone(),
                    GAME_DATA.get(),
                    format!("IsGet.{item}"),
                ),
                false => (
                    flow.events[gate as usize].kind.clone(),
                    "Actor",
                    item.to_owned(),
                ),
            };
            flows::set(params_mut(&mut check), key, string(&value))?;
            set_cases(&mut check, [miss, sale]);
            sale = flow.push(check);
        }
        let mut compare = flow.events[head as usize].kind.clone();
        flows::set(params_mut(&mut compare), "Value", string(item))?;
        set_cases(&mut compare, [sale, next]);
        next = flow.push(compare);
    }
    for link in into_head {
        flow.link(link, Some(next))?;
    }
    Ok(())
}

/// Add `PLAIN_ONE_TIME`: `ONE_TIME` with the vanilla get event, which shows
/// the item in Link's hands first, replaced by taking the rupees and the
/// plain grant that songs and rewards already use.
fn plain_sale(flow: &mut Flowchart) -> Result<()> {
    let copy = flows::copy_entry(flow, ONE_TIME, PLAIN_ONE_TIME)?;
    let shows: Vec<u16> = copy
        .values()
        .copied()
        .filter(|&e| match &flow.events[e as usize].kind {
            EventKind::SubFlow {
                flowchart,
                entry_point,
                ..
            } => flowchart == SHOW_FLOW && entry_point == SHOW,
            _ => false,
        })
        .collect();
    let [show] = shows[..] else {
        bail!("{ONE_TIME} has {} get events, not one", shows.len())
    };
    let EventKind::SubFlow { next, .. } = flow.events[show as usize].kind else {
        unreachable!("found as a call")
    };
    let actor = |flow: &Flowchart, name: &str| {
        let found = flow.actors.iter().find(|a| a.name == name);
        found
            .filter(|a| a.secondary_name.is_empty())
            .cloned()
            .with_context(|| format!("{FLOW} has no {name}"))
    };
    let (pouch, pay) = (actor(flow, PAY.0)?, PAY.1);
    let (ui, wait) = (actor(flow, WAIT_PAID.0)?, WAIT_PAID.1);
    let (pouch, pay) = flow.action(&pouch, pay);
    let (ui, wait) = flow.action(&ui, wait);
    let grant = flow.push(EventKind::SubFlow {
        next,
        params: vec![(NAME.get().into(), Value::Argument(NAME.get().into()))],
        flowchart: GRANT.0.to_string(),
        entry_point: GRANT.1.to_string(),
    });
    let paid = flow.push(EventKind::Action {
        next: Some(grant),
        actor: ui,
        action: wait,
        params: vec![(field::IS_WAIT_END.to_string(), Value::Bool(vec![true]))],
    });
    flow.events[show as usize].kind = EventKind::Action {
        next: Some(paid),
        actor: pouch,
        action: pay,
        params: vec![
            (field::IS_SIGN_INVERSE.to_string(), Value::Bool(vec![true])),
            (GAME_DATA.to_string(), string(PRICE)),
        ],
    };
    Ok(())
}

/// After the talk reads the placement's item, swap in `SELECTED`'s item if
/// a plain spot's entry left one there, and clear it. Anyone else's talk
/// finds it empty and carries on.
fn hook(flow: &mut Flowchart, items: &[&str]) -> Result<()> {
    let root = reachable(flow, ROOT)?;
    let reads = events(flow, WRITE_ITEM).into_iter().map(|(e, _)| e);
    let read = one(reads.filter(|e| root.contains(e)).collect(), "item reads")?;
    let EventKind::Action {
        next: Some(after), ..
    } = flow.events[read as usize].kind
    else {
        bail!("the item read doesn't lead anywhere")
    };
    // Any item compare will do as a template. `sell` may already have added
    // one for the chain's own head item.
    let template = *with(flow, COMPARE, field::GAME_DATA_NAME, TEMP)
        .first()
        .context("no item compare")?;
    let clear = set_string(flow, SELECTED, "", Some(after))?;
    let clear = flow.push(clear);
    let mut miss = after;
    for item in items.iter().rev() {
        let copy = set_string(flow, TEMP, item, Some(clear))?;
        let copy = flow.push(copy);
        let mut compare = flow.events[template as usize].kind.clone();
        flows::set(params_mut(&mut compare), GAME_DATA, string(SELECTED))?;
        flows::set(params_mut(&mut compare), "Value", string(item))?;
        set_cases(&mut compare, [copy, miss]);
        miss = flow.push(compare);
    }
    flow.link(Link::Next(read), Some(miss))
}

/// Give each of `spots` an entry for its talk: it names the item to sell,
/// then runs what the spot's talk ran before.
fn spots_sell(flow: &mut Flowchart, spots: &[&(&Scrub, &str)]) -> Result<()> {
    if spots.is_empty() {
        return Ok(());
    }
    let mut items: Vec<&str> = spots.iter().map(|(_, item)| *item).collect();
    items.sort();
    items.dedup();
    hook(flow, &items)?;
    let root = *calls(flow, ROOT)
        .first()
        .context("no call to the pouch root")?;
    for &&(s, item) in spots {
        let name = sells(s, item);
        ensure!(
            flow.entry_points.iter().all(|e| e.name != name),
            "{FLOW} already has {name}"
        );
        let call = match s.branch {
            Branch::Shop => last(flow, root),
            _ => {
                let vanilla = flow.entry_points.iter().find(|e| e.name == s.talk);
                let vanilla = vanilla.and_then(|e| e.main);
                let kind = vanilla.map(|e| flow.events[e as usize].kind.clone());
                match kind {
                    Some(EventKind::SubFlow {
                        ref entry_point,
                        ref flowchart,
                        ..
                    }) if entry_point == ROOT && flows::is_local(flow, flowchart) => kind.unwrap(),
                    _ => bail!("{} doesn't start at a call to {ROOT}", s.talk),
                }
            }
        };
        let call = flow.push(call);
        let set = set_string(flow, SELECTED, item, Some(call))?;
        let main = Some(flow.push(set));
        flow.entry_points.push(EntryPoint {
            name,
            main,
            vars: vec![],
        });
    }
    Ok(())
}

/// Add `ENTRY`: the pouch root with the greeting every one-time scrub uses,
/// and no sold flag after.
fn entry(flow: &mut Flowchart) -> Result<()> {
    ensure!(
        flow.entry_points.iter().all(|e| e.name != *ENTRY),
        "{FLOW} already has {}",
        *ENTRY
    );
    let root = *calls(flow, ROOT)
        .first()
        .context("no call to the pouch root")?;
    let call = last(flow, root);
    let main = Some(flow.push(call));
    flow.entry_points.push(EntryPoint {
        name: ENTRY.clone(),
        main,
        vars: vec![],
    });
    Ok(())
}
