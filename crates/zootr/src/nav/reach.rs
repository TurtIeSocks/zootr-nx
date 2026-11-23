//! `dev reach`: how far from a holder its button prompt shows ("Open" on a
//! chest, "Lift" on a pot), read off the game rather than guessed. Link is
//! warped to spots along lines out from the holder's front, farthest first,
//! turned to face it, and a vision model says whether the prompt is up.
//! The room logic counts a check from floor this close to it.
//!
//! A creep by stick taps couldn't do it: a tap under 0.2 s didn't move Link,
//! longer ones moved him unevenly, and a locked-on push near a big chest
//! climbed its lip (Deku Tree, other machine, 2026-11-22). Each spot is a
//! warp, and the distance kept is where Link stands after facing, since
//! he settles down a slope after landing.

use std::fmt;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde::Serialize;

use super::drive::Drive;
use crate::checks::flat;
use crate::dev::Dev;
use crate::live::Memory;
use crate::play::Input;
use crate::vision::Vision;
use crate::warp::{self, Pose, Side};

/// How long Link gets to land and settle after a warp.
const SETTLE: Duration = Duration::from_millis(800);
/// How far above the holder's feet each spot is, so Link lands on its floor.
const LIFT: f32 = 0.3;

/// Where to stand: lines out from the holder, and the steps along each.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Degrees off the holder's front, clockwise seen from above.
    pub angles: Vec<f32>,
    pub far: f32,
    pub near: f32,
    pub step: f32,
    /// The prompt's word, like `Open` or `Lift`.
    pub word: String,
}

/// One spot tried.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Tried {
    pub angle: f32,
    /// How far from the holder the warp aimed, and where Link stood: flat
    /// from the holder, and his feet above its origin.
    pub aimed: f32,
    pub off: f32,
    pub up: f32,
    pub faced: bool,
    /// Whether the prompt showed, `None` when the model didn't say.
    pub shown: Option<bool>,
}

/// One line's edge: the farthest Link stood with the prompt up, and the
/// nearest past that without it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Edge {
    pub angle: f32,
    pub shown: Option<f32>,
    pub gone: Option<f32>,
}

/// What a reach run found.
#[derive(Debug, Serialize)]
pub struct Reach {
    pub hash: u64,
    pub name: String,
    pub word: String,
    pub edges: Vec<Edge>,
    pub tried: Vec<Tried>,
}

impl fmt::Display for Reach {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{} {}: \"{}\"", self.name, self.hash, self.word)?;
        let m = |d: Option<f32>| d.map_or("-".into(), |d| format!("{d:.2} m"));
        for e in &self.edges {
            let (shown, gone) = (m(e.shown), m(e.gone));
            writeln!(f, "{:+4.0}°: shown at {shown}, gone at {gone}", e.angle)?;
        }
        for t in &self.tried {
            let shown = t.shown.map_or("?", |s| if s { "shown" } else { "gone" });
            let faced = if t.faced { "" } else { ", not facing" };
            let (angle, aimed, off, up) = (t.angle, t.aimed, t.off, t.up);
            writeln!(
                f,
                "  {angle:+4.0}° aimed {aimed:.2}: stood {off:.2} m, {up:+.2} up{faced}: {shown}"
            )?;
        }
        Ok(())
    }
}

/// The distances along a line, `far` down to `near` by `step`.
fn steps(plan: &Plan) -> Vec<f32> {
    let n = ((plan.far - plan.near) / plan.step).round().max(0.0) as usize;
    (0..=n).map(|i| plan.far - i as f32 * plan.step).collect()
}

/// The flat unit vector `angle` degrees clockwise (seen from above, `+Y`
/// up) off the front of `pose`.
fn line(pose: &Pose, angle: f32) -> [f32; 2] {
    let ahead = warp::landing_at(pose, Side::Front, 1.0);
    let (fx, fz) = (ahead[0] - pose.at[0], ahead[2] - pose.at[2]);
    // Right of front is `(-fz, fx)` (`warp::landing_at`'s `Side::Right`).
    let (s, c) = angle.to_radians().sin_cos();
    [fx * c - fz * s, fz * c + fx * s]
}

/// The spot `d` m out along `dir` from `at`, a little above its feet.
fn spot(at: [f32; 3], dir: [f32; 2], d: f32) -> [f32; 3] {
    [at[0] + dir[0] * d, at[1] + LIFT, at[2] + dir[1] * d]
}

