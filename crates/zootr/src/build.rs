//! A game that bakes the mods' exefs patches in at build time: whether its
//! last build saw the hook patch installed now, and rebuilding it.
//!
//! The build lists the patches it applied in its manifest: `#` comment
//! lines, then one tab-separated line each of mod, file, the file's FNV-1a,
//! kind and note.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

use crate::config::{Config, Target};
use crate::patch;
use crate::plain::{DETAILS, Plain};
use crate::progress::{Run, Step, StepEvent};

/// 32-bit FNV-1a, the hash a manifest gives each patch file.
pub fn fnv1a(bytes: &[u8]) -> u32 {
    (bytes.iter()).fold(0x811c_9dc5, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// One patch file a build saw.
#[derive(Debug, PartialEq)]
pub struct Line {
    pub mod_name: String,
    pub file: String,
    pub hash: String,
    pub kind: String,
    pub note: String,
}

/// The kind of a patch file the build refused, its reason in the note.
const UNSUPPORTED: &str = "unsupported";

/// A manifest's lines, comments and blank lines skipped.
pub fn parse(text: &str) -> Vec<Line> {
    (text.lines())
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut cols = l.split('\t').map(|c| c.trim().to_owned());
            let mut next = || cols.next().unwrap_or_default();
            let line = Line {
                mod_name: next(),
                file: next(),
                hash: next(),
                kind: next(),
                note: next(),
            };
            (!line.file.is_empty()).then_some(line)
        })
        .collect()
}

/// Whether a game's last build saw the hook patch installed now.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename = "BuildStatus", rename_all = "camelCase")]
pub enum Status {
    /// The build has the installed patch, or neither has one.
    Current,
    /// The patch changed, came or went since the build.
    Stale,
    /// The build refused the patch, saying why.
    Refused { note: String },
    /// The build lists no patches, so it may not load them.
    Unrecorded,
}

impl Status {
    /// What the user is told about `name`, or `None` when it's current.
    pub fn text(&self, name: &str) -> Option<String> {
        match self {
            Status::Current => None,
            Status::Stale => Some(format!("{name} needs a rebuild before you play")),
            Status::Refused { note } => Some(format!(
                "{name}'s last build didn't take the randomizer's changes{DETAILS}{note}"
            )),
            Status::Unrecorded => Some(format!(
                "{name}'s build may not load the randomizer's changes. Update it, then rebuild"
            )),
        }
    }

    /// Whether the game would run another hook than the installed one.
    pub fn stale(&self) -> bool {
        matches!(self, Status::Stale | Status::Refused { .. })
    }
}

/// The build status of a target that bakes patches in: one with a
/// `rebuild` table, or a manifest where the default one goes. `None` for
/// a game that loads its patches at launch.
pub fn status(target: &Target) -> Option<Status> {
    let manifest = target.manifest();
    let Some(text) = manifest.and_then(|m| std::fs::read_to_string(m).ok()) else {
        return target.rebuild.is_some().then_some(Status::Unrecorded);
    };
    let name = target.mod_dir.file_name().map(|n| n.to_string_lossy());
    let ours = (parse(&text).into_iter())
        .find(|l| Some(l.mod_name.as_str()) == name.as_deref() && l.file == patch::EXEFS);
    let installed = std::fs::read(patch::exefs(&target.mod_dir)).ok();
    let installed = installed.map(|b| format!("{:08x}", fnv1a(&b)));
    // With no patch installed, a build of today's code patch plays
    // vanilla: with no table, every hook passes through.
    let installed = installed.or_else(|| {
        ours.as_ref()?;
        let h = patch::Hook::of(crate::game::Release::V111);
        let code = h.pchtxt(&h.code_patches().ok()?);
        Some(format!("{:08x}", fnv1a(code.as_bytes())))
    });
    Some(match (ours, installed) {
        (Some(l), _) if l.kind == UNSUPPORTED => Status::Refused { note: l.note },
        (None, None) => Status::Current,
        (Some(l), Some(hash)) if l.hash.eq_ignore_ascii_case(&hash) => Status::Current,
        _ => Status::Stale,
    })
}

/// The targets being rebuilt, by name.
static REBUILDING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A target's place in [`REBUILDING`], given up on drop.
struct Held(String);

