//! Where Link lands beside a holder: how far, and from which side, so that
//! what he does there reaches the holder and not its neighbours.

use std::collections::{BTreeMap, HashMap};

use crate::warp::{self, Pose, Side};

/// How far in front of a chest or a scrub Link lands: close enough that A
/// opens or talks once he faces it. The first night's sweep landed 2 m off
/// and never opened a chest.
pub(super) const REACH: f32 = 1.0;
/// How far, in degrees, a chest's landing may swing off its front, tried
/// in order, to keep clear of a chest beside it.
const SWINGS: [f32; 7] = [0.0, -15.0, 15.0, -30.0, 30.0, -45.0, 45.0];
/// How much farther than the holder every other chest must be from the
/// landing. Mido's back chests each have a front one 1.5 ahead.
const CLEAR: f32 = 0.2;
/// How far from a pot, crate or rock another may stand and still be broken
/// by the sword swing that breaks it, or have its drop picked up by the walk
/// after. Gerudo Valley's pot gave its neighbours' items with them 1.6 and
/// 2.4 m off, so 2.5.
pub(super) const NEIGHBOURS: f32 = 2.5;

/// In front of `pose` at `REACH`, swung off its front as little as keeps
/// every one of `others` `CLEAR` farther away than the holder, so A opens
/// the holder and not its neighbour. Straight in front when no swing does.
pub(super) fn clear_landing(pose: &Pose, others: &[[f32; 3]]) -> [f32; 3] {
    let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[2] - b[2]);
    let swung = |degrees: f32| {
        let [x, y, z] = pose.rotate;
        let rotate = [x, y + degrees.to_radians(), z];
        warp::landing_at(
            &Pose {
                at: pose.at,
                rotate,
            },
            Side::Front,
            REACH,
        )
    };
    (SWINGS.iter().map(|&d| swung(d)))
        .find(|&at| others.iter().all(|&o| flat(at, o) >= REACH + CLEAR))
        .unwrap_or_else(|| swung(0.0))
}

/// The breakables within [`NEIGHBOURS`] of the one at `own`, other than
/// `hash` itself: each one's item and where it stands. `items` holds every
/// breakable spot's item by placement hash, `poses` the map's placements.
pub(super) fn neighbours(
    hash: u64,
    own: [f32; 3],
    poses: &HashMap<u64, Pose>,
    items: &BTreeMap<u64, String>,
) -> Vec<(String, [f32; 3])> {
    let mut found: Vec<_> = (items.iter())
        .filter(|&(&h, _)| h != hash)
        .filter_map(|(h, item)| Some((item, poses.get(h)?.at)))
        .filter(|&(_, at)| flat(at, own) <= NEIGHBOURS)
        .map(|(item, at)| (item.clone(), at))
        .collect();
    found.sort_by(|a, b| flat(a.1, own).total_cmp(&flat(b.1, own)));
    found
}

/// The distance between two points, measured flat.
fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// The side of the holder at `pose` to land on, so that Link faces away
/// from `near`: the one whose landing lies most toward the nearest of them
/// it can, the sword then swinging at the holder with them behind him.
/// The front when nothing stands near.
pub(super) fn away_from(pose: &Pose, near: &[[f32; 3]]) -> Side {
    let unit = |from: [f32; 3], to: [f32; 3]| {
        let n = flat(to, from).max(1e-3);
        [(to[0] - from[0]) / n, (to[2] - from[2]) / n]
    };
    // The worst a neighbour does to the side: how far behind the holder
    // from Link it lies, as the cosine of the angle off his back.
    let cover = |side| {
        let link = unit(pose.at, warp::landing_at(pose, side, REACH));
        (near.iter())
            .map(|&n| {
                let n = unit(pose.at, n);
                link[0] * n[0] + link[1] * n[1]
            })
            .fold(f32::INFINITY, f32::min)
    };
    (warp::sides_from(Side::Front))
        .reduce(|best, side| {
            if cover(side) > cover(best) {
                side
            } else {
                best
            }
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chest_lands_clear_of_the_chest_in_front_of_it() {
        // Mido's Back Right, and the Front Right chest 1.5 ahead of it.
        let back_right = Pose {
            at: [-50.3, 112.3, -42.0],
            rotate: [0.0, -0.48513162, 0.0],
        };
        let front_right = [-49.3, 112.3, -40.9];
        let ahead = warp::landing_at(&back_right, Side::Front, REACH);
        assert!(
            flat(ahead, front_right) < REACH,
            "straight ahead is crowded"
        );
        let at = clear_landing(&back_right, &[front_right]);
        assert!((flat(at, back_right.at) - REACH).abs() < 1e-4);
        assert!(flat(at, front_right) >= REACH + CLEAR, "{at:?}");
        assert_eq!(clear_landing(&back_right, &[]), ahead, "alone, straight on");
    }

    /// A pot at the origin whose front is +Z.
    fn pot() -> Pose {
        Pose {
            at: [0.0; 3],
            rotate: [0.0; 3],
        }
    }

    #[test]
    fn a_pot_is_struck_from_the_side_its_neighbour_is_behind_link() {
        // Alone, or with its neighbour in front, the front stands.
        assert_eq!(away_from(&pot(), &[]), Side::Front);
        assert_eq!(away_from(&pot(), &[[0.0, 0.0, 1.6]]), Side::Front);
        // A neighbour behind the pot lies in the front swing's path.
        assert_eq!(away_from(&pot(), &[[0.0, 0.0, -1.6]]), Side::Back);
        // One to a side is kept behind him from the other.
        let to_the_left = warp::landing_at(&pot(), Side::Left, 1.6);
        assert_eq!(away_from(&pot(), &[to_the_left]), Side::Left);
        let to_the_right = warp::landing_at(&pot(), Side::Right, 2.4);
        assert_eq!(away_from(&pot(), &[to_the_right]), Side::Right);
        // One behind and one in front: a side swings across neither.
        assert_eq!(
            away_from(&pot(), &[[0.0, 0.0, -1.6], [0.0, 0.0, 2.4]]),
            Side::Left
        );
    }

    #[test]
    fn only_the_breakables_within_a_swings_reach_are_neighbours() {
        let at = |x: f32| Pose {
            at: [x, 0.0, 0.0],
            rotate: [0.0; 3],
        };
        let poses: HashMap<u64, Pose> =
            [(1, at(0.0)), (2, at(1.6)), (3, at(2.4)), (4, at(9.0))].into();
        let items = (1..=5).map(|h| (h, format!("item {h}"))).collect();
        let found = neighbours(1, [0.0; 3], &poses, &items);
        let names: Vec<_> = found.iter().map(|(i, _)| i.as_str()).collect();
        // Nearest first. Hash 4 is too far, and 5 has no placement here.
        assert_eq!(names, ["item 2", "item 3"]);
    }
}
