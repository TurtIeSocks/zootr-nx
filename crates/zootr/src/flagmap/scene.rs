//! The flag map's edges from scene logic: the graphs each level places
//! (`Logic/*.ainb`). A graph reads and writes GameData, runs the modules
//! it holds, places actors, and drives each placement it names through
//! the inputs wired into that placement's node. What a graph reads back
//! from a placement (a foe's `Dead`) or a module is that one's output.

use std::collections::HashSet;

use ounce_formats::ainb::{Graph as Ainb, Node as AinbNode, NodeKind, Value};

use super::{Graph, Kind, Link, quests};
use crate::gamedata::GameData;
use crate::gates::{Wiring, path};
use crate::known::field;

/// The boolean classes a graph computes with (`OperateAND` and kin).
const OPERATE: &str = "Operate";

pub(super) fn add(g: &mut Graph, wiring: &Wiring, gd: &GameData) {
    let mut names: Vec<&String> = wiring.graphs.keys().collect();
    names.sort();
    for name in names {
        let graph = g.node(Kind::Graph, name, || None);
        own(g, graph, &path(name), &wiring.graphs[name], gd, |child| {
            child.to_owned()
        });
    }
    let (mut driven, mut placed_actors) = (HashSet::new(), HashSet::new());
    for placed in &wiring.placed {
        let ainb = &wiring.graphs[&placed.graph];
        let (graph, file) = (
            g.node(Kind::Graph, &placed.graph, || None),
            path(&placed.graph),
        );
        for (n, hash, actor) in &placed.actors {
            let (node, at) = (&ainb.nodes[*n], Some(*n as u16));
            let place = g.node(Kind::Placement, &format!("{actor} {hash}"), || {
                Some(format!("in {}", placed.area))
            });
            if !driven.insert((graph, place)) {
                continue;
            }
            g.edge(graph, place, Link::Drives, wired(node), &file, at);
            let read = read_back(ainb, *n);
            if !read.is_empty() {
                g.edge(place, graph, Link::Reads, read.join(", "), &file, at);
            }
            let actor = g.node(Kind::Actor, actor, || None);
            if placed_actors.insert((graph, actor)) {
                g.edge(graph, actor, Link::Places, node.name.clone(), &file, at);
            }
        }
    }
}

/// A graph's own edges, the same in every copy: the GameData it reads and
/// writes, the modules it runs (named by `module`), and the node classes
/// it uses.
pub(super) fn own(
    g: &mut Graph,
    graph: usize,
    file: &str,
    ainb: &Ainb,
    gd: &GameData,
    module: impl Fn(&str) -> String,
) {
    let mut classes = HashSet::new();
    for (n, node) in ainb.nodes.iter().enumerate() {
        let at = Some(n as u16);
        if node.module {
            let child = g.node(Kind::Graph, &module(&node.name), || None);
            g.edge(graph, child, Link::Runs, "module".into(), file, at);
            let read = read_back(ainb, n);
            if !read.is_empty() {
                g.edge(child, graph, Link::Reads, read.join(", "), file, at);
            }
        } else if asks_anything(node) && classes.insert(&node.name) {
            let class = g.node(Kind::Class, &node.name, || None);
            g.edge(class, graph, Link::Uses, String::new(), file, at);
        }
        for (key, flag) in game_data(node) {
            let state = g.state(gd, flag);
            let label = node.name.clone();
            match writes(&node.name, key) {
                true => g.edge(graph, state, Link::Writes, label, file, at),
                false => g.edge(state, graph, Link::Reads, label, file, at),
            }
        }
        // A node naming a quest moves it, or checks its step when the graph
        // reads its answer. One that does neither (the progress UI) is
        // left out.
        if let Some(quest) = node.str(field::CHALLENGE_NAME) {
            let step =
                (node.str(field::STEP_NAME)).or_else(|| node.str(field::CHALLENGE_STEP_NAME));
            let read = read_back(ainb, n);
            let label = quests::label(&node.name, step, &read);
            if quests::moves(&node.name) {
                let state = g.state(gd, &quests::state(quest));
                g.edge(graph, state, Link::Writes, label, file, at);
            } else if !read.is_empty() {
                let state = g.state(gd, &quests::state(quest));
                g.edge(state, graph, Link::Reads, label, file, at);
            }
        }
    }
}

/// Whether a node's class is game code worth a `class` node: one the game
/// defines, not a boolean the graph does itself.
fn asks_anything(node: &AinbNode) -> bool {
    node.kind == NodeKind::UserDefined && !node.name.starts_with(OPERATE)
}

