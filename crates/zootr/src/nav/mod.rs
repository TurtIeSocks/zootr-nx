//! Getting around a map: its navmesh as a graph of faces, routes over it,
//! what lies around Link by clock hour, and the words that name a
//! placement.

use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{NavMesh, Romfs};

use crate::dev::Dev;
use crate::known::field;
use crate::live::Memory;
use crate::warp::{self, Placement, Side};

/// `Display` and `Serialize` for an enum whose variants are said in plain
/// words, like `no route`.
macro_rules! said {
    ($type:ty { $($variant:ident => $words:literal),+ $(,)? }) => {
        impl std::fmt::Display for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(match self {
                    $(Self::$variant => $words),+
                })
            }
        }

        impl serde::Serialize for $type {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
    };
}

pub mod act;
pub mod clear;
pub mod climb;
pub mod crawl;
pub mod drive;
pub mod filter;
pub mod floors;
pub mod graph;
pub mod kinds;
pub mod leap;
pub mod limits;
pub mod mesh;
pub mod picture;
pub mod reach;
pub mod route;
pub mod scene;
pub mod shape;
pub mod shove;
#[cfg(test)]
mod sim;
pub mod sink;
pub mod terrain;
pub mod trace;
pub mod vec3;
pub mod water;
pub mod went;

/// The stick directions clockwise from up, an eighth of a turn apart.
pub(crate) const CLOCKWISE: [&str; 8] = [
    "up",
    "up-right",
    "right",
    "down-right",
    "down",
    "down-left",
    "left",
    "up-left",
];

/// The stick direction, with the camera behind Link, that points him from
/// `yaw` at `from` toward `to`, to the nearest eighth of a turn.
pub fn heading(yaw: f32, from: [f32; 3], to: [f32; 3]) -> &'static str {
    CLOCKWISE[eighths(yaw, from, to).rem_euclid(8) as usize]
}

/// How many eighths of a turn clockwise of `yaw` Link at `from` turns to
/// face `to`: a tap that far clockwise of up turns him that far down from
/// `yaw`.
pub fn eighths(yaw: f32, from: [f32; 3], to: [f32; 3]) -> i32 {
    (crate::around::clockwise(yaw, from, to) / std::f32::consts::FRAC_PI_4).round() as i32
}

/// A map's navmesh file, decompressed, or `None` for a map whose mesh
/// comes in tiles instead (Hyrule Field, under `Phive/StitchedNavMesh`).
pub fn navmesh_file(romfs: &Romfs, game: &Path, map: &str) -> Result<Option<Vec<u8>>> {
    dump_file(romfs, game, &format!("Phive/{}/{map}.", field::NAV_MESH))
}

/// The one file in the dump whose path contains `path`, decompressed, or
/// `None` without one.
pub fn dump_file(romfs: &Romfs, game: &Path, path: &str) -> Result<Option<Vec<u8>>> {
    match &crate::game::search(game, path)?[..] {
        [] => Ok(None),
        [(path, data)] => romfs.dicts().decompress(path, data).map(Some),
        found => anyhow::bail!("{} files match {path}", found.len()),
    }
}

/// A map's navmesh. A tiled map reads as an empty mesh, since the tools
/// don't stitch tiles yet, and a walk there goes in a straight line.
pub fn load(romfs: &Romfs, game: &Path, map: &str) -> Result<NavMesh> {
    match navmesh_file(romfs, game, map)? {
        Some(bytes) => NavMesh::parse(&bytes),
        None => Ok(NavMesh::default()),
    }
}

/// How near two poses are to be the same placement's, in m.
const SAME: f32 = 0.01;

