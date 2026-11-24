//! BFEVFL event flows (`Event/EventFlow/*.bfevfl.zs`). Read, written back
//! byte for byte (`write.rs`), and relinked in place.
//!
//! The header says version 4. The layout is otherwise Breath of the Wild's
//! version 3, with 0x30-byte events and 0x20-byte entry points. Every
//! pointer is an absolute u64 file offset, and every string is a u16 length
//! followed by the bytes. Timelines are skipped.
//!
//! Strings live in a pool, except a parameter's string values, which sit
//! right after the value.
//!
//! Two event types are new since BotW: a sub-flow call that branches on the
//! callee's return code (5), and the return itself (6).

use anyhow::{Context, Result, bail, ensure};

use crate::reader::Reader;

mod write;

const MAGIC: &[u8; 8] = b"BFEVFL\0\0";
const VERSION: [u8; 4] = [0, 4, 0, 0];
const ACTOR_LEN: usize = 0x38;
const EVENT_LEN: usize = 0x30;
const ENTRY_POINT_LEN: usize = 0x20;
const VAR_LEN: usize = 0x10;
const NONE: u16 = 0xFFFF;
/// How deep containers may nest, so a container that holds itself fails
/// to parse instead of overflowing the stack.
const MAX_DEPTH: usize = 32;

/// Field offsets from an event's start, and its type codes.
mod ev {
    pub(super) const KIND: usize = 8;
    /// An action's, join's or sub-flow's next event, and a switch's or
    /// fork's case or branch count.
    pub(super) const NEXT: usize = 0xA;
    pub(super) const COUNT: usize = 0xA;
    /// An action's or switch's actor, a fork's join, a return's code.
    pub(super) const ACTOR: usize = 0xC;
    pub(super) const JOIN: usize = 0xC;
    pub(super) const CODE: usize = 0xC;
    /// An action's action or a switch's query.
    pub(super) const CALL: usize = 0xE;
    /// Parameters, or a fork's branch list.
    pub(super) const PARAMS: usize = 0x10;
    pub(super) const BRANCHES: usize = 0x10;
    pub(super) const CASES: usize = 0x18;
    pub(super) const SUB_FLOWCHART: usize = 0x18;
    pub(super) const SUB_ENTRY: usize = 0x20;
    pub(super) const SWITCH_FLOWCHART: usize = 0x20;
    pub(super) const SWITCH_ENTRY: usize = 0x28;

    pub(super) const ACTION: u8 = 0;
    pub(super) const SWITCH: u8 = 1;
    pub(super) const FORK: u8 = 2;
    pub(super) const JOIN_KIND: u8 = 3;
    pub(super) const SUB_FLOW: u8 = 4;
    pub(super) const SUB_FLOW_SWITCH: u8 = 5;
    pub(super) const RETURN: u8 = 6;
}
/// An entry point's first event, from its start.
const ENTRY_MAIN: usize = 0x1C;

