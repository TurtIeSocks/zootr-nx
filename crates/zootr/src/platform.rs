//! What differs between macOS, Windows and Linux: per-user folders,
//! finding a running app, and opening apps and folders.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

/// A command for a program outside zootr. Started from the AppImage,
/// zootr's environment points into its mount (`LD_LIBRARY_PATH` and the
/// rest), and a child inheriting it loads zootr's libraries in place of its
/// own: podman broke that way under a target's rebuild.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for (key, value) in outside_appimage(std::env::vars_os()) {
        match value {
            Some(v) => command.env(key, v),
            None => command.env_remove(key),
        };
    }
    command
}

/// What `vars` change to for a program outside the AppImage: a value
/// pointing into the mount (`APPDIR`) loses those entries, and goes when
/// none are left. Outside an AppImage nothing changes.
fn outside_appimage(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, Option<OsString>)> {
    let vars: Vec<_> = vars.into_iter().collect();
    let mount = vars
        .iter()
        .find(|(k, _)| k == "APPDIR")
        .and_then(|(_, v)| v.to_str());
    let Some(mount) = mount.filter(|m| !m.is_empty()).map(str::to_owned) else {
        return Vec::new();
    };
    let into_mount = |(key, value): (OsString, OsString)| {
        let value = value.to_str().filter(|v| v.contains(&mount))?.to_owned();
        let kept: Vec<_> = (value.split(':'))
            .filter(|e| !e.is_empty() && !e.contains(&mount))
            .collect();
        Some((key, (!kept.is_empty()).then(|| kept.join(":").into())))
    };
    let markers = ["APPIMAGE", "ARGV0", "OWD"].map(|k| (k.into(), None));
    vars.into_iter()
        .filter_map(into_mount)
        .chain(markers)
        .collect()
}

/// A per-user folder zootr keeps its own subfolder in.
#[derive(Clone, Copy)]
pub(crate) enum Folder {
    Config,
    Cache,
}

/// zootr's `folder` for this OS and user. `env` is the override to suggest
/// when it can't be found.
pub(crate) fn user_dir(folder: Folder, env: &str) -> Result<PathBuf> {
    locate(folder, std::env::consts::OS, |k| std::env::var_os(k))
        .map_err(|home| anyhow!("{home} is not set; set {env} instead"))
}

/// zootr's `folder` on `os`, reading variables through `var`. Errs with the
/// home variable it needed.
fn locate(
    folder: Folder,
    os: &str,
    var: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, &'static str> {
    // A variable naming the folder, else a home variable and the folder in it.
    let (own, home, under): (_, _, &[&str]) = match (os, folder) {
        ("macos", Folder::Config) => (None, "HOME", &["Library", "Application Support"]),
        ("macos", Folder::Cache) => (None, "HOME", &["Library", "Caches"]),
        ("windows", Folder::Config) => (Some("APPDATA"), "USERPROFILE", &["AppData", "Roaming"]),
        ("windows", Folder::Cache) => (Some("LOCALAPPDATA"), "USERPROFILE", &["AppData", "Local"]),
        (_, Folder::Config) => (Some("XDG_CONFIG_HOME"), "HOME", &[".config"]),
        (_, Folder::Cache) => (Some("XDG_CACHE_HOME"), "HOME", &[".cache"]),
    };
    let var = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = match own.and_then(var) {
        Some(dir) => dir,
        None => under.iter().fold(var(home).ok_or(home)?, |p, d| p.join(d)),
    };
    Ok(base.join("zootr"))
}

