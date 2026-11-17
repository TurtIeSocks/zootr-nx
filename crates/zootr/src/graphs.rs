//! Edits to AINB logic graphs (`Logic/*.logic.module.ainb`): find a node by
//! a string it holds, and move what reads one output onto another.

use anyhow::{Context, Result};
use ounce_formats::ainb::{Graph, Patch};

use crate::known::Name;

/// The romfs path of logic module `name`.
pub(crate) fn logic(name: &Name) -> String {
    format!("Logic/{name}.logic.module.ainb")
}

/// The one node of class `name` whose string `key` is `value`.
pub(crate) fn node(graph: &Graph, (name, key): (&Name, &Name), value: &str) -> Result<usize> {
    let mut found = (0..)
        .zip(&graph.nodes)
        .filter(|(_, n)| n.name == name && n.str(key) == Some(value));
    let (i, _) = found
        .next()
        .with_context(|| format!("no {name} with {key} {value}"))?;
    anyhow::ensure!(found.next().is_none(), "two {name} with {key} {value}");
    Ok(i)
}

/// `(node, input, source)` of the one link from `from`'s `output`.
pub(crate) fn link_from(
    graph: &Graph,
    from: usize,
    output: &str,
) -> Result<(usize, String, usize)> {
    let links = graph.nodes.iter().enumerate().flat_map(|(n, node)| {
        node.inputs.iter().flat_map(move |input| {
            let at = input
                .sources
                .iter()
                .position(|l| l.node == from && l.output == output);
            at.map(|i| (n, input.name.clone(), i))
        })
    });
    let links: Vec<_> = links.collect();
    match &links[..] {
        [one] => Ok(one.clone()),
        _ => anyhow::bail!("{} links from node {from}'s {output}, not one", links.len()),
    }
}

/// What reads `from.output` reads `to.output` instead.
pub(crate) fn relink(patch: &mut Patch, from: (usize, &str), to: (usize, &str)) -> Result<()> {
    let (n, input, i) = link_from(patch.graph(), from.0, from.1)?;
    patch.relink(n, &input, i, to.0, to.1)
}
