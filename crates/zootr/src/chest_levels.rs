//! A chest dealt a level of an upgrade line gives the level Link needs,
//! and one dealt a key ring or boss key copy credits its dungeon, both in
//! the chest's own flow rather than by the hook. A chest's get
//! event replaces its default one, and every event a chest runs to give its
//! item (each chest flow's `Success`, and Mido's, which sets a Navi flag
//! first) ends in one call naming the item. Each gets a copy per such item:
//! the call naming it, behind the pouch questions a gift asks
//! (`gifts/levels.rs`) or followed by the key's credit. An event row of the copy's name runs it, and each
//! chest holding the item names the copy of the event it runs.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, EventKind, Flowchart, Romfs, Value};

use crate::checks::{self, DROP_ACTOR, Drop, GYAML, str_field};
use crate::events::Flows;
use crate::flows::{self, Model, Rewrites};
use crate::gifts::levels;
use crate::key_tints::Sets;
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

/// What a copy does about its item.
#[derive(Debug, PartialEq)]
enum Gives {
    /// Picks the level Link needs.
    Level,
    /// Gives it, then sets its credit.
    Credit(Sets),
}

/// The items a seed's chests give through a copy, and the chest events
/// each gets a copy of.
pub struct Levels {
    items: BTreeMap<String, Gives>,
    bases: Vec<Base>,
}

impl Levels {
    /// The items in `drops` a copy gives: line items when progressive items
    /// are on, and the key copies `credits` names.
    pub fn new(
        romfs: &Romfs,
        drops: &HashMap<u64, Drop>,
        progressive: bool,
        credits: Vec<(String, Sets)>,
    ) -> Result<Self> {
        // An add stays the hook's alone: it credits the copy from any holder,
        // so the chest adding too would count the key twice.
        let set = credits
            .into_iter()
            .filter(|(_, s)| !matches!(s, Sets::Add(..)));
        let mut credits: HashMap<String, Sets> = set.collect();
        let mut items = BTreeMap::new();
        for item in drops.values().map(|d| &d.item) {
            let gives = match credits.remove(item) {
                Some(sets) => Some(Gives::Credit(sets)),
                None => (progressive && progressive::in_line(item)).then_some(Gives::Level),
            };
            if let Some(g) = gives {
                items.entry(item.clone()).or_insert(g);
            }
        }
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
        let models = Models {
            level: levels::model(flows)?,
            set_bool: setter(romfs)?,
        };
        for base in &self.bases {
            for (item, gives) in &self.items {
                flows::edit(romfs, rewrites, &base.flow, |f| {
                    entry(f, &models, base, item, gives)
                })
                .with_context(|| format!("{} giving {item}", base.event))?;
            }
        }
        Ok(())
    }

