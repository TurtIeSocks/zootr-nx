//! The dev requests, run on Link's update. Each one proven first from
//! outside the game, see `docs/research/teleport.md`.

use core::arch::asm;
use core::sync::atomic::Ordering;

use super::rt::{memcpy, memset};
use super::{at, name, shared};
use crate::{
    ARG_ACTOR, ARG_CHALLENGE, ARG_EQUIP, ARG_GIVE, ARG_STEP, ARG_STEP_NAME, Ask, GUARD, Mailbox,
    NAME, Offsets, Outcome, play_event,
};

type Malloc = unsafe extern "C" fn(usize) -> usize;
type MakeArg = unsafe extern "C" fn(usize);
type Request = unsafe extern "C" fn(usize, usize) -> u64;
type SetLink = unsafe extern "C" fn(usize, usize, usize);
type Release = unsafe extern "C" fn(usize);
type GetBool = unsafe extern "C" fn(usize, *mut u64, u32) -> u32;
type SetBool = unsafe extern "C" fn(usize, u32, u32);
type SetTime = unsafe extern "C" fn(usize, u32);
type MoveTo = unsafe extern "C" fn(usize, *const f32);

/// GameData name hashes.
const CHILD: u32 = 0xcea2_20d3;
const LOADING: u32 = 0xef2b_60c2;
/// Left set by an area whose exit a teleport skips, it hides the
/// minimap.
const NO_MINIMAP: u32 = 0x9a4d_d779;

/// The event an entrance asks for to change maps.
const EVENT: &[u8] = b"Dm_SY_BancChangeGo\0";
/// The request block's size, with room to spare after it.
const BLOCK: usize = 0x1b20 + 0x100;
/// Argument kinds: a string pointer, a bool.
const TEXT: u32 = 1;
const BOOL: u32 = 4;
/// Where the block keeps its event name, its arguments, and the
/// two actor links.
const EVENT_NAME: usize = 6800;
const ARGS: usize = 8;
const ARG_COUNT: usize = 16;
const ARG_SIZE: usize = 200;
const ARG_VALUE: usize = 152;
const STARTER: usize = 0x1950;
const LINKS: [usize; 2] = [0x1968, 0x1950];
/// The event manager's playing event.
const PLAYING: usize = 104;
/// The world's clock.
const CLOCK: usize = 0x510;
/// Link's components, and the steps from them to his character
/// controller.
const COMPONENTS: usize = 624;
/// Link's position.
const POSITION: usize = 796;
const CONTROLLER: [usize; 4] = [80, 32, 16, 8];

pub unsafe fn serve(main: usize, o: &Offsets, actor: usize) {
    let mail = &shared(main).dev;
    let asked = mail.asked.load(Ordering::Acquire);
    if asked == mail.answered.load(Ordering::Relaxed)
        || !unsafe { is_player(main, o, actor) }
        || mail.taken.swap(asked, Ordering::AcqRel) == asked
    {
        return;
    }
    let outcome = match Ask::from_word(mail.ask) {
        Some(Ask::Teleport) => unsafe { teleport(main, o, mail, actor) },
        Some(Ask::Age) => unsafe { age(main, o, mail, actor) },
        Some(Ask::Time) => unsafe { time(main, o, mail.hour) },
        Some(Ask::Step) => unsafe { step(main, o, mail, actor) },
        Some(Ask::Give) => unsafe { give(main, o, mail, actor) },
        Some(Ask::Play) => unsafe { play(main, o, mail, actor) },
        Some(Ask::Warp) => unsafe { warp(main, o, mail, actor) },
        Some(Ask::Where) => unsafe { locate(mail, actor) },
        None => Outcome::Unknown,
    };
    mail.outcome.store(outcome as u32, Ordering::Relaxed);
    mail.answered.store(asked, Ordering::Release);
}

/// Whether `actor` is Link. See [`player`].
pub(super) unsafe fn is_player(main: usize, o: &Offsets, actor: usize) -> bool {
    unsafe { player(main, o, actor) }.is_some()
}

/// Link's player object, when `actor` is Link: the player's vtable, a
/// movement component that points back at it, and the resource `Player`.
pub(super) unsafe fn player(main: usize, o: &Offsets, actor: usize) -> Option<usize> {
    let vtable = |p: usize, v: u32| unsafe { *(p as *const usize) } == main + v as usize;
    unsafe {
        if !vtable(actor, o.player_vtable) {
            return None;
        }
        let movement = at(at(actor, 624)?, 800)?;
        let owner = movement - 0x1d50;
        let resource = at(owner, 16)?;
        let ok = at(movement, 24)? == actor
            && vtable(owner, o.move_vtable)
            && name((resource + 88) as *const u8) == Some(&b"Player"[..]);
        ok.then_some(owner)
    }
}

/// `[[main + slot]]`, a singleton's pointer.
pub(super) unsafe fn single(main: usize, slot: u32) -> Option<usize> {
    unsafe { at(at(main, slot as usize)?, 0) }
}