/// Each line's edge from what was tried along it, nearest first: the
/// farthest spot with the prompt up, and the nearest beyond it without.
fn edges(tried: &[Tried], angles: &[f32]) -> Vec<Edge> {
    let edge = |angle: f32| {
        let on = tried.iter().filter(|t| t.angle == angle);
        let shown =
            (on.clone().filter(|t| t.shown == Some(true)).map(|t| t.off)).max_by(f32::total_cmp);
        let past = |t: &&Tried| shown.is_none_or(|s| t.off > s);
        let gone = (on
            .filter(|t| t.shown == Some(false))
            .filter(past)
            .map(|t| t.off))
        .min_by(f32::total_cmp);
        Edge { angle, shown, gone }
    };
    angles.iter().map(|&a| edge(a)).collect()
}

/// Run `plan` around placement `hash` of the map Link is in, asking
/// `vision` whether its prompt is on screen at each spot. Frames
/// land in `out` when given.
pub fn reach<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    romfs: &Romfs,
    vision: &Vision,
    hash: u64,
    plan: &Plan,
    out: Option<&Path>,
) -> Result<Reach> {
    let map = dev.state()?.map;
    let placed = warp::placements_in(romfs, &map)?;
    let holder = (placed.iter().find(|p| p.hash == Some(hash)))
        .with_context(|| format!("{hash} isn't placed in {map}"))?;
    let pose = warp::facing(&holder.name, &holder.pose);
    let at = pose.at;
    let question = format!(
        "Is a button prompt with the word {} shown on screen?",
        plan.word
    );
    let mut tried = Vec::new();
    for &angle in &plan.angles {
        let dir = line(&pose, angle);
        for aimed in steps(plan) {
            dev.warp(spot(at, dir, aimed))?;
            sleep(SETTLE);
            let faced = Drive::new(dev, input).face(at)?;
            input.press("ZL")?;
            let looked = crate::look::look(vision, input, std::slice::from_ref(&question), out)?;
            let here = dev.link()?;
            tried.push(Tried {
                angle,
                aimed,
                off: flat(here, at),
                up: here[1] - at[1],
                faced,
                shown: looked.answers.get(&question).and_then(|v| v.as_bool()),
            });
        }
    }
    Ok(Reach {
        hash,
        name: holder.name.clone(),
        word: plan.word.clone(),
        edges: edges(&tried, &plan.angles),
        tried,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(yaw: f32) -> Pose {
        Pose {
            at: [10.0, 5.0, -3.0],
            rotate: [0.0, yaw, 0.0],
            scale: [1.0; 3],
        }
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn steps_run_far_to_near() {
        let plan = Plan {
            angles: vec![0.0],
            far: 1.6,
            near: 1.0,
            step: 0.2,
            word: "Open".into(),
        };
        let got = steps(&plan);
        assert_eq!(got.len(), 4);
        assert!((got[0] - 1.6).abs() < 1e-5 && (got[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn front_line_matches_warps_front_and_turns_clockwise() {
        // Unturned, the front is +Z; its right is -X.
        assert!(close(line(&pose(0.0), 0.0), [0.0, 1.0]));
        assert!(close(line(&pose(0.0), 90.0), [-1.0, 0.0]));
        assert!(close(line(&pose(0.0), -90.0), [1.0, 0.0]));
        // A quarter turn about +Y faces -X (the game's way round).
        assert!(close(
            line(&pose(std::f32::consts::FRAC_PI_2), 0.0),
            [-1.0, 0.0]
        ));
    }

    #[test]
    fn spot_is_out_along_the_line_and_lifted() {
        assert_eq!(
            spot([1.0, 2.0, 3.0], [0.0, 1.0], 1.5),
            [1.0, 2.0 + LIFT, 4.5]
        );
    }

    fn tried(angle: f32, off: f32, shown: Option<bool>) -> Tried {
        Tried {
            angle,
            aimed: off,
            off,
            up: 0.0,
            faced: true,
            shown,
        }
    }

    #[test]
    fn edge_is_farthest_shown_and_nearest_gone_past_it() {
        let t = [
            tried(0.0, 1.8, Some(false)),
            tried(0.0, 1.5, Some(false)),
            tried(0.0, 1.3, Some(true)),
            tried(0.0, 1.1, Some(true)),
            // A miss by the model nearer in doesn't move the edge.
            tried(0.0, 0.9, Some(false)),
            tried(30.0, 1.2, None),
        ];
        let got = edges(&t, &[0.0, 30.0]);
        assert_eq!(
            got[0],
            Edge {
                angle: 0.0,
                shown: Some(1.3),
                gone: Some(1.5)
            }
        );
        assert_eq!(
            got[1],
            Edge {
                angle: 30.0,
                shown: None,
                gone: None
            }
        );
    }
}
