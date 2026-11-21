//! The flow edits a gift makes in place: a renamed grant, a renamed call,
//! a bag or quiver the giver asks about, and the waits and gates in his way.

use super::*;

/// Answer every `ELAPSED` query in `flow` as over, so nothing waits.
pub(super) fn waited(flow: &mut Flowchart) -> Result<()> {
    let over = |flow: &Flowchart, i: u16| match &flow.events[i as usize].kind {
        EventKind::Switch {
            actor,
            query,
            cases,
            ..
        } if flow.actor_of(*actor).queries[*query as usize].contains(&**ELAPSED) => {
            cases.iter().find(|c| c.0 == 0).map(|c| c.1)
        }
        _ => None,
    };
    let waits: Vec<u16> = (0..flow.events.len() as u16)
        .filter(|&i| over(flow, i).is_some())
        .collect();
    ensure!(!waits.is_empty(), "{} never waits", flow.name);
    for &wait in &waits {
        let mut to = over(flow, wait);
        while let Some(next) = to.and_then(|t| over(flow, t)) {
            to = Some(next);
        }
        for link in flow.links_to(wait) {
            flow.link(link, to)?;
        }
    }
    Ok(())
}

/// The switches the fountain melody runs that read Din's Fire.
fn din_gates(flow: &Flowchart) -> Result<Vec<u16>> {
    Ok(flows::reachable(flow, FOUNTAIN_ENTRY)?
        .into_iter()
        .filter(|&e| {
            matches!(&flow.events[e as usize].kind,
                EventKind::Switch { params, .. } if param(params, GAME_DATA) == Some(DIN_FIRE))
        })
        .collect())
}

/// Every fountain opens for its melody, whether Link holds Din's Fire or
/// not. The game sends the other five to a Navi line until he does.
pub(super) fn ungated(flow: &mut Flowchart) -> Result<()> {
    let &[gate] = &din_gates(flow)?[..] else {
        bail!("{} gates on Din's Fire not exactly once", flow.name);
    };
    let EventKind::Switch { cases, .. } = &flow.events[gate as usize].kind else {
        unreachable!("din_gates found a switch");
    };
    let open = cases.iter().find(|c| c.0 == 0).map(|c| c.1);
    let open = open.context("the Din's Fire gate has no case 0")?;
    for link in flow.links_to(gate) {
        flow.link(link, Some(open))?;
    }
    Ok(())
}

/// Every entry point of `flow` ends at once.
pub(super) fn empty(flow: &mut Flowchart) -> Result<()> {
    (0..flow.entry_points.len()).try_for_each(|i| flow.link(Link::Entry(i), None))
}

/// The pouch call an event makes, and the item it names.
fn pouch_call(e: &EventKind) -> Option<(&str, Option<&str>)> {
    match e {
        EventKind::SubFlow {
            flowchart,
            entry_point,
            params,
            ..
        } if flowchart == POUCH => Some((entry_point, param(params, NAME))),
        _ => None,
    }
}

/// `flow`'s one call to `GetPouchContent::<entry>`, naming `name` if given.
pub(super) fn only_call(flow: &Flowchart, entry: &str, name: Option<&str>) -> Result<u16> {
    let calls: Vec<u16> = (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| {
            pouch_call(&e.kind)
                .is_some_and(|(at, n)| at == entry && name.is_none_or(|w| n == Some(w)))
        })
        .map(|(i, _)| i)
        .collect();
    match calls[..] {
        [at] => Ok(at),
        _ => bail!(
            "{} calls {POUCH}::{entry} {} times, not once",
            flow.name,
            calls.len()
        ),
    }
}

pub(super) fn in_place(flow: &mut Flowchart, item: &str) -> Result<()> {
    let grant = only(flow, GRANT)?;
    flows::set(
        params_mut(&mut flow.events[grant as usize].kind),
        ACTOR,
        string(item),
    )?;
    let window = only_call(flow, WINDOW, None)?;
    flows::set(
        params_mut(&mut flow.events[window as usize].kind),
        NAME,
        string(item),
    )
}

