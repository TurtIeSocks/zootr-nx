//! What Link nocks: the drawn bow's own projectile. The game nocks the
//! one Link's `ArrowActorName` names, read once at load, and the child's
//! names the Deku Seed, so a child Bow (items for both ages) would fire
//! seeds. Each bow's `BowParam.ShootableActor` already names its own Link's
//! projectile, so vanilla play nocks what it always did.

use core::mem::transmute;

use crate::OFFSETS;

/// Where Link's EquipmentUser component keeps his projectile's name
/// hash, which the nock, the shot and the releases all read.
const PROJECTILE: usize = 0x12b4;

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

/// Link is about to nock with `user`, his EquipmentUser component: set
/// his projectile to the drawn bow's, as the release at `0x2e007c0` names
/// it. No bow drawn, or one naming none (the Hookshot), leaves it.
///
/// # Safety
/// `user` is Link's EquipmentUser component, as the nock gets it.
pub unsafe fn nocked(main: usize, user: usize) {
    let call = |off: u32| main + off as usize;
    unsafe {
        let drawn: Drawn = transmute(call(OFFSETS.drawn_bow));
        let bow = drawn(user);
        if bow == 0 {
            return;
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
            return;
        }
        let hash: Hash = transmute(call(OFFSETS.hash));
        *((user + PROJECTILE) as *mut u32) = hash(fixed.text, 0);
    }
}
