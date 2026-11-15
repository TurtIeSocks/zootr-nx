//! Items a new game starts with: granted in the opening flow before the
//! first autosave, and swapped out of the shuffle pool.

use anyhow::{Context, Result, bail};
use clap::builder::{PossibleValue, PossibleValuesParser};
use ounce_formats::{Actor, EventKind, Flowchart, Params, Romfs, Value};

use crate::flows::{self, ACTOR, GRANT, Rewrites, only};
use crate::items;
use crate::known::{Name, actor, field, flag, flow, item, scene};
use crate::steps;

#[derive(Debug, PartialEq)]
pub struct Item {
    /// Its setting.
    pub id: &'static str,
    pub label: &'static str,
    /// The actor a chest or flow already grants it as.
    pub actor: &'static Name,
    pub equip: bool,
    /// Granted right after it, e.g. the Slingshot's bullet bag, which the
    /// game's own pickup adds but a quiet grant doesn't.
    pub with: Option<&'static Name>,
}

pub const ITEMS: [Item; 5] = [
    Item {
        id: "start-kokiri-sword",
        label: "Kokiri Sword",
        actor: item::SWORD_KOKIRI,
        equip: true,
        with: None,
    },
    Item {
        id: "start-deku-shield",
        label: "Deku Shield",
        actor: actor::T_BOX_SHIELD_DEKU,
        equip: true,
        with: None,
    },
    Item {
        id: "start-slingshot",
        label: "Slingshot",
        actor: item::RANGE_SLING_SHOT,
        equip: false,
        with: Some(item::BULLET_BAG_SMALL),
    },
    Item {
        id: "start-bomb-bag",
        label: "Bomb Bag",
        actor: item::BOMB_BAG_SMALL,
        equip: false,
        with: None,
    },
    Item {
        id: "start-fairy-ocarina",
        label: "Fairy Ocarina",
        actor: item::OCARINA_FAIRY,
        equip: false,
        with: None,
    },
];

const PREFIX: &str = "start-";

/// Where the app lists what a new game starts with.
pub const SUBGROUP: &str = "Starting items";

/// `--start` names: each setting id without its prefix.
pub fn parser() -> PossibleValuesParser {
    PossibleValuesParser::new(
        ITEMS
            .iter()
            .map(|i| PossibleValue::new(&i.id[PREFIX.len()..]).help(i.label)),
    )
}

/// The setting `--start <name>` turns on.
pub fn id(name: &str) -> String {
    format!("{PREFIX}{name}")
}

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

