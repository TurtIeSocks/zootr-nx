//! Items a new game starts with: granted in the opening flow before the
//! first autosave, and swapped out of the shuffle pool.

use anyhow::{Context, Result, bail};
use ounce_formats::{Actor, EventKind, Flowchart, Params, Romfs, Value};

use crate::flows::{self, ACTOR, GRANT, Rewrites, only};
use crate::items;
use crate::known::{Name, actor, field, flag, flow, scene};
use crate::steps;

mod inventory;
pub use inventory::{
    Grant, SETTING, Start, WORDS, click, drop, granted, items as view, parse, set,
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
/// `template`'s grant event as the model.
fn chain(flow: &mut Flowchart, template: &Flowchart, grants: &[Grant]) -> Result<()> {
    let (actor, params) = grant_event(template)?;
    let events = grants.iter().map(|g| {
        let mut params = granting(
            params,
            Value::String(vec![g.actor.clone()]),
            Value::Bool(vec![g.equip]),
        )?;
        flows::set(&mut params, flows::NUM, Value::Int(vec![g.num]))?;
        Ok((actor, GRANT.1.get(), params))
    });
    before_save(flow, events.collect::<Result<_>>()?)
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

/// Add `grants` to the opening flow in `rewrites`, or the game's own when
/// no other setting rewrote it.
pub fn grant(romfs: &Romfs, rewrites: &mut Rewrites, grants: &[Grant]) -> Result<()> {
    if grants.is_empty() {
        return Ok(());
    }
    for g in grants {
        if items::get(romfs, &g.actor)?.is_none() {
            bail!("{} isn't a pouch item. `zootr items` lists them", g.actor);
        }
    }
    let template = template(romfs)?;
    flows::edit(romfs, rewrites, OPENING, |flow| {
        chain(flow, &template, grants).context("granting the starting items")
    })
}

/// Swap one copy of each of `owned` in `pool` for the filler: the item
/// itself, else its line's level the pool holds it as.
pub fn swap_out<'a>(pool: &mut [&'a str], owned: &[&'a str], level: impl Fn(&'a str) -> &'a str) {
    for &item in owned {
        let slot = match pool.iter().position(|i| *i == item) {
            Some(at) => Some(at),
            None => pool.iter().position(|i| *i == level(item)),
        };
        if let Some(at) = slot {
            pool[at] = FILLER;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Pouch;
    use crate::known::{field, flow, item};

    /// What a new game starting with `words` is granted.
    fn starting(words: &[&str]) -> Vec<Grant> {
        let mut start = Start::default();
        words.iter().for_each(|w| start.add(w, 1).unwrap());
        start.grants(&Pouch::default())
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
        swap_out(&mut pool, &owned, level);
        assert_eq!(pool, [FILLER, "Item_A", item::SWORD_KOKIRI, FILLER]);
    }

    #[test]
    fn grants_run_in_order_before_the_autosave() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let template = flows::load(&romfs, TEMPLATE).unwrap().unwrap();
        let before = flows::load(&romfs, OPENING).unwrap().unwrap();
        let mut flow = Flowchart::parse(&before.to_bytes()).unwrap().unwrap();
        let mut start = Start::default();
        start.add("bombs", 20).unwrap();
        start.add("bomb-bag", 1).unwrap();
        start.add("kokiri-sword", 1).unwrap();
        let pouch = Pouch::load(&romfs).unwrap();
        chain(&mut flow, &template, &start.grants(&pouch)).unwrap();
        let save = only(&flow, AUTOSAVE).unwrap();
        let grants = flows::running(&flow, GRANT);
        // Pushed last to first: the bag, the sword, the bombs emptied, then
        // the bombs.
        let [bombs, empty, sword, bag] = grants[..] else {
            panic!("{grants:?}")
        };
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert_eq!(after.links_to(bag), before.links_to(save));
        assert_eq!(after.links_to(sword), [ounce_formats::Link::Next(bag)]);
        assert_eq!(after.links_to(empty), [ounce_formats::Link::Next(sword)]);
        assert_eq!(after.links_to(bombs), [ounce_formats::Link::Next(empty)]);
        assert_eq!(after.links_to(save), [ounce_formats::Link::Next(bombs)]);
        let params = |e: u16| match &after.events[e as usize].kind {
            EventKind::Action { params, .. } => params.clone(),
            _ => unreachable!(),
        };
        let has = |e: u16, key: &str, v: Value| params(e).contains(&(key.into(), v));
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
        assert!(has(bombs, flows::NUM, Value::Int(vec![20])));
        assert!(has(empty, flows::NUM, Value::Int(vec![-20])));
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
        assert_eq!(
            flows::param(params, field::CHALLENGE_NAME),
            Some(&**CHALLENGE)
        );
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
