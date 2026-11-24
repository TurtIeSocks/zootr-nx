//! One push of the stick and the verdict on it: how far it took Link, and
//! what stopped him. The `move` command's answer.

use std::fmt;
use std::path::Path;

use anyhow::Result;
use ounce_formats::Romfs;
use serde::Serialize;

use super::Level;
use super::drive::{Drive, SPEED};
use super::scene::{BorderKind, Entry, scene};
use crate::checks::flat;
use crate::dev::Dev;
use crate::live::Memory;
use crate::play::Input;

/// A height changed by more than this is a drop or a climb, in m.
pub(super) const LEVEL: f32 = 0.5;
/// `move`: a push that covers this share of the distance is free, and
/// under `SLOWED` is blocked.
const FREE: f32 = 0.8;
const SLOWED: f32 = 0.3;
/// Walls this many hours apart or more (90 degrees, so more than 60)
/// corner Link.
const CORNER: u8 = 3;
/// How near a wall is to be one of those that corner Link, in m.
const CLOSE: f32 = 1.0;
/// How far around Link to look for what a push met, in m.
const LOOK: f32 = 3.0;

/// What a push did, judged from where Link stood before and after.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "verdict", content = "by", rename_all = "snake_case")]
pub enum Contact {
    /// Moved at least 80% of the distance.
    Free,
    /// 30% to 80%: a slope, or sliding along a wall.
    Slowed,
    /// Under 30%.
    Blocked,
    /// His height fell by this many m.
    Dropped(f32),
    /// His height rose by this many m.
    Climbed(f32),
    /// Blocked, with walls close at two hours `CORNER` (90 degrees) apart
    /// or more.
    Cornered,
    /// A loading zone took Link to another map, so where he stands isn't
    /// on this map's mesh.
    LeftMap,
}

impl fmt::Display for Contact {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Contact::Free => write!(f, "free"),
            Contact::Slowed => write!(f, "slowed"),
            Contact::Blocked => write!(f, "blocked"),
            Contact::Dropped(m) => write!(f, "dropped {m:.1} m"),
            Contact::Climbed(m) => write!(f, "climbed {m:.1} m"),
            Contact::Cornered => write!(f, "cornered"),
            Contact::LeftMap => write!(f, "left map"),
        }
    }
}

/// How far apart two clock hours are, in hours.
fn apart(a: u8, b: u8) -> u8 {
    let d = a.abs_diff(b) % 12;
    d.min(12 - d)
}

/// Whether Link rose or fell, and by how much.
pub(super) fn rise(before: [f32; 3], after: [f32; 3]) -> f32 {
    after[1] - before[1]
}

/// What a push meant to cover `expected` m did. `walls` are the hours of
/// the walls close to where Link stopped.
pub fn contact(before: [f32; 3], after: [f32; 3], expected: f32, walls: &[u8]) -> Contact {
    let rise = rise(before, after);
    if rise.abs() > LEVEL {
        return if rise < 0.0 {
            Contact::Dropped(-rise)
        } else {
            Contact::Climbed(rise)
        };
    }
    let moved = flat(before, after);
    if moved >= FREE * expected {
        Contact::Free
    } else if moved >= SLOWED * expected {
        Contact::Slowed
    } else if (walls.iter()).any(|&a| walls.iter().any(|&b| apart(a, b) >= CORNER)) {
        Contact::Cornered
    } else {
        Contact::Blocked
    }
}

/// One push toward an hour: where Link stood before and after it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shove {
    pub hour: u8,
    pub secs: f32,
    /// His yaw before, which the hour is read from.
    pub yaw: f32,
    pub before: [f32; 3],
    pub after: [f32; 3],
    /// Whether the push took him through a loading zone.
    pub left: bool,
}

/// `move`'s answer: the verdict on a push, and where it left Link.
#[derive(Debug, Serialize)]
pub struct Moved {
    pub hour: u8,
    pub secs: f32,
    /// What the push should cover, in m.
    pub expected: f32,
    /// What it covered, in m.
    pub moved: f32,
    #[serde(flatten)]
    pub contact: Contact,
    pub link: [f32; 3],
    pub on_mesh: bool,
    /// For a block, what is there, nearest first.
    pub there: Option<Entry>,
}

/// The verdict on `push` in `level`.
pub fn judge(level: &Level, push: &Shove) -> Moved {
    let expected = SPEED * push.secs;
    // Hours from his facing before the push, the frame the push was in.
    let around = scene(
        &level.map,
        &level.mesh,
        &level.placed,
        push.after,
        push.yaw,
        LOOK,
    );
    let walls: Vec<u8> = (around.entries.iter())
        .filter(|e| e.border == Some(BorderKind::Wall) && e.distance <= CLOSE)
        .map(|e| e.hour)
        .collect();
    let contact = match push.left {
        true => Contact::LeftMap,
        false => contact(push.before, push.after, expected, &walls),
    };
    let blocked = matches!(contact, Contact::Blocked | Contact::Cornered);
    let there = (around.entries.into_iter()).find(|e| blocked && apart(e.hour, push.hour) <= 1);
    Moved {
        hour: push.hour,
        secs: push.secs,
        expected,
        moved: flat(push.before, push.after),
        contact,
        link: push.after,
        on_mesh: around.on_mesh && !push.left,
        there,
    }
}

impl fmt::Display for Moved {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:00 for {:.1} s: {}, moved {:.1} of {:.1} m",
            self.hour, self.secs, self.contact, self.moved, self.expected
        )?;
        let [x, y, z] = self.link;
        let mesh = if self.on_mesh { "on" } else { "off" };
        write!(f, ", now at {x:.1},{y:.1},{z:.1}, {mesh} the mesh")?;
        match &self.there {
            Some(there) => write!(f, "\nthere: {there}"),
            None => Ok(()),
        }
    }
}

