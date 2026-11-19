//! Rewiring a graph's bool links: a node duplicated with every link it
//! reads, a source added to an input, and one source swapped for another.
//!
//! An input fed by several outputs (an `OperateAND`'s `BoolMulti`) points
//! at a run of entries in the file's multi-source table, as its first
//! entry's place and a count. Its one plug, on the reader, names the input
//! and points at the first source, as the game's files do.

use anyhow::{Context, Result, ensure};

use super::edit::{GENERIC, MULTI_COUNT, READ, input_fields, set_word, word};
use super::file::File;
use super::{Graph, MULTI, ParamType, TYPES};

/// A multi-source entry: node, output, then a word the game leaves 0.
const ENTRY: usize = 8;
/// What an input's source node reads when nothing is linked.
const UNLINKED: i16 = -1;

fn entry(node: i16, output: i16) -> [u8; ENTRY] {
    let mut e = [0; ENTRY];
    e[..2].copy_from_slice(&node.to_le_bytes());
    e[2..4].copy_from_slice(&output.to_le_bytes());
    e
}

impl File {
    /// Copies node `node` onto the end of the graph, reading every link it
    /// reads, and gives the copy's index.
    pub fn duplicate(&mut self, node: usize) -> Result<usize> {
        let graph = Graph::parse(&self.to_bytes()?)?;
        let from = self.clone();
        let copy = self.copy_node(&from, node)?;
        let reads = &graph.nodes.get(node).context("no such node")?.inputs;
        for input in reads {
            for s in &input.sources {
                self.add_source(copy, &input.name, s.node, &s.output)?;
            }
        }
        Ok(copy)
    }

    /// Adds `source`'s `output` to what feeds `reader`'s bool `input`:
    /// a link if it reads none yet, after its sources if it reads some.
    pub fn add_source(
        &mut self,
        reader: usize,
        input: &str,
        source: usize,
        output: &str,
    ) -> Result<()> {
        let (t, i) = self.input(reader, input)?;
        let (node, count) = self.source_of(t, i);
        if node == UNLINKED {
            return self.link(reader, input, source, output);
        }
        ensure!(TYPES[t] == ParamType::Bool, "{input} isn't a bool input");
        let added = entry(
            i16::try_from(source)?,
            self.read(reader, t, source, output)?,
        );
        let first = match node > MULTI {
            // One link becomes a run of two.
            true => {
                let first = self.multi.len() / ENTRY;
                self.multi.extend(entry(node, count));
                self.multi.extend(added);
                first
            }
            false => {
                let first = (MULTI - node) as usize;
                let at = first + count as usize;
                self.multi.splice(at * ENTRY..at * ENTRY, added);
                self.shift_runs(at, (t, i));
                first
            }
        };
        let count = if node > MULTI { 2 } else { count + 1 };
        self.set_source_of(t, i, MULTI - i16::try_from(first)?, count);
        let held = self.nodes[reader].u16_at(MULTI_COUNT);
        let grown = held + if node > MULTI { 2 } else { 1 };
        self.nodes[reader].set_u16(MULTI_COUNT, grown);
        Ok(())
    }

    /// Feeds `reader`'s bool `input` from `to` (a node and its output)
    /// where it read `from`.
    pub fn swap_source(
        &mut self,
        reader: usize,
        input: &str,
        from: (usize, &str),
        to: (usize, &str),
    ) -> Result<()> {
        let (t, i) = self.input(reader, input)?;
        ensure!(TYPES[t] == ParamType::Bool, "{input} isn't a bool input");
        let (was, _) = self.output(from.0, t, from.1)?;
        let old = entry(i16::try_from(from.0)?, i16::try_from(was)?);
        let (to_node, to_output) = (i16::try_from(to.0)?, self.read(reader, t, to.0, to.1)?);
        let new = entry(to_node, to_output);
        let (node, count) = self.source_of(t, i);
        let first = match node > MULTI {
            true => {
                ensure!(entry(node, count) == old, "{input} doesn't read that");
                self.set_source_of(t, i, to_node, to_output);
                true
            }
            false => {
                let start = (MULTI - node) as usize;
                let run = start..start + count as usize;
                let place = run
                    .clone()
                    .find(|&k| self.multi[k * ENTRY..k * ENTRY + 4] == old[..4])
                    .with_context(|| format!("{input} doesn't read that"))?;
                self.multi[place * ENTRY..(place + 1) * ENTRY].copy_from_slice(&new);
                place == run.start
            }
        };
        if first {
            let name = self.intern(input);
            let plugs = &mut self.nodes[reader].plugs[GENERIC];
            let plug = plugs.iter_mut().find(|r| word(r, 4) == name);
            let plug = plug.with_context(|| format!("{input} carries no plug"))?;
            set_word(plug, 0, to.0 as u32);
        }
        Ok(())
    }

