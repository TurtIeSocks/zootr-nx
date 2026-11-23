//! The faces of a map's navmesh as a graph. Faces that share an edge are
//! linked both ways. A door, a climb, a drop and a step up add links the
//! mesh alone doesn't have, and a route keeps each as a waypoint.

use std::collections::HashMap;
use std::fmt;

use ounce_formats::{Footing, NavMesh, closest};
use serde::Serialize;

use super::climb::Climb;
use super::crawl::Crawl;
use super::kinds;
use super::limits::limits;
use super::mesh::{Wall, Walls};
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
/// How far from a crawl's anchor the floor at its end may be, in m: the
/// navmesh stops short of the hole.
const CRAWL_REACH: f32 = 2.0;
/// How far off the mesh a start or goal snaps to the nearest face.
const SNAP: f32 = 1.5;
/// How far from a point's height the floor under it may be.
pub(super) const FLOOR: f32 = 3.0;
/// The size of the cells faces are looked up by.
const CELL: f32 = 4.0;
/// The heights above the floor a path is tested against walls at: a
/// child Link's waist and head, which an adult's body spans too.
pub(super) const BODY: [f32; 2] = [0.5, 1.0];
/// The height above the floor a jump or fall is tested against solid
/// walls at: over the lips and curbs a floor's mesh has at its edges.
const OVER: [f32; 1] = [1.0];
/// How far below and above the floor a solid under Link's feet is
/// looked for, past the mesh's edge.
const LIP: f32 = 0.3;
/// How far past the mesh's edge a ledge is looked for, and in what steps.
const LEDGE_MAX: f32 = 2.0;
const LEDGE_STEP: f32 = 0.05;
/// Link's body radius, which clears the ledge before he falls.
const RADIUS: f32 = 0.3;
/// How far over a water's surface a swimmer's head is, in m.
const HEAD: f32 = 0.3;
/// How far out from each shore a swim's line is tested, in m: the shore
/// point sits on land, past its edge.
const SHORE_OFF: f32 = 1.0;

