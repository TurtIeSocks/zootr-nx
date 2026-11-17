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
/// The cell's copy of its actor's name, and its icon's animation.
const CELL_NAME: usize = 0x90;
const CELL_ICON: usize = 0x48;
/// The icon frames the Items build gives a held item its age can use, and
/// one on a button. The build leaves the slot on its own pick's frame,
/// grey when that's another age's item.
const USABLE: f32 = 1.0;
const ON_BUTTON: f32 = 2.0;
/// The cell's layout, its panes (count, array, each `PANE` long), and
/// the cursor line, its border, among them. A pane's material
/// tints it, this cell's alone.
const CELL_LAYOUT: usize = 0x20;
const LAYOUT_PANES: usize = 0x80;
const PANE_COUNT: usize = 0x13c;
const PANE_ARRAY: usize = 0x100;
const PANE: usize = 0xb0;
const CURSOR_LINE: usize = 13;
const PANE_MATERIAL: usize = 8;
const MATERIAL_COLOUR: usize = 0x20;
/// The border while Link cycles the slot, and as the game draws it.
const CYCLING: [f32; 4] = [0.2, 0.6, 1.0, 1.0];
const PLAIN: [f32; 4] = [1.0; 4];
/// An animation's frame controller, and the frame it's on.
const ANIM_CONTROL: usize = 0x20;
const ANIM_FRAME: usize = 0x18;
/// One controller state as `GetNpadStates` writes it, its buttons, and
/// its left stick's x (an `i32`, y follows).
const NPAD_STATE: usize = 0x28;
const NPAD_BUTTONS: usize = 8;
const NPAD_STICK_X: usize = 0x10;

type GetStates = unsafe extern "C" fn(*mut u8, i32, *const u32) -> i32;
type CellSet = unsafe extern "C" fn(usize, *const *const u8, i32, i32, i32);
type Focus = unsafe extern "C" fn(usize, usize, usize, usize);
type AnimFrame = unsafe extern "C" fn(usize, f32);
type Equipped = unsafe extern "C" fn(usize, u8, u32) -> i32;

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
        let (want, cycling) = ui.with(|s| {
            s.seen = seen;
            (s.showing(&slot.actors), s.cycling())
        });
        tint(cell, if cycling { CYCLING } else { PLAIN })?;
        let (actor, kind) = slot.actor(want?)?;
        if name(at(cell, CELL_NAME)? as *const u8) == Some(actor) {
            return Some(());
        }
        let set: CellSet = transmute(main + o.cell_set as usize);
        set(cell, &actor.as_ptr(), i32::from(kind), 0, 1);
        let equipped: Equipped = transmute(main + o.equipped as usize);
        let on_button = equipped(at(main, o.manager as usize)?, child.into(), 1) == i32::from(kind);
        seek(
            main,
            o,
            cell,
            CELL_ICON,
            if on_button { ON_BUTTON } else { USABLE },
        )?;
        if focused {
            for refresh in [o.unfocus, o.focus] {
                transmute::<usize, Focus>(main + refresh as usize)(button, 0, 0, 0);
            }
        }
    }
    Some(())
}

/// Seek the cell's animation at `anim` to `frame`, unless it's there.
///
/// # Safety
/// `cell` is an Items page cell, and the offsets are this build's.
unsafe fn seek(
    main: usize,
    o: &crate::Offsets,
    cell: usize,
    anim: usize,
    frame: f32,
) -> Option<()> {
    unsafe {
        let anim = at(cell, anim)?;
        if *((at(anim, ANIM_CONTROL)? + ANIM_FRAME) as *const f32) != frame {
            transmute::<usize, AnimFrame>(main + o.anim_frame as usize)(anim, frame);
        }
    }
    Some(())
}

/// Tint the cell's cursor border, unless it's that colour.
///
/// # Safety
/// `cell` is an Items page cell.
unsafe fn tint(cell: usize, colour: [f32; 4]) -> Option<()> {
    unsafe {
        let panes = at(at(cell, CELL_LAYOUT)?, LAYOUT_PANES)?;
        (usize::from(*((panes + PANE_COUNT) as *const u16)) > CURSOR_LINE).then_some(())?;
        let line = at(panes, PANE_ARRAY)? + CURSOR_LINE * PANE;
        let now = (at(line, PANE_MATERIAL)? + MATERIAL_COLOUR) as *mut [f32; 4];
        if *now != colour {
            *now = colour;
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
