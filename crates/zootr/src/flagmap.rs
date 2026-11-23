//! The flag map: every GameData entry, what writes it, what reads it, and
//! which event runs which, as one graph built from the dump. `zootr
//! flagmap` asks it questions and the ledger draws it.
//!
//! Edges point the way state flows: an event `writes` a state, a state is
//! `reads` by an event (or by a BoolExp built on it), an event `runs`
//! another. So a node's upstream is what has to happen first and its
//! downstream is what it unlocks.

use std::collections::HashMap;

use anyhow::{Result, bail};
use ounce_formats::{Romfs, murmur3};
use serde::Serialize;

use crate::events::Flows;
use crate::flags::Survey;
use crate::gamedata::{self, GameData};
use crate::gates;
use crate::known::{field, flag};

mod actors;
mod flows;
mod placed;
mod query;
mod quest_gates;
mod quests;
mod scene;
mod tables;

pub use query::*;
pub use quest_gates::Gate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A GameData entry.
    State,
    /// A flow's entry point.
    Event,
    /// A logic graph a level places.
    Graph,
    /// A placed actor, by hash.
    Placement,
    /// An actor, by name.
    Actor,
    /// An AINB node class: game code a graph runs.
    Class,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    Writes,
    Reads,
    Runs,
    /// A graph wiring a placement's inputs: whether it exists, opens.
    Drives,
    /// A graph placing an actor.
    Places,
    /// An actor's own graph, from its pack.
    Has,
    /// A class's answer going into a graph that runs it.
    Uses,
    /// A placement playing an event its params name, from its own code.
    Plays,
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    /// `kind:name`, what every query takes.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    /// A state's table, default and save slot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub id: usize,
    pub from: usize,
    pub to: usize,
    pub link: Link,
    /// What the edge does: the action, its value, a condition.
    pub label: String,
    /// Where it's written: a flow, the event table, GameData.
    pub file: String,
    /// The flow's event index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<u16>,
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    ids: HashMap<String, usize>,
    outs: Vec<Vec<usize>>,
    ins: Vec<Vec<usize>>,
    /// What builds the graph's sources need: flows to print a cited event.
    flows: Option<Flows>,
    gamedata: Option<GameData>,
    /// Graphs the AINB reader can't parse yet, left out.
    unread: Vec<String>,
    /// The scene graphs, to print a cited node.
    scene: HashMap<String, ounce_formats::ainb::Graph>,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::State => "state",
            Kind::Event => "event",
            Kind::Graph => "graph",
            Kind::Placement => "placement",
            Kind::Actor => "actor",
            Kind::Class => "class",
        }
    }
}

impl Graph {
    /// The dev dump's graph, built once for every test: a build holds
    /// close to a gigabyte, so each test building its own ran the test
    /// runner out of memory.
    #[cfg(test)]
    pub(crate) fn dev() -> Option<&'static Self> {
        static GRAPH: std::sync::OnceLock<Option<Graph>> = std::sync::OnceLock::new();
        GRAPH
            .get_or_init(|| crate::game::dev_dump().map(|romfs| Self::build(&romfs).unwrap()))
            .as_ref()
    }

    /// Every GameData entry and every flow, read from the dump.
    pub fn build(romfs: &Romfs) -> Result<Self> {
        let flows = Flows::load(romfs)?;
        let gamedata = Survey::load(romfs, &flows)?.gamedata;
        let mut g = Graph::default();
        let mut hashes: Vec<u32> = gamedata.hashes().collect();
        hashes.sort_unstable();
        for &h in &hashes {
            g.state_hash(&gamedata, h);
        }
        for &h in &hashes {
            for input in gamedata.exp_inputs(h) {
                let (from, to) = (g.state_hash(&gamedata, input), g.state_hash(&gamedata, h));
                g.edge(
                    from,
                    to,
                    Link::Reads,
                    flag::BOOL_EXP.to_string(),
                    gamedata::PATH,
                    None,
                );
            }
        }
        flows::add(&mut g, &flows, &gamedata);
        let wiring = gates::wiring(romfs)?;
        scene::add(&mut g, &wiring, &gamedata);
        g.scene = (wiring.graphs.into_iter())
            .map(|(name, graph)| (gates::path(&name), graph))
            .collect();
        let packs = actors::add(&mut g, romfs, &gamedata)?;
        g.scene.extend(packs);
        tables::add(&mut g, romfs)?;
        placed::add(&mut g, romfs, &flows)?;
        g.flows = Some(flows);
        g.gamedata = Some(gamedata);
        Ok(g)
    }

    fn node(&mut self, kind: Kind, name: &str, detail: impl FnOnce() -> Option<String>) -> usize {
        let id = format!("{}:{name}", kind.word());
        if let Some(&i) = self.ids.get(&id) {
            return i;
        }
        let i = self.nodes.len();
        let detail = detail();
        self.nodes.push(Node {
            id: id.clone(),
            kind,
            name: name.to_owned(),
            detail,
        });
        self.ids.insert(id, i);
        self.outs.push(Vec::new());
        self.ins.push(Vec::new());
        i
    }

    /// The state GameData keys by `hash`.
    fn state_hash(&mut self, gd: &GameData, hash: u32) -> usize {
        let name = gd.bare(hash);
        self.node(Kind::State, &name, || detail(gd, hash))
    }

    /// The state a flow names, an entry or not.
    fn state(&mut self, gd: &GameData, name: &str) -> usize {
        let hash = murmur3(name.as_bytes());
        match gd.kind(hash) {
            Some(_) => self.state_hash(gd, hash),
            None => self.node(Kind::State, name, || {
                Some(format!("not in {}", flag::GAME_DATA))
            }),
        }
    }

    fn event(&mut self, flow: &str, entry: &str) -> usize {
        self.node(Kind::Event, &format!("{flow}/{entry}"), || None)
    }

    fn edge(
        &mut self,
        from: usize,
        to: usize,
        link: Link,
        label: String,
        file: &str,
        at: Option<u16>,
    ) {
        let id = self.edges.len();
        self.edges.push(Edge {
            id,
            from,
            to,
            link,
            label,
            file: file.to_owned(),
            at,
        });
        self.outs[from].push(id);
        self.ins[to].push(id);
    }

    /// The node `text` names: an id, a name, a GameData hash (`#hex` or
    /// `0x`), or the one node whose name holds it.
    pub fn find(&self, text: &str) -> Result<usize> {
        if let Some(&i) = self.ids.get(text) {
            return Ok(i);
        }
        let exact: Vec<_> = (0..self.nodes.len())
            .filter(|&i| self.nodes[i].name == text)
            .collect();
        if let [i] = exact[..] {
            return Ok(i);
        }
        if let Some(hex) = text.strip_prefix('#').or_else(|| text.strip_prefix("0x"))
            && let Ok(h) = u32::from_str_radix(hex, 16)
            && let Some(&i) = self.ids.get(&format!("state:#{h:08x}"))
        {
            return Ok(i);
        }
        let found = self.search(text, None);
        match (exact.len(), &found[..]) {
            (0, [one]) => Ok(*one),
            (0, []) => bail!("no node is named or holds {text:?}"),
            _ => {
                let some: Vec<_> = found
                    .iter()
                    .take(8)
                    .map(|&i| self.nodes[i].id.as_str())
                    .collect();
                bail!(
                    "{text:?} matches {} nodes, like {}",
                    found.len(),
                    some.join(", ")
                )
            }
        }
    }
}

