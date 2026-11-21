//! `dev trace`: control lines played back to back while Link's position is
//! read every frame, with frames saved where the script asks. How far a
//! jump, a hover or a hookshot pull carries him, measured.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use crate::checks::flat;
use crate::dev::Dev;
use crate::live::Memory;
use crate::play::{Input, PRESS, line};

/// Link has left his start height once he's this far below it, in m.
pub const OFF: f32 = 0.3;
/// How long to keep reading after the last step by default, in s: long
/// enough for a fall to land.
pub const SETTLE: f32 = 1.5;
/// What the trace is saved as in its folder.
const FILE: &str = "trace.json";

/// One step of the script, each starting as the one before it ends.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Buttons joined with `+`, held this long.
    Hold(String, f32),
    /// Nothing held this long.
    Wait(f32),
    /// A frame of the game, saved in the trace's folder.
    Shot,
}

impl Step {
    /// `BUTTONS[~SECS]`, `wait~SECS` or `shot`.
    pub fn parse(text: &str) -> Result<Self> {
        let (head, secs) = match text.split_once('~') {
            Some((head, n)) => {
                let secs: f32 = n.parse().with_context(|| format!("{n} isn't seconds"))?;
                ensure!(secs > 0.0, "{text}: a step lasts more than 0 s");
                (head, Some(secs))
            }
            None => (text, None),
        };
        match head {
            "shot" if secs.is_none() => Ok(Step::Shot),
            "shot" => bail!("{text}: a shot takes no time"),
            "wait" => Ok(Step::Wait(secs.context("wait takes ~SECS")?)),
            buttons => {
                line(buttons, None)?;
                let secs = secs.unwrap_or(PRESS as f32 / 1000.0);
                Ok(Step::Hold(buttons.into(), secs))
            }
        }
    }

    fn secs(&self) -> f32 {
        match self {
            Step::Hold(_, s) | Step::Wait(s) => *s,
            Step::Shot => 0.0,
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Step::Hold(b, s) => write!(f, "{b}~{s}"),
            Step::Wait(s) => write!(f, "wait~{s}"),
            Step::Shot => write!(f, "shot"),
        }
    }
}

/// Where Link stood, `t` s after the first step.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Sample {
    pub t: f32,
    pub at: [f32; 3],
}

/// What a script did to Link, from where he stood each frame.
#[derive(Debug, PartialEq, Serialize)]
pub struct Traced {
    pub steps: Vec<String>,
    pub start: [f32; 3],
    pub end: [f32; 3],
    /// Start to end, flat, in m.
    pub flat: f32,
    /// The farthest he got from the start, flat.
    pub farthest: f32,
    /// His highest above the start, and his lowest below it.
    pub rise: f32,
    pub fall: f32,
    /// How far he got, flat, and when, before he first fell `OFF` below
    /// his start height. `None` when he never did.
    pub level: Option<Sample>,
    pub level_flat: Option<f32>,
    /// The frames saved, by when they were asked for.
    pub shots: Vec<(f32, PathBuf)>,
    pub samples: Vec<Sample>,
}

impl Traced {
    fn of(steps: &[Step], samples: Vec<Sample>, shots: Vec<(f32, PathBuf)>) -> Result<Self> {
        let (first, last) = match (samples.first(), samples.last()) {
            (Some(f), Some(l)) => (*f, *l),
            _ => bail!("Link's position was never read"),
        };
        let y0 = first.at[1];
        let heights = samples.iter().map(|s| s.at[1] - y0);
        let off = samples.iter().position(|s| s.at[1] < y0 - OFF);
        let level = off.map(|i| samples[i.saturating_sub(1)]);
        Ok(Traced {
            steps: steps.iter().map(Step::to_string).collect(),
            start: first.at,
            end: last.at,
            flat: flat(first.at, last.at),
            farthest: (samples.iter().map(|s| flat(first.at, s.at))).fold(0.0, f32::max),
            rise: heights.clone().fold(0.0, f32::max),
            fall: -heights.fold(0.0, f32::min),
            level_flat: level.map(|s| flat(first.at, s.at)),
            level,
            shots,
            samples,
        })
    }
}

