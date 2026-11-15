//! What an event actually does: walk every event reachable from an entry
//! point, through sub-flows and transitions into other flows, and collect the
//! actions that change the save (items, equipment, flags, scene changes).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use anyhow::Result;
use ounce_formats::{EventKind, Flowchart, Params, Romfs, Value};

use crate::known::{Name, field, flow, scene};
use crate::{flows, rsdb};

/// Actions worth reporting, by their name in the flow.
const EFFECTS: &[&Name] = &[
    flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT,
    flow::EVENT_TRIGGER_PLAYER_REQUEST_CREATE_DYNAMIC_EQUIPMENT_NO_RELATION_POUCH,
    flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
    flow::EVENT_TRIGGER_SET_GAME_DATA_INT,
    flow::EVENT_TRIGGER_ADD_OR_SUB_GAME_DATA_INT,
    flow::EVENT_RESERVE_BANC_CHANGE,
    flow::EVENT_EXECUTE_PROGRESS_CHALLENGE_STEP,
];

/// A chest event giving Link a sword or such to pose with, not to keep.
const PROP: &Name = flow::EVENT_TRIGGER_PLAYER_REQUEST_CREATE_DYNAMIC_EQUIPMENT_NO_RELATION_POUCH;
/// What a chest with no event of its own plays.
const CHEST: &Name = scene::SY_TREASUREBOX;

/// Whether an actor's action counts as an effect.
pub type IsEffect = fn(actor: &str, action: &str) -> bool;

/// The save changes `zootr effects` reports.
pub fn saves(_actor: &str, action: &str) -> bool {
    EFFECTS.iter().any(|e| *e == action)
}

/// Params on these actions name other flows to run.
const FOLLOW: &[(&Name, &Name)] = &[
    (flow::EVENT_TRANSITION, flow::EVENT_NAME),
    (flow::EVENT_RESERVE_BANC_CHANGE, field::BOOT_EVENT_NAME),
];

/// One save-changing action, with `$arguments` filled in where known.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Effect {
    pub action: String,
    pub params: String,
    /// The flowchart it lives in.
    pub flow: String,
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({}) in {}", self.action, self.params, self.flow)
    }
}

/// Every flowchart, loaded once, and what each event name runs.
pub struct Flows(pub HashMap<String, Flowchart>, BTreeMap<String, rsdb::Runs>);

type Bindings = BTreeMap<String, String>;

impl Flows {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        let mut map = HashMap::new();
        for name in flows::names(romfs)? {
            if let Some(flow) = flows::load(romfs, &name)? {
                map.insert(name, flow);
            }
        }
        Ok(Self(map, rsdb::events(romfs)?))
    }

    fn has_entry(&self, flow: &str, entry: &str) -> bool {
        self.0
            .get(flow)
            .is_some_and(|f| f.entry_points.iter().any(|e| e.name == entry))
    }

    /// Where an event name points, tried in order:
    /// - its `EventSetting` row, the game's own lookup, when the flow and
    ///   entry it names exist;
    /// - a whole flow (`Dm_Kokiri_0063`), meaning every entry point;
    /// - an entry point with the full name in any flow
    ///   (`SageOfShdawo_GetHoverBoots` in `SageOfShadow`);
    /// - flow plus entry (`Get_StoneOfForest_GetSword`).
    ///
    /// The rest cover names without a row, like a cutscene a flow calls
    /// by entry.
    pub fn resolve<'a>(&'a self, event: &'a str) -> Vec<(&'a str, Option<&'a str>)> {
        if let Some((flow, entry)) = self.1.get(event) {
            let entry = entry.as_deref();
            if self.0.contains_key(flow) && entry.is_none_or(|e| self.has_entry(flow, e)) {
                return vec![(flow, entry)];
            }
        }
        if let Some((flow, _)) = self.0.get_key_value(event) {
            return vec![(flow, None)];
        }
        let named: Vec<_> = self
            .0
            .keys()
            .filter(|f| self.has_entry(f, event))
            .map(|f| (f.as_str(), Some(event)))
            .collect();
        if !named.is_empty() {
            return named;
        }
        event
            .match_indices('_')
            .rev()
            .map(|(i, _)| (&event[..i], &event[i + 1..]))
            .filter(|(flow, entry)| self.has_entry(flow, entry))
            .map(|(flow, entry)| (flow, Some(entry)))
            .take(1)
            .collect()
    }

    /// Everything `event` does to the save, or `None` if no flow defines it.
    pub fn effects(&self, event: &str) -> Option<BTreeSet<Effect>> {
        self.effects_where(event, saves)
    }

    /// Whether chest event `event` only shows off the vanilla item: it does
    /// nothing a plain chest doesn't, past posing Link and handing him a
    /// prop. A randomized chest drops these and opens like any other.
    pub fn shows_off(&self, event: &str) -> bool {
        let key = |e: &Effect| (e.action.clone(), e.params.clone());
        let plain: BTreeSet<_> = self
            .effects_where(CHEST, crate::cutscenes::lasting)
            .unwrap_or_default()
            .iter()
            .map(key)
            .collect();
        self.effects_where(event, crate::cutscenes::lasting)
            .is_some_and(|effects| {
                effects
                    .iter()
                    .all(|e| e.action == PROP || plain.contains(&key(e)))
            })
    }

    /// Whether event `index` of `chart` changes the save itself, through
    /// its own action or a call into another flow, ignoring what runs after.
    pub fn changes_save(&self, chart: &Flowchart, index: u16) -> bool {
        let mut walk = Walk {
            flows: self,
            is: saves,
            seen: BTreeSet::new(),
            out: BTreeSet::new(),
        };
        walk.step(&chart.name, chart, index, &Bindings::new());
        !walk.out.is_empty()
    }

    /// Every action the call `flow::entry` reaches that `is` picks out.
    pub fn effects_of_call(&self, flow: &str, entry: &str, is: IsEffect) -> BTreeSet<Effect> {
        let mut walk = Walk {
            flows: self,
            is,
            seen: BTreeSet::new(),
            out: BTreeSet::new(),
        };
        walk.entry(flow, Some(entry), &Bindings::new());
        walk.out
    }

    /// Every action `event` reaches that `is` picks out.
    pub fn effects_where(&self, event: &str, is: IsEffect) -> Option<BTreeSet<Effect>> {
        let targets = self.resolve(event);
        if targets.is_empty() {
            return None;
        }
        let mut walk = Walk {
            flows: self,
            is,
            seen: BTreeSet::new(),
            out: BTreeSet::new(),
        };
        for (flow, entry) in targets {
            walk.entry(flow, entry, &Bindings::new());
        }
        Some(walk.out)
    }
}

