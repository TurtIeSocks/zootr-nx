//! A target as a shared preset: its JSON, with `~` for the home folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Target;

/// The home folder, for the `~` in shared presets and logs.
pub fn home() -> Option<PathBuf> {
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

    pub(super) fn import_at(text: &str, home: Option<&Path>) -> Result<Self> {
        let json: serde_json::Value = serde_json::from_str(text).context("not JSON")?;
        let mut target: Target = serde_json::from_value(json.clone())?;
        reject_unknown(&json, &Self::KEYS, "target")?;
        if let Some(file) = json.get("settings_file").or(json.get("settings_line")) {
            reject_unknown(file, &Self::SETTINGS_FILE_KEYS, "settings_file")?;
        }
        if let Some(rebuild) = json.get("rebuild") {
            reject_unknown(rebuild, &Self::REBUILD_KEYS, "rebuild")?;
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

    /// Every key a target's JSON takes, aliases too.
    /// `every_key_a_target_writes_is_known` keeps it in step with the fields.
    const KEYS: [&str; 14] = [
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
        "rebuild",
        "game",
        "release",
    ];
    /// [`Target::KEYS`] for a settings file.
    const SETTINGS_FILE_KEYS: [&str; 3] = ["file", "lines", "line"];
    /// [`Target::KEYS`] for a rebuild.
    const REBUILD_KEYS: [&str; 2] = ["command", "manifest"];

    pub(super) fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        let file = self.settings_file.iter().map(|s| &s.file);
        [&self.app, &self.mod_dir]
            .into_iter()
            .chain(self.save_dir.iter())
            .chain(self.control.iter())
            .chain(self.bridge.iter())
            .chain(self.rebuild.iter().filter_map(|r| r.manifest.as_ref()))
            .chain(file)
    }

    pub(super) fn map_paths(&mut self, f: impl Fn(&Path) -> PathBuf) {
        self.app = f(&self.app);
        self.mod_dir = f(&self.mod_dir);
        let manifest = self.rebuild.as_mut().and_then(|r| r.manifest.as_mut());
        for p in [&mut self.save_dir, &mut self.control, &mut self.bridge]
            .into_iter()
            .flatten()
            .chain(manifest)
        {
            *p = f(p);
        }
        if let Some(s) = &mut self.settings_file {
            s.file = f(&s.file);
        }
    }

    /// `name`, or `name 2`, `name 3` and so on until none of `taken` has it.
    pub fn unique_name<'a>(&self, taken: impl IntoIterator<Item = &'a Target>) -> String {
        let taken: Vec<&str> = taken.into_iter().map(|t| t.name.as_str()).collect();
        let free = |n: &str| !taken.contains(&n);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::target;
    use crate::config::{Rebuild, SettingsFile};
    use crate::game::Release;

    const PRESET: &str = r#"{
        "name": "My target",
        "app": "~/Games/target/game",
        "mod_dir": "~/Games/target/mods/zootr",
        "settings_file": { "file": "~/Games/target/env.txt", "lines": ["{switch}={romfs}"] },
        "save_dir": "~/Games/target/save"
    }"#;

    #[test]
    fn import_expands_a_leading_tilde_only() {
        let home = Path::new("/home/me");
        let t = Target::import_at(PRESET, Some(home)).unwrap();
        assert_eq!(t.app, Path::new("/home/me/Games/target/game"));
        assert_eq!(t.mod_dir, Path::new("/home/me/Games/target/mods/zootr"));
        assert_eq!(t.save_dir.unwrap(), Path::new("/home/me/Games/target/save"));
        let file = t.settings_file.unwrap();
        assert_eq!(file.file, Path::new("/home/me/Games/target/env.txt"));
        assert_eq!(file.lines, ["{switch}={romfs}"]);
        let t =
            Target::import_at(r#"{"name":"a","app":"~","mod_dir":"/x/~/y"}"#, Some(home)).unwrap();
        assert_eq!(t.app, home);
        assert_eq!(t.mod_dir, Path::new("/x/~/y"));
        let t = Target::import_at(r#"{"name":"a","app":"~x/b","mod_dir":"/m"}"#, Some(home));
        assert_eq!(t.unwrap().app, Path::new("~x/b"));
    }

    #[test]
    fn every_key_a_target_writes_is_known() {
        let full = Target {
            name: "a".into(),
            app: "/a".into(),
            mod_dir: "/m".into(),
            settings_file: Some(SettingsFile {
                file: "/f".into(),
                lines: vec!["K=V".into()],
            }),
            save_dir: Some("/s".into()),
            args: vec!["-g".into()],
            control: Some("/c".into()),
            bridge: Some("/b".into()),
            env: vec!["A=1".into()],
            harness_env: vec!["B=1".into()],
            rebuild: Some(Rebuild {
                command: vec!["make".into()],
                manifest: Some("/p".into()),
            }),
            game: Some("FakeHost".into()),
            release: Some(Release::V100),
        };
        let json = serde_json::to_value(&full).unwrap();
        let keys = |v: &serde_json::Value| -> Vec<String> {
            v.as_object().unwrap().keys().cloned().collect()
        };
        for (value, known) in [
            (&json, &Target::KEYS[..]),
            (&json["settings_file"], &Target::SETTINGS_FILE_KEYS[..]),
            (&json["rebuild"], &Target::REBUILD_KEYS[..]),
        ] {
            for key in keys(value) {
                assert!(known.contains(&key.as_str()), "{key} isn't in {known:?}");
            }
        }
        assert_eq!(keys(&json).len(), 13, "a field went unwritten");
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
        assert!(json.contains("\"~/Games/target/game\""), "{json}");
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
}
