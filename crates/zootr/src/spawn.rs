//! Where a new game starts. The sequence graph boots a new file into a
//! map and spot (`FirstBancParam`, then the opening's map load), so the
//! file starts there, at that map's age, with no warp after the opening.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::Patch;
use ounce_formats::{Actor, EventKind, Flowchart, Params, Romfs, Value};

use crate::flows::{self, Rewrites};
use crate::known::{Name, actor, field, flag, flow, map, scene};
use crate::starting::{self, OPENING};
use crate::world;

const ADULT: &str = "_Adult";
/// Where an adult start wakes: the Prelude of Light's arrival in the
/// Temple of Time, as Ship's.
pub const ADULT_START: (&Name, &Name) = (map::MARKET_ADULT, flag::LIGHT_PRELUDE);
/// The map a new file boots into.
const FIRST: &Name = map::KOKIRI_FOREST_CHILD;
/// The swap's trade of each age's weapon and shield.
const GEAR: &Name = flow::REPLACE_EQUIPMENT_FOR_ERA_CHANGE;

/// `params`' `key` when it's a plain string, not an argument.
fn text<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    match params.iter().find(|(k, _)| k == key)? {
        (_, Value::String(s)) => s.first().map(String::as_str),
        _ => None,
    }
}

/// Every `MAP/SPOT` a spawn can name: where scene exits arrive, and where
/// flows send Link (warp pads, blue warps).
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
            let Some(spot) = text(params, field::NEXT_START_POS) else {
                continue;
            };
            let maps = [field::NEXT_BANC, flow::CHILD_BANC, flow::ADULT_BANC];
            for map in maps.into_iter().filter_map(|k| text(params, k)) {
                out.insert(format!("{map}/{spot}"));
            }
        }
    }
    out.retain(|d| !d.ends_with('/'));
    Ok(out)
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

/// The opening's events set in Kokiri Forest: its terrain, and the
/// camera's and Navi's spots there.
const KOKIRI: [(&Name, &Name); 4] = [
    (
        field::TERRAIN_SYSTEM_ACTOR,
        flow::EVENT_TERRAIN_PIN_RESOURCE,
    ),
    (flow::EVENT_CAMERA, flow::EVENT_MOVE_TO_TARGET_POS_CAMERA),
    (actor::NPC_NAVI, flow::EVENT_WARP_TO_TARGET_POS_AND_ROT),
    (
        actor::NPC_NAVI,
        flow::EVENT_TRIGGER_NAVI_WARP_TO_TARGET_POS_AND_ROT,
    ),
];

/// Step over the opening's Kokiri Forest events, for a boot elsewhere.
fn away(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, OPENING, |flow| {
        for what in KOKIRI {
            for e in flows::running(flow, what) {
                flows::step_over(flow, e)?;
            }
        }
        Ok(())
    })
}

