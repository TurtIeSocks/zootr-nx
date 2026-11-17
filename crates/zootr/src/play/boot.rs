//! Starting the game into play from a save folder, or a new file, with no
//! frame read: A goes in whenever the menus are up. Continue is the first
//! choice when a save exists and New Game the only one when none does, so
//! A alone gets through either.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use super::{Boot, Input};
use crate::build::{self, Status};
use crate::config::{Config, Target};
use crate::dev::Dev;
use crate::live::Game;
use crate::recorder;

/// How long a start gets: the first one prepares shaders.
pub const LIMIT: Duration = Duration::from_secs(600);
/// How often a start looks at how far it has got.
pub const LOOK: Duration = Duration::from_secs(2);
/// How long a game whose title can't be read gets before A goes in: the
/// Mac's title takes buttons after about 34 s, and one sent sooner lands
/// in the menus.
const TITLE: Duration = Duration::from_secs(42);
/// Run folders kept under `boot/`.
const KEEP: usize = 10;
/// The Mac runner's shader cache, which it keeps beside the save folder
/// it's given. A run folder links them to the shared ones, or every run
/// compiles every shader from nothing.
const SHADER_CACHE: [&str; 2] = ["pso_archive", "pso_user.tsv"];

/// What a start plays.
pub enum Start {
    /// Continue the save in this folder. The run plays a copy of it.
    Save(PathBuf),
    /// A new file on the installed seed.
    New,
}

/// The run's own save folder, `boot/<time>-<name>/save` beside the app's
/// live logs: a copy of a `Save`, or empty for `New`. Keeps the newest
/// [`KEEP`] runs.
pub fn prepare(start: &Start, name: &str) -> Result<PathBuf> {
    let run = super::folder("boot", name)?;
    let save = run.join("save");
    std::fs::create_dir_all(&save)?;
    if let Start::Save(from) = start {
        restore(from, &save).with_context(|| format!("copying {}", from.display()))?;
    }
    let runs = std::fs::read_dir(run.parent().context("a run has a folder")?)?;
    recorder::keep_newest(runs.filter_map(|e| Some(e.ok()?.path())), KEEP)?;
    Ok(save)
}

/// Start the game on `save` (its own saves when `None`) and press A
/// through the menus until play. A target with no `{save}` harness line
/// gets `save` restored over its save folder instead.
pub fn boot(
    input: &mut impl Input,
    target: &Target,
    save: Option<&Path>,
    limit: Duration,
) -> Result<()> {
    ensure!(
        !input.running(),
        "the game is already running, and it may be the maintainer's"
    );
    built(target)?;
    match save {
        Some(from) if !target.names_save() => {
            ensure!(
                std::fs::read_dir(from)?.next().is_some(),
                "a new file needs a {{save}} line in {}'s harness env: \
                 start from a kept new file with `--save` instead",
                target.name
            );
            let to = (target.save_dir.as_ref())
                .with_context(|| format!("{} has no save folder to restore into", target.name))?;
            restore(from, to).context("restoring the save")?;
            input.launch(None)?;
        }
        _ => {
            if let Some(save) = save {
                share_cache(target, save)?;
            }
            input.launch(save)?
        }
    }
    into_play(input, |since| by_memory(target, since), (limit, LOOK))
}

/// Make sure a game that bakes the hook patch in was built with the one
/// installed: rebuild it when it has a command, since no one waits on a
/// harness run, else refuse, since a run on an older hook means nothing.
pub fn built(target: &Target) -> Result<()> {
    match build::status(target) {
        Some(status) if status.stale() => {
            let text = status.text(&target.name).unwrap_or_default();
            if !target
                .rebuild
                .as_ref()
                .is_some_and(|r| !r.command.is_empty())
            {
                bail!("{text}. It has no rebuild command, so rebuild it by hand first");
            }
            eprintln!("{text}: rebuilding it first");
            let game = Config::load()?.game;
            let run = crate::progress::Run::silent().with(build::echo);
            build::rebuild(target, game.as_deref(), &run)?;
        }
        Some(status @ Status::Unrecorded) => {
            eprintln!("{}", status.text(&target.name).unwrap_or_default());
        }
        _ => {}
    }
    Ok(())
}

