//! Swap Link's age where he stands (`Ask::AgeHere`), with the game's own
//! resident actor routines and no area load. An area load with an age
//! change makes the new age's player (create, which has the old one go)
//! and wakes it at the arrival point. This does the same at Link's place:
//! create, wait for the new Link, keep the camera on him, wake him, all
//! under the area load's `SceneTransition` pause (the camera stays put
//! only under it). One step a frame, from the UI's update, since Link's
//! own update stops with him. Spec:
//! `docs/superpowers/specs/2026-11-23-age-swap-in-place-design.md`. Found
//! by OoT Character Select's field swap, `docs/research/character-select.md`.

use core::sync::atomic::Ordering;

use super::dev::{CHILD, POSITION, get_bool, idle, set_child, single};
use super::{at, func, shared};
use crate::traps::COMPONENTS;
use crate::{Mailbox, Offsets, Outcome, Swap};

type Create = unsafe extern "C" fn(usize, u32) -> u32;
type Wake = unsafe extern "C" fn(usize, *const [f32; 3], *const [f32; 9]) -> u32;
type Pause = unsafe extern "C" fn(*mut [usize; 5]);

/// The pause an area load holds: the camera's update doesn't run under it,
/// so it can't see the new Link before [`keep_camera`] points it at him.
static SCENE_TRANSITION: &[u8] = b"SceneTransition\0";

/// The stages: none, create, wait for the new Link, wake him.
const IDLE: u32 = 0;
const CREATE: u32 = 1;
const WAIT: u32 = 2;
const WAKE: u32 = 3;
/// Frames a stage may take, about 10 s at 30 fps: the new Link's model
/// loads in about 2.
const LIMIT: u32 = 300;

/// The manager's lists, each a count then its entries: common, child,
/// adult. An entry is 0x70 bytes: its actor link at +8, the actor's id at
/// +0x10. The link holds the actor at +0x40 while its id at +0x48 matches.
const COMMON: usize = 0x28;
const CHILD_LIST: usize = 0x38;
const ADULT_LIST: usize = 0x48;
const ENTRY: usize = 0x70;
/// An actor's state, asleep at 3 or 6, and its rotation, a 3x3 matrix.
const STATE: usize = 0x2c;
const ROTATION: usize = 0x328;
/// The camera's own component and the player's camera target part among
/// an actor's components, and where the camera keeps the target part it
/// followed last. A new part there makes it snap behind Link.
const CAMERA: usize = 0x360;
const TARGET: usize = 0x368;
const FOLLOWED: usize = 0xfe0;

