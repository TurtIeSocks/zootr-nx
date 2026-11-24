//! Where Link lands beside a holder: how far, and from which side, so that
//! what he does there reaches the holder and not its neighbours.

use std::collections::{BTreeMap, HashMap};

use crate::nav::flat_dir;
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
    let swung = |degrees: f32| {
        let [x, y, z] = pose.rotate;
        let rotate = [x, y + degrees.to_radians(), z];
        warp::landing_at(&Pose { rotate, ..*pose }, Side::Front, REACH)
    };
    (SWINGS.iter().map(|&d| swung(d)))
        .find(|&at| others.iter().all(|&o| flat(at, o) >= REACH + CLEAR))
        .unwrap_or_else(|| swung(0.0))
}

/// How near a spot may be and still be taken for the one it's measured
/// from, in m.
const ITSELF: f32 = 0.01;

/// The items of the breakables within [`NEIGHBOURS`] of the one at `own`,
/// other than `hash` itself, nearest first. `items` holds every breakable
/// spot's item by placement hash, `poses` the map's placements.
pub(super) fn neighbours(
    hash: u64,
    own: [f32; 3],
    poses: &HashMap<u64, Pose>,
    items: &BTreeMap<u64, String>,
) -> Vec<String> {
    let mut found: Vec<_> = (items.iter())
        .filter(|&(&h, _)| h != hash)
        .filter_map(|(h, item)| Some((item, flat(poses.get(h)?.at, own))))
        .filter(|&(_, off)| off <= NEIGHBOURS)
        .collect();
    found.sort_by(|a, b| a.1.total_cmp(&b.1));
    found.into_iter().map(|(item, _)| item.clone()).collect()
}

/// The spots of `spots` within [`NEIGHBOURS`] of `own`, but `own` itself.
pub(super) fn around(own: [f32; 3], spots: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let near = |at: &&[f32; 3]| (ITSELF..=NEIGHBOURS).contains(&flat(**at, own));
    spots.iter().filter(near).copied().collect()
}

/// The distance between two points, measured flat.
pub(super) fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// The sides of the holder at `pose` to land on, best first, so that Link
/// faces away from `near`: the best one's landing lies most toward the
/// nearest of them it can, the sword then swinging at the holder with them
/// behind him. The front first when nothing stands near.
pub(super) fn away_from(pose: &Pose, near: &[[f32; 3]]) -> Vec<Side> {
    rank(pose, near, |cos| cos)
}

/// The sides of the holder at `pose` to roll at it from, best first, so
/// that `near` lies off the roll's line: its run-up starts 1.4 m out, so a
/// neighbour behind Link is in its path as much as one past the holder. The crater
/// box's front put the Gold Skulltula box 1.9 m off it right behind him,
/// and the roll broke that one (other machine, 2026-11-17).
pub(super) fn off_line(pose: &Pose, near: &[[f32; 3]]) -> Vec<Side> {
    rank(pose, near, |cos: f32| -cos.abs())
}

/// The side of the holder at `pose` whose landing lies furthest from every
/// one of `far`, so Link faces the holder with them beyond it: a keeper's
/// customers stand across the counter from his shelves.
pub(super) fn across_from(pose: &Pose, far: &[[f32; 3]]) -> Side {
    rank(pose, far, |cos| -cos)[0]
}

/// Every side, best first by how its worst point of `near` scores by
/// `score`, of the cosine of the angle between Link's landing and the
/// point, seen from the holder: 1 for a point right behind him. Ties keep
/// [`warp::sides_from`]'s order, the front first.
fn rank(pose: &Pose, near: &[[f32; 3]], score: impl Fn(f32) -> f32) -> Vec<Side> {
    let worst = |side| {
        let link = flat_dir(pose.at, warp::landing_at(pose, side, REACH));
        (near.iter())
            .map(|&n| {
                let n = flat_dir(pose.at, n);
                score(link[0] * n[0] + link[1] * n[1])
            })
            .fold(f32::INFINITY, f32::min)
    };
    let mut sides: Vec<Side> = warp::sides_from(Side::Front).collect();
    sides.sort_by(|&a, &b| worst(b).total_cmp(&worst(a)));
    sides
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
            scale: [1.0; 3],
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
            scale: [1.0; 3],
        }
    }

    #[test]
    fn a_pot_is_struck_from_the_side_its_neighbour_is_behind_link() {
        // Alone, or with its neighbour in front, the front stands.
        assert_eq!(away_from(&pot(), &[])[0], Side::Front);
        assert_eq!(away_from(&pot(), &[[0.0, 0.0, 1.6]])[0], Side::Front);
        // A neighbour behind the pot lies in the front swing's path.
        assert_eq!(away_from(&pot(), &[[0.0, 0.0, -1.6]])[0], Side::Back);
        // One to a side is kept behind him from the other.
        let to_the_left = warp::landing_at(&pot(), Side::Left, 1.6);
        assert_eq!(away_from(&pot(), &[to_the_left])[0], Side::Left);
        let to_the_right = warp::landing_at(&pot(), Side::Right, 2.4);
        assert_eq!(away_from(&pot(), &[to_the_right])[0], Side::Right);
        // One behind and one in front: a side swings across neither.
        assert_eq!(
            away_from(&pot(), &[[0.0, 0.0, -1.6], [0.0, 0.0, 2.4]])[0],
            Side::Left
        );
    }

    #[test]
    fn only_the_breakables_within_a_swings_reach_are_neighbours() {
        let at = |x: f32| Pose {
            at: [x, 0.0, 0.0],
            rotate: [0.0; 3],
            scale: [1.0; 3],
        };
        let poses: HashMap<u64, Pose> =
            [(1, at(0.0)), (2, at(1.6)), (3, at(2.4)), (4, at(9.0))].into();
        let items = (1..=5).map(|h| (h, format!("item {h}"))).collect();
        let found = neighbours(1, [0.0; 3], &poses, &items);
        // Nearest first. Hash 4 is too far, and 5 has no placement here.
        assert_eq!(found, ["item 2", "item 3"]);
    }
}
