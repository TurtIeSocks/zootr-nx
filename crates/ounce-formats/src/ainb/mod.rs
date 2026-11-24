//! AINB node graphs (`Logic/*.ainb`, and the AI inside actor packs): each
//! node's name, properties, inputs and outputs, and the links from an input
//! to the outputs feeding it. Control flow (plugs), blackboards,
//! attachments and expressions are skipped. `Patch` relinks inputs and
//! sets strings in place, and `File` holds every section so a graph can grow.
//!
//! Version 0x407 and later. Every offset is absolute, and every string an
//! offset into the pool. Properties and parameters live in per-type tables,
//! and a node holds a base index and count into each.

use anyhow::{Context, Result, bail, ensure};

use crate::reader::Reader;

mod edit;
mod file;
mod rewire;

pub use edit::check;
pub use file::File;

const MAGIC: &[u8; 4] = b"AIB ";
const MIN_VERSION: u32 = 0x407;
const COMMAND_LEN: usize = 0x18;
const NODE_LEN: usize = 0x3c;
const COMMANDS: usize = 0x74;
/// A source node index at or below this points into the multi-source table.
const MULTI: i16 = -100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType {
    Int,
    Bool,
    Float,
    String,
    Vec3,
    Pointer,
}

const TYPES: [ParamType; 6] = [
    ParamType::Int,
    ParamType::Bool,
    ParamType::Float,
    ParamType::String,
    ParamType::Vec3,
    ParamType::Pointer,
];

impl ParamType {
    fn property_len(self) -> usize {
        match self {
            ParamType::Vec3 => 0x14,
            _ => 0xc,
        }
    }
    fn input_len(self) -> usize {
        match self {
            ParamType::Vec3 => 0x18,
            ParamType::Pointer => 0x14,
            _ => 0x10,
        }
    }
    fn output_len(self) -> usize {
        match self {
            ParamType::Pointer => 8,
            _ => 4,
        }
    }

