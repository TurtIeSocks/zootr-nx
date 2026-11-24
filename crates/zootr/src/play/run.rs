//! Running a script: the session, the test start, each step, and the
//! watch ticking under them.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};

use super::Input;
use super::report::{Done, Report, Stalled};
use super::script::{Kind, Script, Step};
use super::watch::{self, Stall, Wait};
use crate::config::TestSetup;
use crate::dev::{Dev, Request};
use crate::gamedata::GameData;
use crate::live::{self, Memory, NotSigned, Poll, Unsupported};

/// Attach to the game with `connect`, starting it first unless `attach`,
/// then wait for `ready`. With `attach` the game is taken as it is, with
/// no wait. Without it a game already running is refused, since it may be
/// the maintainer's.
pub fn session<T>(
    input: &mut impl Input,
    attach: bool,
    mut connect: impl FnMut() -> Result<T>,
    ready: impl Fn(&T) -> Result<bool>,
    (limit, tick): (Duration, Duration),
) -> Result<T> {
    if attach {
        return connect().context("no game to attach to");
    }
    ensure!(
        !input.running(),
        "the game is already running, and it may be the maintainer's. Pass --attach to use it"
    );
    input.launch(None)?;
    let start = Instant::now();
    let game = loop {
        match connect() {
            Ok(game) => break game,
            Err(e) if e.is::<NotSigned>() || e.is::<Unsupported>() => return Err(e),
            Err(e) if start.elapsed() > limit => {
                return Err(e.context("the game started but Live never attached"));
            }
            Err(_) => std::thread::sleep(tick),
        }
    };
    let mut last = None;
    loop {
        match ready(&game) {
            Ok(true) => return Ok(game),
            Ok(false) => {}
            Err(e) => last = Some(e),
        }
        if start.elapsed() > limit {
            let never = "the game never finished loading";
            return Err(last.map_or_else(|| anyhow!(never), |e| e.context(never)));
        }
        std::thread::sleep(tick);
    }
}

/// GameData changes and the stall check, ticked between and during steps.
struct Watch<'a, M: Memory> {
    dev: &'a Dev<M>,
    gd: &'a GameData,
    poll: Poll<&'a M>,
    start: Instant,
    stall: Stall,
    limit: Duration,
    changes: Vec<(Duration, crate::savefile::Change)>,
    stalled: Option<Stalled>,
}

impl<M: Memory> Watch<'_, M> {
    /// One look. False once the game has stalled.
    fn tick(&mut self) -> Result<bool> {
        let (changes, _) = self.poll.next(self.gd, false, |_| true)?;
        let at = self.start.elapsed();
        let changed = !changes.is_empty();
        self.changes.extend(changes.into_iter().map(|c| (at, c)));
        let state = self.dev.state()?;
        // The first stall is the one reported: later ticks stay stalled.
        if self.stall.tick(at, state.event, state.loading, changed) && self.stalled.is_none() {
            self.stalled = Some(Stalled {
                at,
                map: state.map,
                quiet: self.limit,
            });
        }
        Ok(self.stalled.is_none())
    }

    /// Tick for `d`, or until a stall.
    fn idle(&mut self, d: Duration) -> Result<bool> {
        let until = Instant::now() + d;
        while Instant::now() < until {
            if !self.tick()? {
                return Ok(false);
            }
            std::thread::sleep(live::TICK);
        }
        self.tick()
    }
}

/// What a run needs besides the game: its script, test start and the
/// spot its `near` warps to, folder and stall limit.
pub struct Run<'a> {
    pub script: &'a Script,
    pub setup: Option<&'a TestSetup>,
    pub near: Option<[f32; 3]>,
    pub dir: &'a Path,
    pub stall: Duration,
}

/// Run `plan.script` on the game and fill `report`. Errors only when the
/// report can't be written, a failing step fails the report instead.
pub fn run<M: Memory>(
    dev: &Dev<M>,
    gd: &GameData,
    input: &mut impl Input,
    plan: &Run,
    mut report: Report,
) -> Result<Report> {
    std::fs::create_dir_all(plan.dir)?;
    if let Err(e) = steps(dev, gd, input, plan, &mut report) {
        report.error = Some(format!("{e:#}"));
    }
    if report.stall.is_some() {
        if let Err(e) = input.dump() {
            report
                .notes
                .push(format!("asking the game what it waits on: {e:#}"));
        }
        shoot(input, plan.dir, "stall.png", &mut report);
    }
    shoot(input, plan.dir, "end.png", &mut report);
    report.write(plan.dir)?;
    Ok(report)
}