/// One state's value in the running game, or why there's none.
#[derive(Serialize)]
pub struct Live {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct Values {
    pub values: Vec<Live>,
}

/// Each state's value, read by GameData hash. A state is its id, its
/// name or its `#hash`. Needs no graph, so it answers a poll fast.
pub fn live(nodes: &[String], read: impl Fn(u32) -> Result<String>) -> Values {
    let one = |node: &str| -> Result<String> {
        let name = node.strip_prefix("state:").unwrap_or(node);
        if name.contains(':') {
            bail!("{node} isn't a state");
        }
        read(crate::live::hash(name)?)
    };
    let values = (nodes.iter())
        .map(|n| match one(n) {
            Ok(v) => Live {
                id: n.clone(),
                value: Some(v),
                error: None,
            },
            Err(e) => Live {
                id: n.clone(),
                value: None,
                error: Some(e.to_string()),
            },
        })
        .collect();
    Values { values }
}

impl std::fmt::Display for Values {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for v in &self.values {
            match (&v.value, &v.error) {
                (Some(value), _) => writeln!(f, "{} = {value}", v.id)?,
                (_, error) => writeln!(f, "{}: {}", v.id, error.as_deref().unwrap_or("?"))?,
            }
        }
        Ok(())
    }
}

/// A state's table, default and save slot, as `probe data` reads them.
fn detail(gd: &GameData, hash: u32) -> Option<String> {
    let info = gd.info(&format!("0x{hash:08x}")).ok()?;
    let mut parts = vec![info.kind];
    parts.extend(info.default.map(|d| format!("default {d}")));
    let save = gd
        .entry(hash)?
        .as_map()
        .ok()?
        .get(&**field::SAVE_FILE_INDEX);
    parts.extend(
        save.and_then(|s| gamedata::int(s).ok())
            .map(|s| format!("save {s}")),
    );
    Some(parts.join(", "))
}

#[cfg(test)]
mod tests {
    use ounce_formats::murmur3;

    use super::{Graph, live};
    use crate::known::{actor, flag};

    /// The Deku Tree's room B opens once its scrub is beaten: no graph
    /// writes the flag, the scrub's talk does, named by its placement's
    /// `DamageEventName`.
    #[test]
    fn a_scrub_that_plays_its_talk_sets_its_rooms_flag() {
        let Some(g) = Graph::dev() else {
            return;
        };
        let by = g.played_by(flag::INSIDE_THE_DEKU_TREE_ROOM_B_CLEARED.get());
        assert_eq!(by.len(), 1, "{by:?}");
        let scrub = g.find(&format!(
            "placement:{} {}",
            actor::ENEMY_SCRUB_DEKU.get(),
            by[0]
        ));
        assert!(scrub.is_ok());
    }

    #[test]
    fn live_reads_a_state_by_id_name_or_hash_and_refuses_an_event() {
        let want = murmur3(b"FakeDoor_Opened");
        let read = |h: u32| match h == want {
            true => Ok("true".to_string()),
            false => anyhow::bail!("no value"),
        };
        let nodes = [
            "state:FakeDoor_Opened",
            "FakeDoor_Opened",
            &format!("#{want:08x}"),
            "event:FakeFlow/FakeEntry",
        ];
        let got = live(&nodes.map(str::to_owned), read).values;
        assert!(got[..3].iter().all(|v| v.value.as_deref() == Some("true")));
        assert!(
            got[3]
                .error
                .as_deref()
                .is_some_and(|e| e.contains("isn't a state"))
        );
    }
}
