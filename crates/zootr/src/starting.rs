//! Items a new game starts with: granted in the opening flow before the
//! first autosave, and swapped out of the shuffle pool.

use anyhow::{Context, Result, bail};
use ounce_formats::{Actor, EventKind, Flowchart, Params, Romfs, Value};

use crate::flows::{self, ACTOR, GRANT, Model, Rewrites, only};
use crate::known::{Name, actor, field, flag, flow, scene};
use crate::steps;
use crate::{grant as inline, items};

mod inventory;
pub use inventory::{
    ARSENAL, DEFAULT, Grant, NONE, SETTING, Start, WORDS, chest_item, click, granted,
    items as view, onto, parse, upgrade,
};

/// What a starting item's chest holds instead.
pub const FILLER: &Name = actor::T_BOX_RUPEE_BLUE;

pub(crate) const OPENING: &Name = flow::OPENING_EVENT;
/// Saria's ocarina, a grant without a "You got" window.
pub(crate) const TEMPLATE: &Name = scene::KOKIRI_0155;
pub(crate) const AUTOSAVE: (&Name, &Name) = (
    field::GAME_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_REQUEST_AUTO_SAVE,
);
/// Navi's tutorial on first entering the Deku Tree sets this, and the D-pad
/// HUD and calling Navi stay off until it does.
pub(crate) fn navi_guide() -> String {
    format!("{}.Navi=true", flag::ACTION_GUIDE_IS_END)
}

/// Run `events` (actor, action, params) in `flow` just before its
/// autosave, in order.
pub(crate) fn before_save(flow: &mut Flowchart, events: Vec<(&Actor, &str, Params)>) -> Result<()> {
    let save = only(flow, AUTOSAVE)?;
    let callers = flow.links_to(save);
    let mut first = save;
    for (actor, action, params) in events.into_iter().rev() {
        let (actor, action) = flow.action(actor, action);
        first = flow.push(EventKind::Action {
            next: Some(first),
            actor,
            action,
            params,
        });
    }
    for link in callers {
        flow.link(link, Some(first))?;
    }
    Ok(())
}

/// Run each `(challenge, step)` in the opening, just before its autosave
/// and in order, as copies of the quest flow's own progress event.
pub fn replay(romfs: &Romfs, rewrites: &mut Rewrites, steps: &[(String, String)]) -> Result<()> {
    if steps.is_empty() {
        return Ok(());
    }
    let progress = steps::progress(&steps::template(romfs)?)?;
    let events = steps
        .iter()
        .map(|(challenge, step)| {
            let params = steps::moving(&progress, flows::string(challenge), flows::string(step))?;
            Ok((&progress.actor, progress.name, params))
        })
        .collect::<Result<Vec<_>>>()?;
    flows::edit(romfs, rewrites, OPENING, |flow| before_save(flow, events))
}

/// Saria's ocarina flow, which holds the grant event every grant copies.
pub(crate) fn template(romfs: &Romfs) -> Result<Flowchart> {
    flows::load(romfs, TEMPLATE)?.context("the ocarina flow is a timeline")
}

/// The template's grant: the actor it runs on and its params.
pub(crate) fn grant_event(template: &Flowchart) -> Result<(&Actor, &Params)> {
    let EventKind::Action { actor, params, .. } =
        &template.events[only(template, GRANT)? as usize].kind
    else {
        unreachable!("only finds actions");
    };
    Ok((template.actor_of(*actor), params))
}

/// `params` granting `actor`, equipped or not. Both are values, so a flow can
/// pass them as arguments.
pub(crate) fn granting(params: &Params, actor: Value, equip: Value) -> Result<Params> {
    let mut params = params.clone();
    flows::set(&mut params, ACTOR, actor)?;
    flows::set(&mut params, field::IS_EQUIP_D_IRECT, equip)?;
    Ok(params)
}

/// Grant `grants` in `flow` just before its autosave, in order, with
/// `template`'s grant event as the model and `wait` after each one.
///
/// A grant queues String64 writes in a fixed pool, and the game aborts
/// when it's full (`0xd24e50`). Grants back to back filled it: 112
/// starting items crashed every new game. With a frame between grants the
/// pool empties, and all 112 land.
fn chain(flow: &mut Flowchart, template: &Flowchart, wait: &Model, grants: &[Grant]) -> Result<()> {
    let (actor, params) = grant_event(template)?;
    let mut events = Vec::with_capacity(grants.len() * 2);
    for g in grants {
        let mut params = granting(
            params,
            Value::String(vec![g.actor.clone()]),
            Value::Bool(vec![g.equip]),
        )?;
        flows::set(&mut params, flows::NUM, Value::Int(vec![g.num]))?;
        events.push((actor, GRANT.1.get(), params));
        events.push((&wait.actor, wait.name, wait.params.clone()));
    }
    before_save(flow, events)
}

