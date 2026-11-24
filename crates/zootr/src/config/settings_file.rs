//! A target's settings file: `KEY=VALUE` lines a seed sets on install.
//! Uninstall removes the ones that point into the mod and keeps the rest,
//! so a vanilla game still starts with them. Other lines stay as they are.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The placeholders that point into the mod folder, and what each becomes.
const MOD_PATHS: [(&str, &str); 2] = [("{romfs}", "romfs"), ("{cheats}", "cheats")];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SettingsFile {
    pub file: PathBuf,
    /// `KEY=VALUE` lines. In a value `{romfs}` becomes the mod's romfs,
    /// `{cheats}` its cheat folder and `{control}` the target's control
    /// file.
    #[serde(alias = "line", deserialize_with = "one_or_more")]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub lines: Vec<String>,
}

/// A list, or the single string configs held before there could be more.
fn one_or_more<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lines {
        One(String),
        More(Vec<String>),
    }
    Ok(match Lines::deserialize(d)? {
        Lines::One(line) => vec![line],
        Lines::More(lines) => lines,
    })
}

impl SettingsFile {
    /// Set these lines in `file`, replacing lines with the same keys.
    pub fn apply(&self, mod_dir: &Path, control: Option<&Path>) -> Result<()> {
        self.rewrite(self.filled(mod_dir, control, |_| true)?)
    }

    /// Drop the lines that point into the mod (`{romfs}`, `{cheats}`) from
    /// `file`, and set the rest: the game runs vanilla with them still on.
    pub fn remove(&self, control: Option<&Path>) -> Result<()> {
        let unmodded = |l: &str| !MOD_PATHS.iter().any(|(from, _)| l.contains(from));
        self.rewrite(self.filled(Path::new(""), control, unmodded)?)
    }

    /// The lines `keep` passes, with `{romfs}`, `{cheats}` and `{control}`
    /// filled in.
    fn filled(
        &self,
        mod_dir: &Path,
        control: Option<&Path>,
        keep: impl Fn(&str) -> bool,
    ) -> Result<Vec<String>> {
        let fill = |l: &String| {
            let l = MOD_PATHS.iter().fold(l.clone(), |l, (from, to)| {
                l.replace(from, &mod_dir.join(to).to_string_lossy())
            });
            match control {
                Some(c) => Ok(l.replace("{control}", &c.to_string_lossy())),
                None if l.contains("{control}") => {
                    bail!(
                        "settings line {l:?} names {{control}}, but the target has no control file"
                    )
                }
                None => Ok(l),
            }
        };
        self.lines.iter().filter(|l| keep(l)).map(fill).collect()
    }

    /// `file` without these lines' keys, plus `add`.
    fn rewrite(&self, add: Vec<String>) -> Result<()> {
        let mut keys = vec![];
        for line in &self.lines {
            let Some((key, _)) = line.split_once('=') else {
                bail!("settings line {line:?} has no `=`");
            };
            keys.push(format!("{key}="));
        }
        let old = match std::fs::read_to_string(&self.file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && add.is_empty() => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", self.file.display())),
        };
        let kept = old
            .lines()
            .filter(|l| !keys.iter().any(|k| l.starts_with(k)));
        let lines: Vec<_> = kept.map(str::to_owned).chain(add).collect();
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, lines.join("\n") + "\n")
            .with_context(|| format!("writing {}", self.file.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::scratch;
    use super::*;
    use crate::config::Target;

    #[test]
    fn settings_lines_replace_their_keys_and_keep_the_rest() {
        let dir = scratch("line");
        let file = dir.join("env.txt");
        std::fs::write(&file, "A=1\nKEY=old\nB=2\nC=old\n").unwrap();
        let lines = SettingsFile {
            file: file.clone(),
            lines: vec![
                "KEY={romfs}".into(),
                "C={cheats}".into(),
                "IN={control}".into(),
            ],
        };
        let (m, control) = (Path::new("/m"), Path::new("/c/input.txt"));
        lines.apply(m, Some(control)).unwrap();
        lines.apply(m, Some(control)).unwrap();
        let (romfs, cheats) = (m.join("romfs"), m.join("cheats"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!(
                "A=1\nB=2\nKEY={}\nC={}\nIN={}\n",
                romfs.display(),
                cheats.display(),
                control.display()
            )
        );
        let err = lines.apply(m, None).unwrap_err().to_string();
        assert!(err.contains("no control file"), "{err}");
    }

    #[test]
    fn an_old_single_settings_line_still_loads() {
        let old =
            r#"{"name":"t","app":"/a","mod_dir":"/m","settings_line":{"file":"/e","line":"K=v"}}"#;
        let t: Target = serde_json::from_str(old).unwrap();
        assert_eq!(t.settings_file.unwrap().lines, ["K=v"]);
    }

    #[test]
    fn removing_drops_only_the_lines_that_point_into_the_mod() {
        let dir = scratch("remove");
        let file = dir.join("env.txt");
        let only_mod = SettingsFile {
            file: file.clone(),
            lines: vec!["KEY={romfs}".into(), "C={cheats}".into()],
        };
        only_mod.remove(None).unwrap();
        assert!(!file.exists());
        let lines = SettingsFile {
            file: file.clone(),
            lines: vec![
                "KEY={romfs}".into(),
                "C={cheats}".into(),
                "FOCUS=1".into(),
                "IN={control}".into(),
            ],
        };
        std::fs::write(&file, "A=1\nKEY=/m/romfs\nB=2\nC=/m/cheats\nFOCUS=1\n").unwrap();
        lines.remove(Some(Path::new("/c/in.txt"))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "A=1\nB=2\nFOCUS=1\nIN=/c/in.txt\n"
        );
    }

    #[test]
    fn settings_line_creates_a_missing_file() {
        let file = scratch("new").join("sub/env.txt");
        let line = SettingsFile {
            file: file.clone(),
            lines: vec!["K=v".into()],
        };
        line.apply(Path::new("/m"), None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "K=v\n");
    }
}
