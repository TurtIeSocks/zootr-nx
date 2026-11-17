//! A run's log: what it generates from, then each step as it ends. Kept
//! beside the live logs, with the home folder as `~`, so a player can post
//! it with a bug report.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{End, Run, StepEvent, Stopped, line};
use crate::config::Target;
use crate::generate::Extras;
use crate::settings::Settings;
use crate::{config, game, recorder, share};

/// What a seed is generated from, for its log.
pub struct About<'a> {
    pub seed: u64,
    pub settings: &'a Settings,
    pub extras: &'a Extras,
    /// The game dump.
    pub game: &'a Path,
    /// Each target's name and mod folder, or the one folder written.
    pub folders: Vec<(String, PathBuf)>,
}

impl About<'_> {
    /// Each target's name and mod folder.
    pub fn folders(targets: &[Target]) -> Vec<(String, PathBuf)> {
        let folders = targets.iter();
        folders
            .map(|t| (t.name.clone(), t.mod_dir.clone()))
            .collect()
    }

    fn lines(&self) -> Vec<String> {
        let dump = match std::fs::metadata(self.game) {
            Ok(m) if m.is_dir() => "a folder".to_owned(),
            Ok(m) => format!("a file of {:.1} GB", m.len() as f64 / 1e9),
            Err(e) => format!("unreadable: {e}"),
        };
        let cache = game::cache().map_or_else(|e| format!("{e:#}"), |c| c.display().to_string());
        let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
        let mut lines = vec![
            format!("zootr {} on {os} {arch}", env!("CARGO_PKG_VERSION")),
            format!(
                "share {}",
                share::encode(self.seed, self.settings, self.extras)
            ),
            format!("dump {}, {dump}", self.game.display()),
            format!("cache {cache}"),
        ];
        let folders = self.folders.iter();
        lines.extend(folders.map(|(name, dir)| format!("target {name}: {}", dir.display())));
        lines.push(String::new());
        lines
    }
}

impl Run {
    /// This run, also logged to a new `generate-<time>.log` beside the live
    /// logs. A log that can't be opened is left out.
    pub fn logged(self, about: &About) -> Run {
        match recorder::logs() {
            Ok(dir) => self.logged_in(&dir, about, config::home().as_deref()),
            Err(_) => self,
        }
    }

    fn logged_in(mut self, dir: &Path, about: &About, home: Option<&Path>) -> Run {
        let Ok((path, file)) = recorder::open(dir, "generate") else {
            return self;
        };
        let home = home.map(|h| h.to_string_lossy().into_owned());
        let home = home.filter(|h| !h.is_empty());
        let file = Mutex::new(file);
        let write = move |text: &str| {
            let text = match &home {
                Some(h) => text.replace(h.as_str(), "~"),
                None => text.to_owned(),
            };
            let _ = writeln!(file.lock().unwrap(), "{text}");
        };
        about.lines().iter().for_each(|l| write(l));
        self.log = Some(path);
        self.with(move |e| {
            if let Some(l) = line(e) {
                write(&l);
            }
            match e {
                StepEvent::End {
                    end: End::Failed { error },
                    ..
                } => write(error),
                StepEvent::End {
                    end: End::Stopped { took_off: true },
                    ..
                } => write(&Stopped { took_off: true }.to_string()),
                _ => {}
            }
        })
    }

    /// The run's log file, if it has one.
    pub fn log(&self) -> Option<&Path> {
        self.log.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::Step;

    #[test]
    fn a_log_hides_the_home_folder_and_keeps_the_newest_of_its_kind() {
        let dir = std::env::temp_dir().join(format!("zootr-genlog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.join("home");
        let old = |i| dir.join(format!("generate-2000-01-01-{i:06}.log"));
        (0..20).for_each(|i| std::fs::write(old(i), "").unwrap());
        let live = dir.join("live-2000-01-01-000000.log");
        std::fs::write(&live, "").unwrap();
        let about = About {
            seed: 7,
            settings: &Settings::new(),
            extras: &Extras::default(),
            game: &home.join("oot.nup"),
            folders: vec![("t".into(), home.join("mod"))],
        };
        let run = Run::silent().logged_in(&dir, &about, Some(&home));
        run.step(Step::Checks).unwrap();
        let err = anyhow::anyhow!("broke at {}", home.join("x").display());
        run.end(&Err::<(), _>(err));
        let text = std::fs::read_to_string(run.log().unwrap()).unwrap();
        let tilde = |rest: &str| Path::new("~").join(rest).display().to_string();
        assert!(!text.contains(&*home.to_string_lossy()), "{text}");
        assert!(
            text.contains(&format!("target t: {}", tilde("mod"))),
            "{text}"
        );
        assert!(
            text.contains(&format!("dump {}", tilde("oot.nup"))),
            "{text}"
        );
        assert!(text.contains("share ZR1-") && text.contains("reading checks"));
        assert!(text.contains(&format!("broke at {}", tilde("x"))), "{text}");
        assert!(!old(0).exists() && old(1).exists() && live.exists());
        let logs = std::fs::read_dir(&dir).unwrap().count() - 1;
        assert_eq!(logs, 20);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