    /// Add the event row running each copy, like its base's row.
    pub fn events(&self, events: &mut Events) -> Result<()> {
        for base in &self.bases {
            for item in self.items.keys() {
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
            let held = str_field(dynamic, DROP_ACTOR).filter(|i| self.items.contains_key(*i));
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

/// The flow events a copy is built from.
struct Models {
    /// A pouch question.
    level: Model,
    /// A GameData Bool setter.
    set_bool: Model,
}

/// The small chest flow's GameData Bool setter (Mido's Navi flag).
fn setter(romfs: &Romfs) -> Result<Model> {
    let small = flow::SY_TREASUREBOX_SMALL;
    let f = flows::load(romfs, small)?.with_context(|| format!("{small} is a timeline"))?;
    let at = (flows::running(&f, flows::SET_BOOL).first().copied())
        .with_context(|| format!("{small} sets no Bool"))?;
    flows::model(&f, flows::SET_BOOL, at)
}

/// Add to chest flow `f` the copy of `base` giving `item`: its entry,
/// whose call to `Success_` names `item`, behind the pouch questions for a
/// level or followed by the credit for a key.
fn entry(f: &mut Flowchart, models: &Models, base: &Base, item: &str, gives: &Gives) -> Result<()> {
    let copied = flows::copy_entry(f, &base.entry, &event(&base.event, item))?;
    let mut calls = copied.into_values().filter(|&e| {
        matches!(&f.events[e as usize].kind, EventKind::SubFlow { entry_point, .. }
            if *entry_point == **flow::SUCCESS_)
    });
    let at = calls.next().context("never calls Success_")?;
    ensure!(calls.next().is_none(), "calls Success_ twice");
    let call = flows::params_mut(&mut f.events[at as usize].kind);
    flows::set(call, flow::CONTENT_NAME, flows::string(item))?;
    match gives {
        Gives::Level => levels::by_level_at(f, &models.level, item, at),
        Gives::Credit(Sets::Bool(name)) => {
            flows::set_after(f, &models.set_bool, at, name, Value::Bool(vec![true]))
        }
        Gives::Credit(Sets::Int(name, value)) => {
            let set = int_setter(models, flow::EVENT_TRIGGER_SET_GAME_DATA_INT);
            flows::set_after(f, &set, at, name, Value::Int(vec![*value]))
        }
        Gives::Credit(Sets::Add(name, n)) => {
            let add = int_setter(models, flow::EVENT_TRIGGER_ADD_OR_SUB_GAME_DATA_INT);
            flows::set_after(f, &add, at, name, Value::Int(vec![*n]))
        }
    }
}

/// The small chest's Bool setter retargeted to `action`, an Int action
/// taking the same params.
fn int_setter(models: &Models, action: &Name) -> Model {
    Model {
        name: action.get(),
        ..models.set_bool.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item};
    use crate::settings::DungeonKeys;

    /// The first ring and the first boss key copy, with what each sets.
    fn keys() -> Vec<(String, Sets)> {
        let all = crate::key_tints::sets(true, DungeonKeys::Rings);
        let first = |bool: bool| {
            let found = all.iter().find(|(_, s)| matches!(s, Sets::Bool(_)) == bool);
            found.cloned().unwrap()
        };
        vec![first(true), first(false)]
    }

    /// A seed's copies with the Hookshot and [`keys`] dealt, and a rupee
    /// that needs none.
    fn hookshot(romfs: &Romfs) -> Levels {
        let mut drops = HashMap::from([
            (1, Drop::from(item::HOOKSHOT.to_string())),
            (2, Drop::from(actor::OBJ_RUPEE_RED.to_string())),
        ]);
        for (n, (key, _)) in (3..).zip(keys()) {
            drops.insert(n, Drop::from(key));
        }
        Levels::new(
            romfs,
            &drops,
            true,
            crate::key_tints::sets(true, DungeonKeys::Rings),
        )
        .unwrap()
    }

    #[test]
    fn keysanity_s_small_keys_are_left_to_the_hook() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let sets = crate::key_tints::sets(true, DungeonKeys::Singles);
        let drops = (1..)
            .zip(&sets)
            .map(|(n, (key, _))| (n, Drop::from(key.clone())));
        let levels = Levels::new(&romfs, &drops.collect(), false, sets.clone()).unwrap();
        for (key, s) in &sets {
            let added = matches!(s, Sets::Add(..));
            assert_eq!(levels.items.contains_key(key), !added, "{key}");
        }
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
    fn each_base_credits_a_key_right_after_its_call() {
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
            for (key, sets) in keys() {
                let call = (flow.events.iter())
                    .find_map(|e| match &e.kind {
                        EventKind::SubFlow { params, next, .. }
                            if flows::param(params, flow::CONTENT_NAME) == Some(&key) =>
                        {
                            *next
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{} never gives {key}", base.event));
                let EventKind::Action {
                    actor,
                    action,
                    params,
                    ..
                } = &flow.events[call as usize].kind
                else {
                    panic!("{key}'s call isn't followed by a set");
                };
                let (setter, name, value) = match &sets {
                    Sets::Bool(n) => (
                        flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
                        n,
                        Value::Bool(vec![true]),
                    ),
                    Sets::Int(n, v) => (
                        flow::EVENT_TRIGGER_SET_GAME_DATA_INT,
                        n,
                        Value::Int(vec![*v]),
                    ),
                    Sets::Add(n, v) => (
                        flow::EVENT_TRIGGER_ADD_OR_SUB_GAME_DATA_INT,
                        n,
                        Value::Int(vec![*v]),
                    ),
                };
                let ran = &flow.actor_of(*actor).actions[*action as usize];
                assert_eq!(ran, setter.get(), "{key}");
                assert_eq!(flows::param(params, flows::GAME_DATA), Some(name.as_str()));
                let set = params
                    .iter()
                    .find(|(k, _)| k == flows::VALUE)
                    .map(|(_, v)| v);
                assert_eq!(set, Some(&value), "{key}");
            }
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
    fn levels_need_progressive_items_and_keys_their_credits() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let drops = HashMap::from([(1, Drop::from(item::HOOKSHOT.to_string()))]);
        let off = Levels::new(&romfs, &drops, false, vec![]).unwrap();
        assert!(off.items.is_empty() && off.bases.is_empty());
        assert_eq!(
            hookshot(&romfs).items.get(item::HOOKSHOT.get()),
            Some(&Gives::Level)
        );
        let (ring, sets) = keys().swap_remove(0);
        assert_eq!(
            hookshot(&romfs).items.get(&ring),
            Some(&Gives::Credit(sets))
        );
    }
}
