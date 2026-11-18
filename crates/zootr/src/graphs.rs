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

#[cfg(test)]
mod tests {
    use ounce_formats::{files_with_suffix, par_map};

    use crate::game::dev_dump;

    /// The AINB writer against every graph in the game, loose and in
    /// packs: each comes back byte for byte, and every link the writer can
    /// make is made there the way the writer makes it.
    #[test]
    fn the_writer_matches_every_game_graph() {
        let Some(romfs) = dev_dump() else { return };
        let files = [".ainb", ".pack.zs"].map(|s| files_with_suffix(romfs.root(), s).unwrap());
        let found = par_map(&files.concat(), |path| {
            let rel = romfs.rel(path).unwrap();
            let data = romfs.read(&rel).unwrap();
            let graphs: Vec<(String, Vec<u8>)> = if rel.ends_with(".ainb") {
                vec![(rel, data)]
            } else {
                let sarc = roead::sarc::Sarc::new(&data).unwrap();
                let members = sarc.files().filter_map(|f| Some((f.name?, f.data)));
                let graphs = members.filter(|(name, _)| name.ends_with(".ainb"));
                graphs
                    .map(|(name, g)| (format!("{rel}/{name}"), g.to_vec()))
                    .collect()
            };
            let check = |(name, g): (String, Vec<u8>)| {
                ounce_formats::ainb::check(&g).map_err(|e| format!("{name}: {e:#}"))
            };
            graphs.into_iter().map(check).collect::<Vec<_>>()
        });
        let results: Vec<_> = found.into_iter().flatten().collect();
        let failed: Vec<_> = results.iter().filter_map(|r| r.as_ref().err()).collect();
        let links: usize = results.iter().flatten().sum();
        assert!(
            failed.is_empty(),
            "{} of {}: {:#?}",
            failed.len(),
            results.len(),
            &failed[..failed.len().min(10)]
        );
        assert!(
            results.len() > 9000 && links > 60_000,
            "{} graphs, {links} links",
            results.len()
        );
    }
}