#[derive(Debug, PartialEq)]
pub struct Flowchart {
    pub name: String,
    pub actors: Vec<Actor>,
    pub events: Vec<Event>,
    pub entry_points: Vec<EntryPoint>,
    /// The string pool in file order, which isn't sorted. Writing keeps it
    /// and appends new strings.
    pool: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub name: String,
    pub secondary_name: String,
    pub argument: String,
    pub argument_entry: Option<u16>,
    pub actions: Vec<String>,
    pub queries: Vec<String>,
    pub params: Params,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    Action {
        next: Option<u16>,
        actor: u16,
        action: u16,
        params: Params,
    },
    Switch {
        actor: u16,
        query: u16,
        params: Params,
        /// (query result, event to run).
        cases: Vec<(u32, u16)>,
    },
    Fork {
        join: u16,
        branches: Vec<u16>,
    },
    Join {
        next: Option<u16>,
    },
    SubFlow {
        next: Option<u16>,
        params: Params,
        /// Empty when the entry point is in this same flowchart.
        flowchart: String,
        entry_point: String,
    },
    /// A sub-flow call that picks its next event by the code the callee
    /// returns with.
    SubFlowSwitch {
        params: Params,
        flowchart: String,
        entry_point: String,
        /// (return code, event to run).
        cases: Vec<(u32, u16)>,
    },
    /// Ends a sub-flow with a code for the caller's `SubFlowSwitch`.
    Return {
        code: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntryPoint {
    pub name: String,
    pub main: Option<u16>,
    /// Variable definitions, each kept as its raw 16 bytes.
    pub vars: Vec<(String, [u8; VAR_LEN])>,
}

pub type Params = Vec<(String, Value)>;

/// Something that runs an event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    /// An entry point, by its index in `entry_points`.
    Entry(usize),
    /// An action, join or sub-flow event's `next`.
    Next(u16),
    /// A switch's or a fork's `n`th case or branch: the event, then `n`.
    Branch(u16, usize),
    /// A fork's join, which `relink` can't move.
    Join(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Argument(String),
    Container(Params),
    Int(Vec<i32>),
    Bool(Vec<bool>),
    Float(Vec<f32>),
    String(Vec<String>),
    ActorId { name: String, secondary: String },
}

impl Value {
    /// The single string an argument or one-element string holds.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Argument(s) => Some(s),
            Value::String(v) if v.len() == 1 => Some(&v[0]),
            _ => None,
        }
    }
}

