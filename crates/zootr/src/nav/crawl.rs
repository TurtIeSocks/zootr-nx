//! Crawl holes: low tunnels only a child crawls through, which the navmesh
//! leaves out. A `CrawlAnchor` stands at each end, facing along the
//! tunnel, so two anchors facing opposite ways along the line between
//! them are one crawl's ends. A route crawls one as a link between the
//! floor at each end, like a door's.

use super::shape::rotation;
use crate::known::actor;
use crate::warp::Placement;

/// The longest crawl looked for, in m. The Deku Tree's compass room one
/// is 5 m.
const LENGTH_MAX: f32 = 30.0;
/// How near to straight along the tunnel two ends face, as a cosine.
const ALONG: f32 = 0.95;

/// A placed crawl, in the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Crawl {
    /// The first end's anchor.
    pub hash: Option<u64>,
    pub ends: [[f32; 3]; 2],
}

/// The way `p` faces, flat and of length 1, up to its sign.
fn facing(p: &Placement) -> [f32; 2] {
    let r = rotation(p.pose.rotate);
    let [x, z] = [r[0][2], r[2][2]];
    let n = x.hypot(z).max(f32::EPSILON);
    [x / n, z / n]
}

/// The crawls `placed` holds: each anchor with the nearest other one
/// facing along the line between them, each pair once.
pub fn crawls(placed: &[Placement]) -> Vec<Crawl> {
    let anchors: Vec<&Placement> = (placed.iter())
        .filter(|p| p.name == **actor::CRAWL_ANCHOR)
        .collect();
    let along = |a: &Placement, b: &Placement| {
        let d = [b.pose.at[0] - a.pose.at[0], b.pose.at[2] - a.pose.at[2]];
        let len = d[0].hypot(d[1]);
        let lines = |f: [f32; 2]| (f[0] * d[0] + f[1] * d[1]).abs() >= ALONG * len;
        (len > 0.0 && len <= LENGTH_MAX && lines(facing(a)) && lines(facing(b))).then_some(len)
    };
    let partner = |i: usize| {
        (0..anchors.len())
            .filter(|&j| j != i)
            .filter_map(|j| along(anchors[i], anchors[j]).map(|d| (j, d)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(j, _)| j)
    };
    (0..anchors.len())
        .filter_map(|i| {
            let j = partner(i)?;
            (i < j && partner(j) == Some(i)).then(|| Crawl {
                hash: anchors[i].hash,
                ends: [anchors[i].pose.at, anchors[j].pose.at],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;
    use crate::nav::graph::dist;

    fn anchor(at: [f32; 3], yaw: f32, hash: u64) -> Placement {
        crate::nav::fixture::turned(actor::CRAWL_ANCHOR, Some(hash), at, [0.0, yaw, 0.0])
    }

    #[test]
    fn two_anchors_facing_along_their_line_are_one_crawl() {
        let placed = [
            anchor([0.0, 0.0, 0.0], FRAC_PI_2, 1),
            anchor([-5.0, 0.0, 0.0], -FRAC_PI_2, 2),
            // Beside the tunnel, facing across it: no end of it.
            anchor([-2.0, 0.0, 3.0], 0.0, 3),
        ];
        let crawls = crawls(&placed);
        assert_eq!(crawls.len(), 1, "{crawls:?}");
        assert_eq!(crawls[0].ends, [[0.0; 3], [-5.0, 0.0, 0.0]]);
    }

    /// The Deku Tree has two crawls: the compass room's to its side room,
    /// and the basement's.
    #[test]
    fn the_deku_tree_has_two_crawls() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let deku = crate::warp::placements_in(&romfs, "InsideTheDekuTree").unwrap();
        let found = crawls(&deku);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found.iter().all(|c| dist(c.ends[0], c.ends[1]) > 4.0),
            "{found:?}"
        );
    }
}
