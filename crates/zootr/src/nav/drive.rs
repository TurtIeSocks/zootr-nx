//! The loop that walks a route through `Input`: each leg turns toward the
//! next waypoint and pushes the stick, then judges what came of it.

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;
use ounce_formats::Romfs;

use super::act;
use super::graph::{Graph, Via};
use super::route::{Route, Waypoint, plan};
use super::shove::{LEVEL, Shove, rise};
use super::went::{Done, Gone, Reason, Went};
use super::{CLOCKWISE, Level, heading};
use crate::around::clockwise;
use crate::checks::flat;
use crate::dev::{Dev, State};
use crate::live::Memory;
use crate::play::Input;
use crate::warp::{self, Side};

/// How fast Link walks with the stick all the way, in m/s. To tune live:
/// every push is sized by it.
pub const SPEED: f32 = 5.0;
/// The longest a leg pushes, in s: the route is read again after it.
const MAX_LEG: f32 = 1.5;
/// How near a waypoint on the way counts as reached, in m.
const REACHED: f32 = 0.5;
/// How near in height the top or foot of a climb is to be reached, in m:
/// its top stands almost over its foot, so flat it's reached at once.
const CLIMBED: f32 = 1.0;
/// How fast Link climbs, in m a second: up held 2 s took him 2.8 m up the
/// Kokiri Forest vines and over the top (Mac, 2026-11-18).
const CLIMB_SPEED: f32 = 1.4;
/// How near the floor at its foot a climb down ends, in m: on the Well
/// ladder's last rung Link stood 0.4 m up it (Mac, 2026-11-18).
const OFF_CLIMB: f32 = 0.2;
/// How near the goal `go` stops, in m.
pub const ARRIVE: f32 = 1.0;
/// How near a goal with no route Link still walks straight at, in m. The
/// mesh leaves out ground he can stand on: Gerudo Valley's red rupee
/// chest stands on rocks 2 m past its edge, and every walk to it gave up
/// there (22 sweeps).
const STRAIGHT: f32 = 3.0;
/// How near a holder a use whose walk stopped short of reach still slides
/// round what stopped it, in m.
pub const SLIDE: f32 = 3.0;
/// How far each slide steps aside, in m.
const ASIDE: f32 = 1.0;
/// How long a walk may take by default.
pub const BUDGET: Duration = Duration::from_secs(60);
/// How long the game gets to settle after ZL, and after a push, so the
/// camera is behind Link and he has stopped.
const SETTLE: Duration = Duration::from_millis(500);
/// How long a thing may take to start after Link touches it.
const WAIT: Duration = Duration::from_secs(2);
/// How often the game is looked at while waiting.
const TICK: Duration = Duration::from_millis(100);
/// A stick tap that turns Link without walking him off his spot, in ms.
pub(super) const TAP_MS: u64 = 80;
/// How far from the way he's asked to face Link may be, in degrees.
const FACING: f32 = 20.0;
/// A push that moves Link less than this moved him nowhere, in m.
const STILL: f32 = 0.05;
/// How many taps Link gets to turn.
const TURNS: usize = 3;
/// `go`: a leg that covers under this share of the distance is a block.
const STUCK: f32 = 0.5;
/// What `drive` reads of the game: where Link stands and faces, and the
/// map's state.
pub trait Sense {
    /// Where he stands.
    fn link(&self) -> Result<[f32; 3]>;
    /// Which way he faces: `(sin, 0, cos)` of it.
    fn yaw(&self) -> Result<f32>;
    /// The map, and whether it is loading or an event plays.
    fn state(&self) -> Result<State>;
    /// His hearts, in quarters.
    fn hearts(&self) -> Result<i32>;
}

impl<M: Memory> Sense for Dev<M> {
    fn link(&self) -> Result<[f32; 3]> {
        Dev::link(self)
    }

    fn yaw(&self) -> Result<f32> {
        Dev::yaw(self)
    }

    fn state(&self) -> Result<State> {
        Dev::state(self)
    }

    fn hearts(&self) -> Result<i32> {
        Dev::hearts(self)
    }
}