impl Reader<'_> {
    fn index(&self, at: usize) -> Result<Option<u16>> {
        Ok(Some(self.u16(at)?).filter(|&i| i != NONE))
    }

    fn string(&self, at: usize) -> Result<String> {
        let len = self.u16(at)? as usize;
        Ok(std::str::from_utf8(self.bytes(at + 2, len)?)?.to_owned())
    }
    /// The string a pointer at `at` points to.
    fn string_at(&self, at: usize) -> Result<String> {
        self.string(self.ptr(at)?)
    }
    fn words<T>(&self, at: usize, n: usize, f: impl Fn(u32) -> T) -> Result<Vec<T>> {
        (0..n).map(|i| Ok(f(self.u32(at + i * 4)?))).collect()
    }
    fn u16s(&self, at: usize, n: usize) -> Result<Vec<u16>> {
        (0..n).map(|i| self.u16(at + i * 2)).collect()
    }

    /// Names in a `DIC ` radix tree, in item order. Entry 0 is the root.
    fn dict(&self, at: usize) -> Result<Vec<String>> {
        ensure!(self.bytes(at, 4)? == b"DIC ", "no DIC at {at:#x}");
        let n = self.u32(at + 4)? as usize;
        (1..=n)
            .map(|i| self.string_at(at + 8 + i * 16 + 8))
            .collect()
    }

    fn params(&self, at: usize) -> Result<Params> {
        if at == 0 {
            return Ok(Vec::new());
        }
        match self.value(at, 0)? {
            Value::Container(p) => Ok(p),
            v => bail!("params at {at:#x} are a {v:?}, not a container"),
        }
    }

    fn value(&self, at: usize, depth: usize) -> Result<Value> {
        ensure!(depth < MAX_DEPTH, "containers nest too deep at {at:#x}");
        let kind = self.u8(at)?;
        let n = self.u16(at + 2)? as usize;
        let data = at + 0x10;
        Ok(match kind {
            0 => Value::Argument(self.string_at(data)?),
            1 => {
                let names = self.dict(self.ptr(at + 8)?)?;
                ensure!(
                    names.len() == n,
                    "container at {at:#x}: {n} items, {} names",
                    names.len()
                );
                let items = (0..n)
                    .map(|i| self.value(self.ptr(data + i * 8)?, depth + 1))
                    .collect::<Result<Vec<_>>>()?;
                Value::Container(names.into_iter().zip(items).collect())
            }
            2 | 7 => Value::Int(self.words(data, n, |w| w as i32)?),
            // True is stored as 0x80000001.
            3 | 8 => Value::Bool(self.words(data, n, |w| w & 1 == 1)?),
            4 | 9 => Value::Float(self.words(data, n, f32::from_bits)?),
            5 | 10 => Value::String(
                (0..n)
                    .map(|i| self.string_at(data + i * 8))
                    .collect::<Result<_>>()?,
            ),
            12 => Value::ActorId {
                name: self.string_at(data)?,
                secondary: self.string_at(data + 8)?,
            },
            k => bail!("unsupported value type {k} at {at:#x}"),
        })
    }

    fn actor(&self, at: usize) -> Result<Actor> {
        let names = |list: usize, n: usize| -> Result<Vec<String>> {
            (0..n).map(|i| self.string_at(list + i * 8)).collect()
        };
        Ok(Actor {
            name: self.string_at(at)?,
            secondary_name: self.string_at(at + 8)?,
            argument: self.string_at(at + 0x10)?,
            argument_entry: self.index(at + 0x34)?,
            actions: names(self.ptr(at + 0x18)?, self.u16(at + 0x30)? as usize)?,
            queries: names(self.ptr(at + 0x20)?, self.u16(at + 0x32)? as usize)?,
            params: self.params(self.ptr(at + 0x28)?)?,
        })
    }

    fn entry_point(&self, at: usize, name: String) -> Result<EntryPoint> {
        let vars = match self.ptr(at + 8)? {
            0 => Vec::new(),
            dic => {
                let list = self.ptr(at + 0x10)?;
                (0..)
                    .zip(self.dict(dic)?)
                    .map(|(i, n)| Ok((n, self.bytes(list + i * VAR_LEN, VAR_LEN)?.try_into()?)))
                    .collect::<Result<_>>()?
            }
        };
        Ok(EntryPoint {
            name,
            main: self.index(at + ENTRY_MAIN)?,
            vars,
        })
    }

    /// The `STR ` pool: a count that leaves out the empty string heading it.
    fn pool(&self, at: usize) -> Result<Vec<String>> {
        ensure!(self.bytes(at, 4)? == b"STR ", "no string pool at {at:#x}");
        let mut next = at + 0x14;
        (0..=self.u32(at + 0x10)?)
            .map(|_| {
                let s = self.string(next)?;
                next = (next + 3 + s.len()).next_multiple_of(2);
                Ok(s)
            })
            .collect()
    }

    /// A switch's `(value, event)` pairs.
    fn cases(&self, event: usize) -> Result<Vec<(u32, u16)>> {
        let (n, list) = (
            self.u16(event + ev::COUNT)? as usize,
            self.ptr(event + ev::CASES)?,
        );
        (0..n)
            .map(|i| Ok((self.u32(list + i * 8)?, self.u16(list + i * 8 + 4)?)))
            .collect()
    }

    fn event(&self, at: usize) -> Result<Event> {
        let params = || self.params(self.ptr(at + ev::PARAMS)?);
        let kind = match self.u8(at + ev::KIND)? {
            ev::ACTION => EventKind::Action {
                next: self.index(at + ev::NEXT)?,
                actor: self.u16(at + ev::ACTOR)?,
                action: self.u16(at + ev::CALL)?,
                params: params()?,
            },
            ev::SWITCH => EventKind::Switch {
                actor: self.u16(at + ev::ACTOR)?,
                query: self.u16(at + ev::CALL)?,
                params: params()?,
                cases: self.cases(at)?,
            },
            ev::FORK => EventKind::Fork {
                join: self.u16(at + ev::JOIN)?,
                branches: self.u16s(
                    self.ptr(at + ev::BRANCHES)?,
                    self.u16(at + ev::COUNT)? as usize,
                )?,
            },
            ev::JOIN_KIND => EventKind::Join {
                next: self.index(at + ev::NEXT)?,
            },
            ev::SUB_FLOW => EventKind::SubFlow {
                next: self.index(at + ev::NEXT)?,
                params: params()?,
                flowchart: self.string_at(at + ev::SUB_FLOWCHART)?,
                entry_point: self.string_at(at + ev::SUB_ENTRY)?,
            },
            ev::SUB_FLOW_SWITCH => EventKind::SubFlowSwitch {
                params: params()?,
                cases: self.cases(at)?,
                flowchart: self.string_at(at + ev::SWITCH_FLOWCHART)?,
                entry_point: self.string_at(at + ev::SWITCH_ENTRY)?,
            },
            ev::RETURN => EventKind::Return {
                code: self.u32(at + ev::CODE)?,
            },
            k => bail!("unknown event type {k} at {at:#x}"),
        };
        Ok(Event {
            name: self.string_at(at)?,
            kind,
        })
    }
}

