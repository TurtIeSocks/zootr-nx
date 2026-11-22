//! The faces of a map's navmesh as a graph. Faces that share an edge are
//! linked both ways. A door, a climb, a drop and a step up add links the
//! mesh alone doesn't have, and a route keeps each as a waypoint.

use std::collections::HashMap;
use std::fmt;

use ounce_formats::{Footing, NavMesh, closest};
use serde::Serialize;

use super::climb::Climb;
use super::kinds;
use super::limits::limits;
use super::shape::Solid;
use super::water::{Shore, Water};
use crate::warp::{self, Placement, Side};

/// The highest ledge Link gets onto from below, in m, from
/// `logic/limits.toml`.
pub fn step_up() -> f32 {
    limits().step_up()
}
/// A drop is a border with mesh past it at least `DROP_MIN` below, and at
/// most the drop limit in `logic/limits.toml`.
pub(super) const DROP_MIN: f32 = 0.5;
/// A step up starts at this height.
pub(super) const STEP_MIN: f32 = 0.5;
/// How far past a border to look for the mesh across it.
pub(super) const LOOK: f32 = 0.5;
/// How far on each side of a door the nearest face may be.
const DOOR_REACH: f32 = 2.5;
/// How far from a climb's foot or top the floor there may be. The mesh
/// keeps off the wall a climb stands on, and a vine can hang down past the
/// ledge it starts from (Kokiri Forest's, 2.9 m).
const CLIMB_REACH: f32 = 3.5;
/// How far off the mesh a start or goal snaps to the nearest face.
const SNAP: f32 = 1.5;
/// How far from a point's height the floor under it may be.
pub(super) const FLOOR: f32 = 3.0;
/// The size of the cells faces are looked up by.
const CELL: f32 = 4.0;

/// How a waypoint is reached from the one before it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Walk,
    /// Through the door placed with this hash.
    Door(Option<u64>),
    Drop,
    Step,
    /// Up or down the ladder or climb wall placed with this hash.
    Climb(Option<u64>),
    /// Across the water placed with this hash.
    Swim(Option<u64>),
}

impl fmt::Display for Via {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Via::Walk => write!(f, "walk"),
            Via::Door(Some(hash)) => write!(f, "door {hash}"),
            Via::Door(None) => write!(f, "door"),
            Via::Drop => write!(f, "drop"),
            Via::Step => write!(f, "step"),
            Via::Climb(Some(hash)) => write!(f, "climb {hash}"),
            Via::Climb(None) => write!(f, "climb"),
            Via::Swim(Some(hash)) => write!(f, "swim {hash}"),
            Via::Swim(None) => write!(f, "swim"),
        }
    }
}

/// What a route crosses to get from one face to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cross {
    /// The edge two faces share, its ends as seen from the first face.
    Edge([[f32; 3]; 2]),
    /// A link: leave the first face at `from`, arrive on the second at `to`.
    Link {
        via: Via,
        from: [f32; 3],
        to: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub to: usize,
    pub cost: f32,
    pub cross: Cross,
}

/// An edge of the mesh's border: where a wall, a drop or a shore begins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Border {
    pub face: usize,
    /// The middle of the edge.
    pub mid: [f32; 3],
    /// Flat and of length 1, out of the face.
    pub out: [f32; 2],
}

/// A spot on a face: where a point off the mesh was put.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub face: usize,
    pub at: [f32; 3],
}

pub struct Graph<'a> {
    pub mesh: &'a NavMesh,
    /// The placed solids the mesh doesn't know, which a route goes round.
    pub solids: &'a [Solid],
    pub centers: Vec<[f32; 3]>,
    pub arcs: Vec<Vec<Arc>>,
    /// The waters a route swims, and the shore points it swims between.
    pub waters: Vec<Water>,
    pub shores: Vec<Shore>,
    /// How deep water is before Link floats in it, at his age.
    pub float: f32,
    /// The faces whose corners' bounds touch each cell, by x and z.
    cells: HashMap<(i32, i32), Vec<usize>>,
}

