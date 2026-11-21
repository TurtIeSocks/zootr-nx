//! Where a new game starts, for setting up a test: once the opening has
//! autosaved, it sends Link to a map and spot the game already uses.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Actor, EventKind, Flowchart, Link, Params, Romfs, Value};

use crate::flows::{self, Rewrites, only};
use crate::known::{Name, actor, field, flag, flow, scene};
use crate::starting::{AUTOSAVE, OPENING};
use crate::world;

/// Loads a map and puts Link at a named spot in it.
const BANC_CHANGE: (&Name, &Name) = (field::GAME_SYSTEM_ACTOR, flow::EVENT_RESERVE_BANC_CHANGE);
/// The warp songs, whose map change is the model.
const TEMPLATE: &Name = scene::SY_OCARINA;
const ADULT: &str = "_Adult";
/// The pedestal's plain swap to adult.
const CHILD_TO_ADULT: &Name = flow::MARKET_CHILD_FIELD_CHILD_TO_ADULT;
/// The swap's trade of each age's weapon and shield.
const GEAR: &Name = flow::REPLACE_EQUIPMENT_FOR_ERA_CHANGE;

/// `params`' `key` when it's a plain string, not an argument.
fn text<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    match params.iter().find(|(k, _)| k == key)? {
        (_, Value::String(s)) => s.first().map(String::as_str),
        _ => None,
    }
}

/// Every child `MAP/SPOT` a spawn can name: where scene exits arrive, and
/// where flows send Link (warp pads, blue warps).
pub fn destinations(romfs: &Romfs) -> Result<BTreeSet<String>> {
    let mut out: BTreeSet<String> = world::exits(romfs)?
        .into_iter()
        .map(|e| format!("{}/{}", e.to, e.to_start))
        .collect();
    for name in flows::names(romfs)? {
        let Some(flow) = flows::load(romfs, &name)? else {
            continue;
        };
        // Map changes, and the warp songs' calls into theirs.
        for event in &flow.events {
            let (EventKind::Action { params, .. } | EventKind::SubFlow { params, .. }) =
                &event.kind
            else {
                continue;
            };
            let map = text(params, field::NEXT_BANC).or_else(|| text(params, flow::CHILD_BANC));
            if let (Some(map), Some(spot)) = (map, text(params, field::NEXT_START_POS)) {
                out.insert(format!("{map}/{spot}"));
            }
        }
    }
    out.retain(|d| !d.ends_with('/') && !d.contains(&format!("{ADULT}/")));
    Ok(out)
}

/// Send Link to `map`/`spot` right after `flow`'s autosave, with
/// `template`'s map change as the model.
fn chain(flow: &mut Flowchart, template: &Flowchart, map: &str, spot: &str) -> Result<()> {
    let model = *flows::running(template, BANC_CHANGE)
        .first()
        .context("no map change to copy")?;
    let EventKind::Action { actor, params, .. } = &template.events[model as usize].kind else {
        unreachable!("running finds actions");
    };
    let mut params = params.clone();
    for (key, value) in [
        (field::NEXT_BANC, map),
        (field::NEXT_START_POS, spot),
        (field::BOOT_EVENT_NAME, ""),
    ] {
        flows::set(&mut params, key, Value::String(vec![value.into()]))?;
    }
    flows::set(&mut params, field::IS_ITEM_WARP, Value::Bool(vec![false]))?;
    let event = (template.actor_of(*actor), BANC_CHANGE.1.get(), params);
    after_save(flow, vec![event])
}

/// Run `events` (actor, action, params) in `flow` right after its
/// autosave, in order.
fn after_save(flow: &mut Flowchart, events: Vec<(&Actor, &str, Params)>) -> Result<()> {
    let save = only(flow, AUTOSAVE)?;
    let EventKind::Action { mut next, .. } = flow.events[save as usize].kind else {
        unreachable!("only finds actions");
    };
    for (actor, action, params) in events.into_iter().rev() {
        let (actor, action) = flow.action(actor, action);
        next = Some(flow.push(EventKind::Action {
            next,
            actor,
            action,
            params,
        }));
    }
    flow.link(Link::Next(save), next)
}

