//! A gift dealt a level of a progressive line gives the level Link needs,
//! as the hook does for a chest. Each grant of the item gets a chain of
//! pouch questions in front, highest level first: the game's own way of
//! picking the archery quiver.

use super::*;
use crate::flows::Model;
use crate::progressive;

/// The pouch question every chain asks, as the archery flow asks it.
pub(super) fn model(flows: &Flows) -> Result<Model> {
    let archery = flow::MINIGAME_HORSEBACK_ARCHERY;
    let chart = (flows.0.get(&**archery)).with_context(|| format!("no {archery} flow"))?;
    let at = flows::switch(chart, IN_POUCH)
        .with_context(|| format!("{archery} never asks the pouch"))?;
    flows::model(chart, IN_POUCH, at)
}

/// Whether `gift` gives through events [`by_level`] can put a chain in
/// front of. A sale grants inside the shop flow every sale shares.
pub(super) fn chained(gift: &Gift) -> bool {
    !gift.is_sale()
}

/// Whether `gift`, dealt `item`, needs a chain: a level of a line, with
/// progressive items on, unless the game already picks its level.
pub(super) fn needs(gift: &Gift, item: &str, progressive: bool) -> bool {
    let picks = item == gift.item.get() && picked(item).is_some();
    progressive && chained(gift) && progressive::in_line(item) && !picks
}

/// Put a chain in front of each event of `flow` granting or showing `item`,
/// a level of a line. A grant gives the level after the highest one Link
/// owns, the top once he owns it. A get window, shown after its own grant,
/// names the highest level he owns.
pub(super) fn by_level(flow: &mut Flowchart, model: &Model, item: &str) -> Result<()> {
    let line = progressive::line(item).with_context(|| format!("{item} is in no line"))?;
    let named = events(flow, item);
    ensure!(!named.is_empty(), "{} never grants {item}", flow.name);
    for (at, after) in named {
        chain(flow, model, line, at, after, item)?;
    }
    Ok(())
}

/// The events of `flow` granting `item`, each with how many levels past
/// the highest owned it gives: 1 for a grant, 0 for a window alone.
fn events(flow: &Flowchart, item: &str) -> Vec<(u16, usize)> {
    let grants = (flows::running(flow, GRANT).into_iter())
        .filter(|&i| param(params(&flow.events[i as usize].kind), ACTOR) == Some(item))
        .map(|i| (i, 1));
    let calls = (0u16..)
        .zip(&flow.events)
        .filter_map(|(i, e)| match &e.kind {
            EventKind::SubFlow {
                flowchart,
                entry_point,
                params,
                ..
            } if ITEM_PARAMS.iter().any(|k| param(params, k) == Some(item)) => {
                let window = flowchart == POUCH && entry_point == WINDOW;
                Some((i, usize::from(!window)))
            }
            _ => None,
        });
    grants.chain(calls).collect()
}

/// An event's params.
fn params(kind: &EventKind) -> &Params {
    match kind {
        EventKind::Action { params, .. } | EventKind::SubFlow { params, .. } => params,
        _ => unreachable!("only actions and calls grant"),
    }
}

/// Send whatever led to event `at` through the pouch questions instead.
/// `at` itself stays as the lowest level's branch.
fn chain(
    flow: &mut Flowchart,
    model: &Model,
    line: &[(&Name, &Name)],
    at: u16,
    after: usize,
    item: &str,
) -> Result<()> {
    let links = flow.links_to(at);
    let top = line.len() - 1;
    let mut head = at;
    for (owned, &(_, pouch)) in line.iter().enumerate().skip(1 - after) {
        let give = (owned + after).min(top);
        let yes = match give {
            0 => at,
            _ => {
                let mut kind = flow.events[at as usize].kind.clone();
                rename(&mut kind, item, line[give].0)?;
                flow.push(kind)
            }
        };
        let mut params = model.params.clone();
        flows::set(&mut params, ACTOR, string(pouch))?;
        let (actor, query) = flow.query(&model.actor, model.name);
        // Case 0 is "he owns it".
        head = flow.push(EventKind::Switch {
            actor,
            query,
            params,
            cases: vec![(0, yes), (1, head)],
        });
    }
    if item != line[0].0.get() {
        rename(&mut flow.events[at as usize].kind, item, line[0].0)?;
    }
    links.into_iter().try_for_each(|l| flow.link(l, Some(head)))
}