/// The params a getting sub-flow takes its item in.
pub(super) const ITEM_PARAMS: [&Name; 3] = [NAME, flow::REVEIVE_ITEM, flow::TAKEN_REWARD_ACTOR];

/// The getting sub-flow that only shows the item.
pub(super) const PLAIN: &Name = flow::GET_IN_EVENT_DEMO_NOT_LOOKAT_OBJECT_ENTRY;

/// Getting sub-flows dressed for one item, with a magic meter animation or
/// the bracelet tutorial around the same grant. A swapped item gets `PLAIN`.
pub(super) const DRESSED: [&Name; 9] = [
    flow::GET_IN_EVENT_GREAT_FAIRY_DIN,
    flow::GET_IN_EVENT_GREAT_FAIRY_WISDOM,
    flow::GET_IN_EVENT_GREAT_FAIRY_COURGE,
    flow::GET_IN_EVENT_GORON,
    flow::GET_IN_EVENT_ITEM_SWORD_BIGGORON,
    flow::GET_IN_EVENT_POESHOP,
    flow::GET_IN_EVENT_NOT_TURN_FIXED_CAMERA_WARP_LINK,
    flow::GET_IN_EVENT_ITEM_SWORD_MEDIGORON,
    flow::GET_IN_EVENT_MASK,
];

/// The events that make any of `calls`.
pub(super) fn calls_to(flow: &Flowchart, calls: &[Call]) -> Result<Vec<usize>> {
    Ok((0..flow.events.len())
        .filter(|&i| match &flow.events[i].kind {
            EventKind::SubFlow {
                flowchart,
                entry_point,
                ..
            } => calls.iter().any(|&Call(chart, entry)| {
                entry_point == entry
                    && match chart {
                        None => is_local(flow, flowchart),
                        Some(chart) => flowchart == chart,
                    }
            }),
            _ => false,
        })
        .collect())
}

/// Rename `from` to `to` in every call to `call` naming it, at least one.
/// A dressed call becomes `PLAIN`.
pub(super) fn rename_calls(
    flow: &mut Flowchart,
    call: &[Call],
    from: &str,
    to: &str,
) -> Result<()> {
    rename_untaken(flow, call, from, to, &BTreeSet::new()).map(drop)
}

/// The calls to `call` naming `item` that aren't `taken` by another gift.
pub(super) fn naming(
    flow: &Flowchart,
    call: &[Call],
    item: &str,
    taken: &BTreeSet<u16>,
) -> Result<Vec<u16>> {
    let names = |i: &u16| match &flow.events[*i as usize].kind {
        EventKind::SubFlow { params, .. } => {
            ITEM_PARAMS.iter().any(|k| param(params, k) == Some(item))
        }
        _ => false,
    };
    Ok((calls_to(flow, call)?.into_iter())
        .map(|i| i as u16)
        .filter(|i| !taken.contains(i) && names(i))
        .collect())
}

/// [`rename_calls`], skipping the calls another gift of the flow has
/// `taken`. Two gifts of one flow can hold each other's items that way.
/// Returns the calls renamed.
pub(super) fn rename_untaken(
    flow: &mut Flowchart,
    call: &[Call],
    from: &str,
    to: &str,
    taken: &BTreeSet<u16>,
) -> Result<Vec<u16>> {
    let calls = naming(flow, call, from, taken)?;
    ensure!(
        !calls.is_empty(),
        "{} never calls {} with {from}",
        flow.name,
        said(call)
    );
    for &i in &calls {
        let kind = &mut flow.events[i as usize].kind;
        let params = params_mut(kind);
        for key in ITEM_PARAMS {
            if param(params, key) == Some(from) {
                flows::set(params, key, string(to))?;
            }
        }
        if let EventKind::SubFlow { entry_point, .. } = kind
            && DRESSED.iter().any(|d| **d == **entry_point)
        {
            *entry_point = PLAIN.to_string();
        }
    }
    Ok(calls)
}

