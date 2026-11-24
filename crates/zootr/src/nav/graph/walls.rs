//! A graph's walls: which walks, jumps and falls an invisible or solid wall
//! stops, and where a fall starts from a ledge.

use super::super::mesh::{Wall, Walls};
use super::{Arc, BODY, Cross, DROP_MIN, Graph, RADIUS, Via, body_hits};

/// The height above the floor a jump or fall is tested against solid
/// walls at: over the lips and curbs a floor's mesh has at its edges.
const OVER: [f32; 1] = [1.0];
/// How far below and above the floor a solid under Link's feet is
/// looked for, past the mesh's edge.
const LIP: f32 = 0.3;
/// How far past the mesh's edge a ledge is looked for, and in what steps.
const LEDGE_MAX: f32 = 2.0;
const LEDGE_STEP: f32 = 0.05;

/// How far over a water's surface a swimmer's head is, in m.
const HEAD: f32 = 0.3;
/// How far out from each shore a swim's line is tested, in m: the shore
/// point sits on land, past its edge.
const SHORE_OFF: f32 = 1.0;

/// [`body_hits`] against `walls`.
fn walls_hit(walls: &[Wall], path: &[[f32; 3]], heights: &[f32]) -> bool {
    body_hits(path, heights, |a, b| walls.iter().any(|w| w.crosses(a, b)))
}

impl<'a> Graph<'a> {
    /// The graph with `walls` for its jumps and falls to hit, without the
    /// walks, steps and drops whose path passes through an invisible one at
    /// Link's waist or head: Link stops at an invisible wall the mesh runs
    /// through.
    pub fn with_walls(mut self, walls: &'a Walls) -> Self {
        (self.walls, self.solid) = (&walls.air, &walls.solid);
        for f in 0..self.arcs.len() {
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
                    Cross::Link {
                        via: Via::Drop,
                        from,
                        to,
                    } => vec![from, to],
                    _ => match self.walk_path(f, arc) {
                        Some(path) => path,
                        None => return true,
                    },
                };
                !(path.windows(2)).any(|leg| self.walled(leg[0], leg[1]))
            });
            self.arcs[f] = arcs;
        }
        self
    }

    /// The path a walk or a step along `arc` from face `f` takes: from
    /// the face's middle over the middle of the edge it crosses to the
    /// next face's, or from where its link leaves to where it arrives.
    /// `None` for any other link.
    pub(in crate::nav) fn walk_path(&self, f: usize, arc: &Arc) -> Option<Vec<[f32; 3]>> {
        match arc.cross {
            Cross::Edge([a, b]) => Some(vec![
                self.centers[f],
                std::array::from_fn(|i| (a[i] + b[i]) / 2.0),
                self.centers[arc.to],
            ]),
            Cross::Link {
                via: Via::Step | Via::Walk,
                from,
                to,
            } => Some(vec![from, to]),
            Cross::Link { .. } => None,
        }
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
        let len = crate::checks::flat(a, b);
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
        walls_hit(self.walls, &[a, b], &BODY)
    }

    /// Whether a jump or a fall from floor `a` to floor `b` hits a wall,
    /// seen or not. A jump goes level with the higher floor. A fall goes
    /// out to `b` or past the real ledge (the mesh stops short of it),
    /// whichever is further, then straight down: whatever it meets on the
    /// way, Link lands on instead.
    pub fn leap_walled(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let path = if b[1] < a[1] - DROP_MIN {
            let Some(p) = self.ledge(a, b) else {
                return true;
            };
            vec![a, [p[0], a[1], p[2]], [p[0], b[1], p[2]]]
        } else if b[1] > a[1] {
            vec![a, [a[0], b[1], a[2]], b]
        } else {
            vec![a, [b[0], a[1], b[2]], b]
        };
        walls_hit(self.walls, &path, &BODY) || walls_hit(self.solid, &path, &OVER)
    }

    /// Where Link starts down from floor `a` heading for `b` below: over
    /// `b`, or past the last solid under his feet and his body's width
    /// more, whichever is further. `None` when the floor runs on further
    /// than a fall looks.
    fn ledge(&self, a: [f32; 3], b: [f32; 3]) -> Option<[f32; 3]> {
        let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
        let len = dx.hypot(dz).max(1e-6);
        let at = |d: f32| [a[0] + dx / len * d, a[1], a[2] + dz / len * d];
        let clear = (0..=(LEDGE_MAX / LEDGE_STEP) as usize)
            .map(|i| i as f32 * LEDGE_STEP)
            .find(|&d| !self.solid_under(at(d)))?;
        Some(at((clear + RADIUS).max(len)))
    }

    /// Whether a solid stands under Link's feet at `p`, within [`LIP`]
    /// over or under it.
    pub(super) fn solid_under(&self, p: [f32; 3]) -> bool {
        let (top, foot) = ([p[0], p[1] + LIP, p[2]], [p[0], p[1] - LIP, p[2]]);
        self.solid.iter().any(|w| w.crosses(top, foot))
    }
}