impl fmt::Display for Traced {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let span = self.samples.last().map_or(0.0, |s| s.t);
        writeln!(f, "{} samples over {span:.2} s", self.samples.len())?;
        writeln!(f, "start {:.2?} end {:.2?}", self.start, self.end)?;
        write!(
            f,
            "flat {:.2} m, farthest {:.2} m, rise {:.2} m, fall {:.2} m",
            self.flat, self.farthest, self.rise, self.fall
        )?;
        if let (Some(s), Some(d)) = (self.level, self.level_flat) {
            write!(f, "\nat level until {:.2} s, {d:.2} m out", s.t)?;
        }
        for (t, p) in &self.shots {
            write!(f, "\nshot {t:.2} s: {}", p.display())?;
        }
        Ok(())
    }
}

/// When each step starts, in s from the first.
fn starts(steps: &[Step]) -> Vec<f32> {
    let mut t = 0.0;
    steps
        .iter()
        .map(|s| {
            let at = t;
            t += s.secs();
            at
        })
        .collect()
}

/// Play `steps` and read Link's position every frame until `settle` s past
/// the last, saving frames and the trace in `out` when given.
pub fn trace<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    steps: &[Step],
    settle: f32,
    out: Option<&Path>,
) -> Result<Traced> {
    let shoots = steps.contains(&Step::Shot);
    ensure!(!shoots || out.is_some(), "a shot needs --out to save it in");
    if let Some(dir) = out {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    }
    let starts = starts(steps);
    let end = starts
        .last()
        .zip(steps.last())
        .map_or(0.0, |(t, s)| t + s.secs())
        + settle;
    let (mut samples, mut shots, mut next) = (Vec::new(), Vec::new(), 0);
    let begun = Instant::now();
    loop {
        let t = begun.elapsed().as_secs_f32();
        while next < steps.len() && starts[next] <= t {
            match &steps[next] {
                Step::Hold(buttons, secs) => {
                    input.queue(&line(buttons, Some((secs * 1000.0) as u64))?)?
                }
                Step::Wait(_) => {}
                Step::Shot => {
                    let dir = out.context("a shot needs --out")?;
                    let to = std::path::absolute(dir.join(format!("{:05.2}s.png", starts[next])))?;
                    input.queue(&format!("shot {}", to.display()))?;
                    shots.push((starts[next], to));
                }
            }
            next += 1;
        }
        samples.push(Sample { t, at: dev.link()? });
        if t >= end && next == steps.len() {
            break;
        }
    }
    let traced = Traced::of(steps, samples, shots)?;
    if let Some(dir) = out {
        let file = dir.join(FILE);
        std::fs::write(&file, serde_json::to_string_pretty(&traced)?)
            .with_context(|| format!("writing {}", file.display()))?;
    }
    Ok(traced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_read_as_control_lines() {
        assert_eq!(
            Step::parse("LUp+A~0.5").unwrap(),
            Step::Hold("LUp+A".into(), 0.5)
        );
        assert_eq!(Step::parse("A").unwrap(), Step::Hold("A".into(), 0.15));
        assert_eq!(Step::parse("wait~1").unwrap(), Step::Wait(1.0));
        assert_eq!(Step::parse("shot").unwrap(), Step::Shot);
        for bad in ["Jump~1", "wait", "shot~1", "A~0", "A~x"] {
            assert!(Step::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn steps_start_back_to_back() {
        let steps = ["LUp~1", "shot", "A~0.2", "wait~0.5"].map(|s| Step::parse(s).unwrap());
        assert_eq!(starts(&steps), [0.0, 1.0, 1.0, 1.2]);
    }

    #[test]
    fn a_hover_off_a_ledge_reads_its_reach_at_level() {
        let at = |t: f32, x: f32, y: f32| Sample { t, at: [x, y, 0.0] };
        let samples = vec![
            at(0.0, 0.0, 10.0),
            at(0.5, 2.0, 10.1),
            at(1.0, 4.0, 9.9),
            at(1.5, 5.0, 9.5),
            at(2.0, 5.5, 8.0),
        ];
        let t = Traced::of(&[], samples, vec![]).unwrap();
        assert_eq!(t.level, Some(at(1.0, 4.0, 9.9)));
        assert_eq!(t.level_flat, Some(4.0));
        assert!((t.rise - 0.1).abs() < 1e-4 && (t.fall - 2.0).abs() < 1e-4);
        assert_eq!((t.flat, t.farthest), (5.5, 5.5));
    }
}
