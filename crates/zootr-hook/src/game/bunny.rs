//! The Bunny Hood setting: Link runs half again as fast while he wears
//! it. The run node asks the character controller for its speed each
//! frame through one call, which the patch sends here in every seed, and
//! only a seed with the setting changes the speed. Rolls, stops and the rest ask from other calls and
//! stay as they are.

use core::mem::transmute;

use super::{at, main, table};
use crate::{BUNNY_HOOD, OFFSETS};

/// How much faster, Ship of Harkinian's factor.
const FAST: f32 = 1.5;

/// The mask Player shows, which the game sets each frame from the head
/// slot by the worn actor's name: 0 for none or a mask put away with the
/// D-pad, 5 for the Bunny Hood. It lives in a part of the component at
/// `+0x358` of Player's components (`+0x7c0`, kind at `+0x34`).
const MASK: usize = 0x7f4;
const BUNNY: u32 = 5;

/// The speed setter: Player's move state, the direction to move in, and
/// metres a frame. It reads no other register.
type Setter = unsafe extern "C" fn(usize, *const f32, f32);

/// Called in place of the run node's speed request, with its arguments.
///
/// # Safety
/// Only the game calls this, from the rewritten call site.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn run_speed(state: usize, dir: *const f32, speed: f32) {
    let on = table().is_some_and(|(_, t)| t.flags & BUNNY_HOOD != 0);
    let speed = match on && unsafe { wears_hood(state) } {
        true => speed * FAST,
        false => speed,
    };
    // SAFETY: the offset was found in this build of `main`.
    unsafe {
        let setter: Setter = transmute(main() + OFFSETS.run_setter as usize);
        setter(state, dir, speed)
    }
}

/// Whether the actor moving by `state` shows the hood.
///
/// # Safety
/// `state` is Player's move state, as the run node passes it.
unsafe fn wears_hood(state: usize) -> bool {
    (unsafe { shown_mask(state) }) == Some(BUNNY)
}

/// The mask kind shown, see [`MASK`].
///
/// # Safety
/// As [`wears_hood`].
unsafe fn shown_mask(state: usize) -> Option<u32> {
    unsafe {
        let actor = at(state, 0x18)?;
        let model = at(at(actor, 0x270)?, 0x358)?;
        Some(*((model + MASK) as *const u32))
    }
}
