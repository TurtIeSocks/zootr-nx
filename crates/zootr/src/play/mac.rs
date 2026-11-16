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
use crate::config::with_save;
use crate::{live, platform};

/// The names a control line takes, joined with `+`.
const BUTTONS: [&str; 24] = [
    "A", "B", "X", "Y", "L", "R", "ZL", "ZR", "Plus", "Minus", "Left", "Up", "Right", "Down", "LS",
    "RS", "LLeft", "LUp", "LRight", "LDown", "RLeft", "RUp", "RRight", "RDown",
];
/// How long the game gets to write a screenshot.
const SHOT: Duration = Duration::from_secs(10);
/// How long the game gets to quit.
const QUIT: Duration = Duration::from_secs(30);
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
        }
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
    for b in buttons.split('+') {
        ensure!(
            BUTTONS.contains(&b),
            "{b} isn't a button. Buttons are {}",
            BUTTONS.join(" ")
        );
    }
    Ok(match ms {
        Some(ms) => format!("{buttons}~{}", ms as f64 / 1000.0),
        None => buttons.into(),
    })
}

/// The left stick's buttons for a direction like `up` or `down-left`.
/// Other names, like `RUp`, pass through.
pub fn stick(direction: &str) -> String {
    let parts = direction.split(['-', '+']).map(|d| match d {
        "up" => "LUp",
        "down" => "LDown",
        "left" => "LLeft",
        "right" => "LRight",
        other => other,
    });
    parts.collect::<Vec<_>>().join("+")
}

impl Input for Mac {
    fn running(&self) -> bool {
        platform::running(&self.app)
    }

    fn launch(&mut self, save: Option<&Path>) -> Result<()> {
        platform::open_app(&self.app, &self.args, &with_save(&self.env, save))
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
        let start = Instant::now();
        while self.running() {
            ensure!(start.elapsed() < QUIT, "the game didn't quit");
            std::thread::sleep(Duration::from_millis(500));
        }
        Ok(())
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
