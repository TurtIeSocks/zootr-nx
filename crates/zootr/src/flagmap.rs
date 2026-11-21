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
use crate::known::{field, flag};

mod flows;
mod query;

pub use query::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A GameData entry.
    State,
    /// A flow's entry point.
    Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    Writes,
    Reads,
    Runs,
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
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::State => "state",
            Kind::Event => "event",
        }
    }
}

impl Graph {
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
