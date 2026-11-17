//! `use`: stand where a placement's kind needs, face it, then press A or
//! walk through, and see whether anything came of it.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use ounce_formats::Romfs;

use super::Level;
use super::drive::{Drive, SLIDE, Sense, TAP_MS, lean, millis};
use super::graph::{Graph, dist};
use super::kinds::{self, Act, Blow};
use super::route;
use super::went::{Done, Reason, Went};
use crate::dev::Dev;
use crate::live::Memory;
use crate::play::Input;
use crate::warp::{self, Placement, Side};

/// How near the spot Link stands, in m.
const STAND: f32 = 0.4;
/// How far past a door Link walks, in m.
const PAST: f32 = 1.5;
/// How far past the door's line Link has to stand to have entered, in m.
const INSIDE: f32 = 0.3;
/// How near a thing Link still faces it and presses A, or how near a door's
/// spot he still walks through it, when the walk is blocked, in m. A
/// chest's own lid can stop him short of the spot with the game already
/// offering "Open" (a big chest's 1.3 m off its middle), and a door's frame
/// short of the door (0.8 m at Mido's).
const REACH: f32 = 1.5;

/// What came of using a placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Touch {
    /// A text box or a scene started.
    Event,
    /// Link stands past the door, or the door's map loads.
    Entered,
    /// Nothing started.
    Nothing,
}

said!(Touch {
    Event => "event",
    Entered => "entered",
    Nothing => "nothing",
});

/// A use: what came of it, the walk to the spot and a frame of the game
/// after it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Used {
    pub name: String,
    pub hash: Option<u64>,
    pub result: Touch,
    pub went: Went,
    /// Quarter hearts Link lost on the way and at it: something attacking,
    /// when it came to nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hurt: Option<i32>,
    /// The side of a thing to face Link faced it from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    pub shot: Option<PathBuf>,
}

impl fmt::Display for Used {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "use {}", self.name)?;
        if let Some(hash) = self.hash {
            write!(f, " {hash}")?;
        }
        write!(f, ": {}\nwalk: {}", self.result, self.went)?;
        if let Some(q) = self.hurt {
            write!(f, "\nhurt: {} hearts", q as f32 / 4.0)?;
        }
        if let Some(shot) = &self.shot {
            write!(f, "\nframe: {}", shot.display())?;
        }
        Ok(())
    }
}

/// The unit vector from `a` to `b`, measured flat.
fn toward(a: [f32; 3], b: [f32; 3]) -> [f32; 2] {
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    let n = dx.hypot(dz).max(1e-3);
    [dx / n, dz / n]
}

/// How near a spot to face from the floor at its holder's level must come,
/// in m. Mido's back chests stop 0.45 m off theirs and still open.
const FOOTED: f32 = 0.5;

/// The spot to stand at to use `placement` by `act` over `graph`, for Link
/// at `link`. A thing to face is faced from its front, unless clutter or a
/// ledge leaves no floor near that at its level (Mido's front chests), then
/// from the first other side with some. A side with floor Link can walk to
/// comes before one he can't (a gap between paired boxes).
pub fn stand(graph: &Graph, placement: &Placement, act: Act, link: [f32; 3]) -> [f32; 3] {
    stand_from(graph, placement, act, link, Side::Front).1
}

/// [`stand`], trying the `first` side before the others, with the side a
/// thing to face is faced from.
fn stand_from(
    graph: &Graph,
    placement: &Placement,
    act: Act,
    link: [f32; 3],
    first: Side,
) -> (Option<Side>, [f32; 3]) {
    let at = placement.pose.at;
    match act {
        Act::Face { stand } | Act::Break { stand, .. } => {
            let level = |p: [f32; 3]| [p[0], at[1], p[2]];
            let footed = |p: &[f32; 3]| {
                (graph.locate(level(*p))).is_some_and(|s| dist(s.at, level(*p)) <= FOOTED)
            };
            let walked = |p: &[f32; 3]| route::plan(graph, link, *p).is_ok();
            let pose = warp::facing(&placement.name, &placement.pose);
            let side = |side| (side, warp::landing_at(&pose, side, stand));
            let sides = || warp::sides_from(first).map(side);
            let found = (sides().find(|(_, p)| footed(p) && walked(p)))
                .or_else(|| sides().find(|(_, p)| footed(p)));
            let (side, spot) = found.unwrap_or_else(|| side(first));
            (Some(side), spot)
        }
        Act::Through { stand } => {
            let [ux, uz] = toward(at, link);
            (None, [at[0] + ux * stand, at[1], at[2] + uz * stand])
        }
        Act::Over => (None, at),
    }
}

