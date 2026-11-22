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
pub mod drive;
pub mod graph;
pub mod kinds;
pub mod leap;
pub mod limits;
pub mod mesh;
pub mod route;
pub mod scene;
pub mod shape;
pub mod shove;
#[cfg(test)]
mod sim;
pub mod sink;
pub mod trace;
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
fn dump_file(romfs: &Romfs, game: &Path, path: &str) -> Result<Option<Vec<u8>>> {
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
    pub climbs: Vec<climb::Climb>,
    /// The waters there, by the flags their presence reads.
    pub waters: Vec<water::Water>,
    /// Whether Link is a child there, which sets how deep he wades.
    pub child: bool,
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
            climbs: climb::climbs(romfs, &placed)?,
            waters: water::waters(romfs, game, &placed, |_| false)?,
            child: map.ends_with("_Child"),
            placed,
        })
    }

    /// A level with no solids or climbs, as a test builds one.
    #[cfg(test)]
    pub fn new(map: &str, mesh: NavMesh, placed: Vec<Placement>) -> Self {
        Self {
            map: map.into(),
            mesh,
            placed,
            solids: vec![],
            climbs: vec![],
            waters: vec![],
            child: false,
        }
    }

    /// The graph of its mesh, with its doors, climbs, swims and solids.
    pub fn graph(&self) -> graph::Graph<'_> {
        graph::Graph::build(&self.mesh, &self.placed, graph::step_up())
            .with_climbs(&self.climbs)
            .with_swims(&self.waters, self.child)
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
    kinds::word(&holder.name) == Some("item") && graph.floor(holder.pose.at, UNDERFOOT).is_none()
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
/// floor first, each kind `first`, then `then`'s, then the rest. The mesh can't see every void: two sides of Kokiri
/// Forest's sticks scrub stand on it and still void Link out (Mac,
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
        let placed = |name: &str, at| Placement {
            name: name.into(),
            hash: None,
            label: None,
            pose: crate::warp::Pose {
                at,
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        };
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
        let at = |name: &str, y| Placement {
            name: name.to_string(),
            hash: None,
            label: None,
            pose: crate::warp::Pose {
                at: [2.0, y, 2.0],
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        };
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
        let scrub = Placement {
            name: actor::ENEMY_SCRUB_BUSINESS.to_string(),
            hash: None,
            label: None,
            pose: crate::warp::Pose {
                at: [2.0, 0.0, 3.5],
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        };
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
        let far = Placement {
            pose: crate::warp::Pose {
                at: [20.0, 0.0, 20.0],
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            ..scrub
        };
        let off = warp::landing_at(&far.pose, warp::Side::Front, 1.2);
        assert_eq!(landing(&graph, &far, off, Side::Front), off);
        let left = warp::landing_at(&far.pose, warp::Side::Left, 1.2);
        assert!(graph::dist(landing(&graph, &far, off, Side::Left), left) < 1e-3);
    }

    #[test]
    fn a_holder_use_acts_on_wins_over_a_wall_or_area_at_its_pose() {
        needs_names!();
        let placed = |name: &str| Placement {
            name: name.into(),
            hash: None,
            label: None,
            pose: crate::warp::Pose {
                at: [2.0, 0.0, 2.0],
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        };
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