/// `move`: one push toward `hour` in the running game, judged.
pub fn moved<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    romfs: &Romfs,
    game: &Path,
    hour: u8,
    secs: f32,
) -> Result<Moved> {
    let level = Level::read(dev, romfs, game)?;
    let push = Drive::new(dev, input).shove(hour, secs)?;
    Ok(judge(&level, &push))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::sim;
    use crate::warp::{Placement, Pose};

    const ORIGIN: [f32; 3] = [0.0; 3];

    #[test]
    fn each_contact_has_its_verdict() {
        let at = |x: f32, y: f32| [x, y, 0.0];
        // Expecting 5 m.
        let cases = [
            (at(4.5, 0.0), &[][..], Contact::Free),
            (at(2.0, 0.0), &[], Contact::Slowed),
            (at(1.0, 0.0), &[], Contact::Blocked),
            (at(0.0, 0.0), &[12], Contact::Blocked),
            (at(4.5, -1.25), &[], Contact::Dropped(1.25)),
            (at(0.2, 0.75), &[], Contact::Climbed(0.75)),
            // 0.5 m is level.
            (at(4.5, 0.5), &[], Contact::Free),
            // Walls 90 degrees apart, then 180, and across 12.
            (at(0.0, 0.0), &[12, 3], Contact::Cornered),
            (at(0.0, 0.0), &[9, 3], Contact::Cornered),
            (at(0.0, 0.0), &[11, 2], Contact::Cornered),
            // 60 degrees apart is not more than 60.
            (at(0.0, 0.0), &[12, 2], Contact::Blocked),
            (at(0.0, 0.0), &[11, 1], Contact::Blocked),
            // A push that got somewhere isn't cornered.
            (at(2.0, 0.0), &[12, 3], Contact::Slowed),
        ];
        for (after, walls, want) in cases {
            assert_eq!(
                contact(ORIGIN, after, 5.0, walls),
                want,
                "{after:?} {walls:?}"
            );
        }
    }

    /// A 4 m square room with a tree just past its east wall, and Link at
    /// `at` having pushed toward 12:00 while facing +X.
    fn judged(at: [f32; 3]) -> Moved {
        let level = Level::new(
            "Test",
            mesh(&[quad(0.0, 0.0, 4.0, 4.0, 0.0)]),
            vec![Placement {
                name: "Tree_FakeKokiri".into(),
                hash: Some(3),
                label: None,
                pose: Pose {
                    at: [4.5, 0.0, at[2]],
                    rotate: ORIGIN,
                    scale: [1.0; 3],
                },
                presence: None,
            }],
        );
        let push = Shove {
            hour: 12,
            secs: 1.0,
            yaw: std::f32::consts::FRAC_PI_2,
            before: [at[0] - 0.1, 0.0, at[2]],
            after: at,
            left: false,
        };
        judge(&level, &push)
    }

    #[test]
    fn a_push_through_a_loading_zone_left_the_map() {
        let (eyes, mut hands) = sim::start(ORIGIN, 0.0);
        eyes.0.borrow_mut().after_push = Some(Box::new(|w| w.map = "Other".into()));
        let push = sim::drive(&eyes, &mut hands).shove(12, 1.0).unwrap();
        let level = Level::new("Test", mesh(&[quad(0.0, 0.0, 4.0, 4.0, 0.0)]), vec![]);
        let moved = judge(&level, &push);
        assert_eq!(moved.contact, Contact::LeftMap);
        assert!(moved.there.is_none() && !moved.on_mesh);
    }

    #[test]
    fn a_blocked_push_names_what_is_there() {
        let moved = judged([3.6, 0.0, 2.0]);
        assert_eq!(moved.contact, Contact::Blocked);
        assert!(moved.on_mesh);
        let there = moved.there.clone().expect("a wall ahead");
        assert_eq!((there.what.as_str(), there.hour), ("wall", 12));
        assert_eq!(there.name.as_deref(), Some("Tree_FakeKokiri"));
        let said = moved.to_string();
        assert!(
            said.starts_with("12:00 for 1.0 s: blocked, moved 0.1 of 5.0 m"),
            "{said}"
        );
        assert!(said.ends_with("there: 12:00  wall 0.4 m (tree)"), "{said}");
    }

    #[test]
    fn a_push_into_a_corner_is_cornered() {
        let moved = judged([3.6, 0.0, 3.6]);
        assert_eq!(moved.contact, Contact::Cornered);
        assert!(moved.there.is_some());
        let json = serde_json::to_string(&moved).unwrap();
        assert!(json.contains(r#""verdict":"cornered""#), "{json}");
    }

    #[test]
    fn a_free_push_names_nothing() {
        let level = Level::new("Test", mesh(&[quad(0.0, 0.0, 20.0, 4.0, 0.0)]), vec![]);
        let push = Shove {
            hour: 12,
            secs: 1.0,
            yaw: std::f32::consts::FRAC_PI_2,
            before: [1.0, 0.0, 2.0],
            after: [6.0, 0.0, 2.0],
            left: false,
        };
        let moved = judge(&level, &push);
        assert_eq!((moved.contact, &moved.there), (Contact::Free, &None));
        let json = serde_json::to_string(&moved).unwrap();
        assert!(json.contains(r#""verdict":"free""#), "{json}");
    }

    #[test]
    fn dropped_and_climbed_say_how_far() {
        assert_eq!(Contact::Dropped(1.25).to_string(), "dropped 1.2 m");
        assert_eq!(Contact::Climbed(0.75).to_string(), "climbed 0.8 m");
        let json = serde_json::to_string(&Contact::Dropped(1.5)).unwrap();
        assert_eq!(json, r#"{"verdict":"dropped","by":1.5}"#);
    }
}
