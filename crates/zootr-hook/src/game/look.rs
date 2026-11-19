//! Writing the ice's size for Link's age, on his update.

use core::sync::atomic::Ordering;

use super::dev::{CHILD, get_bool, single};
use super::{at, shared};
use crate::Table;
use crate::look::PATH;

/// Write the table's fit for Link's age into each shell's record, when it
/// isn't the fit written last. The file loads adult sized, and nothing
/// is written while any read on the way fails. Far code: past the age
/// check it runs only when Link's age changes.
///
/// # Safety
/// The table's offsets are this build's.
#[inline(never)]
#[unsafe(link_section = ".far")]
pub unsafe fn fit(main: usize, table: &Table) {
    let Some(look) = &table.look else {
        return;
    };
    let o = table.offsets;
    let Some(gd) = (unsafe { single(main, o.gamedata) }) else {
        return;
    };
    let Some(child) = (unsafe { get_bool(main, o, gd, CHILD) }) else {
        return;
    };
    let written = &shared(main).child_ice;
    if (written.load(Ordering::Relaxed) != 0) == child {
        return;
    }
    let system = unsafe { at(main, o.effects as usize) };
    let found = system.and_then(|s| PATH.iter().try_fold(s, |p, &off| unsafe { at(p, off) }));
    let Some(fields) = found else {
        return;
    };
    let (s, age) = (look.sizes, child as usize);
    for record in look.records() {
        let record = fields + record as usize;
        // SAFETY: each record is in the effect file, which stays loaded.
        unsafe {
            (record as *mut u64).write_unaligned(s.masks[age]);
            ((record + s.value_at as usize) as *mut u32).write_unaligned(s.values[age]);
        }
    }
    written.store(child as u32, Ordering::Relaxed);
}