/// `grants`, then each of `give` (pouch items by actor) not among them.
pub fn with_gives(mut grants: Vec<Grant>, give: &[String]) -> Vec<Grant> {
    for g in give {
        if !grants.iter().any(|o| &o.actor == g) {
            grants.push(Grant {
                actor: g.clone(),
                num: 1,
                equip: false,
            });
        }
    }
    grants
}

/// The get flow's heart gauge calls, in the order a heart container's get
/// runs them: queue the refill, show the hearts, play it. A granted
/// container raises max health and health but leaves the hearts it added
/// drawn empty (live, 2026-11-23: twenty hearts drew as three and the
/// last), so the opening runs these after its grants.
const GAUGE: [(&Name, &Name); 3] = [
    (
        field::UI_SYSTEM_ACTOR,
        flow::EVENT_TRIGGER_RESERVE_RECOVER_OR_ADD_HEART_GAUGE,
    ),
    (
        field::UI_SYSTEM_ACTOR,
        flow::EVENT_TRIGGER_SET_DISPLAY_HEART_GAUGE,
    ),
    (
        field::UI_SYSTEM_ACTOR,
        flow::EVENT_START_RECOVER_OR_ADD_HEART_GAUGE,
    ),
];

/// [`GAUGE`]'s calls, copied from the get flow, when `grants` holds a
/// heart container. None otherwise.
fn redraw(romfs: &Romfs, grants: &[Grant]) -> Result<Vec<Model>> {
    let container = actor::OBJ_HEART_CONTAINER.get();
    if !grants.iter().any(|g| g.actor == container) {
        return Ok(Vec::new());
    }
    let get = flows::load(romfs, flow::SY_GET)?.context("the get flow is a timeline")?;
    (GAUGE.iter())
        .map(|&what| {
            let at = flows::switch(&get, what)
                .with_context(|| format!("{} never runs {}", flow::SY_GET, what.1))?;
            flows::model(&get, what, at)
        })
        .collect()
}

/// Add `grants` to the opening flow in `rewrites`, or the game's own when
/// no other setting rewrote it, then redraw the hearts any heart container
/// added.
pub fn grant(romfs: &Romfs, rewrites: &mut Rewrites, grants: &[Grant]) -> Result<()> {
    if grants.is_empty() {
        return Ok(());
    }
    for g in grants {
        if items::get(romfs, &g.actor)?.is_none() && !crate::known::is_ours(&g.actor) {
            bail!("{} isn't a pouch item. `zootr items` lists them", g.actor);
        }
    }
    let (template, pouch) = inline::Models::load(romfs)?;
    let wait = inline::frame(&pouch)?;
    let gauge = redraw(romfs, grants)?;
    flows::edit(romfs, rewrites, OPENING, |flow| {
        chain(flow, &template, &wait, grants).context("granting the starting items")?;
        let gauge = gauge.iter().map(|m| (&m.actor, m.name, m.params.clone()));
        before_save(flow, gauge.collect())
    })
}