/// Whether a leg that meant to cover `expected` m was stopped: it moved
/// under half of it, and neither dropped nor climbed.
fn stuck(before: [f32; 3], after: [f32; 3], expected: f32) -> bool {
    flat(before, after) < STUCK * expected && rise(before, after).abs() <= LEVEL
}

/// Link, the stick and the game's state, for walking him around.
pub struct Drive<'a, S, I> {
    pub sense: &'a S,
    pub input: &'a mut I,
    /// How long the game gets to settle after ZL and after a push.
    pub settle: Duration,
    /// How long a thing may take to start after Link touches it.
    pub wait: Duration,
    /// How long a walk may take.
    pub budget: Duration,
    /// The side of a thing to face it from first.
    pub side: Side,
    /// The sides to try after `side`, best first: the plan's ranking.
    pub then: Vec<Side>,
    /// Link already stands where he uses the next thing he touches: a
    /// counter's customer spot, off the navmesh. He doesn't walk, and he
    /// turns without ZL: ZL locks on to the keeper across the counter, and
    /// a tap under lock-on stepped Link 2.6 m back, out of reach (Mac,
    /// 2026-11-14). A tap alone turns him, the counter stops the step.
    pub here: bool,
}

/// The point `far` m from `here`, `angle` radians round from the way to
/// `goal`.
fn beside(here: [f32; 3], goal: [f32; 3], angle: f32, far: f32) -> [f32; 3] {
    let (dx, dz) = (goal[0] - here[0], goal[2] - here[2]);
    let len = dx.hypot(dz).max(f32::EPSILON);
    let (sin, cos) = angle.sin_cos();
    [
        here[0] + (dx * cos - dz * sin) / len * far,
        here[1],
        here[2] + (dx * sin + dz * cos) / len * far,
    ]
}

/// How long a push walks Link from `here` toward `to`, at most `MAX_LEG`.
pub(super) fn lean(here: [f32; 3], to: [f32; 3]) -> f32 {
    (flat(here, to) / SPEED).min(MAX_LEG)
}

/// [`lean`] toward a waypoint reached `via`: up or down a climb, as long
/// as its height takes at climbing speed.
fn lean_via(here: [f32; 3], to: [f32; 3], via: Via) -> f32 {
    match via {
        Via::Climb(_) => ((to[1] - here[1]).abs() / CLIMB_SPEED).clamp(lean(here, to), MAX_LEG),
        _ => lean(here, to),
    }
}

/// Milliseconds in `secs`.
pub(super) fn millis(secs: f32) -> u64 {
    (secs * 1000.0).round() as u64
}

impl<'a, S: Sense, I: Input> Drive<'a, S, I> {
    pub fn new(sense: &'a S, input: &'a mut I) -> Self {
        Self {
            sense,
            input,
            settle: SETTLE,
            wait: WAIT,
            budget: BUDGET,
            side: Side::Front,
            then: vec![],
            here: false,
        }
    }

    /// ZL first, so the camera is behind Link and the stick's direction is
    /// his own, then the stick toward `direction` for `ms`, then a wait
    /// for him to stop.
    pub(super) fn push(&mut self, direction: &str, ms: u64) -> Result<()> {
        self.input.press("ZL")?;
        sleep(self.settle);
        self.input.stick(direction, ms)?;
        sleep(self.settle);
        Ok(())
    }

    /// [`Self::push`] toward `to` for `ms`. ZL turns Link to an enemy in
    /// reach, so his heading is read after it: read before, a Skulltula
    /// in adult Kokiri Forest sent the push and the sword away from a pot
    /// (Mac, 2026-11-14).
    pub(super) fn steer(&mut self, to: [f32; 3], ms: u64) -> Result<()> {
        if !self.here {
            self.input.press("ZL")?;
            sleep(self.settle);
        }
        let direction = heading(self.sense.yaw()?, self.sense.link()?, to);
        self.input.stick(direction, ms)?;
        sleep(self.settle);
        Ok(())
    }

    /// Whether Link faces `at` within 20 degrees.
    fn facing(&self, at: [f32; 3]) -> Result<bool> {
        let turned = clockwise(self.sense.yaw()?, self.sense.link()?, at);
        Ok(turned.abs() <= FACING.to_radians())
    }

