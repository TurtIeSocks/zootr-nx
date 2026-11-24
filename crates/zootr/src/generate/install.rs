//! Installs a seed's mod onto the targets, and takes it off.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, bail, ensure};

use crate::config::Target;
use crate::patch;
use crate::progress::{self, Run, Step, Stopped, TookOff};
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

/// Run `write` into a folder that's gone again after: a seed built for
/// the history alone, which keeps what it needs to build it again.
pub fn throwaway<T>(write: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("zootr-unplaced-{}-{run}", std::process::id()));
    let out = write(&dir);
    std::fs::remove_dir_all(&dir).ok();
    out
}

/// The first target whose game is open.
pub fn running(targets: &[Target]) -> Option<&Target> {
    targets.iter().find(|t| t.running())
}

/// Run `write` once, into the first target's mod folder, copy what it
/// wrote to every other distinct folder, then write each target's settings
/// line. A folder no target reads the cheat from keeps only the
/// exefs patch, or, for a game that builds that in, splits it (see
/// [`patch::split`]). Refuses while another install runs or any target's game
/// is open. Returns what `write` did.
///
/// A stop or failure (a panic too) before any folder's mod is replaced
/// changes nothing. After, the targets would hold a mix of seeds, or a mod
/// without its cheat, so it takes the mod off every one.
pub fn install<T>(
    targets: &[Target],
    run: &Run,
    write: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    guarded(targets, || {
        let wrote: Result<T> = progress::caught(|| {
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
                let here = || targets.iter().filter(|o| o.mod_dir == t.mod_dir);
                if here().any(Target::reads_cheats) {
                    continue;
                }
                match here().any(|o| o.rebuild.is_some()) {
                    true => patch::split(&t.mod_dir)?,
                    false => patch::remove_cheat(&t.mod_dir)?,
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
        });
        match wrote {
            Err(e) if run.replaced() => {
                // A fresh error, so a stop doesn't pass for one that left
                // the targets as they were.
                if let Err(off) = take_off(targets) {
                    bail!("the mod stayed on ({off:#}) after: {e:#}");
                }
                Err(match progress::stopped(&e) {
                    Some(_) => Stopped { took_off: true }.into(),
                    None => e.context(TookOff),
                })
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

/// Take the mod off every target, each tried even after one fails.
fn take_off(targets: &[Target]) -> Result<()> {
    let failed: Vec<String> = (targets.iter())
        .filter_map(|t| take_off_one(t).err())
        .map(|e| format!("{e:#}"))
        .collect();
    ensure!(failed.is_empty(), "{}", failed.join("; "));
    Ok(())
}

fn take_off_one(t: &Target) -> Result<()> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SettingsFile;
    use crate::generate::tests::Scratch;

    /// Install and uninstall refuse to overlap, so their tests take turns.
    static TURNS: Mutex<()> = Mutex::new(());

    #[test]
    fn uninstalling_takes_the_romfs_cheat_and_settings_lines_off_every_target() {
        let _turn = TURNS.lock().unwrap();
        let dir = Scratch::new("uninstall");
        let mod_dir = dir.join("mod");
        std::fs::create_dir_all(mod_dir.join("romfs/Pack")).unwrap();
        std::fs::write(mod_dir.join("spoiler.txt"), "").unwrap();
        crate::patch::install(
            &mod_dir,
            crate::progressive::Lines::climbing(),
            &[],
            0,
            &[],
            &[],
            None,
        )
        .unwrap();
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
            rebuild: None,
            game: None,
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
    }

    #[test]
    fn install_writes_one_folder_copies_it_to_the_rest_then_every_settings_line() {
        let _turn = TURNS.lock().unwrap();
        let dir = Scratch::new("install");
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
            rebuild: None,
            game: None,
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
    }

    #[test]
    fn the_cheat_stays_only_where_a_target_reads_it() {
        let _turn = TURNS.lock().unwrap();
        let dir = Scratch::new("cheat");
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
            rebuild: None,
            game: None,
        };
        let has = |m: &str| crate::patch::path(&dir.join(m)).exists();
        let ips = |m: &str| crate::patch::ips_path(&dir.join(m)).is_file();
        // Either way round: a folder written first without a reader still
        // hands the cheat to the copy that has one.
        for order in [["reads", "not"], ["not", "reads"]] {
            let _ = std::fs::remove_dir_all(&*dir);
            let line = |m| {
                if m == "reads" {
                    "C={cheats}"
                } else {
                    "R={romfs}"
                }
            };
            let targets = order.map(|m| target(m, line(m)));
            install(&targets, &Run::silent(), |d| {
                crate::patch::install(
                    d,
                    crate::progressive::Lines::climbing(),
                    &[],
                    0,
                    &[],
                    &[],
                    None,
                )
            })
            .unwrap();
            assert!(has("reads") && !has("not"), "{order:?}");
            assert!(
                ips("reads") && ips("not"),
                "{order:?}: every folder has the IPS"
            );
            assert!(dir.join("not/exefs/zootr.pchtxt").is_file());
        }
        // A game that builds the exefs patch in gets it without the table,
        // which comes as a cheat.
        let _ = std::fs::remove_dir_all(&*dir);
        let built = Target {
            rebuild: Some(crate::config::Rebuild::default()),
            game: None,
            ..target("built", "R={romfs}")
        };
        install(&[built], &Run::silent(), |d| {
            crate::patch::install(
                d,
                crate::progressive::Lines::climbing(),
                &[],
                0,
                &[],
                &[],
                None,
            )
        })
        .unwrap();
        let exefs = std::fs::read_to_string(dir.join("built/exefs/zootr.pchtxt")).unwrap();
        let code = crate::patch::code_patches().unwrap();
        assert_eq!(exefs, crate::patch::pchtxt(&code));
        assert!(has("built") && ips("built"));
    }

    #[test]
    fn a_stop_or_failure_once_a_folder_is_replaced_takes_the_mod_off_every_target() {
        let _turn = TURNS.lock().unwrap();
        let dir = Scratch::new("stop");
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
            rebuild: None,
            game: None,
        };
        let targets = [target("a"), target("b")];
        let fresh = || {
            for t in &targets {
                std::fs::create_dir_all(t.mod_dir.join("romfs")).unwrap();
                std::fs::write(dir.join(format!("{}.txt", t.name)), "A=1\nK=old\n").unwrap();
            }
        };
        let installed = |t: &Target| t.mod_dir.join("romfs").exists();
        let line = |t: &Target| std::fs::read_to_string(dir.join(format!("{}.txt", t.name)));
        let as_were = || {
            assert!(targets.iter().all(installed));
            assert!(targets.iter().all(|t| line(t).unwrap() == "A=1\nK=old\n"));
        };
        let off = || {
            assert!(!targets.iter().any(installed));
            assert!(targets.iter().all(|t| line(t).unwrap() == "A=1\n"));
        };

        // Before writing replaced a folder, the targets keep what they had.
        fresh();
        let run = Run::silent();
        run.stopper().stop();
        let err = install(&targets, &run, |_| run.step(Step::Checks)).unwrap_err();
        assert!(!progress::took_off(&err));
        as_were();
        let err = install(&targets, &Run::silent(), |_| -> Result<()> {
            bail!("broke")
        })
        .unwrap_err();
        assert!(!progress::took_off(&err));
        as_were();

        // After, a stop, a failure and a panic each take the mod off.
        let run = Run::silent();
        let err = install(&targets, &run, |_| {
            run.mark_replaced();
            run.stopper().stop();
            run.check()
        })
        .unwrap_err();
        assert!(progress::stopped(&err).unwrap().took_off);
        off();
        fresh();
        let run = Run::silent();
        let err = install(&targets, &run, |_| -> Result<()> {
            run.mark_replaced();
            bail!("the table needs 4076 bytes, 3968 are free")
        })
        .unwrap_err();
        assert!(progress::took_off(&err));
        assert!(format!("{err:#}").ends_with(": the table needs 4076 bytes, 3968 are free"));
        off();
        fresh();
        let run = Run::silent();
        let err = install(&targets, &run, |_| -> Result<()> {
            run.mark_replaced();
            panic!("index out of bounds")
        })
        .unwrap_err();
        assert!(progress::took_off(&err));
        assert!(format!("{err:#}").contains("the generator crashed: index out of bounds"));
        off();
        // A target the mod won't come off neither keeps it on the rest nor
        // hides why the install failed.
        fresh();
        let run = Run::silent();
        let stuck = targets[0].mod_dir.join("romfs");
        let err = install(&targets, &run, |_| -> Result<()> {
            run.mark_replaced();
            std::fs::remove_dir_all(&stuck)?;
            std::fs::write(&stuck, "")?;
            bail!("broke")
        })
        .unwrap_err();
        assert!(!installed(&targets[1]) && line(&targets[1]).unwrap() == "A=1\n");
        let message = format!("{err:#}");
        assert!(
            message.contains("removing") && message.ends_with(": broke"),
            "{message}"
        );
        std::fs::remove_file(&stuck).unwrap();
        // A stop there says so too, never that the targets are as they were.
        fresh();
        let run = Run::silent();
        let err = install(&targets, &run, |_| {
            run.mark_replaced();
            std::fs::remove_dir_all(&stuck)?;
            std::fs::write(&stuck, "")?;
            run.stopper().stop();
            run.check()
        })
        .unwrap_err();
        assert!(progress::stopped(&err).is_none());
        let message = format!("{err:#}");
        assert!(message.contains("stayed on"), "{message}");
        std::fs::remove_file(&stuck).unwrap();
        // And the lock it held is free for the next install.
        fresh();
        install(&targets[..1], &Run::silent(), |_| Ok(())).unwrap();
    }
}
