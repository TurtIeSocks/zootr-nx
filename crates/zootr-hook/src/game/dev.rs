//! The dev requests, run on Link's update. Each one proven first from
//! outside the game.

use core::arch::asm;
use core::sync::atomic::Ordering;

use super::rt::{memcpy, memset};
use super::{at, func, level_for, name, owned, shared};
use crate::traps::COMPONENTS;
use crate::{
    ARG_ACTOR, ARG_CHALLENGE, ARG_EQUIP, ARG_GIVE, ARG_MIXED, ARG_NOTICE, ARG_STEP_NAME,
    ARG_SUMMARY, Ask, DEV_EVENT, GUARD, Mailbox, NAME, Notice, Offsets, Outcome, Spot, fnv,
    play_event,
};

type Malloc = unsafe extern "C" fn(usize) -> usize;
type MakeArg = unsafe extern "C" fn(usize);
type Request = unsafe extern "C" fn(usize, usize) -> u64;
type SetLink = unsafe extern "C" fn(usize, usize, usize);
type Release = unsafe extern "C" fn(usize);
/// A GameData getter, Bool and U64 alike: (gamedata, out, hash) -> found.
type Get = unsafe extern "C" fn(usize, *mut u64, u32) -> u32;
type SetBool = unsafe extern "C" fn(usize, u32, u32);
type SetTime = unsafe extern "C" fn(usize, u32);
type MoveTo = unsafe extern "C" fn(usize, *const f32);

/// GameData name hashes.
pub(super) const CHILD: u32 = 0xcea2_20d3;
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
const EVENT_NAME: usize = 0x1a90;
const ARGS: usize = 8;
const ARG_COUNT: usize = 0x10;
const ARG_SIZE: usize = 0xc8;
const ARG_VALUE: usize = 0x98;
const STARTER: usize = 0x1950;
const LINKS: [usize; 2] = [0x1968, 0x1950];
/// The event manager's playing event.
const PLAYING: usize = 0x68;
/// The world's clock.
const CLOCK: usize = 0x510;
/// The steps from Link's components to his character controller.
const CONTROLLER: [usize; 4] = [0x50, 0x20, 0x10, 0x8];
/// Where Link's components keep his movement component; where that sits
/// in the player object that owns it, and keeps its actor; and where the
/// owner keeps its resource, and the resource its name.
const MOVEMENT: usize = 0x320;
const MOVEMENT_IN_OWNER: usize = 0x1d50;
const MOVEMENT_ACTOR: usize = 0x18;
const RESOURCE: usize = 0x10;
const RESOURCE_NAME: usize = 0x58;
/// An actor's position, Link's and a pickup's alike.
pub(super) const POSITION: usize = 0x31c;

/// Run the request the app left in the mailbox. Far code: only a dev
/// seed calls it, and most calls find no request.
#[inline(never)]
#[unsafe(link_section = ".far")]
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
        // Answered by the swap's last step, frames later.
        Some(Ask::AgeHere) => match unsafe { super::swap::start(main, o, mail, actor) } {
            Some(refused) => refused,
            None => return,
        },
        Some(Ask::Teleport) => unsafe { teleport(main, o, mail, actor) },
        Some(Ask::Age) => unsafe { age(main, o, mail, actor) },
        Some(Ask::Time) => unsafe { time(main, o, mail.hour) },
        Some(Ask::Step) => unsafe { step(main, o, mail, actor) },
        Some(Ask::Give) => unsafe { give(main, o, mail, actor, false) },
        Some(Ask::GiveLevel) => unsafe { give(main, o, mail, actor, true) },
        Some(Ask::Play) => unsafe { play(main, o, mail, actor) },
        Some(Ask::Warp) => unsafe { warp(main, o, mail, actor) },
        Some(Ask::Where) => unsafe { locate(mail, actor) },
        Some(Ask::Notice) => unsafe { notice(main, o, mail, actor) },
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
        let movement = at(at(actor, COMPONENTS)?, MOVEMENT)?;
        let owner = movement - MOVEMENT_IN_OWNER;
        let resource = at(owner, RESOURCE)?;
        let ok = at(movement, MOVEMENT_ACTOR)? == actor
            && vtable(owner, o.move_vtable)
            && name((resource + RESOURCE_NAME) as *const u8) == Some(&b"Player"[..]);
        ok.then_some(owner)
    }
}

