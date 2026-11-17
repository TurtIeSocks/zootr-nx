//! A gift dealt a level of a progressive line gives the level Link needs,
//! as the hook does for a chest, and one dealt ammo gives it only once Link
//! has what fires it, as the chest swap does. Each grant of the item gets a
//! chain of pouch questions in front, highest first: the game's own way of
//! picking the archery quiver.

use super::*;
use crate::flows::Model;
use crate::known::Name;
use crate::{ammo, progressive};

/// The pouch question every chain asks, as the archery flow asks it.
pub(crate) fn model(flows: &Flows) -> Result<Model> {
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

/// Whether what `item` gives depends on what Link owns: ammo without what
/// fires it, or a line's level with progressive items on.
pub(crate) fn asks(item: &str, progressive: bool) -> bool {
    ammo::swap(item).is_some() || progressive && progressive::in_line(item)
}

/// Whether `gift`, dealt `item`, needs a chain: ammo, or a level of a line
/// with progressive items on, unless the game already picks its level.
pub(super) fn needs(gift: &Gift, item: &str, progressive: bool) -> bool {
    let picks = item == gift.item.get() && picked(item).is_some();
    let level = progressive && progressive::in_line(item) && !picks;
    chained(gift) && (level || ammo::swap(item).is_some())
}

/// Every item a chain for `item` can give, lowest first.
pub(crate) fn steps(item: &str) -> impl Iterator<Item = &'static str> + use<> {
    let ladder = ladder(item).map(|(l, _)| l).unwrap_or_default();
    ladder.into_iter().map(|(give, _)| give.get())
}

/// What `item` gives by what Link owns, lowest first, each a drop and the
/// pouch name owning it means, with how many steps past the highest owned a
/// grant gives. A line's level gives the next level. Ammo steps from
/// [`ammo::WITHOUT`] to itself, once per level of what fires it, and gives
/// the highest step.
fn ladder(item: &str) -> Option<(Vec<(&'static Name, &'static Name)>, usize)> {
    let Some(&(ammo, fires)) = ammo::NEEDS.iter().find(|(a, _)| *a == item) else {
        return progressive::line(item).map(|l| (l.to_vec(), 1));
    };
    let fired = progressive::line(fires).map_or(vec![(fires, fires)], <[_]>::to_vec);
    let steps = fired.into_iter().map(|(_, pouch)| (ammo, pouch));
    Some((
        std::iter::once((ammo::WITHOUT, ammo::WITHOUT))
            .chain(steps)
            .collect(),
        0,
    ))
}

/// Put a chain in front of each event of `flow` granting or showing `item`,
/// ammo or a level of a line. A grant of a level gives the one after the
/// highest Link owns, the top once he owns it. A grant of ammo, and a get
/// window shown after its own grant, name the highest step he owns.
pub(super) fn by_level(flow: &mut Flowchart, model: &Model, item: &str) -> Result<()> {
    let (ladder, next) = ladder(item).with_context(|| format!("{item} gives by nothing"))?;
    let named = events(flow, item);
    ensure!(!named.is_empty(), "{} never grants {item}", flow.name);
    for (at, after) in named {
        chain(flow, model, &ladder, at, after.min(next), item)?;
    }
    Ok(())
}

/// Put a chain in front of the call `at` of `flow`, which gives `item`, so
/// it gives the level after the highest Link owns.
pub(crate) fn by_level_at(flow: &mut Flowchart, model: &Model, item: &str, at: u16) -> Result<()> {
    let (ladder, next) = ladder(item).with_context(|| format!("{item} gives by nothing"))?;
    chain(flow, model, &ladder, at, next, item)
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
    // A chest's flow passes its item on as `ContentName`, a cow's song as
    // our own argument.
    let calls = ITEM_PARAMS.into_iter().chain([flow::CONTENT_NAME]);
    let keys: Vec<&str> = match kind {
        EventKind::Action { .. } => vec![ACTOR],
        _ => (calls.map(|k| k.get()))
            .chain([crate::cows::ARG_ITEM])
            .collect(),
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

    /// One gift of each `How` that chains.
    fn kinds(g: &Gift) -> bool {
        match g.how {
            How::Calls { .. } => g.giver == "King Zora",
            How::Sale { .. } => false,
            _ => true,
        }
    }

    /// Each of [`kinds`] dealt `item` asks the pouch for `asks` and can
    /// give `gives`.
    fn asks_then_gives(item: &str, asks: &str, gives: &str) {
        let Some(dealt) = dealt(kinds, item) else {
            return;
        };
        let asks = format!("Actor=\"{asks}\"");
        for (gift, flow) in dealt {
            let flow = flows::render(&flow);
            assert!(flow.contains(&*asks), "{} never asks {asks}", gift.giver);
            assert!(flow.contains(gives), "{} never gives {gives}", gift.giver);
        }
    }

    #[test]
    fn a_gift_of_each_kind_gives_the_hookshot_or_the_longshot_by_the_pouch() {
        asks_then_gives(item::HOOKSHOT, item::HOOKSHOT, item::LONGSHOT);
    }

    #[test]
    fn a_gift_of_each_kind_dealt_arrows_gives_a_red_rupee_without_the_bow() {
        let arrows = crate::known::actor::T_BOX_ARROW_BUNDLE_SMALL;
        asks_then_gives(arrows, crate::known::actor::WEAPON_RANGE_BOW, ammo::WITHOUT);
    }

    #[test]
    fn ammo_steps_from_the_rupee_through_every_level_that_fires_it() {
        needs_names!();
        let (bombs, next) = ladder(item::BOMB_BUNDLE_SMALL).unwrap();
        let pouches: Vec<&str> = bombs.iter().map(|(_, p)| p.get()).collect();
        let bags = [
            item::BOMB_BAG_SMALL,
            item::BOMB_BAG_MEDIUM,
            item::BOMB_BAG_LARGE,
        ];
        assert_eq!(pouches[1..], bags.map(|b| b.get()));
        assert_eq!(
            (bombs[0].0, bombs[1].0, next),
            (ammo::WITHOUT, item::BOMB_BUNDLE_SMALL, 0)
        );
        assert_eq!(ladder(item::HOOKSHOT).unwrap().1, 1);
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
