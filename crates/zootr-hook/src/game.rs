//! The hooks as the game runs them: the entry stubs the patches branch to,
//! the handlers behind them, and the reads of the game's memory they share.

use core::{arch::asm, arch::global_asm, slice};

use crate::{
    CAVE, Credit, Credited, DEV, Found, HEADER, Header, ICE_TRAPS, LIST, List, OFFSETS, Shared,
    TABLE, Table, fnv, keys, nearest,
};

mod arrows;
mod bunny;
mod chests;
mod dev;
mod freeze;
mod look;
mod menu;
mod refill;
mod rt;
mod swap;

use chests::{drop_value, level_for, owned, record, recreate, rewrite};

type Childhood = unsafe extern "C" fn(usize) -> bool;
type Setter = unsafe extern "C" fn(*mut u8, *const *const u8, *const *const u8);
type Reader = unsafe extern "C" fn(*mut u8, *const u8, usize, usize) -> usize;
/// The game's name hash, as the nock and the chest list use it.
type Hash = unsafe extern "C" fn(*const u8, u32) -> u32;

/// The argument the chest's get event gives and shows.
const KEY: &[u8] = b"ContentName";
/// Longer than any actor name, so a runaway string stops here.
const MAX: usize = 128;

// The call sites `bl` to content_name at +0, drop_actor at +4, run_speed
// at +24, the controller reads to +32, +36 and +40, and seed_guide at +68. The hooked
// functions `b` to +8, +12, +16, +20, +28, +44, +48, +52, +56, +60, +64,
// +72, +76 and +80 in place of their pacibsp.
// The opening node is in x20 at both content_name call sites.
//
// A function hook keeps every argument register and the link register,
// calls its handler, runs the pacibsp it replaced with the same sp, and
// goes on through its slot at `SLOTS`, where the patch writes a `b`
// past that pacibsp. A controller read calls the game's `GetNpadStates`
// stub through its own slot, where the patch writes a `b` to the stub.
// Every branch is direct, so a build that translates the game's code
// ahead of time can follow each one: it lifts an import stub as an
// import, never as code a pointer could reach.
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
    // A hook that runs its handler after the function returns, for a
    // function called with no stack arguments: it calls the function
    // through the pacibsp and slot, keeps its result, hands the handler
    // that result and the function's first argument, and then returns to
    // the function's caller.
    ".macro function_hook_after handler, resume",
    "stp x29, x30, [sp, #-0x30]!",
    "mov x29, sp",
    "str x0, [sp, #0x18]",
    "bl 1f",
    "str x0, [sp, #0x10]",
    "str q0, [sp, #0x20]",
    "ldr x1, [sp, #0x18]",
    "bl \\handler",
    "ldr q0, [sp, #0x20]",
    "ldr x0, [sp, #0x10]",
    "ldp x29, x30, [sp], #0x30",
    "ret",
    "1:",
    "pacibsp",
    "b \\resume",
    ".endm",
    // A controller read: calls the game's stub through `stub`, then hands
    // the states it wrote and their count to `npad_states`.
    ".macro npad_read stub",
    "stp x29, x30, [sp, #-0x20]!",
    "mov x29, sp",
    "str x0, [sp, #0x10]",
    "bl \\stub",
    "mov x1, x0",
    "ldr x0, [sp, #0x10]",
    "bl npad_states",
    "ldp x29, x30, [sp], #0x20",
    "ret",
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
    "b npad_full_entry",
    "b npad_handheld_entry",
    "b npad_dual_entry",
    "b items_built_entry",
    "b items_left_entry",
    "b ui_update_entry",
    "b taken_entry",
    "b nocked_entry",
    "b emptied_entry",
    "b seed_guide",
    "b equipment_built_entry",
    "b equipment_left_entry",
    "b resident_made_entry",
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
    "items_built_resume:",
    "brk #0",
    "items_left_resume:",
    "brk #0",
    "ui_update_resume:",
    "brk #0",
    "taken_resume:",
    "brk #0",
    "nocked_resume:",
    "brk #0",
    "emptied_resume:",
    "brk #0",
    "equipment_built_resume:",
    "brk #0",
    "equipment_left_resume:",
    "brk #0",
    "resident_made_resume:",
    "brk #0",
    "npad_full_stub:",
    "brk #0",
    "npad_handheld_stub:",
    "brk #0",
    "npad_dual_stub:",
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
    "taken_entry:",
    "function_hook taken, taken_resume",
    "nocked_entry:",
    "function_hook nocked, nocked_resume",
    "emptied_entry:",
    "function_hook_after emptied, emptied_resume",
    "npad_full_entry:",
    "npad_read npad_full_stub",
    "npad_handheld_entry:",
    "npad_read npad_handheld_stub",
    "npad_dual_entry:",
    "npad_read npad_dual_stub",
    "items_built_entry:",
    "function_hook items_built, items_built_resume",
    "items_left_entry:",
    "function_hook items_left, items_left_resume",
    "ui_update_entry:",
    "function_hook_after ui_update, ui_update_resume",
    "equipment_built_entry:",
    "function_hook equipment_built, equipment_built_resume",
    "equipment_left_entry:",
    "function_hook equipment_left, equipment_left_resume",
    "resident_made_entry:",
    "function_hook resident_made, resident_made_resume",
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

