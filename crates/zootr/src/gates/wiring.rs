//! The scene logic as the flag map reads it: every logic graph, and each
//! place a graph is placed with the placements its actor nodes name.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::Romfs;
use ounce_formats::ainb::Graph;

use super::World;
use crate::known::field;

/// One placed copy of a graph.
pub struct Placed {
    pub graph: String,
    /// The Banc folder it's placed in, e.g. `SpiritTemple`.
    pub area: String,
    /// `(node, placement hash, actor)` for each actor node that names one.
    pub actors: Vec<(usize, u64, String)>,
}

pub struct Wiring {
    /// Every placed graph, parsed, by name (`X_1234.logic.module`).
    pub graphs: HashMap<String, Graph>,
    pub placed: Vec<Placed>,
}

/// Every graph the levels place, and every copy of each.
pub fn wiring(romfs: &Romfs) -> Result<Wiring> {
    let world = World::load(romfs)?;
    let placed = (0..world.instances.len())
        .map(|i| {
            let inst = &world.instances[i];
            let nodes = world.graph(i).nodes.iter().enumerate();
            let actors = nodes
                .filter_map(|(n, node)| {
                    let hash = world.placement(i, node.str(field::INSTANCE_NAME))?;
                    Some((n, hash, world.gyaml(hash).to_owned()))
                })
                .collect();
            Placed {
                graph: inst.graph.clone(),
                area: world.bancs[inst.banc].area.clone(),
                actors,
            }
        })
        .collect();
    Ok(Wiring {
        graphs: world.graphs,
        placed,
    })
}

/// The romfs path of a graph [`Wiring`] names.
pub fn path(graph: &str) -> String {
    format!("{}/{graph}.ainb", super::LOGIC)
}
