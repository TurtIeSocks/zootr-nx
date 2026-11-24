//! The slots Link cycles on the pause menu's Items and Equipment pages.
//! The controller reads go through `npad_states`, which runs each slot's
//! cycle and keeps its presses from the game. Each page's build and
//! leaving say when it's up, and after each UI update `ui_update` makes
//! each slot's cell show what the store says, on the menu's own thread.
//! See `docs/research/in-game-ui.md`.

use super::{at, chests, dev, func, hooked, name, shared, table};
use crate::Table;
use crate::ui::{Buttons, MAX_SLOTS, Page, PageUp, Seen};

/// Where a page keeps what the hook reads.
struct Layout {
    /// The page's cells and their count.
    cells: usize,
    count: usize,
    /// A cell's button.
    button: usize,
    /// The cell's copy of its actor's name.
    name: usize,
    /// The animation whose frame says whether the cell's item is usable
    /// or worn (on a button).
    state: usize,
    /// The cell layout's pane that draws the cursor's border.
    cursor_line: usize,
}

impl Page {
    fn layout(self) -> Layout {
        match self {
            Page::Items => Layout {
                cells: 0x78,
                count: 0x70,
                button: 0xe8,
                name: 0x90,
                state: 0x48,
                cursor_line: 13,
            },
            Page::Equipment => Layout {
                cells: 0x50,
                count: 0x48,
                button: 0xd8,
                name: 0x88,
                state: 0x40,
                cursor_line: 15,
            },
        }
    }
}

/// An Equipment cell's icon animation, its frame the item's place in the
/// page's icon list, its picture (the texture panes `0xa737e0` loads by
/// name), and the size of its name buffer.
const EQUIPMENT_ICON: usize = 0x58;
const EQUIPMENT_PICTURE: usize = 0x80;
const NAME_SIZE: usize = 0x90;
/// A button's state, and its states while the cursor is on it: focusing
/// (as a refresh leaves it for a few frames) and focused. Unfocusing is
/// the cursor leaving, and 0 is away.
const STATE: usize = 0x38;
const CURSOR_ON: [u8; 2] = [1, 3];
/// The state frames both pages give a held item its age can use, and one
/// worn or on a button. The Items build leaves the slot on its own pick's
/// frame, grey when that's another age's item.
const USABLE: f32 = 1.0;
const ON_BUTTON: f32 = 2.0;
/// The cell's layout, its panes (count, array, each `PANE` long). A
/// pane's material tints it, this cell's alone.
const CELL_LAYOUT: usize = 0x20;
const LAYOUT_PANES: usize = 0x80;
const PANE_COUNT: usize = 0x13c;
const PANE_ARRAY: usize = 0x100;
const PANE: usize = 0xb0;
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

type CellSet = unsafe extern "C" fn(usize, *const *const u8, i32, i32, i32);
type Focus = unsafe extern "C" fn(usize, usize, usize, usize);
type AnimFrame = unsafe extern "C" fn(usize, f32);
type Equipped = unsafe extern "C" fn(usize, u8, u32) -> i32;
type Worn = unsafe extern "C" fn(usize, *const *const u8) -> u32;
type Redraw = unsafe extern "C" fn(usize, u32);
type Icon = unsafe extern "C" fn(usize, *const *const u8, u32, u32);

/// Called after each of the controller device's `GetNpadStates` calls,
/// with the states it wrote and their count, which it hands back. The
/// newest state comes first.
///
/// # Safety
/// Only the game calls this, from the rewritten call sites.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn npad_states(states: *mut u8, n: i32) -> i32 {
    let Some((main, table)) = table() else {
        return n;
    };
    let slots = table.slots.map(|s| s.map(|s| s.actors));
    if n <= 0 || slots.iter().all(Option::is_none) {
        return n;
    }
    let state = |i: i32| states as usize + i as usize * NPAD_STATE;
    // SAFETY: the call wrote `n` states.
    let now = Buttons(unsafe { *((state(0) + NPAD_BUTTONS) as *const u64) } as u32);
    let (hide, cycling) = shared(main).ui.with(|s| (s.read(now, &slots), s.cycling()));
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
    built(Page::Items, page);
}

/// The Items page is being left, for another page or as the menu closes.
#[unsafe(no_mangle)]
extern "C" fn items_left(page: usize) {
    left(Page::Items, page);
}

/// The Equipment page is being built: it's up.
#[unsafe(no_mangle)]
extern "C" fn equipment_built(page: usize) {
    built(Page::Equipment, page);
}

/// The Equipment page is being left.
#[unsafe(no_mangle)]
extern "C" fn equipment_left(page: usize) {
    left(Page::Equipment, page);
}

fn built(which: Page, page: usize) {
    hooked(|main, _| {
        shared(main)
            .ui
            .with(|s| s.pages[which as usize] = PageUp { page, up: true })
    })
}

fn left(which: Page, page: usize) {
    hooked(|main, _| {
        shared(main).ui.with(|s| {
            let p = &mut s.pages[which as usize];
            p.up &= p.page != page;
        })
    })
}