unsafe fn get_bool(main: usize, o: &Offsets, gd: usize, hash: u32) -> Option<bool> {
    let get: GetBool = unsafe { core::mem::transmute(main + o.get_bool as usize) };
    let mut out = 0;
    (unsafe { get(gd, &mut out, hash) } & 1 == 1).then_some(out & 1 == 1)
}

unsafe fn set_bool(main: usize, o: &Offsets, gd: usize, hash: u32, value: bool) {
    let set: SetBool = unsafe { core::mem::transmute(main + o.set_bool as usize) };
    unsafe { set(gd, value as u32, hash) }
}

/// The system counter and its ticks per second.
fn clock() -> (u64, u64) {
    let (now, rate): (u64, u64);
    // SAFETY: reads of the counter registers, open to user code.
    unsafe {
        asm!(
            "mrs {n}, cntvct_el0",
            "mrs {r}, cntfrq_el0",
            n = out(reg) now,
            r = out(reg) rate,
            options(nomem, nostack, preserves_flags),
        );
    }
    (now, rate)
}

/// Copy `text` to `to`, NUL included, if it fits in `cap`.
unsafe fn put(to: usize, cap: u32, text: &[u8]) -> bool {
    let fits = to != 0 && text.len() <= cap as usize;
    if fits {
        unsafe { memcpy(to as *mut u8, text.as_ptr(), text.len()) };
    }
    fits
}

/// Add an argument to the block, as an entrance does.
unsafe fn add(block: usize, key: &[u8], kind: u32, value: usize) -> bool {
    unsafe {
        let (cap, n) = (*(block as *const u32), *((block + ARG_COUNT) as *const u32));
        let Some(args) = at(block, ARGS).filter(|_| n < cap) else {
            return false;
        };
        let e = args + n as usize * ARG_SIZE;
        if !put(*((e + 8) as *const usize), *((e + 16) as *const u32), key) {
            return false;
        }
        match kind {
            BOOL => *((e + ARG_VALUE) as *mut u8) = value as u8,
            _ => *((e + ARG_VALUE) as *mut usize) = value,
        }
        *(e as *mut u32) = kind;
        *((block + ARG_COUNT) as *mut u32) = n + 1;
        true
    }
}

