//! A game that bakes the mods' exefs patches into its build: the command
//! that rebuilds it, and the file its build lists those patches in.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
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

    /// Whether `other`, another target, plays the same build: the same
    /// patch list, as every slot of a machine does.
    pub fn shares_build(&self, other: &Target) -> bool {
        other.name != self.name
            && self.rebuild.is_some()
            && other.rebuild.is_some()
            && other.manifest() == self.manifest()
    }

    /// The build's folder, symlinks resolved: the one its patch list is in.
    pub fn build_folder(&self) -> Option<PathBuf> {
        std::fs::canonicalize(self.manifest()?.parent()?).ok()
    }

    /// [`Target::running`], `all` being every target saved and `proc` the
    /// process list. A bridge socket another build's target shares (a
    /// machine's own game, two builds on one socket) answers for a game of
    /// either, so there the game runs only while a process works in this
    /// build's folder or names a path inside it. Without a process list
    /// or a build folder, the socket answering still counts.
    pub fn running_among(&self, all: &[Target], proc: &Path) -> bool {
        let Some(socket) = &self.bridge else {
            return crate::platform::running(&self.app);
        };
        let shared = (all.iter()).any(|o| {
            o.name != self.name && o.bridge == self.bridge && o.manifest() != self.manifest()
        });
        crate::live::Bridge::listening(socket)
            && match (shared, self.build_folder()) {
                (true, Some(folder)) => crate::platform::inside(proc, &folder).unwrap_or(true),
                _ => true,
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
                None => Err(anyhow!("{} needs {{game}}", self.name)
                    .context(crate::plain::Plain("Choose your game dump first".into()))),
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
            r#"{"targets": [{"name": "FakeTarget", "app": "/a", "mod_dir": "/g/mods/zootr"}]}"#;
        let config = Config::parse(json.as_bytes()).unwrap();
        let t = &config.targets[0];
        assert_eq!(t.rebuild, None);
        assert_eq!(t.rebuild_args(None).unwrap(), None);
        assert!(!serde_json::to_string(t).unwrap().contains("rebuild"));
    }

    #[test]
    fn the_default_manifest_is_two_above_the_mod_folder() {
        let json = r#"{"name": "FakeTarget", "app": "/a", "mod_dir": "/g/mods/zootr"}"#;
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
        let json = r#"{"name": "FakeTarget", "app": "/a", "mod_dir": "/m",
            "rebuild": {"command": ["make", "--dump", "{game}"], "manifest": "~/g/list.txt"}}"#;
        let t = Target::import_at(json, Some(Path::new("/home/me"))).unwrap();
        assert_eq!(t.manifest().unwrap(), Path::new("/home/me/g/list.txt"));
        let args = t.rebuild_args(Some(Path::new("/d/oot.nup"))).unwrap();
        assert_eq!(args.unwrap(), ["make", "--dump", "/d/oot.nup"]);
        assert!(format!("{:#}", t.rebuild_args(None).unwrap_err()).contains("{game}"));
        let typo = r#"{"name": "a", "app": "/a", "mod_dir": "/m", "rebuild": {"comand": []}}"#;
        let err = Target::import_at(typo, None).unwrap_err().to_string();
        assert!(err.contains("comand"), "{err}");
    }

    /// Two builds' targets on one socket, its game answering, and a fake
    /// process list.
    #[cfg(unix)]
    #[test]
    fn a_shared_socket_runs_only_the_build_a_process_is_in() {
        let root = std::env::temp_dir().join(format!("zootr-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (proc, socket) = (root.join("proc"), root.join("control.sock"));
        let build = |name: &str| {
            std::fs::create_dir_all(root.join(name).join("mods/zootr")).unwrap();
            Target {
                name: name.into(),
                app: root.join("launcher"),
                mod_dir: root.join(name).join("mods/zootr"),
                bridge: Some(socket.clone()),
                ..Target::import_at(r#"{"name": "a", "app": "/a", "mod_dir": "/m"}"#, None).unwrap()
            }
        };
        let (one, two) = (build("FakeOne"), build("FakeTwo"));
        let all = [one.clone(), two.clone()];
        std::fs::create_dir_all(proc.join("7")).unwrap();
        let args = format!(
            "/opt/fake-game\0--crash-watch\0{}/\0",
            two.build_folder().unwrap().display()
        );
        std::fs::write(proc.join("7/cmdline"), args).unwrap();
        assert!(!two.running_among(&all, &proc), "nothing answers");
        let _game = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        assert!(!one.running_among(&all, &proc), "the other build's game");
        assert!(two.running_among(&all, &proc), "its own build's game");
        // No process in either build: the socket alone blames neither.
        std::fs::remove_file(proc.join("7/cmdline")).unwrap();
        assert!(!one.running_among(&all, &proc) && !two.running_among(&all, &proc));
        // A socket only one build uses is its game, and so is one without
        // a process list to tell by.
        assert!(one.running_among(std::slice::from_ref(&one), &proc));
        assert!(one.running_among(&all, &root.join("no proc")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn slots_share_a_build_by_its_patch_list() {
        let slot = |name: &str, mods: &str| {
            let json = format!(
                r#"{{"name": "{name}", "app": "/a", "mod_dir": "{mods}",
                "rebuild": {{"command": ["bake"], "manifest": "/g/list.txt"}}}}"#
            );
            Target::import_at(&json, None).unwrap()
        };
        let (one, two) = (
            slot("FakeOne", "/g/mods/zootr"),
            slot("FakeTwo", "/s/2/mods/zootr"),
        );
        assert!(one.shares_build(&two) && two.shares_build(&one));
        assert!(!one.shares_build(&one), "not itself");
        let mut apart = slot("FakeApart", "/h/mods/zootr");
        apart.rebuild.as_mut().unwrap().manifest = None;
        assert!(!one.shares_build(&apart));
        apart.rebuild = None;
        apart.mod_dir = "/g/mods/zootr".into();
        assert!(!one.shares_build(&apart), "no build of its own");
    }
}
