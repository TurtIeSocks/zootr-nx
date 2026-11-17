//! A mod folder set to the folder an emulator keeps every mod for the game
//! in. Emulators load a mod from a named folder inside that one, so install
//! writes into a folder of its own there instead.

use super::{Config, Target};

/// The folder install makes inside the mods root.
const SUBFOLDER: &str = "zootr";

impl Target {
    /// Moves `mod_dir` into a folder of its own when it's the game's mods
    /// root, the folder named after the title id. Says what it did.
    pub fn fix_mods_root(&mut self) -> Option<String> {
        let title = format!("{:016X}", crate::game::TITLE);
        let name = self.mod_dir.file_name()?.to_str()?;
        if !name.eq_ignore_ascii_case(&title) {
            return None;
        }
        let root = self.mod_dir.clone();
        self.mod_dir = root.join(SUBFOLDER);
        let mut note = format!(
            "{}: {} holds every mod for the game, so zootr installs into {}, its own folder there",
            self.name,
            root.display(),
            self.mod_dir.display()
        );
        if root.join("romfs").exists() {
            note += &format!(
                ". An older install's romfs is still in {}, and the game may load it too",
                root.display()
            );
        }
        Some(note)
    }
}

impl Config {
    /// [`Target::fix_mods_root`] on every target.
    pub fn fix_mods_roots(&mut self) -> Vec<String> {
        (self.targets.iter_mut())
            .filter_map(Target::fix_mods_root)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn target(mod_dir: &Path) -> Target {
        Target {
            name: "FakeRunner".into(),
            app: "/nowhere/app".into(),
            mod_dir: mod_dir.into(),
            settings_file: None,
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
        }
    }

    #[test]
    fn a_mods_root_gains_a_folder_of_its_own() {
        for root in ["/m/load/0400486024DEC000", "/m/load/0400486024dec000"] {
            let mut t = target(Path::new(root));
            let note = t.fix_mods_root().unwrap();
            assert_eq!(t.mod_dir, Path::new(root).join("zootr"));
            assert!(
                note.contains("installs into") && !note.contains("older install"),
                "{note}"
            );
            // Fixed once, it stays put.
            assert!(t.fix_mods_root().is_none());
        }
        for other in ["/m/load/0400486024DEC000/zootr", "/m/mods", "/"] {
            let mut t = target(Path::new(other));
            assert!(t.fix_mods_root().is_none(), "{other}");
            assert_eq!(t.mod_dir, Path::new(other));
        }
    }

    #[test]
    fn an_older_install_in_the_root_is_named() {
        let dir = std::env::temp_dir()
            .join(format!("zootr-mods-root-{}", std::process::id()))
            .join("0400486024DEC000");
        std::fs::create_dir_all(dir.join("romfs")).unwrap();
        let mut config = Config {
            targets: vec![target(&dir), target(Path::new("/m/mods"))],
            ..Default::default()
        };
        let notes = config.fix_mods_roots();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("older install"), "{}", notes[0]);
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn an_older_config_loads_fixed() {
        let json = r#"{"targets": [{"name": "FakeRunner", "app": "/a", "mod_dir": "/m/0400486024DEC000"}]}"#;
        let config = Config::parse(json.as_bytes()).unwrap();
        assert_eq!(
            config.targets[0].mod_dir,
            Path::new("/m/0400486024DEC000/zootr")
        );
    }
}
