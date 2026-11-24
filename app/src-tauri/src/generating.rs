//! A seed being generated: its steps sent to the window as they go, its
//! Stop, and its log for Copy log.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use tauri::State;
use tauri::ipc::Channel;
use zootr::config::Config;
use zootr::generate::{self, Extras, Report};
use zootr::history::History;
use zootr::progress::{self, About, Run, StepEvent, Stopper};
use zootr::settings::Settings;

use crate::Res;

/// The run going now, to stop, and the last run's log.
#[derive(Default)]
pub(crate) struct Generating {
    stop: Mutex<Option<Stopper>>,
    log: Mutex<Option<PathBuf>>,
}

impl Generating {
    /// Build a seed, each step sent to `steps`, and write it to every
    /// target when `install`. A stop that took the mod off leaves History
    /// with nothing installed.
    pub(crate) fn build(
        &self,
        steps: Channel<StepEvent>,
        seed: u64,
        settings: &Settings,
        extras: &Extras,
        install: bool,
    ) -> anyhow::Result<Report> {
        *self.log.lock().unwrap() = None;
        let config = Config::load()?;
        let game = config.game.as_ref().context("Set the game dump first")?;
        let run = Self::run(steps).logged(&About {
            seed,
            settings,
            extras,
            game,
            folders: if install {
                About::folders(&config.targets)
            } else {
                Vec::new()
            },
        });
        *self.log.lock().unwrap() = run.log().map(Into::into);
        let report = self.stoppable(&run, || {
            progress::caught(|| {
                let (romfs, release) = config.seed_romfs(game, extras.release, &run)?;
                let write = |d: &std::path::Path| {
                    generate::generate(&romfs, d, seed, settings, extras, &run)
                };
                if install {
                    generate::install(&config.targets, release, &run, write)
                } else {
                    generate::throwaway(write)
                }
            })
        });
        if report.is_ok() {
            let what = if install { "installed" } else { "generated" };
            let share = zootr::share::encode(seed, settings, extras);
            let log = (run.log()).map_or_else(String::new, |l| format!(", log {}", l.display()));
            log::info!("Seed {seed} {what} (share {share}){log}");
        }
        if let Err(e) = &report
            && progress::took_off(e)
            && let Err(e) = History::load().and_then(|mut history| {
                history.install(None);
                history.save()
            })
        {
            log::warn!("Couldn't clear the installed seed: {e:#}");
        }
        report
    }

    /// Rebuild the target named `name`, its output sent to `steps`. A stop
    /// kills the rebuild.
    pub(crate) fn rebuild(&self, steps: Channel<StepEvent>, name: &str) -> anyhow::Result<()> {
        let config = Config::load()?;
        let target = (config.targets.iter())
            .find(|t| t.name == name)
            .with_context(|| format!("No target named {name}"))?;
        let run = Self::run(steps);
        self.stoppable(&run, || {
            zootr::build::rebuild(target, config.game.as_deref(), &run).map(drop)
        })
    }

    /// A run whose every step goes to `steps`.
    fn run(steps: Channel<StepEvent>) -> Run {
        Run::silent().with(move |e| {
            let _ = steps.send(e.clone());
        })
    }

    /// Do `work` as `run`, which Stop Generate stops until it ends.
    fn stoppable<T>(
        &self,
        run: &Run,
        work: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        *self.stop.lock().unwrap() = Some(run.stopper());
        let done = work();
        run.end(&done);
        *self.stop.lock().unwrap() = None;
        done
    }
}

/// Stop the seed being generated, or the game being rebuilt, at its next
/// checkpoint.
#[tauri::command]
#[specta::specta]
pub(crate) fn stop_generate(generating: State<Generating>) {
    if let Some(stop) = &*generating.stop.lock().unwrap() {
        stop.stop();
    }
}

/// The last generate run's log, to paste into a bug report.
#[tauri::command]
#[specta::specta]
pub(crate) fn generate_log(generating: State<Generating>) -> Res<String> {
    let log = generating.log.lock().unwrap().clone();
    let log = log.ok_or("This run wasn't logged")?;
    std::fs::read_to_string(&log).map_err(|e| format!("Couldn't read {}: {e}", log.display()))
}
