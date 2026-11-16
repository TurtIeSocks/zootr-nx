//! `zootr play`: a script run on the game, on the Mac or the native build.
//! `zootr sweep`: a night of rounds on the native build.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;

use super::args::StartArgs;
use super::running::saved_test;
use crate::config::Config;
use crate::dev::Dev;
use crate::gifts;
use crate::history::History;
use crate::live::{self, Game};
use crate::play::{self, Launch, Report, Run, Script, Start, boot};
use crate::sweep::judge::ModelJudge;
use crate::sweep::night::{self, Night};
use crate::turn::{self, Turn};
use crate::vision::Vision;
use crate::{kept, recorder, warp};

/// How long the game gets to start and load.
const LAUNCH: Duration = Duration::from_secs(180);

/// `zootr boot`: the game started on a kept save or a new file, into play.
pub(super) fn boot_command(start: StartArgs, limit: u64) -> Result<()> {
    let launch = Launch::named(start.save.clone(), start.new)?
        .context("name a start: --save NAME or --new")?;
    let config = Config::load()?;
    let target = config.target()?;
    let mut input = play::input(&target, config.game.as_deref())?;
    let begun = Instant::now();
    let save = boot_on(
        &mut input,
        &target,
        &launch,
        &start,
        Duration::from_secs(limit),
    )?;
    let map = Dev::new(Game::attach(&target)?).and_then(|dev| dev.state());
    let map = map.map(|s| s.map).unwrap_or_else(|_| "?".into());
    println!("in play on {map} after {} s", begun.elapsed().as_secs());
    println!("save folder: {}", save.display());
    Ok(())
}

/// Start the game on `launch` from a run folder of its own, on this
/// session's turn, and return that folder. Once Link is in play the bottle
/// tutorial is marked done, unless `start` keeps it.
fn boot_on(
    input: &mut impl play::Input,
    target: &crate::config::Target,
    launch: &Launch,
    start: &StartArgs,
    limit: Duration,
) -> Result<PathBuf> {
    Turn::load()?.ensure_held(&start.session, turn::from_claude())?;
    let (from, name) = match launch {
        Launch::Kept(name) => (Start::Save(kept::path(name)?), name.as_str()),
        Launch::New => (Start::New, "new"),
    };
    let save = boot::prepare(&from, name)?;
    boot::boot(input, target, Some(&save), limit)?;
    if !start.bottle_tutorial {
        let game = Game::attach(target)?;
        for flag in gifts::BOTTLE_TUTORIAL_DONE {
            game.set(live::hash(flag)?, "true")?;
        }
    }
    Ok(save)
}

pub(super) fn play(
    romfs: &Romfs,
    script: &Path,
    attach: bool,
    stall: u64,
    start: StartArgs,
) -> Result<()> {
    let path = script;
    let script = Script::load(path)?;
    let setup = match &script.test {
        Some(name) => Some(saved_test(name)?),
        None => script.setup.clone(),
    };
    let gd = recorder::gamedata(romfs)?;
    let config = Config::load()?;
    let target = config.target()?;
    let mut input = play::input(&target, config.game.as_deref())?;
    let launch = match attach {
        true => None,
        false => Launch::named(start.save.clone(), start.new)?.or(script.launch.clone()),
    };
    if let Some(launch) = &launch {
        boot_on(&mut input, &target, launch, &start, boot::LIMIT)?;
    }
    let attach = attach || launch.is_some();
    let connect = || Dev::new(Game::attach(&target)?);
    let ready = |dev: &Dev| Ok(dev.state().is_ok_and(|s| !s.loading && !s.map.is_empty()));
    let dev = play::session(&mut input, attach, connect, ready, (LAUNCH, live::TICK))?;
    let dir = play::folder("play", &script.name)?;
    let report = Report {
        name: script.name.clone(),
        script: path.to_path_buf(),
        seed: seed(),
        started: jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S").to_string(),
        ..Default::default()
    };
    let near = match &setup {
        Some(setup) => warp::near(romfs, setup, || Ok(dev.state()?.map))?,
        None => None,
    };
    let run = Run {
        script: &script,
        setup: setup.as_ref(),
        near,
        dir: &dir,
        stall: Duration::from_secs(stall),
    };
    let report = play::run(&dev, &gd, &mut input, run, report)?;
    println!("{}", report.verdict());
    println!("{}", dir.join("report.md").display());
    if !report.passed() {
        bail!("the run failed");
    }
    Ok(())
}

/// `zootr look`: the target's frame, or a saved `frame`, asked about.
pub(super) fn look(
    vision: &Vision,
    questions: &[String],
    keep: Option<&Path>,
    frame: Option<&Path>,
) -> Result<()> {
    std::fs::metadata(&vision.key_file)
        .with_context(|| format!("reading {}", vision.key_file.display()))?;
    let looked = match frame {
        Some(frame) => crate::look::look_at(vision, frame, questions)?,
        None => {
            let config = Config::load()?;
            let mut game = play::input(&config.target()?, config.game.as_deref())?;
            crate::look::look(vision, &mut *game, questions, keep)?
        }
    };
    println!("{}", serde_json::to_string(&looked)?);
    Ok(())
}

/// The installed seed, as the app's history names it.
fn seed() -> String {
    let Ok(history) = History::load() else {
        return "unknown, the history couldn't be read".into();
    };
    match history.installed.map(|id| (id, history.get(id))) {
        Some((id, Ok(e))) if !e.label.name.is_empty() => format!("#{id}, {}", e.label.name),
        Some((id, _)) => format!("#{id}"),
        None => "none installed".into(),
    }
}

pub(super) fn sweep(
    romfs: &Romfs,
    game: &Path,
    judge: ModelJudge,
    dir: PathBuf,
    (rounds, limit, mix, only): (usize, Option<usize>, Option<String>, Option<String>),
    pristine: Option<PathBuf>,
) -> Result<()> {
    let mix = mix.map(|name| night::pinned(&name)).transpose()?;
    let config = Config::load()?;
    let target = config.target()?;
    let input = play::input(&target, config.game.as_deref())?;
    let gd = recorder::gamedata(romfs)?;
    let mut night = Night {
        romfs,
        game,
        gd: &gd,
        target: &target,
        input,
        judge,
        dir,
        pristine,
        limit,
        mix,
        only,
    };
    night.run(rounds)
}
