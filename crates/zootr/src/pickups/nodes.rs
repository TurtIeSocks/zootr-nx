//! A holder whose logic module writes or plays (a loud one) can't share
//! its `AiGroup` with a copy, as a quiet one does ([`super::groups`]): a
//! second run of the group would set its flags and play its events again.
//! So the copy (its picked twin, or a step's) gets a node of its own in the
//! graph: the holder's node duplicated under a name of its own, which the
//! group names beside the holder's.
//!
//! Only one of a holder and its copies stands at a time, and a node whose
//! actor its Presence keeps out reads not dead (research: upgrades off
//! chests, loud watchers). So whatever read the holder's node reads any of
//! them: an OR gains the copy's output, and anything else reads an OR of the
//! holder's and its copies'.
//!
//! A holder a nested module makes or watches is left out: its copy stays a
//! plain placement. A copy named through the module's path (`_1fdf/Name`)
//! hung its map's load, with its node or without, where the same at the
//! top of a group loads: the Deku Tree's regrowing shrubs in Gohma's room
//! against Bottom of the Well's (Mac, 2026-11-22).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::ainb::{File, Graph, ParamType, Value};
use ounce_formats::{Byml, ModBuilder, Romfs, files_with_suffix};

use super::groups::{REFERENCE, REFERENCES, reference};
use crate::known::field;

const ID: &str = "Id";
const PATH: &str = "Path";
const LOGIC: &str = "Logic";
/// An OR's inputs and the output it reads true on.
const MULTI: &str = "BoolMulti";
const TRUE: &str = "True";

/// Graph edits gathered across the maps: per graph, each node a copy
/// duplicates and the suffix the copy's name takes.
#[derive(Default)]
pub struct Wiring {
    graphs: BTreeMap<String, BTreeSet<(usize, String)>>,
}

/// The suffix the `k`th copy of a holder adds to its node's and its
/// reference's names.
fn suffix(k: usize) -> String {
    format!("_Copy{k}")
}

impl Wiring {
    /// Names each copy in `copies` (by the holder it copies, in order)
    /// beside its holder in each of `root`'s groups naming the holder, and
    /// notes the node to duplicate in each graph `creators` (by holder:
    /// graph and node) runs it in. Whether `root`, a map's doc, changed.
    pub fn refer(
        &mut self,
        root: &mut Byml,
        copies: &[(u64, u64)],
        creators: &HashMap<u64, Vec<(String, usize)>>,
    ) -> Result<bool> {
        let Some(Byml::Array(groups)) = root.as_mut_map()?.get_mut(field::AI_GROUPS.get()) else {
            return Ok(false);
        };
        let mut changed = false;
        let mut nth: HashMap<u64, usize> = HashMap::new();
        for (of, copy) in copies {
            let k = nth.entry(*of).or_default();
            *k += 1;
            let tail = suffix(*k);
            let mut named_any = false;
            for group in groups.iter_mut() {
                let Some(Byml::Array(refs)) = group.as_mut_map()?.get_mut(REFERENCES) else {
                    continue;
                };
                let Some(own) = refs.iter().find(|r| reference(r) == Some(*of)) else {
                    continue;
                };
                if nested(own) {
                    continue;
                }
                let mut named = own.clone();
                let map = named.as_mut_map()?;
                for key in [ID, PATH] {
                    let name = map
                        .get(key)
                        .and_then(|v| v.as_string().ok())
                        .map(|s| s.to_string());
                    let name =
                        name.with_context(|| format!("holder {of}'s reference lacks {key}"))?;
                    map.insert(key.into(), Byml::String(format!("{name}{tail}").into()));
                }
                map.insert(REFERENCE.into(), Byml::U64(*copy));
                refs.push(named);
                named_any = true;
            }
            changed |= named_any;
            // A node no group names would never be bound to the copy.
            let creating = creators.get(of).filter(|_| named_any);
            for (graph, node) in creating.into_iter().flatten() {
                let edits = self.graphs.entry(graph.clone()).or_default();
                edits.insert((*node, tail.clone()));
            }
        }
        Ok(changed)
    }