impl EventKind {
    /// The events this one runs next, each with the link that runs it, in
    /// file order: a `next`, then cases or branches, then a fork's join.
    fn targets(&self, e: u16) -> impl Iterator<Item = (Link, u16)> + '_ {
        let (next, cases, branches, join): (_, &[(u32, u16)], &[u16], _) = match self {
            EventKind::Action { next, .. }
            | EventKind::Join { next }
            | EventKind::SubFlow { next, .. } => (*next, &[], &[], None),
            EventKind::Switch { cases, .. } | EventKind::SubFlowSwitch { cases, .. } => {
                (None, cases, &[], None)
            }
            EventKind::Fork { join, branches } => (None, &[], branches, Some(*join)),
            EventKind::Return { .. } => (None, &[], &[], None),
        };
        let cases = cases.iter().map(|c| c.1);
        let branches = cases.chain(branches.iter().copied());
        next.map(|to| (Link::Next(e), to))
            .into_iter()
            .chain(
                branches
                    .enumerate()
                    .map(move |(n, to)| (Link::Branch(e, n), to)),
            )
            .chain(join.map(|to| (Link::Join(e), to)))
    }
}

/// The index of `name` in `list`, appended if it's missing.
fn index_of(list: &mut Vec<String>, name: &str) -> u16 {
    let i = match list.iter().position(|n| n == name) {
        Some(i) => i,
        None => {
            list.push(name.to_owned());
            list.len() - 1
        }
    };
    i as u16
}

impl Flowchart {
    /// The calls into other entry points that running from `main` reaches,
    /// without stepping into what they run. The file lists them beside each
    /// entry point, so `to_bytes` writes them from the events: a call left
    /// off the list hangs the event when it's reached.
    pub fn calls_from(&self, main: Option<u16>) -> Vec<u16> {
        let (mut seen, mut calls) = (vec![false; self.events.len()], Vec::new());
        let mut todo = Vec::from_iter(main);
        while let Some(e) = todo.pop() {
            let Some(seen) = seen.get_mut(e as usize) else {
                continue;
            };
            if std::mem::replace(seen, true) {
                continue;
            }
            let kind = &self.events[e as usize].kind;
            if matches!(
                kind,
                EventKind::SubFlow { .. } | EventKind::SubFlowSwitch { .. }
            ) {
                calls.push(e);
            }
            // Pushed in reverse, so the first target runs next.
            let at = todo.len();
            todo.extend(kind.targets(e).map(|(_, to)| to));
            todo[at..].reverse();
        }
        calls
    }

    /// `None` for a file that only holds a timeline (cutscene tracks).
    pub fn parse(data: &[u8]) -> Result<Option<Self>> {
        let r = Reader(data);
        let Some(t) = tables(&r)? else {
            return Ok(None);
        };
        let actors = r.ptr(t.fc + 0x28)?;

        let flow = Flowchart {
            name: r.string_at(t.fc + 0x20)?,
            actors: (0..r.u16(t.fc + 0x10)? as usize)
                .map(|i| r.actor(actors + i * ACTOR_LEN))
                .collect::<Result<_>>()?,
            events: (0..t.event_count)
                .map(|i| r.event(t.events + i * EVENT_LEN))
                .collect::<Result<_>>()?,
            entry_points: r
                .dict(r.ptr(t.fc + 0x38)?)?
                .into_iter()
                .enumerate()
                .map(|(i, name)| r.entry_point(t.entry_points + i * ENTRY_POINT_LEN, name))
                .collect::<Result<_>>()?,
            pool: r.pool(t.fc + r.u32(t.fc + 4)? as usize)?,
        };
        ensure!(
            flow.entry_points.len() == t.entry_point_count,
            "entry point count disagrees with its dictionary"
        );
        flow.check()?;
        Ok(Some(flow))
    }