pub fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2]).hypot(a[1] - b[1])
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl<'a> Graph<'a> {
    /// The graph of `mesh`, with a door link for each door in `placed` and
    /// a step up link for each ledge up to `step_up` high.
    pub fn build(mesh: &'a NavMesh, placed: &[Placement], step_up: f32) -> Self {
        let centers: Vec<[f32; 3]> = (0..mesh.faces.len())
            .map(|f| {
                let ring = mesh.ring(f);
                let sum = ring.iter().fold([0.0; 3], |s, e| {
                    let v = mesh.vertices[e.a];
                    [s[0] + v[0], s[1] + v[1], s[2] + v[2]]
                });
                sum.map(|c| c / ring.len().max(1) as f32)
            })
            .collect();
        let mut graph = Self {
            mesh,
            solids: &[],
            arcs: vec![vec![]; centers.len()],
            waters: vec![],
            shores: vec![],
            float: super::water::float(false),
            cells: Self::cells(mesh),
            centers,
        };
        graph.edge_arcs();
        graph.border_arcs(step_up);
        graph.door_arcs(placed);
        graph
    }

    /// The graph with a link both ways up each of `climbs` whose foot and
    /// top [`Graph::climb_ends`] finds.
    pub fn with_climbs(mut self, climbs: &[Climb]) -> Self {
        for c in climbs {
            if let Ok((foot, top)) = self.climb_ends(c) {
                self.link(foot.face, top.face, Via::Climb(c.hash), foot.at, top.at);
                self.link(top.face, foot.face, Via::Climb(c.hash), top.at, foot.at);
            }
        }
        self
    }

    /// The floor at the foot of `c` and at its top, or why there's no
    /// climb: the mesh nearest each, within [`CLIMB_REACH`], below its
    /// middle for the foot and above it for the top.
    pub fn climb_ends(&self, c: &Climb) -> std::result::Result<(Snap, Snap), &'static str> {
        let middle = (c.foot[1] + c.top[1]) / 2.0;
        let foot = (self.nearest(c.foot, CLIMB_REACH, |q| q[1] <= middle))
            .ok_or("no floor at its foot")?;
        let top =
            (self.nearest(c.top, CLIMB_REACH, |q| q[1] >= middle)).ok_or("no floor at its top")?;
        if top.face == foot.face || top.at[1] - foot.at[1] < STEP_MIN {
            return Err("its top is no higher than its foot");
        }
        Ok((foot, top))
    }

    /// The graph with `solids` for its routes to go round.
    pub fn with_solids(self, solids: &'a [Solid]) -> Self {
        Self { solids, ..self }
    }

    fn cells(mesh: &NavMesh) -> HashMap<(i32, i32), Vec<usize>> {
        let mut cells: HashMap<_, Vec<usize>> = HashMap::new();
        for f in 0..mesh.faces.len() {
            let corners = mesh.ring(f).iter().map(|e| mesh.vertices[e.a]);
            let (lo, hi) = corners.fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), v| {
                (
                    [lo[0].min(v[0]), lo[1].min(v[2])],
                    [hi[0].max(v[0]), hi[1].max(v[2])],
                )
            });
            for cx in cell(lo[0])..=cell(hi[0]) {
                for cz in cell(lo[1])..=cell(hi[1]) {
                    cells.entry((cx, cz)).or_default().push(f);
                }
            }
        }
        cells
    }

    /// The faces that may lie within `r` of (x, z).
    fn faces_near(&self, x: f32, z: f32, r: f32) -> Vec<usize> {
        let mut found: Vec<usize> = (cell(x - r)..=cell(x + r))
            .flat_map(|cx| (cell(z - r)..=cell(z + r)).map(move |cz| (cx, cz)))
            .filter_map(|c| self.cells.get(&c))
            .flatten()
            .copied()
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The corners of the mesh's border (where a wall or a drop begins)
    /// within `r` of (x, z), measured flat.
    pub fn border_corners(&self, x: f32, z: f32, r: f32) -> Vec<[f32; 3]> {
        let mesh = self.mesh;
        let ends = |f: usize| {
            (mesh.ring(f).iter())
                .filter(|e| e.across.is_none())
                .flat_map(|e| [e.a, e.b])
        };
        let mut found: Vec<usize> = (self.faces_near(x, z, r).into_iter())
            .flat_map(ends)
            .filter(|&v| (mesh.vertices[v][0] - x).hypot(mesh.vertices[v][2] - z) <= r)
            .collect();
        found.sort_unstable();
        found.dedup();
        found.into_iter().map(|v| mesh.vertices[v]).collect()
    }

    /// The floor under `p` nearest its height, within `reach`.
    pub fn floor(&self, p: [f32; 3], reach: f32) -> Option<Footing> {
        self.mesh
            .footing_among(self.faces_near(p[0], p[2], 0.0), p, reach)
    }

    /// The spot of the mesh nearest `p` within `reach`, among those `keep`
    /// allows.
    fn nearest(&self, p: [f32; 3], reach: f32, keep: impl Fn([f32; 3]) -> bool) -> Option<Snap> {
        let mesh = self.mesh;
        (self.faces_near(p[0], p[2], reach).into_iter())
            .flat_map(|f| {
                mesh.ring(f)
                    .iter()
                    .map(move |e| (f, closest(mesh.vertices[e.a], mesh.vertices[e.b], p)))
            })
            .filter(|&(_, at)| dist(at, p) <= reach && keep(at))
            .min_by(|a, b| dist(a.1, p).total_cmp(&dist(b.1, p)))
            .map(|(face, at)| Snap { face, at })
    }

    /// Where `p` stands on the mesh, or the nearest face within 1.5 m of
    /// it when it's off.
    pub fn locate(&self, p: [f32; 3]) -> Option<Snap> {
        match self.floor(p, FLOOR) {
            Some(f) => Some(Snap {
                face: f.face,
                at: [p[0], f.height, p[2]],
            }),
            None => self.nearest(p, SNAP, |_| true),
        }
    }

    fn edge_arcs(&mut self) {
        let mesh = self.mesh;
        for f in 0..mesh.faces.len() {
            for e in mesh.ring(f) {
                let Some(to) = e.across.filter(|&to| to != f) else {
                    continue;
                };
                self.arcs[f].push(Arc {
                    to,
                    cost: dist(self.centers[f], self.centers[to]),
                    cross: Cross::Edge([mesh.vertices[e.a], mesh.vertices[e.b]]),
                });
            }
        }
    }

    /// Add the link leaving face `from` at `leave` and arriving on face
    /// `to` at `arrive`.
    fn link(&mut self, from: usize, to: usize, via: Via, leave: [f32; 3], arrive: [f32; 3]) {
        self.link_weighted(from, to, via, [leave, arrive], 1.0);
    }

    /// [`Graph::link`], each m from `leave` to `arrive` costing `weight`
    /// m walked.
    pub(super) fn link_weighted(
        &mut self,
        from: usize,
        to: usize,
        via: Via,
        [leave, arrive]: [[f32; 3]; 2],
        weight: f32,
    ) {
        let cost = dist(self.centers[from], leave)
            + weight * dist(leave, arrive)
            + dist(arrive, self.centers[to]);
        self.arcs[from].push(Arc {
            to,
            cost,
            cross: Cross::Link {
                via,
                from: leave,
                to: arrive,
            },
        });
    }

    /// A drop from each border with mesh 0.5 m to the drop limit below past
    /// it, a step up from each with mesh 0.5 m to `step_up` above.
    fn border_arcs(&mut self, step_up: f32) {
        let drop = limits().drop();
        for Border { face, mid, out } in self.borders() {
            let past = [mid[0] + out[0] * LOOK, mid[1], mid[2] + out[1] * LOOK];
            let Some(g) = self.floor(past, FLOOR.max(step_up).max(drop)) else {
                continue;
            };
            let rel = g.height - mid[1];
            let via = if (-drop..=-DROP_MIN).contains(&rel) {
                Via::Drop
            } else if (STEP_MIN..=step_up).contains(&rel) {
                Via::Step
            } else {
                continue;
            };
            if g.face != face {
                self.link(face, g.face, via, mid, [past[0], g.height, past[2]]);
            }
        }
    }

    /// Every edge of the mesh's border with a length.
    pub(super) fn borders(&self) -> Vec<Border> {
        let mesh = self.mesh;
        let mut found = vec![];
        for face in 0..mesh.faces.len() {
            for e in mesh.ring(face).iter().filter(|e| e.across.is_none()) {
                let (a, b) = (mesh.vertices[e.a], mesh.vertices[e.b]);
                let mid: [f32; 3] = std::array::from_fn(|i| (a[i] + b[i]) / 2.0);
                let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
                let len = dx.hypot(dz);
                if len < 1e-4 {
                    continue;
                }
                // Out of the face: the normal that points away from its center.
                let mut out = [dz / len, -dx / len];
                let c = self.centers[face];
                if out[0] * (mid[0] - c[0]) + out[1] * (mid[2] - c[2]) < 0.0 {
                    out = [-out[0], -out[1]];
                }
                found.push(Border { face, mid, out });
            }
        }
        found
    }

    /// A link both ways through each door: the nearest face on each side of
    /// it, by its facing, within 2.5 m. The mesh around a doorway is not
    /// centered on the door, so the faces are the nearest anywhere on a
    /// side, not the first along a line through its center.
    fn door_arcs(&mut self, placed: &[Placement]) {
        for door in placed
            .iter()
            .filter(|p| kinds::word(&p.name) == Some("door"))
        {
            let [x, y, z] = door.pose.at;
            let [ahead_x, _, ahead_z] = warp::landing_at(&door.pose, Side::Front, 1.0);
            let facing = [ahead_x - x, ahead_z - z];
            let side = |sign: f32| {
                let keep =
                    |q: [f32; 3]| sign * ((q[0] - x) * facing[0] + (q[2] - z) * facing[1]) > 0.0;
                self.nearest([x, y, z], DOOR_REACH, keep)
            };
            let (Some(front), Some(back)) = (side(1.0), side(-1.0)) else {
                continue;
            };
            if front.face != back.face {
                let via = Via::Door(door.hash);
                self.link(front.face, back.face, via, front.at, back.at);
                self.link(back.face, front.face, via, back.at, front.at);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;
    use crate::nav::fixture::{mesh, quad};
    use crate::warp::Pose;

    fn door(at: [f32; 3]) -> Placement {
        Placement {
            name: actor::OBJ_KOKIRI_DOOR_A_02.to_string(),
            hash: Some(7),
            label: None,
            pose: Pose {
                at,
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        }
    }

    fn links(graph: &Graph, from: usize) -> Vec<(usize, Via)> {
        graph.arcs[from]
            .iter()
            .filter_map(|a| match a.cross {
                Cross::Link { via, .. } => Some((a.to, via)),
                Cross::Edge(_) => None,
            })
            .collect()
    }

    #[test]
    fn faces_that_share_an_edge_link_both_ways() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(2.0, 0.0, 4.0, 2.0, 0.0)]);
        let g = Graph::build(&m, &[], step_up());
        assert_eq!(g.arcs[0].iter().map(|a| a.to).collect::<Vec<_>>(), [1]);
        assert_eq!(g.arcs[1].iter().map(|a| a.to).collect::<Vec<_>>(), [0]);
        assert!(links(&g, 0).is_empty());
    }

    #[test]
    fn a_door_links_the_faces_on_each_side_both_ways() {
        needs_names!();
        // A 3 m doorway between two rooms, the door in its middle facing +Z.
        let m = mesh(&[
            quad(0.0, -3.0, 2.0, 0.5, 0.0),
            quad(0.0, 3.5, 2.0, 7.0, 0.0),
        ]);
        let g = Graph::build(&m, &[door([1.0, 0.0, 2.0])], step_up());
        assert_eq!(links(&g, 0), [(1, Via::Door(Some(7)))]);
        assert_eq!(links(&g, 1), [(0, Via::Door(Some(7)))]);
    }

    /// A ladder 4 m up a wall at z 0.4, from the floor before it to the
    /// ledge over it.
    fn ladder() -> Climb {
        Climb {
            name: "Obj_FakeLadder".into(),
            hash: Some(9),
            foot: [1.0, 0.0, 0.4],
            top: [1.0, 4.0, 0.4],
            out: [0.0, 1.0],
            half: 0.1,
        }
    }

    #[test]
    fn a_climb_links_the_floor_at_its_foot_and_the_ledge_at_its_top_both_ways() {
        let m = mesh(&[
            quad(0.0, -3.0, 2.0, 0.2, 0.0),
            quad(0.0, 0.6, 2.0, 4.0, 4.0),
        ]);
        let g = Graph::build(&m, &[], step_up()).with_climbs(&[ladder()]);
        assert_eq!(links(&g, 0), [(1, Via::Climb(Some(9)))]);
        // Down, the 4 m ledge is also within the drop limit.
        let down = links(&g, 1);
        assert!(down.contains(&(0, Via::Climb(Some(9)))), "{down:?}");
        assert!(down.contains(&(0, Via::Drop)), "{down:?}");
    }

    #[test]
    fn a_climb_with_no_ledge_at_its_top_links_nothing() {
        let m = mesh(&[
            quad(0.0, -3.0, 2.0, 0.2, 0.0),
            quad(0.0, 0.6, 2.0, 4.0, 9.0),
        ]);
        let g = Graph::build(&m, &[], step_up());
        assert_eq!(g.climb_ends(&ladder()), Err("no floor at its top"));
        assert!(links(&g.with_climbs(&[ladder()]), 0).is_empty());
    }

    #[test]
    fn a_door_with_no_face_within_reach_links_nothing() {
        let m = mesh(&[
            quad(0.0, -3.0, 2.0, 0.5, 0.0),
            quad(0.0, 5.5, 2.0, 9.0, 0.0),
        ]);
        let g = Graph::build(&m, &[door([1.0, 0.0, 2.0])], step_up());
        assert!(links(&g, 0).is_empty() && links(&g, 1).is_empty());
    }

    /// A floor and a second one `rise` above it, 0.3 m past its east edge,
    /// so the mesh keeps its gap from the ledge.
    fn ledge(rise: f32, step_up: f32) -> [Vec<(usize, Via)>; 2] {
        let m = mesh(&[
            quad(0.0, 0.0, 2.0, 2.0, 0.0),
            quad(2.3, 0.0, 5.0, 2.0, rise),
        ]);
        let g = Graph::build(&m, &[], step_up);
        [links(&g, 0), links(&g, 1)]
    }

    #[test]
    fn a_drop_goes_down_one_way() {
        let [up, down] = ledge(-2.0, step_up());
        assert_eq!(up.first(), Some(&(1, Via::Drop)));
        assert!(down.is_empty(), "{down:?}");
    }

    #[test]
    fn a_step_goes_up_only_up_to_the_limit() {
        let [low, high] = ledge(0.8, step_up());
        assert_eq!(low.first(), Some(&(1, Via::Step)));
        // The same ledge from above is a drop, so each way has its own kind.
        assert_eq!(high, [(0, Via::Drop)]);
        // Above the limit it is no link, and a drop of 0.3 is just a slope.
        assert!(ledge(1.5, step_up())[0].is_empty());
        assert_eq!(ledge(1.5, 2.0)[0].first(), Some(&(1, Via::Step)));
        assert!(ledge(0.3, step_up())[0].is_empty() && ledge(-0.3, step_up())[1].is_empty());
        // Down to the measured drop limit it is a drop, past it a fall.
        let drop = limits().drop();
        assert_eq!(
            ledge(-(drop - 0.5), step_up())[0].first(),
            Some(&(1, Via::Drop))
        );
        assert!(ledge(-(drop + 0.5), step_up())[0].is_empty());
    }

    #[test]
    fn a_spot_off_the_mesh_snaps_to_the_nearest_face_within_reach() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 5.0)]);
        let g = Graph::build(&m, &[], step_up());
        let on = g.locate([1.0, 5.2, 1.0]).unwrap();
        assert_eq!((on.face, on.at), (0, [1.0, 5.0, 1.0]));
        let off = g.locate([3.0, 5.0, 1.0]).unwrap();
        assert_eq!((off.face, off.at), (0, [2.0, 5.0, 1.0]));
        assert_eq!(g.locate([4.0, 5.0, 1.0]), None);
    }
}
