//! A game that bakes the mods' exefs patches into its build: the command
//! that rebuilds it, and the file its build lists those patches in.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::Target;

/// The file a build lists the exefs patches it applied in, in its game
/// folder.
pub const MANIFEST: &str = "exefs_patches.txt";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Rebuild {
    /// What rebuilds the game, as arguments. `{game}` becomes the game dump.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// The build's patch list, when it isn't in the game folder that holds
    /// `mods/<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<PathBuf>,
}

impl Target {
    /// The build's patch list: `rebuild.manifest`, else the one in the
    /// folder two above `mod_dir`.
    pub fn manifest(&self) -> Option<PathBuf> {
        match self.rebuild.as_ref().and_then(|r| r.manifest.clone()) {
            Some(m) => Some(m),
            None => Some(self.mod_dir.parent()?.parent()?.join(MANIFEST)),
        }
    }

    /// The rebuild command with `{game}` filled in, or `None` when it has
    /// none.
    pub fn rebuild_args(&self, game: Option<&Path>) -> Result<Option<Vec<String>>> {
        match self.rebuild.as_ref().map(|r| &r.command) {
            Some(command) if !command.is_empty() => Ok(Some(self.with_game(command, game)?)),
            _ => Ok(None),
        }
    }

    /// `args` with `{game}` filled in.
    pub(super) fn with_game(&self, args: &[String], game: Option<&Path>) -> Result<Vec<String>> {
        args.iter()
            .map(|a| match game {
                _ if !a.contains("{game}") => Ok(a.clone()),
                Some(g) => Ok(a.replace("{game}", &g.to_string_lossy())),
                None => bail!("{} needs {{game}}: set the game dump first", self.name),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn an_older_config_without_rebuild_loads() {
        let json =
            r#"{"targets": [{"name": "FakeRunner", "app": "/a", "mod_dir": "/g/mods/zootr"}]}"#;
        let config = Config::parse(json.as_bytes()).unwrap();
        let t = &config.targets[0];
        assert_eq!(t.rebuild, None);
        assert_eq!(t.rebuild_args(None).unwrap(), None);
        assert!(!serde_json::to_string(t).unwrap().contains("rebuild"));
    }

    #[test]
    fn the_default_manifest_is_two_above_the_mod_folder() {
        let json = r#"{"name": "FakeRunner", "app": "/a", "mod_dir": "/g/mods/zootr"}"#;
        let mut t = Target::import_at(json, None).unwrap();
        assert_eq!(t.manifest().unwrap(), Path::new("/g/exefs_patches.txt"));
        t.rebuild = Some(Rebuild {
            command: vec![],
            manifest: Some("/elsewhere/list.txt".into()),
        });
        assert_eq!(t.manifest().unwrap(), Path::new("/elsewhere/list.txt"));
        t.mod_dir = "/".into();
        t.rebuild = None;
        assert_eq!(t.manifest(), None);
    }

    #[test]
    fn a_preset_rebuild_fills_in_the_game_and_expands_its_manifest() {
        let json = r#"{"name": "FakeRunner", "app": "/a", "mod_dir": "/m",
            "rebuild": {"command": ["make", "--dump", "{game}"], "manifest": "~/g/list.txt"}}"#;
        let t = Target::import_at(json, Some(Path::new("/home/me"))).unwrap();
        assert_eq!(t.manifest().unwrap(), Path::new("/home/me/g/list.txt"));
        let args = t.rebuild_args(Some(Path::new("/d/oot.nup"))).unwrap();
        assert_eq!(args.unwrap(), ["make", "--dump", "/d/oot.nup"]);
        assert!(
            t.rebuild_args(None)
                .unwrap_err()
                .to_string()
                .contains("{game}")
        );
        let typo = r#"{"name": "a", "app": "/a", "mod_dir": "/m", "rebuild": {"comand": []}}"#;
        let err = Target::import_at(typo, None).unwrap_err().to_string();
        assert!(err.contains("comand"), "{err}");
    }
}