/// Ask the gift's own Bool `name` where `flow` asked whether Link holds
/// `held`, since what the giver gives may be anything. Set goes where
/// "held" went.
pub(super) fn ask_own_bool(flow: &mut Flowchart, held: &str, name: &str) -> Result<()> {
    let at = (flows::events(flow, IN_POUCH).into_iter())
        .find(|(_, p)| param(p, ACTOR) == Some(held))
        .with_context(|| format!("{} never asks for {held}", flow.name))?
        .0;
    let EventKind::Switch { cases, .. } = &flow.events[at as usize].kind else {
        unreachable!("the question was found as a switch");
    };
    // Case 0 is the Bool set, as it was the item held.
    flow.events[at as usize].kind = bool_switch(flow, name, cases.clone())?;
    Ok(())
}

/// A query on `flow`'s Bool `name` with `cases`, modelled on the flow's
/// first Bool query. Case 0 is true.
pub(super) fn bool_switch(
    flow: &mut Flowchart,
    name: &str,
    cases: Vec<(u32, u16)>,
) -> Result<EventKind> {
    let asks =
        flows::switch(flow, IS_BOOL).with_context(|| format!("{} asks no Bool", flow.name))?;
    let asks = flows::model(flow, IS_BOOL, asks)?;
    let mut params = asks.params;
    flows::set(&mut params, GAME_DATA, string(name))?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    Ok(EventKind::Switch {
        actor,
        query,
        params,
        cases,
    })
}

/// Set the gift's own Bool `name` right after each call to `call`, at
/// least one, so the gift is given once. A dressed call becomes `PLAIN`.
pub(super) fn set_own_bool(flow: &mut Flowchart, call: &[Call], name: &str) -> Result<()> {
    let sets = flows::running(flow, SET_BOOL);
    let sets = sets
        .first()
        .with_context(|| format!("{} sets no Bool", flow.name))?;
    let sets = flows::model(flow, SET_BOOL, *sets)?;
    let calls = calls_to(flow, call)?;
    ensure!(
        !calls.is_empty(),
        "{} never calls {}",
        flow.name,
        said(call)
    );
    for i in calls {
        let EventKind::SubFlow {
            entry_point, next, ..
        } = &mut flow.events[i].kind
        else {
            unreachable!("calls_to finds sub-flows");
        };
        if DRESSED.iter().any(|d| **d == **entry_point) {
            *entry_point = PLAIN.to_string();
        }
        let next = *next;
        let mut params = sets.params.clone();
        flows::set(&mut params, GAME_DATA, string(name))?;
        flows::set(&mut params, "Value", Value::Bool(vec![true]))?;
        let (actor, action) = flow.action(&sets.actor, sets.name);
        let mark = flow.push(EventKind::Action {
            next,
            actor,
            action,
            params,
        });
        flow.link(Link::Next(i as u16), Some(mark))?;
    }
    Ok(())
}

/// The bag and quiver lines, each as the start of its levels' names, whose
/// Medium level a gift gives when the game picks it by what Link holds.
const PICKED: [&str; 5] = [
    "Item_BombBag_",
    "Item_Quiver_",
    "Item_BulletBag_",
    "Item_StickBag_",
    "Item_NutBag_",
];

/// The line of `item` when it's the Medium level of a picked one.
pub(super) fn picked(item: &str) -> Option<&'static str> {
    PICKED
        .into_iter()
        .find(|line| item.strip_prefix(line) == Some("Medium"))
}

/// The event a link leaves from, none for an entry point.
pub(super) fn leaves(link: &Link) -> Option<u16> {
    match *link {
        Link::Entry(_) => None,
        Link::Next(e) | Link::Branch(e, _) | Link::Join(e) => Some(e),
    }
}

