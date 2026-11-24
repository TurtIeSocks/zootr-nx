//! Triggers copied from one user to another: [`copy_property`] the calls
//! a user plays as a property changes, [`copy_action`] those it plays
//! during an action. Each trigger then plays the other user's call of the
//! same name, under the same condition.

use anyhow::{Context, Result, ensure};

use super::{
    ACTION_SLOTS, ACTION_TRIGGERS, ACTIONS, CALL, FIRST_TRIGGER, IS_GLOBAL, LAST_TRIGGER, LOCAL,
    LOCALS, NO_OVERWRITE, OVERWRITE, PROPERTIES, PROPERTY_TRIGGERS, TRIGGER_CALL, TRIGGER_COUNTS,
    TRIGGER_TABLES, TRIGGERS, USER_HEADER, User, call_name, find, local_names, moved_past, name_at,
    put, trigger_call, trigger_table, user,
};
use crate::reader::Reader;

// An action slot: its name's offset (8 bytes), then its first and last
// action (s16 each), both included.
const FIRST_ACTION: usize = 0x08;
const LAST_ACTION: usize = 0x0a;
// An action: its name's offset (8 bytes), then its first trigger (u16),
// a flag (u16), and its last trigger (u32), both included.
const ACTION_FIRST: usize = 0x08;
const ACTION_LAST: usize = 0x0c;
/// Where an action trigger keeps the params it overwrites in its call, or
/// [`NO_OVERWRITE`]. Its start frame is at 0x10 and its end at 0x18.
const ACTION_OVERWRITE: usize = 0x20;

/// `file` with the triggers on user `from`'s local property that plays
/// `plays` given to user `to`. The property joins `to`'s locals if it isn't
/// one, so the game keeps its value for `to` too. `to` must have every
/// call they play and no triggers on the property yet.
pub fn copy_property(file: &[u8], from: &str, plays: &str, to: &str) -> Result<Vec<u8>> {
    let r = Reader(file);
    let (a, b) = (user(file, from)?, user(file, to)?);
    let (theirs, mine) = (properties(file, &a)?, properties(file, &b)?);
    let k = playing(file, &a, PROPERTY_TRIGGERS, &theirs, plays)?
        .with_context(|| format!("no property of {from}'s plays {plays}"))?;
    let prop = &theirs[k];
    let name = String::from_utf8_lossy(prop.name);
    let has = |n: &[u8]| mine.iter().any(|p| p.name == n);
    ensure!(!has(prop.name), "{to} already has triggers on {name}");
    ensure!(r.u32(prop.at + IS_GLOBAL)? == 0, "{name} is global");
    let triggers = retargeted(file, &a, &b, PROPERTY_TRIGGERS, prop, OVERWRITE)?;
    // It goes after each property it follows in `from` that `to` has, and
    // its triggers before theirs from there on.
    let place = theirs[..k].iter().filter(|p| has(p.name)).count();
    let (props, first) = ranged(file, &b, PROPERTIES, &mine, place, prop)?;
    let locals = local_names(file, &b)?;
    let local = match locals.contains(&prop.name) {
        true => None,
        false => {
            let theirs = local_names(file, &a)?;
            let k = (theirs.iter().position(|l| *l == prop.name))
                .with_context(|| format!("{name} isn't among {from}'s locals"))?;
            let entry = r.bytes(a.base + USER_HEADER + LOCAL * k, LOCAL)?;
            let at = locals.partition_point(|l| *l < prop.name);
            Some((b.base + USER_HEADER + LOCAL * at, entry))
        }
    };
    // Spliced from the end back, so each place in `file` still holds.
    let mut out = spliced(
        file,
        &b,
        PROPERTY_TRIGGERS,
        first,
        triggers,
        PROPERTIES,
        props,
    )?;
    if let Some((at, entry)) = local {
        out.splice(at..at, entry.iter().copied());
        put(
            &mut out,
            b.base + LOCALS,
            &(locals.len() as u16 + 1).to_le_bytes(),
        );
        let triggers = r.u64(b.base + TRIGGERS)? + LOCAL as u64;
        put(&mut out, b.base + TRIGGERS, &triggers.to_le_bytes());
    }
    counted(
        file,
        &b,
        out,
        [(PROPERTIES, 1), (PROPERTY_TRIGGERS, prop.len())],
    )
}