impl Held {
    fn take(name: &str) -> Result<Held> {
        let mut names = REBUILDING.lock().unwrap();
        if names.iter().any(|n| n == name) {
            bail!("{name} is already being rebuilt");
        }
        names.push(name.to_owned());
        Ok(Held(name.to_owned()))
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        REBUILDING.lock().unwrap().retain(|n| *n != self.0);
    }
}

/// Run `target`'s rebuild command, `{game}` filled in by `game`, each line
/// it prints a detail of `run`'s rebuilding step. Succeeds only when the
/// build's status reads current afterwards, whatever the command's exit.
/// Refuses while the game runs or another rebuild of it does.
pub fn rebuild(target: &Target, game: Option<&Path>, run: &Run) -> Result<Status> {
    let proc = Path::new(crate::platform::PROC);
    rebuild_among(target, Config::load_all(), proc, game, run)
}

/// [`rebuild`], checking the targets in `saved`, the saved config, for a
/// running slot that shares the build, by the process list `proc`. Refuses
/// when the config didn't load, since then it can't tell.
fn rebuild_among(
    target: &Target,
    saved: Result<Config>,
    proc: &Path,
    game: Option<&Path>,
    run: &Run,
) -> Result<Status> {
    let name = &target.name;
    let args =
        (target.rebuild_args(game)?).with_context(|| format!("{name} has no rebuild command"))?;
    // Slots share one build: a rebuild under any running one breaks it.
    let saved = saved.context("reading the config to check for slots sharing the build")?;
    let sharing = (saved.targets.iter()).filter(|t| t.shares_build(target));
    if let Some(busy) = std::iter::once(target)
        .chain(sharing)
        .find(|t| t.running_among(&saved.targets, proc))
    {
        bail!("Close the game first: {} is running", busy.name);
    }
    let _held = Held::take(name)?;
    run.step(Step::Rebuilding)?;
    let (program, rest) = args.split_first().context("an empty rebuild command")?;
    let mut command = crate::platform::command(program);
    // Its own process group, so a stop reaches everything it starts.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let child = (command.args(rest))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {program}"))
        .context(Plain(format!(
            "Couldn't start {name}'s rebuild command. Check it on the Targets page"
        )))?;
    let Some(exit) = watch(child, run)? else {
        return Err(run.check().unwrap_err().context(
            "Stopped the rebuild before it finished, so rebuild the game before playing it",
        ));
    };
    if !exit.success() {
        let raw = anyhow!("{name}'s rebuild command failed ({exit})");
        return Err(raw.context(Plain(format!("{name}'s rebuild failed"))));
    }
    match status(target).unwrap_or(Status::Unrecorded) {
        Status::Current => Ok(Status::Current),
        Status::Stale => Err(anyhow!(
            "{name}'s rebuild command finished, but the build still doesn't have the installed hook patch: the command didn't build the game folder {} is in",
            target.manifest().unwrap_or_default().display()
        )
        .context(Plain(format!(
            "{name}'s rebuild finished, but the game still doesn't have the randomizer's changes. Check that the rebuild command builds the game the mod folder belongs to"
        )))),
        other => bail!("{}", other.text(name).unwrap_or_default()),
    }
}

/// How often a rebuild looks for a stop and its own end.
const TICK: Duration = Duration::from_millis(100);

/// How long a stopped rebuild has to end before it's killed outright.
const GRACE: Duration = Duration::from_secs(3);