/// Start the swap from Link's update: keep his place, flip the age, and
/// leave the rest to [`step`]. `None` once started: [`step`] answers.
///
/// # Safety
/// `actor` is Link, the offsets this build's.
#[inline(never)]
#[unsafe(link_section = ".far")]
pub unsafe fn start(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Option<Outcome> {
    if let Err(busy) = unsafe { idle(main, o) } {
        return Some(busy);
    }
    let sh = shared(main);
    let manager = sh.resident.load(Ordering::Acquire);
    if manager == 0 {
        return Some(Outcome::NoResident);
    }
    let Some(gd) = (unsafe { single(main, o.gamedata) }) else {
        return Some(Outcome::NoGame);
    };
    let Some(child) = (unsafe { get_bool(main, o, gd, CHILD) }) else {
        return Some(Outcome::NoAge);
    };
    let started = sh.swap.with(|s| {
        if s.stage != IDLE {
            return false;
        }
        *s = Swap {
            stage: CREATE,
            child: u32::from(!child),
            asked: mail.taken.load(Ordering::Relaxed),
            manager,
            // SAFETY: Link's actor holds both.
            position: unsafe { *((actor + POSITION) as *const [f32; 3]) },
            rotation: unsafe { *((actor + ROTATION) as *const [f32; 9]) },
            ..Swap::default()
        };
        true
    });
    if !started {
        return Some(Outcome::Soon);
    }
    unsafe {
        pause(main, o.request_pause);
        set_child(main, o, gd, !child);
    }
    None
}

/// Request or release the `SceneTransition` pause through the game's own
/// pause node body, on a stand-in node: its type name's cell at +0x20,
/// its done flags at +0x18.
unsafe fn pause(main: usize, body: u32) {
    let name = [SCENE_TRANSITION.as_ptr() as usize];
    let mut node = [0, 0, 0, 0, name.as_ptr() as usize];
    let run: Pause = unsafe { func(main, body) };
    unsafe { run(&mut node) };
}

/// One step of a swap under way, each frame.
///
/// # Safety
/// The offsets are this build's.
#[inline(never)]
#[unsafe(link_section = ".far")]
pub unsafe fn step(main: usize, o: &Offsets) {
    let sh = shared(main);
    let s = sh.swap.with(|s| {
        s.frames += 1;
        *s
    });
    if s.stage == IDLE {
        return;
    }
    let next = match s.stage {
        CREATE => {
            let create: Create = unsafe { func(main, o.resident_create) };
            (unsafe { create(s.manager, s.child) } & 1 == 1).then_some(WAIT)
        }
        WAIT => unsafe { asleep(s.manager, s.child == 1) }.map(|_| WAKE),
        _ => {
            if let Some(link) = unsafe { asleep(s.manager, s.child == 1) } {
                unsafe { keep_camera(s.manager, link) };
            }
            let wake: Wake = unsafe { func(main, o.cancel_sleep) };
            (unsafe { wake(s.manager, &s.position, &s.rotation) } & 1 == 1).then_some(IDLE)
        }
    };
    match next {
        Some(IDLE) => unsafe { finish(main, o, &s) },
        Some(stage) => sh.swap.with(|w| (w.stage, w.frames) = (stage, 0)),
        None if s.frames > LIMIT => unsafe { give_up(main, o, &s) },
        None => {}
    }
}

/// The new Link, once he's there and asleep, waiting for his wake.
unsafe fn asleep(manager: usize, child: bool) -> Option<usize> {
    let list = if child { CHILD_LIST } else { ADULT_LIST };
    let link = unsafe { actor(manager, list, 0) }?;
    let state = unsafe { *((link + STATE) as *const u32) };
    matches!(state, 3 | 6).then_some(link)
}

/// Entry `i`'s live actor of the manager's list at `list`.
unsafe fn actor(manager: usize, list: usize, i: usize) -> Option<usize> {
    unsafe {
        let count = *((manager + list) as *const u32) as usize;
        if i >= count {
            return None;
        }
        let entry = at(manager, list + 8)? + i * ENTRY;
        let link = at(entry, 8)?;
        let id = *((entry + 0x10) as *const u32);
        let actor = at(link, 0x40)?;
        (id != u32::MAX && *((link + 0x48) as *const u32) == id).then_some(actor)
    }
}

/// Point the player camera at the new Link's target part right before the
/// wake, before it looks,
/// so it stays where it is instead of snapping behind him. The camera is
/// the first of the common residents.
unsafe fn keep_camera(manager: usize, link: usize) {
    unsafe {
        let Some(camera) = actor(manager, COMMON, 0) else {
            return;
        };
        let component = at(camera, COMPONENTS).and_then(|c| at(c, CAMERA));
        let part = at(link, COMPONENTS).and_then(|c| at(c, TARGET));
        if let (Some(component), Some(part)) = (component, part) {
            *((component + FOLLOWED) as *mut usize) = part;
        }
    }
}

/// The new Link never came: make the old age again, where he stood, and
/// say so once that's done. A second give-up just answers.
unsafe fn give_up(main: usize, o: &Offsets, s: &Swap) {
    if s.failed == 1 {
        return unsafe { finish(main, o, s) };
    }
    let back = s.child ^ 1;
    if let Some(gd) = unsafe { single(main, o.gamedata) } {
        unsafe { set_child(main, o, gd, back == 1) };
    }
    shared(main).swap.with(|w| {
        (w.stage, w.frames, w.child, w.failed) = (CREATE, 0, back, 1);
    });
}

/// Answer the request the swap started from, and end it.
unsafe fn finish(main: usize, o: &Offsets, s: &Swap) {
    let sh = shared(main);
    let outcome = if s.failed == 1 {
        Outcome::Stuck
    } else {
        Outcome::Done
    };
    sh.swap.with(|w| *w = Swap::default());
    // SAFETY: start requested it, once per swap.
    unsafe { pause(main, o.release_pause) };
    sh.dev.outcome.store(outcome as u32, Ordering::Relaxed);
    sh.dev.answered.store(s.asked, Ordering::Release);
}