/// Whether a process was started from `app`'s path: its command line opens
/// with the path (the app, or a file inside a .app), or with a shell and
/// then the path (a script target). A process that only mentions the path
/// later on, like a shell command naming it, isn't the app.
#[cfg(unix)]
pub fn running(app: &Path) -> bool {
    use std::process::Stdio;
    let path: String = app
        .to_string_lossy()
        .chars()
        .flat_map(|c| {
            let special = "\\^$.|?*+()[]{}".contains(c);
            special.then_some('\\').into_iter().chain([c])
        })
        .collect();
    let pattern = format!("^([^ ]+ )?{path}(/| |$)");
    command("pgrep")
        .args(["-f", &pattern])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Where Linux lists its processes, one folder each.
pub const PROC: &str = "/proc";

/// Whether a process listed under `proc` (one folder per pid, as in
/// [`PROC`]) works in `folder` or names a path inside it on its command
/// line, zootr itself aside. `None` when `proc` can't be read, as on any
/// system without one.
pub fn inside(proc: &Path, folder: &Path) -> Option<bool> {
    let me = std::process::id().to_string();
    let pids = (std::fs::read_dir(proc).ok()?.flatten()).filter(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name != me && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit())
    });
    let names = |arg: &[u8]| Path::new(&*String::from_utf8_lossy(arg)).starts_with(folder);
    Some(pids.map(|e| e.path()).any(|pid| {
        std::fs::read_link(pid.join("cwd")).is_ok_and(|c| c.starts_with(folder))
            || std::fs::read(pid.join("cmdline")).is_ok_and(|a| a.split(|b| *b == 0).any(names))
    }))
}

/// Whether a process's executable is `app`, or inside it.
#[cfg(windows)]
pub fn running(app: &Path) -> bool {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn K32EnumProcesses(pids: *mut u32, bytes: u32, used: *mut u32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            name: *mut u16,
            len: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    let mut pids = vec![0u32; 1024];
    loop {
        let (bytes, mut used) = ((pids.len() * 4) as u32, 0);
        if unsafe { K32EnumProcesses(pids.as_mut_ptr(), bytes, &mut used) } == 0 {
            return false;
        }
        if used < bytes {
            pids.truncate(used as usize / 4);
            break;
        }
        pids.resize(pids.len() * 2, 0);
    }
    // Windows paths ignore case.
    let lower = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
    let app = lower(app);
    pids.into_iter().any(|pid| {
        let process = unsafe { OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return false;
        }
        let mut name = [0u16; 1024];
        let mut len = name.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut len) };
        unsafe { CloseHandle(process) };
        ok != 0
            && lower(&PathBuf::from(OsString::from_wide(&name[..len as usize]))).starts_with(&app)
    })
}

/// Opens a game like [`open_app`], first emptying its control file when
/// this starts it: the game reads the file from the top, so lines the last
/// run left would replay before any new one.
pub fn open_game(
    app: &Path,
    args: &[String],
    env: &[(String, String)],
    control: Option<&Path>,
) -> Result<()> {
    fresh_control(app, control)?;
    open_app(app, args, env)
}

fn fresh_control(app: &Path, control: Option<&Path>) -> Result<()> {
    match control {
        Some(c) if c.exists() && !running(app) => {
            std::fs::write(c, "").with_context(|| format!("emptying {}", c.display()))
        }
        _ => Ok(()),
    }
}

/// Opens `app`, passing `args` and `env` when this starts it.
pub fn open_app(app: &Path, args: &[String], env: &[(String, String)]) -> Result<()> {
    if cfg!(target_os = "macos") {
        let mut open = Command::new("open");
        for (name, value) in env {
            open.arg("--env").arg(format!("{name}={value}"));
        }
        open.arg(app);
        if !args.is_empty() {
            // Only reaches the app when this starts it, not when it's already open.
            open.arg("--args").args(args);
        }
        return succeeded(&mut open);
    }
    // Like `open`, an app that's already running isn't started twice.
    if running(app) {
        return Ok(());
    }
    let mut child = command(app)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .current_dir(app.parent().unwrap_or(Path::new(".")))
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Shows `path` in the system's file browser.
#[cfg(windows)]
pub fn reveal(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // Explorer reads its own command line, and exits 1 even when it works.
    Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn()?;
    Ok(())
}

/// Shows `path` in the system's file browser.
#[cfg(not(windows))]
pub fn reveal(path: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        succeeded(Command::new("open").arg("-R").arg(path))
    } else {
        succeeded(command("xdg-open").arg(path.parent().unwrap_or(path)))
    }
}