/// The UI has updated for this frame.
#[unsafe(no_mangle)]
extern "C" fn ui_update() {
    hooked(|main, table| {
        if table.flags & crate::DEV != 0 {
            // SAFETY: the table's offsets are this build's.
            unsafe { super::swap::step(main, table.offsets) };
        }
        for i in 0..MAX_SLOTS {
            // SAFETY: the table's offsets are this build's.
            unsafe { show(main, table, i) };
        }
    })
}

/// Look at slot `i`'s page and make its cell show what the store says. A
/// focused cell's button is refreshed as the cursor would, so the item
/// detail panel follows. One not focused never is: focusing it would put
/// a second cursor on the page.
///
/// Far code: it runs only while a page is up.
///
/// # Safety
/// The table's offsets are this build's.
#[inline(never)]
#[unsafe(link_section = ".far")]
unsafe fn show(main: usize, table: &Table, i: usize) -> Option<()> {
    let slot = table.slots[i]?;
    let ui = &shared(main).ui;
    let page = ui.with(|s| {
        let PageUp { page, up } = s.pages[slot.page as usize];
        s.slots[i].seen.on_slot &= up;
        up.then_some(page)
    })?;
    let o = table.offsets;
    let l = slot.page.layout();
    unsafe {
        let cell = cell(page, &l, slot.cell)?;
        let button = at(cell, l.button)?;
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
            let state = &mut s.slots[i];
            state.seen = seen;
            (state.showing(&slot.actors), state.cycling())
        });
        tint(cell, l.cursor_line, if cycling { CYCLING } else { PLAIN })?;
        let (actor, kind) = slot.actor(want?)?;
        if name(at(cell, l.name)? as *const u8) == Some(actor) {
            return Some(());
        }
        let on_button = match slot.page {
            Page::Items => {
                let set: CellSet = func(main, o.cell_set);
                set(cell, &actor.as_ptr(), i32::from(kind), 0, 1);
                let equipped: Equipped = func(main, o.equipped);
                equipped(at(main, o.manager as usize)?, child.into(), 1) == i32::from(kind)
            }
            Page::Equipment => {
                let size = *((cell + NAME_SIZE) as *const u32) as usize;
                let to = at(cell, l.name)? as *mut u8;
                let n = actor.len().min(size.checked_sub(1)?);
                let shown = core::slice::from_raw_parts(to, n + 1);
                if shown[..n] != actor[..n] || shown[n] != 0 {
                    core::ptr::copy_nonoverlapping(actor.as_ptr(), to, n);
                    *to.add(n) = 0;
                    // The picture is a texture loaded by name, so only on a change.
                    let icon: Icon = func(main, o.icon);
                    icon(at(cell, EQUIPMENT_PICTURE)?, &to.cast_const(), 0, 0);
                }
                seek(main, o, cell, EQUIPMENT_ICON, f32::from(kind))?;
                let worn: Worn = func(main, o.worn);
                worn(dev::single(main, o.pouch_menu)?, &to.cast_const()) & 1 != 0
            }
        };
        seek(
            main,
            o,
            cell,
            l.state,
            if on_button { ON_BUTTON } else { USABLE },
        )?;
        if slot.page == Page::Equipment {
            // The Equipment page's own refresh applies the new frames so.
            func::<Redraw>(main, o.redraw)(cell, 0);
        }
        if focused {
            for refresh in [o.unfocus, o.focus] {
                func::<Focus>(main, refresh)(button, 0, 0, 0);
            }
        }
    }
    Some(())
}

/// Seek the cell's animation at `anim` to `frame`, unless it's there.
///
/// # Safety
/// `cell` is a page's cell, and the offsets are this build's.
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
            func::<AnimFrame>(main, o.anim_frame)(anim, frame);
        }
    }
    Some(())
}

/// Tint the cell's cursor border, unless it's that colour.
///
/// # Safety
/// `cell` is a page's cell.
unsafe fn tint(cell: usize, cursor_line: usize, colour: [f32; 4]) -> Option<()> {
    unsafe {
        let panes = at(at(cell, CELL_LAYOUT)?, LAYOUT_PANES)?;
        (usize::from(*((panes + PANE_COUNT) as *const u16)) > cursor_line).then_some(())?;
        let line = at(panes, PANE_ARRAY)? + cursor_line * PANE;
        let now = (at(line, PANE_MATERIAL)? + MATERIAL_COLOUR) as *mut [f32; 4];
        if *now != colour {
            *now = colour;
        }
    }
    Some(())
}

/// Cell `i` of `page`, laid out as `l`.
///
/// # Safety
/// `page` is the page `l` is for.
unsafe fn cell(page: usize, l: &Layout, i: u8) -> Option<usize> {
    let count = unsafe { *((page + l.count) as *const u32) };
    (u32::from(i) < count).then_some(())?;
    unsafe { at(at(page, l.cells)?, 8 * usize::from(i)) }
}

/// Link just got `added`: if a slot holds it, it's his choice there.
pub(super) fn granted(main: usize, table: &Table, added: &[u8]) {
    for (n, slot) in table.slots.iter().enumerate() {
        let Some((slot, i)) = slot.and_then(|s| Some((s, s.index(added)?))) else {
            continue;
        };
        shared(main).ui.with(|s| s.slots[n].choose(i, &slot.actors));
    }
}
