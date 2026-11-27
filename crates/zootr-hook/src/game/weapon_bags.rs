//! The first Bow and Slingshot come with the bag that holds their ammo.

use super::{dev, hooked, name, refill};
use crate::switches::{BULLET_BAG_SLOT, QUIVER_SLOT, STRING64};
use crate::table::fnv;

/// The Small Bullet Bag and Small Quiver names in the game's read-only data.
#[cfg(not(zootr_build = "v65536"))]
const BAG_NAMES: [u32; 2] = [0x3f1_3f1a, 0x3f1_3f44];
#[cfg(zootr_build = "v65536")]
const BAG_NAMES: [u32; 2] = [0x3eb_450a, 0x3eb_4534];

struct StarterBag {
    slot: u32,
    owned: u32,
    weapon_names: [u32; 2],
    bag_name: u32,
}

const BAGS: [StarterBag; 2] = [
    StarterBag {
        slot: BULLET_BAG_SLOT,
        owned: 0x0ec2_2ee1,
        weapon_names: [0x160d_d386, 0xb568_f841],
        bag_name: BAG_NAMES[0],
    },
    StarterBag {
        slot: QUIVER_SLOT,
        owned: 0x69ec_28be,
        weapon_names: [0xde01_58c3, 0x30b6_cafc],
        bag_name: BAG_NAMES[1],
    },
];

const KEY_ITEM: u32 = 6;

unsafe extern "C" {
    /// The pouch add without reentering the grant hook.
    fn add_to_pouch(
        pouch: usize,
        actor: *const *const u8,
        category: u32,
        count: i32,
        reason: u32,
        bundled: u32,
        canonical: u32,
        get_event: u32,
        keep_pending: u8,
    ) -> u32;
}

/// A chest may already have given the bag. Adding the bag normally sets
/// its ownership flag, writes the capacity and fills the ammo.
///
/// # Safety
/// Called by the grant stub with the game's original result, live pouch
/// and the same actor name argument as the original function.
#[unsafe(no_mangle)]
unsafe extern "C" fn ensure_weapon_bags(result: u32, pouch: usize, added: *const *const u8) {
    if result & 1 == 0 {
        return;
    }
    hooked(|main, table| {
        let o = table.offsets;
        // SAFETY: The table and the bag names refer to this build's main.
        unsafe {
            let Some(gd) = dev::single(main, o.gamedata) else {
                return;
            };
            let added = added.as_ref().and_then(|&p| name(p)).map(fnv);
            for bag in BAGS {
                let empty = refill::entry(gd, STRING64, bag.slot)
                    .is_some_and(|value| *(value as *const u8) == 0);
                // Ownership also covers shelf aliases and saves made
                // before the bag was granted.
                if empty
                    && (added.is_some_and(|hash| bag.weapon_names.contains(&hash))
                        || dev::get_bool(main, o, gd, bag.owned) == Some(true))
                {
                    let name = (main + bag.bag_name as usize) as *const u8;
                    add_to_pouch(pouch, &name, KEY_ITEM, 1, 0, 0, 0, 0, 0);
                }
            }
        }
    });
}
