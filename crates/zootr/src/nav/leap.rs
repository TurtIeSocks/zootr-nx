//! `dev leap`: a running jump across a gap, timed by where Link is rather
//! than by a guessed hold. He runs from a start behind the near edge
//! toward the far one, and X goes in once he's due at the near edge (or
//! `past` m beyond it, where the real lip sits past the navmesh's edge).
//! Hand-timed holds over the bridge missed the edge by a metre either way
//! (Deku Tree, other machine, 2026-11-21).
//!
//! No ZL: it locks onto a scrub, a Skulltula or an eye switch in reach,
//! and a locked-on Link strafes instead of running (round 3, 2026-11-21).
//! The stick is camera-relative and the camera isn't read, so the run
//! steers by what Link does: a short push finds which way the camera
//! faces, and each hold after it aims at the far edge from where the
//! hold before it actually took him.

use std::fmt;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;

use super::trace::{Sample, Step, Traced};
use super::{CLOCKWISE, eighths};
use crate::checks::flat;
use crate::dev::Dev;
use crate::live::Memory;
use crate::play::{Input, stick};

/// How long a control line takes to reach Link over the bridge, in s
/// (room science notes: ~0.1 s).
const LATENCY: f32 = 0.1;
/// Each hold of the run, in s, queued again as the last one ends.
const CHUNK: f32 = 0.2;
/// How long X is held.
const JUMP: f32 = 0.15;
/// How long the stick stays forward after X: Link steers in the air.
const STEER: f32 = 0.6;
/// The run gives up after this long without reaching the edge, in s.
const RUN: f32 = 4.0;
/// The push that finds the camera, in s, and the wait for Link to stop.
const PROBE: f32 = 0.2;
const STOP: f32 = 0.5;
/// A push that moves Link less than this, in m, says nothing of the camera.
const MOVED: f32 = 0.1;
/// A landing counts as on the far floor within this height of it, in m.
const LEVEL: f32 = 0.5;
/// How far back from X Link's run speed is read, in s.
const PACE: f32 = 0.35;
/// The jump is over at the first sample past its apex within this of
/// the height X went in at, in m.
const DOWN: f32 = 0.08;
/// What the leap is saved as in its folder.
const FILE: &str = "trace.json";

/// What a leap did.
#[derive(Debug, Serialize)]
pub struct Leap {
    pub from: [f32; 3],
    pub to: [f32; 3],
    /// The gap's width, flat, from the near edge to the far.
    pub width: f32,
    /// Where Link was as X went in, and how far past the near edge,
    /// along the jump's line. `None` when he never got there.
    pub jumped: Option<Sample>,
    pub past: Option<f32>,
    /// His run speed as X went in, flat, in m/s: a jump carries him
    /// about this times its air time.
    pub speed: Option<f32>,
    /// From X to back down at its height: how far, flat, and how long.
    /// `None` when he landed lower, or never jumped.
    pub air: Option<(f32, f32)>,
    /// Whether he ended on the far floor: at its height and past its edge.
    pub made: bool,
    pub traced: Traced,
}

impl fmt::Display for Leap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let verdict = if self.made { "made" } else { "missed" };
        write!(f, "{verdict} a {:.2} m gap", self.width)?;
        match self.past {
            Some(p) => writeln!(f, ", X {p:+.2} m from the near edge")?,
            None => writeln!(f, ", never reached the near edge")?,
        }
        if let Some(v) = self.speed {
            write!(f, "running {v:.2} m/s at X")?;
            match self.air {
                Some((d, t)) => writeln!(f, ", {d:.2} m in the air over {t:.2} s")?,
                None => writeln!(f, ", landed below the height X went in at")?,
            }
        }
        write!(f, "{}", self.traced)
    }
}

/// The jump's line from `from` to `to`, flat: a unit direction.
fn line(from: [f32; 3], to: [f32; 3]) -> (f32, f32) {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    let len = dx.hypot(dz).max(f32::EPSILON);
    (dx / len, dz / len)
}

/// How far `at` is past `from` along the line `(ux, uz)`.
fn along(from: [f32; 3], (ux, uz): (f32, f32), at: [f32; 3]) -> f32 {
    (at[0] - from[0]) * ux + (at[2] - from[2]) * uz
}

/// Whether X should go in now: Link, `ahead` m past the near edge and
/// closing at `speed` m/s, is due `past` m beyond it as the press lands.
fn due(ahead: f32, speed: f32, past: f32) -> bool {
    ahead + speed.max(0.0) * LATENCY >= past
}

