//! Water Link swims in: placed water planes, which the navmesh leaves out
//! past the shallows. Each is its model's box (its water volume's, when it
//! has one) at its surface, scaled and turned by its placement. A route
//! swims one as a link between two stretches of its shore, like a climb's.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use ounce_formats::{ActorPack, Romfs};

use super::graph::{Border, Graph, Snap, Via, dist};
use super::kinds;
use super::shape::place;
use crate::checks::flat;
use crate::warp::Placement;
use crate::world::Presence;

/// How deep a water model with no volume of its own reaches under its
/// surface before its placement scales it, in m: the plain water tiles'
/// cutscene copies (the Water Temple's and Lake Hylia's) have that volume.
const DEEP: f32 = 10.0;
/// The meshes of a water model that hold its volume.
const VOLUME: &str = "WaterVolume";

/// The lowest and highest corner of a box.
pub type Bounds = ([f32; 3], [f32; 3]);
/// How far apart two overlapping waters' surfaces may be and still be one
/// water, in m: Zora's River steps down 3 m from plane to plane.
const FALLS: f32 = 3.0;
/// How far a swim may rise, from the water it leaves to the water it
/// reaches, in m: no one swims up a waterfall.
const UPSTREAM: f32 = 0.5;
/// How fast Link swims, in m a second: the child's `SwimMaxSpeedMps`
/// (the child player's game params). The adult's isn't set there.
pub const SWIM_SPEED: f32 = 3.3;
/// The button that dashes Link through water, and how fast he goes then,
/// in m a second: `SwimDashSpeedMpsReverseStream`, the slowest dash, up a
/// stream. Swimming alone, Zora's River's current swept him 20 m past a
/// shore 8 m off (Mac, 2026-11-18).
pub const DASH: &str = "X";
pub const DASH_SPEED: f32 = 4.0;
/// How far above the surface a shore may stand for Link to climb out onto
/// it, in m: `SwimStartClamberHeight`, both ages.
const CLIMB_OUT: f32 = 0.6;
/// How deep water is before Link floats in it, in m, child and adult:
/// `FloatEffectiveHeight`. Shallower, he wades on the floor. Swimming,
/// he stands that far under the surface (Zora's River and Fountain, Mac,
/// 2026-11-18), so a floor that deep is water.
const FLOAT: [f32; 2] = [0.8, 1.3];

/// How deep water is before Link floats in it, at his age.
pub fn float(child: bool) -> f32 {
    FLOAT[usize::from(!child)]
}
/// How far past a shore its water is looked for, in m.
const OFF_SHORE: f32 = 1.0;
/// How near, flat, two shore points are to be one stretch of shore, in m.
const STRETCH: f32 = 2.0;
/// How far apart a swim's line is checked for land, in m.
const SAMPLE: f32 = 1.0;
/// The most pairs of shore points tried between two stretches, nearest
/// first, before they count as unlinked.
const TRIES: usize = 64;
/// How far over a water's surface Link may be and still be in it, in m:
/// he swims with his head above it.
const IN: f32 = 1.0;

/// A placed water plane, in the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Water {
    pub name: String,
    pub hash: Option<u64>,
    /// Its footprint's corners in x, z, in turn round it.
    pub corners: [[f32; 2]; 4],
    pub surface: f32,
    /// How far under the surface it reaches, in m.
    pub depth: f32,
    /// `None` when it's always there.
    pub presence: Option<Presence>,
}