/// `flow`'s first `action` whose `key` reads `value`, as a copy.
fn event<'a>(
    flow: &'a Flowchart,
    action: &Name,
    (key, value): (&Name, &str),
) -> Result<(&'a Actor, &'a str, Params)> {
    flow.events
        .iter()
        .find_map(|e| {
            let EventKind::Action {
                actor,
                action: a,
                params,
                ..
            } = &e.kind
            else {
                return None;
            };
            let actor = flow.actor_of(*actor);
            let name = actor.actions[*a as usize].as_str();
            (name == action.get() && text(params, key) == Some(value))
                .then(|| (actor, name, params.clone()))
        })
        .with_context(|| format!("no {action} with {key} {value}"))
}

/// Start a new game adult. Once the opening has autosaved, it does what the
/// pedestal's swap does on a first trip, step by step: each age's weapon
/// and shield trade places with the Master Sword equipped, Link grows up,
/// and he wakes in the Temple of Time. The swap itself can't be called from
/// the opening (spike: nothing ran), and played from the forest it never
/// left. The forest sage's quest starts in the opening (`door::open`).
pub fn adult(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    let load =
        |name: &Name| flows::load(romfs, name)?.with_context(|| format!("{name} is a timeline"));
    let (gear, swap) = (load(GEAR)?, load(CHILD_TO_ADULT)?);
    let copy = |to: &Name| {
        event(
            &gear,
            flow::EVENT_TRIGGER_COPY_GAME_DATA_STRING64,
            (field::DST_GAME_DATA_NAME, to.get()),
        )
    };
    let set = |name: &Name, value: &str| -> Result<_> {
        let (actor, action, mut params) = event(
            &gear,
            flow::EVENT_TRIGGER_SET_GAME_DATA_STRING64,
            (
                field::GAME_DATA_NAME,
                flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_WEAPON.get(),
            ),
        )?;
        flows::set(
            &mut params,
            field::GAME_DATA_NAME,
            flows::string(name.get()),
        )?;
        flows::set(&mut params, field::VALUE, flows::string(value))?;
        Ok((actor, action, params))
    };
    let events = vec![
        copy(flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_WEAPON)?,
        set(
            flag::PLAYER_STATUS_EQUIP_WEAPON,
            actor::WEAPON_SWORD_MASTER.get(),
        )?,
        copy(flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_SHIELD)?,
        set(flag::PLAYER_STATUS_EQUIP_SHIELD, "")?,
        event(
            &swap,
            flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
            (field::GAME_DATA_NAME, flag::SEQUENCE_IS_CHILDHOOD.get()),
        )?,
        event(
            &swap,
            BANC_CHANGE.1,
            (field::BOOT_EVENT_NAME, scene::TIME_TEMPLE_0085.get()),
        )?,
    ];
    flows::edit(romfs, rewrites, OPENING, |flow| after_save(flow, events))
}

