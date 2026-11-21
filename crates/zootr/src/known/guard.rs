//! Keeps the game's own names out of our source as text. A name is written
//! as a `known::*` const (`n!(0x…)`), or in the logic vocabulary. This scans
//! the files below and fails on a game-shaped identifier. The first test knows
//! the game's prefixes. The second asks the dump, so a name with any prefix
//! (or none) is found too.

use std::path::{Path, PathBuf};

/// The prefixes the game's identifiers start with.
const PREFIXES: [&str; 8] = ["Item", "TBox", "Obj", "Armor", "Npc", "Enemy", "Dm", "Beat"];

/// What to scan, as `(folder, extension)` under the workspace root. The app's
/// generated `bindings.ts` is scanned too and holds no names.
const SCANNED: [(&str, &str); 4] = [
    ("crates", "rs"),
    ("crates/zootr/logic", "toml"),
    ("app/src", "ts"),
    ("app/src", "tsx"),
];

/// Files the guard skips: its own examples.
const OWN_EXAMPLES: [&str; 1] = ["crates/zootr/src/known/guard.rs"];

/// Files that hold a name as text because nothing can read it from the
/// dump there: the hook runs inside the game, and the extract runs before
/// a dump is open.
const NO_DUMP: [&str; 7] = [
    "crates/zootr-hook/src/game/dev.rs",
    "crates/zootr/src/bombchu_bags/look.rs",
    "crates/zootr/src/foreign.rs",
    "crates/zootr/src/key_tints.rs",
    "crates/zootr/src/magic/look.rs",
    "crates/zootr/src/medallions.rs",
    "crates/zootr/src/sprites.rs",
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

/// The game names in `line`, as written. A part marked `Fake`
/// (`Item_FakeBow`) makes a test's name read as made up, and one marked
/// `Rando` names an actor of our own.
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
            let made_up =
                (rest[..end].split('_')).any(|p| p.starts_with("Fake") || p.starts_with("Rando"));
            if is_name(&rest[..end]) && !made_up {
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

/// Each scanned file under `root` as `(relative path, text)`, sorted.
fn scanned_texts(root: &Path) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for (dir, extension) in SCANNED {
        files(&root.join(dir), extension, &mut paths);
    }
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel = rel.to_string_lossy().replace('\\', "/");
        if OWN_EXAMPLES
            .iter()
            .chain(&NO_DUMP)
            .any(|p| rel.starts_with(p))
        {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.push((rel, text));
        }
    }
    out
}

/// Every `file:line: name` in what's scanned under `root`.
fn offenders(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, text) in scanned_texts(root) {
        for (i, line) in text.lines().enumerate() {
            for name in names_in(line) {
                out.push(format!("{rel}:{}: {name}", i + 1));
            }
        }
    }
    out
}

/// The identifier-shaped words in `line` with an underscore, as written:
/// `[A-Z][A-Za-z0-9]*(_[A-Za-z0-9]+)+`, whole (not part of a longer word).
fn underscored_words(line: &str) -> Vec<&str> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let shaped = |w: &str| {
        w.starts_with(|c: char| c.is_ascii_uppercase())
            && w.contains('_')
            && w.split('_').skip(1).all(|part| !part.is_empty())
    };
    line.split(|c: char| !word(c))
        .filter(|w| shaped(w))
        .collect()
}

/// Every `file:line: name` in what's scanned under `root` that `known` says
/// is a name in the dump, unless it is a made-up fixture.
fn catalog_offenders(root: &Path, in_dump: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, text) in scanned_texts(root) {
        for (i, line) in text.lines().enumerate() {
            for name in underscored_words(line) {
                let rest = name.split_once('_').map_or("", |(_, rest)| rest);
                if is_name(rest) && in_dump(name) {
                    out.push(format!("{rel}:{}: {name}", i + 1));
                }
            }
        }
    }
    out
}

#[test]
fn no_game_name_is_written_as_text() {
    needs_names!();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = offenders(&root);
    assert!(
        found.is_empty(),
        "game names written as text. Use a `known::*` const (`n!(0x…)`, hash \
         from `zootr probe hash`), a logic vocabulary word, a glyph in the \
         app, or a made-up name like `Item_A` in a test:\n{}",
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
        "Mt_Item_FakeMedal_Forest",
        "Objective_FakeThing",
        "Item_FakeBow",
        "Item_RandoFakeBag_Small",
        "Obj_FakeHeart_Piece",
        "Item_",
        "Obj_lower",
    ] {
        assert!(names_in(fine).is_empty(), "{fine}");
    }
}

#[test]
fn no_dump_name_is_written_as_text() {
    if crate::game::dev_dump().is_none() {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = catalog_offenders(&root, super::is_dump_name);
    assert!(
        found.is_empty(),
        "names from the game's data written as text. Use a `known::*` const \
         (`n!(0x…)`, hash from `zootr probe hash`), say it in words in a \
         comment, or a made-up name like `Map_A` in a test:\n{}",
        found.join("\n")
    );
}

#[test]
fn an_underscored_word_is_found_whole_and_a_fixture_is_not_a_hit() {
    let name = |a: &str, b: &str| format!("{a}_{b}");
    let line = format!(
        "f(\"{}\", x{}, {}_);",
        name("Map", "FakeReal"),
        name("Y", "z"),
        "Item"
    );
    assert_eq!(underscored_words(&line), [name("Map", "FakeReal")]);
    let has = |n: &str| n == name("Map", "FakeReal");
    let hits = |text: &str| {
        let dir = std::env::temp_dir().join(format!("guard-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("crates"));
        std::fs::write(dir.join("crates/t.rs"), text).unwrap();
        let found = catalog_offenders(&dir, has);
        let _ = std::fs::remove_dir_all(&dir);
        found
    };
    assert_eq!(
        hits(&name("Map", "FakeReal")),
        ["crates/t.rs:1: Map_FakeReal"]
    );
    assert!(hits(&name("Map", "Other")).is_empty());
    assert!(hits(&name("Map", "A")).is_empty());
}