/// Each line `child` prints a detail of `run`, until it exits, text that
/// isn't UTF-8 made readable. A stop asked of `run` kills it and everything
/// it started, outright after [`GRACE`], and gives `None`.
fn watch(mut child: Child, run: &Run) -> Result<Option<ExitStatus>> {
    let (tx, rx) = mpsc::channel();
    let pipes: [Option<Box<dyn Read + Send>>; 2] = [
        child.stdout.take().map(|p| Box::new(p) as _),
        child.stderr.take().map(|p| Box::new(p) as _),
    ];
    // Left to run: a process the command started apart can hold a pipe
    // open after it exits.
    for pipe in pipes.into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            // Not `lines()`, which ends at the first line that isn't UTF-8
            // and leaves the command writing to a closed pipe.
            for line in BufReader::new(pipe).split(b'\n').map_while(Result::ok) {
                let line = String::from_utf8_lossy(&line);
                let line = line.strip_suffix('\r').unwrap_or(&line);
                if tx.send(line.to_owned()).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    // When it was last asked to end.
    let mut stopped: Option<Instant> = None;
    loop {
        match rx.recv_timeout(TICK) {
            Ok(line) => run.detail(line),
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(TICK),
            Err(RecvTimeoutError::Timeout) => {}
        }
        match stopped {
            None if run.check().is_err() => {
                kill_tree(&mut child, false);
                stopped = Some(Instant::now());
            }
            Some(at) if at.elapsed() >= GRACE => {
                kill_tree(&mut child, true);
                stopped = Some(Instant::now());
            }
            _ => {}
        }
        if let Some(exit) = child.try_wait()? {
            drain(&rx, run);
            return Ok(stopped.is_none().then_some(exit));
        }
    }
}

/// How long a rebuild's output is still read after it exits: its last
/// lines can reach the pipe readers after the exit (Linux CI, 2026-11-17).
const DRAIN: Duration = Duration::from_secs(1);

/// The lines still coming from `rx` as details of `run`, until both pipes
/// close or [`DRAIN`] is up: a process the command started apart can hold
/// one open long after.
fn drain(rx: &mpsc::Receiver<String>, run: &Run) {
    let until = Instant::now() + DRAIN;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(line) => run.detail(line),
            Err(_) => break,
        }
    }
}

/// Kill `child` and every process it started: asked to end, or `hard`,
/// outright for one that won't.
fn kill_tree(child: &mut Child, hard: bool) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    let killed = crate::platform::command("kill")
        .args([
            if hard { "-KILL" } else { "-TERM" },
            "--",
            &format!("-{pid}"),
        ])
        .status();
    #[cfg(windows)]
    let killed = crate::platform::command("taskkill")
        .args(["/T", "/F", "/PID", &pid])
        .status();
    if hard || !killed.is_ok_and(|s| s.success()) {
        let _ = child.kill();
    }
}