/// `[[main + slot]]`, a singleton's pointer.
pub(super) unsafe fn single(main: usize, slot: u32) -> Option<usize> {
    unsafe { at(at(main, slot as usize)?, 0) }
}

pub(super) unsafe fn get_bool(main: usize, o: &Offsets, gd: usize, hash: u32) -> Option<bool> {
    let get: Get = unsafe { func(main, o.get_bool) };
    let mut out = 0;
    (unsafe { get(gd, &mut out, hash) } & 1 == 1).then_some(out & 1 == 1)
}

unsafe fn get_u64(main: usize, o: &Offsets, gd: usize, hash: u32) -> Option<u64> {
    let get: Get = unsafe { func(main, o.get_u64) };
    let mut out = 0;
    (unsafe { get(gd, &mut out, hash) } & 1 == 1).then_some(out)
}

/// Spot `i` of those the seed wrote to GameData.
pub(super) unsafe fn spot(main: usize, o: &Offsets, gd: usize, i: u32) -> Option<Spot> {
    let word = |w| unsafe { get_u64(main, o, gd, Spot::key(i, w)) };
    Some(Spot::from_words([word(0)?, word(1)?, word(2)?, word(3)?]))
}

pub(super) unsafe fn set_bool(main: usize, o: &Offsets, gd: usize, hash: u32, value: bool) {
    let set: SetBool = unsafe { func(main, o.set_bool) };
    unsafe { set(gd, value as u32, hash) }
}