/// `file` with user `from`'s action whose triggers play `plays` given to
/// user `to`, in its slot of the same name, among the slot's actions by
/// name. `to` must have that slot, every call they play, and no action of
/// that name in it yet.
pub fn copy_action(file: &[u8], from: &str, plays: &str, to: &str) -> Result<Vec<u8>> {
    let r = Reader(file);
    let (a, b) = (user(file, from)?, user(file, to)?);
    let (theirs, mine) = (actions(file, &a)?, actions(file, &b)?);
    let k = playing(file, &a, ACTION_TRIGGERS, &theirs, plays)?
        .with_context(|| format!("no action of {from}'s plays {plays}"))?;
    let act = &theirs[k];
    let name = String::from_utf8_lossy(act.name);
    let slot = String::from_utf8_lossy(act.slot);
    let (slots_at, _) = trigger_table(file, &b, ACTION_SLOTS)?;
    let size = TRIGGER_TABLES[ACTION_SLOTS];
    let target = (0..slots(file, &b)?)
        .find(|&s| slot_name(file, slots_at + s * size).ok() == Some(act.slot))
        .with_context(|| format!("{to} has no action slot {slot}"))?;
    let in_slot: Vec<_> = (mine.iter().enumerate())
        .filter(|(_, m)| m.slot == act.slot)
        .collect();
    ensure!(
        in_slot.iter().all(|(_, m)| m.name != act.name),
        "{to} already has {name} in {slot}"
    );
    ensure!(
        in_slot.windows(2).all(|w| w[0].1.name < w[1].1.name),
        "{to}'s {slot} actions aren't sorted by name"
    );
    let place = match in_slot.iter().find(|(_, m)| m.name > act.name) {
        Some((i, _)) => *i,
        None => in_slot
            .last()
            .map(|(i, _)| i + 1)
            .context("an empty slot")?,
    };
    let triggers = retargeted(file, &a, &b, ACTION_TRIGGERS, act, ACTION_OVERWRITE)?;
    let (actions, first) = ranged(file, &b, ACTIONS, &mine, place, act)?;
    let mut out = spliced(file, &b, ACTION_TRIGGERS, first, triggers, ACTIONS, actions)?;
    // The slots after it start an action later, and its own ends one later.
    for s in 0..slots(file, &b)? {
        let at = slots_at + s * size;
        let (f, l) = (r.u16(at + FIRST_ACTION)?, r.u16(at + LAST_ACTION)?);
        let (f, l) = match s {
            _ if s == target => (f, l + 1),
            _ if f as usize >= place => (f + 1, l + 1),
            _ => (f, l),
        };
        put(&mut out, at + FIRST_ACTION, &f.to_le_bytes());
        put(&mut out, at + LAST_ACTION, &l.to_le_bytes());
    }
    counted(file, &b, out, [(ACTIONS, 1), (ACTION_TRIGGERS, act.len())])
}

/// A property or action of a user's: where its entry is, its name, its
/// action slot's name (empty for a property), and its first and last
/// trigger.
struct Ranged<'a> {
    at: usize,
    name: &'a [u8],
    slot: &'a [u8],
    first: usize,
    last: usize,
}

impl Ranged<'_> {
    fn len(&self) -> usize {
        self.last - self.first + 1
    }
}

/// `user`'s properties with triggers, in their order.
fn properties<'a>(file: &'a [u8], user: &User) -> Result<Vec<Ranged<'a>>> {
    let (at, count) = trigger_table(file, user, PROPERTIES)?;
    (0..count)
        .map(|k| entry(file, at + k * TRIGGER_TABLES[PROPERTIES], PROPERTIES, b""))
        .collect()
}

/// `user`'s actions, in their order, each with its slot's name.
fn actions<'a>(file: &'a [u8], user: &User) -> Result<Vec<Ranged<'a>>> {
    let r = Reader(file);
    let (slots_at, _) = trigger_table(file, user, ACTION_SLOTS)?;
    let (at, count) = trigger_table(file, user, ACTIONS)?;
    let mut actions = Vec::with_capacity(count);
    for s in 0..slots(file, user)? {
        let slot = slots_at + s * TRIGGER_TABLES[ACTION_SLOTS];
        let (f, l) = (r.u16(slot + FIRST_ACTION)?, r.u16(slot + LAST_ACTION)?);
        ensure!(
            f as usize == actions.len() && f <= l && (l as usize) < count,
            "action slots that don't cover the actions in order aren't read yet"
        );
        let name = slot_name(file, slot)?;
        for k in f..=l {
            let action = at + k as usize * TRIGGER_TABLES[ACTIONS];
            actions.push(entry(file, action, ACTIONS, name)?);
        }
    }
    ensure!(actions.len() == count, "actions past the action slots");
    Ok(actions)
}

