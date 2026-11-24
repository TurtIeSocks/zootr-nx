//! `Flowchart::to_bytes`, laid out the way the game's own files are:
//!
//! 1. Header, the flowchart and timeline dictionaries, the flowchart block,
//!    then the actor, event, entry point dictionary and entry point tables.
//! 2. Each event's parameters, switch cases or fork branches, in event order.
//! 3. Each actor's parameters, action names and query names.
//! 4. Each entry point's sub-flow events and variables.
//! 5. The string pool, then the relocation table listing every pointer.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::{
    ACTOR_LEN, Actor, ENTRY_MAIN, ENTRY_POINT_LEN, EVENT_LEN, EntryPoint, Event, EventKind,
    Flowchart, MAGIC, NONE, Params, VERSION, Value, ev,
};

const HEADER_LEN: usize = 0x48;
const FLOWCHART_LEN: usize = 0x48;
/// A relocation entry covers its pointer and the next 31 words.
const RELOC_SPAN: usize = 32;

#[derive(Default)]
struct Out {
    buf: Vec<u8>,
    /// Pointer fields, for the relocation table.
    relocs: Vec<usize>,
    /// Pointer fields aimed at a pool string.
    names: Vec<(usize, String)>,
}

impl Out {
    fn pos(&self) -> usize {
        self.buf.len()
    }
    fn align(&mut self, n: usize) {
        self.buf.resize(self.pos().next_multiple_of(n), 0);
    }
    fn reserve(&mut self, n: usize) -> usize {
        let at = self.pos();
        self.buf.resize(at + n, 0);
        at
    }
    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.buf[at..at + bytes.len()].copy_from_slice(bytes);
    }
    fn u16(&mut self, at: usize, v: u16) {
        self.put(at, &v.to_le_bytes());
    }
    fn u32(&mut self, at: usize, v: u32) {
        self.put(at, &v.to_le_bytes());
    }
    fn index(&mut self, at: usize, v: Option<u16>) {
        self.u16(at, v.unwrap_or(NONE));
    }
    /// A pointer field the relocation table lists, even while it's null.
    fn null(&mut self, at: usize) {
        self.relocs.push(at);
    }
    fn ptr(&mut self, at: usize, to: usize) {
        self.put(at, &(to as u64).to_le_bytes());
        self.null(at);
    }
    /// Point the field at `at` to whatever is written next.
    fn here(&mut self, at: usize) {
        self.ptr(at, self.pos());
    }
    /// `here`, or null when there's nothing to point at.
    fn here_unless(&mut self, at: usize, empty: bool) {
        match empty {
            true => self.null(at),
            false => self.here(at),
        }
    }
    fn name(&mut self, at: usize, s: &str) {
        self.names.push((at, s.to_owned()));
        self.null(at);
    }
    /// A string right here, for a value's pointer at `at`.
    fn inline(&mut self, at: usize, s: &str) {
        self.align(2);
        self.here(at);
        self.buf.extend((s.len() as u16).to_le_bytes());
        self.buf.extend(s.as_bytes());
        self.buf.push(0);
    }
    /// A pointer array to pool strings.
    fn names(&mut self, field: usize, list: &[String]) {
        self.here_unless(field, list.is_empty());
        let at = self.reserve(list.len() * 8);
        for (i, s) in list.iter().enumerate() {
            self.name(at + i * 8, s);
        }
    }

    fn dic<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        let names: Vec<&str> = names.into_iter().collect();
        let at = self.reserve(8 + (names.len() + 1) * 16);
        self.put(at, b"DIC ");
        self.u32(at + 4, names.len() as u32);
        for (i, node) in dic_tree(&names).into_iter().enumerate() {
            let e = at + 8 + i * 16;
            self.u32(e, node.bit);
            self.u16(e + 4, node.left);
            self.u16(e + 6, node.right);
            self.name(e + 8, if i == 0 { "" } else { names[i - 1] });
        }
    }

    /// Parameters for the pointer at `field`; none leaves it null.
    fn params(&mut self, field: usize, params: &Params) {
        if !params.is_empty() {
            self.align(8);
            self.here(field);
            self.container(params);
        }
    }

    fn container(&mut self, params: &Params) {
        let at = self.header(1, params.len(), params.len() * 8);
        self.here(at + 8);
        self.dic(params.iter().map(|(k, _)| k.as_str()));
        for (i, (_, v)) in params.iter().enumerate() {
            self.align(8);
            self.here(at + 0x10 + i * 8);
            self.value(v);
        }
    }

    /// A value's 0x10 header plus `data` bytes; returns where it starts.
    fn header(&mut self, kind: u8, n: usize, data: usize) -> usize {
        let at = self.reserve(0x10 + data);
        self.buf[at] = kind;
        self.u16(at + 2, n as u16);
        at
    }

    fn value(&mut self, v: &Value) {
        // Single values and arrays share a variant: one item is a single.
        // So a one-item array reads back as a single, and writes the same.
        let kind = |single: u8, n: usize| if n == 1 { single } else { single + 5 };
        let words = |o: &mut Self, single, w: Vec<u32>| {
            let at = o.header(kind(single, w.len()), w.len(), w.len() * 4);
            for (i, w) in w.into_iter().enumerate() {
                o.u32(at + 0x10 + i * 4, w);
            }
        };
        match v {
            Value::Container(p) => self.container(p),
            Value::Int(v) => words(self, 2, v.iter().map(|&i| i as u32).collect()),
            // True is stored as 0x80000001.
            Value::Bool(v) => words(
                self,
                3,
                v.iter().map(|&b| u32::from(b) * 0x8000_0001).collect(),
            ),
            Value::Float(v) => words(self, 4, v.iter().map(|f| f.to_bits()).collect()),
            Value::Argument(s) => self.strings(0, 1, &[s]),
            Value::String(v) => self.strings(
                kind(5, v.len()),
                v.len(),
                &v.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            Value::ActorId { name, secondary } => self.strings(12, 2, &[name, secondary]),
        }
    }

    fn strings(&mut self, kind: u8, n: usize, list: &[&str]) {
        let at = self.header(kind, n, list.len() * 8);
        for (i, s) in list.iter().enumerate() {
            self.inline(at + 0x10 + i * 8, s);
        }
    }
}

