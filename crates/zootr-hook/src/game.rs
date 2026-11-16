//! The hooks as the game runs them: the entry stubs the patches branch to,
//! the handlers behind them, and the reads of the game's memory they share.

use core::{arch::asm, arch::global_asm, slice};

use crate::{CAVE, Credit, Credited, HEADER, Header, LIST, List, Offsets, Shared, TABLE, Table};

mod bunny;
mod chests;
mod dev;
mod refill;
mod rt;

use chests::{drop_value, level_for, record, recreate, rewrite};

type Setter = unsafe extern "C" fn(*mut u8, *const *const u8, *const *const u8);
type Reader = unsafe extern "C" fn(*mut u8, *const u8, usize, usize) -> usize;

/// The argument the chest's get event gives and shows.
const KEY: &[u8] = b"ContentName";
/// Longer than any actor name, so a runaway string stops here.
const MAX: usize = 128;

// The call sites `bl` to content_name at +0, drop_actor at +4 and
// run_speed at +24. The hooked functions `b` to +8, +12, +16, +20 and +28
// in place of their pacibsp.
// The opening node is in x20 at both content_name call sites.
//
// A function hook keeps every argument register and the link register,
// calls its handler, runs the pacibsp it replaced with the same sp, and
// goes on through its slot at `RESUMES`, where the patch writes a `b`
// past that pacibsp. Every branch is direct, so a build that translates
// the game's code ahead of time can follow each one.
global_asm!(
    ".macro function_hook handler, resume",
    "sub sp, sp, #0xd0",
    "stp x0, x1, [sp]",
    "stp x2, x3, [sp, #0x10]",
    "stp x4, x5, [sp, #0x20]",
    "stp x6, x7, [sp, #0x30]",
    "stp x8, x30, [sp, #0x40]",
    "stp q0, q1, [sp, #0x50]",
    "stp q2, q3, [sp, #0x70]",
    "stp q4, q5, [sp, #0x90]",
    "stp q6, q7, [sp, #0xb0]",
    "bl \\handler",
    "ldp q6, q7, [sp, #0xb0]",
    "ldp q4, q5, [sp, #0x90]",
    "ldp q2, q3, [sp, #0x70]",
    "ldp q0, q1, [sp, #0x50]",
    "ldp x8, x30, [sp, #0x40]",
    "ldp x6, x7, [sp, #0x30]",
    "ldp x4, x5, [sp, #0x20]",
    "ldp x2, x3, [sp, #0x10]",
    "ldp x0, x1, [sp]",
    "add sp, sp, #0xd0",
    "pacibsp",
    "b \\resume",
    ".endm",
    ".section .text.entry, \"ax\"",
    ".global entries",
    "entries:",
    "b content_name_entry",
    "b drop_actor",
    "b map_loaded_entry",
    "b chest_made_entry",
    "b granted_entry",
    "b link_update_entry",
    "b run_speed",
    "b picked_entry",
    "entries_end:",
    "map_loaded_resume:",
    "brk #0",
    "chest_made_resume:",
    "brk #0",
    "granted_resume:",
    "brk #0",
    "link_update_resume:",
    "brk #0",
    "picked_resume:",
    "brk #0",
    ".text",
    "content_name_entry:",
    "mov x4, x20",
    "b content_name",
    "map_loaded_entry:",
    "function_hook map_loaded, map_loaded_resume",
    "chest_made_entry:",
    "function_hook chest_made, chest_made_resume",
    "granted_entry:",
    "function_hook granted, granted_resume",
    "link_update_entry:",
    "function_hook link_update, link_update_resume",
    "picked_entry:",
    "function_hook picked, picked_resume",
);

/// `main`, from where the hook starts.
fn main() -> usize {
    let start: usize;
    // SAFETY: a PC-relative address load, nothing else.
    unsafe {
        asm!(
            "adr {s}, __hook_start",
            s = out(reg) start,
            options(nomem, nostack, preserves_flags),
        );
    }
    start - CAVE as usize
}

/// `main` and the table, or `None` on a bad magic.
fn table() -> Option<(usize, Table<'static>)> {
    let main = main();
    // SAFETY: the cheat writes the table at `TABLE`, 8 byte aligned.
    let header = unsafe { &*((main + TABLE as usize) as *const Header) };
    let at = (header as *const Header).cast::<u8>();
    let size = (header.size + header.credits) as usize;
    let body = unsafe { slice::from_raw_parts(at.add(HEADER), size) };
    Some((main, Table::parse(header, body)?))
}

/// Run `f` for a function hook. The cheat writes the table with the
/// hooks, so a bad magic means the patch itself is broken.
fn hooked(f: impl FnOnce(usize, &Table)) {
    let Some((main, table)) = table() else {
        rt::stop(b"BADTABLE")
    };
    f(main, &table);
}

/// The chest list.
fn shared(main: usize) -> &'static Shared {
    // SAFETY: `LIST` is zeroed at start and only ever holds the list.
    unsafe { &*((main + LIST as usize) as *const Shared) }
}

