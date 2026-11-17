//! What's around Link: the current map's placements, nearest first, each
//! with a clock bearing from his facing. Placements don't move, so a
//! rolling Goron or a dropped item isn't where this says.

use std::f32::consts::{PI, TAU};
use std::fmt;
use std::sync::LazyLock;

use anyhow::Result;
use ounce_formats::Romfs;
use serde::Serialize;

use crate::checks::flat;
use crate::dev::Dev;
use crate::known::{field, flag};
use crate::live::Memory;
use crate::warp::{self, Placement};

/// Name prefixes of what Link can see or touch. The rest (terrain, areas,
/// anchors, sound and effect locators) shows with `all`.
static SEEN: LazyLock<[&str; 17]> = LazyLock::new(|| {
    [
        "Animal",
        field::BELT_CONVEYOR,
        "Enemy",
        "FairySpirit",
        "FldObj",
        "Gimmick",
        "Item",
        "MiniGame",
        "MinusObj",
        "Npc",
        "Obj",
        "OcarinaPoint",
        "Scrub",
        "Shop",
        "Tree",
        flag::WALL_CRACK,
        "Weapon",
    ]
});

/// Link, and what's placed near him.
#[derive(Debug, Serialize)]
pub struct Around {
    pub map: String,
    pub link: [f32; 3],
    /// Radians: he faces `(sin, 0, cos)` of it.
    pub yaw: f32,
    pub near: Vec<Near>,
}

/// One placement near Link.
#[derive(Debug, PartialEq, Serialize)]
pub struct Near {
    pub name: String,
    pub hash: Option<u64>,
    /// Measured flat.
    pub distance: f32,
    /// Above Link, or below when negative.
    pub height: f32,
    /// The clock bearing from his facing: 12 ahead, 3 to his right.
    pub hour: u8,
    /// Where it stands, for callers that measure more than the bearing.
    #[serde(skip)]
    pub at: [f32; 3],
}

/// `to`'s angle clockwise of where Link at `from` faces by `yaw`, in
/// radians from -π to π.
pub fn clockwise(yaw: f32, from: [f32; 3], to: [f32; 3]) -> f32 {
    let want = (to[0] - from[0]).atan2(to[2] - from[2]);
    (yaw - want + PI).rem_euclid(TAU) - PI
}

/// The clock hour of an angle clockwise of ahead.
pub fn hour(clockwise: f32) -> u8 {
    match ((clockwise / (TAU / 12.0)).round() as i32).rem_euclid(12) {
        0 => 12,
        h => h as u8,
    }
}

pub(crate) fn seen(name: &str) -> bool {
    SEEN.iter().any(|p| {
        name.strip_prefix(p)
            .is_some_and(|rest| rest.starts_with('_'))
    })
}

/// Of `placed`, what's within `radius` of Link at `link` facing by `yaw`,
/// nearest first. Only what he can see or touch, unless `all`.
pub fn near(placed: Vec<Placement>, link: [f32; 3], yaw: f32, radius: f32, all: bool) -> Vec<Near> {
    let mut near: Vec<Near> = placed
        .into_iter()
        .filter(|p| all || seen(&p.name))
        .map(|p| Near {
            distance: flat(link, p.pose.at),
            height: p.pose.at[1] - link[1],
            hour: hour(clockwise(yaw, link, p.pose.at)),
            at: p.pose.at,
            name: p.name,
            hash: p.hash,
        })
        .filter(|n| n.distance <= radius)
        .collect();
    near.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    near
}

/// What's around Link in the running game.
pub fn read<M: Memory>(dev: &Dev<M>, romfs: &Romfs, radius: f32, all: bool) -> Result<Around> {
    let map = dev.state()?.map;
    let (link, yaw) = (dev.link()?, dev.yaw()?);
    let near = near(warp::placements_in(romfs, &map)?, link, yaw, radius, all);
    Ok(Around {
        map,
        link,
        yaw,
        near,
    })
}

impl fmt::Display for Around {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let [x, y, z] = self.link;
        writeln!(f, "Link in {} at {x:.1},{y:.1},{z:.1}.", self.map)?;
        writeln!(
            f,
            "Hours are from his facing, 12 ahead and 3 to his right. After ZL the camera is behind him, so the hour is also the stick direction."
        )?;
        writeln!(f, "distance height  hour  name hash")?;
        for n in &self.near {
            let hash = n.hash.map(|h| h.to_string()).unwrap_or_default();
            writeln!(
                f,
                "{:>8.1} {:>+6.1} {:>2}:00  {} {hash}",
                n.distance, n.height, n.hour, n.name
            )?;
        }
        write!(f, "{} placements", self.near.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, flag};
    use std::f32::consts::FRAC_PI_2;

    use super::*;
    use crate::game::dev_dump;
    use crate::warp::Pose;

    const HERE: [f32; 3] = [0.0, 0.0, 0.0];

    fn placed(name: &str, at: [f32; 3]) -> Placement {
        let pose = Pose {
            at,
            rotate: [0.0; 3],
            scale: [1.0; 3],
        };
        Placement {
            name: name.into(),
            hash: Some(1),
            label: None,
            pose,
            presence: None,
        }
    }

    #[test]
    fn hours_run_clockwise_from_his_facing() {
        let at = |yaw, to| hour(clockwise(yaw, HERE, to));
        assert_eq!(at(0.0, [0.0, 0.0, 5.0]), 12);
        assert_eq!(at(0.0, [-5.0, 0.0, 0.0]), 3);
        assert_eq!(at(0.0, [0.0, 0.0, -5.0]), 6);
        assert_eq!(at(0.0, [5.0, 0.0, 0.0]), 9);
        assert_eq!(at(FRAC_PI_2, [5.0, 0.0, 0.0]), 12);
        assert_eq!(at(FRAC_PI_2, [0.0, 0.0, 5.0]), 3);
        assert_eq!(at(0.0, [-5.0, 0.0, 8.66]), 1);
    }

    #[test]
    fn near_lists_what_he_can_see_nearest_first() {
        let placed = vec![
            placed("Obj_F", [0.0, 0.0, 9.0]),
            placed("FakeLand_Floor", [0.0, 0.0, 1.0]),
            placed("Npc_N", [0.0, 2.0, 3.0]),
            placed("Obj_O", [0.0, 0.0, 20.0]),
        ];
        let near = near(placed, HERE, 0.0, 15.0, false);
        let names: Vec<_> = near.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Npc_N", "Obj_F"]);
        assert_eq!(
            (near[0].distance, near[0].height, near[0].hour),
            (3.0, 2.0, 12)
        );
        assert!(!seen("Objective_FakeThing") && seen("Obj_T"));
    }

    #[test]
    fn a_map_s_placements_are_read_once_each() {
        let Some(romfs) = dev_dump() else { return };
        let placed = warp::placements_in(&romfs, flag::GORON_CITY_ADULT).unwrap();
        assert!(
            placed
                .iter()
                .any(|p| p.name.starts_with(&**actor::NPC_GORON))
        );
        let hashes: Vec<_> = placed.iter().filter_map(|p| p.hash).collect();
        let unique: std::collections::HashSet<_> = hashes.iter().collect();
        assert_eq!(hashes.len(), unique.len());
        assert!(warp::placements_in(&romfs, "Nowhere").unwrap().is_empty());
    }
}
