//! A game that bakes the mods' exefs patches in at build time: whether its
//! last build saw the hook patch installed now, and rebuilding it.
//!
//! The build lists the patches it applied in its manifest: `#` comment
//! lines, then one tab-separated line each of mod, file, the file's FNV-1a,
//! kind and note.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::Target;
use crate::patch;
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
#[serde(tag = "kind", rename_all = "camelCase")]
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
            Status::Stale => Some(format!(
                "{name} needs a rebuild: its hook patch changed since the last build"
            )),
            Status::Refused { note } => Some(format!(
                "{name}'s last build refused the hook patch: {note}"
            )),
            Status::Unrecorded => Some(format!(
                "{name}'s build doesn't record its patches, so it may not load them. Rebuild it with a Hookshot that has ExeFS patch support"
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
    let name = &target.name;
    let args =
        (target.rebuild_args(game)?).with_context(|| format!("{name} has no rebuild command"))?;
    if target.running() {
        bail!("Close the game first: {name} is running");
    }
    let _held = Held::take(name)?;
    run.step(Step::Rebuilding)?;
    let (program, rest) = args.split_first().context("an empty rebuild command")?;
    let mut child = (Command::new(program).args(rest))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {program}"))?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    std::thread::scope(|s| {
        if let Some(err) = err {
            s.spawn(|| stream(err, run));
        }
        if let Some(out) = out {
            stream(out, run);
        }
    });
    let exit = child.wait()?;
    if !exit.success() {
        bail!("{name}'s rebuild command failed ({exit})");
    }
    match status(target).unwrap_or(Status::Unrecorded) {
        Status::Current => Ok(Status::Current),
        Status::Stale => bail!(
            "{name}'s rebuild command finished, but the build still doesn't have the installed hook patch: the command didn't build the game folder {} is in",
            target.manifest().unwrap_or_default().display()
        ),
        other => bail!("{}", other.text(name).unwrap_or_default()),
    }
}

/// Each line read from `from` as a detail of `run`.
fn stream(from: impl Read, run: &Run) {
    (BufReader::new(from).lines())
        .map_while(Result::ok)
        .for_each(|line| run.detail(line));
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

    use super::*;
    use crate::config::Rebuild;

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
        assert_eq!(status(&t), Some(Status::Stale), "patch removed, line left");

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
        assert!(
            text.contains("refused the hook patch: fake reason"),
            "{text}"
        );
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
        let heard = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = heard.clone();
        let run = Run::silent().with(move |e| sink.lock().unwrap().push(e.clone()));
        assert_eq!(rebuild(&t, None, &run).unwrap(), Status::Current);
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
        let err = rebuild(&t, None, &Run::silent()).unwrap_err().to_string();
        assert!(err.contains("didn't build the game folder"), "{err}");
        assert_eq!(status(&t), Some(Status::Stale));
        let failed = rebuilt_by(&t, "exit 3".into());
        let err = rebuild(&failed, None, &Run::silent())
            .unwrap_err()
            .to_string();
        assert!(err.contains("rebuild command failed"), "{err}");
        let none = Target {
            rebuild: Some(Rebuild::default()),
            ..t.clone()
        };
        let err = rebuild(&none, None, &Run::silent())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no rebuild command"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
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