/// A map as the tools read it: its navmesh, its placements, and the solids,
/// climbs and waters among them that the mesh doesn't know.
pub struct Level {
    pub map: String,
    pub mesh: NavMesh,
    pub placed: Vec<Placement>,
    pub solids: Vec<shape::Solid>,
    /// The collision meshes its pieces stand.
    pub walls: mesh::Walls,
    pub climbs: Vec<climb::Climb>,
    /// Its crawl holes, which only a child goes through.
    pub crawls: Vec<crawl::Crawl>,
    /// The waters there, by the flags their presence reads.
    pub waters: Vec<water::Water>,
    /// Whether Link is a child there, which sets how deep he wades.
    pub child: bool,
    /// The first of the mesh's faces that are collision floor, not
    /// navmesh: none until [`Level::with_floors`] adds them.
    pub floors: Option<usize>,
    /// Of the collision floor faces, from [`Level::floors`] on, which
    /// are terrain's.
    pub terrain: Vec<bool>,
}

impl Level {
    /// The map the running game is in, its waters by the game's flags: the
    /// Water Temple's levels.
    pub fn read<M: Memory>(dev: &Dev<M>, romfs: &Romfs, game: &Path) -> Result<Self> {
        let state = dev.state()?;
        let mut level = Self::of(romfs, game, &state.map)?;
        level.child = state.child;
        if level.placed.iter().any(|p| p.presence.is_some()) {
            let gd = crate::gamedata::GameData::load(romfs)?;
            let set = |flag: &str| {
                crate::live::hash(flag)
                    .and_then(|h| crate::live::read(dev.game(), &gd, dev.global(), h))
                    .is_ok_and(|v| v == "true")
            };
            level.waters = water::waters(romfs, game, &level.placed, set)?;
        }
        Ok(level)
    }

    /// The map `map`, its waters as with no flag set, Link a child in a
    /// child's map (`_Child`) and an adult anywhere else.
    pub fn of(romfs: &Romfs, game: &Path, map: &str) -> Result<Self> {
        let placed = warp::placements_in(romfs, map)?;
        Ok(Self {
            map: map.to_owned(),
            mesh: load(romfs, game, map)?,
            solids: shape::solids(romfs, &placed)?,
            walls: mesh::walls(romfs, game, &placed)?,
            climbs: climb::climbs(romfs, &placed)?,
            crawls: crawl::crawls(&placed),
            waters: water::waters(romfs, game, &placed, |_| false)?,
            child: map.ends_with("_Child"),
            floors: None,
            terrain: vec![],
            placed,
        })
    }

    /// The level with its collision floors (room reach spec, rules 9 to
    /// 11) added to the mesh as faces, for Link at his age there, none
    /// under the obstacles `under` (shutters, webs). Derive
    /// uses them; the sweep and `dev go` keep the navmesh alone until a
    /// live run shows routes over them hold.
    pub fn with_floors(mut self, romfs: &Romfs, under: &[shape::Solid]) -> Result<Self> {
        let height = floors::height(romfs, self.child)?;
        let float = water::float(self.child);
        let keep = floors::Keep {
            waters: &self.waters,
            float,
            height,
            under,
            shapes: &self.solids,
        };
        let patches = floors::patches(&self.mesh, &self.walls, &keep);
        let first = self.mesh.faces.len();
        let navmesh = std::mem::take(&mut self.mesh);
        self.floors = Some(first);
        self.mesh = floors::joined(&navmesh, &patches);
        // Only the floor Link gets onto from the navmesh: not the top of a
        // doorway's frame he could only drop off.
        let reached = floors::reached(&self.graph(), first, limits::limits().level_jump());
        // The patches' triangles are the faces from `first` on, in order.
        let mut faces = reached[first..].iter();
        let kept: Vec<Vec<floors::Tri>> = (patches.into_iter())
            .map(|p| {
                (p.into_iter().zip(faces.by_ref()))
                    .filter_map(|(t, &reached)| reached.then_some(t))
                    .collect()
            })
            .collect();
        self.terrain = floors::on_terrain(&self.walls, &kept);
        self.mesh = floors::joined(&navmesh, &kept);
        Ok(self)
    }

    /// Whether face `f` is collision floor on terrain.
    pub fn on_terrain(&self, f: usize) -> bool {
        (self.floors).is_some_and(|first| f >= first && self.terrain.get(f - first) == Some(&true))
    }

