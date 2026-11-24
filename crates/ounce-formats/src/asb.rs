//! ASB animation graphs, version 0x41b: the node tree an actor's animation
//! state walks to pick each clip, choosing at each selector by a blackboard
//! value. Only what an edit of the selectors needs is read.

use std::collections::HashSet;

use anyhow::{Result, bail, ensure};

use crate::reader::Reader;

const MAGIC: &[u8] = b"ASB ";
const VERSION: u32 = 0x41b;
const NODE_SIZE: usize = 0x24;
/// A blackboard property's flag word: set when the value is a blackboard
/// entry, whose index is the low half.
const FROM_BLACKBOARD: u32 = 1 << 31;
/// The flag word the game's own `UsingItem` selectors carry.
const INT_SELECTOR_FLAGS: u32 = 0xa000_0000;

/// Node types this module reads.
const CLIP: u16 = 3;
const INT_SELECTOR: u16 = 8;
const BOOL_SELECTOR: u16 = 21;
/// Where an int selector's plug keeps the value its case matches.
const CASE_VALUE: usize = 4;

/// Where a node type's plug table sits in its params, and where each plug
/// keeps its child.
struct Plugs {
    table: usize,
    child: usize,
}

/// A blackboard entry's kind, in the order the blackboard lists them.
#[derive(Clone, Copy)]
pub enum Kind {
    Int = 1,
    Bool = 4,
}

/// A node's part in the tree.
#[derive(Debug, PartialEq)]
pub enum Node {
    /// Plays the clip named.
    Clip(String),
    /// Picks its first child when the bool blackboard entry is true, its
    /// second when false.
    Bool {
        entry: Option<u16>,
        children: Vec<u32>,
    },
    /// Picks the child whose value the int blackboard entry equals.
    Int {
        entry: Option<u16>,
        children: Vec<(i32, u32)>,
    },
    /// Any other node, by its children.
    Other(Vec<u32>),
}

pub struct Asb<'a> {
    r: Reader<'a>,
    pool: usize,
    nodes: usize,
    count: usize,
    blackboard: usize,
}

impl<'a> Asb<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let r = Reader(bytes);
        ensure!(r.bytes(0, 4)? == MAGIC, "not an ASB");
        ensure!(r.u32(4)? == VERSION, "ASB version {:#x}", r.u32(4)?);
        Ok(Self {
            pool: r.u32(0x3c)? as usize,
            count: r.u32(0x14)? as usize,
            nodes: r.u32(0x18)? as usize,
            blackboard: r.u32(0x38)? as usize,
            r,
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn string(&self, at: u32) -> Result<&'a str> {
        let rest = &self.r.0[(self.pool + at as usize).min(self.r.0.len())..];
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        Ok(std::str::from_utf8(&rest[..end])?)
    }

    /// The index of the blackboard entry of `kind` named `name`.
    pub fn entry(&self, kind: Kind, name: &str) -> Result<Option<u16>> {
        let at = self.blackboard + kind as usize * 8;
        let (count, base) = (self.r.u16(at)?, self.r.u16(at + 2)?);
        for j in 0..count {
            let named = self
                .r
                .u32(self.blackboard + 0x38 + (base as usize + j as usize) * 4)?;
            if self.string(named & 0xff_ffff)? == name {
                return Ok(Some(j));
            }
        }
        Ok(None)
    }

    fn header(&self, i: usize) -> Result<(u16, usize)> {
        ensure!(i < self.count, "node {i} of {}", self.count);
        let at = self.nodes + i * NODE_SIZE;
        Ok((self.r.u16(at)?, self.r.u32(at + 8)? as usize))
    }

    /// The blackboard entry a selector's property reads, if it reads one.
    fn reads(&self, at: usize) -> Result<Option<u16>> {
        let flags = self.r.u32(at)?;
        Ok((flags & FROM_BLACKBOARD != 0 && flags & 0x0300_0000 == 0).then_some(flags as u16))
    }

    /// Node type `kind`'s plug layout. A type left out has no children
    /// this module follows.
    fn plugs(kind: u16) -> Option<Plugs> {
        let (table, child) = match kind {
            4 => (0, 0),
            9 | 10 => (0x8, 0),
            6 => (0xc, 0x10),
            1 => (0x14, 0x10),
            2 | INT_SELECTOR => (0x14, 0x8),
            BOOL_SELECTOR => (0x14, 0),
            7 => (0x18, 0),
            20 => (0x1c, 0),
            CLIP => (0x20, 0),
            16 => (0x4, 0x4),
            _ => return None,
        };
        Some(Plugs { table, child })
    }

    /// Each plug of node `i`: where it sits, and its child.
    fn children(&self, i: usize) -> Result<Vec<(usize, u32)>> {
        let (kind, params) = self.header(i)?;
        let Some(Plugs { table, child }) = Self::plugs(kind) else {
            return Ok(vec![]);
        };
        let table = params + table;
        let (count, base) = (self.r.u8(table + 4)?, self.r.u8(table + 5)?);
        (0..count as usize)
            .map(|k| {
                let plug = self.r.u32(table + 0xc + 4 * (base as usize + k))? as usize;
                Ok((plug, self.r.u32(plug + child)?))
            })
            .collect()
    }

    pub fn node(&self, i: usize) -> Result<Node> {
        let (kind, params) = self.header(i)?;
        let children = self.children(i)?;
        Ok(match kind {
            CLIP if self.r.u32(params)? & FROM_BLACKBOARD == 0 => {
                Node::Clip(self.string(self.r.u32(params + 4)?)?.to_string())
            }
            BOOL_SELECTOR => Node::Bool {
                entry: self.reads(params)?,
                children: children.into_iter().map(|(_, c)| c).collect(),
            },
            INT_SELECTOR => Node::Int {
                entry: self.reads(params)?,
                children: (children.into_iter())
                    .map(|(plug, c)| Ok((self.r.i32(plug + CASE_VALUE)?, c)))
                    .collect::<Result<_>>()?,
            },
            _ => Node::Other(children.into_iter().map(|(_, c)| c).collect()),
        })
    }

    /// Every clip node `i` can reach.
    pub fn clips(&self, i: u32) -> Result<Vec<String>> {
        let (mut found, mut seen, mut todo) = (vec![], HashSet::new(), vec![i]);
        while let Some(i) = todo.pop() {
            if !seen.insert(i) || i as usize >= self.count {
                continue;
            }
            match self.node(i as usize)? {
                Node::Clip(name) => found.push(name),
                Node::Bool { children, .. } | Node::Other(children) => todo.extend(children),
                Node::Int { children, .. } => todo.extend(children.into_iter().map(|(_, c)| c)),
            }
        }
        Ok(found)
    }
}