    /// `source`'s type `t` `output`, marked read by `reader`: its place
    /// among the node's outputs of that type.
    fn read(&mut self, reader: usize, t: usize, source: usize, output: &str) -> Result<i16> {
        let (k, o) = self.output(source, t, output)?;
        let at = o * TYPES[t].output_len();
        let read = word(&self.outputs[t], at) | READ;
        set_word(&mut self.outputs[t], at, read);
        if self.nodes[source].is_query() {
            self.add_query(reader, source)?;
        }
        Ok(i16::try_from(k)?)
    }

    /// Type `t` input `i`'s source node and output (or, for several
    /// sources, the encoded first entry and the count).
    fn source_of(&self, t: usize, i: usize) -> (i16, i16) {
        let (at, _, _) = input_fields(TYPES[t]);
        let e = &self.inputs[t][i * TYPES[t].input_len() + at..];
        (
            i16::from_le_bytes([e[0], e[1]]),
            i16::from_le_bytes([e[2], e[3]]),
        )
    }

    fn set_source_of(&mut self, t: usize, i: usize, node: i16, output: i16) {
        let (at, _, _) = input_fields(TYPES[t]);
        let at = i * TYPES[t].input_len() + at;
        let e = &mut self.inputs[t][at..at + 4];
        e[..2].copy_from_slice(&node.to_le_bytes());
        e[2..].copy_from_slice(&output.to_le_bytes());
    }