/// The event of `flow` giving `keep`, the one sub-flow call naming it that
/// another gift hasn't `taken`.
pub(super) fn giving(flow: &Flowchart, keep: &str, taken: &BTreeSet<u16>) -> Result<u16> {
    let calls: Vec<u16> = (0u16..)
        .zip(&flow.events)
        .filter(|(i, _)| !taken.contains(i))
        .filter(|(_, e)| match &e.kind {
            EventKind::SubFlow { params, .. } => {
                ITEM_PARAMS.iter().any(|k| param(params, k) == Some(keep))
            }
            _ => false,
        })
        .map(|(i, _)| i)
        .collect();
    match calls[..] {
        [call] => Ok(call),
        _ => bail!("{} gives {keep} {} times, not once", flow.name, calls.len()),
    }
}

/// The events of `flow` asking whether Link holds a level of the `ask`
/// line, in his pouch or by its Bool, and leading to `call`.
pub(super) fn asked(flow: &Flowchart, ask: &str, call: u16) -> Vec<u16> {
    (0u16..)
        .zip(&flow.events)
        .filter(|(i, e)| match &e.kind {
            EventKind::Switch { params, .. } => {
                ["Actor", field::GAME_DATA_NAME]
                    .iter()
                    .filter_map(|k| param(params, k))
                    .any(|n| n.strip_prefix(IS_GET).unwrap_or(n).starts_with(ask))
                    && flows::reachable_from(flow, Some(*i)).contains(&call)
            }
            _ => false,
        })
        .map(|(i, _)| i)
        .collect()
}