fn succeeded(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        bail!("{:?} exited with {status}", command.get_program());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn a_program_started_from_the_appimage_leaves_its_mount_behind() {
        let vars = |pairs: &[(&str, &str)]| -> Vec<(OsString, OsString)> {
            pairs.iter().map(|(k, v)| (k.into(), v.into())).collect()
        };
        let changed = outside_appimage(vars(&[
            ("APPDIR", "/tmp/.mount_zootr-x"),
            ("PATH", "/tmp/.mount_zootr-x/usr/bin/:/usr/bin:/bin"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_zootr-x/usr/lib/:"),
            ("HOME", "/home/a"),
        ]));
        let to = |k: &str| changed.iter().find(|(c, _)| c == k).map(|(_, v)| v.clone());
        assert_eq!(to("PATH"), Some(Some("/usr/bin:/bin".into())));
        assert_eq!(to("LD_LIBRARY_PATH"), Some(None));
        assert_eq!(to("APPDIR"), Some(None));
        assert_eq!(to("HOME"), None);
        assert!(outside_appimage(vars(&[("PATH", "/usr/bin")])).is_empty());
    }

    fn at(os: &str, folder: Folder, vars: &[(&str, &str)]) -> Result<PathBuf, &'static str> {
        let vars: HashMap<_, _> = vars.iter().copied().collect();
        locate(folder, os, |k| vars.get(k).map(OsString::from))
    }

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    /// A fake process list: `cwds` and `cmdlines` by pid.
    #[cfg(unix)]
    fn procs(name: &str, cwds: &[(u32, &Path)], cmdlines: &[(u32, &[&str])]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("zootr-proc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("self")).unwrap();
        for (pid, cwd) in cwds {
            std::fs::create_dir_all(root.join(pid.to_string())).unwrap();
            std::os::unix::fs::symlink(cwd, root.join(pid.to_string()).join("cwd")).unwrap();
        }
        for (pid, args) in cmdlines {
            let dir = root.join(pid.to_string());
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("cmdline"), args.join("\0") + "\0").unwrap();
        }
        root
    }

    #[cfg(unix)]
    #[test]
    fn a_process_is_inside_a_folder_by_its_cwd_or_a_path_it_names() {
        let folder = Path::new("/fake/build");
        let watch: &[&str] = &[
            "/opt/fake-game",
            "--crash-watch",
            "12",
            "ff",
            "/fake/build/",
        ];
        let named = procs("named", &[], &[(12, watch)]);
        assert_eq!(inside(&named, folder), Some(true));
        let sub = Path::new("/fake/build/bin");
        let working = procs("working", &[(13, sub)], &[(13, &["./game"])]);
        assert_eq!(inside(&working, folder), Some(true));
        let near: &[&str] = &["/fake/build-other/game", "/fake/buildx"];
        let elsewhere = procs("elsewhere", &[(14, Path::new("/fake"))], &[(14, near)]);
        assert_eq!(inside(&elsewhere, folder), Some(false));
        // zootr itself, and folders that aren't a pid, don't count.
        let me = std::process::id();
        let mine = procs("mine", &[(me, folder)], &[]);
        std::os::unix::fs::symlink(folder, mine.join("self").join("cwd")).unwrap();
        assert_eq!(inside(&mine, folder), Some(false));
        assert_eq!(inside(Path::new("/nowhere/proc"), folder), None);
        for root in [named, working, elsewhere, mine] {
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    /// A process counts only when it is the app, or a shell running it, never
    /// one that merely names the path (a command line mentioning it).
    #[cfg(unix)]
    #[test]
    fn running_is_the_app_not_a_mention_of_it() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;
        let dir = std::env::temp_dir().join(format!("zootr-running-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("Fake App.sh");
        std::fs::write(&app, "#!/bin/sh\nsleep 5\n").unwrap();
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        let pause = || std::thread::sleep(std::time::Duration::from_millis(300));

        let quoted = format!("sleep 5; true # \"{}\"", app.display());
        let mut mention = Command::new("sh").args(["-c", &quoted]).spawn().unwrap();
        pause();
        let mentioned = running(&app);
        mention.kill().ok();
        mention.wait().ok();

        let mut real = Command::new(&app).spawn().unwrap();
        pause();
        let started = running(&app);
        real.kill().ok();
        real.wait().ok();
        std::fs::remove_dir_all(&dir).ok();

        assert!(!mentioned, "a shell naming the path isn't the app");
        assert!(started, "the app, run through its shell, is");
    }

    /// Play starts the app with the target's env, which is how a target
    /// learns its control socket.
    #[cfg(target_os = "linux")]
    #[test]
    fn open_app_passes_env_to_what_it_starts() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("zootr-open-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("target.sh");
        let out = dir.join("out.txt");
        std::fs::write(
            &app,
            "#!/bin/sh\nprintf %s \"$ZOOTR_SOCK\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        let env = [("ZOOTR_SOCK".to_owned(), "/run/c.sock".to_owned())];
        open_app(&app, &[out.to_string_lossy().into()], &env).unwrap();
        let read = (0..100).find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::fs::read_to_string(&out).ok()
        });
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(read.as_deref(), Some("/run/c.sock"));
    }

    #[test]
    fn a_start_empties_the_control_file_and_creates_none() {
        let dir = std::env::temp_dir().join(format!("zootr-fresh-ctl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (app, control) = (dir.join("none.app"), dir.join("input.txt"));
        std::fs::write(&control, "A\nLUp~1.5\n").unwrap();
        fresh_control(&app, Some(&control)).unwrap();
        assert_eq!(std::fs::read_to_string(&control).unwrap(), "");
        let missing = dir.join("missing.txt");
        fresh_control(&app, Some(&missing)).unwrap();
        assert!(!missing.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn macos_folders_are_under_library() {
        let home = [("HOME", "/h"), ("XDG_CONFIG_HOME", "/x")];
        assert_eq!(
            at("macos", Folder::Config, &home),
            Ok(path(&["/h", "Library", "Application Support", "zootr"]))
        );
        assert_eq!(
            at("macos", Folder::Cache, &home),
            Ok(path(&["/h", "Library", "Caches", "zootr"]))
        );
        assert_eq!(at("macos", Folder::Config, &[]), Err("HOME"));
    }

    #[test]
    fn windows_folders_are_appdata_then_under_the_profile() {
        let set = [
            ("APPDATA", r"C:\Users\p\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\p\AppData\Local"),
            ("HOME", "/h"),
        ];
        assert_eq!(
            at("windows", Folder::Config, &set),
            Ok(path(&[r"C:\Users\p\AppData\Roaming", "zootr"]))
        );
        assert_eq!(
            at("windows", Folder::Cache, &set),
            Ok(path(&[r"C:\Users\p\AppData\Local", "zootr"]))
        );
        let profile = [("USERPROFILE", r"C:\Users\p"), ("APPDATA", "")];
        assert_eq!(
            at("windows", Folder::Config, &profile),
            Ok(path(&[r"C:\Users\p", "AppData", "Roaming", "zootr"]))
        );
        assert_eq!(
            at("windows", Folder::Cache, &profile),
            Ok(path(&[r"C:\Users\p", "AppData", "Local", "zootr"]))
        );
        assert_eq!(
            at("windows", Folder::Cache, &[("HOME", "/h")]),
            Err("USERPROFILE")
        );
    }

    #[test]
    fn linux_folders_are_xdg_then_under_home() {
        let xdg = [
            ("XDG_CONFIG_HOME", "/c"),
            ("XDG_CACHE_HOME", "/k"),
            ("HOME", "/h"),
        ];
        assert_eq!(
            at("linux", Folder::Config, &xdg),
            Ok(path(&["/c", "zootr"]))
        );
        assert_eq!(at("linux", Folder::Cache, &xdg), Ok(path(&["/k", "zootr"])));
        let home = [("HOME", "/h"), ("XDG_CACHE_HOME", "")];
        assert_eq!(
            at("linux", Folder::Config, &home),
            Ok(path(&["/h", ".config", "zootr"]))
        );
        assert_eq!(
            at("freebsd", Folder::Cache, &home),
            Ok(path(&["/h", ".cache", "zootr"]))
        );
        assert_eq!(at("linux", Folder::Config, &[]), Err("HOME"));
    }
}