    fn check_target(&self, to: u16) -> Result<()> {
        ensure!(
            (to as usize) < self.events.len(),
            "event index {to} out of range"
        );
        Ok(())
    }

    /// Event `e`'s calls and targets must land on a real actor and event.
    fn check_event(&self, e: u16) -> Result<()> {
        let call = |actor: u16, n: u16, list: fn(&Actor) -> &Vec<String>, what: &str| {
            let a = self
                .actors
                .get(actor as usize)
                .context("actor index out of range")?;
            ensure!(
                (n as usize) < list(a).len(),
                "{what} index {n} out of range for {}",
                a.name
            );
            anyhow::Ok(())
        };
        let kind = &self.events[e as usize].kind;
        match *kind {
            EventKind::Action { actor, action, .. } => {
                call(actor, action, |a| &a.actions, "action")?
            }
            EventKind::Switch { actor, query, .. } => call(actor, query, |a| &a.queries, "query")?,
            _ => {}
        }
        kind.targets(e)
            .try_for_each(|(_, to)| self.check_target(to))
    }

    /// Every index the events hold must land on a real event or actor.
    fn check(&self) -> Result<()> {
        for (i, e) in (0u16..).zip(&self.events) {
            self.check_event(i)
                .with_context(|| format!("event {i} ({})", e.name))?;
        }
        for ep in &self.entry_points {
            ep.main
                .map_or(Ok(()), |to| self.check_target(to))
                .with_context(|| format!("entry point {}", ep.name))?;
        }
        Ok(())
    }

    pub fn actor_of(&self, actor: u16) -> &Actor {
        &self.actors[actor as usize]
    }

    /// The index of `template`'s actor, added with `template`'s names and
    /// params if the flow lacks it.
    pub fn actor_like(&mut self, template: &Actor) -> usize {
        let same =
            |a: &Actor| a.name == template.name && a.secondary_name == template.secondary_name;
        match self.actors.iter().position(same) {
            Some(i) => i,
            None => {
                self.actors.push(Actor {
                    argument: String::new(),
                    argument_entry: None,
                    actions: Vec::new(),
                    queries: Vec::new(),
                    ..template.clone()
                });
                self.actors.len() - 1
            }
        }
    }

    /// The indexes of `template`'s actor and its `action`, adding whichever
    /// the flow lacks. A new actor copies `template`'s names and params.
    pub fn action(&mut self, template: &Actor, action: &str) -> (u16, u16) {
        let actor = self.actor_like(template);
        (
            actor as u16,
            index_of(&mut self.actors[actor].actions, action),
        )
    }

    /// [`Self::action`] for a query.
    pub fn query(&mut self, template: &Actor, query: &str) -> (u16, u16) {
        let actor = self.actor_like(template);
        (
            actor as u16,
            index_of(&mut self.actors[actor].queries, query),
        )
    }

    fn kind_mut(&mut self, e: u16) -> Result<&mut EventKind> {
        let event = self.events.get_mut(e as usize).context("no such event")?;
        Ok(&mut event.kind)
    }

    /// Point `link` at event `to`, as `relink` does in a file's bytes.
    pub fn link(&mut self, link: Link, to: Option<u16>) -> Result<()> {
        to.map_or(Ok(()), |to| self.check_target(to))?;
        match link {
            Link::Entry(i) => {
                self.entry_points
                    .get_mut(i)
                    .context("no such entry point")?
                    .main = to;
            }
            Link::Next(e) => match self.kind_mut(e)? {
                EventKind::Action { next, .. }
                | EventKind::Join { next }
                | EventKind::SubFlow { next, .. } => *next = to,
                _ => bail!("event {e} has no next"),
            },
            Link::Branch(e, n) => {
                let to = to.context("a branch can't end the flow")?;
                let slot = match self.kind_mut(e)? {
                    EventKind::Switch { cases, .. } | EventKind::SubFlowSwitch { cases, .. } => {
                        cases.get_mut(n).map(|c| &mut c.1)
                    }
                    EventKind::Fork { branches, .. } => branches.get_mut(n),
                    _ => bail!("event {e} has no branches"),
                };
                *slot.with_context(|| format!("event {e} has no branch {n}"))? = to;
            }
            Link::Join(e) => bail!("event {e} runs it as a fork's join"),
        }
        Ok(())
    }

