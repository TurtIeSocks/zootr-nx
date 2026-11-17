//! Config shared by the CLI and the app: the game files, the randomizer
//! settings, and the targets a seed installs to.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::generate::Extras;
use crate::inventory::TrackerOptions;
use crate::platform::{self, Folder, user_dir};
use crate::shuffle::Fixed;

mod mods_root;

/// Overrides where `config.json` lives, for tests and portable setups.
const ENV: &str = "ZOOTR_CONFIG";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct Config {
    /// The game dump: a `.nup`, or an extracted romfs folder. The CLI's
    /// `--game` wins over it.
    pub game: Option<PathBuf>,
    /// Randomizer setting values by id. Missing ids take their default.
    pub settings: BTreeMap<String, serde_json::Value>,
    pub targets: Vec<Target>,
    /// Name of the target Play opens.
    pub picked: Option<String>,
    /// The app's test setup, kept with each seed it builds.
    #[serde(skip_serializing_if = "TestSetup::is_empty")]
    pub test: TestSetup,
    /// Test setups kept by name, to switch between.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub saved_tests: BTreeMap<String, TestSetup>,
    /// The plando draft the next seed gets: chest hash to the item it must
    /// hold. The hashes are JSON keys, so strings, and reach the UI whole.
    #[serde(skip_serializing_if = "Fixed::is_empty")]
    #[cfg_attr(feature = "specta", specta(type = BTreeMap<String, String>))]
    pub place: Fixed,
    /// Plandos kept by name, to load into `place`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "specta", specta(type = BTreeMap<String, BTreeMap<String, String>>))]
    pub plandos: BTreeMap<String, Fixed>,
    /// The Items tab's look. The UI sets it through its own command, so
    /// it's left out of the UI's config.
    #[cfg_attr(feature = "specta", specta(skip))]
    pub tracker: TrackerOptions,
    /// Seeds install the dev panel's hook, and the app's Dev button opens
    /// the panel.
    pub dev_panel: bool,
    /// Draw item icons from the game dump. zootr-nx ships none of the
    /// game's art, so it's off until the person turns it on.
    pub game_assets: bool,
    /// Don't ask GitHub for a newer release. Inverted so the derived
    /// `Default` leaves checks on.
    pub no_update_check: bool,
}

/// Where a new game starts and what it holds, for a playtest: what the CLI's
/// `--spawn`, `--give` and `--default` do.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct TestSetup {
    /// `MAP/SPOT`.
    pub spawn: Option<String>,
    /// Pouch items by actor.
    pub give: Vec<String>,
    /// `NAME=VALUE` new-game edits.
    pub defaults: Vec<String>,
    /// `CHALLENGE=STEP` quest steps to move on, in order. A new game runs
    /// them in its opening and Apply runs them live.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub progress: Vec<String>,
    /// A placed actor of the spawn's map to warp beside once there, when
    /// applied live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near: Option<String>,
    /// Which side of `near` to land on. Front when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<crate::warp::Side>,
    /// The hour to set before the teleport, when applied live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hour: Option<u32>,
    /// `NAME=VALUE` lines that hold once the test passes. Live only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub goals: Vec<String>,
    /// What a flag can't show, like the item a check should give.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub expect: String,
}

impl TestSetup {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn extras(&self) -> Extras {
        Extras {
            defaults: self.defaults.clone(),
            spawn: self.spawn.clone(),
            give: self.give.clone(),
            progress: self.progress.clone(),
            ..Default::default()
        }
    }
}

/// A runner a seed installs to. Nothing here knows which runner it is.
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
    /// The native build's control socket. With it, live reads, dev tools
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
}

impl Target {
    /// Whether the game loads the cheat, which only a settings line that
    /// passes `{cheats}` hands it. Any other game reads the exefs patch.
    pub fn reads_cheats(&self) -> bool {
        (self.settings_file.iter())
            .flat_map(|f| &f.lines)
            .any(|l| l.contains("{cheats}"))
    }

    /// Whether the game is running: answering on its bridge when it has
    /// one, since `app` may only launch it under another name, else a
    /// process started from `app`'s path.
    pub fn running(&self) -> bool {
        match &self.bridge {
            Some(socket) => crate::live::Bridge::listening(socket),
            None => platform::running(&self.app),
        }
    }