fn shoot(input: &mut impl Input, dir: &Path, name: &str, report: &mut Report) {
    match input.shot(&dir.join(name)) {
        Ok(()) => report.shots.push(name.into()),
        Err(e) => report.notes.push(format!("no screenshot {name}: {e:#}")),
    }
}

fn keyed(step: &Step) -> bool {
    matches!(step.kind, Kind::Press(_) | Kind::Hold(..) | Kind::Stick(..))
}

/// The test start, then each step until one fails or the game stalls.
fn steps<M: Memory>(
    dev: &Dev<M>,
    gd: &GameData,
    input: &mut impl Input,
    run: &Run,
    report: &mut Report,
) -> Result<()> {
    if run.script.steps.iter().any(keyed) {
        input.keys().context("the script presses buttons")?;
    }
    if let Some(setup) = run.setup {
        let applied = dev.apply(gd, setup, run.near)?;
        report.setup = applied.done;
        report
            .setup
            .extend(applied.held.iter().map(|g| format!("{g} held before")));
    }
    let mut watch = Watch {
        dev,
        gd,
        poll: Poll::over(dev.game(), dev.global())?,
        start: Instant::now(),
        stall: Stall::new(run.stall),
        limit: run.stall,
        changes: Vec::new(),
        stalled: None,
    };
    each(&mut watch, input, run, report);
    report.changes = std::mem::take(&mut watch.changes);
    report.stall = watch.stalled.take();
    Ok(())
}

fn each<M: Memory>(watch: &mut Watch<M>, input: &mut impl Input, run: &Run, report: &mut Report) {
    for (n, step) in run.script.steps.iter().enumerate() {
        let at = watch.start.elapsed();
        let outcome = one(watch, input, step);
        let failed = outcome.is_err() || watch.stalled.is_some();
        report.steps.push(Done {
            line: step.line,
            what: step.kind.to_string(),
            at,
            outcome: outcome.map_err(|e| format!("{e:#}")),
        });
        if step.shot {
            shoot(
                input,
                run.dir,
                &format!("{:02}-line-{}.png", n + 1, step.line),
                report,
            );
        }
        if failed {
            break;
        }
    }
}

/// Run one step: what it did, or why it failed.
fn one<M: Memory>(watch: &mut Watch<M>, input: &mut impl Input, step: &Step) -> Result<String> {
    let dev = watch.dev;
    let said = match &step.kind {
        Kind::Play(event) => dev.ask(&Request::Play(event.clone()))?.to_string(),
        Kind::Press(button) => input.press(button).map(|()| "pressed".into())?,
        Kind::Hold(button, ms) => input.hold(button, *ms).map(|()| "held".into())?,
        Kind::Stick(direction, ms) => input.stick(direction, *ms).map(|()| "pushed".into())?,
        Kind::Wait(d) => {
            watch.idle(*d)?;
            "waited".into()
        }
        Kind::Shot => "shot".into(),
        Kind::Until(line, within) => held(watch, std::slice::from_ref(line), *within)?,
        Kind::Expect(lines, within) => held(watch, lines, *within)?,
    };
    if !watch.tick()? {
        bail!("the game stalled");
    }
    Ok(said)
}

