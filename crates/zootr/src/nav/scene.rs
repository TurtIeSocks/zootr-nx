//! What lies around Link by clock hour: the navmesh's borders (a wall, a
//! drop, a step) and the things `around` lists, each named by the kind of
//! placement near it. A pure function of the mesh, the placements, and
//! where Link stands and faces.

use std::fmt;
use std::path::Path;

use anyhow::Result;
use ounce_formats::{NavMesh, Romfs};
use serde::Serialize;

use super::graph::{DROP_MIN, FLOOR, LOOK, STEP_MIN, STEP_UP};
use super::kinds;
use crate::around::{self, clockwise, hour};
use crate::checks::flat;
use crate::dev::Dev;
use crate::live::Memory;
use crate::warp::Placement;

/// How far from a border a placement may be and still name it.
const NAMES: f32 = 3.0;
/// How near a thing's own spot a border is no wall in the way to it: a
/// chest sits in a hole of the mesh, and a doorway is one too.
const HOLE: f32 = 1.5;
/// A height under this is level, and isn't said.
const LEVEL: f32 = 0.5;

/// One thing around Link.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub hour: u8,
    /// `wall`, `drop` or `step`, or a thing's kind word, or its own name
    /// when the word table doesn't know it.
    pub what: String,
    /// Measured flat.
    pub distance: f32,
    /// A thing's height above Link. For a drop or a step, the floor past it
    /// relative to his. Otherwise 0.
    pub height: f32,
    /// The placement this is, or for a border the one that names it.
    pub name: Option<String>,
    pub hash: Option<u64>,
    /// The straight line to it crosses a wall.
    pub behind_wall: bool,
}

#[derive(Debug, Serialize)]
pub struct Scene {
    pub map: String,
    pub link: [f32; 3],
    pub yaw: f32,
    pub on_mesh: bool,
    /// At most a border and a thing for each hour, nearest first.
    pub entries: Vec<Entry>,
}