    /// Turn Link toward `at`: ZL and a tap, again until he faces it within
    /// 20 degrees, for at most 3 taps. Whether he does.
    pub fn face(&mut self, at: [f32; 3]) -> Result<bool> {
        for _ in 0..TURNS {
            if self.facing(at)? {
                return Ok(true);
            }
            self.steer(at, TAP_MS)?;
        }
        self.facing(at)
    }

    /// Push Link in toward `at` until he stands within `stand` of it, at
    /// most 3 short pushes, stopping at one that moves him nowhere. A walk
    /// counts as arrived 0.4 m from its spot, and the Well's key chest
    /// shows "Open" only within 0.9 m: Link stopped 1.0 m off and A did
    /// nothing (other machine and Mac, 2026-11-16).
    pub(super) fn close_in(&mut self, at: [f32; 3], stand: f32) -> Result<()> {
        for _ in 0..TURNS {
            let here = self.sense.link()?;
            let off = flat(here, at);
            if off <= stand {
                break;
            }
            // A tap only turns him: the distance walks on after it.
            self.steer(at, TAP_MS + millis((off - stand) / SPEED))?;
            if flat(self.sense.link()?, here) < STILL {
                break;
            }
        }
        Ok(())
    }

    /// Wait up to `wait` for the game's state to answer `yes`.
    pub(super) fn watch(&self, yes: impl Fn(&State) -> bool) -> Result<bool> {
        let start = Instant::now();
        loop {
            if yes(&self.sense.state()?) {
                return Ok(true);
            }
            if start.elapsed() >= self.wait {
                return Ok(false);
            }
            sleep(TICK);
        }
    }

    /// One push toward a clock hour from Link's facing, for `secs`.
    pub fn shove(&mut self, hour: u8, secs: f32) -> Result<Shove> {
        // Twelve hours onto the stick's eight directions.
        let eighths = (hour as usize % 12 * 2 + 1) / 3;
        let (yaw, before) = (self.sense.yaw()?, self.sense.link()?);
        let map = self.sense.state()?.map;
        self.push(CLOCKWISE[eighths], millis(secs))?;
        let state = self.sense.state()?;
        Ok(Shove {
            hour,
            secs,
            yaw,
            before,
            after: self.sense.link()?,
            left: state.loading || state.map != map,
        })
    }

    /// Walk to `goal` over `graph`, stopping within `arrive` m of it. Gives
    /// up with a reason, never silently.
    pub fn walk(&mut self, graph: &Graph, goal: [f32; 3], arrive: f32) -> Result<Went> {
        let start = Instant::now();
        let (reason, legs) = self.legs(graph, goal, arrive, start)?;
        let left = match reason {
            Some(Reason::LeftMap) => None,
            _ => Some(flat(self.sense.link()?, goal)),
        };
        Ok(Went {
            result: if reason.is_some() {
                Done::GaveUp
            } else {
                Done::Arrived
            },
            reason,
            secs: start.elapsed().as_secs_f32(),
            legs,
            left,
        })
    }

    /// A walk to `goal` that stopped short: a step aside 45 degrees off
    /// the way to it, each way, then 90, each followed by a push straight
    /// at it, until Link is within `arrive`. The Well's key chest stands
    /// behind a Lens of Truth wall that stopped the walk 2 m off, and a
    /// step aside got round it (Mac, 2026-11-16). A drop ends it: the step
    /// went off a ledge.
    pub fn slide(&mut self, went: Went, goal: [f32; 3], arrive: f32) -> Result<Went> {
        use std::f32::consts::FRAC_PI_4;
        let start = Instant::now();
        let (mut reason, mut legs) = (went.reason, went.legs);
        for eighths in [1.0, -1.0, 2.0, -2.0] {
            let here = self.sense.link()?;
            self.steer(
                beside(here, goal, eighths * FRAC_PI_4, ASIDE),
                millis(ASIDE / SPEED),
            )?;
            let stepped = self.sense.link()?;
            legs += 1;
            if rise(here, stepped) < -LEVEL {
                break;
            }
            self.steer(goal, millis(lean(stepped, goal)))?;
            legs += 1;
            let now = self.sense.link()?;
            if self.sense.state()?.event {
                reason = Some(Reason::Event);
                break;
            }
            if flat(now, goal) <= arrive {
                reason = None;
                break;
            }
            if rise(stepped, now) < -LEVEL {
                break;
            }
        }
        Ok(Went {
            result: match reason {
                None => Done::Arrived,
                Some(_) => Done::GaveUp,
            },
            reason,
            secs: went.secs + start.elapsed().as_secs_f32(),
            legs,
            left: Some(flat(self.sense.link()?, goal)),
        })
    }

