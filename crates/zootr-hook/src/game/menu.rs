//! The slot Link cycles on the pause menu's Items page. The controller
//! reads go through `npad_states`, which runs the cycle and keeps its
//! presses from the game. The Items page's build and leaving say when the
//! page is up, and after each UI update `ui_update` makes the slot's cell
//! show what the store says, on the menu's own thread.
//! See `docs/research/in-game-ui.md`.

use core::mem::transmute;

use super::{at, chests, dev, hooked, name, shared, table};
use crate::Table;
use crate::ui::{Buttons, Seen};

/// The Items page's cells and their count.
const CELLS: usize = 0x78;
const COUNT: usize = 0x70;
/// A cell's button, the button's state, and its states while the cursor
/// is on it: focusing (as a refresh leaves it for a few frames) and
/// focused. Unfocusing is the cursor leaving, and 0 is away.
const BUTTON: usize = 0xe8;
const STATE: usize = 0x38;
const CURSOR_ON: [u8; 2] = [1, 3];
/// The cell's copy of its actor's name.
const CELL_NAME: usize = 0x90;
/// One controller state as `GetNpadStates` writes it, its buttons, and
/// its left stick's x (an `i32`, y follows).
const NPAD_STATE: usize = 0x28;
const NPAD_BUTTONS: usize = 8;
const NPAD_STICK_X: usize = 0x10;

type GetStates = unsafe extern "C" fn(*mut u8, i32, *const u32) -> i32;
type CellSet = unsafe extern "C" fn(usize, *const *const u8, i32, i32, i32);
type Focus = unsafe extern "C" fn(usize, usize, usize, usize);

/// Called in place of the controller device's `GetNpadStates` calls, with
/// their arguments and which `style` the call reads. The newest state
/// comes first.
///
/// # Safety
/// Only the game calls this, from the rewritten call sites.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn npad_states(
    states: *mut u8,
    count: i32,
    id: *const u32,
    style: u32,
) -> i32 {
    let Some((main, table)) = table() else {
        return 0;
    };
    let o = table.offsets;
    let stub = match style {
        0 => o.npad_full,
        1 => o.npad_handheld,
        _ => o.npad_dual,
    };
    // SAFETY: the offsets were found in this build of `main`.
    let n = unsafe { transmute::<usize, GetStates>(main + stub as usize)(states, count, id) };
    let Some(slot) = table.slot.filter(|_| n > 0) else {
        return n;
    };
    let state = |i: i32| states as usize + i as usize * NPAD_STATE;
    // SAFETY: the call wrote `n` states.
    let now = Buttons(unsafe { *((state(0) + NPAD_BUTTONS) as *const u64) } as u32);
    let (hide, cycling) = shared(main)
        .ui
        .with(|s| (s.read(now, &slot.actors), s.cycling()));
    for i in 0..n {
        // SAFETY: as above.
        unsafe {
            *((state(i) + NPAD_BUTTONS) as *mut u64) &= !u64::from(hide.0);
            // The menu moves on the stick's position, not only its
            // direction bits, and left and right belong to the cycle.
            // Up and down still reach the menu, to leave the slot.
            if cycling {
                *((state(i) + NPAD_STICK_X) as *mut i32) = 0;
            }
        }
    }
    n
}

/// The Items page is being built: it's up.
#[unsafe(no_mangle)]
extern "C" fn items_built(page: usize) {
    hooked(|main, _| {
        shared(main).ui.with(|s| {
            s.page = page;
            s.up = true;
        })
    })
}

/// The Items page is being left, for another page or as the menu closes.
#[unsafe(no_mangle)]
extern "C" fn items_left(page: usize) {
    hooked(|main, _| shared(main).ui.with(|s| s.up &= s.page != page))
}

/// The UI has updated for this frame.
#[unsafe(no_mangle)]
extern "C" fn ui_update() {
    hooked(|main, table| {
        // SAFETY: the table's offsets are this build's.
        unsafe { show(main, table) };
    })
}

/// Look at the Items page and make the slot's cell show what the store
/// says. A focused cell's button is refreshed as the cursor would, so the
/// item detail panel follows. One not focused never is: focusing it would
/// put a second cursor on the page.
///
/// # Safety
/// The table's offsets are this build's.
unsafe fn show(main: usize, table: &Table) -> Option<()> {
    let slot = table.slot?;
    let ui = &shared(main).ui;
    let page = ui.with(|s| {
        s.seen.on_slot &= s.up;
        s.up.then_some(s.page)
    })?;
    let o = table.offsets;
    unsafe {
        let cell = cell(page, slot.cell)?;
        let button = at(cell, BUTTON)?;
        let focused = CURSOR_ON.contains(&*((button + STATE) as *const u8));
        let owns = chests::owned(main, o)?;
        let held = (0..slot.actors.ages.len())
            .filter(|&i| slot.actor(i).is_some_and(|(n, _)| owns(n)))
            .fold(0, |held, i| held | 1 << i);
        let gd = dev::single(main, o.gamedata)?;
        let child = dev::get_bool(main, o, gd, slot.age_flag)?;
        let seen = Seen {
            on_slot: focused,
            child,
            held,
        };
        let want = ui.with(|s| {
            s.seen = seen;
            s.showing(&slot.actors)
        })?;
        let (actor, kind) = slot.actor(want)?;
        if name(at(cell, CELL_NAME)? as *const u8) == Some(actor) {
            return Some(());
        }
        let set: CellSet = transmute(main + o.cell_set as usize);
        set(cell, &actor.as_ptr(), i32::from(kind), 0, 1);
        if focused {
            for refresh in [o.unfocus, o.focus] {
                transmute::<usize, Focus>(main + refresh as usize)(button, 0, 0, 0);
            }
        }
    }
    Some(())
}

/// The Items page's cell `i`.
///
/// # Safety
/// `page` is the Items page.
unsafe fn cell(page: usize, i: u8) -> Option<usize> {
    let count = unsafe { *((page + COUNT) as *const u32) };
    (u32::from(i) < count).then_some(())?;
    unsafe { at(at(page, CELLS)?, 8 * usize::from(i)) }
}

/// Link just got `added`: if the slot holds it, it's his choice.
pub(super) fn granted(main: usize, table: &Table, added: &[u8]) {
    let Some(slot) = table.slot else {
        return;
    };
    if let Some(i) = slot.index(added) {
        shared(main).ui.with(|s| s.granted(i, &slot.actors));
    }
}
