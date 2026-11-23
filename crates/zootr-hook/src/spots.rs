//! The twinned holders whose drop sets a flag as Link takes it: where each
//! stands and what it drops ([`Spot`]), and which one a take ticks
//! ([`nearest`]).

use crate::table::fnv;

/// How far a drop may lie from its holder, in metres: a pot thrown breaks
/// where it lands.
pub const REACH: f32 = 8.0;

/// The holder whose drop Link just took: `name`, lying `at`. The nearest
/// of `spots` within [`REACH`] that drops that name, of those not yet
/// `picked`. A twinned holder takes the tick only once it broke: its
/// key is set at the break, so one still `whole` (by its placement) can't
/// have dropped anything. Its mark would go while it stands, and one take
/// that runs this twice would tick it too. A holder without a twin (a
/// Gold Skulltula) keeps no key at the break, so `whole` isn't asked.
pub fn nearest(
    spots: impl Iterator<Item = Spot>,
    name: &[u8],
    at: [f32; 3],
    picked: impl Fn(u32) -> bool,
    whole: impl Fn(u64) -> bool,
) -> Option<Spot> {
    let name = Spot::name_hash(name);
    let standing = |s: &Spot| s.twin != 0 && whole(s.placed);
    let mut best: Option<(f32, Spot)> = None;
    for s in spots.filter(|s| s.name == name) {
        let d = s.distance2(at);
        let nearer = best.is_none_or(|(b, _)| d < b);
        if nearer && d <= REACH * REACH && !picked(s.flag) && !standing(&s) {
            best = Some((d, s));
        }
    }
    best.map(|(_, s)| s)
}

/// Where the spots' GameData hashes start, [`Spot::WORDS`] to a spot:
/// murmur3 of `Rando_Spot`, so the hook needs no name hashing.
pub const SPOT_KEY: u32 = 0xa5b7_e07e;

/// A twinned holder (a pot, box, rock or hive), or one of its step
/// copies: the Bool its drop sets once taken, the drop's name by
/// [`Spot::name_hash`], where it stands, in whole metres, its own
/// placement and its twin's (0 for none), whose keys
/// ([`crate::keys`]) the take keeps alike. The seed writes each as
/// unsaved UInt64s in GameData, at [`Spot::key`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub flag: u32,
    pub name: u16,
    pub at: [i16; 3],
    pub placed: u64,
    pub twin: u64,
}

impl Spot {
    /// How many GameData words a spot takes.
    pub const WORDS: u32 = 4;

    /// The GameData hash of spot `i`'s `word`: 0 its flag and name, 1
    /// where it stands, 2 its placement, 3 its twin's.
    pub fn key(i: u32, word: u32) -> u32 {
        SPOT_KEY
            .wrapping_add(i.wrapping_mul(Self::WORDS))
            .wrapping_add(word)
    }

    /// The low half of a name's FNV-1a.
    pub fn name_hash(name: &[u8]) -> u16 {
        fnv(name) as u16
    }

    /// The spot from its GameData words.
    pub fn from_words([who, at, placed, twin]: [u64; Self::WORDS as usize]) -> Self {
        Self {
            flag: who as u32,
            name: (who >> 32) as u16,
            at: [0, 16, 32].map(|shift| (at >> shift) as u16 as i16),
            placed,
            twin,
        }
    }

    /// Its GameData words: the flag and name, where it stands, its
    /// placement, then its twin's.
    #[cfg(not(target_os = "none"))]
    pub fn words(self) -> [u64; Self::WORDS as usize] {
        let who = u64::from(self.flag) | u64::from(self.name) << 32;
        let at = (self.at.iter().zip([0, 16, 32]))
            .fold(0, |at, (&v, shift)| at | u64::from(v as u16) << shift);
        [who, at, self.placed, self.twin]
    }