/// Each GameData name a node holds, in a property or a constant input,
/// with the key it's under.
fn game_data(node: &AinbNode) -> Vec<(&str, &str)> {
    let props = node.properties.iter().map(|(k, v)| (k, v));
    let inputs = (node.inputs.iter())
        .filter(|i| i.sources.is_empty())
        .map(|i| (&i.name, &i.value));
    (props.chain(inputs))
        .filter(|(k, _)| k.contains(&**field::GAME_DATA_NAME))
        .filter_map(|(k, v)| match v {
            Value::String(s) if !s.is_empty() => Some((k.as_str(), s.as_str())),
            _ => None,
        })
        .collect()
}

/// Whether a node of class `class` writes the GameData under `key`: a copy
/// writes its destination and reads its source, else the class says.
fn writes(class: &str, key: &str) -> bool {
    match key {
        _ if key == field::DST_GAME_DATA_NAME => true,
        _ if key == field::SRC_GAME_DATA_NAME => false,
        _ => super::flows::WRITERS.iter().any(|w| class.contains(w)),
    }
}

/// How a graph drives a placement: the inputs wired into its node, or what
/// the node is when nothing is.
fn wired(node: &AinbNode) -> String {
    let linked: Vec<&str> = (node.inputs.iter())
        .filter(|i| !i.sources.is_empty())
        .map(|i| i.name.as_str())
        .collect();
    match linked.is_empty() {
        true => node.name.clone(),
        false => linked.join(", "),
    }
}

/// The outputs of node `n` other nodes read, once each, in order.
fn read_back(ainb: &Ainb, n: usize) -> Vec<String> {
    let mut outs: Vec<String> = (ainb.nodes.iter())
        .flat_map(|node| &node.inputs)
        .flat_map(|i| &i.sources)
        .filter(|l| l.node == n)
        .map(|l| l.output.clone())
        .collect();
    outs.sort();
    outs.dedup();
    outs
}

#[cfg(test)]
mod tests {
    use super::super::{Graph, Link};
    use crate::gates::{self, Atom};
    use crate::known::{actor, field, map};

    #[test]
    fn the_night_module_deletes_each_gold_skulltula_it_places() {
        let Some(g) = Graph::dev() else {
            return;
        };
        let module = format!(
            "graph:{}.logic.module",
            &**map::GIMMICK_NIGHTTIME_SKULLTULA_GOLD_APPEAR_05A9
        );
        let module = g.find(&module).unwrap();
        let gold = format!("placement:{} ", &**actor::ENEMY_SKULLTULA_GOLD);
        let deletes = (g.outs[module].iter().map(|&e| &g.edges[e]))
            .filter(|e| e.link == Link::Drives && g.nodes[e.to].id.starts_with(&gold))
            .filter(|e| e.label.contains(&**field::LOGIC_DELETE))
            .count();
        assert!(deletes >= 10, "{deletes} Gold Skulltulas deleted");
        let places = format!("actor:{}", &**actor::ENEMY_SKULLTULA_GOLD);
        let places = g.find(&places).unwrap();
        assert!(!g.trace(module, places).paths.is_empty());
    }

    /// Every flag, enum, BoolExp and actor a chest's gate reads reaches the
    /// chest through the graph.
    #[test]
    fn every_chest_gate_is_a_path() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let g = Graph::dev().unwrap();
        let ids: std::collections::HashMap<u64, usize> = (0..g.nodes.len())
            .filter_map(|i| {
                let id = g.nodes[i].id.strip_prefix("placement:")?;
                Some((id.rsplit(' ').next()?.parse().ok()?, i))
            })
            .collect();
        let mut missing = Vec::new();
        for gate in gates::trace(&romfs).unwrap() {
            let chest = ids[&gate.hash];
            for atom in gate.expr.atoms() {
                let from = match atom {
                    Atom::Flag(name)
                    | Atom::Exp(name)
                    | Atom::Enum { name, .. }
                    | Atom::After { name, .. } => g.find(&format!("state:{name}")).ok(),
                    Atom::Actor { hash: Some(h), .. } => ids.get(h).copied(),
                    _ => continue,
                };
                if from.is_none_or(|f| g.trace(f, chest).paths.is_empty()) {
                    missing.push(format!("{atom:?} -> {}", gate.hash));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "{} not paths: {:?}",
            missing.len(),
            &missing[..missing.len().min(10)]
        );
    }
}
