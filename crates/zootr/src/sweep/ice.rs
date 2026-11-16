//! Holders frozen in red ice, and the Blue Fire that melts it. Only a pour
//! melts red ice, and no GameData records it (Mac, 2026-11-15), so the
//! sweep pours before it uses such a holder.

use std::time::Duration;

use crate::known::{Name, actor};
use crate::nav::graph::dist;

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

/// The ice of `ices` that encases a holder standing at `at`, if any.
pub fn encasing(ices: &[[f32; 3]], at: [f32; 3]) -> Option<[f32; 3]> {
    (ices.iter().copied())
        .filter(|&ice| dist(ice, at) < ENCASES)
        .min_by(|&a, &b| dist(a, at).total_cmp(&dist(b, at)))
}

/// The buttons, each with the wait after it, that put a just-given bottle
/// on ZR: the Items screen opens on the newest item. The item ring never
/// selected it (12 pushes).
pub const EQUIP: [(&str, Duration); 3] = [
    ("Plus", Duration::from_millis(2500)),
    ("A", Duration::from_millis(500)),
    ("Plus", Duration::from_secs(3)),
];

/// How long the ice takes to melt after the pour: gone about 7 s after ZR.
pub const MELT: Duration = Duration::from_secs(8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ice_within_reach_encases_and_the_nearest_wins() {
        let ices = [[0.0, 0.0, 2.0], [0.0, 0.0, 0.5], [0.3, 0.0, 0.0]];
        assert_eq!(encasing(&ices, [0.0; 3]), Some([0.3, 0.0, 0.0]));
        assert_eq!(encasing(&ices, [0.0, 0.0, 5.0]), None);
        assert_eq!(encasing(&[], [0.0; 3]), None);
    }
}
