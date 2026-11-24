//! Collision floors: floor Link stands on that the navmesh leaves out
//! (room reach spec, rules 9 to 11). The navmesh keeps 0.4 m off walls,
//! so a ledge narrower than that has no face, though the piece's own
//! collision mesh holds its floor. Each patch of such floor joins the
//! level's mesh as faces.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, Edge, Face, NavMesh, Romfs};

use super::graph::{BODY, Cross, Graph, LOOK, STEP_MIN, Via, body_hits};
use super::mesh::{Walls, facing_up, floor_normal, height_in, moller, through};
use super::shape::Solid;
use super::vec3::{cross, dot, sub};
use super::water::Water;
use crate::known::{actor, field};

/// How near the navmesh's floor may be to a triangle's height, in m,
/// before the triangle is floor the navmesh already has.
const NAVMESH_NEAR: f32 = 0.5;
/// The smallest patch kept, in m²: anything less is a prop's sliver.
const PATCH_MIN: f32 = 0.25;
/// How far over a floor a ceiling starts to count, in m: past the floor's
/// own seams and lips.
const CEILING_OVER: f32 = 0.1;
/// Corners this near are one, in m.
const SAME: f32 = 0.01;
/// The size of the cells triangles and faces are looked up by, in m.
const CELL: f32 = 2.0;
/// How high over a floor, and how far round, rays look for the walls
/// Link would stand inside of, in m.
const WAIST: f32 = 0.5;
const LOOK_ROUND: f32 = 2.0;
/// How many rays go round.
const RAYS: usize = 8;
/// How many steps along a ray its cells are looked up at.
const RAY_STEPS: usize = 4;

/// A triangle of a collision mesh, its corners in the world.
pub type Tri = [[f32; 3]; 3];

/// Link's height at his age, from his game params: how much room a
/// floor needs above it.
pub fn height(romfs: &Romfs, child: bool) -> Result<f32> {
    // The child's params are named for his actor, the adult's plain.
    let (who, param) = if child {
        (actor::PLAYER_CHILD, actor::PLAYER_CHILD.get())
    } else {
        (actor::PLAYER_ADULT, "Player")
    };
    let pack = ActorPack::load(romfs, who.get())?;
    let path = format!("Player/PlayerGameParam/{param}.game__player__PlayerGameParam.gyml");
    let params = pack.resolve(&path)?.with_context(|| format!("no {path}"))?;
    let key = field::PLAYER_HEIGHT.get();
    let found = params.iter().find_map(|(_, v)| match v {
        Byml::Map(m) => m.get(key).cloned(),
        _ => None,
    });
    match params.get(key).cloned().or(found) {
        Some(Byml::Float(h)) => Ok(h),
        other => anyhow::bail!("{path}: {key} is {other:?}"),
    }
}

/// Things in a flat grid of `size` m cells, by the bounds of their
/// corners in x and z.
pub(super) struct Grid {
    size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl Grid {
    pub(super) fn of<'p>(
        size: f32,
        each: impl Iterator<Item = impl Iterator<Item = &'p [f32; 3]>>,
    ) -> Self {
        let mut grid = Self {
            size,
            cells: HashMap::new(),
        };
        for (i, corners) in each.enumerate() {
            let (lo, hi) = corners.fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), v| {
                (
                    [lo[0].min(v[0]), lo[1].min(v[2])],
                    [hi[0].max(v[0]), hi[1].max(v[2])],
                )
            });
            for cx in grid.cell(lo[0])..=grid.cell(hi[0]) {
                for cz in grid.cell(lo[1])..=grid.cell(hi[1]) {
                    grid.cells.entry((cx, cz)).or_default().push(i);
                }
            }
        }
        grid
    }

    /// `mesh`'s faces.
    pub(super) fn faces(mesh: &NavMesh, size: f32) -> Self {
        Self::of(
            size,
            (0..mesh.faces.len()).map(|f| mesh.ring(f).iter().map(|e| &mesh.vertices[e.a])),
        )
    }

    fn cell(&self, v: f32) -> i32 {
        (v / self.size).floor() as i32
    }

    fn at(&self, x: f32, z: f32) -> &[usize] {
        let key = (self.cell(x), self.cell(z));
        self.cells.get(&key).map_or(&[], Vec::as_slice)
    }

    /// The things that may lie within `r` of (x, z), each once, in order.
    pub(super) fn near(&self, x: f32, z: f32, r: f32) -> Vec<usize> {
        let found = (self.cell(x - r)..=self.cell(x + r))
            .flat_map(|cx| (self.cell(z - r)..=self.cell(z + r)).map(move |cz| (cx, cz)))
            .filter_map(|c| self.cells.get(&c))
            .flatten();
        sorted(found.copied())
    }

    /// The things in the cells at `steps` even steps from `from` by `by`,
    /// both ends in, each once, in order.
    fn along(&self, from: [f32; 3], by: [f32; 3], steps: usize) -> Vec<usize> {
        let found = (0..=steps).flat_map(|k| {
            let f = k as f32 / steps.max(1) as f32;
            self.at(from[0] + by[0] * f, from[2] + by[2] * f)
        });
        sorted(found.copied())
    }
}