    /// Onto the climb down to `foot` from its top: facing it at its edge,
    /// the game offers "A Climb Down". At the Well ladder it did 0.8 m
    /// from the top's waypoint, not 0.9 m, and Link stops at the edge
    /// (Mac, 2026-11-18), so he steps up to it first.
    fn take_hold(&mut self, foot: [f32; 3]) -> Result<()> {
        self.face(foot)?;
        self.steer(foot, TAP_MS)?;
        self.input.press("A")?;
        sleep(self.settle);
        Ok(())
    }

    /// No route to `goal`: within [`STRAIGHT`] of it, one push straight at
    /// it, else no route. Short of it after the push, he was blocked if he
    /// stopped, as by the chest he walks to.
    fn straight(
        &mut self,
        goal: [f32; 3],
        arrive: f32,
        legs: u32,
    ) -> Result<(Option<Reason>, u32)> {
        let here = self.sense.link()?;
        if flat(here, goal) > STRAIGHT {
            return Ok((Some(Reason::NoRoute), legs));
        }
        let secs = lean(here, goal);
        self.steer(goal, millis(secs))?;
        let now = self.sense.link()?;
        let reason = match () {
            _ if self.sense.state()?.event => Some(Reason::Event),
            _ if flat(now, goal) <= arrive => None,
            _ if stuck(here, now, secs * SPEED) => Some(Reason::Blocked),
            _ => Some(Reason::NoRoute),
        };
        Ok((reason, legs + 1))
    }

    /// The legs of a walk, and the reason it stopped short, if it did. A
    /// block plans again from where Link is, and a second one at the same
    /// waypoint gives up.
    fn legs(
        &mut self,
        graph: &Graph,
        goal: [f32; 3],
        arrive: f32,
        start: Instant,
    ) -> Result<(Option<Reason>, u32)> {
        let map = self.sense.state()?.map;
        let link = self.sense.link()?;
        // Before the plan: a goal beside a hole in the mesh (a chest) is
        // reached with no route to it.
        if flat(link, goal) <= arrive {
            return Ok((None, 0));
        }
        let Ok(mut route) = plan(graph, link, goal) else {
            return self.straight(goal, arrive, 0);
        };
        let (mut next, mut legs, mut blocked) = (1, 0, None::<[f32; 3]>);
        // The waypoint whose climb down Link last took hold of, and the one
        // whose hold missed, so he drops to its foot instead.
        let (mut held, mut missed) = (None, None);
        loop {
            let state = self.sense.state()?;
            if state.loading || state.map != map {
                return Ok((Some(Reason::LeftMap), legs));
            }
            if state.event {
                return Ok((Some(Reason::Event), legs));
            }
            let here = self.sense.link()?;
            let Route { waypoints, .. } = &route;
            let Some(&Waypoint { at: target, via }) = waypoints.get(next) else {
                return Ok((None, legs));
            };
            if flat(here, goal) <= arrive {
                return Ok((None, legs));
            }
            let last = next + 1 == waypoints.len();
            let climb = |w: Option<&Waypoint>| w.is_some_and(|w| matches!(w.via, Via::Climb(_)));
            let on = if held == Some(next) {
                OFF_CLIMB
            } else {
                CLIMBED
            };
            let level = !matches!(via, Via::Climb(_)) || (here[1] - target[1]).abs() <= on;
            // Either end of a climb is reached within CLIMBED: Link stopped
            // 0.4 m short of the Well ladder's top, at its edge (Mac,
            // 2026-11-18).
            let reach = if last {
                arrive
            } else if climb(waypoints.get(next)) || climb(waypoints.get(next + 1)) {
                CLIMBED
            } else {
                REACHED
            };
            if level && flat(here, target) <= reach {
                next += 1;
                continue;
            }
            if start.elapsed() >= self.budget {
                return Ok((Some(Reason::Budget), legs));
            }
            let below = here[1] - target[1] > on;
            if matches!(via, Via::Climb(_)) && below && held != Some(next) {
                self.take_hold(target)?;
                held = Some(next);
                continue;
            }
            let secs = lean_via(here, target, via);
            let down = held == Some(next) && below && missed != Some(next);
            if down {
                // On the climb, down climbs down: steered at the foot, Link
                // hung on the Well ladder until the budget ran out (Mac,
                // 2026-11-18).
                self.push("LDown", millis(secs))?;
            } else {
                self.steer(target, millis(secs))?;
            }
            legs += 1;
            let now = self.sense.link()?;
            if down && here[1] - now[1] < OFF_CLIMB {
                // No "A Climb Down" there: the Kokiri Forest vines run up
                // beside their ledge, 3.2 m over the floor at their foot
                // (Mac, 2026-11-18).
                missed = Some(next);
            }
            if !stuck(here, now, secs * SPEED) {
                continue;
            }
            if blocked.is_some_and(|b| flat(b, target) <= REACHED) {
                return Ok((Some(Reason::Blocked), legs));
            }
            blocked = Some(target);
            let Ok(again) = plan(graph, now, goal) else {
                return self.straight(goal, arrive, legs);
            };
            (route, next, held, missed) = (again, 1, None, None);
        }
    }
}