/// Which way the camera faces, from Link going `from` to `to` on a push
/// `k` eighths clockwise of up. `None` when he barely moved.
fn camera(from: [f32; 3], to: [f32; 3], k: i32) -> Option<f32> {
    let moved = (to[0] - from[0]).atan2(to[2] - from[2]);
    (flat(from, to) >= MOVED).then_some(moved + k as f32 * std::f32::consts::FRAC_PI_4)
}

/// The stick's eighth, clockwise of up, and its control line, that points
/// Link at `at` toward `to` with the camera facing `cam`.
fn aim(cam: f32, at: [f32; 3], to: [f32; 3]) -> (i32, String) {
    let k = eighths(cam, at, to).rem_euclid(8);
    (k, stick(CLOCKWISE[k as usize]))
}

/// Find which way the camera faces: a short push up, else down (a wall
/// ahead). Link is left where the push took him.
fn find_camera<M: Memory, I: Input>(dev: &Dev<M>, input: &mut I) -> Result<f32> {
    for k in [0, 4] {
        let before = dev.link()?;
        input.queue(&format!("{}~{PROBE}", stick(CLOCKWISE[k])))?;
        sleep(Duration::from_secs_f32(PROBE + STOP));
        if let Some(cam) = camera(before, dev.link()?, k as i32) {
            return Ok(cam);
        }
    }
    anyhow::bail!("Link didn't move on a push up or down: walled in at the start")
}

/// Link's flat speed over the [`PACE`] s up to `x`.
fn speed(samples: &[Sample], x: &Sample) -> Option<f32> {
    let first = samples.iter().find(|s| s.t >= x.t - PACE && s.t < x.t)?;
    Some(flat(first.at, x.at) / (x.t - first.t).max(f32::EPSILON))
}

/// How far, flat, and how long Link was in the air from `x`: until the
/// first sample past the apex back within [`DOWN`] of X's height.
fn air(samples: &[Sample], x: &Sample) -> Option<(f32, f32)> {
    let after: Vec<&Sample> = samples.iter().filter(|s| s.t > x.t).collect();
    let apex = (after.iter().enumerate())
        .max_by(|a, b| a.1.at[1].total_cmp(&b.1.at[1]))?
        .0;
    let down = after[apex..].iter().find(|s| s.at[1] <= x.at[1] + DOWN)?;
    (down.at[1] >= x.at[1] - LEVEL).then(|| (flat(x.at, down.at), down.t - x.t))
}

/// Whether Link ended on the far floor: within [`LEVEL`] of its height,
/// and at or past its edge.
fn landed(to: [f32; 3], width: f32, ahead: f32, end: [f32; 3]) -> bool {
    (end[1] - to[1]).abs() <= LEVEL && ahead >= width - LEVEL
}

