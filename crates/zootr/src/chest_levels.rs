//! A chest dealt a level of an upgrade line gives the level Link needs,
//! picked in the chest's own flow rather than by the hook. A chest's get
//! event replaces its default one, and every event a chest runs to give its
//! item (each chest flow's `Success`, and Mido's, which sets a Navi flag
//! first) ends in one call naming the item. Each gets a copy per such item:
//! the call naming it, behind the pouch questions a gift asks
//! (`gifts/levels.rs`). An event row of the copy's name runs it, and each
//! chest holding the item names the copy of the event it runs.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, EventKind, Flowchart, Romfs};

use crate::checks::{self, DROP_ACTOR, Drop, GYAML, str_field};
use crate::events::Flows;
use crate::flows::{self, Model, Rewrites};
use crate::gifts::levels;
use crate::known::{Name, field, flow};
use crate::progressive;
use crate::rsdb::{self, Events, Runs};

/// The chest flows.
const FLOWS: [&Name; 2] = [flow::SY_TREASUREBOX, flow::SY_TREASUREBOX_SMALL];

/// The event, and the entry it runs, standing in for `base` to give `item`
/// by level.
fn event(base: &str, item: &str) -> String {
    format!("{base}_Zootr_{item}")
}

/// A chest event a copy stands in for: the row, and the flow and entry it
/// runs.
#[derive(Debug, PartialEq)]
struct Base {
    event: String,
    flow: String,
    entry: String,
}

/// The events in `runs` that run a chest flow's `Success`, or an entry
/// named for it (`Success_` and a suffix, as Mido's), not the shared `Success_` itself.
fn bases(runs: &BTreeMap<String, Runs>) -> Vec<Base> {
    let success = |e: &str| {
        e == flow::SUCCESS.get() || e.starts_with(&**flow::SUCCESS_) && e != flow::SUCCESS_.get()
    };
    let chest = |f: &str| FLOWS.iter().any(|c| ***c == *f);
    (runs.iter())
        .filter_map(|(event, (flow, entry))| {
            let entry = entry.as_deref().filter(|e| success(e) && chest(flow))?;
            Some(Base {
                event: event.clone(),
                flow: flow.clone(),
                entry: entry.to_owned(),
            })
        })
        .collect()
}

/// The items a seed's chests give by level, and the chest events each
/// gets a copy of.
pub struct Levels {
    items: BTreeSet<String>,
    bases: Vec<Base>,
}

impl Levels {
    /// The line items in `drops`, when progressive items are on.
    pub fn new(romfs: &Romfs, drops: &HashMap<u64, Drop>, progressive: bool) -> Result<Self> {
        let items: BTreeSet<String> = (drops.values())
            .map(|d| &d.item)
            .filter(|i| progressive && progressive::in_line(i))
            .cloned()
            .collect();
        let bases = match items.is_empty() {
            true => vec![],
            false => bases(&rsdb::events(romfs)?),
        };
        Ok(Self { items, bases })
    }

    /// Add each copy to its chest flow.
    pub fn give(&self, romfs: &Romfs, flows: &Flows, rewrites: &mut Rewrites) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }
        let model = levels::model(flows)?;
        for base in &self.bases {
            for item in &self.items {
                flows::edit(romfs, rewrites, &base.flow, |f| {
                    entry(f, &model, base, item)
                })
                .with_context(|| format!("{} giving {item} by level", base.event))?;
            }
        }
        Ok(())
    }

    /// Add the event row running each copy, like its base's row.
    pub fn events(&self, events: &mut Events) -> Result<()> {
        for base in &self.bases {
            for item in &self.items {
                events.copy(&event(&base.event, item), &base.event)?;
            }
        }
        Ok(())
    }

    /// Name, as the get event of each chest in `root` holding one of the
    /// items, the copy of the event it runs: its own when it keeps one,
    /// else its holder's default. A chest whose event is no base (a scene)
    /// is left to the hook. `defaults` caches each holder's default event.
    /// Returns how many were named.
    pub fn name(
        &self,
        romfs: &Romfs,
        root: &mut Byml,
        defaults: &mut HashMap<String, Option<String>>,
    ) -> Result<usize> {
        let mut named = 0;
        for actor in checks::actors_mut(root)? {
            let gyaml = str_field(actor, GYAML).unwrap_or_default().to_owned();
            let Some(dynamic) = actor.as_mut_map()?.get_mut("Dynamic") else {
                continue;
            };
            let held = str_field(dynamic, DROP_ACTOR).filter(|i| self.items.contains(*i));
            let Some(item) = held.map(str::to_owned) else {
                continue;
            };
            let kept = str_field(dynamic, field::GET_EVENT_NAME).map(str::to_owned);
            let runs = match kept {
                Some(event) => Some(event),
                None => default(romfs, &gyaml, defaults)?,
            };
            let Some(base) = runs.filter(|r| self.bases.iter().any(|b| b.event == *r)) else {
                continue;
            };
            let event = Byml::String(event(&base, &item).into());
            dynamic
                .as_mut_map()?
                .insert(field::GET_EVENT_NAME.get().into(), event);
            named += 1;
        }
        Ok(named)
    }
}

/// The get event chest `gyaml` runs when its placement names none, cached
/// in `defaults`.
fn default(
    romfs: &Romfs,
    gyaml: &str,
    defaults: &mut HashMap<String, Option<String>>,
) -> Result<Option<String>> {
    if let Some(d) = defaults.get(gyaml) {
        return Ok(d.clone());
    }
    let d = crate::tools::blackboard_string(romfs, gyaml, field::GET_EVENT_NAME)?;
    defaults.insert(gyaml.to_owned(), d.clone());
    Ok(d)
}

