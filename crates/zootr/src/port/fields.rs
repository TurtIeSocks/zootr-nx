//! The struct field offsets the hook reads, from its own source, and which
//! of them a matched window confirms: a window that matched exactly holds
//! the same unmasked immediates in both builds, so the fields it reads
//! are where they were.

use std::collections::HashSet;

use super::arm;
use super::image::{Image, Window};

/// The hook's files that name field offsets, by their path under its
/// `src`.
const SOURCES: &[(&str, &str)] = &[
    ("game.rs", include_str!("../../../zootr-hook/src/game.rs")),
    (
        "game/arrows.rs",
        include_str!("../../../zootr-hook/src/game/arrows.rs"),
    ),
    (
        "game/bunny.rs",
        include_str!("../../../zootr-hook/src/game/bunny.rs"),
    ),
    (
        "game/chests.rs",
        include_str!("../../../zootr-hook/src/game/chests.rs"),
    ),
    (
        "game/dev.rs",
        include_str!("../../../zootr-hook/src/game/dev.rs"),
    ),
    (
        "game/freeze.rs",
        include_str!("../../../zootr-hook/src/game/freeze.rs"),
    ),
    (
        "game/look.rs",
        include_str!("../../../zootr-hook/src/game/look.rs"),
    ),
    (
        "game/menu.rs",
        include_str!("../../../zootr-hook/src/game/menu.rs"),
    ),
    (
        "game/refill.rs",
        include_str!("../../../zootr-hook/src/game/refill.rs"),
    ),
    (
        "game/rt.rs",
        include_str!("../../../zootr-hook/src/game/rt.rs"),
    ),
    (
        "game/swap.rs",
        include_str!("../../../zootr-hook/src/game/swap.rs"),
    ),
    ("keys.rs", include_str!("../../../zootr-hook/src/keys.rs")),
    (
        "switches.rs",
        include_str!("../../../zootr-hook/src/switches.rs"),
    ),
    ("traps.rs", include_str!("../../../zootr-hook/src/traps.rs")),
];
/// `usize` consts there that count or size things rather than place a
/// field.
const NOT_FIELDS: &[&str] = &["MAX", "STRING64_SIZE"];

/// A field offset the hook reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// Its const, `NAME.i` for a tuple's `i`th.
    pub name: String,
    pub file: &'static str,
    pub value: u32,
    /// Whether a matched window reads at it.
    pub confirmed: bool,
}

/// A literal, or a sum of them.
fn number(expr: &str) -> Option<u32> {
    (expr.split('+').map(str::trim))
        .map(|t| {
            let t = t.replace('_', "");
            match t.strip_prefix("0x") {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => t.parse().ok(),
            }
        })
        .sum()
}

/// Every `usize` const in `src`: each a field, or for a type slot's
/// `(slot, stride, offset)` tuple, its stride and offset.
fn consts(file: &'static str, src: &str) -> Vec<Field> {
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.trim_start();
        let line = line
            .strip_prefix("pub(super) ")
            .or(line.strip_prefix("pub "))
            .unwrap_or(line);
        let Some(rest) = line.strip_prefix("const ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(':') else {
            continue;
        };
        let Some((ty, expr)) = rest.split_once('=') else {
            continue;
        };
        let (ty, expr) = (ty.trim(), expr.trim().trim_end_matches(';'));
        let field = |name: String, value| Field {
            name,
            file,
            value,
            confirmed: false,
        };
        if ty == "usize" && !NOT_FIELDS.contains(&name) {
            out.extend(number(expr).map(|v| field(name.to_string(), v)));
        } else if ty.starts_with("(usize") {
            let parts = expr.trim_matches(|c| c == '(' || c == ')').split(',');
            for (i, part) in parts.enumerate().skip(1) {
                out.extend(number(part).map(|v| field(format!("{name}.{i}"), v)));
            }
        }
    }
    out
}

/// Every field offset the hook names, each marked confirmed if a window in
/// `matched` (where each starts in `from`) reads at it.
pub fn confirm(from: &Image, matched: &[(Window, u32)]) -> Vec<Field> {
    let read: HashSet<u32> = (matched.iter())
        .flat_map(|(w, _)| w.each())
        .filter(|&at| from.unmasked(at))
        .filter_map(|at| arm::field(from.word(at)))
        .collect();
    let mut fields: Vec<Field> = SOURCES
        .iter()
        .flat_map(|&(f, src)| consts(f, src))
        .collect();
    for f in &mut fields {
        f.confirmed = read.contains(&f.value);
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hooks_fields_read_from_its_source() {
        let all: Vec<_> = SOURCES
            .iter()
            .flat_map(|&(f, src)| consts(f, src))
            .collect();
        let value = |name: &str| all.iter().find(|f| f.name == name).map(|f| f.value);
        assert_eq!(value("PROJECTILE"), Some(0x12b4));
        assert_eq!(value("BLOCK"), Some(0x1b20 + 0x100));
        assert_eq!(value("KEYS"), Some(0xc58));
        assert_eq!(value("STRING64.1"), Some(0x108));
        assert_eq!(value("STRING64.2"), Some(0x74));
        assert_eq!(value("STRING64.0"), None);
        assert_eq!(value("MAX"), None);
    }

    #[test]
    fn every_file_of_the_hooks_game_is_read() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../zootr-hook/src/game");
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = format!("game/{}", entry.unwrap().file_name().to_str().unwrap());
            assert!(SOURCES.iter().any(|(f, _)| *f == name), "{name}");
        }
    }
}
