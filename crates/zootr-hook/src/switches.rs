//! The dev switches: refills the app turns on in a word beside the mailbox,
//! which the hook keeps topped up on Link's update. See
//! `docs/superpowers/specs/2026-11-13-dev-switches-design.md`.

use crate::{LIST, Shared};

/// Where the switches word sits in `main`.
pub const SWITCHES: u32 = LIST + core::mem::offset_of!(Shared, switches) as u32;

/// The switches word's bits.
pub const HEALTH: u32 = 1;
pub const MAGIC: u32 = 2;
pub const RESOURCES: u32 = 4;

/// Where a value's cap comes from: another Int, or the bag a
/// `CurrentExpansionActor_*` String64 names and each size's cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cap {
    Int(u32),
    Bag(u32, [i32; 3]),
    /// A fixed number.
    Fixed(i32),
    /// Bombchus: the bomb bag's, or [`FREE_BOMBCHUS_CAP`] in a seed that
    /// frees them.
    Bombchus,
}

/// What a seed with free bombchus caps them at.
pub const FREE_BOMBCHUS_CAP: i32 = 50;

impl Cap {
    /// This cap in a seed with the table's `flags`: only bombchus change.
    pub fn in_seed(self, flags: u32) -> Self {
        match self {
            Self::Bombchus if flags & crate::FREE_BOMBCHUS != 0 => Self::Fixed(FREE_BOMBCHUS_CAP),
            Self::Bombchus => Self::Bag(BOMB_BAG, BOMBS),
            cap => cap,
        }
    }
}

/// `CurrentExpansionActor_Bomb`, the bag bombs and bombchus share.
const BOMB_BAG: u32 = 0xa835_e3b0;

/// A GameData Int a switch holds at its cap. Link keeps his own meters
/// and copies them to GameData every frame, so those are held at the
/// `meter` instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Refill {
    pub switch: u32,
    pub value: u32,
    pub cap: Cap,
    pub meter: Option<Meter>,
}

/// Where Link's player object keeps a meter: through the pointer at `via`,
/// if any, the value at `value`, and its GameData name hash at `hash`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meter {
    pub via: Option<usize>,
    pub value: usize,
    pub hash: usize,
}

const STICKS: [i32; 3] = [10, 20, 30];
const NUTS: [i32; 3] = [20, 30, 40];
const SEEDS: [i32; 3] = [30, 40, 50];
const ARROWS: [i32; 3] = [30, 40, 50];
const BOMBS: [i32; 3] = [20, 30, 40];
const RUPEES: [i32; 3] = [200, 500, 1000];

const fn bag(value: u32, kind: u32, caps: [i32; 3]) -> Refill {
    Refill {
        switch: RESOURCES,
        value,
        cap: Cap::Bag(kind, caps),
        meter: None,
    }
}

/// Every refill, by name hash.
pub const REFILLS: [Refill; 9] = [
    // PlayerStatus.Life to PlayerStatus.MaxLife, in quarter hearts.
    Refill {
        switch: HEALTH,
        value: 0x31ab_5580,
        cap: Cap::Int(0xfbe0_1da1),
        // A ref the player points at, MaxLife's after it.
        meter: Some(Meter {
            via: Some(0x1cd8),
            value: 8,
            hash: 12,
        }),
    },
    // PlayerStatus.MP to PlayerStatus.MPMax.
    Refill {
        switch: MAGIC,
        value: 0x76b1_8200,
        cap: Cap::Int(0x12d6_3a97),
        // In the player itself, after the MP, MPMax and one more hash.
        meter: Some(Meter {
            via: None,
            value: 0x3afc,
            hash: 0x3af0,
        }),
    },
    // Each PouchData.<item>.StockNum to its CurrentExpansionActor_<kind>.
    bag(0xb949_4411, 0x2808_ee1c, STICKS),
    bag(0xf30d_9f54, 0x0da3_a7a7, NUTS),
    bag(0x5fad_f7fc, 0xd732_dd7f, SEEDS),
    bag(0x518e_c74b, 0xb629_4119, ARROWS),
    bag(0xdb1b_cd4f, BOMB_BAG, BOMBS),
    // Bombchus have no bag of their own.
    Refill {
        switch: RESOURCES,
        value: 0xdf7c_517a,
        cap: Cap::Bombchus,
        meter: None,
    },
    // PlayerStatus.CurrentRupee to CurrentExpansionActor_Rupee.
    bag(0xa779_21d7, 0x3e42_affa, RUPEES),
];

/// Where Link's player object `owner` keeps the meter `m`, if the name hash
/// it keeps beside it is `hash`.
///
/// # Safety
///
/// `owner` must be Link's player object, or memory laid out like one.
pub unsafe fn meter(owner: usize, m: Meter, hash: u32) -> Option<usize> {
    let base = match m.via {
        Some(off) => unsafe { *((owner + off) as *const usize) },
        None => owner,
    };
    let named = base != 0 && unsafe { *((base + m.hash) as *const u32) } == hash;
    named.then_some(base + m.value)
}

/// The refills of the switches `word` turns on.
pub fn held(word: u32) -> impl Iterator<Item = &'static Refill> {
    REFILLS.iter().filter(move |r| word & r.switch != 0)
}

/// What to write to hold `value` at `cap`: the cap, when the value is below
/// it.
pub fn top_up(value: i32, cap: i32) -> Option<i32> {
    (value < cap).then_some(cap)
}