/// The bytes before a NUL, if there is one within `MAX`.
///
/// # Safety
/// `p` is null or readable up to its NUL or `MAX`.
unsafe fn name<'a>(p: *const u8) -> Option<&'a [u8]> {
    if p.is_null() {
        return None;
    }
    let len = (0..MAX).find(|&i| unsafe { *p.add(i) } == 0)?;
    Some(unsafe { slice::from_raw_parts(p, len) })
}

/// The pointer at `p + off`, or `None` if `p` or it is null.
///
/// # Safety
/// `p` is null or readable at `off`.
unsafe fn at(p: usize, off: usize) -> Option<usize> {
    if p == 0 {
        return None;
    }
    let v = unsafe { *((p + off) as *const usize) };
    (v != 0).then_some(v)
}

/// Called in place of the event argument setter, with its arguments
/// and the opening node.
///
/// # Safety
/// Only the game calls this, from the rewritten call sites.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn content_name(
    request: *mut u8,
    key: *const *const u8,
    value: *const *const u8,
    _: usize,
    node: usize,
) {
    let Some((main, table)) = table() else {
        return;
    };
    // SAFETY: the offsets were found in this build of `main`.
    unsafe {
        let setter: Setter = core::mem::transmute(main + table.offsets.setter as usize);
        let give = if name(*key).is_some_and(|k| *k == *KEY) {
            if let Some(actor) = at(node, 8).and_then(|n| at(n, 8)) {
                shared(main).with(|l| l.opened(actor));
            }
            name(*value).and_then(|now| level_for(main, &table, now))
        } else {
            None
        };
        let chosen = give.map(<[u8]>::as_ptr);
        // The setter keeps the pointer, and the table's names never move.
        let value = chosen.as_ref().map_or(value, |p| p as *const *const u8);
        setter(request, key, value)
    }
}

/// Called in place of the drop reader, with its arguments.
///
/// # Safety
/// Only the game calls this, from the rewritten call sites.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_actor(
    out: *mut u8,
    placement: *const u8,
    a2: usize,
    a3: usize,
) -> usize {
    let Some((main, table)) = table() else {
        return 0;
    };
    // SAFETY: the offsets were found in this build of `main`.
    unsafe {
        if let Some(value) = drop_value(placement as usize) {
            rewrite(main, &table, value);
        }
        let reader: Reader = core::mem::transmute(main + table.offsets.reader as usize);
        reader(out, placement, a2, a3)
    }
}

/// A map has started loading: its chests come next.
#[unsafe(no_mangle)]
extern "C" fn map_loaded() {
    hooked(|main, _| shared(main).with(List::clear))
}

/// A holder slot's actor is about to be made by `node`.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn chest_made(node: usize) {
    hooked(|main, table| unsafe {
        record(main, table, node);
    })
}

/// `name` is about to be added to the pouch: credit it, then remake the
/// closed chests of its line. Every holder that adds to the pouch passes
/// here once, so a copy credits its dungeon once. Pickups don't add, and
/// [`picked`] credits theirs.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn granted(_: usize, added: *const *const u8) {
    hooked(|main, table| unsafe {
        if let Some(added) = name(*added) {
            credit(main, table, added, false);
            recreate(main, table, added);
        }
    })
}

/// A pickup window is opening on what Link takes (a pot's drop, a
/// freestanding item): credit it. The take stocks the pouch without
/// passing the add, so `granted` never sees it. The window's node binds
/// the item's name, its slot at `node + 0x20` tagged in the low 2 bits.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn picked(node: usize) {
    hooked(|main, table| unsafe {
        let slot = at(node, PICKED_NAME).map(|s| s & !3);
        if let Some(added) = slot
            .and_then(|s| at(s, 0))
            .and_then(|p| name(p as *const u8))
        {
            credit(main, table, added, true);
        }
    })
}

/// How many seconds apart one get's add and window can run, at most. A
/// get event takes a few.
const SAME_GET: u64 = 10;

/// Where the pickup window's node keeps its bound `ActorName`.
const PICKED_NAME: usize = 0x20;

/// Set what the table credits `added` with, if anything, unless the other
/// hook (the add, or with `picked` the pickup window) already credited
/// this get.
///
/// # Safety
/// The table's offsets are this build's.
unsafe fn credit(main: usize, table: &Table, added: &[u8], picked: bool) {
    let Some(credit) = table.credit(added) else {
        return;
    };
    let (at, rate) = dev::clock();
    let name = added.iter().fold(0x811c_9dc5_u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let note = Credited { name, picked, at };
    if !shared(main).with(|l| l.first_credit(note, SAME_GET * rate)) {
        return;
    }
    // SAFETY: the offsets were found in this build of `main`.
    let Some(gd) = (unsafe { dev::single(main, table.offsets.gamedata) }) else {
        return;
    };
    unsafe {
        match credit {
            Credit::Bool(hash) => dev::set_bool(main, table.offsets, gd, hash, true),
            Credit::Int(hash, n) => refill::set_int(gd, hash, n),
            Credit::Add(hash, n) => refill::add_int(gd, hash, n),
        }
    }
}

/// Link is about to update: hold what the dev switches hold, and run the
/// app's request, if there is one.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn link_update(actor: usize) {
    hooked(|main, table| unsafe {
        refill::refill(main, &table, actor);
        dev::serve(main, table.offsets, actor);
    })
}