struct Walk<'a> {
    flows: &'a Flows,
    is: IsEffect,
    seen: BTreeSet<(String, u16, Bindings)>,
    out: BTreeSet<Effect>,
}

/// A param's value as text, with `$argument`s looked up in `args`.
fn text(v: &Value, args: &Bindings) -> String {
    match v {
        Value::Argument(a) => args.get(a).cloned().unwrap_or_else(|| format!("${a}")),
        v => flows::value(v),
    }
}

fn bind(params: &Params, args: &Bindings) -> Bindings {
    params
        .iter()
        .map(|(k, v)| (k.clone(), text(v, args)))
        .collect()
}

impl Walk<'_> {
    fn entry(&mut self, flow: &str, entry: Option<&str>, args: &Bindings) {
        let Some(chart) = self.flows.0.get(flow) else {
            return;
        };
        for ep in &chart.entry_points {
            if entry.is_none_or(|e| e == ep.name)
                && let Some(main) = ep.main
            {
                self.event(flow, main, args);
            }
        }
    }

    fn call(&mut self, flow: &str, callee: &str, entry: &str, params: &Params, args: &Bindings) {
        let callee = if callee.is_empty() { flow } else { callee };
        self.entry(callee, Some(entry), &bind(params, args));
    }

    fn event(&mut self, flow: &str, index: u16, args: &Bindings) {
        if !self.seen.insert((flow.to_owned(), index, args.clone())) {
            return;
        }
        let chart = &self.flows.0[flow];
        for n in self.step(flow, chart, index, args) {
            self.event(flow, n, args);
        }
    }

    /// Record what event `index` of `chart` does itself, following calls and
    /// transitions into other flows, and return the events it runs next.
    fn step(&mut self, flow: &str, chart: &Flowchart, index: u16, args: &Bindings) -> Vec<u16> {
        let mut next = Vec::new();
        match &chart.events[index as usize].kind {
            EventKind::Action {
                next: n,
                actor,
                action,
                params,
            } => {
                let actor = chart.actor_of(*actor);
                let action = &actor.actions[*action as usize];
                let p: Vec<_> = params
                    .iter()
                    .map(|(k, v)| format!("{k}={}", text(v, args)))
                    .collect();
                // UI_ flags only track tutorial popups.
                let ui = p
                    .iter()
                    .any(|p| p.starts_with(&format!("{}=\"UI_", field::GAME_DATA_NAME)));
                if (self.is)(&actor.name, action) && !ui {
                    self.out.insert(Effect {
                        action: action.clone(),
                        params: p.join(" "),
                        flow: flow.to_owned(),
                    });
                }
                for (name, key) in FOLLOW {
                    if action == name
                        && let Some(target) = params.iter().find(|(k, _)| k == key)
                    {
                        let target = text(&target.1, args);
                        let target = target.trim_matches('"');
                        for (f, e) in self.flows.resolve(target) {
                            self.entry(f, e, &Bindings::new());
                        }
                    }
                }
                next.extend(*n);
            }
            EventKind::Switch { cases, .. } => next.extend(cases.iter().map(|c| c.1)),
            EventKind::Fork { join, branches } => {
                next.extend(branches);
                next.push(*join);
            }
            EventKind::Join { next: n } => next.extend(*n),
            EventKind::SubFlow {
                next: n,
                params,
                flowchart,
                entry_point,
            } => {
                self.call(flow, flowchart, entry_point, params, args);
                next.extend(*n);
            }
            EventKind::SubFlowSwitch {
                params,
                flowchart,
                entry_point,
                cases,
            } => {
                self.call(flow, flowchart, entry_point, params, args);
                next.extend(cases.iter().map(|c| c.1));
            }
            EventKind::Return { .. } => {}
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::field;

    #[test]
    fn only_posing_chest_events_show_off() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let flows = Flows::load(&romfs).unwrap();
        assert!(flows.shows_off(field::GET_STONE_OF_FOREST_GET_SWORD));
        assert!(flows.shows_off(field::SAGE_OF_SHDAWO_GET_HOVER_BOOTS));
        assert!(!flows.shows_off(field::SAGE_OF_SPIRIT_NABOOR_IS_TAKEN_AWAY));
        assert!(!flows.shows_off(field::SAGE_OF_WATER_MELODY_GET_WATER));
    }
}