/// How `go` goes.
pub struct How {
    /// Then `use` the placement, once there.
    pub then_use: bool,
    /// Warp instead of giving up when there is no route.
    pub warp: bool,
    pub budget: Duration,
}

/// Walk Link to the placement with `hash` in his map, and then `use` it
/// when asked.
pub fn go<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    romfs: &Romfs,
    game: &Path,
    hash: u64,
    how: &How,
) -> Result<Gone> {
    let level = Level::read(dev, romfs, game)?;
    let placement = level.placement(hash)?;
    let graph = level.graph();
    // To use it, the walk ends at the spot its kind needs, which for a door
    // is on Link's side.
    let goal = match how.then_use {
        true => act::spot(&graph, placement, dev.link()?)?,
        false => placement.pose.at,
    };
    let mut drive = Drive::new(dev, input);
    drive.budget = how.budget;
    let mut went = drive.walk(&graph, goal, ARRIVE)?;
    if how.warp && went.reason == Some(Reason::NoRoute) {
        let start = Instant::now();
        warp::beside(dev, romfs, &hash.to_string(), Side::Front)?;
        went = Went {
            result: Done::Warped,
            secs: went.secs + start.elapsed().as_secs_f32(),
            left: Some(flat(dev.link()?, goal)),
            ..went
        };
    }
    // A block can leave Link already in reach, which `use` judges itself.
    let near =
        matches!(went.result, Done::Arrived | Done::Warped) || went.reason == Some(Reason::Blocked);
    let used = match how.then_use && near {
        true => Some(drive.touch(&graph, placement, act::frame(hash)?)?),
        false => None,
    };
    Ok(Gone { went, used })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::fixture::{mesh, quad};
    use crate::nav::graph::STEP_UP;
    use crate::nav::sim;

    const ORIGIN: [f32; 3] = [0.0; 3];

    #[test]
    fn a_leg_is_stuck_under_half_the_distance_unless_his_height_changed() {
        let to = |x: f32, y: f32| [x, y, 0.0];
        assert!(stuck(ORIGIN, to(2.0, 0.0), 5.0));
        assert!(!stuck(ORIGIN, to(3.0, 0.0), 5.0));
        assert!(!stuck(ORIGIN, to(0.5, 1.0), 5.0));
    }

    #[test]
    fn a_push_up_a_climb_lasts_as_long_as_its_height_takes() {
        let (foot, top) = ([0.0, 0.0, 0.0], [0.5, 1.4, 0.0]);
        assert_eq!(lean_via(foot, top, Via::Walk), 0.1);
        assert!((lean_via(foot, top, Via::Climb(None)) - 1.0).abs() < 1e-6);
        assert_eq!(lean_via(foot, [0.5, 9.0, 0.0], Via::Climb(None)), MAX_LEG);
    }

    #[test]
    fn a_shove_pushes_the_stick_toward_the_hour() {
        let hours = [
            (12, "up"),
            (1, "up-right"),
            (2, "up-right"),
            (3, "right"),
            (4, "down-right"),
            (5, "down-right"),
            (6, "down"),
            (7, "down-left"),
            (8, "down-left"),
            (9, "left"),
            (10, "up-left"),
            (11, "up-left"),
        ];
        for (hour, direction) in hours {
            let (eyes, mut hands) = sim::start([2.0, 0.0, 2.0], 0.0);
            let push = sim::drive(&eyes, &mut hands).shove(hour, 0.5).unwrap();
            let world = eyes.0.borrow();
            assert_eq!(world.pushes, [(direction.to_string(), 500)], "{hour}:00");
            assert_eq!(world.presses, ["ZL"]);
            assert_eq!(
                (push.hour, push.before, push.after),
                (hour, [2.0, 0.0, 2.0], world.at)
            );
        }
    }

    /// A 20 by 4 m room, and Link at (1, 2) facing +X.
    fn walker() -> (sim::Eyes, sim::Hands, ounce_formats::NavMesh) {
        let (eyes, hands) = sim::start([1.0, 0.0, 2.0], std::f32::consts::FRAC_PI_2);
        (eyes, hands, mesh(&[quad(0.0, 0.0, 20.0, 4.0, 0.0)]))
    }

    const GOAL: [f32; 3] = [15.0, 0.0, 2.0];

    fn walked(setup: impl FnOnce(&mut sim::World)) -> (Went, sim::Eyes) {
        let (eyes, mut hands, room) = walker();
        setup(&mut eyes.0.borrow_mut());
        let graph = Graph::build(&room, &[], STEP_UP);
        let went = sim::drive(&eyes, &mut hands)
            .walk(&graph, GOAL, ARRIVE)
            .unwrap();
        (went, eyes)
    }

    #[test]
    fn a_walk_turns_with_zl_first_and_stops_near_the_goal() {
        // Facing +Z, the goal is to his left.
        let (eyes, mut hands, room) = walker();
        eyes.0.borrow_mut().yaw = 0.0;
        let graph = Graph::build(&room, &[], STEP_UP);
        let went = sim::drive(&eyes, &mut hands)
            .walk(&graph, GOAL, ARRIVE)
            .unwrap();
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::Arrived, None, 2)
        );
        assert!(went.left.is_some_and(|m| m <= ARRIVE), "{went:?}");
        let world = eyes.0.borrow();
        assert_eq!(world.pushes[0], ("left".to_string(), 1500));
        assert_eq!(world.pushes[1].0, "up");
        assert!(world.presses.iter().all(|b| b == "ZL"));
        assert_eq!(world.presses.len(), world.pushes.len());
    }

    #[test]
    fn already_there_is_arrived_without_a_push() {
        let (eyes, mut hands, room) = walker();
        let graph = Graph::build(&room, &[], STEP_UP);
        let went = sim::drive(&eyes, &mut hands)
            .walk(&graph, [1.5, 0.0, 2.0], ARRIVE)
            .unwrap();
        assert_eq!((went.result, went.legs), (Done::Arrived, 0));
    }

    #[test]
    fn already_there_off_the_mesh_is_arrived_not_no_route() {
        // A chest is a hole in the mesh: Link at its front has no route
        // to plan, and needs none.
        let (eyes, mut hands) = sim::start([1.0, 0.0, 2.0], 0.0);
        let far = mesh(&[quad(10.0, 0.0, 14.0, 4.0, 0.0)]);
        let graph = Graph::build(&far, &[], STEP_UP);
        let went = sim::drive(&eyes, &mut hands)
            .walk(&graph, [1.2, 0.0, 2.0], ARRIVE)
            .unwrap();
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::Arrived, None, 0)
        );
        assert!(eyes.0.borrow().pushes.is_empty());
    }

    #[test]
    fn a_block_plans_again_from_where_link_is() {
        // A rock in the way, gone after the first push.
        let (went, eyes) = walked(|w| {
            w.walls = vec![[3.0, 0.0, 3.2, 4.0]];
            w.after_push = Some(Box::new(|w| w.walls.clear()));
        });
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::Arrived, None, 3)
        );
        // The second leg started where the first stopped, short of the rock.
        assert!(eyes.0.borrow().at[0] > 14.0);
    }

    #[test]
    fn a_second_block_at_the_same_waypoint_gives_up() {
        let (went, eyes) = walked(|w| w.walls = vec![[3.0, 0.0, 3.2, 4.0]]);
        assert_eq!(
            (went.result, went.reason),
            (Done::GaveUp, Some(Reason::Blocked))
        );
        assert_eq!(went.legs, 2);
        assert!(went.left.is_some_and(|m| m > 10.0));
        assert!(eyes.0.borrow().at[0] < 3.0);
    }

    #[test]
    fn a_goal_on_another_island_is_no_route() {
        let (eyes, mut hands) = sim::start([1.0, 0.0, 2.0], 0.0);
        let two = mesh(&[
            quad(0.0, 0.0, 4.0, 4.0, 0.0),
            quad(10.0, 0.0, 14.0, 4.0, 0.0),
        ]);
        let graph = Graph::build(&two, &[], STEP_UP);
        let went = sim::drive(&eyes, &mut hands)
            .walk(&graph, [12.0, 0.0, 2.0], ARRIVE)
            .unwrap();
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::GaveUp, Some(Reason::NoRoute), 0)
        );
        assert!(eyes.0.borrow().pushes.is_empty());
    }

    #[test]
    fn a_goal_just_off_the_mesh_is_walked_to_straight() {
        // Past the 1.5 m a plan reaches off the mesh for its goal.
        let (eyes, mut hands) = sim::start([3.5, 0.0, 2.0], std::f32::consts::FRAC_PI_2);
        let ledge = mesh(&[quad(0.0, 0.0, 4.0, 4.0, 0.0)]);
        let graph = Graph::build(&ledge, &[], STEP_UP);
        let went = sim::drive(&eyes, &mut hands)
            .walk(&graph, [6.2, 0.0, 2.0], ARRIVE)
            .unwrap();
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::Arrived, None, 1)
        );
        assert_eq!(eyes.0.borrow().pushes.len(), 1);
    }

    #[test]
    fn a_slide_that_drops_stops_there() {
        let (eyes, mut hands) = sim::start([6.0, 0.0, 2.0], std::f32::consts::FRAC_PI_2);
        eyes.0.borrow_mut().after_push = Some(Box::new(|w| w.at[1] = -2.0));
        let stopped = Went {
            result: Done::GaveUp,
            reason: Some(Reason::Blocked),
            secs: 0.0,
            legs: 2,
            left: Some(3.0),
        };
        let went = sim::drive(&eyes, &mut hands)
            .slide(stopped, [9.0, 0.0, 2.0], ARRIVE)
            .unwrap();
        assert_eq!((went.reason, went.legs), (Some(Reason::Blocked), 3));
        assert_eq!(eyes.0.borrow().pushes.len(), 1);
    }

    #[test]
    fn a_loading_zone_is_left_map() {
        let (went, _) = walked(|w| w.after_push = Some(Box::new(|w| w.map = "Other".into())));
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::GaveUp, Some(Reason::LeftMap), 1)
        );
    }

    #[test]
    fn an_event_is_event() {
        let (went, _) = walked(|w| w.after_push = Some(Box::new(|w| w.event = true)));
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::GaveUp, Some(Reason::Event), 1)
        );
    }

    #[test]
    fn no_time_is_budget() {
        let (eyes, mut hands, room) = walker();
        let graph = Graph::build(&room, &[], STEP_UP);
        let mut drive = sim::drive(&eyes, &mut hands);
        drive.budget = Duration::ZERO;
        let went = drive.walk(&graph, GOAL, ARRIVE).unwrap();
        assert_eq!(
            (went.result, went.reason, went.legs),
            (Done::GaveUp, Some(Reason::Budget), 0)
        );
    }
}