/// The game gives the Medium or the Large level of a bag by what Link
/// holds, but a gift has one call. Send whatever leads to the first
/// question about the `ask` line before the call giving `keep` straight to
/// that call, or to the event that runs it, so it's reached whatever Link
/// holds. Calls another gift has `taken` aren't `keep`'s.
pub(super) fn one_of(
    flow: &mut Flowchart,
    ask: &str,
    keep: &str,
    taken: &BTreeSet<u16>,
) -> Result<()> {
    let call = giving(flow, keep, taken)?;
    let asks = asked(flow, ask, call);
    // The first question is the one no other question leads to.
    let first = asks.iter().copied().find(|&q| {
        flow.links_to(q)
            .iter()
            .all(|l| !leaves(l).is_some_and(|e| asks.contains(&e)))
    });
    let first = first.with_context(|| format!("{} never asks for {ask}", flow.name))?;
    let runs = (0u16..).zip(&flow.events).find_map(|(i, e)| match &e.kind {
        EventKind::Action { next: Some(n), .. } if *n == call => Some(i),
        EventKind::Fork { branches, .. } if branches.contains(&call) => Some(i),
        _ => None,
    });
    let links = flow.links_to(first);
    ensure!(
        !links.is_empty(),
        "nothing leads to {}'s {ask} questions",
        flow.name
    );
    let to = runs.unwrap_or(call);
    links.into_iter().try_for_each(|l| flow.link(l, Some(to)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::*;
    use crate::known::{actor, field, flag, flow, scene};

    #[test]
    fn in_place_renames_only_the_grant_and_the_window() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let before = flows::load(&romfs, GIFTS[0].flow).unwrap().unwrap();
        let mut flow = flows::load(&romfs, GIFTS[0].flow).unwrap().unwrap();
        in_place(&mut flow, "Item_A").unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        let (old, new) = (flows::render(&before), flows::render(&after));
        let changed: Vec<_> = old
            .lines()
            .zip(new.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(changed.len(), 2, "{changed:#?}");
        for (was, now) in changed {
            assert!(was.contains(&**GIFTS[0].item), "{was}");
            assert_eq!(was.replace(&**GIFTS[0].item, "Item_A"), now);
        }
        assert!(new.contains(&**scene::ITEM_OCARINA_FAIRY_SARIA));
    }

    #[test]
    fn each_npc_gift_renames_its_calls_and_nothing_else() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for gift in &GIFTS {
            let How::Calls { call, .. } = gift.how else {
                continue;
            };
            let before = flows::load(&romfs, gift.flow).unwrap().unwrap();
            let mut flow = flows::load(&romfs, gift.flow).unwrap().unwrap();
            rename_calls(&mut flow, call, gift.item, "Item_A").unwrap();
            let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
            let (old, new) = (flows::render(&before), flows::render(&after));
            let changed: Vec<_> = old
                .lines()
                .zip(new.lines())
                .filter(|(a, b)| a != b)
                .collect();
            let named: Vec<String> = said(call).split('|').map(|c| format!("{c}(")).collect();
            let calls = old
                .lines()
                .filter(|l| named.iter().any(|n| l.contains(n)) && l.contains(&**gift.item))
                .count()
                .max(1);
            assert_eq!(changed.len(), calls, "{}: {changed:#?}", gift.giver);
            for (was, now) in changed {
                let mut was = was.replace(&**gift.item, "Item_A");
                if let Some(dressed) = DRESSED.iter().find(|d| was.contains(&format!("::{d}("))) {
                    was = was.replace(&***dressed, PLAIN);
                }
                assert_eq!(was, now, "{}", gift.giver);
            }
        }
    }

    /// Whether any entry point of `flow` reaches a call naming `item`.
    fn gives(flow: &Flowchart, item: &str) -> bool {
        let reached: BTreeSet<u16> = (flow.entry_points.iter())
            .flat_map(|e| flows::reachable_from(flow, e.main))
            .collect();
        (0u16..).zip(&flow.events).any(|(i, e)| match &e.kind {
            EventKind::SubFlow { params, .. } => {
                reached.contains(&i) && ITEM_PARAMS.iter().any(|k| param(params, k) == Some(item))
            }
            _ => false,
        })
    }

    fn picked_gifts() -> Vec<&'static Gift> {
        let gifts: Vec<_> = GIFTS.iter().filter(|g| picked(g.item).is_some()).collect();
        assert_eq!(gifts.len(), 8);
        gifts
    }

    #[test]
    fn a_moved_bag_or_quiver_never_asks_what_link_holds_and_gives_the_medium_call() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for gift in picked_gifts() {
            let line = picked(gift.item).unwrap();
            let mut flow = flows::load(&romfs, gift.flow).unwrap().unwrap();
            let asks = asked(
                &flow,
                line,
                giving(&flow, gift.item, &BTreeSet::new()).unwrap(),
            );
            assert!(!asks.is_empty(), "{}", gift.giver);
            one_of(&mut flow, line, gift.item, &BTreeSet::new()).unwrap();
            let reached: BTreeSet<u16> = (flow.entry_points.iter())
                .flat_map(|e| flows::reachable_from(&flow, e.main))
                .collect();
            let still: Vec<_> = asks.iter().filter(|q| reached.contains(q)).collect();
            assert!(still.is_empty(), "{}: {still:?} still asked", gift.giver);
            assert!(gives(&flow, gift.item), "{} never gives it", gift.giver);
            let large = format!("{line}Large");
            assert!(!gives(&flow, &large), "{} still gives {large}", gift.giver);
        }
    }

    #[test]
    fn a_dealt_bag_or_quiver_gift_gives_the_new_item_and_no_other_level() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dealt: HashMap<u64, Drop> = picked_gifts()
            .iter()
            .map(|g| (g.hash(), Drop::from(HEART.to_string())))
            .collect();
        let mut rewrites = Rewrites::default();
        give_all(
            &romfs,
            &Flows::load(&romfs).unwrap(),
            &mut rewrites,
            &dealt,
            crate::progressive::Lines::default(),
            &Default::default(),
        )
        .unwrap();
        for gift in picked_gifts() {
            let flow = written(&rewrites, gift.flow);
            assert!(gives(&flow, HEART), "{} never gives the heart", gift.giver);
            assert!(!gives(&flow, gift.item), "{} still gives it", gift.giver);
            let large = format!("{}Large", picked(gift.item).unwrap());
            assert!(!gives(&flow, &large), "{}", gift.giver);
        }
    }

    #[test]
    fn biggoron_never_makes_link_wait() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, BIGGORON).unwrap().unwrap();
        let waits = |f: &Flowchart| -> Vec<u16> {
            (0..f.events.len() as u16)
                .filter(|&i| match &f.events[i as usize].kind {
                    EventKind::Switch { actor, query, .. } => {
                        f.actor_of(*actor).queries[*query as usize].contains(&**ELAPSED)
                    }
                    _ => false,
                })
                .collect()
        };
        let before = waits(&flow);
        assert!(!before.is_empty());
        waited(&mut flow).unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        for wait in before {
            assert_eq!(after.links_to(wait), [], "#{wait} still runs");
        }
        let claim = flows::reachable(&after, field::SUB_EVENT_GET_SWORD_BIG_GORON_STEP11).unwrap();
        let sword = |e: &u16| {
            matches!(&after.events[*e as usize].kind,
                EventKind::SubFlow { entry_point, .. } if entry_point == flow::GET_IN_EVENT_ITEM_SWORD_BIGGORON)
        };
        assert!(
            claim.iter().any(sword),
            "the claim check never gives the sword"
        );
    }

    #[test]
    fn every_fountain_opens_without_dins_fire() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, FOUNTAIN).unwrap().unwrap();
        let gates = din_gates(&flow).unwrap();
        assert_eq!(gates.len(), 1, "{gates:?}");
        let gate = gates[0];
        let EventKind::Switch { cases, .. } = &flow.events[gate as usize].kind else {
            panic!("#{gate} is no switch");
        };
        let open = cases.iter().find(|c| c.0 == 0).unwrap().1;
        assert!(!flow.links_to(gate).is_empty());
        let before = flows::reachable(&flow, FOUNTAIN_ENTRY).unwrap();
        assert!(
            running(&flow, (actor::NPC_NAVI, flow::EVENT_TALK))
                .iter()
                .any(|e| before.contains(e))
        );
        ungated(&mut flow).unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert_eq!(after.links_to(gate), [], "#{gate} still runs");
        assert!(din_gates(&after).unwrap().is_empty());
        let ran = flows::reachable(&after, FOUNTAIN_ENTRY).unwrap();
        assert!(ran.contains(&open), "the melody never reaches #{open}");
        // The Navi line behind the gate no longer runs.
        let navi = (actor::NPC_NAVI, flow::EVENT_TALK);
        assert!(running(&after, navi).iter().all(|e| !ran.contains(e)));
    }

    #[test]
    fn a_taught_song_moved_leaves_its_lesson_empty() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (gift, lesson) in TAUGHT {
            assert!(GIFTS.iter().any(|g| g.flow == gift));
            let mut flow = flows::load(&romfs, lesson).unwrap().unwrap();
            assert!(flows::render(&flow).contains(&**LESSON), "{lesson}");
            empty(&mut flow).unwrap();
            let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
            assert!(
                flow.entry_points.iter().all(|e| e.main.is_none()),
                "{lesson}"
            );
        }
    }

    #[test]
    fn rutos_window_names_the_sapphire_once() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (gift, flow, call) in FORETOLD {
            let gift = GIFTS.iter().find(|g| g.flow == gift).unwrap();
            let mut flow = flows::load(&romfs, flow).unwrap().unwrap();
            rename_calls(&mut flow, call, gift.item, "Item_A").unwrap();
            assert!(flows::render(&flow).contains(&format!(r#"{}="Item_A""#, flag::ACTOR_NAME)));
        }
    }
}