    /// Moves every run of sources starting at or past entry `at` one entry
    /// on, but `grown`'s (a type and an input), which an entry joined.
    fn shift_runs(&mut self, at: usize, grown: (usize, usize)) {
        for (t, ty) in TYPES.iter().enumerate() {
            for i in 0..self.inputs[t].len() / ty.input_len() {
                let (node, count) = self.source_of(t, i);
                if (t, i) != grown && node <= MULTI && (MULTI - node) as usize >= at {
                    self.set_source_of(t, i, node - 1, count);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ainb::Value;
    use crate::ainb::file::tests::{Fake, fake};

    use ParamType::{Bool, String as Str};

    /// Two made-up rocks, an AND reading the first's death and an OR
    /// reading nothing yet, which deletes the rocks.
    fn rocks() -> File {
        let rock = || Fake {
            name: "ActorLogicFakeRock",
            props: vec![(Str, "InstanceName")],
            inputs: vec![(Bool, "Fake_Delete", Some((3, 1)))],
            outputs: vec![(Bool, "Dead"), (Bool, "Lifted")],
            plugs: vec![(GENERIC, 3, "Fake_Delete", 8)],
            ..Default::default()
        };
        let gate = |name, source| Fake {
            name,
            inputs: vec![(Bool, "BoolMulti", source)],
            outputs: vec![(Bool, "False"), (Bool, "True")],
            plugs: source
                .map(|(n, _)| (GENERIC, n as i32, "BoolMulti", 8))
                .into_iter()
                .collect(),
            ..Default::default()
        };
        fake(vec![
            rock(),
            rock(),
            gate("OperateFakeAND", Some((0, 0))),
            gate("OperateFakeOR", None),
        ])
    }

    fn fed(f: &File, node: usize, input: &str) -> Vec<(usize, String)> {
        let g = Graph::parse(&f.to_bytes().unwrap()).unwrap();
        let sources = &g.nodes[node].input(input).unwrap().sources;
        sources.iter().map(|s| (s.node, s.output.clone())).collect()
    }

    fn plug(f: &File, node: usize) -> u32 {
        word(&f.nodes[node].plugs[GENERIC][0], 0)
    }

    fn out(node: usize, name: &str) -> (usize, String) {
        (node, name.into())
    }

    #[test]
    fn a_duplicate_reads_its_original_links() {
        let mut f = rocks();
        let copy = f.duplicate(0).unwrap();
        f.set_property(copy, "InstanceName", &Value::String("FakeRock_Copy".into()))
            .unwrap();
        assert_eq!(fed(&f, copy, "Fake_Delete"), [out(3, "True")]);
        let g = Graph::parse(&f.to_bytes().unwrap()).unwrap();
        assert_eq!(g.nodes[copy].str("InstanceName"), Some("FakeRock_Copy"));
        assert_ne!(g.nodes[0].str("InstanceName"), Some("FakeRock_Copy"));
    }

    #[test]
    fn inputs_grow_into_runs_that_keep_each_others_place() {
        let mut f = rocks();
        // A link becomes a run, a bare input a link and then a run behind it.
        f.add_source(2, "BoolMulti", 1, "Dead").unwrap();
        f.add_source(3, "BoolMulti", 0, "Lifted").unwrap();
        f.add_source(3, "BoolMulti", 1, "Lifted").unwrap();
        // The first run grows, so the second moves along.
        let copy = f.duplicate(0).unwrap();
        f.add_source(2, "BoolMulti", copy, "Dead").unwrap();

        let deaths = [out(0, "Dead"), out(1, "Dead"), out(copy, "Dead")];
        assert_eq!(fed(&f, 2, "BoolMulti"), deaths);
        let lifts = [out(0, "Lifted"), out(1, "Lifted")];
        assert_eq!(fed(&f, 3, "BoolMulti"), lifts);
        assert_eq!(f.nodes[2].u16_at(MULTI_COUNT), 3);
        assert_eq!(f.nodes[3].u16_at(MULTI_COUNT), 2);
        assert_eq!((plug(&f, 2), plug(&f, 3)), (0, 0), "the first source's");
        assert_eq!(File::parse(&f.to_bytes().unwrap()).unwrap(), f);
    }

    #[test]
    fn a_source_swaps_for_an_or_of_it_and_its_copy() {
        let mut f = rocks();
        f.add_source(2, "BoolMulti", 1, "Dead").unwrap();
        let copy = f.duplicate(0).unwrap();
        let or = f.copy_node(&rocks(), 3).unwrap();
        f.add_source(or, "BoolMulti", 0, "Dead").unwrap();
        f.add_source(or, "BoolMulti", copy, "Dead").unwrap();
        f.swap_source(2, "BoolMulti", (0, "Dead"), (or, "True"))
            .unwrap();
        f.swap_source(2, "BoolMulti", (1, "Dead"), (1, "Lifted"))
            .unwrap();

        assert_eq!(fed(&f, 2, "BoolMulti"), [out(or, "True"), out(1, "Lifted")]);
        assert_eq!(
            fed(&f, or, "BoolMulti"),
            [out(0, "Dead"), out(copy, "Dead")]
        );
        assert_eq!(plug(&f, 2), or as u32, "the plug follows the first source");
        assert!(
            f.swap_source(2, "BoolMulti", (0, "Dead"), (1, "Dead"))
                .is_err(),
            "no longer read"
        );
        assert_eq!(File::parse(&f.to_bytes().unwrap()).unwrap(), f);
    }

    #[test]
    fn a_single_link_swaps_in_place() {
        let mut f = rocks();
        f.swap_source(2, "BoolMulti", (0, "Dead"), (1, "Dead"))
            .unwrap();
        assert_eq!(fed(&f, 2, "BoolMulti"), [out(1, "Dead")]);
        assert_eq!(plug(&f, 2), 1);
        assert!(f.add_source(0, "InstanceName", 1, "Dead").is_err());
    }
}
