//! Growing a graph: nodes copied in from other graphs, an output linked to
//! an input, children hung under a node. Enough to add a branch built from
//! the game's own nodes.
//!
//! Links follow the game's own files, checked over every graph in the
//! dump: a bool, float or string input carries a plug on its reader, named
//! by the input and pointing at the source. A pointer output carries one
//! plug on its source, named by the output and pointing at one of its
//! readers. Int and vec3 links follow no one rule, so they aren't made here.
//!
//! An edit that fails can leave the file half-edited: drop it then.

use anyhow::{Context, Result, bail, ensure};

use super::file::{File, Node, PARAMS, set_word, word};
use super::{Graph, MULTI, NODE_LEN, NodeKind, ParamType, TYPES, Value};
use crate::murmur3;

/// An input flag: the input reads its own value, not a link.
const USES_DEFAULT: u32 = 0x80_0000;
/// Param flags pointing into the blackboard or expressions of their file.
const BOUND: u32 = 0xc200_0000;
/// An output flag: something reads the output.
pub(super) const READ: u32 = 0x8000_0000;
/// An output name's flag bits.
const OUTPUT_FLAGS: u32 = 0xc000_0000;
/// Where a copied child plug points until it's set.
pub(super) const UNSET: i32 = -1;
/// What an input's source node reads when nothing is linked.
pub(super) const UNLINKED: i16 = -1;

// Plug kinds.
pub(super) const GENERIC: usize = 0;
const CHILD: usize = 2;
const STRINGS: usize = 4;
const INTS: usize = 5;

// Node kinds.
const USER: u16 = 0;
const SEQUENCE: u16 = 2;
const SIMULTANEOUS: u16 = 3;
const STRING_SELECTOR: u16 = 5;
const BOOL_SELECTOR: u16 = 7;
const EXPRESSION: u16 = 20;

// Node entry fields.
const INDEX: usize = 0x02;
const ATTACHMENTS: usize = 0x04;
const NAME: usize = 0x08;
const EXPRESSIONS: usize = 0x18;
pub(super) const MULTI_COUNT: usize = 0x1c;
const ATTACHMENT_BASE: usize = 0x20;
const QUERY_BASE: usize = 0x24;
const QUERY_COUNT: usize = 0x26;
const GUID: usize = 0x2c;

/// The plug a link carries.
#[derive(Debug, PartialEq)]
pub(super) struct Plug<'a> {
    pub holder: usize,
    pub kind: usize,
    pub name: &'a str,
    pub target: usize,
    pub len: usize,
    /// One plug per name, whichever reader it points at.
    pub shared: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Table {
    Property,
    Input,
    Output,
}

impl Table {
    fn len(self, ty: ParamType) -> usize {
        match self {
            Table::Property => ty.property_len(),
            Table::Input => ty.input_len(),
            Table::Output => ty.output_len(),
        }
    }

    /// Where an entry keeps its strings.
    fn strings(self, ty: ParamType) -> &'static [usize] {
        match (self, ty) {
            (_, ParamType::Pointer) => &[0, 4],
            (Table::Property, ParamType::String) => &[0, 8],
            (Table::Input, ParamType::String) => &[0, 12],
            _ => &[0],
        }
    }

    /// The flag bits sharing an entry's string word at `at`.
    fn string_flags(self, at: usize) -> u32 {
        match (self, at) {
            (Table::Output, 0) => OUTPUT_FLAGS,
            _ => 0,
        }
    }

    fn flags(self, ty: ParamType) -> Option<usize> {
        match self {
            Table::Property => Some(ty.property_value() - 4),
            Table::Input => Some(ty.input_fields().1),
            Table::Output => None,
        }
    }

    /// The node param words holding this table's base and count for type `t`.
    fn param(self, t: usize) -> usize {
        match self {
            Table::Property => 2 * t,
            Table::Input => 12 + 4 * t,
            Table::Output => 14 + 4 * t,
        }
    }
}

/// A type `ty` input entry's source node and output.
fn source(e: &[u8], ty: ParamType) -> (i16, i16) {
    let (at, _, _) = ty.input_fields();
    (
        i16::from_le_bytes([e[at], e[at + 1]]),
        i16::from_le_bytes([e[at + 2], e[at + 3]]),
    )
}

fn set_source(e: &mut [u8], ty: ParamType, node: i16, output: i16) {
    let (at, _, _) = ty.input_fields();
    e[at..at + 2].copy_from_slice(&node.to_le_bytes());
    e[at + 2..at + 4].copy_from_slice(&output.to_le_bytes());
}

/// Cuts a type `ty` input entry's link, so it reads its own value.
fn unlink_entry(e: &mut [u8], ty: ParamType) {
    set_source(e, ty, UNLINKED, 0);
    let (_, flags, _) = ty.input_fields();
    set_word(e, flags, word(e, flags) | USES_DEFAULT);
}