/// Where segment `p`..`q` crosses segment `a`..`b`, as how far along
/// `p`..`q` it is, measured flat.
fn crossing(p: [f32; 3], q: [f32; 3], a: [f32; 3], b: [f32; 3]) -> Option<f32> {
    let cross = |u: [f32; 2], v: [f32; 2]| u[0] * v[1] - u[1] * v[0];
    let r = [q[0] - p[0], q[2] - p[2]];
    let s = [b[0] - a[0], b[2] - a[2]];
    let ap = [a[0] - p[0], a[2] - p[2]];
    let denom = cross(r, s);
    if denom.abs() < 1e-9 {
        return None;
    }
    let (t, u) = (cross(ap, s) / denom, cross(ap, r) / denom);
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// The nearest placement within `NAMES` of `at` that lies the way
/// `toward` points from it, and has a name to give.
fn namer(placed: &[Placement], at: [f32; 3], toward: [f32; 2]) -> Option<&Placement> {
    placed
        .iter()
        .filter(|p| kinds::label(&p.name).is_some())
        .filter(|p| flat(at, p.pose.at) <= NAMES)
        .filter(|p| {
            let ahead = (p.pose.at[0] - at[0]) * toward[0] + (p.pose.at[2] - at[2]) * toward[1];
            ahead >= 0.0
        })
        .min_by(|a, b| flat(at, a.pose.at).total_cmp(&flat(at, b.pose.at)))
}

/// What's within `radius` of Link at `link` facing by `yaw` in `map`.
pub fn scene(
    map: &str,
    mesh: &NavMesh,
    placed: &[Placement],
    link: [f32; 3],
    yaw: f32,
    radius: f32,
) -> Scene {
    let mut walls = vec![];
    let mut borders: Vec<Entry> = vec![];
    for b in mesh.borders(link, radius, FLOOR, LOOK) {
        let at = [
            link[0] + b.toward[0] * b.distance,
            link[1],
            link[2] + b.toward[1] * b.distance,
        ];
        let rise = b.beyond.unwrap_or(0.0);
        let what = match b.beyond {
            Some(h) if h <= -DROP_MIN => "drop",
            Some(h) if (STEP_MIN..=STEP_UP).contains(&h) => "step",
            Some(h) if h.abs() < STEP_MIN => continue,
            _ => "wall",
        };
        if what == "wall" {
            let e = mesh.edges[b.edge];
            walls.push((mesh.vertices[e.a], mesh.vertices[e.b]));
        }
        let named = namer(placed, at, b.toward);
        borders.push(Entry {
            hour: hour(clockwise(yaw, link, at)),
            what: what.into(),
            distance: b.distance,
            height: if what == "wall" { 0.0 } else { rise },
            name: named.map(|p| p.name.clone()),
            hash: named.and_then(|p| p.hash),
            behind_wall: false,
        });
    }
    let behind = |at: [f32; 3]| {
        let length = flat(link, at);
        walls
            .iter()
            .any(|&(a, b)| crossing(link, at, a, b).is_some_and(|t| (1.0 - t) * length > HOLE))
    };
    let things = around::near(placed.to_vec(), link, yaw, radius, false)
        .into_iter()
        .map(|n| Entry {
            hour: n.hour,
            what: kinds::word(&n.name).map_or_else(|| n.name.clone(), String::from),
            distance: n.distance,
            height: n.height,
            behind_wall: behind(n.at),
            name: Some(n.name),
            hash: n.hash,
        });
    let things: Vec<Entry> = things.collect();
    // The nearest border of each hour, and the nearest thing the word table
    // knows, else the nearest thing.
    let mut entries: Vec<Entry> = vec![];
    for h in 1..=12 {
        let border = borders
            .iter()
            .filter(|e| e.hour == h)
            .min_by(|a, b| near_first(a, b));
        let thing = (things.iter().filter(|e| e.hour == h))
            .min_by(|a, b| unknown(a).cmp(&unknown(b)).then_with(|| near_first(a, b)));
        entries.extend(border.into_iter().chain(thing).cloned());
    }
    entries.sort_by(near_first);
    Scene {
        map: map.to_owned(),
        link,
        yaw,
        on_mesh: mesh.footing(link, FLOOR).is_some(),
        entries,
    }
}

fn near_first(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.distance.total_cmp(&b.distance)
}

fn unknown(e: &Entry) -> bool {
    e.name.as_deref().is_none_or(|n| kinds::word(n).is_none())
}

/// What's around Link in the running game.
pub fn read<M: Memory>(dev: &Dev<M>, romfs: &Romfs, game: &Path, radius: f32) -> Result<Scene> {
    let level = super::Level::read(dev, romfs, game)?;
    let (link, yaw) = (dev.link()?, dev.yaw()?);
    Ok(scene(
        &level.map,
        &level.mesh,
        &level.placed,
        link,
        yaw,
        radius,
    ))
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:>2}:00  {} {:.1} m",
            self.hour, self.what, self.distance
        )?;
        let height = self.height;
        let border = matches!(self.what.as_str(), "wall" | "drop" | "step");
        match self.what.as_str() {
            "drop" => write!(f, " to {:.1} m below", -height)?,
            "step" => write!(f, " to {height:.1} m up")?,
            _ if !border && height.abs() >= LEVEL => {
                let way = if height > 0.0 { "up" } else { "down" };
                write!(f, ", {:.1} m {way}", height.abs())?;
            }
            _ => {}
        }
        if self.behind_wall {
            write!(f, " behind a wall")?;
        }
        let name = self.name.as_deref().unwrap_or_default();
        match (border, self.hash) {
            (true, _) => {
                if let Some(label) = kinds::label(name) {
                    write!(f, " ({label})")?;
                }
            }
            (false, hash) if kinds::word(name).is_some() => {
                let hash = hash.map(|h| format!(" {h}")).unwrap_or_default();
                write!(f, " ({name}{hash})")?;
            }
            (false, Some(hash)) => write!(f, " ({hash})")?,
            (false, None) => {}
        }
        Ok(())
    }
}