/// `main` and the table, or `None` before one is written: the patch that
/// builds the hook into the game leaves the table to a cheat, which may
/// land after the hook first runs. With none, every hook leaves the game
/// as it is. Any other magic, or a table that doesn't parse, is a broken
/// install, and stops the game.
fn table() -> Option<(usize, Table<'static>)> {
    let main = main();
    // SAFETY: `TABLE` is zero padding the cheat writes, 8 byte aligned.
    let header = unsafe { &*((main + TABLE as usize) as *const Header) };
    match header.found() {
        Found::Nothing => return None,
        Found::Broken => rt::stop(b"BADTABLE"),
        Found::Table => {}
    }
    let at = (header as *const Header).cast::<u8>();
    let size = (header.size + header.credits + header.slots + header.look) as usize;
    let body = unsafe { slice::from_raw_parts(at.add(HEADER), size) };
    match Table::parse(header, body) {
        Some(table) => Some((main, table)),
        None => rt::stop(b"BADTABLE"),
    }
}

/// Run `f` for a function hook, unless there's no table yet.
fn hooked(f: impl FnOnce(usize, &Table)) {
    if let Some((main, table)) = table() {
        f(main, &table);
    }
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

/// The game's function at `off` in `main`, as the fn pointer type `F`.
///
/// # Safety
/// `off` is a function of type `F` in this build of `main`.
#[inline(always)]
unsafe fn func<F: Copy>(main: usize, off: u32) -> F {
    const { assert!(size_of::<F>() == size_of::<usize>()) };
    unsafe { core::mem::transmute_copy(&(main + off as usize)) }
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
    let table = table();
    let main = main();
    // SAFETY: the offsets were found in this build of `main`.
    unsafe {
        let setter: Setter = func(main, OFFSETS.setter);
        let Some((_, table)) = table else {
            return setter(request, key, value);
        };
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
    // SAFETY: the offsets were found in this build of `main`.
    unsafe {
        if let Some((main, table)) = table()
            && let Some(value) = drop_value(placement as usize)
        {
            rewrite(main, &table, value);
        }
        let reader: Reader = func(main(), OFFSETS.reader);
        reader(out, placement, a2, a3)
    }
}

/// Link is about to nock an arrow or seed with `user`, his EquipmentUser
/// component: he nocks the drawn bow's own.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn nocked(user: usize) {
    hooked(|main, _| unsafe { arrows::nocked(main, user) })
}

/// Link drew a bow with no ammo, and the game picked its message by his
/// age. Only the hooked function's return reaches this.
#[unsafe(no_mangle)]
unsafe extern "C" fn emptied(_: usize, node: usize) {
    hooked(|main, _| unsafe { arrows::emptied(main, node) })
}

/// Called in place of the child test the bow draw asks for its guide, with
/// Link: true picks the Slingshot's ("Shoot Seed"), false the Bow's
/// ("Shoot Arrow"). It follows the drawn bow's projectile, not his age.
///
/// # Safety
/// Only the game calls this, from the rewritten call site.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seed_guide(link: usize) -> bool {
    unsafe {
        if let Some((main, _)) = table()
            && let Some(seeds) = arrows::shoots_seeds(main, link)
        {
            return seeds;
        }
        let child: Childhood = func(main(), OFFSETS.childhood);
        child(link)
    }
}