    /// Adds an event under the first free `EventN` name, and returns its
    /// index. Nothing runs it until something links to it.
    pub fn push(&mut self, kind: EventKind) -> u16 {
        let name = (self.events.len()..)
            .map(|n| format!("Event{n}"))
            .find(|n| self.events.iter().all(|e| &e.name != n))
            .unwrap();
        self.events.push(Event { name, kind });
        self.events.len() as u16 - 1
    }

    /// Everything that runs `event`.
    pub fn links_to(&self, event: u16) -> Vec<Link> {
        let entries = self.entry_points.iter().enumerate();
        let entries = entries
            .filter(|(_, ep)| ep.main == Some(event))
            .map(|(i, _)| Link::Entry(i));
        let events = (0u16..)
            .zip(&self.events)
            .flat_map(|(i, e)| e.kind.targets(i))
            .filter(|&(_, to)| to == event)
            .map(|(link, _)| link);
        entries.chain(events).collect()
    }
}

struct Tables {
    fc: usize,
    events: usize,
    event_count: usize,
    entry_points: usize,
    entry_point_count: usize,
}

/// Where the flowchart's tables start; `None` for a timeline.
fn tables(r: &Reader) -> Result<Option<Tables>> {
    ensure!(r.bytes(0, 8)? == MAGIC, "not a BFEVFL");
    ensure!(
        r.bytes(8, 4)? == VERSION,
        "BFEVFL version {:?}",
        r.bytes(8, 4)?
    );
    match r.u16(0x20)? {
        0 => return Ok(None),
        1 => {}
        n => bail!("expected one flowchart, found {n}"),
    }
    let fc = r.ptr(r.ptr(0x28)?)?;
    ensure!(r.bytes(fc, 4)? == b"EVFL", "no EVFL at {fc:#x}");
    Ok(Some(Tables {
        fc,
        events: r.ptr(fc + 0x30)?,
        event_count: r.u16(fc + 0x16)? as usize,
        entry_points: r.ptr(fc + 0x40)?,
        entry_point_count: r.u16(fc + 0x18)? as usize,
    }))
}

