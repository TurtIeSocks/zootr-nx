//! What the hooks do to chests: listing the ones a map makes, picking the
//! level each gives, and recreating the closed ones when the pouch grows.

use core::ffi::CStr;

use super::{Hash, MAX, at, dev, func, name, shared};
use crate::traps::COMPONENTS;
use crate::{CHESTS, Chest, Offsets, Table, after_grant, pick, pick_with};

type Count = unsafe extern "C" fn(usize, *const *const u8, u32, u32) -> i32;
type Delete = unsafe extern "C" fn(usize, usize) -> bool;
type Create = unsafe extern "C" fn(usize, usize, *const usize, usize) -> bool;

/// The placement param a chest's drop is bound from as the room loads.
const DROP: &[u8] = b"Drop__DropActor";
/// The holder slot a chest's content is made in.
const CONTENT: &CStr = c"Content";
/// More params than any placement has, so a bad count stops here.
const PARAMS: i32 = 256;
/// More slot names than any holder has, so a bad count stops here.
const SLOTS: u32 = 64;

/// List the chest `node` makes content for, when that's a line level.
///
/// # Safety
/// `node` is the create node's run argument.
pub(super) unsafe fn record(main: usize, table: &Table, node: usize) -> Option<()> {
    // Node inputs carry flags in their low bits.
    let input = |off| unsafe { at(node, off) }.map(|v| v & !3).filter(|&v| v != 0);
    let key = unsafe { name(*(input(0x20)? as *const *const u8)) }?;
    if *key != *CONTENT.to_bytes() {
        return None;
    }
    let file = unsafe { name(*(input(0x38)? as *const *const u8)) }?;
    let (line, _, level) = table.find(file)?;
    let index = unsafe { *(input(0x30)? as *const u32) };
    let actor = unsafe { at(at(node, 8)?, 8)? };
    let holder = unsafe { at(at(at(actor, 0x1f0)?, COMPONENTS)?, 0x110)? };
    let chest = Chest {
        actor,
        holder,
        name: level.drop.as_ptr() as usize,
        index,
        line,
        done: false,
    };
    shared(main).with(|l| l.made(chest));
    Some(())
}

/// Recreate the content of every closed chest of `added`'s lines as the
/// level it gives once `added` is owned.
///
/// # Safety
/// The table's offsets are this build's.
pub(super) unsafe fn recreate(main: usize, table: &Table, added: &[u8]) -> Option<()> {
    let owns = unsafe { progression_owned(main, table.offsets) }?;
    for (line, next) in after_grant(table, added, &owns) {
        unsafe { remake(main, table, line, next.as_ptr() as usize) };
    }
    Some(())
}

/// A loose pickup may be added under its stand-in name. Refresh the closed
/// chests from the pouch once the get is over.
///
/// # Safety
/// The table's offsets are this build's.
#[inline(never)]
#[unsafe(link_section = ".far")]
pub(super) unsafe fn refresh(main: usize, table: &Table) -> Option<()> {
    let owns = unsafe { progression_owned(main, table.offsets) }?;
    let shared = shared(main);
    for (line, _, levels) in table.lines() {
        let loaded = shared.with(|l| l.chests().iter().any(|c| c.line == line && !c.done));
        if loaded && let Some(next) = pick(levels, &owns) {
            unsafe { remake(main, table, line, next.as_ptr() as usize) };
        }
    }
    Some(())
}

/// Recreate the content of every closed chest of `line` as `next`.
///
/// # Safety
/// The table's offsets are this build's, `next` a table name's address.
unsafe fn remake(main: usize, table: &Table, line: u16, next: usize) {
    let shared = shared(main);
    for i in 0..CHESTS {
        // Claimed under the lock, swapped outside it, since create may
        // reach code that makes chests.
        let claimed = shared.with(|l| {
            let c = l.chests().get_mut(i)?;
            let stale = c.line == line && !c.done && c.name != next;
            stale.then(|| core::mem::replace(c, Chest { name: next, ..*c }))
        });
        let Some(old) = claimed else {
            continue;
        };
        let done = unsafe { swap(main, table.offsets, &old, shared.name_at(i)) };
        if done != Some(true) {
            shared.with(|l| {
                if let Some(c) = l.chests().get_mut(i) {
                    c.name = old.name;
                    c.done = done.is_none();
                }
            });
        }
    }
}

/// Delete `chest`'s content and create `name` in its slot. `None` when
/// the chest fails its check, else whether create took it.
///
/// # Safety
/// The offsets are this build's, `name` holds a table name's address.
unsafe fn swap(main: usize, o: &Offsets, chest: &Chest, name: *const usize) -> Option<bool> {
    let slot = unsafe { slot(main, o, chest)? };
    let owner = unsafe { at(chest.holder, 0x18)? };
    unsafe {
        let delete: Delete = func(main, o.delete);
        let create: Create = func(main, o.create);
        delete(0, slot);
        // Create waits for the old actor to die unless the slot's
        // request id is cleared.
        let id = (slot + 0x38) as *mut i32;
        let old = *id;
        *id = -1;
        let made = create(slot, owner, name, 0);
        if !made && *id == -1 {
            *id = old;
        }
        Some(made)
    }
}