/// A node read out of a graph, with every string it needs, for `insert`.
struct Copied {
    entry: [u8; NODE_LEN],
    name: String,
    /// Per type, its property, input and output entries.
    tables: Vec<Vec<Entry>>,
    /// Each child plug's record and name.
    children: Vec<(Vec<u8>, String)>,
}

/// A table entry, and the string each of its string words names.
struct Entry {
    bytes: Vec<u8>,
    strings: Vec<(usize, String)>,
}

/// The tables a node's entries are copied from, in copy order.
const TABLES: [Table; 3] = [Table::Property, Table::Input, Table::Output];

/// Checks the writer against one of the game's graphs, and gives how many
/// links it checked: the graph comes back byte for byte, and every link the
/// rules above cover already carries the plug `link` would add. A graph the
/// reader can't follow is checked for the round trip alone.
pub fn check(data: &[u8]) -> Result<usize> {
    let f = File::parse(data)?;
    ensure!(f.to_bytes()? == data, "doesn't come back byte for byte");
    let Ok(g) = Graph::parse(data) else {
        return Ok(0);
    };
    let mut checked = 0;
    for (r, node) in g.nodes.iter().enumerate() {
        for input in node.inputs.iter().filter(|i| i.sources.len() == 1) {
            let link = &input.sources[0];
            let Ok(plug) = f.link_plug(r, input.ty, &input.name, link.node, &link.output) else {
                continue;
            };
            checked += 1;
            // A bool selector's plug ends in a word that's 1 in about one in
            // 150 of the game's, and 0 in the rest and in what `link` writes.
            let same = |rec: &&Vec<u8>| {
                f.str(word(rec, 4)).ok() == Some(plug.name)
                    && (plug.shared || word(rec, 0) == plug.target as u32)
                    && rec.len() == plug.len
                    && rec[8..plug.len.min(12)].iter().all(|&b| b == 0)
            };
            let records = &f.nodes[plug.holder].plugs[plug.kind];
            ensure!(
                records.iter().any(|r| same(&r)),
                "node {r}'s {} lacks {plug:?}",
                input.name
            );
        }
    }
    Ok(checked)
}

impl File {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The first node of class `name`, like `OneshotDoNothing`.
    pub fn find(&self, name: &str) -> Option<usize> {
        let named = |n: &Node| self.str(n.u32_at(NAME)).is_ok_and(|s| s == name);
        self.nodes.iter().position(named)
    }

    /// The first bool selector, which picks a child by a bool input.
    pub fn find_bool_selector(&self) -> Option<usize> {
        self.nodes.iter().position(|n| n.kind() == BOOL_SELECTOR)
    }

    /// Copies `from`'s node `node` onto the end of this graph and gives its
    /// index. The copy reads no links and no queries, and its child plugs
    /// point nowhere until `set_child` points them, or `to_bytes` refuses.
    pub fn copy_node(&mut self, from: &File, node: usize) -> Result<usize> {
        let copied = from.copied(node)?;
        self.insert(copied)
    }

    /// [`Self::copy_node`] from this same graph.
    pub(super) fn copy_own(&mut self, node: usize) -> Result<usize> {
        let copied = self.copied(node)?;
        self.insert(copied)
    }

    /// Node `node` read out for a copy, refusing what a copy can't carry.
    fn copied(&self, node: usize) -> Result<Copied> {
        let src = self.node(node)?;
        let kind = src.kind();
        let io = matches!(
            NodeKind::from(kind),
            NodeKind::ModuleInput | NodeKind::ModuleOutput
        );
        let plain = !matches!(kind, STRING_SELECTOR | EXPRESSION) && !io;
        ensure!(plain, "can't copy node {node} of kind {kind}");
        let extras = [ATTACHMENTS, EXPRESSIONS, EXPRESSIONS + 2];
        let extra = extras.iter().any(|&at| src.u16_at(at) != 0);
        ensure!(!extra, "node {node} has attachments or expressions");
        for (k, plugs) in src.plugs.iter().enumerate() {
            let links = matches!(k, GENERIC | CHILD | STRINGS | INTS);
            ensure!(links || plugs.is_empty(), "node {node} has kind {k} plugs");
        }
        let mut tables = Vec::new();
        for t in 0..TYPES.len() {
            for table in TABLES {
                let range = match table {
                    Table::Property => src.properties(t),
                    Table::Input => src.inputs(t),
                    Table::Output => src.outputs(t),
                };
                let entries = range.map(|i| self.entry(table, t, i));
                tables.push(entries.collect::<Result<_>>()?);
            }
        }
        let children = src.plugs[CHILD]
            .iter()
            .map(|r| Ok((r.clone(), self.str(word(r, 4))?.to_owned())));
        Ok(Copied {
            entry: src.entry,
            name: self.str(src.u32_at(NAME))?.to_owned(),
            tables,
            children: children.collect::<Result<_>>()?,
        })
    }