/// Point `link` at event `to`, in the file's own bytes. `None` ends the flow
/// there, which only an entry point or a `next` can do.
pub fn relink(data: &mut [u8], link: Link, to: Option<u16>) -> Result<()> {
    let r = Reader(data);
    let t = tables(&r)?.context("a timeline has no events")?;
    let event = |e: u16| -> Result<usize> {
        ensure!((e as usize) < t.event_count, "no event {e}");
        Ok(t.events + e as usize * EVENT_LEN)
    };
    if let Some(to) = to {
        event(to)?;
    }
    let at = match link {
        Link::Entry(i) => {
            ensure!(i < t.entry_point_count, "no entry point {i}");
            t.entry_points + i * ENTRY_POINT_LEN + ENTRY_MAIN
        }
        Link::Next(e) => {
            let e_at = event(e)?;
            ensure!(
                matches!(
                    r.u8(e_at + ev::KIND)?,
                    ev::ACTION | ev::JOIN_KIND | ev::SUB_FLOW
                ),
                "event {e} has no next"
            );
            e_at + ev::NEXT
        }
        Link::Branch(e, n) => {
            let e_at = event(e)?;
            ensure!(to.is_some(), "event {e} can't end the flow from a branch");
            ensure!(
                n < r.u16(e_at + ev::COUNT)? as usize,
                "event {e} has no branch {n}"
            );
            match r.u8(e_at + ev::KIND)? {
                ev::SWITCH | ev::SUB_FLOW_SWITCH => r.ptr(e_at + ev::CASES)? + n * 8 + 4,
                ev::FORK => r.ptr(e_at + ev::BRANCHES)? + n * 2,
                k => bail!("event {e} is type {k}, which has no branches"),
            }
        }
        Link::Join(e) => bail!("event {e} runs it as a fork's join"),
    };
    r.bytes(at, 2)?;
    data[at..at + 2].copy_from_slice(&to.unwrap_or(NONE).to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str, actions: &[&str], params: Params) -> Actor {
        Actor {
            name: name.into(),
            secondary_name: String::new(),
            argument: String::new(),
            argument_entry: None,
            actions: actions.iter().map(|&a| a.into()).collect(),
            queries: vec!["Ask".into()],
            params,
        }
    }

    fn event(name: &str, kind: EventKind) -> Event {
        Event {
            name: name.into(),
            kind,
        }
    }

    /// Every event type, value type and table the writer handles.
    fn sample() -> Flowchart {
        let values: Params = vec![
            ("Int".into(), Value::Int(vec![-3])),
            ("Bool".into(), Value::Bool(vec![true])),
            ("Float".into(), Value::Float(vec![1.5, 2.0, -0.25])),
            ("Text".into(), Value::String(vec!["odd".into()])),
            ("Arg".into(), Value::Argument("even".into())),
            (
                "Who".into(),
                Value::ActorId {
                    name: "Npc".into(),
                    secondary: String::new(),
                },
            ),
            (
                "Nested".into(),
                Value::Container(vec![("Inner".into(), Value::Int(vec![7]))]),
            ),
        ];
        let action = |next, action| EventKind::Action {
            next,
            actor: 0,
            action,
            params: values.clone(),
        };
        Flowchart {
            name: "Sample".into(),
            actors: vec![
                actor("Hero", &["Walk", "Talk"], values.clone()),
                actor("Door", &[], Vec::new()),
            ],
            events: vec![
                event("Event0", action(Some(1), 0)),
                event(
                    "Event1",
                    EventKind::Switch {
                        actor: 0,
                        query: 0,
                        params: Vec::new(),
                        cases: vec![(0, 2), (1, 5)],
                    },
                ),
                event(
                    "Event2",
                    EventKind::Fork {
                        join: 3,
                        branches: vec![4, 0],
                    },
                ),
                event("Event3", EventKind::Join { next: None }),
                event(
                    "Event4",
                    EventKind::SubFlow {
                        next: Some(3),
                        params: Vec::new(),
                        flowchart: "Other".into(),
                        entry_point: "Start".into(),
                    },
                ),
                event(
                    "Event5",
                    EventKind::SubFlowSwitch {
                        params: values.clone(),
                        flowchart: String::new(),
                        entry_point: "Inner".into(),
                        cases: vec![(1, 6)],
                    },
                ),
                event("Event6", EventKind::Return { code: 1 }),
            ],
            entry_points: vec![
                EntryPoint {
                    name: "Main".into(),
                    main: Some(0),
                    vars: Vec::new(),
                },
                EntryPoint {
                    name: "Inner".into(),
                    main: Some(6),
                    vars: vec![(
                        "Flag".into(),
                        [1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 0, 0],
                    )],
                },
            ],
            pool: Vec::new(),
        }
    }

    fn reparse(flow: &Flowchart) -> Flowchart {
        Flowchart::parse(&flow.to_bytes()).unwrap().unwrap()
    }

    #[test]
    fn written_flows_read_back_and_write_the_same_bytes() {
        let flow = sample();
        let bytes = flow.to_bytes();
        let back = reparse(&flow);
        assert_eq!(back.to_bytes(), bytes);
        assert_eq!(back.name, flow.name);
        assert_eq!(back.actors, flow.actors);
        assert_eq!(back.events, flow.events);
        assert_eq!(back.entry_points, flow.entry_points);
    }

    #[test]
    fn an_added_action_reads_back_with_its_actor_params_and_links() {
        let mut flow = reparse(&sample());
        let params = vec![("Num".into(), Value::Int(vec![1]))];
        let pouch = actor("Pouch", &[], params.clone());
        assert_eq!(flow.action(&flow.actors[0].clone(), "Talk"), (0, 1));
        assert_eq!(flow.action(&pouch, "Give"), (2, 0));
        assert_eq!(flow.action(&pouch, "Give"), (2, 0));
        let given = flow.push(EventKind::Action {
            next: Some(0),
            actor: 2,
            action: 0,
            params: params.clone(),
        });
        flow.link(Link::Entry(0), Some(given)).unwrap();
        flow.link(Link::Branch(1, 1), Some(given)).unwrap();

        let back = reparse(&flow);
        assert_eq!(back.events[given as usize].name, "Event7");
        assert_eq!(back.actors[2].name, "Pouch");
        assert_eq!(back.actors[2].params, params);
        assert_eq!(back.actors[2].actions, ["Give"]);
        assert_eq!(
            back.events[given as usize].kind,
            flow.events[given as usize].kind
        );
        assert_eq!(back.links_to(given), [Link::Entry(0), Link::Branch(1, 1)]);
    }

    #[test]
    fn link_refuses_a_link_its_event_lacks() {
        let mut flow = sample();
        assert!(flow.link(Link::Next(1), Some(0)).is_err());
        assert!(flow.link(Link::Branch(1, 2), Some(0)).is_err());
        assert!(flow.link(Link::Branch(1, 0), None).is_err());
        assert!(flow.link(Link::Join(2), Some(0)).is_err());
        flow.link(Link::Next(0), None).unwrap();
        assert!(flow.links_to(1).is_empty());
    }

    #[test]
    fn link_refuses_a_missing_event() {
        let mut flow = sample();
        assert!(flow.link(Link::Entry(0), Some(99)).is_err());
        assert!(flow.link(Link::Next(0), Some(7)).is_err());
        assert!(flow.link(Link::Branch(1, 0), Some(7)).is_err());
        assert_eq!(flow, sample());
    }

    #[test]
    fn relink_edits_the_bytes_as_link_edits_the_flow() {
        let (mut flow, mut bytes) = (sample(), sample().to_bytes());
        for (link, to) in [
            (Link::Entry(1), Some(0)),
            (Link::Next(4), None),
            (Link::Branch(1, 1), Some(3)),
            (Link::Branch(2, 0), Some(6)),
        ] {
            flow.link(link, to).unwrap();
            relink(&mut bytes, link, to).unwrap();
        }
        let back = Flowchart::parse(&bytes).unwrap().unwrap();
        assert_eq!(back.events, flow.events);
        assert_eq!(back.entry_points, flow.entry_points);
    }

    #[test]
    fn relink_refuses_what_link_refuses() {
        let bytes = sample().to_bytes();
        for (link, to) in [
            (Link::Entry(2), Some(0)),
            (Link::Entry(0), Some(7)),
            (Link::Next(7), None),
            (Link::Next(1), Some(0)),
            (Link::Branch(1, 2), Some(0)),
            (Link::Branch(1, 0), None),
            (Link::Branch(9, 0), Some(0)),
            (Link::Join(2), Some(0)),
        ] {
            let mut edited = bytes.clone();
            assert!(relink(&mut edited, link, to).is_err(), "{link:?} {to:?}");
            assert_eq!(edited, bytes);
        }
    }

    #[test]
    fn a_container_that_holds_itself_fails_to_parse() {
        let mut bytes = sample().to_bytes();
        let r = Reader(&bytes);
        let t = tables(&r).unwrap().unwrap();
        // Event 0's params are a container; aim its first item back at it.
        let params = r.ptr(t.events + ev::PARAMS).unwrap();
        bytes[params + 0x10..params + 0x18].copy_from_slice(&(params as u64).to_le_bytes());
        assert!(Flowchart::parse(&bytes).is_err());
    }
}