/// Whether a walk got Link close enough to act: he arrived, or it stopped
/// `short` m from where he acts, at most `reach`, blocked or with no route.
/// A holder on floor the navmesh leaves out (the Well's chest on its
/// Lens of Truth crypt) has no route to it, and its straight push stops
/// short of the arrival circle, yet A opens it from 0.8 m (Mac,
/// 2026-11-16).
fn within_reach(went: &Went, short: Option<f32>, reach: f32) -> bool {
    let near = matches!(went.reason, Some(Reason::Blocked | Reason::NoRoute));
    went.result == Done::Arrived || (near && short.is_some_and(|d| d <= reach))
}

/// How `use` acts on `placement`, or why it can't.
fn act_of(placement: &Placement) -> Result<Act> {
    kinds::kind_of(&placement.name)
        .and_then(|k| k.act)
        .with_context(|| format!("{} isn't something `use` knows how to use", placement.name))
}

/// The spot `use` walks to for `placement` over `graph`, for Link at `link`.
pub fn spot(graph: &Graph, placement: &Placement, link: [f32; 3]) -> Result<[f32; 3]> {
    Ok(stand(graph, placement, act_of(placement)?, link))
}

/// The frame of using `hash`, in a folder of its own.
pub fn frame(hash: u64) -> Result<PathBuf> {
    let dir = crate::play::folder("play", &format!("use {hash}"))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("use-{hash}.png")))
}