/// The system counter and its ticks per second.
pub(super) fn clock() -> (u64, u64) {
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
        if !put(*((e + 8) as *const usize), *((e + 0x10) as *const u32), key) {
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

/// Whether an event plays, or `None` before the game is up.
///
/// # Safety
/// The offsets are this build's.
pub(super) unsafe fn playing(main: usize, o: &Offsets) -> Option<bool> {
    let events = unsafe { single(main, o.events) }?;
    Some(unsafe { at(events, PLAYING) }.is_some())
}

/// One argument of an event request: a name in `texts` (by index)
/// or a bool.
enum Arg {
    Text(&'static [u8], usize),
    Bool(&'static [u8], bool),
}

/// The event manager, refusing while the game isn't up, an event plays
/// or a map loads.
pub(super) unsafe fn idle(main: usize, o: &Offsets) -> Result<usize, Outcome> {
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
/// loads. Every dev event starts here, not only the teleport's map
/// change. The game reads the block and its strings after the
/// request returns, so both go in one allocation that's never
/// freed.
unsafe fn request_event(
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
    unsafe {
        let malloc: Malloc = func(main, o.malloc);
        let make: MakeArg = func(main, o.make_arg);
        let request: Request = func(main, o.request);
        let set_link: SetLink = func(main, o.set_link);
        let release: Release = func(main, o.release_link);
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
            *((block + link) as *mut usize) = main + o.link_empty as usize;
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
/// [`GUARD`] seconds after the last.
unsafe fn teleport(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let (now, rate) = clock();
    let last = mail.last.load(Ordering::Relaxed);
    if last != 0 && now.wrapping_sub(last) < GUARD * rate {
        return Outcome::Soon;
    }
    let texts = [&mail.map, &mail.spot, unsafe { &*mail.arrive.get() }];
    let args = [
        Arg::Text(b"NextBanc\0", 0),
        Arg::Text(b"NextStartPos\0", 1),
        Arg::Text(b"BootEventName\0", 2),
        Arg::Bool(b"IsCheckAir\0", false),
    ];
    let outcome = unsafe { request_event(main, o, actor, EVENT, &texts, &args) };
    if matches!(outcome, Outcome::Done | Outcome::Unanswered) {
        mail.last.store(now, Ordering::Relaxed);
        if let Some(gd) = unsafe { single(main, o.gamedata) } {
            unsafe { set_bool(main, o, gd, NO_MINIMAP, false) };
        }
    }
    outcome
}

/// A quest step: [`DEV_EVENT`] with `ZootrNotice` and `ZootrGive` false,
/// which a seed's flow answers by running the step, `map` the quest and
/// `spot` its step, and ending. It changes no map, so the teleport guard
/// doesn't apply. Each dev request names every switch the flow asks
/// before its own: one left out keeps the value the last request gave it.
unsafe fn step(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let texts = [&mail.map, &mail.spot];
    let args = [
        Arg::Bool(ARG_NOTICE, false),
        Arg::Bool(ARG_GIVE, false),
        Arg::Text(ARG_CHALLENGE, 0),
        Arg::Text(ARG_STEP_NAME, 1),
    ];
    unsafe { request_event(main, o, actor, DEV_EVENT, &texts, &args) }
}

/// An item: [`DEV_EVENT`] with `ZootrGive` set, which a seed's
/// flow answers by granting the actor `map` names, equipped when
/// `ZootrEquip` is, and ending. The game's own grant sets what a
/// bare write of the item's flags leaves out. With `level`, an item of a
/// progressive line is swapped for the level Link needs next, as a chest
/// picks it.
unsafe fn give(main: usize, o: &Offsets, mail: &Mailbox, actor: usize, level: bool) -> Outcome {
    let mut picked = [0; NAME];
    let item = match level {
        true => unsafe { pick(main, mail, &mut picked) },
        false => &mail.map,
    };
    let texts = [item];
    let args = [
        Arg::Bool(ARG_NOTICE, false),
        Arg::Bool(ARG_GIVE, true),
        Arg::Text(ARG_ACTOR, 0),
        Arg::Bool(ARG_EQUIP, mail.hour != 0),
    ];
    // Copied first: `item` may be `arrive` itself.
    let given = *item;
    let outcome = unsafe { request_event(main, o, actor, DEV_EVENT, &texts, &args) };
    if level {
        unsafe { *mail.arrive.get() = given };
    }
    outcome
}

/// A received item's notice: [`DEV_EVENT`] with `ZootrNotice` set, which
/// a seed's flow answers by showing the banner `hour` picks, and for an
/// item the popup for the actor `map` names. Neither stops Link.
unsafe fn notice(main: usize, o: &Offsets, mail: &Mailbox, actor: usize) -> Outcome {
    let Some(kind) = Notice::from_word(mail.hour) else {
        return Outcome::Unknown;
    };
    if kind == Notice::Item
        && let Some(item) = unsafe { name(mail.map.as_ptr()) }
    {
        shared(main).with(|l| l.notice(fnv(item)));
    }
    let texts = [&mail.map];
    let args = [
        Arg::Bool(ARG_NOTICE, true),
        Arg::Text(ARG_ACTOR, 0),
        Arg::Bool(ARG_SUMMARY, kind != Notice::Item),
        Arg::Bool(ARG_MIXED, kind == Notice::Mixed),
    ];
    unsafe { request_event(main, o, actor, DEV_EVENT, &texts, &args) }
}

/// What a [`Ask::GiveLevel`] gives: `arrive` when `spot` names what fires
/// the item and Link owns no level of it, as a chest's swap does. Else the
/// level of the item's line Link needs next, written into `picked`.
/// Outside every line, or with no table, the item as it is.
///
/// # Safety
/// The table's offsets are this build's.
unsafe fn pick<'a>(main: usize, mail: &'a Mailbox, picked: &'a mut [u8; NAME]) -> &'a [u8; NAME] {
    let item = &mail.map;
    let Some((_, table)) = super::table() else {
        return item;
    };
    let fires = unsafe { name(mail.spot.as_ptr()) }.filter(|f| !f.is_empty());
    if let Some(fires) = fires {
        let Some(owns) = (unsafe { owned(main, table.offsets) }) else {
            return item;
        };
        let has = match table.find(fires) {
            Some((_, mut levels, _)) => levels.any(|l| owns(l.pouch)),
            None => owns(fires),
        };
        return if has {
            item
        } else {
            unsafe { &*mail.arrive.get() }
        };
    }
    let now = unsafe { name(item.as_ptr()) };
    let Some(level) = now.and_then(|now| unsafe { level_for(main, &table, now) }) else {
        return item;
    };
    if level.len() >= NAME {
        return item;
    }
    unsafe { memcpy(picked.as_mut_ptr(), level.as_ptr(), level.len()) };
    picked
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
        let move_to: MoveTo = func(main, o.move_to);
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
        Some(event) => unsafe { start(main, o, actor, event) },
        None => Outcome::NotBuilt,
    }
}

/// Start `event`, NUL ended, with no arguments, as a trigger would.
pub(super) unsafe fn start(main: usize, o: &Offsets, actor: usize, event: &[u8]) -> Outcome {
    unsafe { request_event(main, o, actor, event, &[], &[]) }
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
        let set: SetTime = func(main, o.set_time);
        set(world, hour * 3600);
    }
    Outcome::Done
}