    /// A level with no solids or climbs, as a test builds one.
    #[cfg(test)]
    pub fn new(map: &str, mesh: NavMesh, placed: Vec<Placement>) -> Self {
        Self {
            map: map.into(),
            mesh,
            placed,
            solids: vec![],
            walls: mesh::Walls::default(),
            climbs: vec![],
            crawls: vec![],
            waters: vec![],
            child: false,
            floors: None,
            terrain: vec![],
        }
    }

    /// The graph of its mesh, with its doors, climbs, swims, collision
    /// meshes and solids.
    pub fn graph(&self) -> graph::Graph<'_> {
        let graph = graph::Graph::build(&self.mesh, &self.placed, graph::step_up());
        let graph = match self.floors {
            Some(first) => graph.with_floors(first, &self.walls),
            None => graph,
        };
        // Swims first: a climb out of water starts from the swim sheet.
        graph
            .with_swims(&self.waters, self.child)
            .with_climbs(&self.climbs)
            .with_crawls(if self.child { &self.crawls } else { &[] })
            .with_walls(&self.walls)
            .with_solids(&self.solids)
    }

    /// The placement with `hash`.
    pub fn placement(&self, hash: u64) -> Result<&Placement> {
        (self.placed.iter().find(|p| p.hash == Some(hash)))
            .with_context(|| format!("{hash} isn't placed in {}", self.map))
    }

    /// The placement standing at `at`, for a caller that knows a holder
    /// by its pose rather than its hash. One `use` acts on goes first, then
    /// one Link can see: a chest shares its pose with the area around it,
    /// and the Spirit Temple's boss key chest with the fire wall round it.
    pub fn standing_at(&self, at: [f32; 3]) -> Result<&Placement> {
        let here = || (self.placed.iter()).filter(|p| graph::dist(p.pose.at, at) < SAME);
        let seen = |p: &&Placement| crate::around::seen(&p.name);
        let used = |p: &&Placement| kinds::kind_of(&p.name).is_some_and(|k| k.act.is_some());
        (here().find(|p| seen(p) && used(p)))
            .or_else(|| here().find(seen))
            .or_else(|| here().next())
            .with_context(|| format!("nothing is placed at {at:?} in {}", self.map))
    }
}

/// How far below a warp's landing its floor may lie: the warp lifts Link
/// half a metre, and a floor farther down is past a drop.
const UNDERFOOT: f32 = 1.0;

/// An item lying out up where the mesh has no floor a step under it: the
/// Graveyard's heart piece tops a pillar 1.2 m over the path, and A from
/// the path never reached it in 15 sweeps. Warped onto the pillar, Link
/// took it with A (Mac, 2026-11-17).
pub fn aloft(graph: &graph::Graph, holder: &Placement) -> bool {
    kinds::is_item(&holder.name) && graph.floor(holder.pose.at, UNDERFOOT).is_none()
}

/// Where to warp beside `holder`: its `first` side, `planned` for the
/// front, unless no floor lies under it (past a ledge, over a pit), then
/// the first other side at the same distance that has one. A warp over a
/// drop voids Link out, back to where he entered the map. With no floor on
/// any side, as beside a chest in a hole in the mesh, its `first` side all
/// the same: the Well's key chest sent to its back landed on its front, a
/// step below it, and the walk round went off a ledge (other machine,
/// 2026-11-16).
pub fn landing(
    graph: &graph::Graph,
    holder: &Placement,
    planned: [f32; 3],
    first: Side,
) -> [f32; 3] {
    landings(graph, holder, planned, first, &[])[0]
}

/// Every side [`landing`] could pick, in the order it would: those with
/// floor before those without, and within each, `first`, then `then`'s
/// sides, then the rest. The mesh can't see every void: two sides of
/// Kokiri Forest's sticks scrub stand on it and still void Link out (Mac,
/// 2026-11-17), so a warp that didn't hold tries the next.
pub fn landings(
    graph: &graph::Graph,
    holder: &Placement,
    planned: [f32; 3],
    first: Side,
    then: &[Side],
) -> Vec<[f32; 3]> {
    let ahead = (planned[0] - holder.pose.at[0]).hypot(planned[2] - holder.pose.at[2]);
    let pose = warp::facing(&holder.name, &holder.pose);
    let at = |side| match side {
        Side::Front => planned,
        side => warp::landing_at(&pose, side, ahead),
    };
    let (mut floored, bare): (Vec<_>, Vec<_>) =
        (warp::ranked(first, then).map(at)).partition(|&at| graph.floor(at, UNDERFOOT).is_some());
    floored.extend(bare);
    floored
}

