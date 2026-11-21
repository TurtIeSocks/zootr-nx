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

/// The pouch question, and whether progressive items are on: what a holder
/// needs to give the step Link owns.
#[derive(Clone, Copy)]
pub(crate) struct Level<'a> {
    pub model: &'a Model,
    pub progressive: bool,
}

impl Level<'_> {
    /// Put the pouch questions in front of the call `at` of `flow` giving
    /// `item`, if what it gives depends on what Link owns.
    pub(crate) fn chain(self, flow: &mut Flowchart, item: &str, at: u16) -> Result<()> {
        match asks(item, self.progressive) {
            true => by_level_at(flow, self.model, item, at),
            false => Ok(()),
        }
    }
}

/// Whether what `item` gives depends on what Link owns: ammo without what
/// fires it, or a line's level with progressive items on.
pub(crate) fn asks(item: &str, progressive: bool) -> bool {
    ammo::swap(item).is_some() || progressive && progressive::in_line(item)
}

/// Whether `gift`, dealt `item`, needs a chain: ammo, or a level of a line
/// with progressive items on, unless the game already picks its level.
/// A sale's chain goes in front of its call into the shared shop flow,
/// which hands over the `ActorName` it's passed.
pub(super) fn needs(gift: &Gift, item: &str, progressive: bool) -> bool {
    let picks = item == gift.item.get() && picked(item).is_some();
    let level = progressive && progressive::in_line(item) && !picks;
    level || ammo::swap(item).is_some()
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

/// What a chain for `item` would give a Link who owns `owned`: the level
/// after the highest owned, or for ammo the highest step.
pub(super) fn owned_level<'a>(item: &'a str, owned: &BTreeSet<String>) -> &'a str {
    let Some((ladder, next)) = ladder(item) else {
        return item;
    };
    let has = |(_, own): &(&Name, &Name)| owned.contains(own.get());
    let at = ladder.iter().rposition(has).map_or(0, |i| i + next);
    ladder[at.min(ladder.len() - 1)].0.get()
}

/// Put a chain in front of each event of `flow` granting or showing `item`,
/// ammo or a level of a line. A grant of a level gives the one after the
/// highest Link owns, the top once he owns it. A grant of ammo, and a get
/// window shown after its own grant, name the highest step he owns.
/// With `only`, just the grants at those events.
pub(super) fn by_level(
    flow: &mut Flowchart,
    model: &Model,
    item: &str,
    only: Option<&[u16]>,
) -> Result<()> {
    let (ladder, next) = ladder(item).with_context(|| format!("{item} gives by nothing"))?;
    let mut named = events(flow, item);
    if let Some(only) = only {
        named.retain(|(at, _)| only.contains(at));
    }
    ensure!(!named.is_empty(), "{} never grants {item}", flow.name);
    // A line naming the item, said before the grant, names the same level.
    named.extend(lines::talks(flow, item).into_iter().map(|at| (at, 1)));
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
                let copy = flow.push(flow.events[at as usize].kind.clone());
                rename(flow, copy, item, line[give].0)?;
                copy
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
        rename(flow, at, item, line[0].0)?;
    }
    links.into_iter().try_for_each(|l| flow.link(l, Some(head)))
}

