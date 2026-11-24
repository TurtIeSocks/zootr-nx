//! The jumps and falls a graph adds across its mesh's borders: gaps, leaps
//! and plunges, and the borders they leave from.

use super::{Border, DROP_MIN, Graph, Hop, LOOK, STEP};
use crate::nav::limits::limits;

impl<'a> Graph<'a> {
    /// Each gap across which another face sits within `reach` of a border
    /// edge and no more than a step above or below it: the floor a jump
    /// lands on.
    pub fn gaps(&self, reach: f32) -> Vec<Hop> {
        let rise = self.step_up;
        let mut found = vec![];
        for b in self.borders() {
            let (face, mid) = (b.face, b.mid);
            let mut d = LOOK;
            while d <= reach {
                let past = b.at(d);
                let level = self
                    .floor(past, rise)
                    .filter(|g| (g.height - mid[1]).abs() <= rise);
                if let Some(g) = level {
                    let land = [past[0], g.height, past[2]];
                    if g.face != face && d > LOOK && !self.leap_walled(mid, land) {
                        found.push(Hop {
                            from: face,
                            to: g.face,
                            leave: mid,
                            land,
                        });
                    }
                    break;
                }
                d += STEP;
            }
        }
        found
    }

    /// Each gap Link clears by running off an edge onto floor below it,
    /// further out than a plain drop lands: the first floor under his path
    /// within a drop's fall, no further out than a running jump carries
    /// him for that fall ([`super::limits::Limits::leap`]). Both ends stand
    /// on the level's collision: floor with none under it (terrain, not read
    /// yet) has no walls to stop a leap either.
    pub fn leaps(&self) -> Vec<Hop> {
        let l = limits();
        let (rise, fall) = (self.step_up, l.drop());
        let far = l.leap(fall);
        // The highest floor under `p`, from a step over it to a fall under.
        let under = |p: [f32; 3]| {
            (self.faces_near(p[0], p[2], 0.0).into_iter())
                .filter_map(|f| Some((f, self.mesh.height_on(f, p[0], p[2])?)))
                .filter(|&(_, h)| h <= p[1] + rise && h >= p[1] - fall)
                .max_by(|a, b| a.1.total_cmp(&b.1))
        };
        let mut found = vec![];
        for b in self.borders() {
            let (face, mid) = (b.face, b.mid);
            // Floor just past the edge: he walks on or drops, no leap.
            if under(b.at(LOOK)).is_some() {
                continue;
            }
            let mut d = LOOK + STEP;
            while d <= far {
                let past = b.at(d);
                if let Some((floor, h)) = under(past) {
                    let land = [past[0], h, past[2]];
                    let low = mid[1] - h >= DROP_MIN;
                    let far_enough = d <= l.leap(mid[1] - h);
                    let known = self.solid_under(mid) && self.solid_under(land);
                    if low && floor != face && far_enough && known && !self.leap_walled(mid, land) {
                        found.push(Hop {
                            from: face,
                            to: floor,
                            leave: mid,
                            land,
                        });
                    }
                    break;
                }
                d += STEP;
            }
        }
        found
    }

    /// Each edge off which Link falls further than a drop, into water deep
    /// enough to float in, which breaks the fall: it lands on the floor
    /// under the water.
    pub fn plunges(&self) -> Vec<Hop> {
        let drop = limits().drop();
        let mut found = vec![];
        for b in self.borders() {
            let (face, mid, past) = (b.face, b.mid, b.at(LOOK));
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
            let land = [past[0], h, past[2]];
            if wet && floor != face && !self.leap_walled(mid, land) {
                found.push(Hop {
                    from: face,
                    to: floor,
                    leave: mid,
                    land,
                });
            }
        }
        found
    }

    /// Every edge of the mesh's border with a length.
    pub(in crate::nav) fn borders(&self) -> Vec<Border> {
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
}

#[cfg(test)]
mod tests {
    use super::super::step_up;
    use super::*;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::mesh::{Wall, Walls};

    /// A 1.5 m gap between two level floors is a gap, a 3 m one past the
    /// reach isn't.
    #[test]
    fn a_level_gap_within_reach_is_a_gap() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(3.5, 0.0, 5.5, 2.0, 0.0)]);
        let g = Graph::build(&m, &[], step_up());
        let gaps: Vec<(usize, usize)> = g.gaps(2.0).iter().map(|h| (h.from, h.to)).collect();
        assert!(gaps.contains(&(0, 1)) && gaps.contains(&(1, 0)), "{gaps:?}");
        let far = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 0.0), quad(5.0, 0.0, 7.0, 2.0, 0.0)]);
        assert!(Graph::build(&far, &[], step_up()).gaps(2.0).is_empty());
    }

    /// A wall standing in the yz plane at `x`, `h` m tall.
    fn wall_at(x: f32, h: f32) -> Walls {
        let (p, q, r, t) = ([x, -1.0, -1.0], [x, h, -1.0], [x, h, 3.0], [x, -1.0, 3.0]);
        Walls::solid_only(vec![Wall::new("FakeWall", vec![[p, q, r], [p, r, t]])])
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

    /// A floor from `x0` to `x1` at height `y`, 4 m deep.
    fn slab(x0: f32, x1: f32, y: f32) -> Wall {
        let (p, q, r, t) = ([x0, y, -1.0], [x1, y, -1.0], [x1, y, 3.0], [x0, y, 3.0]);
        Wall::new("FakeFloor", vec![[p, q, r], [p, r, t]])
    }

    /// A ledge 2 m over a floor 1.5 m out is a leap; the same floor with no
    /// collision under either end (terrain) or past a leap's reach isn't.
    #[test]
    fn a_leap_lands_on_collision_floor_within_its_reach() {
        let floors = |far: f32| {
            Walls::solid_only(vec![slab(-1.0, 2.2, 2.0), slab(far - 0.2, far + 2.2, 0.0)])
        };
        let leaps = |far: f32, walls: Option<Walls>| {
            let m = mesh(&[
                quad(0.0, 0.0, 2.0, 2.0, 2.0),
                quad(far, 0.0, far + 2.0, 2.0, 0.0),
            ]);
            let g = Graph::build(&m, &[], step_up());
            let g = match &walls {
                Some(w) => g.with_walls(w),
                None => g,
            };
            g.leaps().iter().map(|h| (h.from, h.to)).collect::<Vec<_>>()
        };
        assert_eq!(leaps(3.5, Some(floors(3.5))), [(0, 1)]);
        assert!(leaps(3.5, None).is_empty());
        assert!(leaps(8.0, Some(floors(8.0))).is_empty());
    }

    /// A fall leaves past the real ledge, which the mesh stops short of,
    /// and meets whatever floor lies under it on the way down.
    #[test]
    fn a_fall_clears_the_ledge_and_hits_a_floor_on_the_way_down() {
        let m = mesh(&[quad(0.0, 0.0, 2.0, 2.0, 5.0)]);
        let (from, to) = ([2.0, 5.0, 1.0], [2.5, 0.0, 1.0]);
        let ledge = Walls::solid_only(vec![slab(-1.0, 2.4, 5.0)]);
        let g = Graph::build(&m, &[], step_up()).with_walls(&ledge);
        assert!(!g.leap_walled(from, to));
        let under = Walls::solid_only(vec![slab(-1.0, 2.4, 5.0), slab(-1.0, 4.0, 2.0)]);
        let g = Graph::build(&m, &[], step_up()).with_walls(&under);
        assert!(g.leap_walled(from, to));
    }
}
