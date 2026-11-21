//! What Link nocks and what an empty bow says: the drawn bow's own
//! projectile. The game nocks the one Link's `ArrowActorName` names, read
//! once at load, and picks the empty message by his age, so a child Bow
//! (items for both ages) would fire seeds and say it has none. Each bow's
//! `BowParam.ShootableActor` already names its own Link's projectile, so
//! vanilla play nocks and says what it always did.

use core::mem::transmute;

use crate::{DEKU_SEED, OFFSETS};

/// Where Link's EquipmentUser component keeps his projectile's name
/// hash, which the nock, the shot and the releases all read.
const PROJECTILE: usize = 0x12b4;
/// The flags word in Link's player component (`+0x270`, `+0x358`) the out
/// of ammo handler raises, then the message bits it raises there.
const FLAGS: usize = 0xe38;
const NO_ARROWS: u32 = 1 << 27;
const NO_SEEDS: u32 = 1 << 29;

/// The game's fixed string: its text, its capacity, then the buffer, laid
/// out as the release at `0x2e007c0` lays it on its stack.
#[repr(C)]
struct Fixed {
    text: *mut u8,
    capacity: i32,
    buffer: [u8; 64],
}

type Drawn = unsafe extern "C" fn(usize) -> usize;
type Shootable = unsafe extern "C" fn(*const usize) -> usize;
type Copy = unsafe extern "C" fn(*mut Fixed, usize);
type Hash = unsafe extern "C" fn(*const u8, u32) -> u32;

/// The drawn bow's projectile, as a name hash, the way the release at
/// `0x2e007c0` names it. None with no bow drawn, or one naming none.
///
/// # Safety
/// `user` is Link's EquipmentUser component.
unsafe fn projectile(main: usize, user: usize) -> Option<u32> {
    let call = |off: u32| main + off as usize;
    unsafe {
        let drawn: Drawn = transmute(call(OFFSETS.drawn_bow));
        let bow = drawn(user);
        if bow == 0 {
            return None;
        }
        let shootable: Shootable = transmute(call(OFFSETS.shootable));
        let name = shootable(&bow);
        let mut fixed = Fixed {
            text: core::ptr::null_mut(),
            capacity: 64,
            buffer: [0; 64],
        };
        fixed.text = fixed.buffer.as_mut_ptr();
        let copy: Copy = transmute(call(OFFSETS.name_copy));
        copy(&mut fixed, name);
        fixed.buffer[63] = 0;
        if *fixed.text == 0 {
            return None;
        }
        let hash: Hash = transmute(call(OFFSETS.hash));
        Some(hash(fixed.text, 0))
    }
}

/// Link is about to nock with `user`, his EquipmentUser component: set
/// his projectile to the drawn bow's. No bow drawn, or one naming none
/// (the Hookshot), leaves it.
///
/// # Safety
/// `user` is Link's EquipmentUser component, as the nock gets it.
pub unsafe fn nocked(main: usize, user: usize) {
    unsafe {
        if let Some(hash) = projectile(main, user) {
            *((user + PROJECTILE) as *mut u32) = hash;
        }
    }
}

/// The Bow node `node` found no ammo and raised the empty message its age
/// picks: raise the one the drawn bow's projectile picks instead.
///
/// # Safety
/// `node` is the Bow node, as the out of ammo handler gets it.
pub unsafe fn emptied(main: usize, node: usize) {
    let at = |p: usize| unsafe { *(p as *const usize) };
    unsafe {
        let link = at(at(at(node + 8) + 8) + 0x1f0);
        let parts = at(link + 0x270);
        if parts == 0 {
            return;
        }
        let (user, player) = (at(parts + 0x200), at(parts + 0x358));
        let flags = (player + FLAGS) as *mut u32;
        if user == 0 || player == 0 || *flags & (NO_ARROWS | NO_SEEDS) == 0 {
            return;
        }
        let Some(hash) = projectile(main, user) else {
            return;
        };
        let says = if hash == DEKU_SEED {
            NO_SEEDS
        } else {
            NO_ARROWS
        };
        *flags = *flags & !(NO_ARROWS | NO_SEEDS) | says;
    }
}
