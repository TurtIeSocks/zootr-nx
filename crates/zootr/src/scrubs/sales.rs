//! The edits to the shared scrub flow: what each edited spot sells, the
//! plain sale for items with no get animation, and the entries spots talk
//! through.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{EntryPoint, EventKind, Flowchart, Link, Romfs, Value};

use super::events::{self, calls, case, last, one, set_cases, set_string, with};
use super::placements::{naming, seller, sells};
use super::spots::{Branch, Scrub, edits, plain};
use super::{
    BOUGHT, COMPARE, FLOW, GRANT, HEAD, HEART, HEART_GAUGE, NAME, ONE_TIME, PAY, PLAIN_ONE_TIME,
    PRICE, PRICE_PARAM, ROOT, SECRET, SELECTED, SELLER, SET_STRING, SHOW, SHOW_FLOW, SOLD_LIKE,
    SOLD_OUT, TEMP, TEXT, WAIT_PAID, WRITE_ITEM,
};
use crate::checks::Drop;
use crate::flows::{
    self, GAME_DATA, IS_BOOL, Rewrites, VALUE, events, param, params_mut, reachable, string,
};
use crate::gifts::levels::Level;
use crate::known::{actor, field, flag};

/// Sell each edited scrub's item in the shared flow, every one at `price`.
/// Sales are picked by item, so one price for all keeps two scrubs selling
/// the same item from asking one price and taking another's.
pub(crate) fn write(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    drops: &HashMap<u64, Drop>,
    price: i32,
    level: Level,
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
                Branch::Secret => secret(flow, item, price, level)?,
                _ if sales.contains(&item) => {}
                _ => sales.push(item),
            }
        }
        sell(flow, &sales, price, level)?;
        let spots: Vec<_> = edits
            .iter()
            .filter(|(s, _)| s.branch != Branch::Secret)
            .collect();
        entries(flow, &spots)?;
        name_lines(flow)
    })
}

/// Hyrule Field's sale, the step Link gets picked before its shop opens:
/// each step opens its own copy of the shop, naming that step and showing
/// the hearts only for a heart, as the pouch root's sales do.
pub(super) fn secret(flow: &mut Flowchart, item: &str, price: i32, level: Level) -> Result<()> {
    let talk = reachable(flow, SECRET)?;
    let only = |found: Vec<u16>, what| {
        one(
            found.into_iter().filter(|e| talk.contains(e)).collect(),
            what,
        )
    };
    let sale = only(calls(flow, ONE_TIME), "secret sales")?;
    let shown = only(
        with(flow, SET_STRING, GAME_DATA, SELECTED),
        "secret item shows",
    )?;
    set_sale(&mut flow.events[sale as usize].kind, item, price)?;
    if *item != **HEART {
        sold_out(flow, item)?;
    }
    let opens = run_into(flow, sale);
    ensure!(
        opens.contains(&shown),
        "the secret sale opens without its item"
    );
    let sales = |f: &Flowchart| [calls(f, ONE_TIME), calls(f, PLAIN_ONE_TIME)].concat();
    let before = sales(flow);
    level.chain(flow, item, sale)?;
    let last = *opens.last().context("the secret sale opens with nothing")?;
    let ask = events::next(flow, last).context("the secret shop opens into nothing")?;
    flow.link(Link::Next(last), None)?;
    let steps = sales(flow)
        .into_iter()
        .filter(|e| *e == sale || !before.contains(e));
    let mut start = ask;
    for step in steps.collect::<Vec<_>>() {
        let into = flow.links_to(step);
        let open = open(flow, &opens, shown, step)?;
        for link in into {
            flow.link(link, Some(open))?;
        }
        if step == ask {
            start = open;
        }
    }
    for link in flow.links_to(opens[0]) {
        flow.link(link, Some(start))?;
    }
    Ok(())
}