/// Each item's grants: its actor, then what comes with it, then `give`,
/// none of those equipped.
fn grants<'a>(items: &[&'a Item], give: &'a [String]) -> Vec<(&'a str, bool)> {
    items
        .iter()
        .flat_map(|i| {
            [(i.actor as &str, i.equip)]
                .into_iter()
                .chain(i.with.map(|w| (&**w, false)))
        })
        .chain(give.iter().map(|g| (g.as_str(), false)))
        .collect()
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

/// Grant `grants` (actor, equipped) in `flow` just before its autosave, in
/// order, with `template`'s grant event as the model.
fn chain(flow: &mut Flowchart, template: &Flowchart, grants: &[(&str, bool)]) -> Result<()> {
    let (actor, params) = grant_event(template)?;
    let events = grants.iter().map(|&(granted, equip)| {
        let params = granting(
            params,
            Value::String(vec![granted.into()]),
            Value::Bool(vec![equip]),
        )?;
        Ok((actor, GRANT.1.get(), params))
    });
    before_save(flow, events.collect::<Result<_>>()?)
}

/// What giving `actor` grants in a running game: the actor, equipped if it's
/// a starting item that is, then what comes with it.
pub fn granted(actor: &str) -> Vec<(&str, bool)> {
    match ITEMS.iter().find(|i| i.actor == actor) {
        Some(item) => grants(&[item], &[]),
        None => vec![(actor, false)],
    }
}

/// Add `items`, then `give` (pouch items by actor), to the opening flow in
/// `rewrites`, or the game's own when no other setting rewrote it.
pub fn grant(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    items: &[&Item],
    give: &[String],
) -> Result<()> {
    if items.is_empty() && give.is_empty() {
        return Ok(());
    }
    if !give.is_empty() {
        let known = items::scan(romfs)?;
        if let Some(g) = give.iter().find(|g| !known.iter().any(|k| &k.actor == *g)) {
            bail!("{g} isn't a pouch item. `zootr items` lists them");
        }
    }
    let template = template(romfs)?;
    flows::edit(romfs, rewrites, OPENING, |flow| {
        chain(flow, &template, &grants(items, give)).context("granting the starting items")
    })
}

/// Swap one copy of each of `items` in `pool` for the filler.
pub fn swap_out(pool: &mut [&str], items: &[&Item]) {
    for item in items {
        if let Some(slot) = pool.iter_mut().find(|i| **i == item.actor) {
            *slot = FILLER;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flow, item};

    #[test]
    fn every_id_takes_the_prefix() {
        assert!(ITEMS.iter().all(|i| i.id == id(&i.id[PREFIX.len()..])));
    }

    #[test]
    fn swapping_leaves_a_filler_once_per_item_and_skips_missing_ones() {
        needs_names!();
        let mut pool = [item::SWORD_KOKIRI, "Item_A", item::SWORD_KOKIRI];
        swap_out(&mut pool, &[&ITEMS[0], &ITEMS[4]]);
        assert_eq!(pool, [FILLER, "Item_A", item::SWORD_KOKIRI]);
    }

    #[test]
    fn grants_run_in_order_before_the_autosave() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let template = flows::load(&romfs, TEMPLATE).unwrap().unwrap();
        let before = flows::load(&romfs, OPENING).unwrap().unwrap();
        let mut flow = Flowchart::parse(&before.to_bytes()).unwrap().unwrap();
        chain(&mut flow, &template, &grants(&[&ITEMS[0], &ITEMS[3]], &[])).unwrap();
        let save = only(&flow, AUTOSAVE).unwrap();
        let grants = flows::running(&flow, GRANT);
        // Pushed last to first.
        let [bag, sword] = grants[..] else {
            panic!("{grants:?}")
        };
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert_eq!(after.links_to(sword), before.links_to(save));
        assert_eq!(after.links_to(bag), [ounce_formats::Link::Next(sword)]);
        assert_eq!(after.links_to(save), [ounce_formats::Link::Next(bag)]);
        let EventKind::Action { params, .. } = &after.events[sword as usize].kind else {
            unreachable!()
        };
        assert!(params.contains(&(
            "Actor".into(),
            Value::String(vec![ITEMS[0].actor.to_string()])
        )));
        assert!(params.contains(&(field::IS_EQUIP_D_IRECT.to_string(), Value::Bool(vec![true]))));
    }

    #[test]
    fn the_slingshot_brings_its_bullet_bag() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let template = flows::load(&romfs, TEMPLATE).unwrap().unwrap();
        let mut flow = flows::load(&romfs, OPENING).unwrap().unwrap();
        chain(&mut flow, &template, &grants(&[&ITEMS[2]], &[])).unwrap();
        let actor = |e: u16| match &flow.events[e as usize].kind {
            EventKind::Action { params, .. } => params.iter().find(|(k, _)| k == "Actor").cloned(),
            _ => None,
        };
        let [bag, sling] = flows::running(&flow, GRANT)[..] else {
            panic!("two grants")
        };
        assert_eq!(flow.links_to(bag), [ounce_formats::Link::Next(sling)]);
        let named = |s: &str| Some(("Actor".into(), Value::String(vec![s.into()])));
        assert_eq!(actor(sling), named(ITEMS[2].actor));
        assert_eq!(actor(bag), named(item::BULLET_BAG_SMALL));
    }

    #[test]
    fn given_items_come_after_the_starting_ones_unequipped() {
        let give = ["Item_A".to_owned()];
        assert_eq!(
            grants(&[&ITEMS[2]], &give),
            [
                (&**ITEMS[2].actor, false),
                (&**item::BULLET_BAG_SMALL, false),
                ("Item_A", false)
            ]
        );
    }

    #[test]
    fn an_unknown_given_item_is_refused() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let err = grant(&romfs, &mut Rewrites::new(), &[], &["Item_A".into()]).unwrap_err();
        assert!(err.to_string().contains("Item_A"), "{err}");
        let mut rewrites = Rewrites::new();
        grant(&romfs, &mut rewrites, &[], &[item::GLASS_TRUTH.to_string()]).unwrap();
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