/// Twice the signed area of a, b, c: positive when they turn
/// counterclockwise.
fn turn(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

impl Water {
    /// The water of a placement whose model spans `min` to `max`, its
    /// surface at the top.
    pub fn of(placed: &Placement, (min, max): Bounds) -> Self {
        let corner = |x: f32, z: f32| {
            let p = place([x, max[1], z], &placed.pose);
            [p[0], p[2]]
        };
        let surface = place([min[0], max[1], min[2]], &placed.pose)[1];
        Self {
            name: placed.name.clone(),
            hash: placed.hash,
            corners: [
                corner(min[0], min[2]),
                corner(max[0], min[2]),
                corner(max[0], max[2]),
                corner(min[0], max[2]),
            ],
            surface,
            depth: (max[1] - min[1]) * placed.pose.scale[1].abs(),
            presence: placed.presence.clone(),
        }
    }

    /// Whether its footprint covers (x, z), edges included.
    pub fn covers(&self, x: f32, z: f32) -> bool {
        let sides = (0..4).map(|i| turn(self.corners[i], self.corners[(i + 1) % 4], [x, z]));
        let (mut left, mut right) = (false, false);
        for t in sides {
            left |= t > 1e-6;
            right |= t < -1e-6;
        }
        !(left && right)
    }

    /// Whether Link at `p` is in it: over its footprint, from its depth
    /// under the surface to [`IN`] over it.
    pub fn holds(&self, p: [f32; 3]) -> bool {
        self.covers(p[0], p[2])
            && (self.surface - self.depth - IN..=self.surface + IN).contains(&p[1])
    }

    /// Whether it's there, with `set` saying which flags are set.
    pub fn shown(&self, set: impl Fn(&str) -> bool) -> bool {
        self.presence
            .as_ref()
            .is_none_or(|p| set(&p.flag) != p.negated)
    }

    /// Whether it and `other` are one water: their footprints overlapping
    /// and their surfaces within [`FALLS`] of each other.
    pub fn joins(&self, other: &Self) -> bool {
        let inside = |a: &Self, b: &Self| a.corners.iter().any(|c| b.covers(c[0], c[1]));
        (self.surface - other.surface).abs() <= FALLS
            && (inside(self, other) || inside(other, self))
    }
}

/// The box `points` span.
fn span(points: impl IntoIterator<Item = [f32; 3]>) -> Option<Bounds> {
    points.into_iter().fold(None, |acc, p| {
        let (lo, hi) = acc.unwrap_or((p, p));
        Some((
            std::array::from_fn(|i| lo[i].min(p[i])),
            std::array::from_fn(|i| hi[i].max(p[i])),
        ))
    })
}

/// The box an actor's water model spans: its volume meshes' when it has
/// them, else all its meshes', [`DEEP`] deep. `None` when it has no model.
pub fn volume(romfs: &Romfs, game: &Path, actor: &str) -> Result<Option<Bounds>> {
    let Some((project, model)) = crate::standins::model_file(&ActorPack::load(romfs, actor)?)?
    else {
        return Ok(None);
    };
    let path = format!("Model/{project}.{model}.bfres.zs");
    let [(path, data)] = &crate::game::search(game, &path)?[..] else {
        anyhow::bail!("no one model file {path} in the game");
    };
    let data = romfs.dicts().decompress(path, data)?;
    // A mesh whose vertices aren't plain floats (the Water Temple's water
    // surface) is left out: the rest of the model still spans its box.
    let meshes: Vec<(bool, Vec<[f32; 3]>)> = (ounce_formats::shape_names(&data).into_iter())
        .filter_map(|shape| {
            Some((
                shape.contains(VOLUME),
                ounce_formats::positions(&data, &shape).ok()?,
            ))
        })
        .collect();
    let has_volume = meshes.iter().any(|(v, _)| *v);
    let points = (meshes.into_iter())
        .filter(|(v, _)| *v || !has_volume)
        .flat_map(|(_, points)| points);
    Ok(span(points).map(|(mut min, max)| {
        if !has_volume {
            min[1] = max[1] - DEEP;
        }
        (min, max)
    }))
}

/// The waters among `placed` that `set` shows: each placement
/// [`kinds::water`] names whose actor has a model. Each actor's model is
/// read once.
pub fn waters(
    romfs: &Romfs,
    game: &Path,
    placed: &[Placement],
    set: impl Fn(&str) -> bool,
) -> Result<Vec<Water>> {
    let mut boxes: HashMap<&str, Option<Bounds>> = HashMap::new();
    let mut out = vec![];
    for p in placed.iter().filter(|p| kinds::water(&p.name)) {
        if !boxes.contains_key(p.name.as_str()) {
            boxes.insert(&p.name, volume(romfs, game, &p.name)?);
        }
        if let Some(b) = boxes[p.name.as_str()] {
            out.extend(Some(Water::of(p, b)).filter(|w| w.shown(&set)));
        }
    }
    Ok(out)
}

/// `0..n` in groups, chaining every pair `joined` says is together, each
/// group in order and the groups by their first.
fn groups(n: usize, joined: impl Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut of: Vec<usize> = (0..n).collect();
    fn root(of: &mut [usize], mut i: usize) -> usize {
        while of[i] != i {
            of[i] = of[of[i]];
            i = of[i];
        }
        i
    }
    for a in 0..n {
        for b in a + 1..n {
            if joined(a, b) {
                let (ra, rb) = (root(&mut of, a), root(&mut of, b));
                of[ra] = rb;
            }
        }
    }
    let mut out: Vec<Vec<usize>> = vec![];
    let mut at = std::collections::HashMap::new();
    for i in 0..n {
        let r = root(&mut of, i);
        let g = *at.entry(r).or_insert_with(|| {
            out.push(vec![]);
            out.len() - 1
        });
        out[g].push(i);
    }
    out
}

/// `waters` in groups that are one water ([`Water::joins`], chained), as
/// indexes into it.
pub fn bodies(waters: &[Water]) -> Vec<Vec<usize>> {
    groups(waters.len(), |a, b| waters[a].joins(&waters[b]))
}

/// A spot on the mesh's border where Link gets into or out of a water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shore {
    pub face: usize,
    pub at: [f32; 3],
    /// The water past it, as an index into the waters.
    pub water: usize,
}