    /// Entry `i` of a type `t` table, with its strings.
    fn entry(&self, table: Table, t: usize, i: usize) -> Result<Entry> {
        let ty = TYPES[t];
        let len = table.len(ty);
        let bytes = (self.table(table, t).get(i * len..(i + 1) * len))
            .context("an entry past its table")?
            .to_vec();
        let strings = table.strings(ty).iter().map(|&at| {
            let w = word(&bytes, at) & !table.string_flags(at);
            Ok((at, self.str(w)?.to_owned()))
        });
        let strings = strings.collect::<Result<_>>()?;
        if let Some(at) = table.flags(ty) {
            ensure!(
                word(&bytes, at) & BOUND == 0,
                "a param bound to its graph's blackboard"
            );
        }
        Ok(Entry { bytes, strings })
    }

    /// Puts `c` on the end of the graph, its strings moved to this pool,
    /// and gives its index.
    fn insert(&mut self, c: Copied) -> Result<usize> {
        let n = self.nodes.len();
        let mut copy = Node {
            entry: c.entry,
            params: [0; PARAMS],
            plugs: Default::default(),
        };
        copy.set_u16(INDEX, u16::try_from(n)?);
        copy.set_u32(NAME, self.intern(&c.name));
        copy.set_u32(ATTACHMENT_BASE, 0);
        copy.set_u32(QUERY_BASE, 0);
        copy.set_u16(MULTI_COUNT, 0);
        copy.entry[GUID..GUID + 16].copy_from_slice(&self.guid(n)?);
        let tables = (0..TYPES.len()).flat_map(|t| TABLES.map(|table| (t, table)));
        for ((t, table), entries) in tables.zip(c.tables) {
            copy.params[table.param(t)] = (self.table(table, t).len() / table.len(TYPES[t])) as u32;
            copy.params[table.param(t) + 1] = entries.len() as u32;
            for e in entries {
                self.paste(table, t, e);
            }
        }
        for (mut record, name) in c.children {
            set_word(&mut record, 0, UNSET as u32);
            set_word(&mut record, 4, self.intern(&name));
            copy.plugs[CHILD].push(record);
        }
        self.nodes.push(copy);
        Ok(n)
    }

    /// Appends `e` to a type `t` table, its strings moved to this pool and
    /// an input unlinked.
    fn paste(&mut self, table: Table, t: usize, e: Entry) {
        let mut bytes = e.bytes;
        for (at, s) in e.strings {
            let flags = table.string_flags(at);
            let w = word(&bytes, at);
            let s = self.intern(&s);
            set_word(&mut bytes, at, s | (w & flags & !READ));
        }
        if table == Table::Input {
            unlink_entry(&mut bytes, TYPES[t]);
        }
        self.table_mut(table, t).extend(bytes);
    }

    fn table(&self, table: Table, t: usize) -> &Vec<u8> {
        match table {
            Table::Property => &self.properties[t],
            Table::Input => &self.inputs[t],
            Table::Output => &self.outputs[t],
        }
    }

    fn table_mut(&mut self, table: Table, t: usize) -> &mut Vec<u8> {
        match table {
            Table::Property => &mut self.properties[t],
            Table::Input => &mut self.inputs[t],
            Table::Output => &mut self.outputs[t],
        }
    }

    /// A node id the same on every run, unique within the graph.
    fn guid(&self, n: usize) -> Result<[u8; 16]> {
        let seed = format!("{}#{n}", self.name()?);
        let mut guid = [0; 16];
        for (k, part) in guid.chunks_mut(4).enumerate() {
            part.copy_from_slice(&murmur3(format!("{seed}#{k}").as_bytes()).to_le_bytes());
        }
        Ok(guid)
    }

    fn node(&self, n: usize) -> Result<&Node> {
        self.nodes.get(n).with_context(|| format!("no node {n}"))
    }

    fn input_entry(&self, t: usize, i: usize) -> &[u8] {
        let len = TYPES[t].input_len();
        &self.inputs[t][i * len..(i + 1) * len]
    }

    fn input_entry_mut(&mut self, t: usize, i: usize) -> &mut [u8] {
        let len = TYPES[t].input_len();
        &mut self.inputs[t][i * len..(i + 1) * len]
    }

    /// Type `t` input `i`'s source node and output (or, for several
    /// sources, the encoded first entry and the count).
    pub(super) fn source_of(&self, t: usize, i: usize) -> (i16, i16) {
        source(self.input_entry(t, i), TYPES[t])
    }