struct Node {
    bit: u32,
    left: u16,
    right: u16,
}

/// Bit `i` of a `DIC ` key: bit `i % 8` of its `i / 8`th byte from the end,
/// and zero past the start.
fn bit(s: &[u8], i: u32) -> bool {
    let byte = (i / 8) as usize;
    byte < s.len() && s[s.len() - 1 - byte] >> (i % 8) & 1 == 1
}

/// The `DIC ` radix tree over `names`, root first.
///
/// # Panics
///
/// On a duplicate or empty name: the tree can't tell it apart.
fn dic_tree(names: &[&str]) -> Vec<Node> {
    // (bit, children, name); the root's bit is -1 so it sorts first.
    let mut nodes: Vec<(i64, [usize; 2], &[u8])> = vec![(-1, [0, 0], b"")];
    for (n, name) in (1..).zip(names) {
        let key = name.as_bytes();
        let side = |at: &(i64, [usize; 2], &[u8])| at.0 >= 0 && bit(key, at.0 as u32);
        let descend = |stop: i64| {
            let (mut p, mut x) = (0, nodes[0].1[0]);
            while nodes[p].0 < nodes[x].0 && nodes[x].0 < stop {
                p = x;
                x = nodes[x].1[side(&nodes[x]) as usize];
            }
            (p, x)
        };
        let closest = nodes[descend(i64::MAX).1].2;
        let bits = 8 * key.len().max(closest.len()) as u32;
        let diff = (0..bits)
            .find(|&i| bit(key, i) != bit(closest, i))
            .unwrap_or_else(|| panic!("duplicate or empty DIC name {name:?}"));
        let (p, x) = descend(diff.into());
        let children = if bit(key, diff) { [x, n] } else { [n, x] };
        nodes.push((diff.into(), children, key));
        let s = side(&nodes[p]) as usize;
        nodes[p].1[s] = n;
    }
    nodes
        .into_iter()
        .map(|(bit, [left, right], _)| Node {
            bit: bit as u32,
            left: left as u16,
            right: right as u16,
        })
        .collect()
}

impl Flowchart {
    /// The file `parse` read, with any edits made since.
    ///
    /// # Panics
    ///
    /// When a parameter container or the entry points hold a duplicate or
    /// empty name.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Out::default();
        let h = o.reserve(HEADER_LEN);
        o.put(h, MAGIC);
        o.put(h + 8, &VERSION);
        // Byte order mark, then 2^3 alignment.
        o.put(h + 0xC, &[0xFF, 0xFE, 3, 0]);
        o.u16(h + 0x20, 1);
        o.here(h + 0x28);
        let charts = o.reserve(8);
        o.here(h + 0x30);
        o.dic([self.name.as_str()]);
        o.null(h + 0x38);
        o.here(h + 0x40);
        o.dic([]);