/// Make `ONE_TIME` call `sale` sell `item` at `price`, through
/// `PLAIN_ONE_TIME` for a plain item.
fn set_sale(sale: &mut EventKind, item: &str, price: i32) -> Result<()> {
    if let EventKind::SubFlow { entry_point, .. } = sale
        && plain(item)
    {
        *entry_point = PLAIN_ONE_TIME.into();
    }
    let params = params_mut(sale);
    flows::set(params, flag::ACTOR_NAME, string(item))?;
    flows::set(params, PRICE_PARAM, Value::Int(vec![price]))?;
    flows::set(params, SELLER, string(&seller(item)))
}

/// Point the riddle scrub's sold-out talks at their lines naming `item`.
fn sold_out(flow: &mut Flowchart, item: &str) -> Result<()> {
    for like in SOLD_OUT {
        let id = |label: &str| format!("{}:{label}", *TEXT);
        let talks = (0u16..)
            .zip(&flow.events)
            .filter_map(|(e, ev)| match &ev.kind {
                EventKind::Action { params, .. }
                    if param(params, field::MESSAGE_ID) == Some(&id(like)) =>
                {
                    Some(e)
                }
                _ => None,
            });
        let talks: Vec<u16> = talks.collect();
        ensure!(!talks.is_empty(), "nothing says {like}");
        for e in talks {
            let params = params_mut(&mut flow.events[e as usize].kind);
            flows::set(params, field::MESSAGE_ID, string(&id(&naming(like, item))))?;
        }
    }
    Ok(())
}

/// The straight run of actions into `e`, first to last: back from it while
/// each event has one way in.
pub(super) fn run_into(flow: &Flowchart, e: u16) -> Vec<u16> {
    let mut run = Vec::new();
    let mut at = e;
    while let [Link::Next(before)] = flow.links_to(at)[..]
        && matches!(flow.events[before as usize].kind, EventKind::Action { .. })
    {
        run.insert(0, before);
        at = before;
    }
    run
}

/// A copy of `opens` leading into `sale`, its `shown` naming what `sale`
/// sells, and the hearts left hidden unless that's a heart. Returns where
/// the copy starts.
fn open(flow: &mut Flowchart, opens: &[u16], shown: u16, sale: u16) -> Result<u16> {
    let kind = &flow.events[sale as usize].kind;
    let EventKind::SubFlow { params, .. } = kind else {
        bail!("event {sale} isn't a sale")
    };
    let item = param(params, flag::ACTOR_NAME)
        .context("a sale names no item")?
        .to_owned();
    let hearts = [actor::OBJ_HEART_PIECE, actor::OBJ_HEART_CONTAINER]
        .iter()
        .any(|h| &***h == item.as_str());
    let gauges: Vec<u16> = events(flow, HEART_GAUGE)
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    let mut next = sale;
    for &e in opens.iter().rev() {
        if !hearts && gauges.contains(&e) {
            continue;
        }
        let mut kind = flow.events[e as usize].kind.clone();
        if e == shown {
            flows::set(params_mut(&mut kind), VALUE, string(&item))?;
        }
        if let EventKind::Action { next: n, .. } = &mut kind {
            *n = Some(next);
        }
        next = flow.push(kind);
    }
    Ok(next)
}

/// Put a compare for each of `sales` in front of the vanilla chain. A match
/// sells the item at `price`, the step Link owns for one that asks.
fn sell(flow: &mut Flowchart, sales: &[&str], price: i32, level: Level) -> Result<()> {
    if sales.is_empty() {
        return Ok(());
    }
    let compares = with(flow, COMPARE, GAME_DATA, TEMP);
    let head = one(
        with(flow, COMPARE, VALUE, HEAD)
            .into_iter()
            .filter(|e| compares.contains(e))
            .collect(),
        "chain heads",
    )?;
    let call = *calls(flow, ONE_TIME).first().context("no one-time sale")?;
    let into_head = flow.links_to(head);
    let mut next = head;
    for &item in sales.iter().rev() {
        let mut sale = last(flow, call);
        set_sale(&mut sale, item, price)?;
        let sale = flow.push(sale);
        let mut compare = flow.events[head as usize].kind.clone();
        flows::set(params_mut(&mut compare), VALUE, string(item))?;
        set_cases(&mut compare, [sale, next]);
        next = flow.push(compare);
        // After the compare, which the chain then leads through.
        level.chain(flow, item, sale)?;
    }
    for link in into_head {
        flow.link(link, Some(next))?;
    }
    Ok(())
}

