//! `zootr flagmap`: questions for the flag map.

use anyhow::Result;
use clap::Subcommand;
use ounce_formats::Romfs;

use super::running::{attach, installed_gamedata, said};
use crate::flagmap::{self, Dir, Graph, Kind};

/// Each query takes a node as its id (`state:NAME`, `event:FLOW/ENTRY`),
/// its name, a `#hash`, or text only one node's name holds.
#[derive(Subcommand)]
pub enum FlagmapQuery {
    /// Nodes whose name holds this, ignoring case.
    Search {
        text: String,
        #[arg(long)]
        kind: Option<Kind>,
    },
    /// One node and every edge touching it.
    Node {
        node: String,
        /// Only edges to nodes of this kind.
        #[arg(long)]
        kind: Option<Kind>,
    },
    /// What sets a state, and the value each sets.
    Writers { node: String },
    /// What asks a state, and how.
    Readers { node: String },
    /// What has to happen first: writers, the events running them, and on.
    Up {
        node: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Only edges touching nodes of this kind.
        #[arg(long)]
        kind: Option<Kind>,
    },
    /// What this unlocks: readers, what they run and set, and on.
    Down {
        node: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Only edges touching nodes of this kind.
        #[arg(long)]
        kind: Option<Kind>,
    },
    /// The shortest paths from one node to another, edges followed forward.
    Trace { from: String, to: String },
    /// The lines an edge cites, by its `#id`.
    Source { edge: String },
    /// Node and edge counts, and what the extractors don't cover yet.
    Stats,
    /// The running game's values of these states. Reads only, on the
    /// session's turn: it never takes or renews one.
    Live { nodes: Vec<String> },
}

pub(super) fn flagmap(romfs: &Romfs, query: FlagmapQuery, json: bool) -> Result<()> {
    if let FlagmapQuery::Live { nodes } = query {
        let gd = installed_gamedata(romfs)?;
        let game = attach(false)?;
        let values = flagmap::live(&nodes, |hash| game.get(&gd, hash));
        println!("{}", said(&values, json)?.trim_end());
        return Ok(());
    }
    let g = Graph::build(romfs)?;
    let out = match query {
        FlagmapQuery::Search { text, kind } => said(&g.found(&text, kind), json)?,
        FlagmapQuery::Node { node, kind } => said(&g.around(g.find(&node)?, kind), json)?,
        FlagmapQuery::Writers { node } => said(&g.writers(g.find(&node)?), json)?,
        FlagmapQuery::Readers { node } => said(&g.readers(g.find(&node)?), json)?,
        FlagmapQuery::Up { node, depth, kind } => {
            said(&g.walk(g.find(&node)?, Dir::Up, depth, kind), json)?
        }
        FlagmapQuery::Down { node, depth, kind } => {
            said(&g.walk(g.find(&node)?, Dir::Down, depth, kind), json)?
        }
        FlagmapQuery::Trace { from, to } => said(&g.trace(g.find(&from)?, g.find(&to)?), json)?,
        FlagmapQuery::Source { edge } => {
            let text = g.source(edge.trim_start_matches('#').parse()?)?;
            match json {
                true => serde_json::to_string(&text)?,
                false => text,
            }
        }
        FlagmapQuery::Stats => said(&g.stats(), json)?,
        FlagmapQuery::Live { .. } => unreachable!("answered without the graph"),
    };
    println!("{}", out.trim_end());
    Ok(())
}
