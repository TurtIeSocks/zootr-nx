//! The quest step gates in front of a check: walking up from the event or
//! placement that gives it, the nearest nodes on each path that only go on
//! for some steps of a quest. `zootr logic quest-gates` checks them against
//! the logic.
//!
//! A flow branching on a step is a gate only when every path from its entry
//! to what it gives goes through that branch, and some answers don't lead
//! there. Most of a flow's step reads pick lines (Saria's story recap reads
//! nine), and those aren't gates.

use std::collections::{BTreeSet, HashMap};

use ounce_formats::murmur3;
use ounce_formats::{EventKind, Flowchart};
use serde::Serialize;

use super::quests::{answers, compares, last, state};
use super::{Graph, Kind, Link};
use crate::flows::{reachable_avoiding, reachable_from};
use crate::steps::SAVED;

/// How far up from a giver the walk looks for a gate.
const DEPTH: usize = 6;

/// The links a giver can be started through: a flow running or calling
/// it, a placement playing it, a graph driving or placing it.
const STARTS: [Link; 4] = [Link::Runs, Link::Plays, Link::Drives, Link::Places];

/// One quest step gate.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Gate {
    pub quest: String,
    pub step: String,
    /// The sides of `step` that pass: `after`, `equal|after`, `before|equal`.
    pub side: String,
    /// The node that branches on it.
    pub by: String,
    /// The edge that reads it, for `flagmap source`.
    pub edge: usize,
}

impl Gate {
    /// The lowest step index that passes, from the quest's steps in order,
    /// or none when a later step fails it too (a `before` side), which no
    /// logic word can promise.
    pub fn least(&self, steps: &[String]) -> Option<usize> {
        let at = steps.iter().position(|s| *s == self.step)?;
        match self.side.as_str() {
            s if s.contains("before") => None,
            "after" => Some(at + 1),
            _ => Some(at),
        }
    }

    /// What the gate asks, for intersecting the gates of several paths.
    fn ask(&self) -> (&str, &str, &str) {
        (&self.quest, &self.step, &self.side)
    }
}

/// A quest step compared in a BoolExp, or read by a graph straight from
/// the step enum.
#[derive(Clone)]
pub(super) struct Cmp {
    pub(super) quest: String,
    /// `after`, `before` or `equal`: the BoolExp's true side. None for the
    /// step enum itself, whose reader names the answers it takes.
    pub(super) op: Option<&'static str>,
    pub(super) step: Option<String>,
    /// Whether the BoolExp is that one comparison, so its false side is
    /// the comparison's opposite.
    simple: bool,
}

impl Graph {
    /// Quest `quest`'s steps in order.
    pub fn steps_of(&self, quest: &str) -> Vec<String> {
        let gd = self.gamedata.as_ref();
        gd.and_then(|gd| gd.enum_values(&state(quest)))
            .unwrap_or_default()
    }

    /// The gates before the placement `hash`. One the flag map has no node
    /// for is one no graph or flow touches, so nothing gates it.
    pub fn gates_of_placement(&self, hash: u64) -> Vec<Gate> {
        let suffix = format!(" {hash}");
        let node = (self.nodes.iter())
            .position(|n| n.kind == Kind::Placement && n.name.ends_with(&suffix));
        node.map(|n| self.gates(vec![(n, None)]))
            .unwrap_or_default()
    }

    /// The gates before flow `flow` gives `item`: from every entry point
    /// that reaches an event naming it. None when no event names it.
    pub fn gates_of_grant(&self, flow: &str, item: &str) -> Option<Vec<Gate>> {
        let chart = self.flows.as_ref()?.0.get(flow)?;
        let quoted = format!("{item:?}");
        let grants: Vec<u16> = (0u16..)
            .zip(&chart.events)
            .filter(|(_, e)| params_of(&e.kind).is_some_and(|p| p.contains(&quoted)))
            .map(|(i, _)| i)
            .collect();
        self.gates_of_targets(chart, &grants)
    }

    /// The gates before the event `entry` of `flow` writes quest `quest`'s
    /// step `step`: a door the quest opens.
    pub fn gates_of_write(
        &self,
        (flow, entry): (&str, &str),
        (quest, step): (&str, &str),
    ) -> Option<Vec<Gate>> {
        let chart = self.flows.as_ref()?.0.get(flow)?;
        let node = *self.ids.get(&format!("event:{flow}/{entry}"))?;
        let saved = state(quest);
        let targets: Vec<u16> = (self.outs[node].iter())
            .map(|&e| &self.edges[e])
            .filter(|e| e.link == Link::Writes && self.nodes[e.to].name == saved)
            .filter(|e| e.label.ends_with(&format!(" {step}")))
            .filter_map(|e| e.at)
            .collect();
        self.gates_of_targets(chart, &targets)
    }