/// Point each of our sales' seller line at the line naming what it sells,
/// once the chains have copied a sale per step.
fn name_lines(flow: &mut Flowchart) -> Result<()> {
    let ours = seller("");
    for e in 0..flow.events.len() {
        let EventKind::SubFlow {
            entry_point,
            params,
            ..
        } = &mut flow.events[e].kind
        else {
            continue;
        };
        if ![ONE_TIME.get(), PLAIN_ONE_TIME].contains(&entry_point.as_str())
            || !param(params, SELLER).is_some_and(|l| l.starts_with(&ours))
        {
            continue;
        }
        let item = param(params, NAME).context("a sale names no item")?;
        let line = seller(item);
        flows::set(params, SELLER, string(&line))?;
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
    let template = *with(flow, COMPARE, GAME_DATA, TEMP)
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
        flows::set(params_mut(&mut compare), VALUE, string(item))?;
        set_cases(&mut compare, [copy, miss]);
        miss = flow.push(compare);
    }
    flow.link(Link::Next(read), Some(miss))
}

/// Give each of `spots` an entry for its talk: it names a plain item to
/// sell, runs what the spot's talk ran before, then sets the spot's own
/// Bool once Link pays. Never the vanilla sold Bool, which spots share.
fn entries(flow: &mut Flowchart, spots: &[&(&Scrub, &str)]) -> Result<()> {
    if spots.is_empty() {
        return Ok(());
    }
    let mut items: Vec<&str> = (spots.iter().map(|(_, item)| *item))
        .filter(|i| plain(i))
        .collect();
    items.sort();
    items.dedup();
    if !items.is_empty() {
        hook(flow, &items)?;
    }
    let root = *calls(flow, ROOT)
        .first()
        .context("no call to the pouch root")?;
    let like = flow.entry_points.iter().find(|e| e.name == SOLD_LIKE);
    let like = like.and_then(|e| e.main).context("no one-time talk")?;
    let paid = (events::next(flow, like)).context("a one-time talk ends at the root")?;
    ensure!(
        with(flow, IS_BOOL, GAME_DATA, BOUGHT).contains(&paid),
        "{SOLD_LIKE} doesn't ask whether Link paid"
    );
    let sold = case(flow, paid, 0)?;
    for &&(s, item) in spots {
        let name = sells(s, item);
        ensure!(
            flow.entry_points.iter().all(|e| e.name != name),
            "{FLOW} already has {name}"
        );
        let mut mark = flow.events[sold as usize].kind.clone();
        flows::set(params_mut(&mut mark), GAME_DATA, string(&s.flag()))?;
        let EventKind::Action { next, .. } = &mut mark else {
            bail!("a paid sale's sold flag isn't an action")
        };
        *next = None;
        let mark = flow.push(mark);
        let mut ask = flow.events[paid as usize].kind.clone();
        if let EventKind::Switch { cases, .. } = &mut ask {
            *cases = vec![(0, mark)];
        }
        let ask = flow.push(ask);
        let mut call = match s.branch {
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
        if let EventKind::SubFlow { next, .. } = &mut call {
            *next = Some(ask);
        }
        let mut main = flow.push(call);
        if plain(item) {
            let set = set_string(flow, SELECTED, item, Some(main))?;
            main = flow.push(set);
        }
        flow.entry_points.push(EntryPoint {
            name,
            main: Some(main),
            vars: vec![],
        });
    }
    Ok(())
}
