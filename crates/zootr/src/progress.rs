//! A seed's generation as steps: each timed and reported as it goes, with a
//! stop asked for between them.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use serde::Serialize;

mod log;

pub use log::About;

/// The steps of generating and installing a seed, in order, and the one
/// of rebuilding a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Step {
    Dump,
    Checks,
    Logic,
    Placing,
    Writing,
    Spoiler,
    Installing,
    Rebuilding,
}

pub const STEPS: [Step; 8] = [
    Step::Dump,
    Step::Checks,
    Step::Logic,
    Step::Placing,
    Step::Writing,
    Step::Spoiler,
    Step::Installing,
    Step::Rebuilding,
];

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Dump => "reading the dump",
            Step::Checks => "reading checks",
            Step::Logic => "building logic",
            Step::Placing => "placing items",
            Step::Writing => "writing the mod",
            Step::Spoiler => "writing the spoiler",
            Step::Installing => "installing",
            Step::Rebuilding => "rebuilding the game",
        }
    }
}

/// A step and its name, for the app's list.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Named {
    pub step: Step,
    pub name: &'static str,
}

pub fn names() -> Vec<Named> {
    (STEPS.into_iter())
        .map(|step| Named {
            step,
            name: step.name(),
        })
        .collect()
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum End {
    Done,
    Failed { error: String },
    Stopped { took_off: bool },
}

/// What a run reports as it goes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StepEvent {
    Began {
        step: Step,
    },
    /// What the current step is doing now.
    Detail {
        text: String,
    },
    /// A step ended, with its last detail.
    Ended {
        step: Step,
        detail: String,
        secs: f64,
    },
    End {
        end: End,
        secs: f64,
    },
}

/// A stop landed at a checkpoint. Once writing replaced a mod folder, the
/// install took the mod off every target.
#[derive(Debug)]
pub struct Stopped {
    pub took_off: bool,
}

impl fmt::Display for Stopped {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.took_off {
            false => write!(f, "Stopped: the targets are as they were"),
            true => write!(
                f,
                "Stopped while writing: the mod is off every target, so the game runs vanilla until the next install"
            ),
        }
    }
}

impl std::error::Error for Stopped {}

/// The stop `e` carries, if it's one.
pub fn stopped(e: &anyhow::Error) -> Option<&Stopped> {
    e.downcast_ref()
}

/// Asks a run to stop at its next checkpoint, from any thread.
#[derive(Clone, Default)]
pub struct Stopper(Arc<AtomicBool>);

impl Stopper {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

type Sink = Box<dyn Fn(&StepEvent) + Send + Sync>;

#[derive(Default)]
struct Now {
    step: Option<(Step, Instant)>,
    detail: String,
    reached: Vec<Step>,
    /// The target being written, the detail writing the mod starts with.
    target: String,
}

/// Where a generation's steps go, and whether it was asked to stop.
pub struct Run {
    sinks: Vec<Sink>,
    stop: Stopper,
    started: Instant,
    now: Mutex<Now>,
    log: Option<PathBuf>,
}

impl Run {
    /// No sinks and never stopped, for tests and the sweep.
    pub fn silent() -> Run {
        Run {
            sinks: Vec::new(),
            stop: Stopper::default(),
            started: Instant::now(),
            now: Mutex::default(),
            log: None,
        }
    }

    /// This run, also reporting to `sink`.
    pub fn with(mut self, sink: impl Fn(&StepEvent) + Send + Sync + 'static) -> Run {
        self.sinks.push(Box::new(sink));
        self
    }

    pub fn stopper(&self) -> Stopper {
        self.stop.clone()
    }

    fn send(&self, event: StepEvent) {
        self.sinks.iter().for_each(|s| s(&event));
    }

    /// Ends the current step, if one runs.
    fn close(&self, now: &mut Now) {
        if let Some((step, at)) = now.step.take() {
            let detail = std::mem::take(&mut now.detail);
            let secs = at.elapsed().as_secs_f64();
            self.send(StepEvent::Ended { step, detail, secs });
        }
    }

    /// Start a step. Errors with `Stopped` once a stop was asked.
    pub fn step(&self, step: Step) -> Result<()> {
        self.check()?;
        let mut now = self.now.lock().unwrap();
        self.close(&mut now);
        now.step = Some((step, Instant::now()));
        now.reached.push(step);
        self.send(StepEvent::Began { step });
        if step == Step::Writing && !now.target.is_empty() {
            now.detail = now.target.clone();
            self.send(StepEvent::Detail {
                text: now.detail.clone(),
            });
        }
        Ok(())
    }

    /// What the current step is doing now: "try 3 of 20".
    pub fn detail(&self, text: impl Into<String>) {
        let text = text.into();
        self.now.lock().unwrap().detail = text.clone();
        self.send(StepEvent::Detail { text });
    }