/// The game makes the resident actors (the player of an age, the
/// cameras, Navi) with their manager, on every area load: keep it for an
/// age swap in place.
#[unsafe(no_mangle)]
extern "C" fn resident_made(manager: usize) {
    hooked(|main, _| {
        let resident = &shared(main).resident;
        resident.store(manager, core::sync::atomic::Ordering::Release)
    })
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
            menu::granted(main, table, added);
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
            if !shared(main).with(|l| l.noticed(fnv(added))) {
                credit(main, table, added, true);
            }
        }
    })
}

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
    let note = Credited {
        name: fnv(added),
        picked,
    };
    if !shared(main).with(|l| l.first_credit(note)) {
        return;
    }
    if credit == Credit::Freeze {
        shared(main)
            .traps
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
            Credit::Freeze => {}
        }
    }
}

/// Link takes a pickup into the pouch, window or not: set the flag of the
/// twinned holder it dropped from. A repeat take opens no window, so
/// `picked` misses it, and skips the pickup's own event too.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn taken(pouched: usize) {
    hooked(|main, table| unsafe {
        if table.spots == 0 {
            return;
        }
        let Some(actor) = at(pouched, OWNER) else {
            return;
        };
        let Some(name) = at(actor, ACTOR_NAME).and_then(|p| name(p as *const u8)) else {
            return;
        };
        let lies = *((actor + dev::POSITION) as *const [f32; 3]);
        let Some(gd) = dev::single(main, table.offsets.gamedata) else {
            return;
        };
        // A flag missing from GameData counts as picked, so nothing sets it.
        let picked = |flag| dev::get_bool(main, table.offsets, gd, flag) != Some(false);
        // A key missing from GameData counts as broken, as before keys
        // were asked.
        let whole = |placed| key_entries(gd).and_then(|e| keys::value(e, placed)) == Some(0);
        let spots = (0..table.spots).map_while(|i| dev::spot(main, table.offsets, gd, i));
        if let Some(spot) = nearest(spots, name, lies, picked, whole) {
            dev::set_bool(main, table.offsets, gd, spot.flag, true);
            if spot.twin != 0
                && let Some(entries) = key_entries(gd)
            {
                // Breaks the twin as the holder is.
                keys::follow(entries, spot.placed, spot.twin);
            }
        }
    })
}

/// The GameData manager `gd`'s keys, laid end to end.
unsafe fn key_entries(gd: usize) -> Option<&'static mut [u8]> {
    let count = unsafe { *((gd + keys::KEYS) as *const u32) } as usize;
    let at = unsafe { at(gd, keys::KEYS + 8) }?;
    // SAFETY: the manager's own sorted list, `count` entries long.
    Some(unsafe { slice::from_raw_parts_mut(at as *mut u8, count * keys::KEY_STRIDE) })
}

/// The pickup a pouch content belongs to.
const OWNER: usize = 0x18;
/// An actor's name.
const ACTOR_NAME: usize = 0x260;

/// Link is about to update: hold what the dev switches hold, run the app's
/// request, if there is one, and between events forget the last credit.
///
/// # Safety
/// Only the hooked function's entry calls this.
#[unsafe(no_mangle)]
unsafe extern "C" fn link_update(actor: usize) {
    hooked(|main, table| unsafe {
        if table.flags & DEV != 0 {
            refill::refill(main, &table, actor);
            dev::serve(main, table.offsets, actor);
        }
        if table.flags & ICE_TRAPS != 0 {
            look::fit(main, &table);
            freeze::freeze(main, &table, actor);
        }
        if dev::playing(main, table.offsets) == Some(false) {
            shared(main).with(List::idle);
        }
    })
}