/// Start a new game at `spawn` (`MAP/SPOT`), editing the opening flow in
/// `rewrites`.
pub fn warp(romfs: &Romfs, rewrites: &mut Rewrites, spawn: &str) -> Result<()> {
    let (map, spot) = spawn.split_once('/').context("a spawn is MAP/SPOT")?;
    ensure!(
        !map.ends_with(ADULT),
        "{map} is an adult map, and only child spawns work for now"
    );
    ensure!(
        destinations(romfs)?.contains(spawn),
        "the game never sends Link to {spawn}. `zootr spawns` lists where it does"
    );
    let template = flows::load(romfs, TEMPLATE)?.context("the ocarina flow is a timeline")?;
    flows::edit(romfs, rewrites, OPENING, |flow| {
        chain(flow, &template, map, spot).context("sending Link to the spawn")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, map};

    #[test]
    fn the_map_change_follows_the_autosave() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let template = flows::load(&romfs, TEMPLATE).unwrap().unwrap();
        let mut flow = flows::load(&romfs, OPENING).unwrap().unwrap();
        chain(
            &mut flow,
            &template,
            map::GRAVEYARD_CHILD,
            flag::SHADOW_NOCTURNE,
        )
        .unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        let save = only(&after, AUTOSAVE).unwrap();
        let warp = only(&after, BANC_CHANGE).unwrap();
        assert_eq!(after.links_to(warp), [Link::Next(save)]);
        let EventKind::Action { next, params, .. } = &after.events[warp as usize].kind else {
            unreachable!()
        };
        assert_eq!(*next, None);
        assert_eq!(
            text(params, field::NEXT_BANC),
            Some(map::GRAVEYARD_CHILD.get())
        );
        assert_eq!(
            text(params, field::NEXT_START_POS),
            Some(flag::SHADOW_NOCTURNE.get())
        );
        assert_eq!(text(params, field::BOOT_EVENT_NAME), Some(""));
    }

    #[test]
    fn an_adult_start_swaps_gear_grows_up_and_wakes_in_the_temple() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut rewrites = Rewrites::new();
        adult(&romfs, &mut rewrites).unwrap();
        let flow = Flowchart::parse(&rewrites[&flows::rel(OPENING)])
            .unwrap()
            .unwrap();
        let mut at = only(&flow, AUTOSAVE).unwrap();
        let mut after = Vec::new();
        while let EventKind::Action {
            next: Some(next), ..
        } = flow.events[at as usize].kind
        {
            at = next;
            let EventKind::Action {
                actor,
                action,
                params,
                ..
            } = &flow.events[at as usize].kind
            else {
                panic!("a call follows the autosave")
            };
            let name = flow.actor_of(*actor).actions[*action as usize].as_str();
            after.push((name, params.clone()));
        }
        let names: Vec<&str> = after.iter().map(|(n, _)| *n).collect();
        let (copy, set) = (
            flow::EVENT_TRIGGER_COPY_GAME_DATA_STRING64.get(),
            flow::EVENT_TRIGGER_SET_GAME_DATA_STRING64.get(),
        );
        let bool_ = flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL.get();
        assert_eq!(names, [copy, set, copy, set, bool_, BANC_CHANGE.1.get()]);
        let read = |i: usize, key: &Name| text(&after[i].1, key).map(str::to_owned);
        let equip = flag::PLAYER_STATUS_EQUIP_WEAPON.get().to_owned();
        assert_eq!(read(1, field::GAME_DATA_NAME), Some(equip));
        assert_eq!(
            read(1, field::VALUE).as_deref(),
            Some(actor::WEAPON_SWORD_MASTER.get())
        );
        let childhood = flag::SEQUENCE_IS_CHILDHOOD.get().to_owned();
        assert_eq!(read(4, field::GAME_DATA_NAME), Some(childhood));
        assert_eq!(
            read(5, field::NEXT_BANC).as_deref(),
            Some(map::MARKET_ADULT.get())
        );
    }

    #[test]
    fn only_known_child_spots_are_taken() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let known = destinations(&romfs).unwrap();
        assert!(known.contains(&format!(
            "{}/{}",
            map::GRAVEYARD_CHILD,
            flag::SHADOW_NOCTURNE
        )));
        assert!(!known.iter().any(|d| d.contains(ADULT)));
        let refused = |spawn: &str| {
            warp(&romfs, &mut Rewrites::new(), spawn)
                .unwrap_err()
                .to_string()
        };
        assert!(
            refused(&format!(
                "{}/{}",
                field::GRAVEYARD_ADULT,
                flag::SHADOW_NOCTURNE
            ))
            .contains("adult")
        );
        assert!(refused(&format!("{}/Nowhere", map::GRAVEYARD_CHILD)).contains("never sends"));
        assert!(refused(map::GRAVEYARD_CHILD).contains("MAP/SPOT"));
        let mut rewrites = Rewrites::new();
        warp(
            &romfs,
            &mut rewrites,
            &format!("{}/{}", map::GRAVEYARD_CHILD, flag::SHADOW_NOCTURNE),
        )
        .unwrap();
        assert!(rewrites.contains_key(&flows::rel(OPENING)));
    }
}
