//! An item Link uses on something by a holder before he uses the holder:
//! Blue Fire poured on the red ice it's frozen in, or the Megaton Hammer
//! driven onto the pillar pinning down the switch that opens it. Only a
//! pour melts red ice and no GameData records it (Mac, 2026-11-15), and
//! the Spirit Trial's flames around a chest go out only under the pounded
//! pillar (Mac, 2026-11-17).

use std::time::Duration;

use crate::known::{Name, actor, item};
use crate::nav::flat_dir;
use crate::nav::graph::dist;

/// Give `item`, put it on ZR, face `on` and press ZR, then wait `after`
/// for what it does, and stand on `press` when there's a switch to.
#[derive(Clone, Debug, PartialEq)]
pub struct Wield {
    pub item: &'static Name,
    pub on: [f32; 3],
    pub after: Duration,
    pub press: Option<[f32; 3]>,
}

/// Every red ice there is.
pub const RED_ICE: [&Name; 6] = [
    actor::OBJ_RED_ICE_A_01,
    actor::OBJ_RED_ICE_A_04,
    actor::OBJ_RED_ICE_A_06,
    actor::OBJ_RED_ICE_A_07,
    actor::OBJ_RED_ICE_A_08,
    actor::OBJ_RED_ICE_A_09,
];

/// How close ice stands to a holder it encases. The Ice Cavern's frozen
/// pots and chest are 0.1 to 0.6 m from theirs, and no other holder there
/// comes within 1.5 m of any.
const ENCASES: f32 = 0.75;

/// The pour that melts the ice of `ices` encasing a holder standing at
/// `at`, the nearest, if any does.
pub fn melt(ices: &[[f32; 3]], at: [f32; 3]) -> Option<Wield> {
    let ice = (ices.iter().copied())
        .filter(|&ice| dist(ice, at) < ENCASES)
        .min_by(|&a, &b| dist(a, at).total_cmp(&dist(b, at)))?;
    Some(Wield {
        item: actor::ITEM_BOTTLE_BLUE_FIRE,
        on: ice,
        after: MELT,
        press: None,
    })
}

/// How long the ice takes to melt after the pour: gone about 7 s after ZR.
const MELT: Duration = Duration::from_secs(8);

/// The pillar a hammer drives down.
pub const PILLAR: &Name = actor::OBJ_FIRE_TEMPLE_HAMMER_SLIP_PILLAR_A_01;

/// How close a pillar stands to a switch it pins: the Spirit Trial's is
/// 0.4 m off its switch, and the nearest other pillar 7 m.
const PINS: f32 = 0.75;

/// The pound on the pillar of `pillars` pinning down a switch of
/// `switches`, if one does, and the switch to stand on once it slides off.
/// The pound alone left the flames up (other machine, 2026-11-17).
pub fn pound(pillars: &[[f32; 3]], switches: &[[f32; 3]]) -> Option<Wield> {
    let pinned = |&p: &[f32; 3]| Some((p, *switches.iter().find(|&&s| dist(p, s) < PINS)?));
    let (pillar, switch) = pillars.iter().find_map(pinned)?;
    Some(Wield {
        item: item::HAMMER,
        on: pillar,
        after: SLIDES,
        press: Some(switch),
    })
}

/// How long the pillar takes to slide off its switch: down in the frame
/// read 3 s after ZR.
const SLIDES: Duration = Duration::from_secs(3);

/// How long Link stands on a switch: it reads him in 15 frames.
pub const PRESSED: Duration = Duration::from_secs(1);

/// How far from the thing it acts on Link can wield: a pour on ice in
/// front of him from the holder's landing, a hammer's swing.
pub const REACH: f32 = 2.0;

/// How far from the thing it acts on Link stands to wield when he warps
/// to it: a hammer swung from 1.4 m only glanced off the pillar's side.
const STEP: f32 = 1.0;

/// Where Link warps to wield on `on` from `from`: [`STEP`] from it on the
/// way to `from`, at its height.
pub fn beside(on: [f32; 3], from: [f32; 3]) -> [f32; 3] {
    let [ux, uz] = flat_dir(on, from);
    [on[0] + ux * STEP, on[1], on[2] + uz * STEP]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ice_within_reach_is_melted_and_the_nearest_wins() {
        let ices = [[0.0, 0.0, 2.0], [0.0, 0.0, 0.5], [0.3, 0.0, 0.0]];
        let melted = melt(&ices, [0.0; 3]).unwrap();
        assert_eq!(
            (melted.item, melted.on),
            (actor::ITEM_BOTTLE_BLUE_FIRE, [0.3, 0.0, 0.0])
        );
        assert_eq!(melt(&ices, [0.0, 0.0, 5.0]), None);
        assert_eq!(melt(&[], [0.0; 3]), None);
    }

    #[test]
    fn only_a_pillar_on_a_switch_is_pounded() {
        let pillars = [[7.0, 0.0, 0.0], [0.4, 0.5, 0.0]];
        let pounded = pound(&pillars, &[[0.0; 3]]).unwrap();
        assert_eq!((pounded.item, pounded.on), (item::HAMMER, [0.4, 0.5, 0.0]));
        assert_eq!(pounded.press, Some([0.0; 3]));
        assert_eq!(pound(&pillars[..1], &[[0.0; 3]]), None);
        assert_eq!(pound(&pillars, &[]), None);
    }

    #[test]
    fn link_warps_a_step_from_what_he_wields_on_toward_where_he_stood() {
        assert_eq!(beside([1.0, 2.0, 0.0], [11.0, 9.0, 0.0]), [2.0, 2.0, 0.0]);
    }
}
