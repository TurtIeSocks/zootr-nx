//! Where a seed installs and the game runs, and its preset JSON.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::{Config, Rebuild, SettingsFile};
use crate::game::Release;
use crate::platform;

/// Where a seed installs and the game runs. Nothing here knows which one it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Target {
    pub name: String,
    /// What Play opens, and whose process means the game is running.
    pub app: PathBuf,
    /// The mod folder: install writes `<mod_dir>/romfs`.
    pub mod_dir: PathBuf,
    /// Settings file lines written on install, e.g. `KEY={romfs}`.
    #[serde(
        default,
        alias = "settings_line",
        skip_serializing_if = "Option::is_none"
    )]
    pub settings_file: Option<SettingsFile>,
    /// The game's save folder, holding `slot_NN/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<PathBuf>,
    /// Passed to `app` when Play starts it. `{game}` becomes the game dump.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// A file the game polls for input lines, for the play harness. A
    /// settings line names it with `{control}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<PathBuf>,
    /// The target's control socket. With it, live reads, dev tools
    /// and the play harness go through the socket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<PathBuf>,
    /// `NAME=VALUE` lines set when Play starts `app`. `{bridge}` becomes
    /// the socket.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    /// `NAME=VALUE` lines the play harness adds to `env` when it starts
    /// `app`, never the app's own Play: muted, slower, its own saves.
    /// `{save}` becomes the run's save folder, and a line naming it is left
    /// out when a start has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub harness_env: Vec<String>,
    /// How to rebuild a game that bakes exefs patches in at build time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebuild: Option<Rebuild>,
    /// The turn queue's game this target is part of: one of several slots
    /// on a machine. None is this machine's game (`turn.here`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// The game build this target runs. None takes a seed of either build
    /// (a game that picks its patch by the build IDs in the file names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<Release>,
}

impl Target {
    /// Why a seed for `release` skips this target: it runs another build.
    pub fn skips(&self, release: Release) -> Option<String> {
        let runs = self.release.filter(|&r| r != release)?;
        Some(format!(
            "skipped {}: it runs {}, this seed is for {}",
            self.name,
            runs.name(),
            release.name()
        ))
    }

    /// Whether the game loads the cheat, which only a settings line that
    /// passes `{cheats}` hands it. Any other game reads the exefs patch.
    pub fn reads_cheats(&self) -> bool {
        (self.settings_file.iter())
            .flat_map(|f| &f.lines)
            .any(|l| l.contains("{cheats}"))
    }

    /// Whether the game is running: answering on its bridge when it has
    /// one, since `app` may only launch it under another name, else a
    /// process started from `app`'s path. A socket another build shares
    /// counts only with a process in this build ([`Target::running_among`]).
    pub fn running(&self) -> bool {
        let all = Config::load_all().map(|c| c.targets).unwrap_or_default();
        self.running_among(&all, Path::new(platform::PROC))
    }

    /// `args` with `{game}` filled in.
    pub fn launch_args(&self, game: Option<&Path>) -> Result<Vec<String>> {
        self.with_game(&self.args, game)
    }

    /// `env` as pairs, with `{bridge}` filled in: what the app's Play
    /// starts the game with.
    pub fn launch_env(&self) -> Result<Vec<(String, String)>> {
        Ok(with_save(&self.pairs(&self.env)?, None))
    }

    /// `env` and then `harness_env` as pairs, `{bridge}` filled in and
    /// `{save}` left for [`with_save`]: what the play harness starts the
    /// game with.
    pub fn harness_env(&self) -> Result<Vec<(String, String)>> {
        let mut env = self.pairs(&self.env)?;
        env.extend(self.pairs(&self.harness_env)?);
        Ok(env)
    }

    /// Whether the harness can start the game on a save folder of its own.
    pub fn names_save(&self) -> bool {
        self.harness_env.iter().any(|l| l.contains(SAVE))
    }

    /// Also makes the bridge's folder when a line names it: the game only
    /// serves a socket whose folder exists, and one under `/run/user` is
    /// gone after every reboot.
    fn pairs(&self, lines: &[String]) -> Result<Vec<(String, String)>> {
        if let Some(dir) = self.bridge.as_deref().and_then(Path::parent)
            && lines.iter().any(|l| l.contains("{bridge}"))
        {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("making the bridge's folder {}", dir.display()))?;
        }
        lines
            .iter()
            .map(|line| {
                let (name, value) = line
                    .split_once('=')
                    .with_context(|| format!("env line {line:?} isn't NAME=VALUE"))?;
                let value = match &self.bridge {
                    Some(b) => value.replace("{bridge}", &b.to_string_lossy()),
                    None if value.contains("{bridge}") => {
                        bail!(
                            "env line {line:?} names {{bridge}}, but {} has none",
                            self.name
                        )
                    }
                    None => value.to_owned(),
                };
                Ok((name.to_owned(), value))
            })
            .collect()
    }
}

/// What a harness line names for the run's save folder.
const SAVE: &str = "{save}";