impl fmt::Display for Scene {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let [x, y, z] = self.link;
        let mesh = if self.on_mesh { "on" } else { "off" };
        write!(
            f,
            "Link in {} at {x:.1},{y:.1},{z:.1}, {mesh} the mesh, facing 12:00",
            self.map
        )?;
        for e in &self.entries {
            write!(f, "\n{e}")?;
        }
        if self.entries.is_empty() {
            write!(f, "\nnothing around")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, map};
    use crate::nav::fixture::{mesh, quad};
    use crate::warp::{self, Pose};

    fn placed(name: &str, hash: u64, at: [f32; 3]) -> Placement {
        Placement {
            name: name.into(),
            hash: Some(hash),
            label: None,
            pose: Pose {
                at,
                rotate: [0.0; 3],
            },
            presence: None,
        }
    }

    /// Link at the origin facing +Z (12:00), on a 4 m square floor. +X is
    /// his 9:00 and -X his 3:00. A floor 1.7 m lower begins west of it and
    /// one 0.8 m higher east.
    fn room() -> NavMesh {
        mesh(&[
            quad(-2.0, -2.0, 2.0, 2.0, 0.0),
            quad(-5.0, -2.0, -2.3, 2.0, -1.7),
            quad(2.3, -2.0, 5.0, 2.0, 0.8),
        ])
    }

