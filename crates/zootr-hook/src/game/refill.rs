//! The dev switches, held on Link's update.

use core::slice;
use core::sync::atomic::Ordering;

use super::dev::{player, single};
use super::{at, shared};
use crate::Table;
use crate::switches::{
    COUNT, Cap, ENTRIES, INT, MAP, MAP_SIZE, STRING64, STRING64_SIZE, TABLE, bag_cap, held, index,
    meter, top_up,
};

/// Top each value a switch that is on holds up to its cap. A value at its
/// cap, or one GameData doesn't hold yet, is left alone.
pub unsafe fn refill(main: usize, table: &Table, actor: usize) {
    let word = shared(main).switches.load(Ordering::Relaxed);
    if word == 0 {
        return;
    }
    let o = table.offsets;
    let Some(owner) = (unsafe { player(main, o, actor) }) else {
        return;
    };
    let Some(gd) = (unsafe { single(main, o.gamedata) }) else {
        return;
    };
    for r in held(word) {
        let at = match r.meter {
            Some(m) => unsafe { meter(owner, m, r.value) },
            None => unsafe { entry(gd, INT, r.value) },
        };
        let Some(value) = at else {
            continue;
        };
        let cap = match r.cap.in_seed(table.flags) {
            Cap::Int(hash) => unsafe { entry(gd, INT, hash).map(|e| *(e as *const i32)) },
            Cap::Bag(hash, caps) => unsafe {
                let text = |e| slice::from_raw_parts(e as *const u8, STRING64_SIZE);
                entry(gd, STRING64, hash).map(|e| bag_cap(text(e), caps))
            },
            Cap::Fixed(cap) => Some(cap),
            // `in_seed` has picked the bag or a fixed cap.
            Cap::Bombchus => None,
        };
        let value = value as *mut i32;
        if let Some(full) = cap.and_then(|cap| top_up(unsafe { *value }, cap)) {
            unsafe { *value = full };
            // GameData's saved copy follows its value. Link copies a meter
            // to GameData himself.
            if r.meter.is_none() {
                unsafe { *value.add(1) = full };
            }
        }
    }
}

/// Where `hash`'s current value sits in the GameData table given by its
/// slot, entry stride and value offset, like [`INT`].
unsafe fn entry(
    gd: usize,
    (slot, stride, value): (usize, usize, usize),
    hash: u32,
) -> Option<usize> {
    let table = gd + 8 + TABLE * slot;
    let word = |off: usize| unsafe { *((table + off) as *const u32) };
    let map = unsafe { at(table, MAP)? } as *const [u32; 2];
    let pairs = unsafe { slice::from_raw_parts(map, word(MAP_SIZE) as usize) };
    let i = index(pairs, word(COUNT), hash)?;
    Some(unsafe { at(table, ENTRIES)? } + i * stride + value)
}
