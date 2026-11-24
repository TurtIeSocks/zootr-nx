//! [`Input`] for a game on the Mac. With a control file, buttons, the stick
//! and screenshots are lines appended to it, which the game polls. Without
//! one there are no keys, and screenshots come from `screencapture`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use super::Input;
use super::input::{check_buttons, gone, stick};
use crate::config::with_save;
use crate::{live, platform};

/// How long the game gets to write a screenshot.
const SHOT: Duration = Duration::from_secs(10);
const LOOK: Duration = Duration::from_millis(100);

/// The game's front window by owner pid, through the system's own
/// scripting bridge. Prints its window number.
const WINDOW: &str = r#"
ObjC.import('CoreGraphics');
function run(argv) {
  const pid = Number(argv[0]);
  const info = $.CGWindowListCopyWindowInfo($.kCGWindowListOptionAll, 0);
  const list = ObjC.deepUnwrap(ObjC.castRefToObject(info));
  const w = list.find(w => w.kCGWindowOwnerPID === pid && w.kCGWindowLayer === 0);
  return w ? String(w.kCGWindowNumber) : '';
}
"#;

/// A game started from `app` with `args` and the harness's `env`, taking
/// input from `control`.
pub struct Mac {
    app: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    control: Option<PathBuf>,
    /// The game's own log, where it writes how a run ended.
    log: Option<PathBuf>,
}

impl Mac {
    pub fn new(
        app: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
        control: Option<PathBuf>,
    ) -> Self {
        Self {
            app,
            args,
            env,
            control,
            log: None,
        }
    }

    /// This game, reading how a run ended from `log`.
    pub fn logging(self, log: Option<PathBuf>) -> Self {
        Self { log, ..self }
    }

    fn control(&self) -> Result<&Path> {
        (self.control.as_deref()).context(
            "the target has no control file, so it takes no keys. Set control on the target",
        )
    }

    fn send(&self, line: &str) -> Result<()> {
        let path = self.control()?;
        let mut file = (OpenOptions::new().create(true).append(true))
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        writeln!(file, "{line}").with_context(|| format!("writing {}", path.display()))
    }

    /// The window number of the game's main window.
    fn window(&self) -> Result<String> {
        let pid = *live::pids(&self.app)?
            .first()
            .context("the game isn't running")?;
        let out = Command::new("osascript")
            .args(["-l", "JavaScript", "-e", WINDOW, &pid.to_string()])
            .output()?;
        ensure!(out.status.success(), "osascript exited with {}", out.status);
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        ensure!(!id.is_empty(), "the game has no window");
        Ok(id)
    }
}

/// A control line pressing `buttons` (joined with `+`) for `ms`, or for
/// the game's default when `None`.
pub fn line(buttons: &str, ms: Option<u64>) -> Result<String> {
    check_buttons(buttons)?;
    Ok(match ms {
        Some(ms) => format!("{buttons}~{}", ms as f64 / 1000.0),
        None => buttons.into(),
    })
}

/// The log a Mac app keeps beside the game's saves at `save`
/// (`Library/Application Support/<app>/<title>/save`): by the platform's
/// layout, `Library/Logs/<app>/<title>.log`.
pub fn log_beside(save: &Path) -> Option<PathBuf> {
    let title = save.parent()?;
    let app = title.parent()?;
    let support = app.parent()?;
    if support.file_name()? != "Application Support" {
        return None;
    }
    let name = format!("{}.log", title.file_name()?.to_str()?);
    Some(
        support
            .parent()?
            .join("Logs")
            .join(app.file_name()?)
            .join(name),
    )
}

/// The lines of the game's log that say how its run ended: its
/// crash line, then the launcher's exit code.
fn crash_lines(log: &str) -> Option<String> {
    let last = |text| log.lines().rev().find(|l| l.contains(text));
    let (crash, exit) = (last("] CRASH "), last("host exited"));
    let lines: Vec<_> = [crash, exit].into_iter().flatten().map(str::trim).collect();
    (!lines.is_empty()).then(|| lines.join("; "))
}

impl Input for Mac {
    fn running(&self) -> bool {
        platform::running(&self.app)
    }

    fn crash(&self) -> Option<String> {
        let log = self.log.as_ref()?;
        // The host flushes its log as it goes, a moment after it's gone.
        let found = (0..10).find_map(|try_| {
            std::thread::sleep(Duration::from_millis(500 * u64::from(try_ > 0)));
            crash_lines(&String::from_utf8_lossy(&std::fs::read(log).ok()?))
        })?;
        Some(format!("{found} (log: {})", log.display()))
    }

    fn launch(&mut self, save: Option<&Path>) -> Result<()> {
        let env = with_save(&self.env, save);
        platform::open_game(&self.app, &self.args, &env, self.control.as_deref())
    }