    fn entry<'a>(s: &'a Scene, hour: u8, what: &str) -> &'a Entry {
        (s.entries.iter())
            .find(|e| e.hour == hour && e.what == what)
            .unwrap_or_else(|| panic!("no {what} at {hour}:00 in {s}"))
    }

    #[test]
    fn borders_fall_in_the_hour_they_lie_and_say_what_is_past_them() {
        let placed = [placed(actor::LAND_KOKIRI_CLIFF_E_05, 1, [0.0, 0.0, 3.5])];
        let s = scene("Test", &room(), &placed, [0.0; 3], 0.0, 4.0);
        let ahead = entry(&s, 12, "wall");
        assert!((ahead.distance - 2.0).abs() < 1e-4, "{ahead:?}");
        assert_eq!(
            ahead.name.as_deref(),
            Some(&**actor::LAND_KOKIRI_CLIFF_E_05)
        );
        assert_eq!(entry(&s, 6, "wall").name, None);
        let drop = entry(&s, 3, "drop");
        assert!((drop.height + 1.7).abs() < 1e-4, "{drop:?}");
        let step = entry(&s, 9, "step");
        assert!((step.height - 0.8).abs() < 1e-4, "{step:?}");
        assert!(s.on_mesh);
    }

    #[test]
    fn a_turn_moves_every_hour_with_him() {
        let s = scene(
            "Test",
            &room(),
            &[],
            [0.0; 3],
            std::f32::consts::FRAC_PI_2,
            4.0,
        );
        // Facing +X now, so the step is dead ahead and the wall at 3:00.
        assert!((entry(&s, 12, "step").height - 0.8).abs() < 1e-4);
        entry(&s, 6, "drop");
    }

    #[test]
    fn a_step_over_the_limit_is_a_wall_and_a_level_gap_is_nothing() {
        let high = mesh(&[
            quad(-2.0, -2.0, 2.0, 2.0, 0.0),
            quad(2.3, -2.0, 5.0, 2.0, 1.5),
        ]);
        let s = scene("Test", &high, &[], [0.0; 3], 0.0, 4.0);
        assert!(s.entries.iter().all(|e| e.what == "wall"), "{s}");
        let level = mesh(&[
            quad(-2.0, -2.0, 2.0, 2.0, 0.0),
            quad(2.3, -2.0, 5.0, 2.0, 0.1),
        ]);
        let s = scene("Test", &level, &[], [0.0; 3], 0.0, 4.0);
        assert!(s.entries.iter().all(|e| e.hour != 9), "{s}");
    }

    #[test]
    fn things_are_named_and_marked_behind_a_wall() {
        let placed = [
            placed(actor::OBJ_T_BOX_A_01, 10, [-1.0, 0.0, 5.0]),
            placed(actor::OBJ_KOKIRI_DOOR_A_02, 11, [-1.5, -0.2, 0.0]),
            placed("Obj_Mystery_A", 12, [0.0, 0.0, -1.5]),
            placed(actor::LAND_KOKIRI_FIELD_PROP_A_01, 13, [1.0, 0.0, 1.0]),
        ];
        let s = scene("Test", &room(), &placed, [0.0; 3], 0.0, 6.0);
        let chest = entry(&s, 12, "chest");
        assert!(chest.behind_wall, "{chest:?}");
        assert_eq!(chest.hash, Some(10));
        assert!((chest.distance - 5.1).abs() < 0.05);
        // Inside the room, in plain sight.
        assert!(!entry(&s, 3, "door").behind_wall);
        assert_eq!(entry(&s, 6, "Obj_Mystery_A").hash, Some(12));
        // Scenery that isn't something he sees isn't listed.
        assert!(s.entries.iter().all(|e| e.hash != Some(13)));
    }

    #[test]
    fn a_thing_in_its_own_hole_is_not_behind_the_wall_around_it() {
        // A chest in a hole of the mesh: the hole's border is no wall to it.
        let holed = mesh(&[
            quad(-3.0, -3.0, 3.0, 0.5, 0.0),
            quad(-3.0, 0.5, -0.5, 3.0, 0.0),
            quad(0.5, 0.5, 3.0, 3.0, 0.0),
            quad(-0.5, 2.0, 0.5, 3.0, 0.0),
        ]);
        let placed = [placed(actor::OBJ_T_BOX_A_01, 10, [0.0, 0.0, 1.2])];
        let s = scene("Test", &holed, &placed, [0.0, 0.0, -2.0], 0.0, 6.0);
        assert!(!entry(&s, 12, "chest").behind_wall, "{s}");
    }

    #[test]
    fn the_nearest_known_thing_shows_for_an_hour() {
        let placed = [
            placed("Obj_Mystery_A", 1, [0.0, 0.0, 1.0]),
            placed(actor::OBJ_T_BOX_A_01, 2, [0.0, 0.0, 1.5]),
        ];
        let s = scene("Test", &room(), &placed, [0.0; 3], 0.0, 4.0);
        let at_12: Vec<_> = s.entries.iter().filter(|e| e.hour == 12).collect();
        assert!(at_12.iter().any(|e| e.what == "chest"), "{s}");
        assert!(at_12.iter().all(|e| e.what != "Obj_Mystery_A"), "{s}");
    }

    #[test]
    fn the_lines_read_the_way_the_spec_shows() {
        let placed = [
            placed(actor::LAND_KOKIRI_CLIFF_E_05, 1, [0.0, 0.0, 3.5]),
            placed(actor::OBJ_T_BOX_A_01, 10, [-1.0, 0.0, 5.0]),
        ];
        let s = scene("Test", &room(), &placed, [0.0, 0.0, 0.0], 0.0, 6.0);
        let text = s.to_string();
        assert!(
            text.starts_with("Link in Test at 0.0,0.0,0.0, on the mesh"),
            "{text}"
        );
        assert!(text.contains("12:00  wall 2.0 m (cliff)"), "{text}");
        assert!(text.contains(" 3:00  drop 2.0 m to 1.7 m below"), "{text}");
        assert!(text.contains(" 9:00  step 2.0 m to 0.8 m up"), "{text}");
        assert!(
            text.contains(&format!(
                "12:00  chest 5.1 m behind a wall ({} 10)",
                actor::OBJ_T_BOX_A_01
            )),
            "{text}"
        );
    }

    /// Outside Mido's house, facing the door.
    #[test]
    fn mido_s_door_shows_ahead_from_the_forest_floor() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = std::path::Path::new("../../dump/oot.nup");
        let map = map::KOKIRI_FOREST_CHILD;
        let mesh = crate::nav::load(&romfs, game, map).unwrap();
        let placed = warp::placements_in(&romfs, map).unwrap();
        let facing = std::f32::consts::PI;
        let s = scene(map, &mesh, &placed, [-51.0, 112.2, -31.0], facing, 8.0);
        assert!(s.on_mesh);
        let door = entry(&s, 12, "door");
        assert_eq!(door.hash, Some(5297260056723171008));
        assert!((door.distance - 2.8).abs() < 0.1, "{door:?}");
        assert!(s.entries.iter().any(|e| e.what == "wall"), "{s}");
    }
}