/// Two stretches of one water's shore, and the swim between them or why
/// there is none.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    pub body: usize,
    pub from: Shore,
    pub to: Shore,
    /// Why Link doesn't swim it, or `None` when he does.
    pub not: Option<NoSwim>,
}

/// Why a crossing isn't swum, at the first spot of its line that fails.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoSwim {
    /// Mesh near the surface: Link walks there, or a wall stands there.
    Land([f32; 3]),
    /// No water of its body covers it.
    Dry([f32; 3]),
}

impl Graph<'_> {
    /// The graph with a link both ways across each water between each two
    /// stretches of its shore that [`Graph::crossings`] swims.
    pub fn with_swims(mut self, waters: &[Water], child: bool) -> Self {
        self.float = float(child);
        self.shores = self.find_shores(waters);
        self.waters = waters.to_vec();
        for c in self
            .crossings(waters)
            .into_iter()
            .filter(|c| c.not.is_none())
        {
            let via = Via::Swim(waters[c.from.water].hash);
            let weight = super::drive::SPEED / SWIM_SPEED;
            for (a, b) in [(c.from, c.to), (c.to, c.from)] {
                if waters[b.water].surface - waters[a.water].surface <= UPSTREAM {
                    self.link_weighted(a.face, b.face, via, [a.at, b.at], weight);
                }
            }
        }
        self
    }

    /// Whether `p` stands on land Link walks: mesh under it from
    /// above his [`float`] depth under `surface` to [`CLIMB_OUT`] over it.
    fn land(&self, p: [f32; 3], surface: f32) -> bool {
        (self.floor([p[0], surface, p[2]], self.float.max(CLIMB_OUT)))
            .is_some_and(|f| f.height > surface - self.float && f.height <= surface + CLIMB_OUT)
    }

    /// The shores of each water: each border edge's middle from Link's
    /// [`float`] depth under its surface to [`CLIMB_OUT`] over it, with that water and no
    /// land [`OFF_SHORE`] past it.
    pub fn find_shores(&self, waters: &[Water]) -> Vec<Shore> {
        let mut found = vec![];
        for Border { face, mid, out } in self.borders() {
            let past = [
                mid[0] + out[0] * OFF_SHORE,
                mid[1],
                mid[2] + out[1] * OFF_SHORE,
            ];
            let water = waters.iter().position(|w| {
                (w.surface - self.float..=w.surface + CLIMB_OUT).contains(&mid[1])
                    && w.covers(past[0], past[2])
                    && !self.land(past, w.surface)
            });
            if let Some(water) = water {
                found.push(Shore {
                    face,
                    at: mid,
                    water,
                });
            }
        }
        found
    }

    /// Whether Link swims straight from `a` to `b` in the body `body`, or
    /// the first spot that stops him: every [`SAMPLE`] m between them, past
    /// [`OFF_SHORE`] from each, is over its water and not over land.
    fn swims(&self, waters: &[Water], body: &[usize], a: [f32; 3], b: [f32; 3]) -> Option<NoSwim> {
        let far = flat(a, b);
        let steps = (far / SAMPLE).ceil() as usize;
        (0..=steps)
            .map(|i| i as f32 / steps.max(1) as f32)
            .filter(|t| t * far >= OFF_SHORE && (1.0 - t) * far >= OFF_SHORE)
            .find_map(|t| {
                let p: [f32; 3] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
                let over: Vec<&Water> = (body.iter().map(|&w| &waters[w]))
                    .filter(|w| w.covers(p[0], p[2]))
                    .collect();
                if over.is_empty() {
                    Some(NoSwim::Dry(p))
                } else if over.iter().all(|w| self.land(p, w.surface)) {
                    Some(NoSwim::Land(p))
                } else {
                    None
                }
            })
    }

    /// The spot on the surface over `p`, when `p` lies under a water the
    /// graph swims, deeper than Link floats and within its depth: where he
    /// sinks from to reach it.
    pub fn above(&self, p: [f32; 3]) -> Option<[f32; 3]> {
        let under = |w: &&Water| {
            w.covers(p[0], p[2])
                && (w.surface - w.depth - IN..w.surface - self.float).contains(&p[1])
        };
        let w = self.waters.iter().find(under)?;
        Some([p[0], w.surface, p[2]])
    }

    /// Whether Link at `p` swims: a water holds him, with no land under him
    /// near its surface.
    pub fn in_water(&self, p: [f32; 3]) -> bool {
        (self.waters.iter()).any(|w| w.holds(p) && !self.land(p, w.surface))
    }

    /// Where Link, in water at `p` off the mesh, swims ashore: the nearest
    /// shore point of the water he's in that he [`Graph::swims`] straight
    /// to, and the way there. `None` when no water the graph swims holds
    /// him, or no shore of it is in reach.
    pub fn ashore(&self, p: [f32; 3]) -> Option<(Snap, Via)> {
        let waters = &self.waters;
        let w = waters.iter().position(|w| w.holds(p))?;
        let body = bodies(waters).into_iter().find(|b| b.contains(&w))?;
        let mut near: Vec<&Shore> = (self.shores.iter())
            .filter(|s| body.contains(&s.water))
            .collect();
        near.sort_by(|a, b| flat(a.at, p).total_cmp(&flat(b.at, p)));
        let s = (near.into_iter().take(TRIES))
            .find(|s| self.swims(waters, &body, p, s.at).is_none())?;
        Some((
            Snap {
                face: s.face,
                at: s.at,
            },
            Via::Swim(waters[s.water].hash),
        ))
    }

    /// Each two stretches of shore of each body of water: its shore points
    /// within [`STRETCH`] of each other and at one water's shore, chained. The nearest pair between
    /// them that [`Graph::swims`], or the nearest pair and why not when none
    /// does.
    pub fn crossings(&self, waters: &[Water]) -> Vec<Crossing> {
        let shores = self.find_shores(waters);
        let mut found = vec![];
        for (n, body) in bodies(waters).iter().enumerate() {
            let on: Vec<Shore> = (shores.iter())
                .filter(|s| body.contains(&s.water))
                .copied()
                .collect();
            let stretches = groups(on.len(), |a, b| {
                on[a].water == on[b].water && flat(on[a].at, on[b].at) <= STRETCH
            });
            for (i, one) in stretches.iter().enumerate() {
                for other in &stretches[i + 1..] {
                    let mut pairs: Vec<(usize, usize)> = (one.iter())
                        .flat_map(|&a| other.iter().map(move |&b| (a, b)))
                        .collect();
                    pairs.sort_by(|x, y| {
                        dist(on[x.0].at, on[x.1].at).total_cmp(&dist(on[y.0].at, on[y.1].at))
                    });
                    let tried = (pairs.iter().take(TRIES))
                        .map(|&(a, b)| (a, b, self.swims(waters, body, on[a].at, on[b].at)));
                    let mut first = None;
                    for (a, b, not) in tried {
                        first = first.or(Some((a, b, not)));
                        if not.is_none() {
                            first = Some((a, b, None));
                            break;
                        }
                    }
                    let (a, b, not) = first.expect("two stretches have a pair");
                    found.push(Crossing {
                        body: n,
                        from: on[a],
                        to: on[b],
                        not,
                    });
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_4;

    use super::*;
    use crate::warp::Pose;

    pub(crate) fn placed(at: [f32; 3], yaw: f32, scale: [f32; 3]) -> Placement {
        Placement {
            name: "Obj_FakeWaterPlane".into(),
            hash: Some(7),
            label: None,
            pose: Pose {
                at,
                rotate: [0.0, yaw, 0.0],
                scale,
            },
            presence: None,
        }
    }

    /// Zora's River as a child, from the dump.
    fn zora_river() -> Option<crate::nav::Level> {
        let romfs = crate::game::dev_dump()?;
        let game = std::path::Path::new("../../dump/oot.nup");
        Some(crate::nav::Level::of(&romfs, game, crate::known::map::ZORA_RIVER_CHILD).unwrap())
    }

    /// Zora's River's Gossip Stone stands on a bank Link reaches from the
    /// east one only by swimming (Mac, 2026-11-18).
    #[test]
    fn a_route_across_zoras_river_swims() {
        let Some(level) = zora_river() else {
            return;
        };
        assert!(level.child);
        let (bank, stone) = ([124.6, 112.7, -44.7], [117.9, 112.1, -33.7]);
        let route = crate::nav::route::plan(&level.graph(), bank, stone).unwrap();
        assert!(
            route
                .waypoints
                .iter()
                .any(|w| matches!(w.via, Via::Swim(Some(_)))),
            "{route:?}"
        );
        let dry = Graph::build(&level.mesh, &level.placed, super::super::graph::STEP_UP);
        assert!(crate::nav::route::plan(&dry, bank, stone).is_err());
    }

    /// From the pool below the falls, off the mesh, a route swims ashore
    /// first: Link reached the Gold Skulltula from there twice (Mac,
    /// 2026-11-18).
    #[test]
    fn a_route_from_the_water_swims_ashore_first() {
        let Some(level) = zora_river() else {
            return;
        };
        let graph = level.graph();
        let pool = [101.9, 109.7, -36.0];
        assert!(graph.in_water(pool));
        let spider = level.placement(8695975724114049706).unwrap().pose.at;
        let route = crate::nav::route::plan(&graph, pool, spider).unwrap();
        assert!(
            matches!(route.waypoints[1].via, Via::Swim(Some(_))),
            "{route:?}"
        );
    }

    #[test]
    fn a_child_floats_in_shallower_water_than_an_adult() {
        assert_eq!((float(true), float(false)), (0.8, 1.3));
    }

    /// A plain water tile's box: 10 m square, [`DEEP`] deep.
    pub(crate) const TILE: Bounds = ([-5.0, -DEEP, -5.0], [5.0, 0.0, 5.0]);

    #[test]
    fn a_plane_covers_its_scaled_tile_at_its_height() {
        let w = Water::of(&placed([100.0, 20.0, 0.0], 0.0, [2.0, 0.5, 1.0]), TILE);
        assert_eq!(w.surface, 20.0);
        assert_eq!(w.depth, 5.0);
        assert!(w.covers(90.5, 4.5) && w.covers(109.0, -4.9));
        assert!(!w.covers(111.0, 0.0) && !w.covers(100.0, 5.5));
    }

    #[test]
    fn a_model_off_center_covers_its_own_box() {
        // Zora's River's first flowing water: its box isn't centered.
        let b = ([-10.2, -4.0, -12.1], [10.0, 0.0, 11.9]);
        let w = Water::of(&placed([0.0, 50.0, 0.0], 0.0, [1.0, 2.0, 1.0]), b);
        assert_eq!((w.surface, w.depth), (50.0, 8.0));
        assert!(w.covers(-10.0, -12.0) && !w.covers(10.1, 0.0));
    }

    #[test]
    fn a_turned_plane_covers_its_turned_tile() {
        // An eighth of a turn: the tile's corner now points along +x.
        let w = Water::of(&placed([0.0; 3], FRAC_PI_4, [1.0; 3]), TILE);
        assert!(w.covers(6.9, 0.0));
        assert!(!w.covers(4.5, 4.5));
    }

    #[test]
    fn a_spot_deeper_than_link_floats_is_sunk_to_from_the_surface_over_it() {
        let w = Water::of(&placed([0.0, 20.0, 0.0], 0.0, [1.0; 3]), TILE);
        let mesh = crate::nav::fixture::mesh(&[]);
        let graph = Graph::build(&mesh, &[], crate::nav::graph::STEP_UP).with_swims(&[w], false);
        assert_eq!(graph.above([1.0, 12.0, 2.0]), Some([1.0, 20.0, 2.0]));
        assert_eq!(graph.above([1.0, 19.0, 2.0]), None, "he floats there");
        assert_eq!(graph.above([1.0, 8.0, 2.0]), None, "under its depth");
        assert_eq!(graph.above([6.0, 12.0, 2.0]), None, "off its tile");
    }

    #[test]
    fn a_plane_shows_by_its_flag() {
        let mut p = placed([0.0; 3], 0.0, [1.0; 3]);
        p.presence = Some(Presence {
            flag: "FakeLevel_Low".into(),
            negated: true,
        });
        let w = Water::of(&p, TILE);
        assert!(w.shown(|_| false));
        assert!(!w.shown(|f| f == "FakeLevel_Low"));
        assert!(Water::of(&placed([0.0; 3], 0.0, [1.0; 3]), TILE).shown(|_| false));
    }

    #[test]
    fn overlapping_planes_a_fall_apart_or_less_are_one_water() {
        let at = |x: f32, y: f32| Water::of(&placed([x, y, 0.0], 0.0, [1.0; 3]), TILE);
        let waters = [
            at(0.0, 0.0),
            at(8.0, 0.0),
            at(16.0, 2.0),
            at(0.0, 8.0),
            at(40.0, 0.0),
        ];
        assert_eq!(bodies(&waters), vec![vec![0, 1, 2], vec![3], vec![4]]);
    }
}
