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
        let run = Run::silent().with(move |e| {
            let _ = steps.send(e.clone());
        });
        let run = run.logged(&About {
            seed,
            settings,
            extras,
            game,
            folders: match install {
                true => About::folders(&config.targets),
                false => Vec::new(),
            },
        });
        *self.log.lock().unwrap() = run.log().map(Into::into);
        *self.stop.lock().unwrap() = Some(run.stopper());
        let report = progress::caught(|| {
            let romfs = zootr::game::prepare(game, &run)?;
            let write =
                |d: &std::path::Path| generate::generate(&romfs, d, seed, settings, extras, &run);
            match install {
                true => generate::install(&config.targets, &run, write),
                false => generate::throwaway(write),
            }
        });
        run.end(&report);
        *self.stop.lock().unwrap() = None;
        if report.is_ok() {
            let what = if install { "installed" } else { "generated" };
            let share = zootr::share::encode(seed, settings, extras);
            let log = match run.log() {
                Some(log) => format!(", log {}", log.display()),
                None => String::new(),
            };
            log::info!("Seed {seed} {what} (share {share}){log}");
        }
        if let Err(e) = &report
            && progress::took_off(e)
        {
            let mut history = History::load()?;
            history.install(None);
            history.save()?;
        }
        report
    }
}

impl Generating {
    /// Rebuild the target named `name`, its output sent to `steps`. A stop
    /// kills the rebuild.
    pub(crate) fn rebuild(&self, steps: Channel<StepEvent>, name: &str) -> anyhow::Result<()> {
        let config = Config::load()?;
        let target = (config.targets.iter())
            .find(|t| t.name == name)
            .with_context(|| format!("No target named {name}"))?;
        let run = Run::silent().with(move |e| {
            let _ = steps.send(e.clone());
        });
        *self.stop.lock().unwrap() = Some(run.stopper());
        let done = zootr::build::rebuild(target, config.game.as_deref(), &run).map(drop);
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