/// Add to chest flow `f` the copy of `base` giving `item` by level: its
/// entry, whose call to `Success_` names `item`, behind the pouch
/// questions.
fn entry(f: &mut Flowchart, model: &Model, base: &Base, item: &str) -> Result<()> {
    let copied = flows::copy_entry(f, &base.entry, &event(&base.event, item))?;
    let mut calls = copied.into_values().filter(|&e| {
        matches!(&f.events[e as usize].kind, EventKind::SubFlow { entry_point, .. }
            if *entry_point == **flow::SUCCESS_)
    });
    let at = calls.next().context("never calls Success_")?;
    ensure!(calls.next().is_none(), "calls Success_ twice");
    let call = flows::params_mut(&mut f.events[at as usize].kind);
    flows::set(call, flow::CONTENT_NAME, flows::string(item))?;
    levels::by_level_at(f, model, item, at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item};

    /// A seed's levels with the Hookshot dealt, and the rupee chest that
    /// isn't chained.
    fn hookshot(romfs: &Romfs) -> Levels {
        let drops = HashMap::from([
            (1, Drop::from(item::HOOKSHOT.to_string())),
            (2, Drop::from(actor::OBJ_RUPEE_RED.to_string())),
        ]);
        Levels::new(romfs, &drops, true).unwrap()
    }

    #[test]
    fn the_bases_are_both_chest_flows_and_mido_s_chests() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let levels = hookshot(&romfs);
        let mut runs: Vec<(&str, &str)> = (levels.bases.iter())
            .map(|b| (b.flow.as_str(), b.entry.as_str()))
            .collect();
        runs.sort();
        let small = flow::SY_TREASUREBOX_SMALL.get();
        assert_eq!(runs.len(), 3, "{runs:?}");
        assert_eq!(runs[0], (flow::SY_TREASUREBOX.get(), flow::SUCCESS.get()));
        assert_eq!(runs[1], (small, flow::SUCCESS.get()));
        assert!(runs[2].0 == small && runs[2].1.starts_with(flow::SUCCESS_.get()));
    }

    #[test]
    fn each_base_gives_a_line_item_by_the_pouch() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let flows = Flows::load(&romfs).unwrap();
        let mut rewrites = Rewrites::default();
        let levels = hookshot(&romfs);
        levels.give(&romfs, &flows, &mut rewrites).unwrap();
        for base in &levels.bases {
            let data = &rewrites[&flows::rel(&base.flow)];
            let flow = Flowchart::parse(data).unwrap().unwrap();
            let name = event(&base.event, item::HOOKSHOT);
            assert!(flow.entry_points.iter().any(|e| e.name == name), "{name}");
            let text = flows::render(&flow);
            let asks = format!("Actor=\"{}\"", item::HOOKSHOT.get());
            assert!(text.contains(&asks), "{} never asks {asks}", base.flow);
            let gives = format!("ContentName=\"{}\"", item::LONGSHOT.get());
            assert!(text.contains(&gives), "{} never gives {gives}", base.flow);
        }
    }

    #[test]
    fn a_chest_holding_a_line_item_names_the_copy_of_the_event_it_runs() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let levels = hookshot(&romfs);
        let mido = (levels.bases.iter())
            .find(|b| b.entry != **flow::SUCCESS)
            .map(|b| b.event.clone())
            .unwrap();
        let chest = |gyaml: &str, item: &str, event: Option<&str>| {
            let mut dynamic = vec![(DROP_ACTOR.get(), Byml::from(item))];
            dynamic.extend(event.map(|e| (field::GET_EVENT_NAME.get(), Byml::from(e))));
            Byml::from_iter([(GYAML, gyaml.into()), ("Dynamic", Byml::from_iter(dynamic))])
        };
        let small = actor::OBJ_T_BOX_A_01;
        let mut root = Byml::from_iter([(
            "Actors",
            Byml::from(vec![
                chest(actor::OBJ_T_BOX_A_02, item::HOOKSHOT, None),
                chest(small, item::HOOKSHOT, None),
                chest(small, item::HOOKSHOT, Some(&mido)),
                chest(small, item::HOOKSHOT, Some("FakeScene_Open")),
                chest(small, actor::OBJ_RUPEE_RED, None),
            ]),
        )]);
        let n = levels.name(&romfs, &mut root, &mut HashMap::new()).unwrap();
        assert_eq!(n, 3);
        let actors = checks::actors_mut(&mut root).unwrap();
        let named: Vec<Option<&str>> = (actors.iter())
            .map(|a| str_field(&a.as_map().unwrap()["Dynamic"], field::GET_EVENT_NAME))
            .collect();
        let copy = |base: &str| event(base, item::HOOKSHOT);
        assert_eq!(named[0], Some(&*copy(flow::SY_TREASUREBOX)));
        assert_eq!(named[1], Some(&*copy(flow::SY_TREASUREBOX_SMALL)));
        assert_eq!(named[2], Some(&*copy(&mido)));
        assert_eq!(named[3], Some("FakeScene_Open"));
        assert_eq!(named[4], None);
    }

    #[test]
    fn nothing_is_chained_without_progressive_items() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let drops = HashMap::from([(1, Drop::from(item::HOOKSHOT.to_string()))]);
        let off = Levels::new(&romfs, &drops, false).unwrap();
        assert!(off.items.is_empty() && off.bases.is_empty());
        assert_eq!(
            hookshot(&romfs).items,
            BTreeSet::from([item::HOOKSHOT.to_string()])
        );
    }
}