/// One argument of an event request: a name in `texts` (by index)
/// or a bool.
enum Arg {
    Text(&'static [u8], usize),
    Bool(&'static [u8], bool),
}

/// The event manager, refusing while the game isn't up, an event plays
/// or a map loads.
unsafe fn idle(main: usize, o: &Offsets) -> Result<usize, Outcome> {
    let (Some(events), Some(gd)) = (unsafe { (single(main, o.events), single(main, o.gamedata)) })
    else {
        return Err(Outcome::NoGame);
    };
    if unsafe { at(events, PLAYING) }.is_some() {
        return Err(Outcome::Event);
    }
    if unsafe { get_bool(main, o, gd, LOADING) } == Some(true) {
        return Err(Outcome::Loading);
    }
    Ok(events)
}

/// Ask the event manager for `event`, NUL ended, with `args` and
/// Link as its starter, refusing while an event plays or a map
/// loads. The game reads the block and its strings after the
/// request returns, so both go in one allocation that's never
/// freed.
unsafe fn request_map_change(
    main: usize,
    o: &Offsets,
    actor: usize,
    event: &[u8],
    texts: &[&[u8; NAME]],
    args: &[Arg],
) -> Outcome {
    let events = match unsafe { idle(main, o) } {
        Ok(events) => events,
        Err(busy) => return busy,
    };
    let call = |f: u32| main + f as usize;
    unsafe {
        let malloc: Malloc = core::mem::transmute(call(o.malloc));
        let make: MakeArg = core::mem::transmute(call(o.make_arg));
        let request: Request = core::mem::transmute(call(o.request));
        let set_link: SetLink = core::mem::transmute(call(o.set_link));
        let release: Release = core::mem::transmute(call(o.release_link));
        let block = malloc(BLOCK + texts.len() * NAME);
        if block == 0 {
            return Outcome::NotBuilt;
        }
        memset(block as *mut u8, 0, BLOCK);
        make(block);
        let strings = block + BLOCK;
        for (i, text) in texts.iter().enumerate() {
            memcpy((strings + i * NAME) as *mut u8, text.as_ptr(), NAME);
            *((strings + (i + 1) * NAME - 1) as *mut u8) = 0;
        }
        let mut built = put(
            *((block + EVENT_NAME) as *const usize),
            *((block + EVENT_NAME + 8) as *const u32),
            event,
        );
        for arg in args {
            built = built
                && match *arg {
                    Arg::Text(key, i) => add(block, key, TEXT, strings + i * NAME),
                    Arg::Bool(key, value) => add(block, key, BOOL, value as usize),
                };
        }
        let mut answer = 0;
        if built {
            set_link(block + STARTER, actor, 0);
            answer = request(events, block);
        }
        for link in LINKS {
            *((block + link) as *mut usize) = call(o.link_empty);
            release(block + link);
        }
        if !built {
            return Outcome::NotBuilt;
        }
        if answer == 0 {
            Outcome::Unanswered
        } else {
            Outcome::Done
        }
    }
}

/// A teleport: the map change an entrance asks for, held back for
/// [`GUARD`] seconds after the last. It names the dev switches false: a
/// switch the request leaves out keeps the value the last request gave
/// it, so after a quest step every teleport ran the step again.
unsafe fn teleport(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let (now, rate) = clock();
    let last = mail.last.load(Ordering::Relaxed);
    if last != 0 && now.wrapping_sub(last) < GUARD * rate {
        return Outcome::Soon;
    }
    let texts = [&mail.map, &mail.spot, &mail.arrive];
    let args = [
        Arg::Text(b"NextBanc\0", 0),
        Arg::Text(b"NextStartPos\0", 1),
        Arg::Text(b"BootEventName\0", 2),
        Arg::Bool(b"IsCheckAir\0", false),
        Arg::Bool(ARG_GIVE, false),
        Arg::Bool(ARG_STEP, false),
    ];
    let outcome = unsafe { request_map_change(main, o, actor, EVENT, &texts, &args) };
    if matches!(outcome, Outcome::Done | Outcome::Unanswered) {
        mail.last.store(now, Ordering::Relaxed);
        if let Some(gd) = unsafe { single(main, o.gamedata) } {
            unsafe { set_bool(main, o, gd, NO_MINIMAP, false) };
        }
    }
    outcome
}

/// A quest step: the same event with `ZootrStep` set, which a dev
/// seed's flow answers by running the step, `map` the quest and
/// `spot` its step, and ending. It changes no map, so the teleport
/// guard doesn't apply. `ZootrGive` is asked first, so it's named false.
unsafe fn step(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let texts = [&mail.map, &mail.spot];
    let args = [
        Arg::Bool(ARG_GIVE, false),
        Arg::Bool(ARG_STEP, true),
        Arg::Text(ARG_CHALLENGE, 0),
        Arg::Text(ARG_STEP_NAME, 1),
    ];
    unsafe { request_map_change(main, o, actor, EVENT, &texts, &args) }
}

/// An item: the same event with `ZootrGive` set, which a dev seed's
/// flow answers by granting the actor `map` names, equipped when
/// `ZootrEquip` is, and ending. The game's own grant sets what a
/// bare write of the item's flags leaves out.
unsafe fn give(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let texts = [&mail.map];
    let args = [
        Arg::Bool(ARG_GIVE, true),
        Arg::Text(ARG_ACTOR, 0),
        Arg::Bool(ARG_EQUIP, mail.hour != 0),
    ];
    unsafe { request_map_change(main, o, actor, EVENT, &texts, &args) }
}

/// A warp: Link's character controller moved to `at`, the way the game
/// moves him itself. Not during an event, which holds him.
unsafe fn warp(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    if let Err(busy) = unsafe { idle(main, o) } {
        return busy;
    }
    let mut controller = unsafe { at(actor, COMPONENTS) };
    for off in CONTROLLER {
        controller = controller.and_then(|p| unsafe { at(p, off) });
    }
    let vtable = main + o.controller_vtable as usize;
    let Some(controller) = controller.filter(|&c| unsafe { *(c as *const usize) } == vtable) else {
        return Outcome::NoController;
    };
    // Read as a 16 byte vector.
    let [x, y, z] = mail.at;
    let point = [x, y, z, 0.0];
    unsafe {
        let move_to: MoveTo = core::mem::transmute(main + o.move_to as usize);
        move_to(controller, point.as_ptr());
    }
    Outcome::Done
}

/// Where Link stands, for the host to check a warp held.
unsafe fn locate(mail: &Mailbox, actor: usize) -> Outcome {
    unsafe { *mail.link.get() = *((actor + POSITION) as *const [f32; 3]) };
    Outcome::Done
}

/// The event `map` names, with no arguments, as a trigger would start
/// it.
unsafe fn play(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    match play_event(&mail.map) {
        Some(event) => unsafe { request_map_change(main, o, actor, event, &[], &[]) },
        None => Outcome::NotBuilt,
    }
}

/// Flip the age and teleport. The flag goes back if the teleport
/// is refused.
unsafe fn age(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let Some(gd) = (unsafe { single(main, o.gamedata) }) else {
        return Outcome::NoGame;
    };
    let Some(child) = (unsafe { get_bool(main, o, gd, CHILD) }) else {
        return Outcome::NoAge;
    };
    unsafe { set_bool(main, o, gd, CHILD, !child) };
    let outcome = unsafe { teleport(main, o, mail, actor) };
    if !matches!(outcome, Outcome::Done | Outcome::Unanswered) {
        unsafe { set_bool(main, o, gd, CHILD, child) };
    }
    outcome
}

unsafe fn time(main: usize, o: &Offsets, hour: u32) -> Outcome {
    if hour >= 24 {
        return Outcome::Unknown;
    }
    let world = unsafe { single(main, o.world) };
    let Some(world) = world.filter(|&w| unsafe { at(w, CLOCK) }.is_some()) else {
        return Outcome::NoClock;
    };
    unsafe {
        let set: SetTime = core::mem::transmute(main + o.set_time as usize);
        set(world, hour * 3600);
    }
    Outcome::Done
}
