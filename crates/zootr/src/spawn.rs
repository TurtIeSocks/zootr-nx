//! Where a new game starts, for setting up a test: once the opening has
//! autosaved, it sends Link to a map and spot the game already uses.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Flowchart, Link, Params, Romfs, Value};

use crate::flows::{self, Rewrites, only};
use crate::known::{Name, field, flow, scene};
use crate::starting::{AUTOSAVE, OPENING};
use crate::world;

/// Loads a map and puts Link at a named spot in it.
const BANC_CHANGE: (&Name, &Name) = (field::GAME_SYSTEM_ACTOR, flow::EVENT_RESERVE_BANC_CHANGE);
/// The warp songs, whose map change is the model.
const TEMPLATE: &Name = scene::SY_OCARINA;
const ADULT: &str = "_Adult";

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
    let (actor, action) = flow.action(template.actor_of(*actor), BANC_CHANGE.1);
    let save = only(flow, AUTOSAVE)?;
    let EventKind::Action { next, .. } = flow.events[save as usize].kind else {
        unreachable!("only finds actions");
    };
    let warp = flow.push(EventKind::Action {
        next,
        actor,
        action,
        params,
    });
    flow.link(Link::Next(save), Some(warp))
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