    pub(super) fn set_source_of(&mut self, t: usize, i: usize, node: i16, output: i16) {
        set_source(self.input_entry_mut(t, i), TYPES[t], node, output);
    }

    /// `source`'s type `t` `output`, marked read by `reader`: its place
    /// among the node's outputs of that type. Fails before any write.
    pub(super) fn mark_read(
        &mut self,
        reader: usize,
        t: usize,
        source: usize,
        output: &str,
    ) -> Result<i16> {
        let (k, o) = self.output(source, t, output)?;
        let k = i16::try_from(k)?;
        if self.nodes[source].is_query() {
            self.add_query(reader, source)?;
        }
        let at = o * TYPES[t].output_len();
        let read = word(&self.outputs[t], at) | READ;
        set_word(&mut self.outputs[t], at, read);
        Ok(k)
    }

    /// `node`'s input `name`: its type and its index in that type's table.
    pub(super) fn input(&self, node: usize, name: &str) -> Result<(usize, usize)> {
        let n = self.node(node)?;
        for (t, ty) in TYPES.iter().enumerate() {
            let len = ty.input_len();
            for i in n.inputs(t) {
                if self.str(word(&self.inputs[t], i * len))? == name {
                    return Ok((t, i));
                }
            }
        }
        bail!("node {node} has no input {name}")
    }

    /// `node`'s type `t` output `name`: its place among the node's outputs
    /// of that type, and its index in the table.
    pub(super) fn output(&self, node: usize, t: usize, name: &str) -> Result<(usize, usize)> {
        let len = TYPES[t].output_len();
        for (k, i) in self.node(node)?.outputs(t).enumerate() {
            if self.str(word(&self.outputs[t], i * len) & !OUTPUT_FLAGS)? == name {
                return Ok((k, i));
            }
        }
        bail!("node {node} has no {:?} output {name}", TYPES[t])
    }