fn sorted(each: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut found: Vec<usize> = each.collect();
    found.sort_unstable();
    found.dedup();
    found
}

fn middle(t: &Tri) -> [f32; 3] {
    std::array::from_fn(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0)
}

fn area([p, q, r]: &Tri) -> f32 {
    let n = cross(sub(*q, *p), sub(*r, *p));
    dot(n, n).sqrt() / 2.0
}

/// How far along the ray from `o` by `d` (times its length) it meets
/// triangle `t`, and whether it meets the triangle's back.
fn ray_hit(o: [f32; 3], d: [f32; 3], tri: &Tri) -> Option<(f32, bool)> {
    let [u, v, t] = moller(o, d, tri)?;
    let inside = u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0;
    // The front is the side its winding faces: the ray meets its back
    // when it runs the way the triangle faces.
    let [p, q, r] = tri;
    inside.then_some((t, dot(cross(sub(*q, *p), sub(*r, *p)), d) > 0.0))
}

/// A corner as a key, so corners that meet compare equal.
fn key(v: [f32; 3]) -> [i64; 3] {
    v.map(|c| (c / SAME).round() as i64)
}

/// The solid walls round a spot, to tell whether it is inside one: a
/// floor runs on under the walls stood on it, with no ceiling there.
pub struct Round<'w> {
    solid: Vec<&'w Tri>,
    grid: Grid,
}

impl<'w> Round<'w> {
    pub fn of(walls: &'w Walls) -> Self {
        let solid: Vec<_> = walls.solid.iter().flat_map(|w| &w.triangles).collect();
        let grid = Grid::of(CELL, solid.iter().map(|t| t.iter()));
        Self { solid, grid }
    }

    /// What each of [`RAYS`] rays round floor `p` at Link's waist meets
    /// first within [`LOOK_ROUND`]: how far, and whether a wall's back.
    pub fn rays(&self, [x, y, z]: [f32; 3]) -> Vec<Option<(f32, bool)>> {
        let o = [x, y + WAIST, z];
        (0..RAYS)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / RAYS as f32;
                let d = [a.cos() * LOOK_ROUND, 0.0, a.sin() * LOOK_ROUND];
                (self.grid.along(o, d, RAY_STEPS).into_iter())
                    .filter_map(|j| ray_hit(o, d, self.solid[j]))
                    .filter(|(t, _)| *t <= 1.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(t, back)| (t * LOOK_ROUND, back))
            })
            .collect()
    }

    /// Whether the line from `a` to `b` passes through a solid wall.
    pub fn crosses(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let steps = ((a[0] - b[0]).hypot(a[2] - b[2]) / (CELL / 2.0)).ceil() as usize;
        (self.grid.along(a, sub(b, a), steps).into_iter()).any(|j| through(a, b, self.solid[j]))
    }

    /// Whether floor `p` is inside a wall: most rays round it that meet
    /// one meet its back first.
    pub fn inside(&self, p: [f32; 3]) -> bool {
        let rays = self.rays(p);
        let hits = rays.iter().flatten().count();
        let backs = rays.iter().flatten().filter(|(_, back)| *back).count();
        hits > 0 && 2 * backs > hits
    }
}

/// What a collision floor triangle must clear besides walls.
pub struct Keep<'a> {
    /// Water he'd float in, from `float` deep.
    pub waters: &'a [Water],
    pub float: f32,
    /// Link's height: room he needs over a floor.
    pub height: f32,
    /// Obstacles a floor may not run under (shutters, webs).
    pub under: &'a [Solid],
}

/// How far over or under an obstacle a floor still runs under it, in m.
const UNDER: f32 = 0.5;