    fn gates_of_targets(&self, chart: &Flowchart, targets: &[u16]) -> Option<Vec<Gate>> {
        let mut starts = Vec::new();
        for ep in &chart.entry_points {
            let reached = reachable_from(chart, ep.main);
            let Some(&node) = self.ids.get(&format!("event:{}/{}", chart.name, ep.name)) else {
                continue;
            };
            starts.extend(
                (targets.iter())
                    .filter(|t| reached.contains(t))
                    .map(|&t| (node, Some(t))),
            );
        }
        (!starts.is_empty()).then(|| self.gates(starts))
    }

    /// The gates every one of `starts` (a node, and the event in its flow
    /// that matters) has: any one of them is a way to the check.
    fn gates(&self, starts: Vec<(usize, Option<u16>)>) -> Vec<Gate> {
        let cmps = self.step_reads();
        let ways = starts.into_iter().map(|(node, target)| {
            let mut seen = BTreeSet::from([node]);
            self.nearest(&cmps, node, target, DEPTH, &mut seen)
        });
        every(ways)
    }

    /// The nearest gates up from `node` on every path: its own, or the ones
    /// all of whatever starts it have.
    fn nearest(
        &self,
        cmps: &HashMap<usize, Vec<Cmp>>,
        node: usize,
        target: Option<u16>,
        depth: usize,
        seen: &mut BTreeSet<usize>,
    ) -> Vec<Gate> {
        let own = self.own(cmps, node, target);
        if !own.is_empty() || depth == 0 {
            return own;
        }
        let starts: Vec<(usize, Option<u16>)> = (self.ins[node].iter())
            .map(|&e| &self.edges[e])
            .filter(|e| STARTS.contains(&e.link) && !seen.contains(&e.from))
            .map(|e| (e.from, e.at))
            .collect();
        if starts.is_empty() {
            return Vec::new();
        }
        let ways: Vec<Vec<Gate>> = (starts.into_iter())
            .map(|(from, at)| {
                seen.insert(from);
                let gates = self.nearest(cmps, from, at, depth - 1, seen);
                seen.remove(&from);
                gates
            })
            .collect();
        every(ways)
    }