/// Run Link from `run` m behind `from` toward `to` and jump, X going in
/// as he's due `past` m beyond `from`. Reads him every frame until
/// `settle` s after the jump, saving the trace in `out` when given.
pub fn leap<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    (from, to): ([f32; 3], [f32; 3]),
    run: f32,
    past: f32,
    settle: f32,
    out: Option<&Path>,
) -> Result<Leap> {
    let dir = line(from, to);
    let start = [from[0] - dir.0 * run, from[1], from[2] - dir.1 * run];
    dev.warp(start)?;
    sleep(Duration::from_secs_f32(STOP));
    let mut cam = find_camera(dev, input)?;
    let (mut samples, mut jumped) = (Vec::<Sample>::new(), None::<Sample>);
    let (begun, mut hold) = (Instant::now(), 0.0);
    // The last hold: its eighth and where Link was as it went in.
    let mut last: Option<(i32, [f32; 3])> = None;
    loop {
        let t = begun.elapsed().as_secs_f32();
        let at = dev.link()?;
        let ahead = along(from, dir, at);
        if jumped.is_none() {
            let speed = samples.last().map_or(0.0, |s| {
                (ahead - along(from, dir, s.at)) / (t - s.t).max(f32::EPSILON)
            });
            if due(ahead, speed, past) {
                let (_, way) = aim(cam, at, to);
                input.queue(&format!("{way}+X~{JUMP}"))?;
                (jumped, hold) = (Some(Sample { t, at }), t + JUMP);
            } else if t >= RUN {
                samples.push(Sample { t, at });
                break;
            }
        }
        let after = jumped.map(|j| t - j.t);
        if t >= hold && after.is_none_or(|a| a < STEER) {
            if let Some(c) = last.and_then(|(k, was)| camera(was, at, k)) {
                cam = c;
            }
            let (k, way) = aim(cam, at, to);
            input.queue(&format!("{way}~{CHUNK}"))?;
            (hold, last) = (t + CHUNK, Some((k, at)));
        }
        samples.push(Sample { t, at });
        if after.is_some_and(|a| a >= STEER + settle) {
            break;
        }
    }
    let run_secs = jumped.map_or(RUN, |j| j.t);
    let steps = [
        Step::Hold("toward TO".into(), run_secs.max(f32::EPSILON)),
        Step::Hold("toward TO+X".into(), JUMP),
        Step::Hold("toward TO".into(), STEER),
    ];
    let traced = Traced::of(&steps, samples, vec![])?;
    let width = flat(from, to);
    let made = jumped.is_some() && landed(to, width, along(from, dir, traced.end), traced.end);
    let past = jumped.map(|j| along(from, dir, j.at));
    let speed = jumped.and_then(|j| speed(&traced.samples, &j));
    let air = jumped.and_then(|j| air(&traced.samples, &j));
    let leap = Leap {
        from,
        to,
        width,
        jumped,
        past,
        speed,
        air,
        made,
        traced,
    };
    if let Some(dir) = out {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
        let file = dir.join(FILE);
        std::fs::write(&file, serde_json::to_string_pretty(&leap)?)
            .with_context(|| format!("writing {}", file.display()))?;
    }
    Ok(leap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_goes_in_a_latency_early() {
        // At 4.5 m/s, 0.45 m short of the edge is due at it.
        assert!(due(-0.45, 4.5, 0.0));
        assert!(!due(-0.6, 4.5, 0.0));
        // Past the navmesh edge, the press waits for the lip.
        assert!(!due(0.0, 4.5, 1.0));
        assert!(due(0.6, 4.5, 1.0));
    }

    #[test]
    fn a_push_says_which_way_the_camera_faces() {
        // Up moved Link along +Z: the camera faces +Z, and up aims there.
        let cam = camera([0.0; 3], [0.0, 0.0, 1.0], 0).unwrap();
        assert!(cam.abs() < 1e-5);
        assert_eq!(aim(cam, [0.0; 3], [0.0, 0.0, 5.0]), (0, "LUp".into()));
        // Right (a quarter clockwise of up) moved him along -X: the camera
        // still faces +Z, and aiming at -X picks right again.
        let cam = camera([0.0; 3], [-1.0, 0.0, 0.0], 2).unwrap();
        assert!(cam.abs() < 1e-5);
        assert_eq!(aim(cam, [0.0; 3], [-5.0, 0.0, 0.0]), (2, "LRight".into()));
        // A push that barely moved him says nothing.
        assert_eq!(camera([0.0; 3], [0.05, 0.0, 0.0], 0), None);
    }

    fn arc() -> (Vec<Sample>, Sample) {
        // Running at 4 m/s along +Z, X at t 1, 0.7 m up and down over 0.7 s.
        let at = |t: f32, y: f32| Sample {
            t,
            at: [0.0, y, 4.0 * t],
        };
        let samples = vec![
            at(0.7, 10.0),
            at(0.9, 10.0),
            at(1.0, 10.0),
            at(1.2, 10.5),
            at(1.4, 10.7),
            at(1.6, 10.4),
            at(1.7, 10.05),
            at(1.8, 10.0),
        ];
        let x = samples[2];
        (samples, x)
    }

    #[test]
    fn a_jump_reads_its_run_speed_and_its_air() {
        let (samples, x) = arc();
        assert!((speed(&samples, &x).unwrap() - 4.0).abs() < 1e-3);
        let (d, t) = air(&samples, &x).unwrap();
        assert!((t - 0.7).abs() < 1e-3 && (d - 2.8).abs() < 1e-3, "{d} {t}");
        // Landing a metre below X's height has no level air to read.
        let mut low = samples.clone();
        for s in &mut low[6..] {
            s.at[1] = 9.0;
        }
        assert_eq!(air(&low, &x), None);
    }

    #[test]
    fn a_landing_counts_only_on_the_far_floor() {
        let to = [0.0, 10.0, 3.0];
        let dir = line([0.0, 10.0, 0.0], to);
        assert_eq!(dir, (0.0, 1.0));
        assert!(landed(to, 3.0, 3.4, [0.0, 10.1, 3.4]));
        // Short of the far edge, or on a lip below it.
        assert!(!landed(to, 3.0, 2.0, [0.0, 10.0, 2.0]));
        assert!(!landed(to, 3.0, 3.2, [0.0, 8.9, 3.2]));
    }
}