/// `env` with `{save}` filled in by `save`, or the lines naming it left out
/// when there's none.
pub fn with_save(env: &[(String, String)], save: Option<&Path>) -> Vec<(String, String)> {
    env.iter()
        .filter_map(|(name, value)| match save {
            Some(s) => Some((name.clone(), value.replace(SAVE, &s.to_string_lossy()))),
            None if value.contains(SAVE) => None,
            None => Some((name.clone(), value.clone())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::{scratch, target};

    #[test]
    fn launch_args_fill_in_the_game() {
        let t = Target {
            args: vec!["-g".into(), "{game}".into(), "-f".into()],
            ..target("a", None)
        };
        let game = Path::new("/My Games/oot.nup");
        assert_eq!(
            t.launch_args(Some(game)).unwrap(),
            ["-g", "/My Games/oot.nup", "-f"]
        );
        assert!(t.launch_args(None).is_err());
        assert!(target("b", None).launch_args(None).unwrap().is_empty());
    }

    #[test]
    fn a_target_of_another_build_skips_the_seed() {
        let either = target("a", None);
        let base = Target {
            release: Some(Release::V100),
            ..target("FakeBase", None)
        };
        assert_eq!(either.skips(Release::V100), None);
        assert_eq!(either.skips(Release::V111), None);
        assert_eq!(base.skips(Release::V100), None);
        assert_eq!(
            base.skips(Release::V111).unwrap(),
            "skipped FakeBase: it runs 1.0.0, this seed is for 1.1.1"
        );
    }

    #[test]
    fn a_target_without_a_build_still_loads_and_writes_none() {
        let old = Target::import_at(r#"{"name":"n","app":"/a","mod_dir":"/m"}"#, None).unwrap();
        assert_eq!(old.release, None);
        assert!(!old.export().contains("release"));
        let config: Config =
            serde_json::from_str(r#"{"targets":[{"name":"n","app":"/a","mod_dir":"/m"}]}"#)
                .unwrap();
        assert_eq!(config.targets[0].release, None);
        let base = r#"{"name":"n","app":"/a","mod_dir":"/m","release":"1.0.0"}"#;
        let base = Target::import_at(base, None).unwrap();
        assert_eq!(base.release, Some(Release::V100));
        assert_eq!(Target::import_at(&base.export(), None).unwrap(), base);
    }

    #[test]
    fn running_ignores_paths_nothing_runs() {
        let t = Target {
            app: "/nowhere/zootr-test.app (copy)".into(),
            ..target("a", None)
        };
        assert!(!t.running());
    }

    #[cfg(unix)]
    #[test]
    fn a_bridge_target_runs_while_its_socket_answers() {
        let path = std::env::temp_dir().join(format!("zootr-running-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let t = Target {
            bridge: Some(path.clone()),
            ..target("a", None)
        };
        assert!(!t.running_among(&[], Path::new("/nowhere/proc")));
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(t.running_among(&[], Path::new("/nowhere/proc")));
        drop(listener);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_bridge_and_env_round_trip_and_fill_in() {
        let sock = scratch("round-trip").join("c.sock");
        let text = format!(
            r#"{{"name":"n","app":"/a","mod_dir":"/m","bridge":{},"env":["SOCK={{bridge}}","MEM=rw"]}}"#,
            serde_json::to_string(&sock).unwrap()
        );
        let t = Target::import_at(&text, None).unwrap();
        assert_eq!(t.bridge.as_deref(), Some(sock.as_path()));
        let env = t.launch_env().unwrap();
        assert_eq!(env[0], ("SOCK".into(), sock.display().to_string()));
        assert_eq!(env[1], ("MEM".into(), "rw".into()));
        assert_eq!(Target::import_at(&t.export(), None).unwrap(), t);
        let old = Target::import_at(r#"{"name":"n","app":"/a","mod_dir":"/m"}"#, None).unwrap();
        assert!(old.bridge.is_none() && old.env.is_empty());
        assert!(!old.export().contains("bridge"));
    }

    #[test]
    fn a_launch_makes_the_bridge_folder() {
        let run = scratch("bridge-folder").join("zootr");
        let t = Target {
            bridge: Some(run.join("control.sock")),
            env: vec!["SOCK={bridge}".into()],
            ..target("a", None)
        };
        t.launch_env().unwrap();
        assert!(run.is_dir());
        std::fs::remove_dir(&run).unwrap();
        t.harness_env().unwrap();
        assert!(run.is_dir());
    }

    #[test]
    fn the_harness_adds_its_lines_and_fills_or_drops_the_save() {
        let t = Target {
            env: vec!["FOCUS=1".into()],
            harness_env: vec!["SAVES={save}".into(), "FPS=30".into()],
            ..target("a", None)
        };
        assert!(t.names_save() && !target("b", None).names_save());
        let pair = |n: &str, v: &str| (n.to_string(), v.to_string());
        assert_eq!(t.launch_env().unwrap(), [pair("FOCUS", "1")]);
        let env = t.harness_env().unwrap();
        assert_eq!(
            with_save(&env, Some(Path::new("/run/save"))),
            [
                pair("FOCUS", "1"),
                pair("SAVES", "/run/save"),
                pair("FPS", "30")
            ]
        );
        assert_eq!(
            with_save(&env, None),
            [pair("FOCUS", "1"), pair("FPS", "30")]
        );
        let back = Target::import_at(&t.export(), None).unwrap();
        assert_eq!(back.harness_env, t.harness_env);
    }

    #[test]
    fn env_needs_a_bridge_to_name_one_and_an_equals_sign() {
        let mut t = Target::import_at(
            r#"{"name":"n","app":"/a","mod_dir":"/m","env":["S={bridge}"]}"#,
            None,
        )
        .unwrap();
        assert!(t.launch_env().unwrap_err().to_string().contains("{bridge}"));
        t.env = vec!["NOEQUALS".into()];
        assert!(
            t.launch_env()
                .unwrap_err()
                .to_string()
                .contains("NAME=VALUE")
        );
    }
}