/// `chest`'s slot, if its holder still gives one there.
///
/// # Safety
/// The offsets are this build's, `chest` was made in this map.
unsafe fn slot(main: usize, o: &Offsets, chest: &Chest) -> Option<usize> {
    let hash: Hash = unsafe { func(main, o.hash) };
    let want = unsafe { hash(CONTENT.as_ptr().cast(), 0) };
    let holder = chest.holder;
    let n = unsafe { *((holder + 0x34) as *const u32) };
    if n > SLOTS {
        return None;
    }
    let names = unsafe { at(holder, 0x28)? };
    let entry = (0..n as usize)
        .map(|i| names + i * 32)
        .find(|&e| unsafe { *(e as *const u32) } == want)?;
    if chest.index >= unsafe { *((entry + 0x10) as *const u32) } {
        return None;
    }
    let slot = unsafe { at(at(entry, 0x18)?, chest.index as usize * 8)? };
    let vtable = unsafe { *(slot as *const usize) };
    (vtable == main + o.slot_vtable as usize).then_some(slot)
}

/// The placement's `Drop__DropActor` string.
///
/// # Safety
/// `placement` is the reader's placement argument.
pub(super) unsafe fn drop_value(placement: usize) -> Option<*mut u8> {
    let list = unsafe { at(at(at(at(placement, 0x40)?, COMPONENTS)?, 0)?, 0x10)? };
    let count = unsafe { *(list as *const i32) };
    if !(0..=PARAMS).contains(&count) {
        return None;
    }
    let (names, values) = unsafe { (at(list, 0x8)?, at(list, 0x18)?) };
    (0..count as usize)
        .find(|&i| {
            unsafe { name(*((names + i * 8) as *const *const u8)) }.is_some_and(|n| *n == *DROP)
        })
        .and_then(|i| unsafe { at(values, i * 8) })
        .map(|v| v as *mut u8)
}

/// Write the level Link should get over the fixed string at `value`,
/// when it has room.
///
/// # Safety
/// `value` is a placement param's string buffer.
pub(super) unsafe fn rewrite(main: usize, table: &Table, value: *mut u8) {
    let Some(give) =
        (unsafe { name(value) }).and_then(|now| unsafe { level_for(main, table, now) })
    else {
        return;
    };
    let header = value as usize - 0x10;
    let (top, cap) = unsafe { (*(header as *const usize), *((header + 0x8) as *const i32)) };
    if top != value as usize || cap <= give.len() as i32 {
        return;
    }
    // SAFETY: the buffer holds `cap` bytes, more than the name and NUL.
    unsafe {
        core::ptr::copy_nonoverlapping(give.as_ptr(), value, give.len());
        *value.add(give.len()) = 0;
    }
}

/// Whether Link owns a pouch name.
///
/// # Safety
/// The offsets are this build's.
pub(super) unsafe fn owned(main: usize, o: &Offsets) -> Option<impl Fn(&[u8]) -> bool> {
    let manager = unsafe { at(main, o.manager as usize) }?;
    let count: Count = unsafe { func(main, o.count) };
    Some(move |pouch: &[u8]| unsafe { count(manager, &pouch.as_ptr(), 0, 0) } > 0)
}

/// A stand-in can set a level's ownership flag and capacity without adding
/// stock under the original pouch name.
///
/// # Safety
/// The offsets are this build's.
unsafe fn progression_owned(main: usize, o: &Offsets) -> Option<impl Fn(&[u8]) -> bool> {
    let count = unsafe { owned(main, o) }?;
    let gd = unsafe { dev::single(main, o.gamedata) };
    Some(move |pouch: &[u8]| {
        count(pouch) || gd.is_some_and(|gd| unsafe { owns_flag(main, o, gd, pouch) })
    })
}

/// # Safety
/// The GameData manager and offsets are this build's.
#[inline(never)]
#[unsafe(link_section = ".far")]
unsafe fn owns_flag(main: usize, o: &Offsets, gd: usize, pouch: &[u8]) -> bool {
    const PREFIX: &[u8] = b"IsGet.";
    let mut key = [0; PREFIX.len() + MAX + 1];
    if pouch.len() > MAX {
        return false;
    }
    key[..PREFIX.len()].copy_from_slice(PREFIX);
    // SAFETY: The bounded pouch name fits after the prefix and before the NUL.
    unsafe {
        core::ptr::copy_nonoverlapping(
            pouch.as_ptr(),
            key.as_mut_ptr().add(PREFIX.len()),
            pouch.len(),
        );
    }
    let hash: Hash = unsafe { func(main, o.hash) };
    let hash = unsafe { hash(key.as_ptr(), 0) };
    unsafe { dev::get_bool(main, o, gd, hash) == Some(true) }
}

/// The level to give in place of `now`, when it needs changing.
///
/// # Safety
/// The table's offsets are this build's.
pub(super) unsafe fn level_for<'a>(main: usize, table: &Table<'a>, now: &[u8]) -> Option<&'a [u8]> {
    let owns = unsafe { progression_owned(main, table.offsets) }?;
    let (_, levels, _) = table.find(now)?;
    pick_with(levels, b"", &owns).filter(|&give| give != now)
}