        o.here(charts);
        let fc = o.reserve(FLOWCHART_LEN);
        o.u16(h + 0x16, fc as u16);
        o.put(fc, b"EVFL");
        let counts = [
            self.actors.len(),
            self.actors.iter().map(|a| a.actions.len()).sum(),
            self.actors.iter().map(|a| a.queries.len()).sum(),
            self.events.len(),
            self.entry_points.len(),
        ];
        for (i, n) in counts.into_iter().enumerate() {
            o.u16(fc + 0x10 + i * 2, n as u16);
        }
        o.name(fc + 0x20, &self.name);
        o.here_unless(fc + 0x28, self.actors.is_empty());
        let actors = o.reserve(self.actors.len() * ACTOR_LEN);
        o.here_unless(fc + 0x30, self.events.is_empty());
        let events = o.reserve(self.events.len() * EVENT_LEN);
        o.here(fc + 0x38);
        o.dic(self.entry_points.iter().map(|e| e.name.as_str()));
        o.here_unless(fc + 0x40, self.entry_points.is_empty());
        let entry_points = o.reserve(self.entry_points.len() * ENTRY_POINT_LEN);

        for (i, e) in self.events.iter().enumerate() {
            o.event(events + i * EVENT_LEN, e);
        }
        for (i, a) in self.actors.iter().enumerate() {
            o.actor(actors + i * ACTOR_LEN, a);
        }
        for (i, ep) in self.entry_points.iter().enumerate() {
            o.entry_point(
                entry_points + i * ENTRY_POINT_LEN,
                ep,
                &self.calls_from(ep.main),
            );
        }

        o.align(8);
        o.u32(fc + 4, (o.pos() - fc) as u32);
        o.pool(&self.pool);
        // The file name field points past the length, at the characters.
        let name = u64::from_le_bytes(o.buf[fc + 0x20..fc + 0x28].try_into().unwrap());
        o.u32(h + 0x10, name as u32 + 2);
        o.align(2);
        let end = o.pos();
        o.align(8);
        o.u32(h + 0x18, o.pos() as u32);
        o.relocations(end);
        o.u32(h + 0x1C, o.pos() as u32);
        o.buf
    }
}

impl Out {
    fn actor(&mut self, at: usize, a: &Actor) {
        self.name(at, &a.name);
        self.name(at + 8, &a.secondary_name);
        self.name(at + 0x10, &a.argument);
        self.u16(at + 0x30, a.actions.len() as u16);
        self.u16(at + 0x32, a.queries.len() as u16);
        self.index(at + 0x34, a.argument_entry);
        self.u16(at + 0x36, 1);
        self.params(at + 0x28, &a.params);
        self.align(8);
        self.names(at + 0x18, &a.actions);
        self.names(at + 0x20, &a.queries);
    }

    /// `ep`, listing `calls`, the calls it reaches.
    fn entry_point(&mut self, at: usize, ep: &EntryPoint, calls: &[u16]) {
        self.u16(at + 0x18, calls.len() as u16);
        self.u16(at + 0x1A, ep.vars.len() as u16);
        self.index(at + ENTRY_MAIN, ep.main);
        self.align(8);
        self.here_unless(at, calls.is_empty());
        for &e in calls {
            self.buf.extend(e.to_le_bytes());
        }
        self.align(8);
        if ep.vars.is_empty() {
            // The game's own files leave room here the size of an empty DIC.
            self.null(at + 0x10);
            self.reserve(0x18);
        } else {
            self.here(at + 8);
            self.dic(ep.vars.iter().map(|(n, _)| n.as_str()));
            self.here(at + 0x10);
            for (_, raw) in &ep.vars {
                self.buf.extend(raw);
            }
        }
    }

    fn event(&mut self, at: usize, e: &Event) {
        self.name(at, &e.name);
        let kind = match &e.kind {
            EventKind::Action {
                next,
                actor,
                action,
                params,
            } => {
                self.index(at + ev::NEXT, *next);
                self.u16(at + ev::ACTOR, *actor);
                self.u16(at + ev::CALL, *action);
                self.params(at + ev::PARAMS, params);
                ev::ACTION
            }
            EventKind::Switch {
                actor,
                query,
                params,
                cases,
            } => {
                self.u16(at + ev::ACTOR, *actor);
                self.u16(at + ev::CALL, *query);
                self.cases(at, cases);
                self.params(at + ev::PARAMS, params);
                ev::SWITCH
            }
            EventKind::Fork { join, branches } => {
                self.u16(at + ev::COUNT, branches.len() as u16);
                self.u16(at + ev::JOIN, *join);
                self.align(8);
                self.here(at + ev::BRANCHES);
                for &b in branches {
                    self.buf.extend(b.to_le_bytes());
                }
                ev::FORK
            }
            EventKind::Join { next } => {
                self.index(at + ev::NEXT, *next);
                ev::JOIN_KIND
            }
            EventKind::SubFlow {
                next,
                params,
                flowchart,
                entry_point,
            } => {
                self.index(at + ev::NEXT, *next);
                self.params(at + ev::PARAMS, params);
                self.name(at + ev::SUB_FLOWCHART, flowchart);
                self.name(at + ev::SUB_ENTRY, entry_point);
                ev::SUB_FLOW
            }
            EventKind::SubFlowSwitch {
                params,
                flowchart,
                entry_point,
                cases,
            } => {
                self.cases(at, cases);
                self.params(at + ev::PARAMS, params);
                self.name(at + ev::SWITCH_FLOWCHART, flowchart);
                self.name(at + ev::SWITCH_ENTRY, entry_point);
                ev::SUB_FLOW_SWITCH
            }
            EventKind::Return { code } => {
                self.u32(at + ev::CODE, *code);
                ev::RETURN
            }
        };
        self.buf[at + ev::KIND] = kind;
    }