    /// Writes each graph a copy needs a node in.
    pub fn write(&self, romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
        if self.graphs.is_empty() {
            return Ok(());
        }
        let donor = donor(romfs)?;
        for (graph, edits) in &self.graphs {
            let rel = format!("{LOGIC}/{graph}.ainb");
            let mut f = File::parse(&romfs.read(&rel)?).with_context(|| rel.clone())?;
            for (node, tail) in edits {
                wire(&mut f, *node, tail, &donor).with_context(|| format!("{rel} node {node}"))?;
            }
            builder.write(&rel, &f.to_bytes()?)?;
        }
        Ok(())
    }
}

/// Each loud holder's creating nodes, by its hash: graph and node.
pub fn creators(romfs: &Romfs) -> Result<HashMap<u64, Vec<(String, usize)>>> {
    let mut out: HashMap<u64, Vec<(String, usize)>> = HashMap::new();
    for c in crate::gates::creators(romfs)?
        .into_iter()
        .filter(|c| !c.quiet)
    {
        out.entry(c.hash).or_default().push((c.graph, c.node));
    }
    Ok(out)
}

/// Whether reference `r` names an actor of a module nested in the group's
/// own, by a path through the module (`_1fdf/Name`).
fn nested(r: &Byml) -> bool {
    let path = r.as_map().ok().and_then(|m| m.get(PATH)?.as_string().ok());
    path.is_some_and(|p| p.contains('/'))
}

/// A graph holding an `OperateOR`, and its index there.
fn donor(romfs: &Romfs) -> Result<(File, usize)> {
    for path in files_with_suffix(&romfs.root().join(LOGIC), ".ainb")? {
        let f = File::parse(&romfs.read(&romfs.rel(&path)?)?)?;
        if let Some(or) = f.find(field::OPERATE_OR) {
            return Ok((f, or));
        }
    }
    bail!("no graph holds an {}", field::OPERATE_OR)
}