    /// Every process of the app, the game and its host alike: a host left
    /// behind makes the next install say the game still runs.
    fn quit(&mut self) -> Result<()> {
        let pids = live::pids(&self.app)?;
        if !pids.is_empty() {
            Command::new("kill")
                .arg("-TERM")
                .args(pids.iter().map(i32::to_string))
                .status()?;
        }
        gone(self)
    }

    fn keys(&self) -> Result<()> {
        self.control().map(drop)
    }

    fn press(&mut self, button: &str) -> Result<()> {
        self.send(&line(button, None)?)
    }

    fn hold(&mut self, button: &str, ms: u64) -> Result<()> {
        self.send(&line(button, Some(ms))?)?;
        std::thread::sleep(Duration::from_millis(ms));
        Ok(())
    }

    fn stick(&mut self, direction: &str, ms: u64) -> Result<()> {
        self.hold(&stick(direction), ms)
    }

    fn shot(&mut self, to: &Path) -> Result<()> {
        if self.control.is_none() {
            let window = self.window()?;
            let status = (Command::new("screencapture"))
                .args([format!("-l{window}"), "-x".into()])
                .arg(to)
                .status()?;
            ensure!(status.success(), "screencapture exited with {status}");
            return Ok(());
        }
        let _ = std::fs::remove_file(to);
        let to = std::path::absolute(to)?;
        self.send(&format!("shot {}", to.display()))?;
        let start = Instant::now();
        // The game may still be writing the file when it appears.
        let mut size = None;
        loop {
            let now = std::fs::metadata(&to)
                .ok()
                .map(|m| m.len())
                .filter(|&n| n > 0);
            if now.is_some() && now == size {
                return Ok(());
            }
            if start.elapsed() > SHOT {
                bail!("the game wrote no screenshot in {} s", SHOT.as_secs());
            }
            size = now;
            std::thread::sleep(LOOK);
        }
    }

    fn queue(&mut self, line: &str) -> Result<()> {
        self.send(line)
    }

    fn dump(&mut self) -> Result<()> {
        match self.control {
            Some(_) => self.send("dump"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_sits_in_logs_beside_the_saves() {
        let save = Path::new("/h/Library/Application Support/FakeRunner/0100/save");
        assert_eq!(
            log_beside(save).unwrap(),
            Path::new("/h/Library/Logs/FakeRunner/0100.log")
        );
        assert_eq!(log_beside(Path::new("/h/Games/FakeRunner/0100/save")), None);
    }

    #[test]
    fn a_crash_reads_as_its_crash_line_and_exit() {
        let log = "[dm] frame\n[dm] CRASH sig=11 addr=0 pc=main+0x10\nx0=1\nlauncher: the host exited with 139\n";
        assert_eq!(
            crash_lines(log).unwrap(),
            "[dm] CRASH sig=11 addr=0 pc=main+0x10; launcher: the host exited with 139"
        );
        assert_eq!(crash_lines("[dm] frame\n"), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-play-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn each_step_appends_its_line_to_the_control_file() {
        let dir = scratch("control");
        let control = dir.join("input.txt");
        let mut game = Mac::new(dir.join("none.app"), vec![], vec![], Some(control.clone()));
        game.keys().unwrap();
        game.press("A").unwrap();
        game.press("A+B").unwrap();
        game.hold("ZL", 800).unwrap();
        game.stick("up", 150).unwrap();
        game.stick("down-left", 1000).unwrap();
        game.stick("RUp", 250).unwrap();
        game.dump().unwrap();
        let shot = dir.join("one.png");
        // The game's side: write the frame the line asks for.
        let writer = std::thread::spawn({
            let (control, shot) = (control.clone(), shot.clone());
            move || {
                while !std::fs::read_to_string(&control).unwrap().contains("shot ") {
                    std::thread::sleep(LOOK);
                }
                std::fs::write(shot, b"png").unwrap();
            }
        });
        game.shot(&shot).unwrap();
        writer.join().unwrap();
        let lines = std::fs::read_to_string(&control).unwrap();
        let shot = std::path::absolute(&shot).unwrap();
        assert_eq!(
            lines,
            format!(
                "A\nA+B\nZL~0.8\nLUp~0.15\nLDown+LLeft~1\nRUp~0.25\ndump\nshot {}\n",
                shot.display()
            )
        );
    }

    #[test]
    fn a_bad_button_or_no_control_file_is_refused() {
        let dir = scratch("bad");
        let control = dir.join("input.txt");
        let mut game = Mac::new(dir.join("none.app"), vec![], vec![], Some(control.clone()));
        let err = game.press("Start").unwrap_err().to_string();
        assert!(err.contains("Start isn't a button"), "{err}");
        assert!(!control.exists());
        let mut bare = Mac::new(dir.join("none.app"), vec![], vec![], None);
        assert!(bare.keys().unwrap_err().to_string().contains("control"));
        assert!(bare.press("A").is_err());
        bare.dump().unwrap();
    }
}