/// Whether triangle `t` lies under `solid`'s footprint: a corner of
/// either inside the other, or an edge of `t` across it.
fn under(solid: &Solid, t: &Tri) -> bool {
    let (lo, hi) = t.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
        (lo.min(v[1]), hi.max(v[1]))
    });
    if hi < solid.low - UNDER || lo > solid.high + UNDER {
        return false;
    }
    let flat = |v: [f32; 3]| [v[0], v[2]];
    let edge = (0..3).any(|i| solid.over(flat(t[i]), flat(t[(i + 1) % 3])).is_some());
    let ring = |c: &[f32; 2]| height_in(t, c[0], c[1]).is_some();
    edge || solid.ring.iter().any(ring)
}

/// The collision floor triangles of `walls`, in patches: each a triangle
/// that touches Link on foot and lets him stand (ridable, facing up no
/// steeper than he walks), with `height` clear above its middle, out of
/// water he'd float in, and no navmesh floor within [`NAVMESH_NEAR`] of
/// its height. Triangles that share corners make a patch, and a patch
/// under [`PATCH_MIN`] is dropped.
pub fn patches(mesh: &NavMesh, walls: &Walls, keep: &Keep) -> Vec<Vec<Tri>> {
    let Keep {
        waters,
        float,
        height,
        under: obstacles,
    } = *keep;
    let faces = Grid::faces(mesh, CELL);
    let all: Vec<&Tri> = (walls.solid.iter().chain(&walls.air))
        .flat_map(|w| &w.triangles)
        .collect();
    let above = Grid::of(CELL, all.iter().map(|t| t.iter()));
    let navmesh_has = |[x, y, z]: [f32; 3]| {
        (faces.at(x, z).iter())
            .filter_map(|&f| mesh.height_on(f, x, z))
            .any(|h| (h - y).abs() <= NAVMESH_NEAR)
    };
    let roofed = |[x, y, z]: [f32; 3]| {
        (above.at(x, z).iter())
            .filter_map(|&i| height_in(all[i], x, z))
            .any(|h| h > y + CEILING_OVER && h < y + height)
    };
    let wet =
        |[x, y, z]: [f32; 3]| (waters.iter()).any(|w| w.covers(x, z) && w.surface - y >= float);
    let round = Round::of(walls);
    // Stairs stand twice, as their slope and their steps: the slope until
    // a live check says which Link's controller stands on (rule 7).
    let step = format!("__{}", field::STAIR_STEP.get());
    let kept: Vec<Tri> = (walls.solid.iter())
        .filter(|w| w.ridable && !w.name.ends_with(&step))
        .flat_map(|w| w.triangles.iter().copied())
        .filter(|t| key(t[0]) != key(t[1]) && key(t[1]) != key(t[2]) && key(t[0]) != key(t[2]))
        .filter(|t| floor_normal(t) && facing_up(t))
        .filter(|t| !obstacles.iter().any(|o| under(o, t)))
        .filter(|t| {
            let m = middle(t);
            !navmesh_has(m) && !wet(m) && !roofed(m) && !round.inside(m)
        })
        .collect();
    // Join triangles that share a corner.
    let mut parent: Vec<usize> = (0..kept.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut first_at: HashMap<[i64; 3], usize> = HashMap::new();
    for (i, t) in kept.iter().enumerate() {
        for v in t {
            let j = *first_at.entry(key(*v)).or_insert(i);
            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
            parent[a] = b;
        }
    }
    let mut by_root: HashMap<usize, Vec<Tri>> = HashMap::new();
    for (i, t) in kept.iter().enumerate() {
        by_root.entry(root(&mut parent, i)).or_default().push(*t);
    }
    let mut out: Vec<_> = (by_root.into_values())
        .filter(|p| p.iter().map(area).sum::<f32>() >= PATCH_MIN)
        .collect();
    // In a fixed order, so derive writes the same files each run.
    out.sort_by(|a, b| {
        let at = |p: &Vec<Tri>| key(middle(&p[0]));
        at(a).cmp(&at(b))
    });
    out
}

/// Which faces Link gets to from the navmesh's (those before `first`):
/// by any arc, a jump within `jump` or a plunge, one way only. A ledge
/// he can only drop off is out of his reach.
pub fn reached(graph: &Graph, first: usize, jump: f32) -> Vec<bool> {
    let n = graph.arcs.len();
    let mut out: Vec<Vec<usize>> = (graph.arcs.iter())
        .map(|arcs| arcs.iter().map(|a| a.to).collect())
        .collect();
    for hop in graph.gaps(jump).into_iter().chain(graph.plunges()) {
        out[hop.from].push(hop.to);
    }
    let mut seen: Vec<bool> = (0..n).map(|f| f < first).collect();
    let mut stack: Vec<usize> = (0..first.min(n)).collect();
    while let Some(f) = stack.pop() {
        for &to in &out[f] {
            if !seen[to] {
                seen[to] = true;
                stack.push(to);
            }
        }
    }
    seen
}

/// `mesh` with each of `patches`' triangles added as a face, after its
/// own and in their order. Triangles that share an edge face each other
/// across it.
pub fn joined(mesh: &NavMesh, patches: &[Vec<Tri>]) -> NavMesh {
    let mut out = NavMesh {
        vertices: mesh.vertices.clone(),
        faces: mesh.faces.clone(),
        edges: mesh.edges.clone(),
        ..*mesh
    };
    let mut corner: HashMap<[i64; 3], usize> = HashMap::new();
    // Each edge by its two corners, either way round: the face it rings.
    let mut sides: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for t in patches.iter().flatten() {
        let ids = t.map(|v| {
            *corner.entry(key(v)).or_insert_with(|| {
                out.vertices.push(v);
                out.vertices.len() - 1
            })
        });
        debug_assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
        let face = out.faces.len();
        out.faces.push(Face {
            start_edge: out.edges.len(),
            edges: 3,
        });
        for k in 0..3 {
            let (a, b) = (ids[k], ids[(k + 1) % 3]);
            sides
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((face, out.edges.len()));
            out.edges.push(Edge { a, b, across: None });
        }
    }
    for pair in sides.values().filter(|s| s.len() == 2) {
        let [(f, e), (g, d)] = [pair[0], pair[1]];
        out.edges[e].across = Some(g);
        out.edges[d].across = Some(f);
    }
    for v in &out.vertices[mesh.vertices.len()..] {
        out.min = std::array::from_fn(|i| out.min[i].min(v[i]));
        out.max = std::array::from_fn(|i| out.max[i].max(v[i]));
    }
    out
}

impl<'a> Graph<'a> {
    /// The graph with the collision floor faces (from `first` on) joined
    /// in: a walk both ways from each border edge to the floor [`LOOK`]
    /// past it, within a step of its height, wherever one side is a
    /// collision floor, so patches join each other and the navmesh where
    /// no shared edge does. Then each walk or step on or off a collision
    /// floor that runs through one of `walls`' solid walls at Link's waist
    /// or head is cut: unlike the navmesh, a floor's triangles run on
    /// under the walls stood on them.
    pub fn with_floors(mut self, first: usize, walls: &Walls) -> Self {
        for b in self.borders() {
            let past = b.at(LOOK);
            let Some(g) = self.floor(past, STEP_MIN) else {
                continue;
            };
            if g.face == b.face || (b.face < first && g.face < first) {
                continue;
            }
            let to = [past[0], g.height, past[2]];
            self.link(b.face, g.face, Via::Walk, b.mid, to);
            self.link(g.face, b.face, Via::Walk, to, b.mid);
        }
        let round = Round::of(walls);
        for f in 0..self.arcs.len() {
            let mut arcs = std::mem::take(&mut self.arcs[f]);
            arcs.retain(|arc| {
                if f < first && arc.to < first {
                    return true;
                }
                let path = match arc.cross {
                    Cross::Link {
                        via: Via::Drop,
                        from,
                        to,
                    } => {
                        // Off the edge, then straight down: no ceiling or
                        // floor between him and where he lands.
                        let over = [to[0], from[1], to[2]];
                        let fall = round.crosses(
                            [to[0], from[1] + BODY[0], to[2]],
                            [to[0], to[1] + CEILING_OVER, to[2]],
                        );
                        if fall {
                            return false;
                        }
                        vec![from, over]
                    }
                    _ => match self.walk_path(f, arc) {
                        Some(path) => path,
                        None => return true,
                    },
                };
                !body_hits(&path, &BODY, |a, b| round.crosses(a, b))
            });
            self.arcs[f] = arcs;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::mesh::Wall;

    fn wall(name: &str, triangles: Vec<Tri>) -> Wall {
        let mut w = Wall::new(name, triangles);
        w.ridable = true;
        w
    }

    /// A 2 m square of floor at `y`, centred on (x, z), as two triangles.
    fn square(x: f32, y: f32, z: f32) -> Vec<Tri> {
        let (a, b, c, d) = (
            [x - 1.0, y, z - 1.0],
            [x + 1.0, y, z - 1.0],
            [x + 1.0, y, z + 1.0],
            [x - 1.0, y, z + 1.0],
        );
        vec![[a, c, b], [a, d, c]]
    }

    /// Triangle `t` wound to face along `d`.
    fn toward(t: Tri, d: [f32; 3]) -> Tri {
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        if dot(n, d) >= 0.0 {
            t
        } else {
            [t[0], t[2], t[1]]
        }
    }

    /// A closed 2 m pillar round (0, 0), 3 m tall from y 0, facing out.
    fn pillar() -> Vec<Tri> {
        let mut out = vec![];
        for (axis, sign) in [(0, 1.0), (0, -1.0), (2, 1.0), (2, -1.0)] {
            let mut d = [0.0; 3];
            d[axis] = sign;
            let other = 2 - axis;
            let corner = |s: f32, y: f32| {
                let mut v = [0.0, y, 0.0];
                v[axis] = sign;
                v[other] = s;
                v
            };
            let (a, b, c, e) = (
                corner(-1.0, 0.0),
                corner(1.0, 0.0),
                corner(1.0, 3.0),
                corner(-1.0, 3.0),
            );
            out.push(toward([a, b, c], d));
            out.push(toward([a, c, e], d));
        }
        out
    }

    const KEEP: Keep = Keep {
        waters: &[],
        float: 1.0,
        height: 1.17,
        under: &[],
    };

    fn empty() -> NavMesh {
        NavMesh::default()
    }

    #[test]
    fn a_ledge_with_room_over_it_is_a_patch() {
        let walls = Walls::solid_only(vec![wall("FakeLedge", square(0.0, 5.0, 0.0))]);
        let found = patches(&empty(), &walls, &KEEP);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].len(), 2);
    }

    #[test]
    fn a_low_ceiling_slivers_and_the_navmesh_keep_floor_out() {
        // A ceiling 1 m over: a child's 1.17 m doesn't fit.
        let mut low = square(0.0, 6.0, 0.0);
        low.extend(square(0.0, 5.0, 0.0));
        let walls = Walls::solid_only(vec![wall("FakeCrawl", low)]);
        let found = patches(&empty(), &walls, &KEEP);
        // The ceiling's own top is floor with room over it; the floor under it isn't.
        assert!(found.iter().flatten().all(|t| t[0][1] == 6.0), "{found:?}");
        // A sliver under 0.25 m².
        let sliver = vec![[[0.0, 0.0, 0.0], [0.3, 0.0, 0.0], [0.0, 0.0, 0.3]]];
        let walls = Walls::solid_only(vec![wall("FakeSliver", sliver)]);
        assert!(patches(&empty(), &walls, &KEEP).is_empty());
    }

    #[test]
    fn floor_under_a_pillar_is_no_patch_and_floor_beside_one_is() {
        let floor = |x: f32| wall("FakeFloor", square(x, 0.0, 0.0));
        let walls = |x: f32| Walls::solid_only(vec![floor(x), wall("FakePillar", pillar())]);
        assert!(patches(&empty(), &walls(0.0), &KEEP).is_empty(), "under");
        assert_eq!(patches(&empty(), &walls(2.5), &KEEP).len(), 1, "beside");
    }

    #[test]
    fn floor_under_a_shutter_or_a_stair_step_is_no_patch() {
        let shutter = Solid {
            name: "FakeShutter".into(),
            hash: None,
            ring: vec![[-0.1, -1.0], [0.1, -1.0], [0.1, 1.0], [-0.1, 1.0]],
            low: 0.0,
            high: 3.0,
        };
        let walls = Walls::solid_only(vec![wall("FakeDoorway", square(0.0, 0.0, 0.0))]);
        let keep = Keep {
            under: std::slice::from_ref(&shutter),
            ..KEEP
        };
        assert!(
            patches(&empty(), &walls, &keep).is_empty(),
            "under the shutter"
        );
        let walls = Walls::solid_only(vec![wall(
            &format!("Fake__{}", field::STAIR_STEP.get()),
            square(0.0, 0.0, 0.0),
        )]);
        assert!(
            patches(&empty(), &walls, &KEEP).is_empty(),
            "a stair's steps"
        );
    }

    #[test]
    fn a_drop_off_a_patch_through_a_ceiling_is_cut() {
        // A roof at y 4 over a room whose ceiling is at y 3.5, floor at 0.
        let mut solid = vec![wall("FakeRoof", square(0.0, 4.0, 0.0))];
        let ceiling: Vec<_> = (square(2.5, 3.5, 0.0).into_iter())
            .map(|t| toward(t, [0.0, -1.0, 0.0]))
            .collect();
        solid.push(wall("FakeCeiling", ceiling));
        let walls = Walls::solid_only(solid);
        let mesh =
            crate::nav::fixture::mesh(&[crate::nav::fixture::quad(1.2, -1.0, 3.5, 1.0, 0.0)]);
        let patches = patches(&mesh, &walls, &KEEP);
        let joined = joined(&mesh, &patches);
        let first = mesh.faces.len();
        let graph = Graph::build(&joined, &[], 0.3).with_floors(first, &walls);
        let drops = (first..graph.arcs.len())
            .flat_map(|f| graph.arcs[f].iter())
            .filter(|a| a.to < first)
            .filter(|a| matches!(a.cross, Cross::Link { via: Via::Drop, .. }))
            .count();
        assert_eq!(drops, 0);
    }

    #[test]
    fn a_ceiling_facing_down_is_no_floor() {
        let down: Vec<_> = square(0.0, 3.0, 0.0)
            .into_iter()
            .map(|t| [t[0], t[2], t[1]])
            .collect();
        let walls = Walls::solid_only(vec![wall("FakeCeiling", down)]);
        assert!(patches(&empty(), &walls, &KEEP).is_empty());
    }

    #[test]
    fn an_unridable_or_steep_triangle_is_no_floor() {
        let mut slick = wall("FakeSlick", square(0.0, 0.0, 0.0));
        slick.ridable = false;
        let steep = wall(
            "FakeSteep",
            vec![[[0.0, 0.0, 0.0], [2.0, 4.0, 0.0], [0.0, 0.0, 2.0]]],
        );
        let walls = Walls::solid_only(vec![slick, steep]);
        assert!(patches(&empty(), &walls, &KEEP).is_empty());
    }

    #[test]
    fn patch_triangles_join_as_faces_across_their_shared_edge() {
        let mesh = joined(&empty(), &[square(0.0, 5.0, 0.0)]);
        assert_eq!(mesh.faces.len(), 2);
        assert_eq!(mesh.vertices.len(), 4);
        let across: Vec<_> = mesh.edges.iter().filter_map(|e| e.across).collect();
        assert_eq!(across, [1, 0]);
        assert_eq!(mesh.height_on(0, 0.5, -0.2), Some(5.0));
    }

    #[test]
    fn a_patch_beside_the_navmesh_joins_it_by_a_walk() {
        // Two separate squares 0.4 m apart, the second a patch.
        let base = joined(&empty(), &[square(0.0, 0.0, 0.0)]);
        let mesh = joined(&base, &[square(2.4, 0.2, 0.0)]);
        let first = base.faces.len();
        let graph = Graph::build(&mesh, &[], 1.0).with_floors(first, &Walls::default());
        let walks = |from: std::ops::Range<usize>| {
            from.flat_map(|f| graph.arcs[f].iter().map(move |a| (f, a)))
                .filter(|(f, a)| {
                    matches!(
                        a.cross,
                        crate::nav::graph::Cross::Link { via: Via::Walk, .. }
                    ) && (*f < first) != (a.to < first)
                })
                .count()
        };
        assert!(
            walks(0..first) > 0,
            "no walk from the navmesh onto the patch"
        );
        assert!(walks(first..mesh.faces.len()) > 0, "no walk back");
        // A wall between them cuts both.
        let wall = vec![
            [[1.2, 0.0, -2.0], [1.2, 3.0, -2.0], [1.2, 0.0, 2.0]],
            [[1.2, 3.0, -2.0], [1.2, 3.0, 2.0], [1.2, 0.0, 2.0]],
        ];
        let walls = Walls::solid_only(vec![crate::nav::mesh::Wall::new("FakeWall", wall)]);
        let graph = Graph::build(&mesh, &[], 1.0).with_floors(first, &walls);
        let crossing = (0..mesh.faces.len())
            .flat_map(|f| graph.arcs[f].iter().map(move |a| (f, a.to)))
            .filter(|(f, to)| (*f < first) != (*to < first))
            .count();
        assert_eq!(crossing, 0, "a walk runs through the wall");
    }
}