impl<S: Sense, I: Input> Drive<'_, S, I> {
    /// Walk through the door at `door` from the spot `from` before it: a
    /// push toward a point past it, no A.
    fn through(&mut self, door: [f32; 3], from: [f32; 3]) -> Result<Touch> {
        let map = self.sense.state()?.map;
        let [ux, uz] = toward(from, door);
        let past = [door[0] + ux * PAST, door[1], door[2] + uz * PAST];
        self.steer(past, millis(lean(self.sense.link()?, past)))?;
        let state = self.sense.state()?;
        if state.event {
            return Ok(Touch::Event);
        }
        if state.loading || state.map != map {
            return Ok(Touch::Entered);
        }
        let link = self.sense.link()?;
        let inside = (link[0] - door[0]) * ux + (link[2] - door[2]) * uz;
        Ok(if inside > INSIDE {
            Touch::Entered
        } else {
            Touch::Nothing
        })
    }

    /// Break the thing at `at` by holding each of `by` in turn, Link
    /// facing it, then step onto its spot and press A to take what it
    /// dropped there. A floor rupee is taken by the step, but a stand-in
    /// lies there under "Take" until A (Mac, 2026-11-14). No ZL between
    /// the blows: it would turn him to an enemy in reach.
    fn smash(&mut self, at: [f32; 3], by: Blow) -> Result<Touch> {
        self.face(at)?;
        // The camera behind him, so a run-up's stick runs where he faces:
        // `face` leaves it where it was when he already faced `at`, and the
        // box's roll went off sideways and climbed it (Mac, 2026-11-14).
        self.input.press("ZL")?;
        std::thread::sleep(self.settle);
        for &(buttons, ms) in by {
            self.input.hold(buttons, ms)?;
        }
        std::thread::sleep(self.settle);
        self.steer(at, millis(lean(self.sense.link()?, at)))?;
        if !self.sense.state()?.event {
            self.input.press("A")?;
        }
        self.event()
    }

    /// Whether an event started within the wait.
    fn event(&self) -> Result<Touch> {
        Ok(if self.watch(|s| s.event)? {
            Touch::Event
        } else {
            Touch::Nothing
        })
    }

    /// Walk to the spot `placement` needs and use it, saving a frame of
    /// what came of it as `shot`.
    pub fn touch(&mut self, graph: &Graph, placement: &Placement, shot: PathBuf) -> Result<Used> {
        let act = act_of(placement)?;
        let hearts = self.sense.hearts()?;
        let (side, spot) = stand_from(graph, placement, act, self.sense.link()?, self.side);
        let went = match self.here {
            true => Went::here(),
            false => self.walk(graph, spot, STAND)?,
        };
        // Stopped short of reach but near: slide round what stopped him.
        let near = dist(self.sense.link()?, placement.pose.at);
        let went = match (went.reason, act) {
            (Some(Reason::Blocked | Reason::NoRoute), Act::Face { .. })
                if near > REACH && near <= SLIDE =>
            {
                self.slide(went, spot, STAND)?
            }
            _ => went,
        };
        // Blocked within reach is as good as there: a chest's lid or a
        // door's frame can stop Link short of his spot.
        let off = dist(self.sense.link()?, placement.pose.at);
        let (short, reach) = match act {
            Act::Face { .. } => (Some(off), REACH),
            // A roll's run-up starts further off than REACH.
            Act::Break { stand, .. } => (Some(off), REACH.max(stand)),
            _ => (went.left, REACH),
        };
        let reached = within_reach(&went, short, reach);
        let result = match (went.result, act) {
            (_, Act::Face { stand }) if reached => {
                match self.here {
                    // One tap: the camera the warp left behind him stays
                    // only until he turns. A talks from 30 degrees off.
                    true => self.steer(placement.pose.at, TAP_MS)?,
                    false => {
                        self.face(placement.pose.at)?;
                        self.close_in(placement.pose.at, stand)?;
                    }
                }
                self.input.press("A")?;
                self.event()?
            }
            (_, Act::Break { by, .. }) if reached => self.smash(placement.pose.at, by)?,
            (_, Act::Through { .. }) if reached => self.through(placement.pose.at, spot)?,
            (Done::Arrived, Act::Over) => self.event()?,
            // Walking onto an item starts its event before the walk is over.
            (_, Act::Over) if went.reason == Some(Reason::Event) => Touch::Event,
            _ => Touch::Nothing,
        };
        let lost = hearts - self.sense.hearts()?;
        Ok(Used {
            name: placement.name.clone(),
            hash: placement.hash,
            result,
            went,
            hurt: (lost > 0).then_some(lost),
            side,
            shot: self.input.shot(&shot).is_ok().then_some(shot),
        })
    }
}