/// Hand-built meshes for the tests.
#[cfg(test)]
pub(crate) mod fixture {
    use ounce_formats::{Edge, Face, NavMesh};

    use crate::warp::{Placement, Pose};

    /// `name` placed at `at` with `hash`, unturned.
    pub fn placed(name: &str, hash: Option<u64>, at: [f32; 3]) -> Placement {
        turned(name, hash, at, [0.0; 3])
    }

    /// `name` placed at `at` with `hash`, turned by `rotate`.
    pub fn turned(name: &str, hash: Option<u64>, at: [f32; 3], rotate: [f32; 3]) -> Placement {
        Placement {
            name: name.into(),
            hash,
            label: None,
            pose: Pose {
                at,
                rotate,
                scale: [1.0; 3],
            },
            presence: None,
        }
    }

    /// A mesh of convex faces, each a ring of corners `[x, y, z]`. Corners
    /// at one spot share a vertex, and rings that share an edge are linked
    /// across it.
    pub fn mesh(rings: &[Vec<[f32; 3]>]) -> NavMesh {
        let mut mesh = NavMesh::default();
        for ring in rings {
            let ids: Vec<usize> = ring
                .iter()
                .map(|&p| match mesh.vertices.iter().position(|&v| v == p) {
                    Some(i) => i,
                    None => {
                        mesh.vertices.push(p);
                        mesh.vertices.len() - 1
                    }
                })
                .collect();
            let start_edge = mesh.edges.len();
            for (i, &a) in ids.iter().enumerate() {
                let b = ids[(i + 1) % ids.len()];
                mesh.edges.push(Edge { a, b, across: None });
            }
            mesh.faces.push(Face {
                start_edge,
                edges: ids.len(),
            });
        }
        for f in 0..mesh.faces.len() {
            for e in mesh.faces[f].start_edge..mesh.faces[f].start_edge + mesh.faces[f].edges {
                let (a, b) = (mesh.edges[e].a, mesh.edges[e].b);
                let across = (0..mesh.faces.len())
                    .filter(|&g| g != f)
                    .find(|&g| (mesh.ring(g).iter()).any(|o| o.a == b && o.b == a));
                mesh.edges[e].across = across;
            }
        }
        mesh.erosion = 0.4;
        mesh
    }