/// Name `to` wherever `kind` names `from` as its item.
fn rename(kind: &mut EventKind, from: &str, to: &str) -> Result<()> {
    let keys: &[&str] = match kind {
        EventKind::Action { .. } => &[ACTOR],
        _ => &ITEM_PARAMS.map(|k| k.get()),
    };
    let params = params_mut(kind);
    for key in keys {
        if param(params, key) == Some(from) {
            flows::set(params, key, string(to))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::*;
    use crate::known::item;

    /// Each gift `pick` finds dealt `dealt`, with progressive items on, and
    /// the flow its grant ends up in.
    fn dealt(pick: impl Fn(&Gift) -> bool, dealt: &str) -> Option<Vec<(&'static Gift, Flowchart)>> {
        let romfs = crate::game::dev_dump()?;
        let gifts: Vec<&Gift> = GIFTS.iter().filter(|g| pick(g)).collect();
        let drops = (gifts.iter())
            .map(|g| (g.hash(), Drop::from(dealt.to_string())))
            .collect();
        let mut rewrites = Rewrites::default();
        let flows = Flows::load(&romfs).unwrap();
        give_all(&romfs, &flows, &mut rewrites, &drops, true).unwrap();
        let at = |g: &Gift| match g.how {
            How::Later { caller, .. } => caller.get(),
            _ => g.flow.get(),
        };
        let written = |g| (g, written(&rewrites, at(g)));
        Some(gifts.into_iter().map(written).collect())
    }

    #[test]
    fn a_gift_of_each_kind_gives_the_hookshot_or_the_longshot_by_the_pouch() {
        let kinds = |g: &Gift| match g.how {
            How::Calls { .. } => g.giver == "King Zora",
            How::Sale { .. } => false,
            _ => true,
        };
        let Some(dealt) = dealt(kinds, item::HOOKSHOT) else {
            return;
        };
        let asks = format!("Actor=\"{}\"", item::HOOKSHOT);
        for (gift, flow) in dealt {
            let flow = flows::render(&flow);
            assert!(flow.contains(&*asks), "{} never asks for it", gift.giver);
            assert!(
                flow.contains(&**item::LONGSHOT),
                "{} never gives the Longshot",
                gift.giver
            );
        }
    }

    #[test]
    fn no_level_is_granted_after_another() {
        // One gift a flow: two gifts of one scene each grant in turn.
        let checks = crate::gifts::checks();
        let mut one = HashMap::new();
        for g in GIFTS.iter().filter(|g| chained(g)) {
            let check = checks.iter().find(|c| c.hash == g.hash()).unwrap();
            if !clashes(check, item::HOOKSHOT) {
                one.entry(g.flow).or_insert(g.hash());
            }
        }
        let fits = |g: &Gift| one.get(g.flow) == Some(&g.hash());
        let Some(dealt) = dealt(fits, item::HOOKSHOT) else {
            return;
        };
        for (_, flow) in dealt {
            let grants: Vec<u16> = [item::HOOKSHOT, item::LONGSHOT]
                .iter()
                .flat_map(|i| events(&flow, i))
                .filter(|&(_, after)| after == 1)
                .map(|(e, _)| e)
                .collect();
            for &g in &grants {
                let next = flow.events[g as usize].kind.clone();
                let after = match next {
                    EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } => next,
                    _ => None,
                };
                let reached = flows::reachable_from(&flow, after);
                let twice: Vec<_> = grants.iter().filter(|e| reached.contains(e)).collect();
                assert!(twice.is_empty(), "{}: #{g} then {twice:?}", flow.name);
            }
        }
    }

    #[test]
    fn two_bowling_prizes_dealt_a_bomb_bag_each_give_by_the_pouch() {
        let bowling =
            |g: &Gift| g.giver.starts_with("Bombchu bowling") && g.item != item::BOMB_BAG_MEDIUM;
        let Some(dealt) = dealt(bowling, item::BOMB_BAG_SMALL) else {
            return;
        };
        assert_eq!(dealt.len(), 2);
        let flow = flows::render(&dealt[0].1);
        assert!(flow.contains(&**item::BOMB_BAG_LARGE));
    }
}
