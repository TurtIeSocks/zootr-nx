//! A chest dealt a level of an upgrade line gives the level Link needs,
//! picked in the chest's own flow rather than by the hook. Each chest flow
//! gets an entry per such item: the pouch questions a gift asks
//! (`gifts/levels.rs`), then the call its `Success` makes, naming the
//! level. An event row of the same name runs that entry, and each chest
//! holding the item names the row as its get event, which replaces the
//! chest's default one.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, EventKind, Flowchart, Romfs};

use crate::checks::{self, DROP_ACTOR, Drop, GYAML, str_field};
use crate::events::Flows;
use crate::flows::{self, Model, Rewrites};
use crate::gifts::levels;
use crate::known::{Name, field, flow};
use crate::progressive;
use crate::rsdb::Events;

/// The chest flows. Each is the get event its chests run by default.
const FLOWS: [&Name; 2] = [flow::SY_TREASUREBOX, flow::SY_TREASUREBOX_SMALL];

/// The event, and the entry it runs, giving `item` by level in chest flow
/// `flow`.
fn event(flow: &str, item: &str) -> String {
    format!("{flow}_Zootr_{item}")
}

/// The items in `drops` a chest gives by level.
pub fn chained(drops: &HashMap<u64, Drop>, progressive: bool) -> BTreeSet<String> {
    (drops.values())
        .map(|d| &d.item)
        .filter(|i| progressive && progressive::in_line(i))
        .cloned()
        .collect()
}

/// Add an entry giving each of `items` by level to both chest flows.
pub fn give(
    romfs: &Romfs,
    flows: &Flows,
    rewrites: &mut Rewrites,
    items: &BTreeSet<String>,
) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let model = levels::model(flows)?;
    for chest in FLOWS {
        for item in items {
            flows::edit(romfs, rewrites, chest, |f| entry(f, &model, chest, item))
                .with_context(|| format!("{} giving {item} by level", chest.get()))?;
        }
    }
    Ok(())
}

/// Add to chest flow `f`, named `chest`, the entry giving `item` by level:
/// a copy of `Success` whose call to `Success_` names `item`, behind the
/// pouch questions. The small chest's sets a Navi flag first.
fn entry(f: &mut Flowchart, model: &Model, chest: &str, item: &str) -> Result<()> {
    let copied = flows::copy_entry(f, flow::SUCCESS, &event(chest, item))?;
    let mut calls = copied.into_values().filter(|&e| {
        matches!(&f.events[e as usize].kind, EventKind::SubFlow { entry_point, .. }
            if *entry_point == **flow::SUCCESS_)
    });
    let at = calls.next().context("Success never calls Success_")?;
    ensure!(calls.next().is_none(), "Success calls Success_ twice");
    let call = flows::params_mut(&mut f.events[at as usize].kind);
    flows::set(call, flow::CONTENT_NAME, flows::string(item))?;
    levels::by_level_at(f, model, item, at)
}

/// Add the event rows running each entry [`give`] adds, each like its
/// flow's own row.
pub fn events(events: &mut Events, items: &BTreeSet<String>) -> Result<()> {
    for chest in FLOWS {
        for item in items {
            events.copy(&event(chest, item), chest)?;
        }
    }
    Ok(())
}