    fn distance2(&self, at: [f32; 3]) -> f32 {
        at.iter()
            .zip(self.at)
            .map(|(a, s)| a - f32::from(s))
            .map(|d| d * d)
            .sum()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Two pots dropping a green rupee 3 m apart, one a stand-in further
    /// on, and one far off.
    pub(crate) const SPOTS: &[Spot] = &[
        spot(1, "Obj_FakeGreen", [10, 0, 10]),
        spot(2, "Obj_FakeGreen", [13, 0, 10]),
        spot(3, "Obj_FakeStandIn_001", [16, 0, 10]),
        spot(4, "Obj_FakeGreen", [100, 0, 10]),
    ];

    pub(crate) const fn spot(flag: u32, name: &str, at: [i16; 3]) -> Spot {
        // FNV-1a's low half, by hand, since `fnv` isn't const.
        let (b, mut h, mut i) = (name.as_bytes(), 0x811c_9dc5_u32, 0);
        while i < b.len() {
            h = (h ^ b[i] as u32).wrapping_mul(0x0100_0193);
            i += 1;
        }
        Spot {
            flag,
            name: h as u16,
            at,
            placed: flag as u64 + 100,
            twin: flag as u64 + 200,
        }
    }

    fn take(name: &str, at: [f32; 3], picked: &[u32]) -> Option<u32> {
        take_whole(name, at, picked, &[])
    }

    /// A take while the holders placed as `whole` still stand unbroken.
    fn take_whole(name: &str, at: [f32; 3], picked: &[u32], whole: &[u64]) -> Option<u32> {
        let spots = SPOTS.iter().map(|s| Spot::from_words(s.words()));
        let (picked, whole) = (|f| picked.contains(&f), |p| whole.contains(&p));
        nearest(spots, name.as_bytes(), at, picked, whole).map(|s| s.flag)
    }

    #[test]
    fn a_take_ticks_the_nearest_holder_dropping_that_name() {
        assert_eq!(take("Obj_FakeGreen", [10.4, 0.2, 9.0], &[]), Some(1));
        assert_eq!(take("Obj_FakeGreen", [12.0, 0.0, 10.0], &[]), Some(2));
        assert_eq!(take("Obj_FakeStandIn_001", [10.0, 0.0, 10.0], &[]), Some(3));
    }

    #[test]
    fn a_picked_holder_passes_the_tick_to_the_next() {
        assert_eq!(take("Obj_FakeGreen", [10.0, 0.0, 10.0], &[1]), Some(2));
        assert_eq!(take("Obj_FakeGreen", [10.0, 0.0, 10.0], &[1, 2]), None);
    }

    #[test]
    fn a_holder_still_standing_never_takes_the_tick() {
        // Holder 1 broke, but its drop lies nearer holder 2, still whole.
        let whole = [SPOTS[1].placed];
        assert_eq!(
            take_whole("Obj_FakeGreen", [12.0, 0.0, 10.0], &[], &whole),
            Some(1)
        );
        // A second tick of the same take finds no other broken holder.
        assert_eq!(
            take_whole("Obj_FakeGreen", [12.0, 0.0, 10.0], &[1], &whole),
            None
        );
        // A holder without a twin keeps no key at the break (a Gold
        // Skulltula), so whole or not it may take the tick.
        let mut skulltula = SPOTS[0];
        skulltula.twin = 0;
        let whole = |p| p == skulltula.placed;
        let picked = |_| false;
        let ticked = nearest(
            [skulltula].into_iter(),
            b"Obj_FakeGreen",
            [10.0, 0.0, 10.0],
            picked,
            whole,
        );
        assert_eq!(ticked, Some(skulltula));
    }

    #[test]
    fn a_take_far_from_every_holder_or_of_another_name_ticks_none() {
        assert_eq!(take("Obj_FakeGreen", [50.0, 0.0, 10.0], &[]), None);
        assert_eq!(take("Obj_FakeGreen", [10.0, 9.0, 10.0], &[]), None);
        assert_eq!(take("Obj_FakeBlue", [10.0, 0.0, 10.0], &[]), None);
        assert_eq!(take("Obj_FakeGreen", [100.0, 7.0, 13.0], &[]), Some(4));
    }

    #[test]
    fn a_spot_round_trips_through_its_words_and_far_off_spots_keep_their_sign() {
        let far = spot(u32::MAX, "Obj_FakeGreen", [-32768, -1, 32767]);
        assert_eq!(Spot::from_words(far.words()), far);
        assert_eq!(
            (Spot::key(0, 0), Spot::key(0, 3), Spot::key(1, 0)),
            (SPOT_KEY, SPOT_KEY + 3, SPOT_KEY + 4)
        );
    }

    #[test]
    fn the_name_hash_is_fnvs_low_half() {
        assert_eq!(Spot::name_hash(b"Obj_FakeGreen"), SPOTS[0].name);
        assert_eq!(fnv(b""), 0x811c_9dc5);
    }
}