    /// The target the mod is written for next.
    pub fn target(&self, name: &str) {
        self.now.lock().unwrap().target = name.to_owned();
    }

    /// Whether `step` has started.
    pub fn reached(&self, step: Step) -> bool {
        self.now.lock().unwrap().reached.contains(&step)
    }

    /// A checkpoint inside a long step. Errors with `Stopped` too.
    pub fn check(&self) -> Result<()> {
        match self.stop.0.load(Ordering::Relaxed) {
            true => Err(Stopped { took_off: false }.into()),
            false => Ok(()),
        }
    }

    /// Ends the run: the last step's time, and done, failed or stopped.
    pub fn end(&self, result: &Result<impl Sized>) {
        self.close(&mut self.now.lock().unwrap());
        let end = match result {
            Ok(_) => End::Done,
            Err(e) => match stopped(e) {
                Some(s) => End::Stopped {
                    took_off: s.took_off,
                },
                None => End::Failed {
                    error: format!("{e:#}"),
                },
            },
        };
        let secs = self.started.elapsed().as_secs_f64();
        self.send(StepEvent::End { end, secs });
    }
}

/// `event` as a line of the log or the command line, if it makes one: an
/// ended step with its time, or the end.
pub fn line(event: &StepEvent) -> Option<String> {
    match event {
        StepEvent::Ended { step, detail, secs } => {
            let detail = match detail.is_empty() {
                true => String::new(),
                false => format!(" ({detail})"),
            };
            Some(format!("{secs:>5.1} s  {}{detail}", step.name()))
        }
        StepEvent::End { end, secs } => Some(match end {
            End::Done => format!("  done in {secs:.1} s"),
            End::Failed { .. } => format!("  failed after {secs:.1} s"),
            End::Stopped { .. } => format!("  stopped after {secs:.1} s"),
        }),
        StepEvent::Began { .. } | StepEvent::Detail { .. } => None,
    }
}

/// Prints each ended step and the end to stderr.
pub fn print(event: &StepEvent) {
    if let Some(line) = line(event) {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run whose events land in the returned list.
    fn heard() -> (Run, Arc<Mutex<Vec<StepEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let run = Run::silent().with(move |e| sink.lock().unwrap().push(e.clone()));
        (run, events)
    }

    #[test]
    fn events_arrive_in_step_order_with_times() {
        let (run, events) = heard();
        run.step(Step::Dump).unwrap();
        run.detail("cached");
        run.step(Step::Checks).unwrap();
        run.end(&Ok(()));
        let events = events.lock().unwrap();
        let kinds: Vec<String> = events
            .iter()
            .map(|e| match e {
                StepEvent::Began { step } => format!("began {}", step.name()),
                StepEvent::Detail { text } => format!("detail {text}"),
                StepEvent::Ended { step, detail, .. } => format!("ended {} {detail}", step.name()),
                StepEvent::End { end, .. } => format!("end {end:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "began reading the dump",
                "detail cached",
                "ended reading the dump cached",
                "began reading checks",
                "ended reading checks ",
                "end Done",
            ]
        );
        let secs = |e: &StepEvent| match e {
            StepEvent::Ended { secs, .. } | StepEvent::End { secs, .. } => *secs,
            _ => 0.0,
        };
        assert!(secs(&events[5]) >= secs(&events[2]) + secs(&events[4]));
        assert_eq!(
            line(&events[2]).unwrap(),
            format!("{:>5.1} s  reading the dump (cached)", secs(&events[2]))
        );
    }

    #[test]
    fn a_stop_makes_the_next_step_or_check_return_stopped() {
        let (run, events) = heard();
        run.step(Step::Placing).unwrap();
        run.check().unwrap();
        run.stopper().stop();
        assert!(stopped(&run.check().unwrap_err()).is_some());
        let err = run.step(Step::Writing).unwrap_err();
        assert!(!stopped(&err).unwrap().took_off);
        assert!(run.reached(Step::Placing) && !run.reached(Step::Writing));
        run.end(&Err::<(), _>(err));
        let events = events.lock().unwrap();
        assert!(matches!(
            &events[events.len() - 2..],
            [
                StepEvent::Ended {
                    step: Step::Placing,
                    ..
                },
                StepEvent::End {
                    end: End::Stopped { took_off: false },
                    ..
                }
            ]
        ));
        assert!(Run::silent().check().is_ok());
    }

    #[test]
    fn writing_the_mod_names_its_target_and_a_failure_keeps_its_error() {
        let (run, events) = heard();
        run.target("t");
        run.step(Step::Writing).unwrap();
        run.end(&Err::<(), _>(anyhow::anyhow!("broke")));
        let events = events.lock().unwrap();
        assert!(matches!(&events[2], StepEvent::Ended { detail, .. } if detail == "t"));
        assert!(
            matches!(&events[3], StepEvent::End { end: End::Failed { error }, .. } if error == "broke")
        );
    }
}
