//! Keeps the game's own names out of our source as text. A name is written
//! as a `known::*` const (`n!(0x…)`), or in the logic vocabulary. This scans
//! the files below and fails on a game-shaped identifier.

use std::path::{Path, PathBuf};

/// The prefixes the game's identifiers start with.
const PREFIXES: [&str; 8] = ["Item", "TBox", "Obj", "Armor", "Npc", "Enemy", "Dm", "Beat"];

/// What to scan, as `(folder, extension)` under the workspace root. To
/// guard the logic rule files too, add `("crates/zootr/logic", "toml")`.
const SCANNED: [(&str, &str); 1] = [("crates", "rs")];

/// Files and folders not scanned yet, by path prefix from the workspace
/// root. TEMPORARY: another branch is cleaning these. Delete this list once
/// that branch has merged.
const NOT_YET: [&str; 6] = [
    "crates/zootr/src/rules.rs",
    "crates/zootr/src/logic.rs",
    "crates/zootr/src/tools.rs",
    "crates/zootr/src/probe/logic.rs",
    "crates/zootr/logic/",
    // The guard's own examples.
    "crates/zootr/src/known/guard.rs",
];

/// Files that hold a name as text because nothing can read it from the
/// dump there: the hook runs inside the game, and the extract runs before
/// a dump is open.
const NO_DUMP: [&str; 2] = [
    "crates/zootr-hook/src/game/dev.rs",
    "crates/zootr/src/medallions.rs",
];

/// Whether `word`, what follows a prefix and its `_`, is a game name. Made-up
/// fixtures (`Item_A`, `Dm_X_0000`) and bare prefixes (`Item_Melody_`, for a
/// name that goes on) are not.
fn is_name(word: &str) -> bool {
    if !word.starts_with(|c: char| c.is_ascii_uppercase()) || word.ends_with('_') {
        return false;
    }
    !word.split('_').all(|part| {
        part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() == 1 && part.starts_with(|c: char| c.is_ascii_uppercase()))
    })
}

/// The game names in `line`, as written.
fn names_in(line: &str) -> Vec<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut found = Vec::new();
    for prefix in PREFIXES {
        for (at, _) in line.match_indices(prefix) {
            let (before, after) = (&line[..at], &line[at + prefix.len()..]);
            let Some(rest) = after.strip_prefix('_') else {
                continue;
            };
            if before.ends_with(word) {
                continue;
            }
            let end = rest.find(|c: char| !word(c)).unwrap_or(rest.len());
            if is_name(&rest[..end]) {
                found.push(format!("{prefix}_{}", &rest[..end]));
            }
        }
    }
    found
}

fn files(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target"
                && name != "node_modules"
                && !name.to_string_lossy().starts_with('.')
            {
                files(&path, extension, out);
            }
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
}

/// Every `file:line: name` in what's scanned under `root`.
fn offenders(root: &Path) -> Vec<String> {
    let mut paths = Vec::new();
    for (dir, extension) in SCANNED {
        files(&root.join(dir), extension, &mut paths);
    }
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel = rel.to_string_lossy().replace('\\', "/");
        if NOT_YET.iter().chain(&NO_DUMP).any(|p| rel.starts_with(p)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            for name in names_in(line) {
                out.push(format!("{rel}:{}: {name}", i + 1));
            }
        }
    }
    out
}

#[test]
fn no_game_name_is_written_as_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = offenders(&root);
    assert!(
        found.is_empty(),
        "game names written as text. Use a `known::*` const (`n!(0x…)`, hash \
         from `zootr probe hash`) or a logic vocabulary word, or a made-up \
         name like `Item_A` in a test:\n{}",
        found.join("\n")
    );
}

#[test]
fn a_game_shaped_name_is_found_and_a_fixture_is_not() {
    let name = |prefix: &str, rest: &str| format!("{prefix}_{rest}");
    let real = format!("let x = \"{}\";", name("Item", "Hookshot"));
    assert_eq!(names_in(&real), [name("Item", "Hookshot")]);
    let path = format!(
        "{}/{}.pack",
        name("Pack", "Actor"),
        name("Enemy", "Stinger_02")
    );
    assert_eq!(names_in(&path), [name("Enemy", "Stinger_02")]);
    for fine in [
        "Item_A",
        "Npc_N_02",
        "Dm_X_0000",
        "Item_Melody_{s}",
        "Mt_Item_Medal_Forest",
        "Objective_Thing",
        "Item_",
        "Obj_lower",
    ] {
        assert!(names_in(fine).is_empty(), "{fine}");
    }
}