    /// `args` with `{game}` filled in.
    pub fn launch_args(&self, game: Option<&Path>) -> Result<Vec<String>> {
        self.args
            .iter()
            .map(|a| match game {
                _ if !a.contains("{game}") => Ok(a.clone()),
                Some(g) => Ok(a.replace("{game}", &g.to_string_lossy())),
                None => bail!(
                    "{} launches with {{game}}: set the game dump first",
                    self.name
                ),
            })
            .collect()
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

    fn pairs(&self, lines: &[String]) -> Result<Vec<(String, String)>> {
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

/// The home folder, for the `~` in shared presets.
fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .find(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// A leading `~` or `~/` becomes `home`.
fn expand_home(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_owned(),
    }
}

/// A path under `home` starts with `~` instead, joined with `/` on every
/// system so a preset reads the same wherever it's shared.
fn collapse_home(path: &Path, home: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix(home) else {
        return path.to_owned();
    };
    let parts: Vec<_> = rest.iter().map(|part| part.to_string_lossy()).collect();
    match parts.is_empty() {
        true => "~".into(),
        false => format!("~/{}", parts.join("/")).into(),
    }
}

impl Target {
    /// A target from a shared preset: `config.json`'s target JSON, with `~`
    /// standing for the home folder.
    pub fn import(text: &str) -> Result<Self> {
        Self::import_at(text, home().as_deref())
    }

    fn import_at(text: &str, home: Option<&Path>) -> Result<Self> {
        let json: serde_json::Value = serde_json::from_str(text).context("not JSON")?;
        let mut target: Target = serde_json::from_value(json.clone())?;
        reject_unknown(&json, &Self::keys(), "target")?;
        if let Some(file) = json.get("settings_file").or(json.get("settings_line")) {
            reject_unknown(file, &["file", "lines", "line"], "settings_file")?;
        }
        if target.name.trim().is_empty() {
            bail!("the target needs a name");
        }
        if let Some(home) = home {
            target.map_paths(|p| expand_home(p, home));
        } else if target.paths().any(|p| p.starts_with("~")) {
            bail!("the preset uses `~` but there is no home folder");
        }
        Ok(target)
    }

    /// The preset's JSON, with the home folder written as `~`.
    pub fn export(&self) -> String {
        let mut target = self.clone();
        if let Some(home) = home() {
            target.map_paths(|p| collapse_home(p, &home));
        }
        serde_json::to_string_pretty(&target).expect("a target is JSON")
    }

    /// Every key a target's JSON takes, aliases too. Built from a full
    /// target, so a new field has to be named here to compile.
    fn keys() -> Vec<&'static str> {
        let Target {
            name,
            app,
            mod_dir,
            settings_file,
            save_dir,
            args,
            control,
            bridge,
            env,
            harness_env,
        } = Target {
            name: String::new(),
            app: PathBuf::new(),
            mod_dir: PathBuf::new(),
            settings_file: None,
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
        };
        let _ = (
            name,
            app,
            mod_dir,
            settings_file,
            save_dir,
            args,
            control,
            bridge,
            env,
            harness_env,
        );
        vec![
            "name",
            "app",
            "mod_dir",
            "settings_file",
            "settings_line",
            "save_dir",
            "args",
            "control",
            "bridge",
            "env",
            "harness_env",
        ]
    }

    fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        let file = self.settings_file.iter().map(|s| &s.file);
        [&self.app, &self.mod_dir]
            .into_iter()
            .chain(self.save_dir.iter())
            .chain(self.control.iter())
            .chain(self.bridge.iter())
            .chain(file)
    }

    fn map_paths(&mut self, f: impl Fn(&Path) -> PathBuf) {
        self.app = f(&self.app);
        self.mod_dir = f(&self.mod_dir);
        for p in [&mut self.save_dir, &mut self.control, &mut self.bridge]
            .into_iter()
            .flatten()
        {
            *p = f(p);
        }
        if let Some(s) = &mut self.settings_file {
            s.file = f(&s.file);
        }
    }

    /// `name`, or `name 2`, `name 3` and so on until no target has it.
    pub fn unique_name(&self, taken: &[Target]) -> String {
        let free = |n: &str| !taken.iter().any(|t| t.name == n);
        let name = self.name.trim();
        if free(name) {
            return name.to_owned();
        }
        (2..)
            .map(|n| format!("{name} {n}"))
            .find(|n| free(n))
            .expect("some number is free")
    }
}

/// An error naming the first key of `json` that isn't in `known`.
fn reject_unknown(json: &serde_json::Value, known: &[&str], what: &str) -> Result<()> {
    if let Some(obj) = json.as_object()
        && let Some(key) = obj.keys().find(|k| !known.contains(&k.as_str()))
    {
        bail!(
            "unknown key {key:?} in the {what} (known: {})",
            known.join(", ")
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SettingsFile {
    pub file: PathBuf,
    /// `KEY=VALUE` lines. In a value `{romfs}` becomes the mod's romfs,
    /// `{cheats}` its cheat folder and `{control}` the target's control
    /// file.
    #[serde(alias = "line", deserialize_with = "one_or_more")]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub lines: Vec<String>,
}

/// A list, or the single string configs held before there could be more.
fn one_or_more<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lines {
        One(String),
        More(Vec<String>),
    }
    Ok(match Lines::deserialize(d)? {
        Lines::One(line) => vec![line],
        Lines::More(lines) => lines,
    })
}

impl SettingsFile {
    /// Set these lines in `file`, replacing lines with the same keys.
    pub fn apply(&self, mod_dir: &Path, control: Option<&Path>) -> Result<()> {
        let fill = |l: &String| {
            let l = [("{romfs}", "romfs"), ("{cheats}", "cheats")]
                .iter()
                .fold(l.clone(), |l, (from, to)| {
                    l.replace(from, &mod_dir.join(to).to_string_lossy())
                });
            match control {
                Some(c) => Ok(l.replace("{control}", &c.to_string_lossy())),
                None if l.contains("{control}") => {
                    bail!(
                        "settings line {l:?} names {{control}}, but the target has no control file"
                    )
                }
                None => Ok(l),
            }
        };
        self.rewrite(self.lines.iter().map(fill).collect::<Result<_>>()?)
    }

    /// Drop these lines' keys from `file`. A missing file stays missing.
    pub fn remove(&self) -> Result<()> {
        self.rewrite(vec![])
    }

    /// `file` without these lines' keys, plus `add`.
    fn rewrite(&self, add: Vec<String>) -> Result<()> {
        let mut keys = vec![];
        for line in &self.lines {
            let Some((key, _)) = line.split_once('=') else {
                bail!("settings line {line:?} has no `=`");
            };
            keys.push(format!("{key}="));
        }
        let old = match std::fs::read_to_string(&self.file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && add.is_empty() => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", self.file.display())),
        };
        let kept = old
            .lines()
            .filter(|l| !keys.iter().any(|k| l.starts_with(k)));
        let lines: Vec<_> = kept.map(str::to_owned).chain(add).collect();
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, lines.join("\n") + "\n")
            .with_context(|| format!("writing {}", self.file.display()))
    }
}

impl Config {
    /// What Generate builds with besides the settings: the test setup and
    /// the plando draft.
    pub fn extras(&self) -> Extras {
        Extras {
            place: self.place.clone(),
            ..self.test.extras()
        }
    }

    pub fn path() -> Result<PathBuf> {
        if let Some(p) = std::env::var_os(ENV) {
            return Ok(p.into());
        }
        Ok(user_dir(Folder::Config, ENV)?.join("config.json"))
    }

    /// The saved config, or the default when there is none yet.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        match std::fs::read(&path) {
            Ok(b) => Self::parse(&b).with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Configs from before the game field held a `romfs` folder instead, and
    /// older settings. Not a serde alias, which would leak into the app's
    /// TypeScript type.
    fn parse(bytes: &[u8]) -> Result<Self> {
        let mut json: serde_json::Value = serde_json::from_slice(bytes)?;
        if let Some(obj) = json.as_object_mut()
            && let Some(romfs) = obj.remove("romfs")
        {
            obj.entry("game").or_insert(romfs);
        }
        let mut config: Self = serde_json::from_value(json)?;
        crate::settings::upgrade(&mut config.settings);
        config.fix_mods_roots();
        Ok(config)
    }

    /// Add an imported target under a name no other target has.
    pub fn add_imported(&mut self, mut target: Target) -> String {
        target.name = target.unique_name(&self.targets);
        let name = target.name.clone();
        self.targets.push(target);
        name
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        std::fs::create_dir_all(path.parent().context("config path has no folder")?)?;
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }

    /// `get` from the picked target, else from the first target that has it.
    fn first<T>(&self, what: &str, get: impl Fn(&Target) -> Option<T>) -> Result<T> {
        let picked = self
            .targets
            .iter()
            .filter(|t| self.picked.as_ref() == Some(&t.name));
        match picked.chain(&self.targets).find_map(get) {
            Some(v) => Ok(v),
            None => bail!(
                "no target has a {what}; pass one or add a target in the app ({})",
                Self::path()?.display()
            ),
        }
    }

    pub fn save_dir(&self) -> Result<PathBuf> {
        self.first("save folder", |t| t.save_dir.clone())
    }

    /// The picked target, else the first.
    pub fn target(&self) -> Result<Target> {
        self.first("game app", |t| Some(t.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, flow, item, map};

    #[test]
    fn extras_carry_the_plando_draft() {
        needs_names!();
        let fixed = Fixed::from([(u64::MAX, item::HOOKSHOT.to_string())]);
        let mut config = Config {
            test: TestSetup {
                give: vec![item::BOMB.to_string()],
                ..Default::default()
            },
            plandos: BTreeMap::from([("Hookshot".into(), fixed.clone())]),
            ..Default::default()
        };
        assert!(config.extras().place.is_empty(), "a saved plando waits");
        config.place = fixed.clone();
        assert_eq!(config.extras().place, fixed);
        assert_eq!(config.extras().give, [item::BOMB], "the test setup stays");
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["place"][u64::MAX.to_string()], item::HOOKSHOT.get());
        assert_eq!(
            json["plandos"]["Hookshot"][u64::MAX.to_string()],
            item::HOOKSHOT.get()
        );
        assert_eq!(serde_json::from_value::<Config>(json).unwrap(), config);
        let bare = serde_json::to_value(Config::default()).unwrap();
        assert!(bare.get("plandos").is_none() && bare.get("place").is_none());
    }

    /// A fresh empty folder under the system temp dir.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn target(name: &str, save_dir: Option<&str>) -> Target {
        Target {
            name: name.into(),
            app: "/a.app".into(),
            mod_dir: format!("/mods/{name}").into(),
            settings_file: None,
            save_dir: save_dir.map(PathBuf::from),
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
        }
    }

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
    fn settings_lines_replace_their_keys_and_keep_the_rest() {
        let dir = scratch("line");
        let file = dir.join("env.txt");
        std::fs::write(&file, "A=1\nKEY=old\nB=2\nC=old\n").unwrap();
        let lines = SettingsFile {
            file: file.clone(),
            lines: vec![
                "KEY={romfs}".into(),
                "C={cheats}".into(),
                "IN={control}".into(),
            ],
        };
        let (m, control) = (Path::new("/m"), Path::new("/c/input.txt"));
        lines.apply(m, Some(control)).unwrap();
        lines.apply(m, Some(control)).unwrap();
        let (romfs, cheats) = (m.join("romfs"), m.join("cheats"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!(
                "A=1\nB=2\nKEY={}\nC={}\nIN={}\n",
                romfs.display(),
                cheats.display(),
                control.display()
            )
        );
        let err = lines.apply(m, None).unwrap_err().to_string();
        assert!(err.contains("no control file"), "{err}");
    }

    #[test]
    fn an_old_single_settings_line_still_loads() {
        let old =
            r#"{"name":"t","app":"/a","mod_dir":"/m","settings_line":{"file":"/e","line":"K=v"}}"#;
        let t: Target = serde_json::from_str(old).unwrap();
        assert_eq!(t.settings_file.unwrap().lines, ["K=v"]);
    }

    #[test]
    fn removing_a_settings_line_keeps_the_rest() {
        let dir = scratch("remove");
        let file = dir.join("env.txt");
        let line = SettingsFile {
            file: file.clone(),
            lines: vec!["KEY={romfs}".into()],
        };
        line.remove().unwrap();
        assert!(!file.exists());
        std::fs::write(&file, "A=1\nKEY=/m/romfs\nB=2\n").unwrap();
        line.remove().unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "A=1\nB=2\n");
    }

    #[test]
    fn settings_line_creates_a_missing_file() {
        let file = scratch("new").join("sub/env.txt");
        let line = SettingsFile {
            file: file.clone(),
            lines: vec!["K=v".into()],
        };
        line.apply(Path::new("/m"), None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "K=v\n");
    }

    #[test]
    fn a_test_setup_is_left_out_until_set_and_becomes_extras() {
        let plain = serde_json::to_value(Config::default()).unwrap();
        assert!(plain.get("test").is_none() && plain.get("saved_tests").is_none());
        let test = TestSetup {
            spawn: Some(format!(
                "{}/{}",
                map::GRAVEYARD_CHILD,
                flag::SHADOW_NOCTURNE
            )),
            give: vec![item::GLASS_TRUTH.to_string()],
            defaults: vec!["A=1".into()],
            progress: vec![
                format!("{}=Step02", flow::SAGE_OF_SPIRIT),
                format!("{}=Step02", flow::GOTO_HYRULE_CASTLE),
            ],
            hour: Some(18),
            near: Some(actor::NPC_POE.to_string()),
            side: Some(crate::warp::Side::Back),
            goals: vec!["B=true".into()],
            expect: "a lens".into(),
        };
        let config = Config {
            test: test.clone(),
            saved_tests: BTreeMap::from([("Well".into(), test.clone())]),
            ..Default::default()
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["test"]["side"], "back");
        let back: Config = serde_json::from_value(json).unwrap();
        assert_eq!(back, config);
        let extras = test.extras();
        let unchecked = TestSetup {
            goals: vec![],
            expect: String::new(),
            ..test.clone()
        };
        assert_eq!(
            test.extras(),
            unchecked.extras(),
            "goals never change a seed"
        );
        assert_eq!(extras.spawn, test.spawn);
        assert_eq!((extras.give, extras.defaults), (test.give, test.defaults));
        assert_eq!(extras.progress, test.progress);
        let bare = serde_json::to_value(TestSetup::default()).unwrap();
        assert!(bare.get("progress").is_none());
        let moved = TestSetup {
            progress: vec!["A=B".into()],
            ..Default::default()
        };
        assert!(
            !moved.is_empty(),
            "a test with only steps still changes a game"
        );
        assert_eq!(moved.extras().progress, ["A=B"]);
        assert!(extras.place.is_empty());
        assert!(TestSetup::default().extras().is_empty());
    }

    #[test]
    fn the_save_folder_comes_from_the_picked_target_then_the_first_with_one() {
        let mut config = Config {
            targets: vec![
                target("a", None),
                target("b", Some("/saves")),
                target("c", Some("/picked")),
            ],
            ..Default::default()
        };
        assert_eq!(config.save_dir().unwrap(), Path::new("/saves"));
        config.picked = Some("c".into());
        assert_eq!(config.save_dir().unwrap(), Path::new("/picked"));
        config.picked = Some("a".into());
        assert_eq!(config.save_dir().unwrap(), Path::new("/saves"));
        assert!(Config::default().save_dir().is_err());
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
        assert!(!t.running());
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(t.running());
        drop(listener);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn round_trips_and_tolerates_missing_fields() {
        let config = Config {
            targets: vec![target("a", Some("/s"))],
            picked: Some("a".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
        assert_eq!(
            serde_json::from_str::<Config>("{}").unwrap(),
            Config::default()
        );
        let old = Config::parse(br#"{"romfs": "/dump/romfs"}"#).unwrap();
        assert_eq!(old.game.as_deref(), Some(Path::new("/dump/romfs")));
    }

    const PRESET: &str = r#"{
        "name": "My runner",
        "app": "~/Games/runner/game",
        "mod_dir": "~/Games/runner/mods/zootr",
        "settings_file": { "file": "~/Games/runner/env.txt", "lines": ["{switch}={romfs}"] },
        "save_dir": "~/Games/runner/save"
    }"#;

    #[test]
    fn import_expands_a_leading_tilde_only() {
        let home = Path::new("/home/me");
        let t = Target::import_at(PRESET, Some(home)).unwrap();
        assert_eq!(t.app, Path::new("/home/me/Games/runner/game"));
        assert_eq!(t.mod_dir, Path::new("/home/me/Games/runner/mods/zootr"));
        assert_eq!(t.save_dir.unwrap(), Path::new("/home/me/Games/runner/save"));
        let file = t.settings_file.unwrap();
        assert_eq!(file.file, Path::new("/home/me/Games/runner/env.txt"));
        assert_eq!(file.lines, ["{switch}={romfs}"]);
        let t =
            Target::import_at(r#"{"name":"a","app":"~","mod_dir":"/x/~/y"}"#, Some(home)).unwrap();
        assert_eq!(t.app, home);
        assert_eq!(t.mod_dir, Path::new("/x/~/y"));
        let t = Target::import_at(r#"{"name":"a","app":"~x/b","mod_dir":"/m"}"#, Some(home));
        assert_eq!(t.unwrap().app, Path::new("~x/b"));
    }

    #[test]
    fn import_names_a_misspelled_key() {
        let text = r#"{"name":"a","app":"/a","mod_dir":"/m","save_dri":"/s"}"#;
        let err = Target::import_at(text, None).unwrap_err().to_string();
        assert!(err.contains("save_dri"), "{err}");
        let text = r#"{"name":"a","app":"/a","mod_dir":"/m","settings_file":{"file":"/f","lines":[],"lnes":1}}"#;
        let err = Target::import_at(text, None).unwrap_err().to_string();
        assert!(err.contains("lnes"), "{err}");
    }

    #[test]
    fn import_takes_the_old_aliases_and_later_fields() {
        let text = r#"{"name":"a","app":"/a","mod_dir":"/m","settings_line":{"file":"/f","line":"K={romfs}"},"args":["-g"],"control":"/c"}"#;
        let t = Target::import_at(text, None).unwrap();
        assert_eq!(t.settings_file.unwrap().lines, ["K={romfs}"]);
        assert_eq!(t.args, ["-g"]);
        assert_eq!(t.control.unwrap(), Path::new("/c"));
    }

    #[test]
    fn a_bridge_and_env_round_trip_and_fill_in() {
        let text = r#"{"name":"n","app":"/a","mod_dir":"/m","bridge":"/run/c.sock","env":["SOCK={bridge}","MEM=rw"]}"#;
        let t = Target::import_at(text, None).unwrap();
        assert_eq!(t.bridge.as_deref(), Some(Path::new("/run/c.sock")));
        let env = t.launch_env().unwrap();
        assert_eq!(env[0], ("SOCK".into(), "/run/c.sock".into()));
        assert_eq!(env[1], ("MEM".into(), "rw".into()));
        assert_eq!(Target::import_at(&t.export(), None).unwrap(), t);
        let old = Target::import_at(r#"{"name":"n","app":"/a","mod_dir":"/m"}"#, None).unwrap();
        assert!(old.bridge.is_none() && old.env.is_empty());
        assert!(!old.export().contains("bridge"));
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

    #[test]
    fn import_rejects_bad_text() {
        assert!(Target::import_at("nope", None).is_err());
        assert!(Target::import_at(r#"{"name":"a"}"#, None).is_err());
        let blank = r#"{"name":" ","app":"/a","mod_dir":"/m"}"#;
        assert!(Target::import_at(blank, None).is_err());
        assert!(Target::import_at(PRESET, None).is_err());
    }

    #[test]
    fn export_writes_the_home_folder_as_a_tilde_and_round_trips() {
        let home = Path::new("/home/me");
        let t = Target::import_at(PRESET, Some(home)).unwrap();
        let mut out = t.clone();
        out.map_paths(|p| collapse_home(p, home));
        let json = serde_json::to_string(&out).unwrap();
        assert!(json.contains("\"~/Games/runner/game\""), "{json}");
        assert!(!json.contains("/home/me"), "{json}");
        assert_eq!(
            collapse_home(Path::new("/home/mex/a"), home),
            Path::new("/home/mex/a")
        );
        assert_eq!(Target::import_at(&json, Some(home)).unwrap(), t);
    }

    #[test]
    fn export_runs_on_the_real_home() {
        let t = target("a", None);
        assert_eq!(Target::import(&t.export()).unwrap(), t);
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let mut config = Config::default();
        let t = target("Runner", None);
        assert_eq!(config.add_imported(t.clone()), "Runner");
        assert_eq!(config.add_imported(t.clone()), "Runner 2");
        assert_eq!(config.add_imported(t), "Runner 3");
        assert_eq!(config.targets.len(), 3);
    }
}