/// Wait for `lines` to hold, ticking the watch meanwhile.
fn held<M: Memory>(watch: &mut Watch<M>, lines: &[String], within: Duration) -> Result<String> {
    let (dev, gd) = (watch.dev, watch.gd);
    let waited = watch::wait(
        dev.game(),
        gd,
        dev.global(),
        lines,
        (within, live::TICK),
        || watch.tick(),
    )?;
    let shown = |goals: &[crate::dev::Goal]| {
        let each = goals.iter().filter(|g| !g.met).map(|g| {
            let now = g.now.as_deref().or(g.error.as_deref()).unwrap_or("?");
            format!("{} is {now}, not {}", g.name, g.want)
        });
        each.collect::<Vec<_>>().join(", ")
    };
    match waited {
        Wait::Held => Ok("held".into()),
        Wait::TimedOut(goals) => Err(anyhow!(
            "timed out after {} s: {}",
            within.as_secs_f64(),
            shown(&goals)
        )),
        Wait::Failed(goal) => Err(anyhow!("can't read it: {}", shown(&[goal]))),
        Wait::Stopped => bail!("the game stalled"),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;

    use super::*;

    /// A game that's running or not, and starts when launched.
    #[derive(Default)]
    struct Fake {
        running: bool,
        launched: u32,
        lines: Vec<String>,
    }

    impl Input for Fake {
        fn running(&self) -> bool {
            self.running
        }

        fn launch(&mut self, _: Option<&Path>) -> Result<()> {
            self.launched += 1;
            self.running = true;
            Ok(())
        }

        fn keys(&self) -> Result<()> {
            bail!("no control file")
        }

        fn hold(&mut self, button: &str, ms: u64) -> Result<()> {
            self.lines.push(format!("{button}~{ms}"));
            Ok(())
        }

        fn stick(&mut self, direction: &str, ms: u64) -> Result<()> {
            self.hold(direction, ms)
        }

        fn shot(&mut self, to: &Path) -> Result<()> {
            self.lines.push(format!("shot {}", to.display()));
            Ok(())
        }
    }

    const QUICK: (Duration, Duration) = (Duration::from_millis(200), Duration::ZERO);

    #[test]
    fn a_session_refuses_a_running_game_unless_attaching() {
        let mut game = Fake {
            running: true,
            ..Default::default()
        };
        let err = session(&mut game, false, || Ok(()), |_| Ok(true), QUICK).unwrap_err();
        assert!(err.to_string().contains("--attach"), "{err}");
        assert_eq!(game.launched, 0);
        session(&mut game, true, || Ok(()), |_| Ok(true), QUICK).unwrap();
        assert_eq!(game.launched, 0);
    }

    #[test]
    fn a_session_launches_then_waits_for_live_and_the_load() {
        let mut game = Fake::default();
        let (tries, looks) = (Cell::new(0), Cell::new(0));
        let connect = || {
            tries.set(tries.get() + 1);
            ensure!(tries.get() > 2, "not yet");
            Ok(7)
        };
        let ready = |&n: &i32| {
            looks.set(looks.get() + 1);
            Ok(n == 7 && looks.get() > 1)
        };
        assert_eq!(session(&mut game, false, connect, ready, QUICK).unwrap(), 7);
        assert_eq!((game.launched, tries.get(), looks.get()), (1, 3, 2));
        let signed = || -> Result<()> { Err(NotSigned { pid: 1 }.into()) };
        let mut fresh = Fake::default();
        let long = (Duration::from_secs(60), Duration::ZERO);
        let err = session(&mut fresh, false, signed, |_| Ok(true), long).unwrap_err();
        assert!(err.is::<NotSigned>());
        let mut never = Fake::default();
        let err = session(&mut never, false, || Ok(()), |_| Ok(false), QUICK);
        assert!(err.unwrap_err().to_string().contains("never finished"));
        let mut failing = Fake::default();
        let err = session(&mut failing, false, || Ok(()), |_| bail!("no map"), QUICK);
        let err = format!("{:#}", err.unwrap_err());
        assert!(
            err.contains("never finished") && err.contains("no map"),
            "{err}"
        );
    }

    #[test]
    fn only_button_steps_need_keys() {
        let script = Script::parse(
            "name = \"x\"\n[[step]]\nplay = \"E\"\n[[step]]\nshot = true\n[[step]]\nstick = \"up\"\n",
        )
        .unwrap();
        let keyed: Vec<bool> = script.steps.iter().map(keyed).collect();
        assert_eq!(keyed, [false, false, true]);
        let mut report = Report::default();
        let dir = PathBuf::from("/nonexistent");
        let mut game = Fake::default();
        shoot(&mut game, &dir, "end.png", &mut report);
        assert_eq!(report.shots, ["end.png"]);
        assert_eq!(
            game.lines,
            [format!("shot {}", dir.join("end.png").display())]
        );
    }
}