/// Whether a path over the floor through `path` meets one of `walls` at
/// any of `heights`.
fn body_hits(walls: &[Wall], path: &[[f32; 3]], heights: &[f32]) -> bool {
    heights.iter().any(|&h| {
        let up = |p: [f32; 3]| [p[0], p[1] + h, p[2]];
        (path.windows(2)).any(|leg| walls.iter().any(|w| w.crosses(up(leg[0]), up(leg[1]))))
    })
}

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
    /// Through the crawl hole whose first anchor has this hash: a child
    /// only.
    Crawl(Option<u64>),
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
            Via::Crawl(Some(hash)) => write!(f, "crawl {hash}"),
            Via::Crawl(None) => write!(f, "crawl"),
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
    /// The invisible walls the mesh runs through, which Link stops at.
    pub walls: &'a [Wall],
    /// The walls and floors Link sees, which a jump or a fall hits.
    pub solid: &'a [Wall],
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
            walls: &[],
            solid: &[],
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
        self.climbs_into_water();
        self
    }

    /// The graph with a link both ways through each of `crawls` with floor
    /// at both ends, within [`CRAWL_REACH`] of its anchor and level with it.
    pub fn with_crawls(mut self, crawls: &[Crawl]) -> Self {
        for c in crawls {
            let end =
                |p: [f32; 3]| self.nearest(p, CRAWL_REACH, |_, q| (q[1] - p[1]).abs() < STEP_MIN);
            if let (Some(a), Some(b)) = (end(c.ends[0]), end(c.ends[1])) {
                self.link(a.face, b.face, Via::Crawl(c.hash), a.at, b.at);
                self.link(b.face, a.face, Via::Crawl(c.hash), b.at, a.at);
            }
        }
        self
    }

    /// The floor at the foot of `c` and at its top, or why there's no
    /// climb: the mesh nearest each, within [`CLIMB_REACH`], below its
    /// middle for the foot and above it for the top.
    pub fn climb_ends(&self, c: &Climb) -> std::result::Result<(Snap, Snap), &'static str> {
        let middle = (c.foot[1] + c.top[1]) / 2.0;
        // Floor under the sheet Link swims on is the pool's bed, not where he
        // grabs on, unless the mesh has no sheet there.
        let low = |q: [f32; 3]| q[1] <= middle;
        let foot = (self.nearest(c.foot, CLIMB_REACH, |_, q| low(q) && !self.sunk(q)))
            .or_else(|| self.nearest(c.foot, CLIMB_REACH, |_, q| low(q)))
            .ok_or("no floor at its foot")?;
        let top = (self.nearest(c.top, CLIMB_REACH, |_, q| q[1] >= middle))
            .ok_or("no floor at its top")?;
        if top.face == foot.face || top.at[1] - foot.at[1] < STEP_MIN {
            return Err("its top is no higher than its foot");
        }
        Ok((foot, top))
    }

    /// The graph with `walls` for its jumps and falls to hit, without the
    /// walks, steps and drops whose path passes through an invisible one at
    /// Link's waist or head: Link stops at an invisible wall the mesh runs
    /// through.
    pub fn with_walls(mut self, walls: &'a Walls) -> Self {
        (self.walls, self.solid) = (&walls.air, &walls.solid);
        for f in 0..self.arcs.len() {
            let from = self.centers[f];
            let mut arcs = std::mem::take(&mut self.arcs[f]);
            arcs.retain(|arc| {
                let path = match arc.cross {
                    // A swim crosses open water: a wall standing out of it
                    // over the line between the shores stops it. Tested at
                    // a swimmer's head, off the shores' own edges.
                    Cross::Link {
                        via: Via::Swim(h),
                        from,
                        to,
                    } => return !self.swim_walled(h, from, to),
                    Cross::Edge([a, b]) => {
                        vec![
                            from,
                            std::array::from_fn(|i| (a[i] + b[i]) / 2.0),
                            self.centers[arc.to],
                        ]
                    }
                    Cross::Link {
                        via: Via::Drop | Via::Step | Via::Walk,
                        from,
                        to,
                    } => vec![from, to],
                    Cross::Link { .. } => return true,
                };
                !(path.windows(2)).any(|leg| self.walled(leg[0], leg[1]))
            });
            self.arcs[f] = arcs;
        }
        self
    }

    /// Whether a wall stands over the water `hash` between shores `a` and
    /// `b`, at a swimmer's head and [`SHORE_OFF`] out from each.
    fn swim_walled(&self, hash: Option<u64>, a: [f32; 3], b: [f32; 3]) -> bool {
        let Some(w) = self
            .waters
            .iter()
            .find(|w| w.hash == hash && hash.is_some())
        else {
            return false;
        };
        let len = dist([a[0], 0.0, a[2]], [b[0], 0.0, b[2]]);
        if len <= 2.0 * SHORE_OFF {
            return false;
        }
        let at = |t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                w.surface + HEAD,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        let (s, e) = (at(SHORE_OFF / len), at(1.0 - SHORE_OFF / len));
        (self.walls.iter().chain(self.solid)).any(|wall| wall.crosses(s, e))
    }

    /// Whether an invisible wall stands on the way from floor `a` to floor
    /// `b`, at Link's waist or head.
    pub fn walled(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        body_hits(self.walls, &[a, b], &BODY)
    }

    /// Whether a jump or a fall from floor `a` to floor `b` hits a wall,
    /// seen or not. A jump goes level with the higher floor. A fall goes
    /// out to `b` or past the real ledge (the mesh stops short of it),
    /// whichever is further, then straight down: whatever it meets on the
    /// way, Link lands on instead.
    pub fn leap_walled(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let path = match b[1] < a[1] - DROP_MIN {
            false if b[1] > a[1] => vec![a, [a[0], b[1], a[2]], b],
            false => vec![a, [b[0], a[1], b[2]], b],
            true => match self.ledge(a, b) {
                Some(p) => vec![a, [p[0], a[1], p[2]], [p[0], b[1], p[2]]],
                None => return true,
            },
        };
        body_hits(self.walls, &path, &BODY) || body_hits(self.solid, &path, &OVER)
    }

    /// Where Link starts down from floor `a` heading for `b` below: over
    /// `b`, or past the last solid under his feet and his body's width
    /// more, whichever is further. `None` when the floor runs on further
    /// than a fall looks.
    fn ledge(&self, a: [f32; 3], b: [f32; 3]) -> Option<[f32; 3]> {
        let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
        let len = dx.hypot(dz).max(1e-6);
        let at = |d: f32| [a[0] + dx / len * d, a[1], a[2] + dz / len * d];
        let under = |p: [f32; 3]| {
            let (top, foot) = ([p[0], p[1] + LIP, p[2]], [p[0], p[1] - LIP, p[2]]);
            self.solid.iter().any(|w| w.crosses(top, foot))
        };
        let clear = (0..=(LEDGE_MAX / LEDGE_STEP) as usize)
            .map(|i| i as f32 * LEDGE_STEP)
            .find(|&d| !under(at(d)))?;
        Some(at((clear + RADIUS).max(len)))
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

    /// The spot of the mesh's edges nearest `p` within `reach`, among
    /// those `keep` allows by face and spot.
    fn nearest(
        &self,
        p: [f32; 3],
        reach: f32,
        keep: impl Fn(usize, [f32; 3]) -> bool,
    ) -> Option<Snap> {
        let mesh = self.mesh;
        (self.faces_near(p[0], p[2], reach).into_iter())
            .flat_map(|f| {
                mesh.ring(f)
                    .iter()
                    .map(move |e| (f, closest(mesh.vertices[e.a], mesh.vertices[e.b], p)))
            })
            .filter(|&(f, at)| dist(at, p) <= reach && keep(f, at))
            .min_by(|a, b| dist(a.1, p).total_cmp(&dist(b.1, p)))
            .map(|(face, at)| Snap { face, at })
    }

    /// The floor spot nearest `p` within `reach`, on a face `keep` allows:
    /// straight under it, or else the nearest edge.
    pub fn nearest_floor(
        &self,
        p: [f32; 3],
        reach: f32,
        keep: impl Fn(usize) -> bool,
    ) -> Option<Snap> {
        let under = (self.faces_near(p[0], p[2], 0.0).into_iter())
            .filter(|&f| keep(f))
            .filter_map(|face| {
                let height = self.mesh.height_on(face, p[0], p[2])?;
                (height <= p[1] && p[1] - height <= reach).then_some(Snap {
                    face,
                    at: [p[0], height, p[2]],
                })
            })
            .max_by(|a, b| a.at[1].total_cmp(&b.at[1]));
        let edge = self.nearest(p, reach, |f, _| keep(f));
        (under.into_iter().chain(edge)).min_by(|a, b| dist(a.at, p).total_cmp(&dist(b.at, p)))
    }

    /// Where `p` stands on the mesh, or the nearest face within 1.5 m of
    /// it when it's off.
    pub fn locate(&self, p: [f32; 3]) -> Option<Snap> {
        self.locate_where(p, |_| true)
    }

    /// [`Graph::locate`] among the faces `keep` allows.
    pub fn locate_where(&self, p: [f32; 3], keep: impl Fn(usize) -> bool) -> Option<Snap> {
        let near = (self.faces_near(p[0], p[2], 0.0).into_iter()).filter(|&f| keep(f));
        match self.mesh.footing_among(near, p, FLOOR) {
            Some(f) => Some(Snap {
                face: f.face,
                at: [p[0], f.height, p[2]],
            }),
            None => self.nearest(p, SNAP, |f, _| keep(f)),
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
    pub(super) fn link(
        &mut self,
        from: usize,
        to: usize,
        via: Via,
        leave: [f32; 3],
        arrive: [f32; 3],
    ) {
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

    /// Each gap across which another face sits within `reach` of a border
    /// edge and no more than a step above or below it: the floor a jump
    /// lands on. The face the edge is on, the face across, and the two
    /// spots.
    pub fn gaps(&self, reach: f32) -> Vec<(usize, usize, [f32; 3], [f32; 3])> {
        let rise = step_up();
        let mut found = vec![];
        for Border { face, mid, out } in self.borders() {
            let mut d = LOOK;
            while d <= reach {
                let past = [mid[0] + out[0] * d, mid[1], mid[2] + out[1] * d];
                let level = self
                    .floor(past, rise)
                    .filter(|g| (g.height - mid[1]).abs() <= rise);
                if let Some(g) = level {
                    let to = [past[0], g.height, past[2]];
                    if g.face != face && d > LOOK && !self.leap_walled(mid, to) {
                        found.push((face, g.face, mid, to));
                    }
                    break;
                }
                d += LOOK / 2.0;
            }
        }
        found
    }

    /// Each edge off which Link falls further than a drop, into water deep
    /// enough to float in, which breaks the fall: the face the edge is on,
    /// the floor under the water, and the two spots.
    pub fn plunges(&self) -> Vec<(usize, usize, [f32; 3], [f32; 3])> {
        let drop = limits().drop();
        let mut found = vec![];
        for Border { face, mid, out } in self.borders() {
            let past = [mid[0] + out[0] * LOOK, mid[1], mid[2] + out[1] * LOOK];
            // Floor level past the edge: he walks on, no fall.
            if self.floor(past, DROP_MIN).is_some() {
                continue;
            }
            // The first floor Link meets on the way down.
            let under = (self.faces_near(past[0], past[2], 0.0).into_iter())
                .filter_map(|f| Some((f, self.mesh.height_on(f, past[0], past[2])?)))
                .filter(|&(_, h)| h < mid[1] - DROP_MIN)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let Some((floor, h)) = under.filter(|&(_, h)| h < mid[1] - drop) else {
                continue;
            };
            let wet = (self.waters.iter()).any(|w| {
                w.covers(past[0], past[2]) && w.surface < mid[1] && w.surface - h >= self.float
            });
            let to = [past[0], h, past[2]];
            if wet && floor != face && !self.leap_walled(mid, to) {
                found.push((face, floor, mid, to));
            }
        }
        found
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
                    |_, q: [f32; 3]| sign * ((q[0] - x) * facing[0] + (q[2] - z) * facing[1]) > 0.0;
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

    /// A 1.5 m gap between two level floors is a gap, a 3 m one past the
    /// reach isn't.
    #[test]
    fn a_level_gap_within_reach_is_a_gap() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(3.5, 0.0, 5.5, 2.0, 0.0)]);
        let g = Graph::build(&m, &[], step_up());
        let gaps: Vec<(usize, usize)> = g.gaps(2.0).iter().map(|&(a, b, ..)| (a, b)).collect();
        assert!(gaps.contains(&(0, 1)) && gaps.contains(&(1, 0)), "{gaps:?}");
        let far = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(5.0, 0.0, 7.0, 2.0, 0.0)]);
        assert!(Graph::build(&far, &[], step_up()).gaps(2.0).is_empty());
    }

    /// A wall standing in the yz plane at `x`, `h` m tall.
    fn wall_at(x: f32, h: f32) -> Walls {
        let (p, q, r, t) = ([x, -1.0, -1.0], [x, h, -1.0], [x, h, 3.0], [x, -1.0, 3.0]);
        Walls {
            air: vec![],
            solid: vec![Wall::new("FakeWall", vec![[p, q, r], [p, r, t]])],
        }
    }

    /// A solid wall across a level gap cuts it; one lower than a body's
    /// height, a lip, doesn't.
    #[test]
    fn a_solid_wall_cuts_a_level_gap_and_a_lip_does_not() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(3.5, 0.0, 5.5, 2.0, 0.0)]);
        let (wall, lip) = (wall_at(2.75, 3.0), wall_at(2.75, 0.6));
        let g = Graph::build(&m, &[], step_up()).with_walls(&wall);
        assert!(g.gaps(2.0).is_empty());
        let g = Graph::build(&m, &[], step_up()).with_walls(&lip);
        assert!(!g.gaps(2.0).is_empty());
    }

    /// A fall leaves past the real ledge, which the mesh stops short of,
    /// and meets whatever floor lies under it on the way down.
    #[test]
    fn a_fall_clears_the_ledge_and_hits_a_floor_on_the_way_down() {
        let slab = |x1: f32, y: f32| {
            let (p, q, r, t) = ([-1.0, y, -1.0], [x1, y, -1.0], [x1, y, 3.0], [-1.0, y, 3.0]);
            Wall::new("FakeFloor", vec![[p, q, r], [p, r, t]])
        };
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 5.0)]);
        let (from, to) = ([2.0, 5.0, 1.0], [2.5, 0.0, 1.0]);
        let ledge = Walls {
            air: vec![],
            solid: vec![slab(2.4, 5.0)],
        };
        let g = Graph::build(&m, &[], step_up()).with_walls(&ledge);
        assert!(!g.leap_walled(from, to));
        let under = Walls {
            air: vec![],
            solid: vec![slab(2.4, 5.0), slab(4.0, 2.0)],
        };
        let g = Graph::build(&m, &[], step_up()).with_walls(&under);
        assert!(g.leap_walled(from, to));
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

    #[test]
    fn the_nearest_floor_is_straight_under_or_else_the_nearest_edge_of_a_kept_face() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(4.0, 0.0, 6.0, 2.0, 3.0)]);
        let g = Graph::build(&m, &[], step_up());
        let first = |f: usize| f == 0;
        assert_eq!(
            g.nearest_floor([1.0, 8.0, 1.0], 20.0, first).unwrap().at,
            [1.0, 0.0, 1.0]
        );
        assert_eq!(
            g.nearest_floor([3.0, 8.0, 1.0], 20.0, first).unwrap().at,
            [2.0, 0.0, 1.0]
        );
        let higher = g.nearest_floor([5.0, 8.0, 1.0], 20.0, |_| true).unwrap();
        assert_eq!((higher.face, higher.at), (1, [5.0, 3.0, 1.0]));
        assert!(g.nearest_floor([1.0, 8.0, 1.0], 5.0, first).is_none());
    }
}