/// Swap one copy of each of `owned` in `pool` for the filler: the item
/// itself, else its line's level the pool holds it as. Returns the slot
/// each of `owned` left, `None` for one the pool doesn't hold.
pub fn swap_out<'a>(
    pool: &mut [&'a str],
    owned: &[&'a str],
    level: impl Fn(&'a str) -> &'a str,
) -> Vec<Option<usize>> {
    let swap = |pool: &mut [&'a str], item: &'a str| {
        let at = (pool.iter().position(|i| *i == item))
            .or_else(|| pool.iter().position(|i| *i == level(item)))?;
        pool[at] = FILLER;
        Some(at)
    };
    owned.iter().map(|&item| swap(pool, item)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flow, item};

    /// What a new game starting with `words` is granted.
    fn starting(words: &[&str]) -> Vec<Grant> {
        let mut start = Start::default();
        words.iter().for_each(|w| start.add(w, 1).unwrap());
        start.grants(&crate::inventory::Pouch::default())
    }

    #[test]
    fn swapping_leaves_a_filler_once_per_item_or_its_level_and_skips_missing_ones() {
        needs_names!();
        let mut pool = [
            item::SWORD_KOKIRI,
            "Item_A",
            item::SWORD_KOKIRI,
            item::HOOKSHOT,
        ];
        let owned = [item::SWORD_KOKIRI, item::LONGSHOT, item::OCARINA_FAIRY].map(|n| n.get());
        let level = |i: &str| match i == item::LONGSHOT.get() {
            true => item::HOOKSHOT.get(),
            false => "",
        };
        let slots = swap_out(&mut pool, &owned, level);
        assert_eq!(pool, [FILLER, "Item_A", item::SWORD_KOKIRI, FILLER]);
        assert_eq!(slots, [Some(0), Some(3), None]);
    }

    #[test]
    fn a_heart_container_redraws_the_hearts_after_the_grants() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let opening = |words: &[(&str, i32)]| {
            let mut start = Start::default();
            words.iter().for_each(|&(w, n)| start.add(w, n).unwrap());
            let pouch = crate::inventory::Pouch::load(&romfs).unwrap();
            let mut rewrites = Rewrites::new();
            grant(&romfs, &mut rewrites, &start.grants(&pouch)).unwrap();
            flows::current(&romfs, &rewrites, OPENING).unwrap()
        };
        let flow = opening(&[("heart-containers", 2), ("kokiri-sword", 1)]);
        let save = only(&flow, AUTOSAVE).unwrap();
        // Pushed last to first: the last grant, then the gauge in order,
        // then the save.
        let last = *flows::running(&flow, GRANT).first().unwrap();
        let mut at = last;
        for what in GAUGE {
            at = loop {
                let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
                    unreachable!()
                };
                let next = next.unwrap();
                if flows::running(&flow, what).contains(&next) {
                    break next;
                }
                at = next;
            };
        }
        let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
            unreachable!()
        };
        assert_eq!(next, Some(save));
        let none = opening(&[("kokiri-sword", 1)]);
        assert!(GAUGE.iter().all(|&w| flows::running(&none, w).is_empty()));
    }

    #[test]
    fn grants_run_in_order_before_the_autosave() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let before = flows::load(&romfs, OPENING).unwrap().unwrap();
        let mut flow = Flowchart::parse(&before.to_bytes()).unwrap().unwrap();
        let mut start = Start::default();
        start.add("bombs", 20).unwrap();
        start.add("bomb-bag", 1).unwrap();
        start.add("kokiri-sword", 1).unwrap();
        let pouch = crate::inventory::Pouch::load(&romfs).unwrap();
        let (template, pouch_flow) = inline::Models::load(&romfs).unwrap();
        let wait = inline::frame(&pouch_flow).unwrap();
        chain(&mut flow, &template, &wait, &start.grants(&pouch)).unwrap();
        let save = only(&flow, AUTOSAVE).unwrap();
        // Pushed last to first, so reversed they run in order: the bag, the
        // sword, then the bombs.
        let mut run = flows::running(&flow, GRANT);
        run.reverse();
        assert_eq!(run.len(), 3, "{run:?}");
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert_eq!(after.links_to(run[0]), before.links_to(save));
        // Each grant waits a frame before the next one, and the last before
        // the save.
        let next = |e: u16| match after.events[e as usize].kind {
            EventKind::Action { next, .. } => next.unwrap(),
            _ => unreachable!(),
        };
        let waits: Vec<_> = run.iter().map(|&g| next(g)).collect();
        assert_eq!(
            waits.iter().map(|&w| next(w)).collect::<Vec<_>>(),
            [run[1], run[2], save]
        );
        for &w in &waits {
            let EventKind::Action {
                actor,
                action,
                params,
                ..
            } = &after.events[w as usize].kind
            else {
                unreachable!()
            };
            let actor = after.actor_of(*actor);
            assert_eq!(
                (
                    actor.name.as_str(),
                    actor.actions[*action as usize].as_str()
                ),
                flows::said(inline::WAIT)
            );
            assert!(params.contains(&("Frames".into(), Value::Int(vec![1]))));
        }
        let params = |e: u16| match &after.events[e as usize].kind {
            EventKind::Action { params, .. } => params.clone(),
            _ => unreachable!(),
        };
        let has = |e: u16, key: &str, v: Value| params(e).contains(&(key.into(), v));
        let (bag, sword) = (run[0], run[1]);
        assert!(has(
            sword,
            "Actor",
            Value::String(vec![item::SWORD_KOKIRI.to_string()])
        ));
        assert!(has(
            sword,
            field::IS_EQUIP_D_IRECT.get(),
            Value::Bool(vec![true])
        ));
        assert!(has(run[2], flows::NUM, Value::Int(vec![20])));
        assert!(has(
            bag,
            field::IS_EQUIP_D_IRECT.get(),
            Value::Bool(vec![false])
        ));
    }

    #[test]
    fn the_slingshot_brings_its_bullet_bag() {
        needs_names!();
        let actors: Vec<_> = starting(&["slingshot"])
            .into_iter()
            .map(|g| g.actor)
            .collect();
        assert_eq!(
            actors,
            [item::RANGE_SLING_SHOT.get(), item::BULLET_BAG_SMALL.get()]
        );
    }

    #[test]
    fn given_items_come_after_the_starting_ones_once_and_unequipped() {
        needs_names!();
        let give = ["Item_A".to_owned(), item::RANGE_SLING_SHOT.to_string()];
        let actors: Vec<_> = (with_gives(starting(&["slingshot"]), &give).into_iter())
            .map(|g| (g.actor, g.equip))
            .collect();
        let want = [
            (item::RANGE_SLING_SHOT.get(), false),
            (item::BULLET_BAG_SMALL.get(), false),
            ("Item_A", false),
        ];
        assert_eq!(actors, want.map(|(a, e)| (a.to_owned(), e)));
    }

    #[test]
    fn an_unknown_given_item_is_refused() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let err = grant(
            &romfs,
            &mut Rewrites::new(),
            &with_gives(vec![], &["Item_A".into()]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Item_A"), "{err}");
        let mut rewrites = Rewrites::new();
        grant(
            &romfs,
            &mut rewrites,
            &with_gives(vec![], &[item::GLASS_TRUTH.to_string()]),
        )
        .unwrap();
        assert!(rewrites.contains_key(&flows::rel(OPENING)));
    }

    #[test]
    fn the_opening_replays_the_steps_it_is_given() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        const CHALLENGE: &Name = flow::SAGE_OF_SPIRIT;
        const STEP: &str = "Step02";
        let mut rewrites = Rewrites::new();
        let steps = [(CHALLENGE.to_string(), STEP.to_owned())];
        replay(&romfs, &mut rewrites, &steps).unwrap();
        let flow = Flowchart::parse(&rewrites[&flows::rel(OPENING)])
            .unwrap()
            .unwrap();
        // The vanilla opening already starts the Deku Tree quest.
        let ours = |e: &&u16| match &flow.events[**e as usize].kind {
            EventKind::Action { params, .. } => {
                flows::param(params, field::CHALLENGE_NAME) == Some(CHALLENGE)
            }
            _ => false,
        };
        let running = flows::running(&flow, steps::PROGRESS);
        let [at] = running.iter().filter(ours).copied().collect::<Vec<_>>()[..] else {
            panic!("one progress of ours in {running:?}")
        };
        let EventKind::Action { params, .. } = &flow.events[at as usize].kind else {
            unreachable!()
        };
        assert_eq!(flows::param(params, field::STEP_NAME), Some(STEP));
        assert!(params.contains(&(steps::QUIET.to_string(), Value::Bool(vec![true]))));
        let save = only(&flow, AUTOSAVE).unwrap();
        assert_eq!(flow.links_to(save), [ounce_formats::Link::Next(at)]);
        let none = &mut Rewrites::new();
        replay(&romfs, none, &[]).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn steps_replay_in_the_order_given() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut rewrites = Rewrites::new();
        let steps = [
            (flow::SAGE_OF_SPIRIT.to_string(), "Step02".to_owned()),
            (flow::GOTO_HYRULE_CASTLE.to_string(), "Step02".to_owned()),
        ];
        replay(&romfs, &mut rewrites, &steps).unwrap();
        let flow = Flowchart::parse(&rewrites[&flows::rel(OPENING)])
            .unwrap()
            .unwrap();
        let save = only(&flow, AUTOSAVE).unwrap();
        let [ounce_formats::Link::Next(last)] = flow.links_to(save)[..] else {
            panic!("one event before the autosave")
        };
        let named = |e: u16| match &flow.events[e as usize].kind {
            EventKind::Action { params, next, .. } => (
                flows::param(params, field::CHALLENGE_NAME).map(str::to_owned),
                *next,
            ),
            _ => panic!("not an action"),
        };
        assert_eq!(
            named(last),
            (Some(flow::GOTO_HYRULE_CASTLE.to_string()), Some(save))
        );
        let [ounce_formats::Link::Next(first)] = flow.links_to(last)[..] else {
            panic!("the first step runs the second")
        };
        assert_eq!(named(first).0.as_deref(), Some(&**flow::SAGE_OF_SPIRIT));
    }
}
