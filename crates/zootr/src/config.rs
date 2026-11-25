//! Config shared by the CLI and the app: the game files, the randomizer
//! settings, and the targets a seed installs to.

use crate::settings::SettingsVersion;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

use crate::game::Release;
use crate::generate::Extras;
use crate::inventory::TrackerOptions;
use crate::platform::{Folder, user_dir};
use crate::progress::Run;
use crate::shuffle::Fixed;

mod mods_root;
mod preset;
mod rebuild;
mod settings_file;
mod slots;
mod target;

pub use preset::home;
pub use rebuild::Rebuild;
pub use settings_file::SettingsFile;
pub use target::{Target, with_save};

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
    /// Which defaults `settings` and `favorites` were saved under. Loading
    /// brings both up to date, so the UI never sees it.
    #[serde(default = "SettingsVersion::unwritten")]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub settings_version: SettingsVersion,
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
    /// The Colors pane's draft: palette id to Vanilla, Random or a colour.
    /// Random is drawn when a seed is built.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: crate::colors::Draft,
    /// Colour drafts kept by name, to load into `colors`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub color_sets: BTreeMap<String, crate::colors::Draft>,
    /// The colour picker's recent colours, as `#rrggbb`, newest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub color_recents: Vec<String>,
    /// The palettes Randomize all and Reset all leave alone.
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub color_locks: std::collections::BTreeSet<String>,
    /// Setting sets kept by name, newest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub favorites: Vec<crate::settings::favorites::Favorite>,
    /// The Items tab's look. The UI sets it through its own command, so
    /// it's left out of the UI's config.
    #[cfg_attr(feature = "specta", specta(skip))]
    pub tracker: TrackerOptions,
    /// The app shows its developer tools: the dev panel, Test setup, and
    /// the targets' scripted-play fields. Off, a hidden test setup leaves
    /// seeds alone. Every seed carries the dev panel's hook either way, so
    /// hiding the tools never changes the hook patch.
    pub dev_tools: bool,
    /// The turn queue: where it is and the games it hands out.
    #[cfg_attr(feature = "specta", specta(skip))]
    #[serde(skip_serializing_if = "crate::turn::TurnConfig::is_unset")]
    pub turn: crate::turn::TurnConfig,
    /// The queue's other machines, by game name, for what runs there over
    /// ssh (`ap room up --on`, `ap bench --on`).
    #[cfg_attr(feature = "specta", specta(skip))]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub machines: BTreeMap<String, crate::ap::there::Machine>,
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

    /// The part of a seed's extras a test setup holds: what
    /// [`TestSetup::extras`] writes.
    pub fn from_extras(extras: &Extras) -> Self {
        Self {
            spawn: extras.spawn.clone(),
            give: extras.give.clone(),
            defaults: extras.defaults.clone(),
            progress: extras.progress.clone(),
            ..Default::default()
        }
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

impl Config {
    /// What Generate builds with besides the settings: the test setup,
    /// while developer tools show it, and the plando draft.
    pub fn extras(&self) -> Extras {
        let test = match self.dev_tools {
            true => self.test.extras(),
            false => Extras::default(),
        };
        Extras {
            place: self.place.clone(),
            ..test
        }
    }

    pub fn path() -> Result<PathBuf> {
        if let Some(p) = std::env::var_os(ENV) {
            return Ok(p.into());
        }
        Ok(user_dir(Folder::Config, ENV)?.join("config.json"))
    }

    /// The saved config, or the default when there is none yet, with only
    /// the targets of the game this shell plays ([`turn::playing`]): a
    /// command on one slot never sees another's.
    pub fn load() -> Result<Self> {
        let all = Self::load_all()?;
        let game = crate::turn::playing(&all.turn);
        Ok(all.of_game(&game))
    }

    /// The saved config with every game's targets.
    pub fn load_all() -> Result<Self> {
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
        // From before developer tools had a switch: whoever used one keeps
        // them. `dev_panel` was the dev panel's own switch, since dropped.
        // An empty test setup isn't written, so being there means set.
        if let Some(obj) = json.as_object_mut()
            && !obj.contains_key("dev_tools")
        {
            let used = obj.get("dev_panel") == Some(&serde_json::Value::Bool(true))
                || obj.contains_key("test")
                || obj.contains_key("saved_tests");
            obj.insert("dev_tools".into(), used.into());
        }
        let mut config: Self = serde_json::from_value(json)?;
        let from = std::mem::take(&mut config.settings_version);
        crate::settings::upgrade(&mut config.settings, from);
        for f in &mut config.favorites {
            crate::settings::upgrade(&mut f.settings, from);
        }
        config.fix_mods_roots();
        Ok(config)
    }

    /// Add an imported target under a name no other target has, `saved`'s
    /// other games' included: [`Config::save`] drops another game's target
    /// that shares a name with one of this config's.
    pub fn add_imported(&mut self, mut target: Target, saved: &Config) -> String {
        target.name = target.unique_name(self.targets.iter().chain(&saved.targets));
        let name = target.name.clone();
        self.targets.push(target);
        name
    }

    /// Saves, keeping the other games' targets as they are, through a
    /// temporary file so a crash never leaves half a config.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        std::fs::create_dir_all(path.parent().context("config path has no folder")?)?;
        let all = self.with_others(Self::load_all()?, &crate::turn::playing(&self.turn));
        let part = path.with_extension(format!("{}.part", std::process::id()));
        std::fs::write(&part, serde_json::to_vec_pretty(&all)?)
            .and_then(|()| std::fs::rename(&part, &path))
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

    /// The build a seed is made for, short of the dump: `asked` (`--release`,
    /// or a kept seed's), else the picked target's. None leaves it to the
    /// dump's own.
    fn release(&self, asked: Option<Release>) -> Option<Release> {
        asked.or_else(|| self.target().ok()?.release)
    }

    /// The game files a seed is built from out of the dump at `game`, and
    /// the build it's for: [`Config::release`], else the dump's own.
    pub fn seed_romfs(
        &self,
        game: &Path,
        asked: Option<Release>,
        run: &Run,
    ) -> Result<(Romfs, Release)> {
        let romfs = crate::game::prepare(game, self.release(asked), run)?;
        let release = Release::of(romfs.root());
        Ok((romfs, release))
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
            dev_tools: true,
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
    pub(super) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(super) fn target(name: &str, save_dir: Option<&str>) -> Target {
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
            rebuild: None,
            game: None,
            release: None,
        }
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
        let only_seed = TestSetup {
            hour: None,
            near: None,
            side: None,
            ..unchecked
        };
        assert_eq!(
            TestSetup::from_extras(&extras),
            only_seed,
            "a share code's extras read back as what a seed keeps"
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
    fn a_seed_is_for_the_asked_build_then_the_picked_targets_then_the_dumps() {
        let mut config = Config {
            targets: vec![
                target("a", None),
                Target {
                    release: Some(Release::V100),
                    ..target("b", None)
                },
            ],
            ..Default::default()
        };
        assert_eq!(config.release(None), None, "the first target takes either");
        assert_eq!(config.release(Some(Release::V111)), Some(Release::V111));
        config.picked = Some("b".into());
        assert_eq!(config.release(None), Some(Release::V100));
        assert_eq!(
            config.release(Some(Release::V111)),
            Some(Release::V111),
            "--release wins over the target"
        );
        assert_eq!(Config::default().release(None), None, "no target at all");
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
        // A file without a settings version is from before it was kept.
        assert_eq!(
            serde_json::from_str::<Config>("{}").unwrap(),
            Config {
                settings_version: SettingsVersion::unwritten(),
                ..Config::default()
            }
        );
        let old = Config::parse(br#"{"romfs": "/dump/romfs"}"#).unwrap();
        assert_eq!(old.game.as_deref(), Some(Path::new("/dump/romfs")));
    }

    #[test]
    fn settings_saved_before_vanilla_defaults_keep_the_randomizers() {
        use crate::settings::favorites::randomizer;
        let json = br#"{"settings": {"chests": false}, "favorites": [
            {"name": "F", "time": 0, "settings": {}}
        ]}"#;
        let old = Config::parse(json).unwrap();
        let mut want = randomizer();
        want.insert("chests".into(), false.into());
        assert_eq!(old.settings, want);
        assert_eq!(old.favorites[0].settings, randomizer());
        assert_eq!(old.settings_version, SettingsVersion::CURRENT);
        let saved = serde_json::to_vec(&old).unwrap();
        assert_eq!(Config::parse(&saved).unwrap(), old);
        let new = br#"{"settings_version": 2, "settings": {"chests": true}}"#;
        let new = Config::parse(new).unwrap();
        let chests = crate::settings::Settings::from([("chests".into(), true.into())]);
        assert_eq!(new.settings, chests);
    }

    #[test]
    fn a_hidden_test_setup_leaves_seeds_alone() {
        let mut config = Config {
            test: TestSetup {
                give: vec!["FakeItem".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.extras().give.is_empty());
        config.dev_tools = true;
        assert_eq!(config.extras().give, ["FakeItem"]);
    }

    #[test]
    fn configs_from_before_the_switch_keep_tools_they_used() {
        let tools = |json: &str| Config::parse(json.as_bytes()).unwrap().dev_tools;
        assert!(!tools("{}"));
        assert!(!tools(r#"{"dev_panel": false}"#));
        assert!(tools(r#"{"dev_panel": true}"#));
        assert!(tools(r#"{"test": {"give": ["FakeItem"]}}"#));
        assert!(tools(r#"{"saved_tests": {"FakeTest": {}}}"#));
        assert!(!tools(r#"{"dev_tools": false, "dev_panel": true}"#));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let (mut config, saved) = (Config::default(), Config::default());
        let t = target("FakeTarget", None);
        assert_eq!(config.add_imported(t.clone(), &saved), "FakeTarget");
        assert_eq!(config.add_imported(t.clone(), &saved), "FakeTarget 2");
        assert_eq!(config.add_imported(t, &saved), "FakeTarget 3");
        assert_eq!(config.targets.len(), 3);
    }
}
