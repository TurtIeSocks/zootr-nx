//! Our own names for values the game never names. Built-in ones ship in
//! `labels.txt`, the user's own live in a `labels.txt` next to the config
//! file and win over them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::Config;

const BUILT_IN: &str = include_str!("../labels.txt");

/// Labels by hash: one per line, a hex hash then the label. `#` starts a
/// comment line.
pub fn parse(text: &str) -> HashMap<u32, String> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .filter_map(|(h, label)| Some((u32::from_str_radix(h, 16).ok()?, label.trim().into())))
        .collect()
}

pub fn path() -> Result<PathBuf> {
    Ok(Config::path()?.with_file_name("labels.txt"))
}

/// The built-in labels with the user's file over them, if it exists.
pub fn load(user: &Path) -> HashMap<u32, String> {
    let mut labels = parse(BUILT_IN);
    labels.extend(parse(&std::fs::read_to_string(user).unwrap_or_default()));
    labels
}

/// Label `hash` in the user's file, or drop its label for empty `text`.
pub fn set(user: &Path, hash: u32, text: &str) -> Result<()> {
    let old = std::fs::read_to_string(user).unwrap_or_default();
    let key = format!("{hash:08x} ");
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| !l.starts_with(&key))
        .map(String::from)
        .collect();
    let text = text.trim();
    if !text.is_empty() {
        lines.push(format!("{key}{text}"));
    }
    if let Some(dir) = user.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(user, body).with_context(|| format!("writing {}", user.display()))
}

#[cfg(test)]
pub(crate) fn built_in() -> HashMap<u32, String> {
    parse(BUILT_IN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_label_parses_once() {
        let lines = BUILT_IN.lines().filter(|l| !l.starts_with('#'));
        let labels = built_in();
        assert_eq!(labels.len(), lines.count());
        assert!(labels.values().all(|l| !l.is_empty()));
    }

    #[test]
    fn user_labels_win_and_can_be_removed() {
        let dir = std::env::temp_dir().join(format!("zootr-labels-{}", std::process::id()));
        let user = dir.join("labels.txt");
        let _ = std::fs::remove_dir_all(&dir);
        set(&user, 0xc66e58f2, "hud rupees").unwrap();
        set(&user, 0x1, " one ").unwrap();
        assert_eq!(load(&user)[&0xc66e58f2], "hud rupees");
        assert_eq!(load(&user)[&0x1], "one");
        set(&user, 0x1, "").unwrap();
        set(&user, 0xc66e58f2, "").unwrap();
        let labels = load(&user);
        assert!(!labels.contains_key(&0x1));
        assert!(labels[&0xc66e58f2].starts_with("rupees"));
        assert_eq!(std::fs::read_to_string(&user).unwrap(), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
