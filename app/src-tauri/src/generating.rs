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
    /// Write a seed to every target, each step sent to `steps`. A stop that
    /// took the mod off leaves History with nothing installed.
    pub(crate) fn install(
        &self,
        steps: Channel<StepEvent>,
        seed: u64,
        settings: &Settings,
        extras: &Extras,
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
            folders: About::folders(&config.targets),
        });
        *self.log.lock().unwrap() = run.log().map(Into::into);
        *self.stop.lock().unwrap() = Some(run.stopper());
        let mut extras = extras.clone();
        extras.dev_panel = config.dev_panel;
        let report = zootr::game::prepare(game, &run).and_then(|romfs| {
            generate::install(&config.targets, &run, |d| {
                generate::generate(&romfs, d, seed, settings, &extras, &run)
            })
        });
        run.end(&report);
        *self.stop.lock().unwrap() = None;
        if let Err(e) = &report
            && progress::stopped(e).is_some_and(|s| s.took_off)
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