/// How many action slots `user` has.
fn slots(file: &[u8], user: &User) -> Result<usize> {
    Ok(trigger_table(file, user, ACTION_SLOTS)?.1)
}

/// The name of the action slot at `at`.
fn slot_name(file: &[u8], at: usize) -> Result<&[u8]> {
    name_at(file, Reader(file).u64(at)? as usize)
}

/// Where an entry of table `t` (properties or actions) keeps its first and
/// last trigger, and whether the first is a u16, as an action's is.
fn range_at(t: usize) -> (usize, bool, usize) {
    match t {
        PROPERTIES => (FIRST_TRIGGER, false, LAST_TRIGGER),
        _ => (ACTION_FIRST, true, ACTION_LAST),
    }
}

/// The entry of table `t` (properties or actions) at `at`.
fn entry<'a>(file: &'a [u8], at: usize, t: usize, slot: &'a [u8]) -> Result<Ranged<'a>> {
    let r = Reader(file);
    let (first, short, last) = range_at(t);
    let first = match short {
        true => r.u16(at + first)? as u32,
        false => r.u32(at + first)?,
    };
    let last = r.u32(at + last)?;
    ensure!(first <= last, "an entry with no triggers isn't read yet");
    Ok(Ranged {
        at,
        name: name_at(file, r.u64(at)? as usize)?,
        slot,
        first: first as usize,
        last: last as usize,
    })
}

/// The name of the call `user`'s trigger at `at` plays.
fn played<'a>(file: &'a [u8], user: &User, at: usize) -> Result<&'a str> {
    let call = trigger_call(file, user, at + TRIGGER_CALL)?;
    Ok(std::str::from_utf8(call_name(
        file,
        user.calls + call * CALL,
    )?)?)
}

/// Which of `entries`, ranges of `user`'s triggers in table `t`, has a
/// trigger that plays `plays`.
fn playing(
    file: &[u8],
    user: &User,
    t: usize,
    entries: &[Ranged],
    plays: &str,
) -> Result<Option<usize>> {
    let (at, _) = trigger_table(file, user, t)?;
    for (k, e) in entries.iter().enumerate() {
        for i in e.first..=e.last {
            if played(file, user, at + i * TRIGGER_TABLES[t])? == plays {
                return Ok(Some(k));
            }
        }
    }
    Ok(None)
}

/// `a`'s triggers in table `t` that `e` ranges over, each playing `b`'s
/// call of the same name. A trigger that overwrites its call's params (at
/// `overwrite`) is refused, as is a call `b` doesn't have.
fn retargeted(
    file: &[u8],
    a: &User,
    b: &User,
    t: usize,
    e: &Ranged,
    overwrite: usize,
) -> Result<Vec<u8>> {
    let r = Reader(file);
    let (at, _) = trigger_table(file, a, t)?;
    let size = TRIGGER_TABLES[t];
    let name = String::from_utf8_lossy(e.name);
    let mut triggers = Vec::with_capacity(size * e.len());
    for i in e.first..=e.last {
        let at = at + i * size;
        ensure!(
            r.u32(at + overwrite)? == NO_OVERWRITE,
            "a trigger on {name} overwrites its call's params"
        );
        let call = played(file, a, at)?;
        let own = find(file, b, call)?.with_context(|| format!("no {call} to play"))?;
        let mut entry = r.bytes(at, size)?.to_vec();
        put(
            &mut entry,
            TRIGGER_CALL,
            &((own * CALL) as u64).to_le_bytes(),
        );
        triggers.extend(entry);
    }
    Ok(triggers)
}

