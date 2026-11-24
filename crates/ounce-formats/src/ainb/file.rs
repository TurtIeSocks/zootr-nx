//! An AINB file as its sections, so a graph can grow. `File::parse` then
//! `to_bytes` gives back the same bytes, and a node added in between moves
//! every section after it.
//!
//! Sections nothing here edits stay bytes. The header's counts and
//! offsets, each node body's plug offsets and the jump table's offsets are
//! rebuilt on write. Attachments hold absolute offsets this doesn't track:
//! a file with them round-trips, but can't grow.

use std::ops::Range;

use anyhow::{Context, Result, ensure};

use super::{COMMAND_LEN, COMMANDS, NODE_LEN, TYPES, check_header, until_nul};
use crate::reader::Reader;

/// A node body's property and input/output base and count per type.
pub(super) const PARAMS: usize = 0x24;
/// Plug kinds, each with a count and a base in the node body.
pub(super) const PLUG_KINDS: usize = 10;
const PLUG_TABLE: usize = PARAMS * 4 + PLUG_KINDS * 2;
/// The node entry's body offset, rebuilt on write.
const BODY: usize = 0x14;
/// The node entry's state offset, which only older versions use.
const STATE: usize = 0x28;

/// Where the header keeps each count and section offset.
mod slot {
    pub const FILENAME: usize = 0x08;
    pub const COMMANDS: usize = 0x0c;
    pub const NODES: usize = 0x10;
    pub const QUERY_NODES: usize = 0x14;
    pub const BLACKBOARD: usize = 0x20;
    pub const POOL: usize = 0x24;
    pub const ENUMS: usize = 0x28;
    pub const PROPERTIES: usize = 0x2c;
    pub const JUMPS: usize = 0x30;
    pub const IO: usize = 0x34;
    pub const MULTI: usize = 0x38;
    pub const ATTACHMENT_LIST: usize = 0x3c;
    pub const ATTACHMENT_SLOTS: usize = 0x40;
    pub const EXB: usize = 0x44;
    pub const REPLACEMENTS: usize = 0x48;
    pub const QUERIES: usize = 0x4c;
    /// Always the jump table's offset again.
    pub const JUMPS_AGAIN: usize = 0x50;
    pub const ZERO: usize = 0x54;
    /// A section only older games use.
    pub const OLD: usize = 0x58;
    pub const MODULES: usize = 0x5c;
    pub const ACTIONS: usize = 0x68;
    pub const TAIL: usize = 0x6c;
    pub const IDS: usize = 0x70;

    /// Every slot rebuilt on write, cleared in a parsed header. The
    /// attachment count (0x18) and the output node count (0x1c), which in
    /// some files isn't quite the module outputs, stay as read.
    pub const DERIVED: [usize; 21] = [
        COMMANDS,
        NODES,
        QUERY_NODES,
        BLACKBOARD,
        POOL,
        ENUMS,
        PROPERTIES,
        JUMPS,
        IO,
        MULTI,
        ATTACHMENT_LIST,
        ATTACHMENT_SLOTS,
        EXB,
        REPLACEMENTS,
        QUERIES,
        JUMPS_AGAIN,
        MODULES,
        ACTIONS,
        TAIL,
        IDS,
        OLD,
    ];

    /// The small sections between the expressions and the pool, in order.
    pub const REST: [usize; 6] = [MODULES, ACTIONS, IDS, REPLACEMENTS, TAIL, ENUMS];

    /// The sections after the input/output table, in file order. The
    /// expression binary is the one that may be missing.
    pub const LATE: [usize; 11] = [
        MULTI,
        JUMPS,
        QUERIES,
        EXB,
        MODULES,
        ACTIONS,
        IDS,
        REPLACEMENTS,
        TAIL,
        ENUMS,
        POOL,
    ];
}

/// The little-endian word at `at`.
pub(super) fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