    fn cases(&mut self, event: usize, cases: &[(u32, u16)]) {
        self.u16(event + ev::COUNT, cases.len() as u16);
        self.align(8);
        self.here_unless(event + ev::CASES, cases.is_empty());
        let at = self.reserve(cases.len() * 8);
        for (i, &(value, to)) in cases.iter().enumerate() {
            self.u32(at + i * 8, value);
            self.u16(at + i * 8 + 4, to);
        }
    }

    /// The pool, headed by the empty string its count leaves out, then
    /// every name pointer aimed into it.
    fn pool(&mut self, parsed: &[String]) {
        let at = self.reserve(0x14);
        self.put(at, b"STR ");
        let names = std::mem::take(&mut self.names);
        let all = std::iter::once("")
            .chain(parsed.iter().map(String::as_str))
            .chain(names.iter().map(|(_, s)| s.as_str()));
        let mut offsets = HashMap::new();
        for s in all {
            if let Entry::Vacant(slot) = offsets.entry(s) {
                self.align(2);
                slot.insert(self.pos());
                self.buf.extend((s.len() as u16).to_le_bytes());
                self.buf.extend(s.as_bytes());
                self.buf.push(0);
            }
        }
        self.u32(at + 0x10, offsets.len() as u32 - 1);
        for (field, s) in &names {
            self.put(*field, &(offsets[s.as_str()] as u64).to_le_bytes());
        }
    }

    /// The relocation table for everything before `end`.
    fn relocations(&mut self, end: usize) {
        let mut ptrs = std::mem::take(&mut self.relocs);
        ptrs.sort_unstable();
        ptrs.dedup();
        let mut entries = Vec::new();
        let mut rest = &ptrs[..];
        while let Some(&base) = rest.first() {
            let mut mask = 0u32;
            while let Some(&p) = rest.first() {
                let word = (p - base) / 8;
                if word >= RELOC_SPAN || (p - base) % 8 != 0 {
                    break;
                }
                mask |= 1 << word;
                rest = &rest[1..];
            }
            entries.push((base as u32, mask));
        }
        let relt = self.pos();
        let at = self.reserve(0x28 + entries.len() * 8);
        self.put(at, b"RELT");
        self.u32(at + 4, relt as u32);
        self.u32(at + 8, 1);
        self.u32(at + 0x1C, end as u32);
        self.u32(at + 0x24, entries.len() as u32);
        for (i, (base, mask)) in entries.into_iter().enumerate() {
            self.u32(at + 0x28 + i * 8, base);
            self.u32(at + 0x2C + i * 8, mask);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_the_tree_finds_every_name() {
        let names = ["Walk", "Talk", "A", "AB", "BA", "Event10", "Event1", "Flag"];
        let tree = dic_tree(&names);
        for (i, name) in (1..).zip(names) {
            let side = |n: &Node| bit(name.as_bytes(), n.bit) as usize;
            let (mut p, mut x) = (0, tree[0].left as usize);
            // The root's bit is u32::MAX, so compare as signed.
            while (tree[p].bit as i32) < tree[x].bit as i32 {
                p = x;
                x = [tree[x].left, tree[x].right][side(&tree[x])] as usize;
            }
            assert_eq!(x, i, "{name}");
        }
    }

    #[test]
    #[should_panic(expected = "duplicate or empty DIC name")]
    fn a_duplicate_name_panics_instead_of_spinning() {
        dic_tree(&["Walk", "Walk"]);
    }

    #[test]
    #[should_panic(expected = "duplicate or empty DIC name")]
    fn an_empty_name_panics_instead_of_spinning() {
        dic_tree(&[""]);
    }
}