/// Prints a rebuild's output lines to stderr as they come.
pub fn echo(event: &StepEvent) {
    if let StepEvent::Detail { text } = event {
        eprintln!("{text}");
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::sync::Arc;

    use super::*;
    use crate::config::Rebuild;
    #[cfg(unix)]
    use crate::progress::Stopper;

    /// A run that records every event it reports.
    #[cfg(unix)]
    fn recorded() -> (Arc<Mutex<Vec<StepEvent>>>, Run) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = heard.clone();
        let run = Run::silent().with(move |e| sink.lock().unwrap().push(e.clone()));
        (heard, run)
    }

    /// Stop through `stop` once `heard` has the detail `text`. Without it
    /// in 10 s, stops anyway and fails, so a run that never prints it can't
    /// hang the test.
    #[cfg(unix)]
    fn stop_once_heard(heard: &Mutex<Vec<StepEvent>>, stop: &Stopper, text: &str) {
        let until = Instant::now() + Duration::from_secs(10);
        let said = || {
            (heard.lock().unwrap().iter())
                .any(|e| matches!(e, StepEvent::Detail { text: t } if t == text))
        };
        while !said() {
            if Instant::now() > until {
                stop.stop();
                panic!("never heard {text:?}");
            }
            std::thread::sleep(TICK);
        }
        stop.stop();
    }

    #[test]
    fn fnv1a_matches_the_reference_values() {
        assert_eq!(fnv1a(b""), 0x811c_9dc5);
        assert_eq!(fnv1a(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
    }

    #[test]
    fn a_manifest_parses_every_kind_of_line() {
        let text = "# The mods' ExeFS patch files: mod, file, FNV-1a, kind, note\n\
            zootr\tzootr.pchtxt\t02c921c4\tcode\t\n\
            FakeMod\tfake.pchtxt\t0000abcd\tcode\tfine\n\
            \n\
            # another comment\n\
            Other\tbad.ips\t12345678\tunsupported\tnot a pchtxt\n\
            Short\tshort.pchtxt\tdeadbeef\r\n";
        let lines = parse(text);
        let line = |m: &str, f: &str, h: &str, k: &str, n: &str| Line {
            mod_name: m.into(),
            file: f.into(),
            hash: h.into(),
            kind: k.into(),
            note: n.into(),
        };
        assert_eq!(
            lines,
            [
                line("zootr", "zootr.pchtxt", "02c921c4", "code", ""),
                line("FakeMod", "fake.pchtxt", "0000abcd", "code", "fine"),
                line(
                    "Other",
                    "bad.ips",
                    "12345678",
                    "unsupported",
                    "not a pchtxt"
                ),
                line("Short", "short.pchtxt", "deadbeef", "", ""),
            ]
        );
    }

    /// A game folder holding `mods/zootr`, emptied.
    fn game(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-build-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mods/zootr")).unwrap();
        dir
    }

    /// A target named for its game folder, so tests rebuild apart.
    fn target(game: &Path) -> Target {
        Target {
            name: game.file_name().unwrap().to_string_lossy().into(),
            app: game.join("nothing runs this"),
            mod_dir: game.join("mods/zootr"),
            settings_file: None,
            save_dir: None,
            args: vec![],
            control: None,
            bridge: None,
            env: vec![],
            harness_env: vec![],
            rebuild: None,
            game: None,
        }
    }

    const PATCH: &[u8] = b"@nsobid-fake\n@stop\n";

    fn install(t: &Target) {
        let path = patch::exefs(&t.mod_dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, PATCH).unwrap();
    }

    /// A manifest listing `mod_name`'s patch with `hash`, and another mod's.
    fn manifest(game: &Path, mod_name: &str, hash: u32, kind: &str) {
        let text = format!(
            "# fake build\nFakeMod\tother.pchtxt\t00000000\tcode\t\n{mod_name}\tzootr.pchtxt\t{hash:08x}\t{kind}\tfake reason\n"
        );
        std::fs::write(game.join("exefs_patches.txt"), text).unwrap();
    }

    #[test]
    fn each_status_reads_from_the_manifest_and_the_installed_patch() {
        let dir = game("status");
        let t = target(&dir);
        assert_eq!(status(&t), None, "no manifest and no table");
        let with_table = Target {
            rebuild: Some(Rebuild::default()),
            game: None,
            ..t.clone()
        };
        assert_eq!(status(&with_table), Some(Status::Unrecorded));

        std::fs::write(dir.join("exefs_patches.txt"), "# nothing applied\n").unwrap();
        assert_eq!(status(&t), Some(Status::Current), "neither has one");
        install(&t);
        assert_eq!(status(&t), Some(Status::Stale), "first install");

        manifest(&dir, "zootr", fnv1a(PATCH), "code");
        assert_eq!(status(&t), Some(Status::Current));
        std::fs::write(patch::exefs(&t.mod_dir), b"changed").unwrap();
        assert_eq!(status(&t), Some(Status::Stale), "patch changed");
        install(&t);
        patch::remove(&t.mod_dir).unwrap();
        assert_eq!(
            status(&t),
            Some(Status::Stale),
            "patch removed, an old build"
        );
        let h = patch::Hook::of(crate::game::Release::V111);
        let code = h.pchtxt(&h.code_patches().unwrap());
        manifest(&dir, "zootr", fnv1a(code.as_bytes()), "code");
        assert_eq!(
            status(&t),
            Some(Status::Current),
            "patch removed, today's code built"
        );
        manifest(&dir, "zootr", fnv1a(PATCH), "code");

        install(&t);
        manifest(&dir, "zootr-old", fnv1a(PATCH), "code");
        assert_eq!(status(&t), Some(Status::Stale), "mod folder renamed");

        manifest(&dir, "zootr", fnv1a(PATCH), "unsupported");
        let refused = status(&t).unwrap();
        assert_eq!(
            refused,
            Status::Refused {
                note: "fake reason".into()
            }
        );
        assert!(refused.stale());
        let text = refused.text("FakeBuild").unwrap();
        assert!(text.ends_with(&format!("{DETAILS}fake reason")), "{text}");
        assert_eq!(Status::Current.text("FakeBuild"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// `t` rebuilt by a shell line.
    #[cfg(unix)]
    fn rebuilt_by(t: &Target, line: String) -> Target {
        Target {
            rebuild: Some(Rebuild {
                command: vec!["sh".into(), "-c".into(), line],
                manifest: None,
            }),
            ..t.clone()
        }
    }

    /// A process list that isn't there, never the machine's own.
    #[cfg(unix)]
    const NO_PROC: &str = "/nowhere/proc";

    /// `t` rebuilt with no other slot saved, never the real config.
    #[cfg(unix)]
    fn rebuilt(t: &Target, run: &Run) -> Result<Status> {
        rebuild_among(t, Ok(Config::default()), Path::new(NO_PROC), None, run)
    }

    #[cfg(unix)]
    #[test]
    fn a_config_that_did_not_load_refuses_the_rebuild_before_it_starts() {
        let dir = game("rebuild-no-config");
        let marker = dir.join("ran");
        let t = rebuilt_by(&target(&dir), format!("touch '{}'", marker.display()));
        let (heard, run) = recorded();
        let err = rebuild_among(
            &t,
            Err(anyhow!("fake unreadable config")),
            Path::new(NO_PROC),
            None,
            &run,
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("fake unreadable config"),
            "{err:#}"
        );
        assert!(!marker.exists(), "the rebuild ran without the config");
        assert!(heard.lock().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_game_of_another_build_on_the_socket_doesnt_block_this_rebuild() {
        let (one, two) = (game("shared-one"), game("shared-two"));
        let (socket, proc) = (one.join("control.sock"), one.join("proc"));
        std::fs::create_dir_all(proc.join("7")).unwrap();
        let on_socket = |t: Target| Target {
            bridge: Some(socket.clone()),
            ..t
        };
        let marker = two.join("ran");
        let other = on_socket(target(&one));
        let mine = on_socket(rebuilt_by(
            &target(&two),
            format!("touch '{}'", marker.display()),
        ));
        let both = || {
            Ok(Config {
                targets: vec![other.clone(), mine.clone()],
                ..Config::default()
            })
        };
        let _game = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let in_build = |dir: &Path| {
            let args = format!(
                "/opt/fake-game\0--crash-watch\0{}/\0",
                dir.canonicalize().unwrap().display()
            );
            std::fs::write(proc.join("7/cmdline"), args).unwrap();
        };
        in_build(&one);
        let _ = rebuild_among(&mine, both(), &proc, None, &Run::silent());
        assert!(
            marker.exists(),
            "the other build's game blocked the rebuild"
        );
        in_build(&two);
        let err = rebuild_among(&mine, both(), &proc, None, &Run::silent()).unwrap_err();
        assert!(
            format!("{err:#}").contains("Close the game first"),
            "{err:#}"
        );
        std::fs::remove_dir_all(one).unwrap();
        std::fs::remove_dir_all(two).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_rebuild_that_writes_a_matching_manifest_ends_current() {
        let dir = game("rebuild-current");
        let t = target(&dir);
        install(&t);
        let list = dir.join("exefs_patches.txt");
        let line = format!(
            "echo building; echo warming >&2; printf '# built\\nzootr\\tzootr.pchtxt\\t{:08x}\\tcode\\t\\n' > '{}'",
            fnv1a(PATCH),
            list.display()
        );
        let t = rebuilt_by(&t, line);
        assert_eq!(status(&t), Some(Status::Unrecorded));
        let (heard, run) = recorded();
        assert_eq!(rebuilt(&t, &run).unwrap(), Status::Current);
        let heard = heard.lock().unwrap();
        assert_eq!(
            heard[0],
            StepEvent::Began {
                step: Step::Rebuilding
            }
        );
        let detail = |s: &str| StepEvent::Detail { text: s.into() };
        assert!(heard.contains(&detail("building")) && heard.contains(&detail("warming")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_clean_exit_that_leaves_it_stale_says_so() {
        let dir = game("rebuild-stale");
        let t = target(&dir);
        install(&t);
        manifest(&dir, "zootr", 0, "code");
        let t = rebuilt_by(&t, "exit 0".into());
        let err = rebuilt(&t, &Run::silent()).unwrap_err();
        assert!(
            format!("{err:#}").contains("didn't build the game folder"),
            "{err:#}"
        );
        assert!(
            err.to_string().contains("'s rebuild finished, but"),
            "{err}"
        );
        assert_eq!(status(&t), Some(Status::Stale));
        let failed = rebuilt_by(&t, "exit 3".into());
        let err = rebuilt(&failed, &Run::silent()).unwrap_err();
        assert!(
            format!("{err:#}").contains("rebuild command failed"),
            "{err:#}"
        );
        assert!(err.to_string().ends_with("'s rebuild failed"), "{err}");
        let none = Target {
            rebuild: Some(Rebuild::default()),
            game: None,
            ..t.clone()
        };
        let err = rebuilt(&none, &Run::silent()).unwrap_err().to_string();
        assert!(err.contains("no rebuild command"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn lines_printed_after_the_rebuild_exits_are_heard_but_not_waited_on() {
        let (heard, run) = recorded();
        let child = crate::platform::command("sh")
            .args(["-c", "(sleep 0.3; echo late) & (sleep 5) & echo early"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let begun = Instant::now();
        assert!(watch(child, &run).unwrap().is_some_and(|e| e.success()));
        assert!(begun.elapsed() < DRAIN * 3, "{:?}", begun.elapsed());
        let detail = |s: &str| StepEvent::Detail { text: s.into() };
        let heard = heard.lock().unwrap();
        assert!(heard.contains(&detail("early")) && heard.contains(&detail("late")));
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_kills_the_rebuild_and_what_it_started() {
        let dir = game("rebuild-stop");
        let t = target(&dir);
        let pid = dir.join("pid");
        let line = format!(
            "sleep 30 & echo $! > '{}'; echo started; sleep 30; echo never",
            pid.display()
        );
        let t = rebuilt_by(&t, line);
        let (heard, run) = recorded();
        let stop = run.stopper();
        let begun = Instant::now();
        std::thread::scope(|s| {
            s.spawn(|| stop_once_heard(&heard, &stop, "started"));
            let err = rebuilt(&t, &run).unwrap_err();
            assert!(crate::progress::stopped(&err).is_some(), "{err:#}");
            assert!(
                format!("{err:#}").starts_with("Stopped the rebuild"),
                "{err:#}"
            );
        });
        assert!(begun.elapsed() < Duration::from_secs(10));
        let pid = std::fs::read_to_string(&pid).unwrap();
        let alive = || {
            crate::platform::command("kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        let gone = Instant::now();
        while alive() && gone.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(TICK);
        }
        assert!(!alive(), "the command's own child outlived the stop");
        assert!(
            !heard
                .lock()
                .unwrap()
                .iter()
                .any(|e| { matches!(e, StepEvent::Detail { text } if text == "never") })
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_kills_a_rebuild_that_ignores_it() {
        let (heard, run) = recorded();
        let stop = run.stopper();
        let mut command = crate::platform::command("sh");
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = (command.args(["-c", "trap '' TERM; echo started; sleep 30"]))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let begun = Instant::now();
        std::thread::scope(|s| {
            s.spawn(|| stop_once_heard(&heard, &stop, "started"));
            assert!(watch(child, &run).unwrap().is_none());
        });
        let took = begun.elapsed();
        assert!(took < GRACE + Duration::from_secs(5), "{took:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_line_that_isnt_utf8_is_heard_and_so_are_the_rest() {
        let (heard, run) = recorded();
        let child = crate::platform::command("sh")
            .args(["-c", r"printf 'bad \377\nafter\r\n'; echo last"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        assert!(watch(child, &run).unwrap().is_some_and(|e| e.success()));
        let heard = heard.lock().unwrap();
        for text in ["bad \u{fffd}", "after", "last"] {
            let detail = StepEvent::Detail { text: text.into() };
            assert!(heard.contains(&detail), "{text:?} in {heard:?}");
        }
    }

    #[test]
    fn one_rebuild_per_target_at_a_time() {
        let held = Held::take("FakeHeld").unwrap();
        assert!(Held::take("FakeHeld").is_err());
        let other = Held::take("FakeOther").unwrap();
        drop((held, other));
        assert!(Held::take("FakeHeld").is_ok());
    }
}