    /// The gates `node` itself puts before event `target` of its flow.
    fn own(&self, cmps: &HashMap<usize, Vec<Cmp>>, node: usize, target: Option<u16>) -> Vec<Gate> {
        let n = &self.nodes[node];
        let flow = match n.kind {
            Kind::Graph => None,
            Kind::Event => match (n.name.split_once('/'), target, &self.flows) {
                (Some((flow, entry)), Some(t), Some(flows)) => {
                    flows.0.get(flow).map(|c| (c, entry, t))
                }
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let mut out = BTreeSet::new();
        for &e in &self.ins[node] {
            let edge = &self.edges[e];
            let Some(found) = (edge.link == Link::Reads)
                .then(|| cmps.get(&edge.from))
                .flatten()
            else {
                continue;
            };
            for cmp in found {
                let side = match flow {
                    None if !self.wired(edge, target) => None,
                    None => graph_side(cmp, &edge.label),
                    Some((chart, entry, t)) => {
                        edge.at.and_then(|q| flow_side(cmp, chart, entry, q, t))
                    }
                };
                let Some(side) = side else {
                    continue;
                };
                let step = cmp
                    .step
                    .clone()
                    .unwrap_or_else(|| last(&edge.label).to_owned());
                // A flow giving something once checks its quest isn't done
                // yet, then moves it on: a guard, met the first time.
                if let Some((chart, ..)) = flow
                    && side.contains("before")
                    && self.moves_quest(&chart.name, &cmp.quest)
                {
                    continue;
                }
                let (quest, by) = (cmp.quest.clone(), n.id.clone());
                out.insert(Gate {
                    quest,
                    step,
                    side,
                    by,
                    edge: e,
                });
            }
        }
        out.into_iter().collect()
    }

    /// Whether event `event` brings quest `quest` to step index `least` or
    /// past it: something it runs moves the quest there, or a gate before
    /// it waits for that. An `implies` row is checked this way.
    #[cfg(test)]
    pub fn promises(&self, event: &str, quest: &str, least: usize) -> bool {
        let Some(&start) = self.ids.get(&format!("event:{event}")) else {
            return false;
        };
        let (steps, saved) = (self.steps_of(quest), state(quest));
        let at = |step: &str| steps.iter().position(|s| s == step);
        let (mut seen, mut todo) = (BTreeSet::from([start]), vec![(start, DEPTH)]);
        while let Some((n, depth)) = todo.pop() {
            for &e in &self.outs[n] {
                let edge = &self.edges[e];
                let written = edge.link == Link::Writes && self.nodes[edge.to].name == saved;
                if written && at(last(&edge.label)).is_some_and(|i| i >= least) {
                    return true;
                }
                if edge.link == Link::Runs && depth > 0 && seen.insert(edge.to) {
                    todo.push((edge.to, depth - 1));
                }
            }
        }
        (self.gates(vec![(start, None)]).iter())
            .any(|g| g.quest == quest && g.least(&steps).is_some_and(|i| i >= least))
    }

    /// The entry points of `flow`, as `FLOW/ENTRY` event names.
    #[cfg(test)]
    pub fn entries(&self, flow: &str) -> Vec<String> {
        let chart = self.flows.as_ref().and_then(|f| f.0.get(flow));
        let entries = chart.into_iter().flat_map(|c| &c.entry_points);
        entries.map(|e| format!("{flow}/{}", e.name)).collect()
    }

    /// Whether a graph's read `edge` feeds its node `target`, the one that
    /// drives or plays what's below: a read that only lights a room isn't
    /// a gate on the room's minigame. True when either end isn't known.
    fn wired(&self, edge: &super::Edge, target: Option<u16>) -> bool {
        let (Some(from), Some(to), Some(ainb)) = (edge.at, target, self.scene.get(&edge.file))
        else {
            return true;
        };
        let (from, to) = (from as usize, to as usize);
        let (mut seen, mut todo) = (BTreeSet::from([to]), vec![to]);
        while let Some(n) = todo.pop() {
            if n == from {
                return true;
            }
            let Some(node) = ainb.nodes.get(n) else {
                continue;
            };
            let sources = node.inputs.iter().flat_map(|i| &i.sources);
            todo.extend(sources.map(|l| l.node).filter(|&s| seen.insert(s)));
        }
        false
    }

    /// Whether an entry point of `flow` moves quest `quest`.
    fn moves_quest(&self, flow: &str, quest: &str) -> bool {
        let (prefix, saved) = (format!("{flow}/"), state(quest));
        (self.nodes.iter().enumerate())
            .filter(|(_, n)| n.kind == Kind::Event && n.name.starts_with(&prefix))
            .flat_map(|(i, _)| &self.outs[i])
            .map(|&e| &self.edges[e])
            .any(|e| e.link == Link::Writes && self.nodes[e.to].name == saved)
    }

    /// Every state that is a quest's step enum or a BoolExp built on one,
    /// with what it compares.
    pub(super) fn step_reads(&self) -> HashMap<usize, Vec<Cmp>> {
        let mut out: HashMap<usize, Vec<Cmp>> = HashMap::new();
        let Some(gd) = &self.gamedata else {
            return out;
        };
        for (node, n) in self.nodes.iter().enumerate() {
            let Some(quest) = (n.kind == Kind::State)
                .then(|| n.name.strip_prefix(SAVED))
                .flatten()
            else {
                continue;
            };
            if gd.enum_values(&n.name).is_none() {
                continue;
            }
            let direct = Cmp {
                quest: quest.to_owned(),
                op: None,
                step: None,
                simple: true,
            };
            out.entry(node).or_default().push(direct);
            for &e in &self.outs[node] {
                let to = self.edges[e].to;
                if self.edges[e].link != Link::Reads || self.nodes[to].kind != Kind::State {
                    continue;
                }
                let Ok(Some(expr)) = gd.expr_of(hash_of(&self.nodes[to].name)) else {
                    continue;
                };
                let simple = !expr.contains("&&") && !expr.contains("||");
                for (op, step) in compares(&expr, &n.name) {
                    let cmp = Cmp {
                        quest: quest.to_owned(),
                        op: Some(op),
                        step: Some(step.to_owned()),
                        simple,
                    };
                    for exp in self.exps_from(to) {
                        // A BoolExp built on this one shares only its true side.
                        let simple = cmp.simple && exp == to;
                        out.entry(exp).or_default().push(Cmp {
                            simple,
                            ..cmp.clone()
                        });
                    }
                }
            }
        }
        out
    }

    /// `exp` and every BoolExp built on it.
    fn exps_from(&self, exp: usize) -> BTreeSet<usize> {
        let (mut seen, mut todo) = (BTreeSet::from([exp]), vec![exp]);
        while let Some(n) = todo.pop() {
            for &e in &self.outs[n] {
                let to = self.edges[e].to;
                if self.edges[e].link == Link::Reads
                    && self.nodes[to].kind == Kind::State
                    && seen.insert(to)
                {
                    todo.push(to);
                }
            }
        }
        seen
    }
}

/// The gates every way has: what the check needs whichever way Link takes.
fn every(ways: impl IntoIterator<Item = Vec<Gate>>) -> Vec<Gate> {
    let mut ways = ways.into_iter();
    let Some(first) = ways.next() else {
        return Vec::new();
    };
    ways.fold(first, |kept, way| {
        (kept.into_iter())
            .filter(|g| way.iter().any(|w| w.ask() == g.ask()))
            .collect()
    })
}

/// The side a graph's read passes: the answers it takes from a step enum,
/// or a BoolExp's true side. None when it takes every answer.
fn graph_side(cmp: &Cmp, label: &str) -> Option<String> {
    if let Some(op) = cmp.op {
        return Some(op.to_owned());
    }
    let answers = answers(label);
    (!answers.is_empty() && answers.len() < 3).then(|| answers.join("|"))
}

/// The side a flow's query on a BoolExp passes on the way from `entry` to
/// event `t`: none unless the query `q` is on every path there and only
/// one answer leads on. A bool query takes case 0 when true.
fn flow_side(cmp: &Cmp, chart: &Flowchart, entry: &str, q: u16, t: u16) -> Option<String> {
    let op = cmp.op?;
    let main = chart.entry_points.iter().find(|e| e.name == entry)?.main;
    if reachable_avoiding(chart, main, Some(q)).contains(&t)
        || !reachable_from(chart, main).contains(&t)
    {
        return None;
    }
    let cases = match &chart.events.get(q as usize)?.kind {
        EventKind::Switch { cases, .. } | EventKind::SubFlowSwitch { cases, .. } => cases,
        _ => return None,
    };
    let leads: Vec<u32> = (cases.iter())
        .filter(|(_, to)| *to == t || reachable_from(chart, Some(*to)).contains(&t))
        .map(|&(value, _)| value)
        .collect();
    match leads[..] {
        [0] => Some(op.to_owned()),
        [1] if cmp.simple => Some(opposite(op).to_owned()),
        _ => None,
    }
}

/// The sides a comparison's false side passes.
fn opposite(op: &str) -> &'static str {
    match op {
        "after" => "before|equal",
        "before" => "equal|after",
        _ => "before|after",
    }
}

/// The params an event carries, rendered, if it has any.
fn params_of(kind: &EventKind) -> Option<String> {
    match kind {
        EventKind::Action { params, .. }
        | EventKind::Switch { params, .. }
        | EventKind::SubFlow { params, .. }
        | EventKind::SubFlowSwitch { params, .. } => Some(crate::flows::params(params)),
        _ => None,
    }
}

/// A state node's GameData hash, from its name or `#hash`.
fn hash_of(name: &str) -> u32 {
    match name.strip_prefix('#') {
        Some(hex) => u32::from_str_radix(hex, 16).unwrap_or_default(),
        None => murmur3(name.as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Gate, every};

    fn gate(step: &str, side: &str, by: &str) -> Gate {
        Gate {
            quest: "FakeSage".into(),
            step: step.into(),
            side: side.into(),
            by: by.into(),
            edge: 0,
        }
    }

    #[test]
    fn a_gate_holds_only_when_every_way_to_the_check_has_it() {
        let ways = [
            vec![
                gate("Step02", "after", "event:FakeA"),
                gate("Step01", "after", "event:FakeA"),
            ],
            vec![gate("Step02", "after", "graph:FakeB")],
        ];
        let kept = every(ways);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].step, "Step02");
        assert!(every([vec![gate("Step02", "after", "event:FakeA")], vec![]]).is_empty());
    }

    #[test]
    fn a_gate_passes_from_its_least_step_on_and_a_before_side_from_none() {
        let steps: Vec<String> = ["Ready", "Step01", "Step02", "Complete"]
            .map(String::from)
            .into();
        assert_eq!(gate("Step01", "after", "").least(&steps), Some(2));
        assert_eq!(gate("Step01", "equal|after", "").least(&steps), Some(1));
        assert_eq!(gate("Step01", "equal", "").least(&steps), Some(1));
        assert_eq!(gate("Step01", "before|equal", "").least(&steps), None);
    }
}