/// Link the shader cache beside `target`'s own save folder into the run
/// folder holding `save`. Pruning a run removes the links, never what
/// they point to.
fn share_cache(target: &Target, save: &Path) -> Result<()> {
    let shared = target.save_dir.as_deref().and_then(Path::parent);
    let (Some(shared), Some(run)) = (shared, save.parent()) else {
        return Ok(());
    };
    link_cache(shared, run)
}

fn link_cache(shared: &Path, run: &Path) -> Result<()> {
    for name in SHADER_CACHE {
        let (from, to) = (shared.join(name), run.join(name));
        if from.exists() && to.symlink_metadata().is_err() {
            link(&from, &to).with_context(|| format!("linking {}", to.display()))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}

#[cfg(windows)]
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    match from.is_dir() {
        true => std::os::windows::fs::symlink_dir(from, to),
        false => std::os::windows::fs::symlink_file(from, to),
    }
}

/// Press A whenever the game is in its menus, until play. `look` says how
/// far a start has got, by the time since launch, for an input that can't.
fn into_play(
    input: &mut impl Input,
    look: impl Fn(Duration) -> Boot,
    (limit, tick): (Duration, Duration),
) -> Result<()> {
    let start = Instant::now();
    loop {
        ensure!(start.elapsed() <= limit, "the game never reached play");
        sleep(tick);
        let boot = match input.boot() {
            Ok(Some(boot)) => boot,
            Ok(None) => look(start.elapsed()),
            Err(_) => continue,
        };
        match boot {
            Boot::Play => return Ok(()),
            Boot::Menus => {
                let _ = input.press("A");
            }
            Boot::Starting => {}
        }
    }
}

/// How far a start has got, by the game's memory: menus once the title's
/// menu opens (or [`TITLE`] after launch, when it can't be read), and play
/// once a map is up with no load or event. An event after play (a new
/// file's opening talk) counts as menus, so A presses through it.
fn by_memory(target: &Target, since: Duration) -> Boot {
    let timed = || match since > TITLE {
        true => Boot::Menus,
        false => Boot::Starting,
    };
    let Ok(dev) = Game::attach(target).and_then(Dev::new) else {
        return timed();
    };
    match dev.state() {
        Ok(s) if !s.map.is_empty() && !s.loading && !s.event => Boot::Play,
        _ if dev.title_open().unwrap_or(false) => Boot::Menus,
        _ => timed(),
    }
}

/// Make the save folder `to` the one at `from`. Below `to`'s top level,
/// whatever `from` lacks goes: the game keeps writing autosaves into slots
/// `from` never had, and Continue loads the newest. `to`'s own neighbours
/// of the save stay.
pub fn restore(from: &Path, to: &Path) -> Result<()> {
    mirror(from, to, false)
}

fn mirror(from: &Path, to: &Path, prune: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let mut kept = HashSet::new();
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            mirror(&entry.path(), &dest, true)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
        kept.insert(entry.file_name());
    }
    for entry in std::fs::read_dir(to)?.filter(|_| prune) {
        let entry = entry?;
        if kept.contains(&entry.file_name()) {
            continue;
        }
        match entry.file_type()?.is_dir() {
            true => std::fs::remove_dir_all(entry.path())?,
            false => std::fs::remove_file(entry.path())?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use anyhow::bail;

    use super::*;

    /// A game that reports nothing of its own start, counting presses
    /// and launches.
    #[derive(Default)]
    struct Fake {
        presses: Vec<String>,
        launches: u32,
    }

    impl Input for Fake {
        fn running(&self) -> bool {
            false
        }
        fn launch(&mut self, _: Option<&Path>) -> Result<()> {
            self.launches += 1;
            Ok(())
        }
        fn keys(&self) -> Result<()> {
            bail!("no control file")
        }
        fn hold(&mut self, button: &str, _: u64) -> Result<()> {
            self.presses.push(button.into());
            Ok(())
        }
        fn stick(&mut self, _: &str, _: u64) -> Result<()> {
            bail!("no stick")
        }
        fn shot(&mut self, _: &Path) -> Result<()> {
            bail!("no shot")
        }
    }

    const QUICK: (Duration, Duration) = (Duration::from_millis(200), Duration::ZERO);

    #[test]
    fn a_stale_target_with_no_command_refuses_before_launch() {
        let dir = std::env::temp_dir().join(format!("zootr-boot-stale-{}", std::process::id()));
        let mod_dir = dir.join("mods/zootr");
        std::fs::create_dir_all(mod_dir.join("exefs")).unwrap();
        std::fs::write(mod_dir.join("exefs/zootr.pchtxt"), "@stop\n").unwrap();
        std::fs::write(
            dir.join("exefs_patches.txt"),
            "# fake build\nzootr\tzootr.pchtxt\t00000000\tcode\t\n",
        )
        .unwrap();
        let target = Target {
            name: "FakeBaked".into(),
            app: dir.join("nothing runs this"),
            mod_dir,
            settings_file: None,
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
            rebuild: None,
        };
        let mut game = Fake::default();
        let err = boot(&mut game, &target, None, Duration::ZERO).unwrap_err();
        let err = err.to_string();
        assert!(err.contains("FakeBaked needs a rebuild"), "{err}");
        assert!(err.contains("rebuild it by hand"), "{err}");
        assert_eq!(game.launches, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_start_presses_a_only_in_the_menus_and_stops_at_play() {
        let mut game = Fake::default();
        let looks = Cell::new(0);
        let look = |_| {
            looks.set(looks.get() + 1);
            match looks.get() {
                1 | 2 => Boot::Starting,
                3..=5 => Boot::Menus,
                _ => Boot::Play,
            }
        };
        into_play(&mut game, look, QUICK).unwrap();
        assert_eq!(game.presses, ["A"; 3]);
        assert_eq!(looks.get(), 6);
    }

    #[test]
    fn a_start_gives_up_at_its_limit() {
        let mut game = Fake::default();
        let err = into_play(&mut game, |_| Boot::Starting, QUICK).unwrap_err();
        assert!(err.to_string().contains("never reached play"), "{err}");
        assert!(game.presses.is_empty());
    }

    #[test]
    fn a_run_shares_the_shader_cache_and_pruning_it_keeps_the_cache() {
        let dir = std::env::temp_dir().join(format!("zootr-cache-{}", std::process::id()));
        let (shared, run) = (dir.join("shared"), dir.join("boot").join("run"));
        std::fs::create_dir_all(shared.join(SHADER_CACHE[0])).unwrap();
        std::fs::write(shared.join(SHADER_CACHE[0]).join("a.keys"), "k").unwrap();
        std::fs::write(shared.join(SHADER_CACHE[1]), "t").unwrap();
        std::fs::create_dir_all(&run).unwrap();

        link_cache(&shared, &run).unwrap();
        link_cache(&shared, &run).unwrap();
        assert!(run.join(SHADER_CACHE[0]).join("a.keys").exists());
        assert_eq!(
            std::fs::read_to_string(run.join(SHADER_CACHE[1])).unwrap(),
            "t"
        );

        recorder::keep_newest([run.clone()].into_iter(), 0).unwrap();
        assert!(!run.exists());
        assert!(shared.join(SHADER_CACHE[0]).join("a.keys").exists());
        assert!(shared.join(SHADER_CACHE[1]).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restore_drops_autosaves_the_kept_save_never_had() {
        let root = std::env::temp_dir().join(format!("zootr-restore-{}", std::process::id()));
        let (kept, saves) = (root.join("kept"), root.join("data"));
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(kept.join("runner/save/p/slot_00/progress.sav"), "start");
        write(saves.join("runner/save/p/slot_00/progress.sav"), "played");
        write(saves.join("runner/save/p/slot_03/progress.sav"), "autosave");
        write(saves.join("runner/save/p/stray.sav"), "stray");
        write(saves.join("containers/keep"), "the runner's own");
        restore(&kept, &saves).unwrap();
        let read = |rel: &str| std::fs::read_to_string(saves.join(rel)).ok();
        assert_eq!(
            read("runner/save/p/slot_00/progress.sav").as_deref(),
            Some("start")
        );
        assert!(!saves.join("runner/save/p/slot_03").exists());
        assert_eq!(read("runner/save/p/stray.sav"), None);
        assert_eq!(read("containers/keep").as_deref(), Some("the runner's own"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
