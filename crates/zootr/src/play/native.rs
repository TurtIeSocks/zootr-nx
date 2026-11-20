//! [`Input`] for a target with a bridge, through its control socket: presses,
//! the stick and shots are requests, each answered once it's done.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::mac::{line, stick};
use super::{Boot, Input};
use crate::config::with_save;
use crate::live::{Bridge, PATIENCE};

/// The game's frames a second, for turning a hold's milliseconds into
/// frames.
const FPS: u64 = 60;

/// The target's game started from `app` with `args` and `env`, driven
/// through the socket at `socket`.
pub struct Native {
    app: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    socket: PathBuf,
    bridge: Option<Bridge>,
}

impl Native {
    pub fn new(
        app: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
        socket: PathBuf,
    ) -> Self {
        Self {
            app,
            args,
            env,
            socket,
            bridge: None,
        }
    }

    /// The connection, made on first use and kept.
    fn bridge(&mut self) -> Result<&Bridge> {
        if self.bridge.is_none() {
            self.bridge = Some(Bridge::connect(&self.socket)?);
        }
        Ok(self.bridge.as_ref().unwrap())
    }

    /// `cmd`, given `held` longer than [`PATIENCE`] to answer. A broken
    /// connection is dropped, so the next request connects again.
    fn send(&mut self, cmd: &str, args: Value, held: Duration) -> Result<Value> {
        let reply = self.bridge()?.ask(cmd, args, PATIENCE + held);
        if reply.is_err() {
            self.bridge = None;
        }
        reply
    }

    /// The game's state, map, whether an event plays or a map loads, and
    /// Link's age, as `status` gives them.
    pub fn status(&mut self) -> Result<Value> {
        self.send("status", Value::Null, Duration::ZERO)
    }
}

/// `ms` as game frames, at least one.
fn frames(ms: u64) -> u64 {
    (ms * FPS).div_ceil(1000).max(1)
}

/// Our button names as the socket spells them: `A+LUp` is `A+LUP`.
fn buttons(names: &str) -> Result<String> {
    line(names, None)?;
    Ok(names.to_uppercase())
}

impl Input for Native {
    fn running(&self) -> bool {
        Bridge::listening(&self.socket)
    }

    fn launch(&mut self, save: Option<&Path>) -> Result<()> {
        crate::platform::command(&self.app)
            .args(&self.args)
            .envs(with_save(&self.env, save))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("starting {}", self.app.display()))?;
        Ok(())
    }

    fn keys(&self) -> Result<()> {
        Ok(())
    }

    fn hold(&mut self, button: &str, ms: u64) -> Result<()> {
        let args = json!({"buttons": buttons(button)?, "frames": frames(ms)});
        self.send("press", args, Duration::from_millis(ms))
            .map(drop)
    }

    fn stick(&mut self, direction: &str, ms: u64) -> Result<()> {
        self.hold(&stick(direction), ms)
    }

    fn shot(&mut self, to: &Path) -> Result<()> {
        let to = std::path::absolute(to)?;
        self.send("shot", json!({"path": to}), Duration::ZERO)
            .map(drop)
    }

    fn quit(&mut self) -> Result<()> {
        // A game already gone answers nothing, which is as good.
        let _ = self.send("quit", Value::Null, Duration::ZERO);
        Ok(())
    }

    fn boot(&mut self) -> Result<Option<Boot>> {
        let status = self.status()?;
        let game = &status["game"];
        Ok(Some(
            if status["state"] == "play" && game["loading"] == false && game["event"] == false {
                Boot::Play
            } else if status["frame"].as_u64().unwrap_or(0) > 0 {
                Boot::Menus
            } else {
                Boot::Starting
            },
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    use super::*;
    use crate::known::map;

    /// A game that answers every request with ok, and hands each request
    /// to the test.
    fn fake(name: &str) -> (Native, mpsc::Receiver<Value>) {
        let path = std::env::temp_dir().join(format!("zootr-native-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let req: Value = serde_json::from_str(&line.unwrap()).unwrap();
                let reply = json!({"id": req["id"], "ok": true, "state": "play", "map": map::KOKIRI_FOREST_CHILD.get()});
                tx.send(req).unwrap();
                writeln!(out, "{reply}").unwrap();
            }
        });
        (Native::new("/none".into(), vec![], vec![], path), rx)
    }

    #[test]
    fn steps_map_to_requests() {
        let (mut game, sent) = fake("steps");
        let next = || {
            let mut req = sent.recv().unwrap();
            req.as_object_mut().unwrap().remove("id");
            req
        };
        game.press("A").unwrap();
        assert_eq!(next(), json!({"cmd": "press", "buttons": "A", "frames": 9}));
        game.hold("ZL+A", 1000).unwrap();
        assert_eq!(
            next(),
            json!({"cmd": "press", "buttons": "ZL+A", "frames": 60})
        );
        game.stick("down-left", 500).unwrap();
        assert_eq!(
            next(),
            json!({"cmd": "press", "buttons": "LDOWN+LLEFT", "frames": 30})
        );
        game.shot(Path::new("/s/one.png")).unwrap();
        assert_eq!(next(), json!({"cmd": "shot", "path": "/s/one.png"}));
        assert_eq!(
            game.status().unwrap()["map"],
            map::KOKIRI_FOREST_CHILD.get()
        );
        assert_eq!(next(), json!({"cmd": "status"}));
    }

    #[test]
    fn a_bad_button_is_caught_before_sending() {
        let (mut game, _) = fake("bad");
        assert!(
            game.press("Q")
                .unwrap_err()
                .to_string()
                .contains("isn't a button")
        );
    }

    #[test]
    fn holds_round_up_to_whole_frames() {
        assert_eq!(
            (frames(0), frames(16), frames(17), frames(150)),
            (1, 1, 2, 9)
        );
    }
}