/// Name `to` wherever event `at` names `from` as its item.
fn rename(flow: &mut Flowchart, at: u16, from: &str, to: &str) -> Result<()> {
    if param(params_mut(&mut flow.events[at as usize].kind), NAME) == Some(from) {
        // A sale shows its item.
        sales::show(flow, at, to)?;
    }
    let kind = &mut flow.events[at as usize].kind;
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
    // A talk, or a sale's thanks, by a line naming it.
    lines::repoint(params, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::*;
    use crate::known::item;

    #[test]
    fn an_owned_level_is_the_next_one_up_to_the_top() {
        needs_names!();
        let owned = |names: &[&Name]| names.iter().map(|n| n.to_string()).collect();
        let (hook, long) = (item::HOOKSHOT, item::LONGSHOT);
        assert_eq!(owned_level(hook, &owned(&[])), hook.get());
        assert_eq!(owned_level(hook, &owned(&[hook])), long.get());
        assert_eq!(owned_level(hook, &owned(&[long])), long.get());
        assert_eq!(
            owned_level("Item_FakeRupee", &owned(&[hook])),
            "Item_FakeRupee"
        );
    }

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
        // Talon's grant is the trade chain's, which a seed adds first.
        crate::trade_chain::deal(&romfs, &mut rewrites, &drops).unwrap();
        give_all(
            &romfs,
            &flows,
            &mut rewrites,
            &drops,
            true,
            &Default::default(),
        )
        .unwrap();
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

    /// Each sale's calls naming a level of the Hookshot's line, by the
    /// `ActorName` they sell, with what they show and the line said once
    /// paid.
    #[test]
    fn a_sale_shows_and_thanks_by_the_level_it_sells() {
        let Some(dealt) = dealt(Gift::is_sale, item::HOOKSHOT) else {
            return;
        };
        let line = [item::HOOKSHOT, item::LONGSHOT];
        for (gift, flow) in dealt {
            let mut sold = BTreeSet::new();
            for e in &flow.events {
                let EventKind::SubFlow { params, .. } = &e.kind else {
                    continue;
                };
                let Some(item) = param(params, NAME).filter(|i| line.iter().any(|l| l == i)) else {
                    continue;
                };
                sold.insert(item.to_string());
                for (key, value) in params {
                    if let Value::ActorId { name, .. } = value
                        && key == super::super::sales::DISPLAY
                    {
                        assert_eq!(name, item, "{} shows {name}", gift.giver);
                    }
                }
                if let Some(paid) = param(params, flow::PAYMENT_MESSAGE_ID)
                    && paid.contains("Talk_03")
                {
                    assert!(
                        paid.ends_with(&format!("_{item}")),
                        "{}: {paid}",
                        gift.giver
                    );
                }
            }
            let line: BTreeSet<_> = line.iter().map(|i| i.to_string()).collect();
            assert_eq!(sold, line, "{}", gift.giver);
        }
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
        let mut one = HashMap::new();
        for g in &GIFTS {
            one.entry(g.flow).or_insert(g.hash());
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
    fn a_gift_holding_its_flow_mates_level_chains_only_its_own_call() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let archery = |item: &str| {
            let g = GIFTS
                .iter()
                .find(|g| g.flow == flow::MINIGAME_HORSEBACK_ARCHERY && g.item == item);
            g.unwrap()
        };
        let (quiver, heart) = (archery(item::QUIVER_MEDIUM), archery(HEART));
        let How::Calls { call, .. } = quiver.how else {
            panic!("the archery prizes are calls")
        };
        let before = flows::load(&romfs, quiver.flow).unwrap().unwrap();
        let at = |gift: &Gift| naming(&before, call, gift.item, &BTreeSet::new()).unwrap();
        let drops = HashMap::from([(heart.hash(), Drop::from(item::QUIVER_MEDIUM.to_string()))]);
        let mut rewrites = Rewrites::default();
        let flows = Flows::load(&romfs).unwrap();
        give_all(
            &romfs,
            &flows,
            &mut rewrites,
            &drops,
            true,
            &Default::default(),
        )
        .unwrap();
        let after = written(&rewrites, quiver.flow);
        let led = |flow: &Flowchart, gift: &Gift| {
            at(gift)
                .iter()
                .map(|&e| flow.links_to(e))
                .collect::<Vec<_>>()
        };
        // The heart prize asks the pouch first. The quiver prize, whose
        // level the game picks, is reached as it was.
        assert_ne!(led(&before, heart), led(&after, heart));
        assert_eq!(led(&before, quiver), led(&after, quiver));
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