    /// How far a pointer's class name, after the entry's own name, pushes
    /// the fields behind it.
    fn class(self) -> usize {
        match self {
            ParamType::Pointer => 4,
            _ => 0,
        }
    }
    /// A property entry's value: after its name and flags.
    fn property_value(self) -> usize {
        8 + self.class()
    }
    /// An input entry's source (node then output), flags and value.
    fn input_fields(self) -> (usize, usize, usize) {
        let c = self.class();
        (4 + c, 8 + c, 12 + c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    UserDefined,
    /// A module's input, named by its `Name` property.
    ModuleInput,
    /// A module's output, named by its `Name` property, fed by its input.
    ModuleOutput,
    /// Selectors, sequences and the rest of the control flow.
    Other(u16),
}

impl From<u16> for NodeKind {
    fn from(raw: u16) -> Self {
        match raw {
            0 => NodeKind::UserDefined,
            100..200 => NodeKind::ModuleInput,
            200..300 => NodeKind::ModuleOutput,
            r => NodeKind::Other(r),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Float(f32),
    String(String),
    Vec3([f32; 3]),
    Pointer,
}

/// Another node's output.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub node: usize,
    pub output: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub name: String,
    pub ty: ParamType,
    /// The value used when nothing is linked.
    pub value: Value,
    /// Every output feeding this input. Multi-input nodes, like `OperateAND`,
    /// have several.
    pub sources: Vec<Link>,
    /// Where `value` sits in the file.
    pub value_at: usize,
    /// Where each of `sources` sits in the file.
    pub sources_at: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub ty: ParamType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    /// The node's class, like `OperateAND`, or for a module the module's
    /// file name without `.ainb`. Empty on module inputs and outputs.
    pub name: String,
    pub module: bool,
    pub properties: Vec<(String, Value)>,
    /// Where each property's value sits in the file.
    pub properties_at: Vec<usize>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

impl Node {
    /// Each property, then each unlinked input: its name, its value and
    /// where the value sits in the file.
    fn values(&self) -> impl Iterator<Item = (&str, &Value, usize)> {
        let props = (self.properties.iter().zip(&self.properties_at))
            .map(|((n, v), &at)| (n.as_str(), v, at));
        let inputs = self.inputs.iter().filter(|i| i.sources.is_empty());
        props.chain(inputs.map(|i| (i.name.as_str(), &i.value, i.value_at)))
    }

    /// A property's or an unlinked input's value.
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.values().find(|&(n, ..)| n == name).map(|(_, v, _)| v)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        match self.value(name)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn input(&self, name: &str) -> Option<&Input> {
        self.inputs.iter().find(|i| i.name == name)
    }
}

#[derive(Debug, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

/// An input's raw source before multi-sources and output names resolve.
struct Source {
    node: i16,
    output: i16,
}

/// A property as its type's table holds it.
struct RawProperty {
    name: String,
    value: Value,
    at: usize,
}

/// An input as its type's table holds it, its sources not yet resolved.
struct RawInput {
    name: String,
    value: Value,
    value_at: usize,
    /// Each source's node, output, and where it sits in the file.
    sources: Vec<(usize, usize, usize)>,
}

/// Checks the magic and that the version is one this reads.
fn check_header(r: &Reader) -> Result<()> {
    ensure!(r.bytes(0, 4)? == MAGIC, "not an AINB file");
    let version = r.u32(4)?;
    ensure!(
        version >= MIN_VERSION,
        "unsupported AINB version {version:#x}"
    );
    Ok(())
}

impl Graph {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let r = Reader(data);
        check_header(&r)?;
        let pool = r.u32(0x24)? as usize;
        let s = |at: usize| -> Result<String> { string(&r, pool + r.u32(at)? as usize) };
        let (props_at, io_at) = (r.u32(0x2c)? as usize, r.u32(0x34)? as usize);
        let (multi_at, trans_at) = (r.u32(0x38)? as usize, r.u32(0x30)? as usize);

        let properties = raw_properties(&r, &s, props_at, io_at)?;
        let multi = (multi_at..trans_at)
            .step_by(8)
            .map(|at| Ok((source(&r, at)?, at)))
            .collect::<Result<Vec<_>>>()?;
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for (i, &t) in TYPES.iter().enumerate() {
            let at = io_at + i * 8;
            let (in_at, out_at) = (r.u32(at)? as usize, r.u32(at + 4)? as usize);
            let out_end = match TYPES.get(i + 1) {
                Some(_) => r.u32(at + 8)? as usize,
                None => multi_at,
            };
            inputs.push(raw_inputs(&r, &s, t, in_at..out_at, &multi)?);
            let outs = (out_at..out_end)
                .step_by(t.output_len())
                .map(|at| string(&r, pool + (r.u32(at)? & 0x3fff_ffff) as usize))
                .collect::<Result<Vec<_>>>()?;
            outputs.push(outs);
        }

        let first = COMMANDS + COMMAND_LEN * r.u32(0x0c)? as usize;
        let mut nodes = Vec::new();
        let mut raw_links = Vec::new();
        for n in 0..r.u32(0x10)? as usize {
            let at = first + n * NODE_LEN;
            let params = r.u32(at + 0x14)? as usize;
            let slice = |at: usize| -> Result<std::ops::Range<usize>> {
                let base = r.u32(at)? as usize;
                Ok(base..base + r.u32(at + 4)? as usize)
            };
            let mut node = Node {
                kind: NodeKind::from(r.u16(at)?),
                name: s(at + 8)?,
                module: r.u8(at + 6)? & 2 != 0,
                properties: Vec::new(),
                properties_at: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
            };
            let mut links = Vec::new();
            for (i, &ty) in TYPES.iter().enumerate() {
                let range = slice(params + i * 8)?;
                for p in pick(&properties[i], range)? {
                    node.properties.push((p.name.clone(), p.value.clone()));
                    node.properties_at.push(p.at);
                }
                let io = params + 0x30 + i * 16;
                for input in pick(&inputs[i], slice(io)?)? {
                    let sources = input.sources.iter().map(|&(n, o, _)| (n, o)).collect();
                    links.push((node.inputs.len(), ty, sources));
                    node.inputs.push(Input {
                        name: input.name.clone(),
                        ty,
                        value: input.value.clone(),
                        sources: Vec::new(),
                        value_at: input.value_at,
                        sources_at: input.sources.iter().map(|&(.., at)| at).collect(),
                    });
                }
                for name in pick(&outputs[i], slice(io + 8)?)? {
                    node.outputs.push(Output {
                        name: name.clone(),
                        ty,
                    });
                }
            }
            nodes.push(node);
            raw_links.push(links);
        }
        resolve(&mut nodes, raw_links)?;
        Ok(Self { nodes })
    }
}

/// Each type's property entries.
fn raw_properties(
    r: &Reader,
    s: &impl Fn(usize) -> Result<String>,
    props_at: usize,
    io_at: usize,
) -> Result<Vec<Vec<RawProperty>>> {
    property_ranges(r, props_at, io_at)?
        .into_iter()
        .zip(TYPES)
        .map(|((start, end), t)| {
            (start..end)
                .step_by(t.property_len())
                .map(|at| {
                    let value_at = at + t.property_value();
                    Ok(RawProperty {
                        name: s(at)?,
                        value: value(r, t, value_at, s)?,
                        at: value_at,
                    })
                })
                .collect()
        })
        .collect()
}

/// Type `t`'s input entries in `range`, with the multi-source table's
/// entries, each with where it sits, for an input fed by several.
fn raw_inputs(
    r: &Reader,
    s: &impl Fn(usize) -> Result<String>,
    t: ParamType,
    range: std::ops::Range<usize>,
    multi: &[(Source, usize)],
) -> Result<Vec<RawInput>> {
    let (source_at, _, value_at) = t.input_fields();
    range
        .step_by(t.input_len())
        .map(|at| {
            let single = (source(r, at + source_at)?, at + source_at);
            let sources = match single.0.node {
                n if n <= MULTI => {
                    let from = (MULTI - n) as usize;
                    let run = usize::try_from(single.0.output)
                        .ok()
                        .and_then(|count| multi.get(from..from + count));
                    run.unwrap_or_default().iter().collect()
                }
                _ => vec![&single],
            };
            let sources = sources
                .into_iter()
                .filter(|(s, _)| s.node >= 0 && s.output >= 0)
                .map(|(s, at)| (s.node as usize, s.output as usize, *at))
                .collect();
            Ok(RawInput {
                name: s(at)?,
                value: value(r, t, at + value_at, s)?,
                value_at: at + value_at,
                sources,
            })
        })
        .collect()
}

/// Per node, each input's place, type and raw `(node, output)` sources.
type RawLinks = Vec<Vec<(usize, ParamType, Vec<(usize, usize)>)>>;

/// Names the output each input's raw sources reach.
fn resolve(nodes: &mut [Node], raw_links: RawLinks) -> Result<()> {
    for (n, links) in raw_links.into_iter().enumerate() {
        for (input, ty, sources) in links {
            let resolved = sources
                .into_iter()
                .map(|(node, output)| {
                    let src = nodes.get(node).context("link to a missing node")?;
                    let output = linked_output(src, ty, output);
                    Ok(Link { node, output })
                })
                .collect::<Result<Vec<_>>>()?;
            nodes[n].inputs[input].sources = resolved;
        }
    }
    Ok(())
}

/// The output a link from `src` reaches: its `output`th output of the
/// input's type `ty`, or, for a link the game converts across types (a
/// Float read from a Vec3), its `output`th of the one type that has one.
/// When several types do, which one the game means isn't read yet (it's in
/// the graph's expressions), so the output is named `?N`.
fn linked_output(src: &Node, ty: ParamType, output: usize) -> String {
    let nth = |t: ParamType| src.outputs.iter().filter(|o| o.ty == t).nth(output);
    if let Some(o) = nth(ty) {
        return o.name.clone();
    }
    let mut others = TYPES.iter().filter_map(|&t| nth(t));
    match (others.next(), others.next()) {
        (Some(o), None) => o.name.clone(),
        _ => format!("?{output}"),
    }
}

/// Edits a graph's bytes in place. Links and string offsets keep their
/// size, and a new string goes on the end of the pool, the file's last
/// section, so nothing else moves.
pub struct Patch {
    data: Vec<u8>,
    graph: Graph,
}

impl Patch {
    pub fn new(data: Vec<u8>) -> Result<Self> {
        let graph = Graph::parse(&data)?;
        Ok(Self { data, graph })
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// Feed source `i` of `node`'s input `input` from `from`'s `output`.
    pub fn relink(
        &mut self,
        node: usize,
        input: &str,
        i: usize,
        from: usize,
        output: &str,
    ) -> Result<()> {
        let nodes = &self.graph.nodes;
        let found = nodes.get(node).and_then(|n| n.input(input));
        let input = found.ok_or_else(|| anyhow::anyhow!("node {node} has no input {input}"))?;
        let Some(&at) = input.sources_at.get(i) else {
            bail!("node {node}'s {} has no source {i}", input.name);
        };
        let outputs = nodes
            .get(from)
            .map(|n| n.outputs.as_slice())
            .unwrap_or_default();
        let found = outputs
            .iter()
            .filter(|o| o.ty == input.ty)
            .position(|o| o.name == output);
        let index = found.ok_or_else(|| anyhow::anyhow!("node {from} has no output {output}"))?;
        let (from, index) = (i16::try_from(from)?, i16::try_from(index)?);
        self.data[at..at + 2].copy_from_slice(&from.to_le_bytes());
        self.data[at + 2..at + 4].copy_from_slice(&index.to_le_bytes());
        self.reparse()
    }

    /// Set `node`'s string property or unlinked string input `name`.
    pub fn set_str(&mut self, node: usize, name: &str, value: &str) -> Result<()> {
        let at = self.value_at(node, name, "string", |v| matches!(v, Value::String(_)))?;
        let offset = self.intern(value)?;
        self.data[at..at + 4].copy_from_slice(&offset.to_le_bytes());
        self.reparse()
    }

    /// Set `node`'s bool property or unlinked bool input `name`.
    pub fn set_bool(&mut self, node: usize, name: &str, value: bool) -> Result<()> {
        let at = self.value_at(node, name, "bool", |v| matches!(v, Value::Bool(_)))?;
        self.data[at..at + 4].copy_from_slice(&u32::from(value).to_le_bytes());
        self.reparse()
    }

    /// Where `node`'s property or unlinked input `name` holds its value,
    /// when `is` (a `kind`) takes it.
    fn value_at(
        &self,
        node: usize,
        name: &str,
        kind: &str,
        is: fn(&Value) -> bool,
    ) -> Result<usize> {
        let n = self
            .graph
            .nodes
            .get(node)
            .ok_or_else(|| anyhow::anyhow!("no node {node}"))?;
        n.values()
            .find(|&(k, v, _)| k == name && is(v))
            .map(|(.., at)| at)
            .ok_or_else(|| anyhow::anyhow!("node {node} has no {kind} {name}"))
    }

    fn intern(&mut self, s: &str) -> Result<u32> {
        let pool = Reader(&self.data).u32(0x24)? as usize;
        Ok(intern(&mut self.data, pool, s))
    }

    fn reparse(&mut self) -> Result<()> {
        self.graph = Graph::parse(&self.data)?;
        Ok(())
    }
}

/// `s`'s offset in the string pool that runs from `pool` to the end of
/// `data`, added to its end if it isn't there.
fn intern(data: &mut Vec<u8>, pool: usize, s: &str) -> u32 {
    let mut at = pool;
    while let Some(held) = data.get(at..).and_then(until_nul) {
        if held == s.as_bytes() {
            return (at - pool) as u32;
        }
        at += held.len() + 1;
    }
    let offset = (data.len() - pool) as u32;
    data.extend(s.as_bytes());
    data.push(0);
    offset
}

/// Each type's `[start, end)` in the property table, which begins with
/// six u32 offsets, the last running to `end`.
fn property_ranges(r: &Reader, at: usize, end: usize) -> Result<Vec<(usize, usize)>> {
    (0..TYPES.len())
        .map(|i| {
            let stop = match i + 1 < TYPES.len() {
                true => r.u32(at + (i + 1) * 4)? as usize,
                false => end,
            };
            Ok((r.u32(at + i * 4)? as usize, stop))
        })
        .collect()
}

fn pick<T>(table: &[T], range: std::ops::Range<usize>) -> Result<&[T]> {
    let (start, end) = (range.start, range.end);
    table.get(range).ok_or_else(|| {
        anyhow::anyhow!("node params {start}..{end} past a table of {}", table.len())
    })
}

fn source(r: &Reader, at: usize) -> Result<Source> {
    Ok(Source {
        node: r.u16(at)? as i16,
        output: r.u16(at + 2)? as i16,
    })
}

fn value(
    r: &Reader,
    t: ParamType,
    at: usize,
    s: &impl Fn(usize) -> Result<String>,
) -> Result<Value> {
    let f = |at: usize| -> Result<f32> { Ok(f32::from_bits(r.u32(at)?)) };
    Ok(match t {
        ParamType::Int => Value::Int(r.u32(at)? as i32),
        ParamType::Bool => Value::Bool(r.u32(at)? != 0),
        ParamType::Float => Value::Float(f(at)?),
        ParamType::String => Value::String(s(at)?),
        ParamType::Vec3 => Value::Vec3([f(at)?, f(at + 4)?, f(at + 8)?]),
        ParamType::Pointer => Value::Pointer,
    })
}

/// The bytes `rest` starts with, up to its first NUL; `None` without one.
fn until_nul(rest: &[u8]) -> Option<&[u8]> {
    rest.iter().position(|&b| b == 0).map(|len| &rest[..len])
}

/// The string at `at`, running to the end of the file without a NUL: the
/// reader takes what it can, where `File` refuses.
fn string(r: &Reader, at: usize) -> Result<String> {
    let rest = r.0.get(at..).unwrap_or_default();
    Ok(std::str::from_utf8(until_nul(rest).unwrap_or(rest))?.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ainb::file::tests::le;

    /// A node with one output per `(name, type)`.
    fn outputs(outs: &[(&str, ParamType)]) -> Node {
        Node {
            kind: NodeKind::UserDefined,
            name: "FakeQuery".into(),
            module: false,
            properties: Vec::new(),
            properties_at: Vec::new(),
            inputs: Vec::new(),
            outputs: (outs.iter())
                .map(|&(name, ty)| Output {
                    name: name.into(),
                    ty,
                })
                .collect(),
        }
    }

    #[test]
    fn a_link_reaches_its_types_output_then_the_one_type_that_has_it() {
        use ParamType::{Bool, Float, Int, Vec3};
        let src = outputs(&[
            ("Count", Int),
            ("Far", Float),
            ("Near", Float),
            ("At", Vec3),
        ]);
        assert_eq!(linked_output(&src, Float, 1), "Near");
        // A Float read from the only Vec3: the game converts it.
        let gravity = outputs(&[("Vector", Vec3)]);
        assert_eq!(linked_output(&gravity, Float, 0), "Vector");
        // A Bool from output 0, which Int, Float and Vec3 all have.
        assert_eq!(linked_output(&src, Bool, 0), "?0");
        assert_eq!(linked_output(&src, Bool, 1), "Near");
    }

    const BOOL: usize = 1;
    const STRING: usize = 3;

    /// A node of the test file: string properties, unlinked string inputs,
    /// one bool input fed by `(node, bool output)` pairs, and bool outputs.
    struct Spec {
        kind: u16,
        name: &'static str,
        module: bool,
        props: &'static [(&'static str, &'static str)],
        strings: Vec<(&'static str, &'static str)>,
        multi: Option<(&'static str, &'static [(i16, i16)])>,
        outs: &'static [&'static str],
    }

    #[derive(Default)]
    struct Pool(Vec<u8>);

    impl Pool {
        fn add(&mut self, s: &str) -> u32 {
            let at = self.0.len() as u32;
            self.0.extend(s.as_bytes());
            self.0.push(0);
            at
        }
    }

    /// Lays out header, nodes, node param blocks, the property table, the
    /// input/output table, multi-sources and the string pool, in that order.
    fn build(specs: &[Spec]) -> Vec<u8> {
        let mut pool = Pool::default();
        let mut props: [Vec<u8>; 6] = Default::default();
        let mut ins: [Vec<u8>; 6] = Default::default();
        let mut outs: [Vec<u8>; 6] = Default::default();
        let mut multi = Vec::new();
        let mut bases = Vec::new();
        for spec in specs {
            bases.push((
                props[STRING].len() / 12,
                ins[STRING].len() / 16,
                ins[BOOL].len() / 16,
                outs[BOOL].len() / 4,
            ));
            for (k, v) in spec.props {
                props[STRING].extend(le(&[pool.add(k), 0, pool.add(v)]));
            }
            for (k, v) in &spec.strings {
                ins[STRING].extend(le(&[pool.add(k), u32::MAX, 0, pool.add(v)]));
            }
            if let Some((k, sources)) = spec.multi {
                let first = (multi.len() / 8) as i16;
                for (n, out) in sources {
                    multi.extend([n.to_le_bytes(), out.to_le_bytes(), [0; 2], [0; 2]].concat());
                }
                let src = [
                    (MULTI - first).to_le_bytes(),
                    (sources.len() as i16).to_le_bytes(),
                ];
                ins[BOOL].extend(le(&[pool.add(k)]));
                ins[BOOL].extend(src.concat());
                ins[BOOL].extend([0; 8]);
            }
            for name in spec.outs {
                outs[BOOL].extend(le(&[pool.add(name) | 0x8000_0000]));
            }
        }

        let params_at = COMMANDS + specs.len() * NODE_LEN;
        let props_at = params_at + specs.len() * 0x90;
        let io_at = props_at + 24 + props.iter().map(Vec::len).sum::<usize>();
        let io_len: usize = ins.iter().chain(&outs).map(Vec::len).sum();
        let multi_at = io_at + 48 + io_len;
        let pool_at = multi_at + multi.len();

        let mut nodes = Vec::new();
        let mut params = Vec::new();
        for (n, (spec, (p, s, b, o))) in specs.iter().zip(bases).enumerate() {
            let mut node = vec![0; NODE_LEN];
            node[0..2].copy_from_slice(&spec.kind.to_le_bytes());
            node[2..4].copy_from_slice(&(n as u16).to_le_bytes());
            node[6] = if spec.module { 3 } else { 1 };
            node[8..12].copy_from_slice(&pool.add(spec.name).to_le_bytes());
            node[0x14..0x18].copy_from_slice(&((params_at + n * 0x90) as u32).to_le_bytes());
            nodes.extend(node);
            let mut block = [0u32; 0x24];
            block[STRING * 2] = p as u32;
            block[STRING * 2 + 1] = spec.props.len() as u32;
            let io = |t: usize| 12 + t * 4;
            block[io(STRING)] = s as u32;
            block[io(STRING) + 1] = spec.strings.len() as u32;
            block[io(BOOL)] = b as u32;
            block[io(BOOL) + 1] = spec.multi.is_some() as u32;
            block[io(BOOL) + 2] = o as u32;
            block[io(BOOL) + 3] = spec.outs.len() as u32;
            params.extend(le(&block));
        }

        let mut at = props_at + 24;
        let mut prop_table = Vec::new();
        for t in &props {
            prop_table.extend(le(&[at as u32]));
            at += t.len();
        }
        at += 48;
        let mut io_table = Vec::new();
        for t in 0..6 {
            io_table.extend(le(&[at as u32, (at + ins[t].len()) as u32]));
            at += ins[t].len() + outs[t].len();
        }

        let mut header = vec![0; COMMANDS];
        header[0..4].copy_from_slice(MAGIC);
        let fields = [
            (4, 0x408),
            (0x10, specs.len()),
            (0x24, pool_at),
            (0x2c, props_at),
            (0x30, pool_at),
            (0x34, io_at),
            (0x38, multi_at),
        ];
        for (at, v) in fields {
            header[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
        }

        let mut out = [header, nodes, params, prop_table, props.concat(), io_table].concat();
        for t in 0..6 {
            out.extend(&ins[t]);
            out.extend(&outs[t]);
        }
        out.extend(multi);
        out.extend(pool.0);
        out
    }

    fn flag(name: &'static str) -> Spec {
        Spec {
            kind: 0,
            name: "HoldGameDataGetBoolSimple",
            module: false,
            props: &[],
            strings: vec![("GameDataNameBool", name)],
            multi: None,
            outs: &["Bool"],
        }
    }

    #[test]
    fn reads_nodes_values_and_every_multi_source() {
        let data = build(&[
            flag("Flag_A"),
            flag("Flag_B"),
            Spec {
                kind: 0,
                name: "Sub.logic.module",
                module: true,
                props: &[("InstanceName", "Sub_1")],
                strings: vec![],
                multi: Some(("BoolMulti", &[(0, 0), (1, 0)])),
                outs: &["False", "True"],
            },
            Spec {
                kind: 104,
                name: "",
                module: false,
                props: &[("Name", "Clear")],
                strings: vec![],
                multi: None,
                outs: &["Clear"],
            },
        ]);
        let g = Graph::parse(&data).unwrap();
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.nodes[0].name, "HoldGameDataGetBoolSimple");
        assert_eq!(g.nodes[1].str("GameDataNameBool"), Some("Flag_B"));
        let sub = &g.nodes[2];
        assert!(sub.module && !g.nodes[0].module);
        assert_eq!(sub.str("InstanceName"), Some("Sub_1"));
        let input = sub.input("BoolMulti").unwrap();
        assert_eq!(input.ty, ParamType::Bool);
        let link = |node, output: &str| Link {
            node,
            output: output.into(),
        };
        assert_eq!(input.sources, [link(0, "Bool"), link(1, "Bool")]);
        let names: Vec<_> = sub.outputs.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["False", "True"]);
        assert_eq!(g.nodes[3].kind, NodeKind::ModuleInput);
        assert_eq!(g.nodes[3].str("Name"), Some("Clear"));
    }

    #[test]
    fn patches_relink_and_add_strings() {
        let data = build(&[
            flag("Flag_A"),
            flag("Flag_B"),
            Spec {
                kind: 0,
                name: "OperateAND",
                module: false,
                props: &[("InstanceName", "And_1")],
                strings: vec![],
                multi: Some(("BoolMulti", &[(0, 0), (0, 0)])),
                outs: &["True"],
            },
        ]);
        let mut patch = Patch::new(data.clone()).unwrap();
        patch.relink(2, "BoolMulti", 1, 1, "Bool").unwrap();
        patch.set_str(0, "GameDataNameBool", "Flag_B").unwrap();
        patch.set_str(2, "InstanceName", "And_2").unwrap();
        let bytes = patch.into_bytes();
        assert_eq!(bytes.len(), data.len() + "And_2\0".len());
        let g = Graph::parse(&bytes).unwrap();
        let froms: Vec<_> = g.nodes[2].inputs[0]
            .sources
            .iter()
            .map(|l| l.node)
            .collect();
        assert_eq!(froms, [0, 1]);
        assert_eq!(g.nodes[0].str("GameDataNameBool"), Some("Flag_B"));
        assert_eq!(g.nodes[2].str("InstanceName"), Some("And_2"));
        let mut patch = Patch::new(bytes).unwrap();
        assert!(patch.relink(2, "BoolMulti", 2, 1, "Bool").is_err());
        assert!(patch.relink(2, "BoolMulti", 0, 1, "Nope").is_err());
    }

    #[test]
    fn refuses_other_files() {
        assert!(Graph::parse(b"YB\x07\x00").is_err());
    }
}