/// The cap of the bag `text` names, by its size, or 0 for none. The text
/// ends at its first NUL, since clearing a string leaves the rest.
pub fn bag_cap(text: &[u8], caps: [i32; 3]) -> i32 {
    let name = text.split(|&b| b == 0).next().unwrap_or_default();
    let names = |size: &[u8]| name.windows(size.len()).any(|w| w == size);
    // One at a time: a table of the sizes would hold pointers, which the
    // hook can't relocate.
    let [small, medium, large] = caps;
    if names(b"Small") {
        small
    } else if names(b"Medium") {
        medium
    } else if names(b"Large") {
        large
    } else {
        0
    }
}

/// GameData's tables: one per type, `TABLE` bytes apart from the
/// manager's `+8`. See `docs/research/live-flags.md`.
pub const TABLE: usize = 0x60;
/// A table's entry count, entries, `(hash, index)` map and map size.
pub const COUNT: usize = 0x08;
pub const ENTRIES: usize = 0x10;
pub const MAP: usize = 0x18;
pub const MAP_SIZE: usize = 0x24;
/// The Int table: its slot, entry stride, and where an entry keeps its
/// current value, the saved copy right after.
pub const INT: (usize, usize, usize) = (2, 0x28, 0x1c);
/// The String64 table, the same way, to its current text.
pub const STRING64: (usize, usize, usize) = (16, 0x108, 0x74);
/// A String64's size, its NUL included.
pub const STRING64_SIZE: usize = 64;

/// The index `map` gives `hash`, if it is one of the table's `count`.
pub fn index(map: &[[u32; 2]], count: u32, hash: u32) -> Option<usize> {
    let &[_, i] = map.iter().find(|[h, _]| *h == hash)?;
    (i < count).then_some(i as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAILBOX, Mailbox};

    #[test]
    fn a_value_is_topped_up_only_below_its_cap() {
        assert_eq!(top_up(7, 12), Some(12));
        assert_eq!(top_up(12, 12), None);
        assert_eq!(top_up(15, 12), None);
        // No meter, or no bag: nothing to hold.
        assert_eq!(top_up(0, 0), None);
    }

    #[test]
    fn only_the_switches_on_are_held() {
        let values = |word| held(word).map(|r| r.value).collect::<Vec<_>>();
        assert_eq!(values(0), []);
        assert_eq!(values(HEALTH), [0x31ab_5580]);
        assert_eq!(values(MAGIC), [0x76b1_8200]);
        assert_eq!(values(HEALTH | MAGIC).len(), 2);
        assert_eq!(values(RESOURCES).len(), 7);
        assert_eq!(values(HEALTH | MAGIC | RESOURCES).len(), REFILLS.len());
    }

    #[test]
    fn bombchus_hold_at_the_bag_unless_the_seed_frees_them() {
        assert_eq!(Cap::Bombchus.in_seed(0), Cap::Bag(BOMB_BAG, BOMBS));
        let free = crate::FREE_BOMBCHUS;
        assert_eq!(Cap::Bombchus.in_seed(free), Cap::Fixed(FREE_BOMBCHUS_CAP));
        assert_eq!(Cap::Int(7).in_seed(free), Cap::Int(7));
    }

    #[test]
    fn a_meter_is_found_only_beside_its_hash() {
        let [life, mp] = [REFILLS[0], REFILLS[1]].map(|r| (r.value, r.meter.unwrap()));
        // A player object: the MP in place, and a pointer to the life ref.
        let mut owner = vec![0u64; 0x4000 / 8];
        let life_ref = [0u32, 0, 9, life.0];
        let base = owner.as_mut_ptr() as usize;
        let put = |off: usize, v: u32| unsafe { *((base + off) as *mut u32) = v };
        put(mp.1.hash, mp.0);
        put(mp.1.value, 40);
        let via = life.1.via.unwrap();
        unsafe { *((base + via) as *mut usize) = life_ref.as_ptr() as usize };
        let read = |m, hash| unsafe { meter(base, m, hash).map(|at| *(at as *const i32)) };
        assert_eq!(read(life.1, life.0), Some(9));
        assert_eq!(read(mp.1, mp.0), Some(40));
        assert_eq!(read(mp.1, life.0), None, "another value's hash");
        unsafe { *((base + via) as *mut usize) = 0 };
        assert_eq!(read(life.1, life.0), None, "no ref yet");
    }

    #[test]
    fn a_bags_cap_goes_by_its_size() {
        assert_eq!(bag_cap(b"Item_Wallet_Small", RUPEES), 200);
        assert_eq!(bag_cap(b"Item_Wallet_Medium", RUPEES), 500);
        assert_eq!(bag_cap(b"Item_Wallet_Large", RUPEES), 1000);
        assert_eq!(
            bag_cap(
                b"Work/Actor/Item_Quiver_Medium.engine__actor__ActorParam.gyml",
                ARROWS
            ),
            40
        );
        assert_eq!(bag_cap(b"", BOMBS), 0);
        // A cleared string keeps its old text after the NUL.
        assert_eq!(bag_cap(b"\0tem_BombBag_Large\0", BOMBS), 0);
        assert_eq!(bag_cap(b"Item_BombBag_Small\0Large", BOMBS), 20);
    }

    #[test]
    fn an_index_is_found_by_hash_within_the_count() {
        let map = [[0, 0], [0xaa, 3], [0xbb, 9]];
        assert_eq!(index(&map, 5, 0xaa), Some(3));
        assert_eq!(index(&map, 5, 0xbb), None);
        assert_eq!(index(&map, 5, 0xcc), None);
    }

    #[test]
    fn the_switches_word_follows_the_mailbox() {
        assert_eq!(SWITCHES, MAILBOX + size_of::<Mailbox>() as u32);
        assert_eq!(SWITCHES % 4, 0);
    }
}
