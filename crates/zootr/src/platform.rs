//! What differs between macOS, Windows and Linux: per-user folders,
//! finding a running app, and opening apps and folders.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

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

/// Whether a process was started from `app`'s path.
#[cfg(unix)]
pub fn running(app: &Path) -> bool {
    use std::process::Stdio;
    let pattern: String = app
        .to_string_lossy()
        .chars()
        .flat_map(|c| {
            let special = "\\^$.|?*+()[]{}".contains(c);
            special.then_some('\\').into_iter().chain([c])
        })
        .collect();
    Command::new("pgrep")
        .args(["-f", &pattern])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
    let mut child = Command::new(app)
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
        succeeded(Command::new("xdg-open").arg(path.parent().unwrap_or(path)))
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

    fn at(os: &str, folder: Folder, vars: &[(&str, &str)]) -> Result<PathBuf, &'static str> {
        let vars: HashMap<_, _> = vars.iter().copied().collect();
        locate(folder, os, |k| vars.get(k).map(OsString::from))
    }

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    /// Play starts the app with the target's env, which is how a runner
    /// learns its control socket.
    #[cfg(target_os = "linux")]
    #[test]
    fn open_app_passes_env_to_what_it_starts() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("zootr-open-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("runner.sh");
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
