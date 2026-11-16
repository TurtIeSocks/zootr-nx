//! Installs a seed's mod onto the targets, and takes it off.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, bail, ensure};

use crate::config::Target;
use crate::patch;

/// Held while a seed is being installed, so two can't overlap.
static INSTALLING: Mutex<()> = Mutex::new(());

/// Run `f` unless another install runs or any target's game is open.
fn guarded<T>(targets: &[Target], f: impl FnOnce() -> Result<T>) -> Result<T> {
    let Ok(_installing) = INSTALLING.try_lock() else {
        bail!("Already writing a seed");
    };
    ensure!(!targets.is_empty(), "add a target first");
    if let Some(t) = running(targets) {
        bail!("Close the game first: {} is running", t.name);
    }
    f()
}

/// The first target whose game is open.
pub fn running(targets: &[Target]) -> Option<&Target> {
    targets.iter().find(|t| t.running())
}

/// Run `write` once per distinct mod folder, then write each target's
/// settings line. A folder no target reads the cheat from keeps only the
/// exefs patch. Refuses while another install runs or any target's game
/// is open. Returns what the last `write` did.
pub fn install<T>(targets: &[Target], mut write: impl FnMut(&Path) -> Result<T>) -> Result<T> {
    guarded(targets, || {
        let mut done = HashSet::new();
        let mut out = None;
        for t in targets.iter().filter(|t| done.insert(&t.mod_dir)) {
            out = Some(write(&t.mod_dir)?);
            if !(targets.iter()).any(|o| o.mod_dir == t.mod_dir && o.reads_cheats()) {
                patch::remove_cheat(&t.mod_dir)?;
            }
        }
        for t in targets {
            if let Some(line) = &t.settings_file {
                line.apply(&t.mod_dir, t.control.as_deref())?;
            }
        }
        Ok(out.expect("targets is not empty"))
    })
}

/// Take the mod off every target, its romfs and settings line, so the game
/// runs vanilla until the next install.
pub fn uninstall(targets: &[Target]) -> Result<()> {
    guarded(targets, || {
        for t in targets {
            let romfs = t.mod_dir.join("romfs");
            match std::fs::remove_dir_all(&romfs) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("removing {}", romfs.display()));
                }
                _ => {}
            }
            patch::remove(&t.mod_dir)?;
            if let Some(line) = &t.settings_file {
                line.remove()?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SettingsFile;

    /// Install and uninstall refuse to overlap, so their tests take turns.
    static TURNS: Mutex<()> = Mutex::new(());

    #[test]
    fn uninstalling_takes_the_romfs_cheat_and_settings_lines_off_every_target() {
        let _turn = TURNS.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-test-{}-uninstall", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("mod");
        std::fs::create_dir_all(mod_dir.join("romfs/Pack")).unwrap();
        std::fs::write(mod_dir.join("spoiler.txt"), "").unwrap();
        crate::patch::install(&mod_dir, true, false, 0, &[]).unwrap();
        let env = dir.join("env.txt");
        std::fs::write(&env, "A=1\nKEY=x\n").unwrap();
        let target = |line| Target {
            name: "t".into(),
            app: dir.join("nothing runs this"),
            mod_dir: mod_dir.clone(),
            settings_file: line,
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
        };
        let line = SettingsFile {
            file: env.clone(),
            lines: vec!["KEY={romfs}".into()],
        };
        let targets = [target(None), target(Some(line))];
        uninstall(&targets).unwrap();
        uninstall(&targets).unwrap();
        assert!(!mod_dir.join("romfs").exists() && mod_dir.join("spoiler.txt").exists());
        assert!(!crate::patch::path(&mod_dir).exists());
        assert_eq!(std::fs::read_to_string(&env).unwrap(), "A=1\n");
        assert!(uninstall(&[]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn install_writes_each_folder_once_then_every_settings_line() {
        let _turn = TURNS.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-install-{}", std::process::id()));
        let target = |name: &str, mod_dir: &str, line: bool| Target {
            name: name.into(),
            app: "/nowhere/zootr-test.app".into(),
            mod_dir: dir.join(mod_dir),
            settings_file: line.then(|| SettingsFile {
                file: dir.join(format!("{name}.txt")),
                lines: vec!["K={romfs}".into()],
            }),
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
        };
        let targets = [
            target("a", "m1", false),
            target("b", "m1", true),
            target("c", "m2", true),
        ];
        let mut wrote = vec![];
        let last = install(&targets, |d| {
            wrote.push(d.to_owned());
            Ok(wrote.len())
        })
        .unwrap();
        assert_eq!((wrote, last), (vec![dir.join("m1"), dir.join("m2")], 2));
        assert!(dir.join("b.txt").is_file() && dir.join("c.txt").is_file());
        assert!(install(&[], |_| Ok(())).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_cheat_stays_only_where_a_target_reads_it() {
        let _turn = TURNS.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-cheat-{}", std::process::id()));
        let target = |mod_dir: &str, line: &str| Target {
            name: mod_dir.into(),
            app: "/nowhere/zootr-test.app".into(),
            mod_dir: dir.join(mod_dir),
            settings_file: Some(SettingsFile {
                file: dir.join(format!("{mod_dir}.txt")),
                lines: vec![line.into()],
            }),
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
        };
        let targets = [target("reads", "C={cheats}"), target("not", "R={romfs}")];
        install(&targets, |d| crate::patch::install(d, true, false, 0, &[])).unwrap();
        let has = |m: &str| crate::patch::path(&dir.join(m)).exists();
        assert!(has("reads") && !has("not"));
        assert!(dir.join("not/exefs/zootr.pchtxt").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