/// `use`: walk to the spot the placement with `hash` needs in Link's map,
/// and use it.
pub fn used<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    romfs: &Romfs,
    game: &Path,
    hash: u64,
) -> Result<Used> {
    let level = Level::read(dev, romfs, game)?;
    let graph = level.graph();
    let placement = level.placement(hash)?;
    Drive::new(dev, input).touch(&graph, placement, frame(hash)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::flat;
    use crate::known::actor;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph::STEP_UP;
    use crate::nav::kinds::{ROLL, SWORD, THROW};
    use crate::nav::sim::{self, Eyes};
    use crate::warp::{Pose, Side};

    fn placed(name: &str, hash: u64, at: [f32; 3]) -> Placement {
        Placement {
            name: name.into(),
            hash: Some(hash),
            label: None,
            pose: Pose {
                at,
                rotate: [0.0; 3],
                scale: [1.0; 3],
            },
            presence: None,
        }
    }

    /// Use `thing` in a 20 by 4 m room, from Link at (2, 2) facing +Z,
    /// after `setup` has set the fake game up.
    fn touched(thing: &Placement, setup: impl FnOnce(&Eyes)) -> (Used, Eyes) {
        let room = mesh(&[quad(0.0, 0.0, 20.0, 4.0, 0.0)]);
        let graph = Graph::build(&room, &[], STEP_UP);
        let (eyes, mut hands) = sim::start([2.0, 0.0, 2.0], 0.0);
        setup(&eyes);
        let used = sim::drive(&eyes, &mut hands)
            .touch(&graph, thing, PathBuf::from("frames/use-77.png"))
            .unwrap();
        (used, eyes)
    }

    /// An A press starts an event when Link faces -Z.
    fn answers_to_the_north(eyes: &Eyes) {
        eyes.0.borrow_mut().after_press = Some(Box::new(|w| {
            w.event |= w.presses.last().is_some_and(|b| b == "A") && w.yaw.cos() < -0.9;
        }));
    }

    #[test]
    fn a_chest_fronting_clutter_is_used_from_a_side_with_floor() {
        // A chest at the origin facing +Z. Clutter 0.8 m up covers its
        // front and left, the floor at its level lies to its right.
        let room = mesh(&[
            quad(-3.0, -3.0, -0.5, 3.0, 0.0),
            quad(-0.5, 0.3, 3.0, 3.0, 0.8),
        ]);
        let graph = Graph::build(&room, &[], STEP_UP);
        let chest = placed(actor::OBJ_T_BOX_A_01, 1, [0.0; 3]);
        let face = Act::Face { stand: 0.8 };
        let at = stand(&graph, &chest, face, [0.0; 3]);
        assert_eq!(at, warp::landing_at(&chest.pose, Side::Right, 0.8));
        // One in the open is used from its front.
        let open = placed(actor::OBJ_T_BOX_A_01, 2, [-1.5, 0.0, -1.5]);
        let at = stand(&graph, &open, face, [0.0; 3]);
        assert_eq!(at, warp::landing_at(&open.pose, Side::Front, 0.8));
        // A big chest turns the other way round.
        let mut big = placed(actor::OBJ_T_BOX_A_02, 3, [-1.5, 0.0, -1.5]);
        big.pose.rotate = [0.0, 0.7, 0.0];
        let at = stand(&graph, &big, face, [0.0; 3]);
        let faced = warp::facing(&big.name, &big.pose);
        assert_eq!(at, warp::landing_at(&faced, Side::Front, 0.8));
        assert_ne!(at, warp::landing_at(&big.pose, Side::Front, 0.8));
    }

    #[test]
    fn a_box_is_rolled_into_from_a_side_link_can_walk_to() {
        // A box at the origin. Its left run-up lies on a patch of floor no
        // walk reaches, the way a pair of boxes leaves only a gap between
        // them (Death Mountain Crater, Mac, 2026-11-14).
        let stand = 1.4;
        let roll = Act::Break { stand, by: ROLL };
        let crate_ = placed(actor::OBJ_BROKEN_BOX_A_01, 1, [0.0; 3]);
        let left = warp::landing_at(&crate_.pose, Side::Left, stand);
        let [x, z] = [left[0], left[2]];
        let (x0, x1) = if x < 0.0 {
            (x + 0.8, 4.0)
        } else {
            (-4.0, x - 0.8)
        };
        let room = mesh(&[
            quad(x - 0.4, z - 0.4, x + 0.4, z + 0.4, 0.0),
            quad(x0, -4.0, x1, 4.0, 0.0),
        ]);
        let graph = Graph::build(&room, &[], STEP_UP);
        let (side, at) = stand_from(&graph, &crate_, roll, [0.0, 0.0, 3.0], Side::Left);
        assert_ne!(side, Some(Side::Left), "{at:?}");
        assert!(route::plan(&graph, [0.0, 0.0, 3.0], at).is_ok(), "{at:?}");
    }

    #[test]
    fn a_use_says_how_much_link_was_hurt_on_the_way() {
        needs_names!();
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        // Something bites him a quarter heart each push, and A opens nothing.
        let (used, _) = touched(&chest, |eyes| {
            eyes.0.borrow_mut().after_push = Some(Box::new(|w| w.hearts -= 1));
        });
        assert_eq!(used.result, Touch::Nothing);
        assert!(used.hurt.is_some_and(|q| q > 0), "{used:?}");
        assert!(used.to_string().contains("hurt: "), "{used}");
        let (used, _) = touched(&chest, answers_to_the_north);
        assert_eq!(used.hurt, None);
    }

    #[test]
    fn a_keeper_across_a_counter_is_turned_to_with_one_tap_and_no_zl() {
        needs_names!();
        // Warped onto the customer spot, facing away from the keeper.
        let keeper = placed(actor::NPC_GORON_011_SHOP_CHILD, 77, [2.0, 0.0, 0.0]);
        let room = mesh(&[quad(0.0, 0.0, 20.0, 4.0, 0.0)]);
        let graph = Graph::build(&room, &[], STEP_UP);
        let (eyes, mut hands) = sim::start([2.0, 0.0, 2.0], 0.0);
        answers_to_the_north(&eyes);
        eyes.0.borrow_mut().warped();
        let mut drive = sim::drive(&eyes, &mut hands);
        drive.here = true;
        let used = drive
            .touch(&graph, &keeper, PathBuf::from("use.png"))
            .unwrap();
        assert_eq!(used.result, Touch::Event);
        let world = eyes.0.borrow();
        assert_eq!(world.pushes.len(), 1, "{:?}", world.pushes);
        assert!(
            !world.presses.iter().any(|b| b == "ZL"),
            "{:?}",
            world.presses
        );
        assert!((world.at[2] - 2.0).abs() < 0.1, "{:?}", world.at);
    }

    #[test]
    fn a_chest_is_faced_from_in_front_then_opened_with_a() {
        needs_names!();
        // It faces +Z, so its front is where Link has to stand.
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&chest, answers_to_the_north);
        assert_eq!(used.result, Touch::Event);
        assert_eq!(used.went.result, Done::Arrived);
        let world = eyes.0.borrow();
        assert!((world.at[0] - 10.0).abs() < 0.5 && (world.at[2] - 2.8).abs() < 0.5);
        assert_eq!(world.presses.last().map(String::as_str), Some("A"));
        // ZL goes before the walk and each turn, and A isn't pressed twice.
        assert_eq!(world.presses.iter().filter(|b| *b == "A").count(), 1);
        assert_eq!(used.shot.as_deref(), Some(Path::new("frames/use-77.png")));
        assert_eq!(world.shots, [PathBuf::from("frames/use-77.png")]);
    }

    /// Using `name` at (10, 0, 2) holds every one of `by`, then leaves
    /// Link on its spot and presses A there. The buttons pressed.
    fn broken_by(name: &str, by: Blow) -> Vec<String> {
        let thing = placed(name, 77, [10.0, 0.0, 2.0]);
        let (_, eyes) = touched(&thing, |_| {});
        let world = eyes.0.borrow();
        let presses = &world.presses;
        for (buttons, _) in by {
            assert!(presses.iter().any(|b| b == buttons), "{presses:?}");
        }
        let off = (world.at[0] - 10.0).hypot(world.at[2] - 2.0);
        assert!(off < 0.5, "{:?} is {off} m off its spot", world.at);
        assert_eq!(presses.last().map(String::as_str), Some("A"), "{presses:?}");
        presses.clone()
    }

    #[test]
    fn a_pot_is_struck_with_the_sword_and_its_drop_taken_on_its_spot() {
        needs_names!();
        let presses = broken_by(actor::OBJ_POT_A_01, SWORD);
        // Only the take: an A before the blow would lift it.
        assert_eq!(presses.iter().filter(|b| *b == "A").count(), 1);
    }

    #[test]
    fn a_rock_is_lifted_and_thrown_and_its_spot_stepped_on() {
        needs_names!();
        let presses = broken_by(actor::OBJ_LIFT_ROCK_A_01, THROW);
        assert_eq!(presses.iter().filter(|b| *b == "A").count(), 3);
    }

    #[test]
    fn a_box_is_rolled_into_and_its_spot_stepped_on() {
        needs_names!();
        broken_by(actor::OBJ_BROKEN_BOX_A_01, ROLL);
    }

    #[test]
    fn a_chest_that_answers_nothing_is_nothing_with_a_frame() {
        needs_names!();
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&chest, |_| {});
        assert_eq!(used.result, Touch::Nothing);
        assert_eq!(eyes.0.borrow().shots.len(), 1);
    }

    #[test]
    fn a_door_is_walked_through_without_a() {
        needs_names!();
        let door = placed(actor::OBJ_KOKIRI_DOOR_A_02, 5, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&door, |_| {});
        assert_eq!(used.result, Touch::Entered);
        let world = eyes.0.borrow();
        assert!(world.at[0] > 10.3, "{:?}", world.at);
        assert!(
            world.presses.iter().all(|b| b == "ZL"),
            "{:?}",
            world.presses
        );
    }

    #[test]
    fn a_door_that_stays_shut_is_nothing() {
        needs_names!();
        let door = placed(actor::OBJ_KOKIRI_DOOR_A_02, 5, [10.0, 0.0, 2.0]);
        let (used, _) = touched(&door, |e| {
            e.0.borrow_mut().walls = vec![[9.9, 0.0, 10.1, 4.0]]
        });
        assert_eq!(used.result, Touch::Nothing);
    }

    #[test]
    fn a_door_that_loads_a_map_is_entered() {
        needs_names!();
        let door = placed(actor::OBJ_KOKIRI_DOOR_A_02, 5, [10.0, 0.0, 2.0]);
        let (used, _) = touched(&door, |e| {
            e.0.borrow_mut().after_push = Some(Box::new(|w| {
                if w.at[0] > 9.9 {
                    w.map = "Inside".into();
                }
            }));
        });
        assert_eq!(used.result, Touch::Entered);
    }

    #[test]
    fn an_item_is_walked_over_and_its_event_counts() {
        let item = placed("Item_G", 9, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&item, |e| {
            e.0.borrow_mut().after_push = Some(Box::new(|w| {
                w.event |= (w.at[0] - 10.0).abs() < 0.5;
            }));
        });
        assert_eq!(used.result, Touch::Event);
        assert!(eyes.0.borrow().presses.iter().all(|b| b == "ZL"));
    }

    #[test]
    fn a_walk_that_gives_up_presses_nothing_more() {
        needs_names!();
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&chest, |e| {
            e.0.borrow_mut().walls = vec![[5.0, 0.0, 5.2, 4.0]];
            answers_to_the_north(e);
        });
        assert_eq!(used.result, Touch::Nothing);
        assert_eq!(used.went.reason, Some(Reason::Blocked));
        assert!(eyes.0.borrow().presses.iter().all(|b| b == "ZL"));
        assert!(used.shot.is_some());
    }

    #[test]
    fn a_walk_blocked_within_reach_still_faces_and_presses_a() {
        needs_names!();
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&chest, |e| {
            e.0.borrow_mut().walls = vec![[9.2, 0.0, 9.4, 4.0]];
        });
        assert_eq!(used.went.reason, Some(Reason::Blocked));
        assert_eq!(
            eyes.0.borrow().presses.last().map(String::as_str),
            Some("A")
        );
    }

    #[test]
    fn a_walk_stopped_near_slides_round_and_presses_a() {
        needs_names!();
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&chest, |e| {
            e.0.borrow_mut().walls = vec![[8.0, 0.0, 8.2, 2.8]];
            answers_to_the_north(e);
        });
        let pushes = eyes.0.borrow().pushes.len();
        assert_eq!(used.result, Touch::Event, "{used:?} after {pushes} pushes");
        assert_eq!(
            eyes.0.borrow().presses.last().map(String::as_str),
            Some("A")
        );
    }

    #[test]
    fn a_walk_that_stops_out_of_range_steps_in_before_a() {
        // The Well's key chest shows "Open" only within 0.9 m, and the
        // walk's arrival let Link stop 1.0 m off (Mac, 2026-11-16).
        needs_names!();
        let chest = placed(actor::OBJ_T_BOX_A_01, 77, [10.0, 0.0, 2.0]);
        let (used, eyes) = touched(&chest, |e| {
            // The walk's first push past 1.0 m stops there, short of the
            // 0.8 m spot but within arrival of it.
            let mut short = true;
            e.0.borrow_mut().after_push = Some(Box::new(move |w| {
                let off = flat(w.at, [10.0, 0.0, 2.0]);
                if short && off < 1.0 {
                    let k = 1.0 / off;
                    w.at = [10.0 + (w.at[0] - 10.0) * k, 0.0, 2.0 + (w.at[2] - 2.0) * k];
                    short = false;
                }
            }));
            e.0.borrow_mut().after_press = Some(Box::new(|w| {
                let near = flat(w.at, [10.0, 0.0, 2.0]) <= 0.9;
                w.event |= near && w.presses.last().is_some_and(|b| b == "A");
            }));
        });
        let at = eyes.0.borrow().at;
        assert_eq!(used.result, Touch::Event, "Link at {at:?}");
    }

    #[test]
    fn blocked_or_no_route_within_reach_counts_as_there() {
        let went = |result, reason| Went {
            result,
            reason,
            secs: 1.0,
            legs: 2,
            left: Some(0.8),
        };
        let blocked = went(Done::GaveUp, Some(Reason::Blocked));
        assert!(within_reach(&blocked, Some(0.8), REACH));
        assert!(!within_reach(&blocked, Some(REACH + 0.1), REACH));
        assert!(!within_reach(&blocked, None, REACH));
        let no_route = went(Done::GaveUp, Some(Reason::NoRoute));
        assert!(within_reach(&no_route, Some(0.8), REACH));
        assert!(!within_reach(&no_route, Some(REACH + 0.1), REACH));
        let budget = went(Done::GaveUp, Some(Reason::Budget));
        assert!(!within_reach(&budget, Some(0.1), REACH));
        assert!(within_reach(&went(Done::Arrived, None), None, REACH));
    }

    #[test]
    fn a_tree_has_no_use() {
        let tree = placed("Tree_FakeKokiri", 1, [10.0, 0.0, 2.0]);
        let room = mesh(&[quad(0.0, 0.0, 20.0, 4.0, 0.0)]);
        let graph = Graph::build(&room, &[], STEP_UP);
        let (eyes, mut hands) = sim::start([2.0, 0.0, 2.0], 0.0);
        let err = (sim::drive(&eyes, &mut hands))
            .touch(&graph, &tree, PathBuf::from("frames/use-tree.png"))
            .unwrap_err();
        assert!(err.to_string().contains("isn't something"), "{err}");
    }

    #[test]
    fn each_kind_stands_where_the_table_says() {
        needs_names!();
        let at = [10.0, 0.0, 2.0];
        let near = |a: [f32; 3], b: [f32; 2]| (a[0] - b[0]).hypot(a[2] - b[1]) < 1e-4;
        let chest = placed(actor::OBJ_T_BOX_A_01, 1, at);
        let act = |p: &Placement| kinds::kind_of(&p.name).and_then(|k| k.act).unwrap();
        // With no floor anywhere, a chest keeps its front.
        let bare = mesh(&[]);
        let graph = Graph::build(&bare, &[], STEP_UP);
        // 0.8 m in front by its own facing, whichever side Link is on.
        assert!(near(
            stand(&graph, &chest, act(&chest), [0.0; 3]),
            [10.0, 2.8]
        ));
        assert!(near(
            stand(&graph, &chest, act(&chest), [30.0, 0.0, 9.0]),
            [10.0, 2.8]
        ));
        // 1 m before a door, on Link's side.
        let door = placed(actor::OBJ_KOKIRI_DOOR_A_02, 2, at);
        assert!(near(
            stand(&graph, &door, act(&door), [2.0, 0.0, 2.0]),
            [9.0, 2.0]
        ));
        assert!(near(
            stand(&graph, &door, act(&door), [30.0, 0.0, 2.0]),
            [11.0, 2.0]
        ));
        // On an item.
        let item = placed("Item_G", 3, at);
        assert!(near(
            stand(&graph, &item, act(&item), [0.0; 3]),
            [10.0, 2.0]
        ));
    }
}