/// Name, as the get event of each chest in `root` holding one of `items`,
/// the row giving it by level in the flow the chest runs. A chest that
/// keeps an event of its own (a scene) is left to the hook. `defaults`
/// caches each holder's default event. Returns how many were named.
pub fn name(
    romfs: &Romfs,
    root: &mut Byml,
    items: &BTreeSet<String>,
    defaults: &mut HashMap<String, Option<String>>,
) -> Result<usize> {
    let mut named = 0;
    for actor in checks::actors_mut(root)? {
        let gyaml = str_field(actor, GYAML).unwrap_or_default().to_owned();
        let Some(dynamic) = actor.as_mut_map()?.get_mut("Dynamic") else {
            continue;
        };
        let Some(item) = str_field(dynamic, DROP_ACTOR).filter(|i| items.contains(*i)) else {
            continue;
        };
        if str_field(dynamic, field::GET_EVENT_NAME).is_some() {
            continue;
        }
        let item = item.to_owned();
        let default = match defaults.get(&gyaml) {
            Some(d) => d.clone(),
            None => {
                let d = crate::tools::blackboard_string(romfs, &gyaml, field::GET_EVENT_NAME)?;
                defaults.insert(gyaml, d.clone());
                d
            }
        };
        let Some(chest) = default.filter(|d| FLOWS.iter().any(|f| ***f == **d)) else {
            continue;
        };
        let event = Byml::String(event(&chest, &item).into());
        dynamic
            .as_mut_map()?
            .insert(field::GET_EVENT_NAME.get().into(), event);
        named += 1;
    }
    Ok(named)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item};

    #[test]
    fn each_chest_flow_gives_a_line_item_by_the_pouch() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let flows = Flows::load(&romfs).unwrap();
        let mut rewrites = Rewrites::default();
        let items = BTreeSet::from([item::HOOKSHOT.to_string()]);
        give(&romfs, &flows, &mut rewrites, &items).unwrap();
        for chest in FLOWS {
            let data = &rewrites[&flows::rel(chest)];
            let flow = Flowchart::parse(data).unwrap().unwrap();
            let name = event(chest, item::HOOKSHOT);
            assert!(flow.entry_points.iter().any(|e| e.name == name), "{name}");
            let text = flows::render(&flow);
            let asks = format!("Actor=\"{}\"", item::HOOKSHOT.get());
            assert!(text.contains(&asks), "{} never asks {asks}", chest.get());
            let gives = format!("ContentName=\"{}\"", item::LONGSHOT.get());
            assert!(text.contains(&gives), "{} never gives {gives}", chest.get());
        }
    }

    #[test]
    fn a_chest_holding_a_line_item_names_its_flow_s_row() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let chest = |gyaml: &str, item: &str| {
            let dynamic = Byml::from_iter([(DROP_ACTOR.get(), Byml::from(item))]);
            Byml::from_iter([
                ("Hash", Byml::from(1u64)),
                (GYAML, gyaml.into()),
                ("Dynamic", dynamic),
            ])
        };
        let mut root = Byml::from_iter([(
            "Actors",
            Byml::from(vec![
                chest(actor::OBJ_T_BOX_A_02, item::HOOKSHOT),
                chest(actor::OBJ_T_BOX_A_01, item::HOOKSHOT),
                chest(actor::OBJ_T_BOX_A_01, actor::OBJ_RUPEE_RED),
            ]),
        )]);
        let items = BTreeSet::from([item::HOOKSHOT.to_string()]);
        let n = name(&romfs, &mut root, &items, &mut HashMap::new()).unwrap();
        assert_eq!(n, 2);
        let actors = checks::actors_mut(&mut root).unwrap();
        let named = |a: &Byml| {
            let dynamic = &a.as_map().unwrap()["Dynamic"];
            str_field(dynamic, field::GET_EVENT_NAME).map(str::to_owned)
        };
        let big = event(flow::SY_TREASUREBOX, item::HOOKSHOT);
        let small = event(flow::SY_TREASUREBOX_SMALL, item::HOOKSHOT);
        assert_eq!(named(&actors[0]), Some(big));
        assert_eq!(named(&actors[1]), Some(small));
        assert_eq!(named(&actors[2]), None);
    }

    #[test]
    fn only_line_items_are_chained_and_only_with_progressive_items() {
        needs_names!();
        let drops = HashMap::from([
            (1, Drop::from(item::HOOKSHOT.to_string())),
            (2, Drop::from(actor::OBJ_RUPEE_RED.to_string())),
        ]);
        let hookshot = BTreeSet::from([item::HOOKSHOT.to_string()]);
        assert_eq!(chained(&drops, true), hookshot);
        assert!(chained(&drops, false).is_empty());
    }
}