pub(super) fn set_word(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// A plug record's shortest: the node it points at, then its name.
const PLUG_MIN: usize = 8;

/// Each kind's plug records, each starting with the node it points at.
pub(super) type Plugs = [Vec<Vec<u8>>; PLUG_KINDS];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Node {
    /// The node's entry, with its body offset cleared.
    pub entry: [u8; NODE_LEN],
    /// Per type, the property base and count, then per type the input
    /// base and count and the output base and count.
    pub params: [u32; PARAMS],
    /// Each kind's plug records, each starting with the node it points at.
    pub plugs: Plugs,
}

impl Node {
    pub fn kind(&self) -> u16 {
        u16::from_le_bytes([self.entry[0], self.entry[1]])
    }
    pub fn flags(&self) -> u8 {
        self.entry[6]
    }
    pub fn is_query(&self) -> bool {
        self.flags() & 1 != 0
    }
    pub fn u16_at(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.entry[at], self.entry[at + 1]])
    }
    pub fn u32_at(&self, at: usize) -> u32 {
        word(&self.entry, at)
    }
    pub fn set_u16(&mut self, at: usize, v: u16) {
        self.entry[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn set_u32(&mut self, at: usize, v: u32) {
        set_word(&mut self.entry, at, v);
    }

    /// The `[base, base + count)` of type `t`'s properties.
    pub fn properties(&self, t: usize) -> Range<usize> {
        span(self.params[2 * t], self.params[2 * t + 1])
    }
    pub fn inputs(&self, t: usize) -> Range<usize> {
        span(self.params[12 + 4 * t], self.params[13 + 4 * t])
    }
    pub fn outputs(&self, t: usize) -> Range<usize> {
        span(self.params[14 + 4 * t], self.params[15 + 4 * t])
    }

    fn parse(r: &Reader, at: usize, end: usize) -> Result<([u32; PARAMS], Plugs)> {
        let mut params = [0; PARAMS];
        for (i, p) in params.iter_mut().enumerate() {
            *p = r.u32(at + i * 4)?;
        }
        let mut plugs = Plugs::default();
        let counts = r.bytes(at + PARAMS * 4, PLUG_KINDS * 2)?;
        let total: usize = counts.chunks(2).map(|c| c[0] as usize).sum();
        let table = at + PLUG_TABLE;
        let mut next = table + 4 * total;
        let mut i = 0;
        for (kind, c) in counts.chunks(2).enumerate() {
            ensure!(c[1] as usize == i, "plug kinds out of order at {at:#x}");
            for _ in 0..c[0] {
                let start = r.u32(table + 4 * i)? as usize;
                ensure!(start == next, "plug records out of order at {at:#x}");
                i += 1;
                next = match i < total {
                    true => r.u32(table + 4 * i)? as usize,
                    false => end,
                };
                ensure!(
                    next >= start + PLUG_MIN,
                    "a plug record at {start:#x} too short"
                );
                plugs[kind].push(r.bytes(start, next - start)?.to_vec());
            }
        }
        ensure!(next == end, "a node body at {at:#x} ends short of the next");
        Ok((params, plugs))
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        let at = out.len();
        out.extend(self.params.iter().flat_map(|p| p.to_le_bytes()));
        let mut base = 0usize;
        for kind in &self.plugs {
            let byte = |n: usize| u8::try_from(n).context("more than 255 plugs");
            out.extend([byte(kind.len())?, byte(base)?]);
            base += kind.len();
        }
        let mut next = at + PLUG_TABLE + 4 * base;
        for record in self.plugs.iter().flatten() {
            out.extend((next as u32).to_le_bytes());
            next += record.len();
        }
        out.extend(self.plugs.iter().flatten().flatten());
        Ok(())
    }
}

fn span(base: u32, count: u32) -> Range<usize> {
    base as usize..base as usize + count as usize
}

/// Attachments and their parameter blocks, with the offsets inside them.
#[derive(Clone, Debug, PartialEq)]
struct Attachments {
    /// Where the section sat when read, when it holds anything: offsets
    /// inside it point there, so it can't move.
    was: Option<usize>,
    /// The attachment list's offset from the section's start.
    list: usize,
    data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    /// The header, with every count and offset rebuilt on write cleared.
    header: [u8; COMMANDS],
    commands: Vec<u8>,
    pub(super) nodes: Vec<Node>,
    blackboard: Vec<u8>,
    attachments: Attachments,
    /// Each type's property entries.
    pub(super) properties: [Vec<u8>; 6],
    pub(super) inputs: [Vec<u8>; 6],
    pub(super) outputs: [Vec<u8>; 6],
    pub(super) multi: Vec<u8>,
    jumps: Vec<Vec<u8>>,
    pub(super) queries: Vec<u8>,
    exb: Option<Vec<u8>>,
    /// Modules, actions, the blackboard ids, the replacement table, an
    /// unknown word and the enum relocations, in file order.
    rest: [Vec<u8>; 6],
    pool: Vec<u8>,
}

impl File {
    /// The node the graph's first command starts at: its root.
    pub fn root(&self) -> Result<usize> {
        ensure!(
            self.commands.len() >= COMMAND_LEN,
            "a graph without a command"
        );
        Ok(u16::from_le_bytes([self.commands[0x14], self.commands[0x15]]) as usize)
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let r = Reader(data);
        check_header(&r)?;
        let slot_at = |s: usize| slot_at(&r, s);
        ensure!(slot_at(slot::ZERO)? == 0, "an unknown header word set");
        ensure!(slot_at(slot::OLD)? == 0, "a section only older games use");
        ensure!(
            slot_at(slot::JUMPS_AGAIN)? == slot_at(slot::JUMPS)?,
            "two jump tables"
        );
        ensure!(slot_at(slot::REPLACEMENTS)? != 0, "no replacement table");

        let commands = COMMANDS..COMMANDS + COMMAND_LEN * slot_at(slot::COMMANDS)?;
        let blackboard = slot_at(slot::BLACKBOARD)?;
        let (nodes, first_body) = nodes(&r, commands.end, blackboard)?;
        let blackboard_len =
            (first_body.checked_sub(blackboard)).context("a node body before the blackboard")?;

        let properties = slot_at(slot::PROPERTIES)?;
        let io = slot_at(slot::IO)?;
        let late = slot::LATE
            .iter()
            .map(|&s| slot_at(s))
            .collect::<Result<Vec<_>>>()?;
        let props = sections(&r, properties, TYPES.len(), io, "property")?;
        let io = sections(&r, io, 2 * TYPES.len(), late[0], "input/output")?;
        let (mut inputs, mut outputs): ([Vec<u8>; 6], [Vec<u8>; 6]) = Default::default();
        for (i, v) in io.into_iter().enumerate() {
            match i % 2 {
                0 => inputs[i / 2] = v,
                _ => outputs[i / 2] = v,
            }
        }
        let Late {
            multi,
            jumps,
            queries,
            exb,
            rest,
            pool,
        } = Late::parse(&r, &late)?;

        let mut header: [u8; COMMANDS] = r.bytes(0, COMMANDS)?.try_into()?;
        for s in slot::DERIVED {
            header[s..s + 4].fill(0);
        }
        let file = Self {
            header,
            commands: r.bytes(commands.start, commands.len())?.to_vec(),
            nodes,
            blackboard: r.bytes(blackboard, blackboard_len)?.to_vec(),
            attachments: Attachments::parse(&r, properties)?,
            properties: props
                .try_into()
                .map_err(|_| anyhow::anyhow!("property table"))?,
            inputs,
            outputs,
            multi,
            jumps,
            queries,
            exb,
            rest,
            pool,
        };
        for (t, ty) in TYPES.iter().enumerate() {
            let fits = |v: &Vec<u8>, len: usize| v.len().is_multiple_of(len);
            ensure!(
                fits(&file.properties[t], ty.property_len()),
                "{ty:?} properties"
            );
            ensure!(fits(&file.inputs[t], ty.input_len()), "{ty:?} inputs");
            ensure!(fits(&file.outputs[t], ty.output_len()), "{ty:?} outputs");
        }
        Ok(file)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if let Some((n, name)) = self.unset() {
            anyhow::bail!("node {n}'s child plug {name:?} points nowhere");
        }
        let mut out = self.header.to_vec();
        let queries = self.nodes.iter().filter(|n| n.is_query()).count();
        put(&mut out, slot::COMMANDS, self.commands.len() / COMMAND_LEN);
        put(&mut out, slot::NODES, self.nodes.len());
        put(&mut out, slot::QUERY_NODES, queries);
        out.extend(&self.commands);
        let entries = out.len();
        out.resize(entries + NODE_LEN * self.nodes.len(), 0);
        here(&mut out, slot::BLACKBOARD);
        out.extend(&self.blackboard);
        for (n, node) in self.nodes.iter().enumerate() {
            let at = entries + n * NODE_LEN;
            out[at..at + NODE_LEN].copy_from_slice(&node.entry);
            let body = out.len();
            put(&mut out, at + BODY, body);
            node.write(&mut out)?;
        }

        let slots = out.len();
        if let Some(was) = self.attachments.was {
            ensure!(was == slots, "can't move this graph's attachments");
        }
        put(&mut out, slot::ATTACHMENT_SLOTS, slots);
        put(
            &mut out,
            slot::ATTACHMENT_LIST,
            slots + self.attachments.list,
        );
        out.extend(&self.attachments.data);

        here(&mut out, slot::PROPERTIES);
        table(&mut out, self.properties.iter());
        here(&mut out, slot::IO);
        let io = self.inputs.iter().zip(&self.outputs);
        table(&mut out, io.flat_map(|(i, o)| [i, o]));

        here(&mut out, slot::MULTI);
        out.extend(&self.multi);
        here(&mut out, slot::JUMPS);
        here(&mut out, slot::JUMPS_AGAIN);
        table(&mut out, self.jumps.iter());
        here(&mut out, slot::QUERIES);
        out.extend(&self.queries);
        if let Some(exb) = &self.exb {
            here(&mut out, slot::EXB);
            out.extend(exb);
        }
        for (&s, data) in slot::REST.iter().zip(&self.rest) {
            here(&mut out, s);
            out.extend(data);
        }
        here(&mut out, slot::POOL);
        out.extend(&self.pool);
        Ok(out)
    }

    /// The string at `offset` in the pool.
    pub(super) fn str(&self, offset: u32) -> Result<&str> {
        let rest = self
            .pool
            .get(offset as usize..)
            .context("a string past the pool")?;
        Ok(std::str::from_utf8(
            until_nul(rest).context("an unended string")?,
        )?)
    }

    /// `s`'s offset in the pool, added to its end if it isn't there.
    pub(super) fn intern(&mut self, s: &str) -> u32 {
        super::intern(&mut self.pool, 0, s)
    }

    /// The file's own name, from its header.
    pub(super) fn name(&self) -> Result<&str> {
        let at = slot::FILENAME;
        self.str(u32::from_le_bytes(self.header[at..at + 4].try_into()?))
    }
}

/// Header slot `s`'s count or offset.
fn slot_at(r: &Reader, s: usize) -> Result<usize> {
    Ok(r.u32(s)? as usize)
}

/// The `n` words of the offset table at `at`.
fn offsets(r: &Reader, at: usize, n: usize) -> Result<Vec<usize>> {
    (0..n).map(|i| Ok(r.u32(at + 4 * i)? as usize)).collect()
}

/// The node entries starting at `entries`, which the blackboard follows,
/// each with its body. Also gives where the first body starts.
fn nodes(r: &Reader, entries: usize, blackboard: usize) -> Result<(Vec<Node>, usize)> {
    let count = slot_at(r, slot::NODES)?;
    ensure!(
        blackboard == entries + NODE_LEN * count,
        "the blackboard isn't after the nodes"
    );
    let bodies = (0..count)
        .map(|n| r.u32(entries + n * NODE_LEN + BODY).map(|b| b as usize))
        .chain([slot_at(r, slot::ATTACHMENT_SLOTS)])
        .collect::<Result<Vec<_>>>()?;
    let mut nodes = Vec::with_capacity(count);
    for n in 0..count {
        let at = entries + n * NODE_LEN;
        let mut entry: [u8; NODE_LEN] = r.bytes(at, NODE_LEN)?.try_into()?;
        ensure!(r.u16(at + 2)? as usize == n, "node {n} has another index");
        ensure!(r.u32(at + STATE)? == 0, "node {n} has a state");
        entry[BODY..BODY + 4].fill(0);
        let (params, plugs) = Node::parse(r, bodies[n], bodies[n + 1])?;
        nodes.push(Node {
            entry,
            params,
            plugs,
        });
    }
    Ok((nodes, bodies[0]))
}

/// The `n` parts of the `what` table at `at`, the last running to `end`.
/// Its offsets come first, right before the parts.
fn sections(r: &Reader, at: usize, n: usize, end: usize, what: &str) -> Result<Vec<Vec<u8>>> {
    let mut starts = offsets(r, at, n)?;
    ensure!(starts[0] == at + 4 * n, "the {what} table has a gap");
    starts.push(end);
    slices(r, &starts)
}

impl Attachments {
    /// The section from the attachment slots to the property table at
    /// `properties`.
    fn parse(r: &Reader, properties: usize) -> Result<Self> {
        let slots = slot_at(r, slot::ATTACHMENT_SLOTS)?;
        let list = slot_at(r, slot::ATTACHMENT_LIST)?;
        ensure!(
            (slots..=properties).contains(&list),
            "attachments out of order"
        );
        let data = r.bytes(slots, properties - slots)?.to_vec();
        Ok(Self {
            was: (!data.is_empty()).then_some(slots),
            list: list - slots,
            data,
        })
    }
}

/// The sections after the input/output table.
struct Late {
    multi: Vec<u8>,
    jumps: Vec<Vec<u8>>,
    queries: Vec<u8>,
    exb: Option<Vec<u8>>,
    rest: [Vec<u8>; 6],
    pool: Vec<u8>,
}

impl Late {
    /// The sections at `late`, the offsets of `slot::LATE`.
    fn parse(r: &Reader, late: &[usize]) -> Result<Self> {
        let mut starts: Vec<usize> = late.iter().copied().filter(|&s| s != 0).collect();
        starts.push(r.0.len());
        let mut late = slices(r, &starts)?.into_iter();
        let mut next = || late.next().context("a section missing");
        let multi = next()?;
        let jump_table = next()?;
        let queries = next()?;
        let exb = match slot_at(r, slot::EXB)? {
            0 => None,
            _ => Some(next()?),
        };
        let rest = [next()?, next()?, next()?, next()?, next()?, next()?];
        let pool = next()?;
        ensure!(rest[5] == [0; 4], "enum relocations");

        let jumps = match jump_table.is_empty() {
            true => Vec::new(),
            false => {
                let jumps_at = slot_at(r, slot::JUMPS)?;
                let first = r.u32(jumps_at)? as usize;
                let len = (first.checked_sub(jumps_at)).context("a jump before its table")?;
                let mut starts = offsets(r, jumps_at, len / 4)?;
                starts.push(jumps_at + jump_table.len());
                slices(r, &starts)?
            }
        };
        Ok(Self {
            multi,
            jumps,
            queries,
            exb,
            rest,
            pool,
        })
    }
}

/// The bytes between each start and the next, the last start ending them.
fn slices(r: &Reader, starts: &[usize]) -> Result<Vec<Vec<u8>>> {
    starts
        .windows(2)
        .map(|w| {
            ensure!(w[0] <= w[1], "sections out of order at {:#x}", w[0]);
            Ok(r.bytes(w[0], w[1] - w[0])?.to_vec())
        })
        .collect()
}

fn put(out: &mut [u8], at: usize, v: usize) {
    out[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
}

/// Points header slot `s` at the end of `out`.
fn here(out: &mut [u8], s: usize) {
    let at = out.len();
    put(out, s, at);
}

/// A table of absolute offsets, one per part, then the parts.
fn table<'a>(out: &mut Vec<u8>, parts: impl Iterator<Item = &'a Vec<u8>> + Clone) {
    let mut next = out.len() + 4 * parts.clone().count();
    for part in parts.clone() {
        out.extend((next as u32).to_le_bytes());
        next += part.len();
    }
    out.extend(parts.flatten());
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::ainb::{MAGIC, ParamType};

    /// An input's type, name, and the node and output feeding it.
    pub type FakeInput = (ParamType, &'static str, Option<(i16, i16)>);

    /// A made-up node: its kind, flags (1 for a query), class name,
    /// properties, inputs (unlinked, or fed by a node's output), outputs,
    /// plugs (kind, target, name, length) and query places.
    #[derive(Default)]
    pub struct Fake {
        pub kind: u16,
        pub flags: u8,
        pub name: &'static str,
        pub props: Vec<(ParamType, &'static str)>,
        pub inputs: Vec<FakeInput>,
        pub outputs: Vec<(ParamType, &'static str)>,
        pub plugs: Vec<(usize, i32, &'static str, usize)>,
        pub queries: Vec<u16>,
    }

    pub fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A graph of `nodes` named `FakeGraph`, every other section empty.
    pub fn fake(nodes: Vec<Fake>) -> File {
        let mut header = [0; COMMANDS];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&0x408u32.to_le_bytes());
        let mut f = File {
            header,
            commands: Vec::new(),
            nodes: Vec::new(),
            blackboard: vec![0; 0x38],
            attachments: Attachments {
                was: None,
                list: 0,
                data: Vec::new(),
            },
            properties: Default::default(),
            inputs: Default::default(),
            outputs: Default::default(),
            multi: Vec::new(),
            jumps: Vec::new(),
            queries: Vec::new(),
            exb: None,
            rest: [
                vec![0; 4],
                vec![0; 4],
                vec![0; 8],
                le(&[0, u32::MAX]),
                vec![0; 4],
                vec![0; 4],
            ],
            pool: Vec::new(),
        };
        let name = f.intern("FakeGraph");
        f.header[slot::FILENAME..slot::FILENAME + 4].copy_from_slice(&name.to_le_bytes());
        for (n, fake) in nodes.into_iter().enumerate() {
            let mut entry = [0; NODE_LEN];
            entry[..2].copy_from_slice(&fake.kind.to_le_bytes());
            entry[2..4].copy_from_slice(&(n as u16).to_le_bytes());
            entry[6] = fake.flags;
            entry[8..12].copy_from_slice(&f.intern(fake.name).to_le_bytes());
            let base = (f.queries.len() / 4) as u16;
            let base = if fake.queries.is_empty() { 0 } else { base };
            entry[0x24..0x26].copy_from_slice(&base.to_le_bytes());
            entry[0x26..0x28].copy_from_slice(&(fake.queries.len() as u16).to_le_bytes());
            for q in &fake.queries {
                f.queries.extend(le(&[*q as u32]));
            }
            let mut node = Node {
                entry,
                params: [0; PARAMS],
                plugs: Default::default(),
            };
            for (t, &ty) in TYPES.iter().enumerate() {
                let class = f.intern("FakeClass");
                node.params[2 * t] = (f.properties[t].len() / ty.property_len()) as u32;
                for (_, name) in fake.props.iter().filter(|p| p.0 == ty) {
                    let name = f.intern(name);
                    let words = match ty {
                        ParamType::Pointer => vec![name, class, 0],
                        ParamType::Vec3 => vec![name, 0, 0, 0, 0],
                        _ => vec![name, 0, 0],
                    };
                    f.properties[t].extend(le(&words));
                    node.params[2 * t + 1] += 1;
                }
                node.params[12 + 4 * t] = (f.inputs[t].len() / ty.input_len()) as u32;
                for (_, name, source) in fake.inputs.iter().filter(|i| i.0 == ty) {
                    let name = f.intern(name);
                    let (from, out) = source.unwrap_or((-1, 0));
                    let link = (from as u16 as u32) | ((out as u16 as u32) << 16);
                    let flags = if source.is_some() { 0 } else { 0x80_0000 };
                    let empty = f.intern("");
                    let words = match ty {
                        ParamType::Pointer => vec![name, class, link, flags, 0],
                        ParamType::Vec3 => vec![name, link, flags, 0, 0, 0],
                        ParamType::String => vec![name, link, flags, empty],
                        _ => vec![name, link, flags, 0],
                    };
                    f.inputs[t].extend(le(&words));
                    node.params[13 + 4 * t] += 1;
                }
                node.params[14 + 4 * t] = (f.outputs[t].len() / ty.output_len()) as u32;
                for (_, name) in fake.outputs.iter().filter(|o| o.0 == ty) {
                    let name = f.intern(name);
                    let words = match ty {
                        ParamType::Pointer => vec![name, class],
                        _ => vec![name],
                    };
                    f.outputs[t].extend(le(&words));
                    node.params[15 + 4 * t] += 1;
                }
            }
            for (kind, target, name, len) in fake.plugs {
                let mut record = le(&[target as u32, f.intern(name)]);
                record.resize(len, 0);
                node.plugs[kind].push(record);
            }
            f.nodes.push(node);
        }
        f
    }

    #[test]
    fn a_fake_graph_comes_back_the_same() {
        let f = fake(vec![
            Fake {
                kind: 3,
                name: "",
                plugs: vec![(2, 1, "", 8)],
                ..Default::default()
            },
            Fake {
                flags: 1,
                name: "QueryFake",
                props: vec![(ParamType::Bool, "OutputOnlyOnEnter")],
                inputs: vec![(ParamType::String, "Name", None)],
                outputs: vec![(ParamType::Bool, "Value"), (ParamType::Pointer, "Actor")],
                ..Default::default()
            },
        ]);
        let bytes = f.to_bytes().unwrap();
        let back = File::parse(&bytes).unwrap();
        assert_eq!(back, f);
        assert_eq!(back.to_bytes().unwrap(), bytes);
        assert_eq!(back.name().unwrap(), "FakeGraph");
        let g = crate::ainb::Graph::parse(&bytes).unwrap();
        assert_eq!(g.nodes[1].name, "QueryFake");
        assert_eq!(g.nodes[1].outputs.len(), 2);
    }

    #[test]
    fn the_root_is_where_the_first_command_starts() {
        let mut f = fake(vec![Fake::default(), Fake::default()]);
        assert!(f.root().is_err());
        let mut command = vec![0; COMMAND_LEN];
        command[0x14] = 1;
        f.commands = command;
        let back = File::parse(&f.to_bytes().unwrap()).unwrap();
        assert_eq!(back.root().unwrap(), 1);
    }

    #[test]
    fn an_empty_string_lands_in_the_pool() {
        let mut f = fake(vec![]);
        let empty = f.intern("");
        let next = f.intern("Fake");
        assert_eq!(f.str(empty).unwrap(), "");
        assert_eq!(f.intern(""), empty);
        assert_eq!(f.str(next).unwrap(), "Fake");
    }

    #[test]
    fn refuses_what_it_cant_write() {
        let f = fake(vec![Fake::default()]);
        let bytes = f.to_bytes().unwrap();
        let set = |at: usize, v: u32| {
            let mut b = bytes.clone();
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
            File::parse(&b).map(|_| ())
        };
        assert!(set(slot::OLD, 4).is_err());
        assert!(set(slot::ZERO, 1).is_err());
        assert!(set(slot::JUMPS_AGAIN, 0).is_err());
        assert!(set(4, 0x404).is_err());
        let enums = u32::from_le_bytes(bytes[slot::ENUMS..slot::ENUMS + 4].try_into().unwrap());
        assert!(set(enums as usize, 1).is_err());
        assert!(File::parse(b"YB\x07\x00").is_err());
    }
}