    /// The plug the game's files put on a link from `source`'s `output` to
    /// `reader`'s `input`.
    pub(super) fn link_plug<'a>(
        &self,
        reader: usize,
        ty: ParamType,
        input: &'a str,
        source: usize,
        output: &'a str,
    ) -> Result<Plug<'a>> {
        let (r, s) = (self.node(reader)?.kind(), self.node(source)?.kind());
        Ok(match ty {
            ParamType::Pointer => {
                ensure!(s == USER, "a pointer link from a node of kind {s}");
                let (holder, name, target) = (source, output, reader);
                Plug {
                    holder,
                    kind: GENERIC,
                    name,
                    target,
                    len: 8,
                    shared: true,
                }
            }
            ParamType::Bool | ParamType::Float | ParamType::String => {
                let len = match (ty, r) {
                    (_, USER) => 8,
                    (ParamType::Bool, BOOL_SELECTOR) => 16,
                    _ => bail!("a {ty:?} link into a node of kind {r}"),
                };
                let kind = if ty == ParamType::String {
                    STRINGS
                } else {
                    GENERIC
                };
                let (holder, name, target) = (reader, input, source);
                Plug {
                    holder,
                    kind,
                    name,
                    target,
                    len,
                    shared: false,
                }
            }
            ParamType::Int | ParamType::Vec3 => bail!("{ty:?} links follow no one rule"),
        })
    }

    /// Feeds `reader`'s `input` from `source`'s `output`.
    pub fn link(&mut self, reader: usize, input: &str, source: usize, output: &str) -> Result<()> {
        let (t, i) = self.input(reader, input)?;
        let ty = TYPES[t];
        self.output(source, t, output)?;
        let plug = self.link_plug(reader, ty, input, source, output)?;
        ensure!(
            self.source_of(t, i).0 > MULTI,
            "{input} has several sources"
        );
        let from = i16::try_from(source)?;
        let k = self.mark_read(reader, t, source, output)?;
        self.set_source_of(t, i, from, k);
        let (_, flags, _) = ty.input_fields();
        let e = self.input_entry_mut(t, i);
        set_word(e, flags, word(e, flags) & !USES_DEFAULT);
        self.add_plug(plug)
    }

    /// Points `parent`'s child plug on `from` at `to` instead.
    pub fn repoint_child(&mut self, parent: usize, from: usize, to: usize) -> Result<()> {
        self.node(to)?;
        self.node(parent)?;
        let plugs = &mut self.nodes[parent].plugs[CHILD];
        let plug = (plugs.iter_mut()).find(|r| word(r, 0) == from as u32);
        set_word(
            plug.with_context(|| format!("#{parent} has no child #{from}"))?,
            0,
            to as u32,
        );
        Ok(())
    }

    /// Cuts the link into `reader`'s `input`, which reads `value` from then
    /// on: what `link` does, undone. A query source stays among the
    /// reader's queries.
    pub fn unlink(&mut self, reader: usize, input: &str, value: &Value) -> Result<()> {
        let (t, i) = self.input(reader, input)?;
        let ty = TYPES[t];
        let raw = self.raw(ty, input, value)?;
        let (source, k) = self.source_of(t, i);
        ensure!(source >= 0, "{input} has no single source");
        let (source, k) = (source as usize, usize::try_from(k)?);
        let o = (self.node(source)?.outputs(t).nth(k)).context("no such output")?;
        let output = self.str(word(&self.outputs[t], o * ty.output_len()) & !OUTPUT_FLAGS)?;
        let output = output.to_owned();
        let plug = self.link_plug(reader, ty, input, source, &output)?;
        let name = self.intern(plug.name);
        let records = &mut self.nodes[plug.holder].plugs[plug.kind];
        let held = records.len();
        records.retain(|r| !(word(r, 4) == name && word(r, 0) == plug.target as u32));
        ensure!(records.len() < held, "{input} carries no plug");
        let (_, _, value_at) = ty.input_fields();
        let e = self.input_entry_mut(t, i);
        unlink_entry(e, ty);
        set_word(e, value_at, raw);
        Ok(())
    }

    /// Adds `plug` unless its holder has it. A pointer output keeps the one
    /// plug it has, whichever reader that points at.
    fn add_plug(&mut self, plug: Plug) -> Result<()> {
        let name = self.intern(plug.name);
        let records = &self.nodes[plug.holder].plugs[plug.kind];
        let same =
            |r: &Vec<u8>| word(r, 4) == name && (plug.shared || word(r, 0) == plug.target as u32);
        if records.iter().any(same) {
            return Ok(());
        }
        let mut record = vec![0; plug.len];
        set_word(&mut record, 0, plug.target as u32);
        set_word(&mut record, 4, name);
        self.nodes[plug.holder].plugs[plug.kind].push(record);
        Ok(())
    }

    /// Adds `source` to the query nodes `reader` waits on, inserting it
    /// after the reader's own and moving the ones behind, so the game's
    /// order stays. Fails before any write.
    pub(super) fn add_query(&mut self, reader: usize, source: usize) -> Result<()> {
        let place = self.nodes[..source].iter().filter(|n| n.is_query()).count();
        let place = u16::try_from(place)?;
        let node = &self.nodes[reader];
        let (base, count) = (node.u16_at(QUERY_BASE) as usize, node.u16_at(QUERY_COUNT));
        let held = self.queries.get(base * 4..(base + count as usize) * 4);
        let held = held.context("queries past the table")?;
        if held.chunks(4).any(|q| q[..2] == place.to_le_bytes()) {
            return Ok(());
        }
        let at = match count {
            0 => self.queries.len() / 4,
            _ => base + count as usize,
        };
        let new_base = u16::try_from(at - count as usize)?;
        let entry = [place.to_le_bytes(), [0, 0]].concat();
        self.queries.splice(at * 4..at * 4, entry);
        for (i, n) in self.nodes.iter_mut().enumerate() {
            let b = n.u16_at(QUERY_BASE);
            if i != reader && n.u16_at(QUERY_COUNT) > 0 && b as usize >= at {
                n.set_u16(QUERY_BASE, b + 1);
            }
        }
        let node = &mut self.nodes[reader];
        node.set_u16(QUERY_BASE, new_base);
        node.set_u16(QUERY_COUNT, count + 1);
        Ok(())
    }

    /// Points `parent`'s child plug `name` at `child`.
    pub fn set_child(&mut self, parent: usize, name: &str, child: usize) -> Result<()> {
        self.node(child)?;
        self.node(parent)?;
        // Interned first: the plug's name word is compared against it.
        let name = self.intern(name);
        let plugs = &mut self.nodes[parent].plugs[CHILD];
        let mut named = plugs.iter_mut().filter(|r| word(r, 4) == name);
        let (Some(plug), None) = (named.next(), named.next()) else {
            bail!("node {parent} hasn't exactly one such child plug");
        };
        set_word(plug, 0, child as u32);
        Ok(())
    }

    /// Hangs `child` under `parent` on a new child plug `name`.
    pub fn add_child(&mut self, parent: usize, name: &str, child: usize) -> Result<()> {
        let kind = self.node(parent)?.kind();
        ensure!(
            matches!(kind, USER | SEQUENCE | SIMULTANEOUS),
            "a child under kind {kind}"
        );
        self.node(child)?;
        let name = self.intern(name);
        let plugs = &mut self.nodes[parent].plugs[CHILD];
        ensure!(
            plugs.iter().all(|r| r.len() == 8),
            "node {parent}'s child plugs hold more"
        );
        let mut record = vec![0; 8];
        set_word(&mut record, 0, child as u32);
        set_word(&mut record, 4, name);
        plugs.push(record);
        Ok(())
    }

    /// `value` as a type `ty` param's word.
    fn raw(&mut self, ty: ParamType, name: &str, value: &Value) -> Result<u32> {
        Ok(match (ty, value) {
            (ParamType::Int, Value::Int(v)) => *v as u32,
            (ParamType::Bool, Value::Bool(v)) => *v as u32,
            (ParamType::Float, Value::Float(v)) => v.to_bits(),
            (ParamType::String, Value::String(s)) => self.intern(s),
            _ => bail!("{name} is {ty:?}, not {value:?}"),
        })
    }

    /// Sets `node`'s property `name` to `value`.
    pub fn set_property(&mut self, node: usize, name: &str, value: &Value) -> Result<()> {
        for (t, &ty) in TYPES.iter().enumerate() {
            let len = ty.property_len();
            for i in self.node(node)?.properties(t) {
                if self.str(word(&self.properties[t], i * len))? != name {
                    continue;
                }
                let raw = self.raw(ty, name, value)?;
                set_word(&mut self.properties[t][i * len..], ty.property_value(), raw);
                return Ok(());
            }
        }
        bail!("node {node} has no property {name}")
    }

    /// Sets `node`'s unlinked `input` to `value`.
    pub fn set_input(&mut self, node: usize, input: &str, value: &Value) -> Result<()> {
        let (t, i) = self.input(node, input)?;
        let ty = TYPES[t];
        let raw = self.raw(ty, input, value)?;
        let (_, flags, at) = ty.input_fields();
        let e = self.input_entry_mut(t, i);
        ensure!(word(e, flags) & USES_DEFAULT != 0, "{input} reads a link");
        set_word(e, at, raw);
        Ok(())
    }

    /// The query nodes `node` waits on, by node index.
    pub fn queries(&self, node: usize) -> Result<Vec<usize>> {
        let n = self.node(node)?;
        let (base, count) = (
            n.u16_at(QUERY_BASE) as usize,
            n.u16_at(QUERY_COUNT) as usize,
        );
        let held = self
            .queries
            .get(base * 4..(base + count) * 4)
            .context("queries past the table")?;
        let queries: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| self.nodes[i].is_query())
            .collect();
        held.chunks(4)
            .map(|q| {
                let place = u16::from_le_bytes([q[0], q[1]]) as usize;
                queries
                    .get(place)
                    .copied()
                    .context("a query past the query nodes")
            })
            .collect()
    }

    /// `node`'s child plugs, by name and the node each points at.
    pub fn children(&self, node: usize) -> Result<Vec<(String, i32)>> {
        let plugs = &self.node(node)?.plugs[CHILD];
        plugs
            .iter()
            .map(|r| Ok((self.str(word(r, 4))?.to_owned(), word(r, 0) as i32)))
            .collect()
    }

    /// `node`'s plugs past its children (kind 3 and up), by kind and the
    /// node each points at.
    pub fn links(&self, node: usize) -> Result<Vec<(usize, i32)>> {
        let plugs = &self.node(node)?.plugs;
        Ok((plugs.iter().enumerate().skip(CHILD + 1))
            .flat_map(|(kind, records)| records.iter().map(move |r| (kind, word(r, 0) as i32)))
            .collect())
    }

    /// The first child plug still pointing nowhere, by node and plug name.
    pub(super) fn unset(&self) -> Option<(usize, String)> {
        let nodes = self.nodes.iter().enumerate();
        nodes
            .flat_map(|(n, node)| node.plugs[CHILD].iter().map(move |r| (n, r)))
            .find_map(|(n, r)| {
                (word(r, 0) as i32 == UNSET)
                    .then(|| (n, self.str(word(r, 4)).unwrap_or("?").into()))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ainb::Link;
    use crate::ainb::file::tests::{Fake, fake};

    use ParamType::{Bool, Int, Pointer, String as Str};

    /// A made-up game graph holding every node a drop branch copies.
    fn game() -> File {
        fake(vec![
            Fake {
                kind: SIMULTANEOUS,
                plugs: vec![(CHILD, 3, "", 8)],
                ..Default::default()
            },
            Fake {
                flags: 1,
                name: "QueryFakeActor",
                outputs: vec![(Pointer, "Actor")],
                plugs: vec![(GENERIC, 2, "Actor", 8)],
                ..Default::default()
            },
            Fake {
                flags: 1,
                name: "QueryFakeName",
                inputs: vec![(Pointer, "Actor", Some((1, 0))), (Str, "Other", None)],
                outputs: vec![(Str, "Name")],
                queries: vec![0],
                ..Default::default()
            },
            Fake {
                name: "OneshotFakeNothing",
                props: vec![(Bool, "ExecuteEveryFrame")],
                ..Default::default()
            },
            Fake {
                kind: BOOL_SELECTOR,
                inputs: vec![(Bool, "Input", Some((5, 0)))],
                plugs: vec![
                    (GENERIC, 5, "Input", 16),
                    (CHILD, 3, "True", 8),
                    (CHILD, 3, "False", 8),
                ],
                queries: vec![2],
                ..Default::default()
            },
            Fake {
                flags: 1,
                name: "QueryFakeStartsWith",
                inputs: vec![(Str, "Input", Some((2, 0))), (Str, "Token", None)],
                outputs: vec![(Bool, "Result")],
                plugs: vec![(STRINGS, 2, "Input", 8)],
                queries: vec![1],
                ..Default::default()
            },
            Fake {
                name: "OneShotFakeDeath",
                inputs: vec![(Int, "Reason", None)],
                ..Default::default()
            },
            Fake {
                flags: 1,
                name: "QueryFakeOtherName",
                outputs: vec![(Str, "Name")],
                ..Default::default()
            },
        ])
    }

    fn plug(f: &File, node: usize, kind: usize) -> Vec<(i32, String, usize)> {
        let plugs = &f.nodes[node].plugs[kind];
        plugs
            .iter()
            .map(|r| {
                (
                    word(r, 0) as i32,
                    f.str(word(r, 4)).unwrap().into(),
                    r.len(),
                )
            })
            .collect()
    }

    #[test]
    fn links_list_the_plugs_past_the_children() {
        let f = game();
        // The string input link, not the generic one or the children.
        assert_eq!(f.links(5).unwrap(), vec![(STRINGS, 2)]);
        assert!(f.links(4).unwrap().is_empty());
        assert!(f.links(0).unwrap().is_empty());
    }

    #[test]
    fn a_branch_of_copied_nodes_reads_back() {
        let game = game();
        let mut f = fake(vec![
            Fake {
                kind: SIMULTANEOUS,
                plugs: vec![(CHILD, 1, "", 8)],
                ..Default::default()
            },
            Fake {
                name: "OneshotFakeNothing",
                ..Default::default()
            },
        ]);
        let actor = f.copy_node(&game, 1).unwrap();
        let name = f.copy_node(&game, 2).unwrap();
        let starts = f.copy_node(&game, 5).unwrap();
        let select = f.copy_node(&game, 4).unwrap();
        let death = f.copy_node(&game, 6).unwrap();
        assert!(
            f.to_bytes().is_err(),
            "the selector's children point nowhere"
        );
        f.link(name, "Actor", actor, "Actor").unwrap();
        f.link(starts, "Input", name, "Name").unwrap();
        f.set_input(starts, "Token", &Value::String("Fake_".into()))
            .unwrap();
        f.link(select, "Input", starts, "Result").unwrap();
        f.set_child(select, "True", death).unwrap();
        f.set_child(select, "False", 1).unwrap();
        f.set_input(death, "Reason", &Value::Int(10)).unwrap();
        f.add_child(0, "", select).unwrap();

        let bytes = f.to_bytes().unwrap();
        let back = File::parse(&bytes).unwrap();
        assert_eq!(back, f);
        let g = Graph::parse(&bytes).unwrap();
        let fed = |n: usize, input: &str| g.nodes[n].input(input).unwrap().sources.clone();
        let link = |node, output: &str| {
            vec![Link {
                node,
                output: output.into(),
            }]
        };
        assert_eq!(fed(name, "Actor"), link(actor, "Actor"));
        assert_eq!(fed(starts, "Input"), link(name, "Name"));
        assert_eq!(fed(select, "Input"), link(starts, "Result"));
        assert_eq!(g.nodes[name].str("Other"), Some(""));
        assert_eq!(g.nodes[starts].str("Token"), Some("Fake_"));
        assert_eq!(g.nodes[death].value("Reason"), Some(&Value::Int(10)));
        assert_eq!(g.nodes[select].name, "");
        assert_eq!(g.nodes[death].name, "OneShotFakeDeath");

        assert_eq!(back.queries(actor).unwrap(), Vec::<usize>::new());
        assert_eq!(back.queries(name).unwrap(), [actor]);
        assert_eq!(back.queries(starts).unwrap(), [name]);
        assert_eq!(back.queries(select).unwrap(), [starts]);
        assert_eq!(back.queries(death).unwrap(), Vec::<usize>::new());
        let children = back.children(select).unwrap();
        assert_eq!(
            children,
            [("True".into(), death as i32), ("False".into(), 1)]
        );
        assert_eq!(
            back.children(0).unwrap(),
            [("".into(), 1), ("".into(), select as i32)]
        );
        assert_eq!(
            plug(&back, actor, GENERIC),
            [(name as i32, "Actor".into(), 8)]
        );
        assert_eq!(
            plug(&back, starts, STRINGS),
            [(name as i32, "Input".into(), 8)]
        );
        assert_eq!(
            plug(&back, select, GENERIC),
            [(starts as i32, "Input".into(), 16)]
        );
        assert!(plug(&back, name, GENERIC).is_empty());
        assert_ne!(back.guid(actor).unwrap(), back.guid(name).unwrap());
        assert_eq!(back.find("OneShotFakeDeath"), Some(death));
        assert_eq!(back.find_bool_selector(), Some(select));
    }

    #[test]
    fn a_new_query_keeps_the_others_in_place_and_fails_first() {
        let mut f = game();
        let before = [f.queries(4).unwrap(), f.queries(5).unwrap()];
        let (base, outputs) = (f.nodes[2].u16_at(QUERY_BASE), f.outputs.clone());
        f.nodes[2].set_u16(QUERY_BASE, 0xfff);
        assert!(
            f.link(2, "Other", 7, "Name").is_err(),
            "queries past the table"
        );
        assert_eq!(f.outputs, outputs, "a failed link marks no output read");
        f.nodes[2].set_u16(QUERY_BASE, base);
        f.link(2, "Other", 7, "Name").unwrap();
        assert_ne!(f.outputs, outputs);
        assert_eq!(f.queries(2).unwrap(), [1, 7]);
        assert_eq!([f.queries(4).unwrap(), f.queries(5).unwrap()], before);
        let base = |n: usize| f.nodes[n].u16_at(QUERY_BASE);
        assert_eq!([base(2), base(4), base(5)], [0, 2, 3]);
        f.link(2, "Other", 7, "Name").unwrap();
        assert_eq!(f.queries(2).unwrap(), [1, 7], "linking again adds nothing");
        assert_eq!(
            f.nodes[7].plugs[STRINGS].len() + f.nodes[2].plugs[STRINGS].len(),
            1
        );
    }

    #[test]
    fn refuses_what_the_game_does_no_one_way() {
        let game = game();
        let mut f = game.clone();
        assert!(
            f.link(6, "Reason", 1, "Actor").is_err(),
            "an int input fed a pointer"
        );
        assert!(f.link(2, "Nope", 1, "Actor").is_err());
        assert!(f.link(2, "Actor", 1, "Nope").is_err());
        assert!(
            f.set_input(2, "Actor", &Value::Int(1)).is_err(),
            "a linked input"
        );
        assert!(
            f.set_input(6, "Reason", &Value::Bool(true)).is_err(),
            "the wrong type"
        );
        assert!(
            f.add_child(4, "", 3).is_err(),
            "a selector's children are its cases"
        );
        assert!(f.set_child(0, "Nope", 3).is_err());
        assert!(
            f.set_child(4, "True", 99).is_err(),
            "a child past the nodes"
        );
        assert!(f.set_child(99, "True", 3).is_err());
        let mut odd = fake(vec![Fake {
            kind: STRING_SELECTOR,
            ..Default::default()
        }]);
        assert!(f.copy_node(&odd.clone(), 0).is_err());
        odd.nodes[0].set_u16(0, USER);
        odd.nodes[0].set_u16(ATTACHMENTS, 1);
        assert!(f.copy_node(&odd, 0).is_err());
    }

    #[test]
    fn a_repointed_child_runs_the_other_node() {
        let mut f = game();
        f.repoint_child(4, 3, 7).unwrap();
        assert_eq!(
            plug(&f, 4, CHILD).iter().map(|p| p.0).collect::<Vec<_>>(),
            [7, 3]
        );
        assert!(f.repoint_child(4, 6, 7).is_err());
    }

    #[test]
    fn an_unlinked_input_reads_its_value_and_loses_its_plug() {
        let mut f = game();
        f.unlink(4, "Input", &Value::Bool(true)).unwrap();
        assert!(plug(&f, 4, GENERIC).is_empty());
        assert_eq!(plug(&f, 4, CHILD).len(), 2);
        let bytes = f.to_bytes().unwrap();
        assert_eq!(check(&bytes).unwrap(), 2);
        let input = &crate::ainb::Graph::parse(&bytes).unwrap().nodes[4].inputs[0];
        assert!(input.sources.is_empty());
        assert_eq!(input.value, Value::Bool(true));
        assert!(f.unlink(4, "Input", &Value::Bool(true)).is_err());
    }

    #[test]
    fn checks_a_graph_against_the_link_rules() {
        let game = game();
        assert_eq!(check(&game.to_bytes().unwrap()).unwrap(), 3);
        let mut lost = game.clone();
        lost.nodes[5].plugs[STRINGS].clear();
        assert!(check(&lost.to_bytes().unwrap()).is_err());
        assert!(check(b"YB\x07\x00").is_err());
    }
}
