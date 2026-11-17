//! Installs a seed's mod onto the targets, and takes it off.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, bail, ensure};

use crate::config::Target;
use crate::patch;
use crate::progress::{self, Run, Step, Stopped};
use ounce_formats::copy_mod;

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
///
/// A stop before any folder's mod is replaced changes nothing. After, the
/// targets would hold a mix of seeds, so it takes the mod off every one.
pub fn install<T>(
    targets: &[Target],
    run: &Run,
    mut write: impl FnMut(&Path) -> Result<T>,
) -> Result<T> {
    guarded(targets, || {
        let wrote: Result<T> = (|| {
            let mut done = HashSet::new();
            let folders: Vec<&Target> =
                targets.iter().filter(|t| done.insert(&t.mod_dir)).collect();
            let (first, rest) = folders.split_first().context("no targets")?;
            run.target(&first.name);
            let out = write(&first.mod_dir)?;
            // One seed is the same in every folder: the rest get a copy.
            for t in rest {
                run.check()?;
                run.target(&t.name);
                copy_mod(&first.mod_dir, &t.mod_dir)?;
            }
            for t in &folders {
                if !(targets.iter()).any(|o| o.mod_dir == t.mod_dir && o.reads_cheats()) {
                    patch::remove_cheat(&t.mod_dir)?;
                }
            }
            run.step(Step::Installing)?;
            for t in targets {
                run.detail(&t.name);
                if let Some(line) = &t.settings_file {
                    line.apply(&t.mod_dir, t.control.as_deref())?;
                }
            }
            Ok(out)
        })();
        match wrote {
            // Writing the spoiler starts once a mod folder was replaced.
            Err(e) if progress::stopped(&e).is_some() && run.reached(Step::Spoiler) => {
                take_off(targets)?;
                Err(Stopped { took_off: true }.into())
            }
            wrote => wrote,
        }
    })
}

/// Take the mod off every target, its romfs and settings line, so the game
/// runs vanilla until the next install.
pub fn uninstall(targets: &[Target]) -> Result<()> {
    guarded(targets, || take_off(targets))
}

fn take_off(targets: &[Target]) -> Result<()> {
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
            harness_env: vec![],
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
    fn install_writes_one_folder_copies_it_to_the_rest_then_every_settings_line() {
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
            harness_env: vec![],
        };
        let targets = [
            target("a", "m1", false),
            target("b", "m1", true),
            target("c", "m2", true),
        ];
        let mut wrote = vec![];
        let last = install(&targets, &Run::silent(), |d| {
            wrote.push(d.to_owned());
            std::fs::create_dir_all(d.join("romfs/Pack"))?;
            std::fs::write(d.join("romfs/Pack/seed.txt"), "7")?;
            Ok(wrote.len())
        })
        .unwrap();
        // One seed, written once: a second folder gets a copy of it.
        assert_eq!((wrote, last), (vec![dir.join("m1")], 1));
        let copied = std::fs::read_to_string(dir.join("m2/romfs/Pack/seed.txt")).unwrap();
        assert_eq!(copied, "7");
        assert!(dir.join("b.txt").is_file() && dir.join("c.txt").is_file());
        assert!(install(&[], &Run::silent(), |_| Ok(())).is_err());
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
            harness_env: vec![],
        };
        let has = |m: &str| crate::patch::path(&dir.join(m)).exists();
        // Either way round: a folder written first without a reader still
        // hands the cheat to the copy that has one.
        for order in [["reads", "not"], ["not", "reads"]] {
            let _ = std::fs::remove_dir_all(&dir);
            let line = |m| {
                if m == "reads" {
                    "C={cheats}"
                } else {
                    "R={romfs}"
                }
            };
            let targets = order.map(|m| target(m, line(m)));
            install(&targets, &Run::silent(), |d| {
                crate::patch::install(d, true, false, 0, &[])
            })
            .unwrap();
            assert!(has("reads") && !has("not"), "{order:?}");
            assert!(dir.join("not/exefs/zootr.pchtxt").is_file());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stop_once_a_folder_is_replaced_takes_the_mod_off_every_target() {
        let _turn = TURNS.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-stop-{}", std::process::id()));
        let target = |name: &str| Target {
            name: name.into(),
            app: "/nowhere/zootr-test.app".into(),
            mod_dir: dir.join(name),
            settings_file: Some(SettingsFile {
                file: dir.join(format!("{name}.txt")),
                lines: vec!["K={romfs}".into()],
            }),
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
        };
        let targets = [target("a"), target("b")];
        for t in &targets {
            std::fs::create_dir_all(t.mod_dir.join("romfs")).unwrap();
            std::fs::write(dir.join(format!("{}.txt", t.name)), "A=1\nK=old\n").unwrap();
        }
        let installed = |t: &Target| t.mod_dir.join("romfs").exists();
        let line = |t: &Target| std::fs::read_to_string(dir.join(format!("{}.txt", t.name)));

        let run = Run::silent();
        run.stopper().stop();
        let err = install(&targets, &run, |_| run.step(Step::Checks)).unwrap_err();
        assert!(!progress::stopped(&err).unwrap().took_off);
        assert!(targets.iter().all(installed));
        assert_eq!(line(&targets[0]).unwrap(), "A=1\nK=old\n");

        let run = Run::silent();
        let err = install(&targets, &run, |_| {
            run.step(Step::Checks)?;
            run.step(Step::Spoiler)?;
            run.stopper().stop();
            Ok(())
        })
        .unwrap_err();
        assert!(progress::stopped(&err).unwrap().took_off);
        assert!(!targets.iter().any(installed));
        assert!(targets.iter().all(|t| line(t).unwrap() == "A=1\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