/// Duplicates `node` under its name plus `tail`, and has whatever reads it
/// read the copy too.
fn wire(f: &mut File, node: usize, tail: &str, (donor, or): &(File, usize)) -> Result<()> {
    let graph = Graph::parse(&f.to_bytes()?)?;
    let name = graph.nodes[node]
        .str(field::INSTANCE_NAME)
        .context("no instance name")?;
    let name = format!("{name}{tail}");
    let copy = f.duplicate(node)?;
    f.set_property(copy, field::INSTANCE_NAME, &Value::String(name))?;
    for (r, reader) in graph.nodes.iter().enumerate() {
        for input in &reader.inputs {
            for s in input.sources.iter().filter(|s| s.node == node) {
                ensure!(
                    input.ty == ParamType::Bool,
                    "node {r} reads {} as {:?}",
                    s.output,
                    input.ty
                );
                if reader.name.starts_with(&**field::OPERATE_OR) {
                    f.add_source(r, &input.name, copy, &s.output)?;
                    continue;
                }
                let either = f.copy_node(donor, *or)?;
                f.add_source(either, MULTI, node, &s.output)?;
                f.add_source(either, MULTI, copy, &s.output)?;
                f.swap_source(r, &input.name, (node, &s.output), (either, TRUE))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    /// A group naming a top-level holder `1` and a nested one `2`.
    fn group() -> Byml {
        let map = |pairs: Vec<(&str, Byml)>| {
            Byml::Map(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
        };
        let named = |path: &str, hash: u64| {
            let id = path.rsplit('/').next().unwrap();
            map(vec![
                (ID, Byml::String(id.into())),
                (PATH, Byml::String(path.into())),
                (REFERENCE, Byml::U64(hash)),
            ])
        };
        let refs = vec![named("FakePot_1a2b", 1), named("_3c4d/FakePot_5e6f", 2)];
        let group = map(vec![(REFERENCES, Byml::Array(refs))]);
        map(vec![(field::AI_GROUPS.get(), Byml::Array(vec![group]))])
    }

    fn paths(root: &Byml) -> Vec<String> {
        let groups = root.as_map().unwrap()[field::AI_GROUPS.get()]
            .as_array()
            .unwrap();
        let refs = groups[0].as_map().unwrap()[REFERENCES].as_array().unwrap();
        refs.iter()
            .map(|r| text(r, PATH).unwrap().to_owned())
            .collect()
    }

    fn text<'a>(r: &'a Byml, key: &str) -> Option<&'a str> {
        r.as_map()
            .ok()?
            .get(key)?
            .as_string()
            .ok()
            .map(|s| s.as_str())
    }

    /// A top-level holder's copy is named beside it and gets a node. A
    /// nested module's holder gets neither: named through the module, its
    /// copy hung the Deku Tree's load.
    #[test]
    fn only_a_top_level_holder_names_its_copy() {
        let mut root = group();
        let creators = HashMap::from([
            (1, vec![("FakeRoom".to_owned(), 3)]),
            (2, vec![("FakeBoss".to_owned(), 4)]),
        ]);
        let mut wiring = Wiring::default();
        assert!(
            wiring
                .refer(&mut root, &[(1, 11), (2, 22)], &creators)
                .unwrap()
        );
        assert_eq!(
            paths(&root),
            ["FakePot_1a2b", "_3c4d/FakePot_5e6f", "FakePot_1a2b_Copy1"]
        );
        let edits: Vec<_> = wiring.graphs.keys().cloned().collect();
        assert_eq!(edits, ["FakeRoom"]);

        let mut root = group();
        assert!(
            !Wiring::default()
                .refer(&mut root, &[(2, 22)], &creators)
                .unwrap()
        );
    }

    /// Every graph a loud holder's node sits in takes two copies of it,
    /// and comes back a graph the writer's check passes, where whatever
    /// read the holder reads each copy too.
    #[test]
    fn every_loud_holder_takes_copies() {
        let Some(romfs) = dev_dump() else { return };
        let loud: BTreeSet<u64> = (crate::pickups::watched(&romfs).unwrap().into_iter())
            .filter(|c| c.watched == crate::checks::Watched::Loud)
            .map(|c| c.hash)
            .collect();
        let mut by_graph: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let creators = creators(&romfs).unwrap();
        for nodes in loud.iter().filter_map(|h| creators.get(h)).cloned() {
            for (graph, node) in nodes {
                by_graph.entry(graph).or_default().insert(node);
            }
        }
        // Two of them are loose rupees or hearts, which no setting deals yet.
        assert_eq!((loud.len(), by_graph.len()), (108, 40));
        let donor = donor(&romfs).unwrap();
        for (graph, nodes) in &by_graph {
            let rel = format!("{LOGIC}/{graph}.ainb");
            let mut f = File::parse(&romfs.read(&rel).unwrap()).unwrap();
            let before = Graph::parse(&f.to_bytes().unwrap()).unwrap();
            for &node in nodes {
                for k in [1, 2] {
                    wire(&mut f, node, &suffix(k), &donor)
                        .unwrap_or_else(|e| panic!("{rel} node {node}: {e:#}"));
                }
            }
            let bytes = f.to_bytes().unwrap();
            ounce_formats::ainb::check(&bytes).unwrap_or_else(|e| panic!("{rel}: {e:#}"));
            let after = Graph::parse(&bytes).unwrap();
            let name = |n: usize| after.nodes[n].str(field::INSTANCE_NAME).map(str::to_owned);
            for &node in nodes {
                let copies: Vec<usize> = (0..after.nodes.len())
                    .filter(|&n| {
                        let own = name(node).unwrap();
                        (1..=2).any(|k| name(n) == Some(format!("{own}{}", suffix(k))))
                    })
                    .collect();
                assert_eq!(copies.len(), 2, "{rel} node {node}");
                // Each output the holder fed still reaches a reader through
                // the holder and through each copy, straight or by an OR.
                let reads = |from: usize, out: &str| {
                    after.nodes.iter().any(|r| {
                        r.inputs
                            .iter()
                            .any(|i| i.sources.iter().any(|s| s.node == from && s.output == out))
                    })
                };
                for reader in &before.nodes {
                    for input in &reader.inputs {
                        for s in input.sources.iter().filter(|s| s.node == node) {
                            for &c in &copies {
                                assert!(reads(c, &s.output), "{rel}: {node}.{} copy {c}", s.output);
                            }
                        }
                    }
                }
            }
        }
    }
}