/// Make node `i` of the graph in `bytes` an int selector on blackboard
/// entry `entry`: each case's child when the entry equals its value, else
/// `otherwise`. Its new params go at the end; the old ones stay, unread.
pub fn select_int(
    bytes: &mut Vec<u8>,
    i: usize,
    entry: u16,
    cases: &[(i32, u32)],
    otherwise: u32,
) -> Result<()> {
    let asb = Asb::parse(bytes)?;
    // Only checks that node `i` is there.
    asb.header(i)?;
    let node = asb.nodes + i * NODE_SIZE;
    let layout = Asb::plugs(INT_SELECTOR).expect("an int selector has plugs");
    let Ok(count) = u8::try_from(cases.len() + 1) else {
        bail!("{} cases", cases.len())
    };
    bytes.resize(bytes.len().next_multiple_of(8), 0);
    let params = bytes.len();
    let table = params + layout.table;
    let plugs = table + 0xc + 4 * count as usize;
    let put = |b: &mut Vec<u8>, words: &[u32]| {
        for w in words {
            b.extend(w.to_le_bytes());
        }
    };
    // The selector, then 1: the last plug is the default.
    put(bytes, &[INT_SELECTOR_FLAGS | entry as u32, 0, 0, 1, 0]);
    debug_assert_eq!(bytes.len(), table);
    // The plug table: its count and first index, then its pointer counts.
    let header = u32::from_le_bytes([count, 0, 0, count]);
    let totals = u32::from_le_bytes([count, 0, count, 0]);
    put(bytes, &[0, header, totals]);
    let plug = |k: usize| (plugs + 12 * k) as u32;
    put(bytes, &(0..count as usize).map(plug).collect::<Vec<_>>());
    // Each plug: a word, the case's value, then its child.
    debug_assert_eq!((CASE_VALUE, layout.child), (4, 8));
    for &(value, child) in cases.iter().chain([&(0, otherwise)]) {
        put(bytes, &[0, value as u32, child]);
    }
    bytes[node..node + 2].copy_from_slice(&INT_SELECTOR.to_le_bytes());
    bytes[node + 8..node + 12].copy_from_slice(&(params as u32).to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph of `n` blank nodes, right after a 0x40-byte header.
    fn graph(n: usize) -> Vec<u8> {
        let mut bytes = vec![0; 0x40 + n * NODE_SIZE];
        bytes[..4].copy_from_slice(MAGIC);
        bytes[4..8].copy_from_slice(&VERSION.to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&(n as u32).to_le_bytes());
        bytes[0x18..0x1c].copy_from_slice(&0x40u32.to_le_bytes());
        bytes
    }

    #[test]
    fn a_node_made_an_int_selector_reads_back_its_cases_and_default() {
        let mut bytes = graph(2);
        select_int(&mut bytes, 1, 2, &[(5, 7), (-1, 8)], 3).unwrap();
        let asb = Asb::parse(&bytes).unwrap();
        assert_eq!(
            asb.node(1).unwrap(),
            Node::Int {
                entry: Some(2),
                children: vec![(5, 7), (-1, 8), (0, 3)],
            }
        );
        assert_eq!(asb.node(0).unwrap(), Node::Other(vec![]));
        assert!(select_int(&mut bytes, 2, 2, &[], 3).is_err());
    }
}
