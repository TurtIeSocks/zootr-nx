//! Springing the ice traps Link has got, on his update.

use core::sync::atomic::Ordering;

use super::dev::{clock, player, playing, start};
use super::{at, shared};
use crate::traps::{BAD_CONDITIONS, COMPONENTS, END, EVENT, ON, spring, unmake};
use crate::{Outcome, Table};

/// Start the trap event for the next trap waiting, while no event plays
/// and the last has thawed. A trap the game refuses waits for the next
/// update.
///
/// # Safety
/// `actor` is the actor whose update is hooked, and the table's offsets
/// are this build's.
pub unsafe fn freeze(main: usize, table: &Table, actor: usize) {
    let o = table.offsets;
    let shared = shared(main);
    let out = shared.sensing.load(Ordering::Relaxed) != 0;
    let nothing_to_do = shared.traps.load(Ordering::Relaxed) == 0 && !out;
    if nothing_to_do
        || unsafe { playing(main, o) } != Some(false)
        || unsafe { player(main, o, actor) }.is_none()
    {
        return;
    }
    let Some(conditions) = (unsafe { at(actor, COMPONENTS).and_then(|c| at(c, BAD_CONDITIONS)) })
    else {
        return;
    };
    let on = unsafe { *((conditions + ON) as *const u32) };
    let (now, rate) = clock();
    let last = shared.sprung.load(Ordering::Relaxed);
    let since = (last != 0).then(|| now.wrapping_sub(last));
    let started = |event| {
        matches!(
            unsafe { start(main, o, actor, event) },
            Outcome::Done | Outcome::Unanswered
        )
    };
    if out {
        if unmake(on, since, rate) && started(END) {
            shared.sensing.store(0, Ordering::Relaxed);
        }
        return;
    }
    if !spring(&shared.traps, on, since, rate) {
        return;
    }
    if started(EVENT) {
        shared.sprung.store(now, Ordering::Relaxed);
        shared.sensing.store(1, Ordering::Relaxed);
    } else {
        shared.traps.fetch_add(1, Ordering::Relaxed);
    }
}