/// `b`'s table `t` (properties or actions, `mine`) with `e` at `place`,
/// and the first trigger of `e`'s there. `e`'s triggers go before those of
/// the entries after it, whose ranges move up.
fn ranged(
    file: &[u8],
    b: &User,
    t: usize,
    mine: &[Ranged],
    place: usize,
    e: &Ranged,
) -> Result<(Vec<u8>, usize)> {
    let r = Reader(file);
    let (first_at, short, last_at) = range_at(t);
    let count = trigger_table(file, b, t + 1)?.1;
    ensure!(
        !short || count + e.len() <= 0x1_0000,
        "more triggers than an action's first trigger field holds"
    );
    let first = mine.get(place).map_or(count, |m| m.first);
    let entry = |at: usize, first: usize, last: usize| -> Result<Vec<u8>> {
        let mut entry = r.bytes(at, TRIGGER_TABLES[t])?.to_vec();
        let bytes = (first as u32).to_le_bytes();
        put(&mut entry, first_at, &bytes[..if short { 2 } else { 4 }]);
        put(&mut entry, last_at, &(last as u32).to_le_bytes());
        Ok(entry)
    };
    let mut table = Vec::with_capacity(TRIGGER_TABLES[t] * (mine.len() + 1));
    for (i, m) in mine.iter().enumerate() {
        if i == place {
            table.extend(entry(e.at, first, first + e.len() - 1)?);
        }
        let by = if m.first >= first { e.len() } else { 0 };
        table.extend(entry(m.at, m.first + by, m.last + by)?);
    }
    if place == mine.len() {
        table.extend(entry(e.at, first, first + e.len() - 1)?);
    }
    Ok((table, first))
}

/// `file` with `b`'s `triggers` put in its table `t` at `first`, and its
/// table `entries` replaced with `table`, spliced from the end back so
/// each place in `file` still holds.
fn spliced(
    file: &[u8],
    b: &User,
    t: usize,
    first: usize,
    triggers: Vec<u8>,
    entries: usize,
    table: Vec<u8>,
) -> Result<Vec<u8>> {
    let (triggers_at, _) = trigger_table(file, b, t)?;
    let (table_at, count) = trigger_table(file, b, entries)?;
    let mut out = file.to_vec();
    let at = triggers_at + first * TRIGGER_TABLES[t];
    out.splice(at..at, triggers);
    out.splice(table_at..table_at + TRIGGER_TABLES[entries] * count, table);
    Ok(out)
}

/// `out`, `file` with `b` grown, with `b`'s trigger table counts up by
/// `more` and every offset past `b` moved.
fn counted(file: &[u8], b: &User, mut out: Vec<u8>, more: [(usize, usize); 2]) -> Result<Vec<u8>> {
    let r = Reader(file);
    for (t, more) in more {
        let count = r.u32(b.base + TRIGGER_COUNTS + 4 * t)? + more as u32;
        put(
            &mut out,
            b.base + TRIGGER_COUNTS + 4 * t,
            &count.to_le_bytes(),
        );
    }
    moved_past(file, b, &mut out)?;
    Ok(out)
}

/// Each of `user`'s properties with triggers, and the calls they play.
pub fn property_names<'a>(file: &'a [u8], user_name: &str) -> Result<Vec<(&'a str, Vec<&'a str>)>> {
    let u = user(file, user_name)?;
    let (at, _) = trigger_table(file, &u, PROPERTY_TRIGGERS)?;
    let size = TRIGGER_TABLES[PROPERTY_TRIGGERS];
    (properties(file, &u)?.into_iter())
        .map(|p| {
            let calls = (p.first..=p.last).map(|t| played(file, &u, at + t * size));
            Ok((std::str::from_utf8(p.name)?, calls.collect::<Result<_>>()?))
        })
        .collect()
}

/// Each of `user`'s actions as its slot and name, and the calls it plays.
pub fn action_names<'a>(
    file: &'a [u8],
    user_name: &str,
) -> Result<Vec<(&'a str, &'a str, Vec<&'a str>)>> {
    let u = user(file, user_name)?;
    let (at, _) = trigger_table(file, &u, ACTION_TRIGGERS)?;
    let size = TRIGGER_TABLES[ACTION_TRIGGERS];
    (actions(file, &u)?.into_iter())
        .map(|a| {
            let calls = (a.first..=a.last).map(|t| played(file, &u, at + t * size));
            let calls = calls.collect::<Result<_>>()?;
            Ok((
                std::str::from_utf8(a.slot)?,
                std::str::from_utf8(a.name)?,
                calls,
            ))
        })
        .collect()
}