    /// A flat rectangle at height `y`.
    pub fn quad(x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> Vec<[f32; 3]> {
        vec![[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    #[test]
    fn link_turns_toward_the_holder_by_one_tap() {
        use std::f32::consts::{FRAC_PI_2, PI};
        let here = [0.0, 0.0, 0.0];
        // Measured on the Mac: facing -Z (yaw pi), a right tap after ZL
        // turned him to +X (yaw pi/2).
        assert_eq!(heading(PI, here, [1.0, 0.0, 0.0]), "right");
        assert_eq!(heading(FRAC_PI_2, here, [0.0, 0.0, 1.0]), "right");
        assert_eq!(heading(0.0, here, [-1.0, 0.0, 1.0]), "up-right");
        assert_eq!(heading(0.0, here, [1.0, 0.0, 0.0]), "left");
        assert_eq!(heading(0.0, here, [0.0, 0.0, -1.0]), "down");
        assert_eq!(heading(-PI, here, [0.0, 0.0, 1.0]), "down");
        assert_eq!(heading(0.1, [5.0, 9.0, 5.0], [5.0, 0.0, 6.0]), "up");
    }

    #[test]
    fn a_holder_is_found_by_where_it_stands() {
        let placed = |name: &str, at| fixture::placed(name, None, at);
        let level = Level::new(
            "Test",
            fixture::mesh(&[fixture::quad(0.0, 0.0, 4.0, 4.0, 0.0)]),
            vec![
                placed("Tree", [1.0, 0.0, 1.0]),
                placed("Npc", [2.5, 0.0, 3.0]),
            ],
        );
        assert_eq!(level.standing_at([2.5, 0.0, 3.0]).unwrap().name, "Npc");
        assert!(level.standing_at([2.5, 0.0, 2.0]).is_err());
    }

    #[test]
    fn an_item_is_aloft_only_with_no_floor_a_step_under_it() {
        let floor = fixture::mesh(&[fixture::quad(0.0, 0.0, 4.0, 4.0, 0.0)]);
        let graph = graph::Graph::build(&floor, &[], graph::step_up());
        // Made up: a `known` name reads as its hash without the dump.
        let at = |name: &str, y| fixture::placed(name, None, [2.0, y, 2.0]);
        assert!(
            aloft(&graph, &at("Obj_FakeHeart_Piece", 1.5)),
            "on a pillar"
        );
        assert!(
            !aloft(&graph, &at("Obj_FakeHeart_Piece", 0.0)),
            "on the floor"
        );
        assert!(!aloft(&graph, &at("Obj_FakeTBox", 1.5)), "a chest isn't");
    }

    #[test]
    fn a_landing_past_the_edge_moves_to_a_side_with_floor() {
        // A 4 by 4 m ledge, and a scrub at its +Z edge facing off it.
        let ledge = fixture::mesh(&[fixture::quad(0.0, 0.0, 4.0, 4.0, 0.0)]);
        let graph = graph::Graph::build(&ledge, &[], graph::step_up());
        let scrub = fixture::placed(actor::ENEMY_SCRUB_BUSINESS, None, [2.0, 0.0, 3.5]);
        let front = warp::landing_at(&scrub.pose, warp::Side::Front, 1.2);
        assert!(graph.floor(front, UNDERFOOT).is_none(), "{front:?}");
        let at = landing(&graph, &scrub, front, Side::Front);
        assert_ne!(at, front);
        assert!(graph.floor(at, UNDERFOOT).is_some(), "{at:?}");
        // One on the floor already stays where it was planned.
        let inside = warp::landing_at(&scrub.pose, warp::Side::Back, 1.2);
        assert_eq!(landing(&graph, &scrub, inside, Side::Front), inside);
        // A first side with floor is landed on ahead of the front.
        let back = landing(&graph, &scrub, front, Side::Back);
        assert!(graph::dist(back, inside) < 1e-3, "{back:?}");
        // Every side is listed for a warp that doesn't hold, floor first.
        let all = landings(&graph, &scrub, front, Side::Front, &[]);
        assert_eq!(all.len(), 4);
        assert_eq!(all.last(), Some(&front), "the front has no floor");
        assert!(
            all[..3]
                .iter()
                .all(|&at| graph.floor(at, UNDERFOOT).is_some())
        );
        // With no floor on any side, the first side all the same.
        let far = fixture::placed(actor::ENEMY_SCRUB_BUSINESS, None, [20.0, 0.0, 20.0]);
        let off = warp::landing_at(&far.pose, warp::Side::Front, 1.2);
        assert_eq!(landing(&graph, &far, off, Side::Front), off);
        let left = warp::landing_at(&far.pose, warp::Side::Left, 1.2);
        assert!(graph::dist(landing(&graph, &far, off, Side::Left), left) < 1e-3);
    }

    #[test]
    fn a_holder_use_acts_on_wins_over_a_wall_or_area_at_its_pose() {
        needs_names!();
        let placed = |name: &str| fixture::placed(name, None, [2.0, 0.0, 2.0]);
        let level = Level::new(
            "Test",
            fixture::mesh(&[fixture::quad(0.0, 0.0, 4.0, 4.0, 0.0)]),
            vec![
                placed(actor::DROP_KEEP_AWAY_AREA_T_BOX_A_01),
                placed("Obj_W_A_02"),
                placed(actor::OBJ_T_BOX_A_01),
            ],
        );
        let holder = level.standing_at([2.0, 0.0, 2.0]).unwrap();
        assert_eq!(holder.name, actor::OBJ_T_BOX_A_01);
    }
}
