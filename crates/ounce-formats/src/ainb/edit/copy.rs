//! Copying a node out of one graph into another: its entries and strings
//! read out, then pasted onto the end of the target.

use anyhow::{Context, Result, ensure};

use super::{
    ATTACHMENTS, CHILD, File, GENERIC, MULTI_COUNT, NAME, Node, OUTPUT_FLAGS, ParamType,
    QUERY_BASE, READ, STRING_SELECTOR, STRINGS, TYPES, UNSET, set_word, unlink_entry, word,
};
use crate::ainb::file::PARAMS;
use crate::ainb::{NODE_LEN, NodeKind};
use crate::murmur3;

/// Param flags pointing into the blackboard or expressions of their file.
const BOUND: u32 = 0xc200_0000;
const INTS: usize = 5;
const EXPRESSION: u16 = 20;
const INDEX: usize = 0x02;
const EXPRESSIONS: usize = 0x18;
const ATTACHMENT_BASE: usize = 0x20;
const GUID: usize = 0x2c;

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

impl File {
    /// Copies `from`'s node `node` onto the end of this graph and gives its
    /// index. The copy reads no links and no queries, and its child plugs
    /// point nowhere until `set_child` points them, or `to_bytes` refuses.
    pub fn copy_node(&mut self, from: &File, node: usize) -> Result<usize> {
        let copied = from.copied(node)?;
        self.insert(copied)
    }

    /// [`Self::copy_node`] from this same graph.
    pub(in crate::ainb) fn copy_own(&mut self, node: usize) -> Result<usize> {
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
    pub(super) fn guid(&self, n: usize) -> Result<[u8; 16]> {
        let seed = format!("{}#{n}", self.name()?);
        let mut guid = [0; 16];
        for (k, part) in guid.chunks_mut(4).enumerate() {
            part.copy_from_slice(&murmur3(format!("{seed}#{k}").as_bytes()).to_le_bytes());
        }
        Ok(guid)
    }
}
