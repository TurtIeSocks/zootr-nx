//! `dev leap`: a running jump across a gap, timed by where Link is rather
//! than by a guessed hold. He runs from a start behind the near edge
//! toward the far one, and X goes in once he's due at the near edge (or
//! `past` m beyond it, where the real lip sits past the navmesh's edge).
//! Hand-timed holds over the bridge missed the edge by a metre either way
//! (Deku Tree, other machine, 2026-11-21).

use std::fmt;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use serde::Serialize;

use super::drive::Drive;
use super::trace::{Sample, Step, Traced};
use crate::checks::flat;
use crate::dev::Dev;
use crate::live::Memory;
use crate::play::Input;

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
/// A landing counts as on the far floor within this height of it, in m.
const LEVEL: f32 = 0.5;
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
    Drive::new(dev, input).face(to)?;
    let (mut samples, mut jumped) = (Vec::<Sample>::new(), None::<Sample>);
    let (begun, mut hold) = (Instant::now(), 0.0);
    loop {
        let t = begun.elapsed().as_secs_f32();
        let at = dev.link()?;
        let ahead = along(from, dir, at);
        if jumped.is_none() {
            let speed = samples.last().map_or(0.0, |s| {
                (ahead - along(from, dir, s.at)) / (t - s.t).max(f32::EPSILON)
            });
            if due(ahead, speed, past) {
                input.queue(&format!("LUp+X~{JUMP}"))?;
                (jumped, hold) = (Some(Sample { t, at }), t + JUMP);
            } else if t >= RUN {
                samples.push(Sample { t, at });
                break;
            }
        }
        let after = jumped.map(|j| t - j.t);
        if t >= hold && after.is_none_or(|a| a < STEER) {
            input.queue(&format!("LUp~{CHUNK}"))?;
            hold = t + CHUNK;
        }
        samples.push(Sample { t, at });
        if after.is_some_and(|a| a >= STEER + settle) {
            break;
        }
    }
    let run_secs = jumped.map_or(RUN, |j| j.t);
    let steps = [
        Step::Hold("LUp".into(), run_secs.max(f32::EPSILON)),
        Step::Hold("LUp+X".into(), JUMP),
        Step::Hold("LUp".into(), STEER),
    ];
    let traced = Traced::of(&steps, samples, vec![])?;
    let width = flat(from, to);
    let made = jumped.is_some() && landed(to, width, along(from, dir, traced.end), traced.end);
    let past = jumped.map(|j| along(from, dir, j.at));
    let leap = Leap {
        from,
        to,
        width,
        jumped,
        past,
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