/// Before the opening's autosave, equip each age's gear from the start
/// inventory (`owned`), as the pedestal's swap leaves it on a first trip:
/// child's Kokiri Sword and shield, adult's Master Sword only when held,
/// and no adult shield.
fn adult_gear(romfs: &Romfs, rewrites: &mut Rewrites, owned: &[&str]) -> Result<()> {
    let gear = flows::timeline(romfs, GEAR)?;
    // Adult Link never equips child gear, so a grant can't fill the slots.
    // The start inventory names each item as it drops, the slots as held.
    let first = |names: &[&'static Name]| {
        let held = names
            .iter()
            .find(|&&n| owned.contains(&starting::chest_item(n).get()));
        held.map_or("", |n| n.get())
    };
    let slots = [
        (
            flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_WEAPON,
            first(&[actor::WEAPON_SWORD_KOKIRI]),
        ),
        (
            flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_SHIELD,
            first(&[actor::WEAPON_SHIELD_DEKU, actor::WEAPON_SHIELD_HYLIAN]),
        ),
        (
            flag::PLAYER_STATUS_EQUIP_WEAPON,
            first(&[actor::WEAPON_SWORD_MASTER]),
        ),
        (flag::PLAYER_STATUS_EQUIP_SHIELD, ""),
    ];
    let set = |(name, value): (&Name, &str)| -> Result<_> {
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
    let events = slots.into_iter().map(set).collect::<Result<Vec<_>>>()?;
    flows::edit(romfs, rewrites, OPENING, |flow| {
        starting::before_save(flow, events)
    })
}

/// A sequence graph's path.
fn sequence(name: &Name, kind: &str) -> String {
    format!("Sequence/{name}.{kind}.ainb")
}

/// The first node in `patch`'s graph named `name`.
fn named(patch: &Patch, name: &Name) -> Result<usize> {
    (patch.graph().nodes.iter())
        .position(|n| n.name == name.get())
        .with_context(|| format!("no {name} node"))
}

/// Patch the sequence graphs so a new file boots into `map` at `spot`.
fn first_map(romfs: &Romfs, rewrites: &mut Rewrites, map: &str, spot: &str) -> Result<()> {
    let rel = sequence(field::FORMAL_SEQUENCE, "root");
    let mut root = Patch::new(romfs.read(&rel)?)?;
    let title = named(&root, field::TITLE_MODULE)?;
    let param = (root.graph().nodes[title].str(field::FIRST_BANC_PARAM)).with_context(|| {
        format!(
            "{} reads no {}",
            field::TITLE_MODULE,
            field::FIRST_BANC_PARAM
        )
    })?;
    ensure!(
        param.contains(FIRST.get()),
        "the first map is no longer {FIRST}"
    );
    let param = param.replacen(FIRST.get(), map, 1);
    root.set_str(title, field::FIRST_BANC_PARAM, &param)?;
    rewrites.insert(rel, root.into_bytes());
    let rel = sequence(field::OPENING, "module");
    let mut opening = Patch::new(romfs.read(&rel)?)?;
    let load = named(&opening, field::CHANGE_BANC_MODULE)?;
    opening.set_bool(load, field::IS_OVERRIDE_TRAVERSE_BASE_POS, true)?;
    opening.set_str(load, field::OVERRIDE_TRAVERSE_START_POS_NAME, spot)?;
    rewrites.insert(rel, opening.into_bytes());
    Ok(())
}

/// Start a new game at `spawn` (`MAP/SPOT`): the file boots there, adult
/// on an adult map, geared from the start inventory (`owned`).
/// Returns the GameData edits it needs.
pub fn boot(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    spawn: &str,
    owned: &[&str],
) -> Result<Vec<String>> {
    let (map, spot) = spawn.split_once('/').context("a spawn is MAP/SPOT")?;
    ensure!(
        destinations(romfs)?.contains(spawn),
        "the game never sends Link to {spawn}. `zootr spawns` lists where it does"
    );
    first_map(romfs, rewrites, map, spot)?;
    if map != FIRST.get() {
        away(romfs, rewrites)?;
    }
    if !map.ends_with(ADULT) {
        return Ok(vec![]);
    }
    adult_gear(romfs, rewrites, owned)?;
    // Child Link first stands in the Temple after a swap, never through its
    // door, so the scene of his first entry would fire on his way out.
    let entered = format!("{}.{}", flag::HAVE_PLAYED_EVENT, scene::TIME_TEMPLE_0000);
    Ok(vec![
        format!("{}=false", flag::SEQUENCE_IS_CHILDHOOD),
        format!("{entered}=true"),
    ])
}

/// `MAP/SPOT` of [`ADULT_START`].
pub fn adult_start() -> String {
    format!("{}/{}", ADULT_START.0, ADULT_START.1)
}

#[cfg(test)]
mod tests {
    use ounce_formats::ainb::Graph;

    use super::*;
    use crate::known::field;

    /// A spawn's rewrites and GameData edits, for a start holding `owned`.
    fn booted(romfs: &Romfs, spawn: &str, owned: &[&str]) -> (Rewrites, Vec<String>) {
        let mut rewrites = Rewrites::new();
        let flags = boot(romfs, &mut rewrites, spawn, owned).unwrap();
        (rewrites, flags)
    }

    /// The node `name` of patched sequence graph `graph`.
    fn node<'a>(graph: &'a Graph, name: &Name) -> &'a ounce_formats::ainb::Node {
        graph.nodes.iter().find(|n| n.name == name.get()).unwrap()
    }

    #[test]
    fn a_spawn_boots_the_new_file_into_its_map_and_spot() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let spawn = format!("{}/{}", map::GRAVEYARD_CHILD, flag::SHADOW_NOCTURNE);
        let (rewrites, flags) = booted(&romfs, &spawn, &[]);
        let root = Graph::parse(&rewrites[&sequence(field::FORMAL_SEQUENCE, "root")]).unwrap();
        let first = node(&root, field::TITLE_MODULE)
            .str(field::FIRST_BANC_PARAM)
            .unwrap();
        assert!(first.contains(map::GRAVEYARD_CHILD.get()) && !first.contains(FIRST.get()));
        let opening = Graph::parse(&rewrites[&sequence(field::OPENING, "module")]).unwrap();
        let load = node(&opening, field::CHANGE_BANC_MODULE);
        assert_eq!(
            load.value(field::IS_OVERRIDE_TRAVERSE_BASE_POS),
            Some(&ounce_formats::ainb::Value::Bool(true))
        );
        assert_eq!(
            load.str(field::OVERRIDE_TRAVERSE_START_POS_NAME),
            Some(flag::SHADOW_NOCTURNE.get())
        );
        // A child map leaves age and gear alone, and the opening keeps
        // nothing set in Kokiri Forest.
        assert!(flags.is_empty());
        let flow = Flowchart::parse(&rewrites[&flows::rel(OPENING)]);
        let flow = flow.unwrap().unwrap();
        let rendered = flows::render(&flow);
        assert!(!rendered.contains(flag::PLAYER_STATUS_EQUIP_WEAPON.get()));
        let start = flow
            .entry_points
            .iter()
            .map(|e| flows::reachable(&flow, &e.name));
        let reached: BTreeSet<u16> = start.flat_map(|r| r.unwrap()).collect();
        for what in KOKIRI {
            assert!(
                flows::running(&flow, what)
                    .iter()
                    .all(|e| !reached.contains(e))
            );
        }
    }

    #[test]
    fn an_adult_boot_is_adult_and_geared_from_the_start_inventory() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let opening = |owned: &[&'static Name]| {
            let owned: Vec<_> = owned
                .iter()
                .map(|&n| starting::chest_item(n).get())
                .collect();
            let (rewrites, flags) = booted(&romfs, &adult_start(), &owned);
            let entered = format!("{}.{}", flag::HAVE_PLAYED_EVENT, scene::TIME_TEMPLE_0000);
            let edits = [
                format!("{}=false", flag::SEQUENCE_IS_CHILDHOOD),
                format!("{entered}=true"),
            ];
            assert_eq!(flags, edits);
            let flow = Flowchart::parse(&rewrites[&flows::rel(OPENING)]);
            flow.unwrap().unwrap()
        };
        // Each slot's value, from the event that sets it.
        let slots = |flow: &Flowchart| -> Vec<String> {
            let rendered = flows::render(flow);
            let slot = |name: &Name| {
                let named = format!("{}=\"{}\"", field::GAME_DATA_NAME, name.get());
                let line = rendered.lines().find(|l| l.contains(&named)).unwrap();
                let value = line.split(&format!("{}=\"", field::VALUE)).nth(1).unwrap();
                value.split('"').next().unwrap().to_string()
            };
            [
                flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_WEAPON,
                flag::PLAYER_STATUS_ANOTHER_ERA_EQUIP_SHIELD,
                flag::PLAYER_STATUS_EQUIP_WEAPON,
                flag::PLAYER_STATUS_EQUIP_SHIELD,
            ]
            .map(slot)
            .to_vec()
        };
        let bare = opening(&[]);
        assert_eq!(slots(&bare), ["", "", "", ""]);
        // The gear is set before the autosave, and the save ends it.
        let save = flows::only(&bare, starting::AUTOSAVE).unwrap();
        let EventKind::Action { next, .. } = bare.events[save as usize].kind else {
            unreachable!()
        };
        assert_eq!(next, None);
        let geared = opening(&[
            actor::WEAPON_SWORD_KOKIRI,
            actor::WEAPON_SHIELD_HYLIAN,
            actor::WEAPON_SHIELD_DEKU,
            actor::WEAPON_SWORD_MASTER,
        ]);
        let held = [
            actor::WEAPON_SWORD_KOKIRI.get(),
            actor::WEAPON_SHIELD_DEKU.get(),
            actor::WEAPON_SWORD_MASTER.get(),
            "",
        ];
        assert_eq!(slots(&geared), held);
    }

    #[test]
    fn only_spots_the_game_uses_are_taken() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let known = destinations(&romfs).unwrap();
        assert!(known.contains(&adult_start()));
        let refused = |spawn: &str| {
            boot(&romfs, &mut Rewrites::new(), spawn, &[])
                .unwrap_err()
                .to_string()
        };
        assert!(refused(&format!("{}/Nowhere", map::GRAVEYARD_CHILD)).contains("never sends"));
        assert!(refused(map::GRAVEYARD_CHILD.get()).contains("MAP/SPOT"));
    }
}
