//! `zootr logic set`: one check's entry in the rule files, edited line by
//! line so comments and order stay. `toml_edit` refuses hashes above
//! `i64::MAX`, which most check hashes are.

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::Book;
use super::files::FILES;
use crate::world::Era;

/// What `logic set` changes. `None` leaves a field as it is.
#[derive(Default)]
pub struct Edit {
    pub name: Option<String>,
    pub needs: Option<String>,
    pub place: Option<(String, Era)>,
}

/// Where a check goes when its area has no file of its own.
const FALLBACK: &str = "vanilla.toml";

/// Sets a check's fields in the rule files under `dir`, then loads every
/// file as the game would and writes only if that works. A check with no
/// entry gets one in its area's file; `new` names its area and generated
/// name, read only then. Returns the file written.
pub fn set(
    dir: &Path,
    hash: u64,
    edit: &Edit,
    new: impl FnOnce() -> Result<(String, String)>,
) -> Result<PathBuf> {
    let mut files = rule_files(dir)?;
    let at = match files
        .iter()
        .position(|(_, text)| check(text, hash).is_some())
    {
        Some(at) => {
            files[at].1 = update(&files[at].1, hash, edit);
            at
        }
        None => {
            let (area, name) = new()?;
            let own = area_file(&area);
            let at = (files.iter().position(|(n, _)| *n == own))
                .or_else(|| files.iter().position(|(n, _)| *n == FALLBACK))
                .context("no rule file to add the check to")?;
            files[at].1 = append(&files[at].1, hash, &name, edit);
            at
        }
    };
    Book::from_files(
        &files
            .iter()
            .map(|(n, t)| (*n, t.as_str()))
            .collect::<Vec<_>>(),
    )
    .context("the edited rules don't load, nothing written")?;
    let path = dir.join(files[at].0);
    fs::write(&path, &files[at].1)?;
    Ok(path)
}

/// The rule files under `dir`, as they are on disk.
pub(super) fn rule_files(dir: &Path) -> Result<Vec<(&'static str, String)>> {
    (FILES.iter())
        .map(|(name, _)| {
            let path = dir.join(name);
            let text = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            Ok((*name, text))
        })
        .collect()
}

/// The lines of `hash`'s `[[check]]` entry.
fn check(text: &str, hash: u64) -> Option<Range<usize>> {
    block(text, "[[check]]", &format!("hash = {hash}"))
}

/// The lines of the `table` entry whose line after the table line is
/// `key`, from its table line to the blank, comment or table line after it.
pub(super) fn block(text: &str, table: &str, key: &str) -> Option<Range<usize>> {
    let lines: Vec<&str> = text.lines().collect();
    entries(text, table).find(|r| lines.get(r.start + 1) == Some(&key))
}

/// Each `table` entry's lines, from its table line to the blank, comment
/// or table line after it.
fn entries(text: &str, table: &str) -> std::vec::IntoIter<Range<usize>> {
    let lines: Vec<&str> = text.lines().collect();
    let end = |at: usize| {
        (at + 1..lines.len())
            .find(|&i| {
                let l = lines[i].trim();
                l.is_empty() || l.starts_with('[') || l.starts_with('#')
            })
            .unwrap_or(lines.len())
    };
    (0..lines.len())
        .filter(|&i| lines[i] == table)
        .map(|at| at..end(at))
        .collect::<Vec<_>>()
        .into_iter()
}

/// A way into a place whose need `logic set-exit` or `set-inside` sets.
pub enum Way {
    Exit { from: String, to: String },
    Inside { place: String },
}

impl Way {
    /// Its entry's lines in `text`.
    fn find(&self, text: &str) -> Option<Range<usize>> {
        let lines: Vec<&str> = text.lines().collect();
        let has = |r: &Range<usize>, key: &str, value: &str| {
            lines[r.clone()].contains(&format!("{key} = {}", quote(value)).as_str())
        };
        match self {
            Way::Exit { from, to } => {
                entries(text, "[[exit]]").find(|r| has(r, "from", from) && has(r, "to", to))
            }
            Way::Inside { place } => entries(text, "[[inside]]").find(|r| has(r, "place", place)),
        }
    }
}

/// Sets a hand exit's or inside rule's need in the rule files under
/// `dir`, then writes only if every file still loads. Returns the file.
pub fn set_way(dir: &Path, way: &Way, needs: &str) -> Result<PathBuf> {
    let mut files = rule_files(dir)?;
    let (at, range) = (files.iter().enumerate())
        .find_map(|(i, (_, text))| way.find(text).map(|r| (i, r)))
        .context("no such exit or inside rule")?;
    let text = &files[at].1;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    set_lines(&mut lines, range, vec![("needs", quote(needs))]);
    files[at].1 = lines.join("\n") + if text.ends_with('\n') { "\n" } else { "" };
    Book::from_files(&super::files::pairs(&files))
        .context("the edited rules don't load, nothing written")?;
    let path = dir.join(files[at].0);
    fs::write(&path, &files[at].1)?;
    Ok(path)
}

/// Each `key = value` set in `range`: replaced where its line is, added
/// at the end of the entry where it isn't.
fn set_lines(lines: &mut Vec<String>, mut range: Range<usize>, fields: Vec<(&str, String)>) {
    for (key, value) in fields {
        let line = format!("{key} = {value}");
        let prefix = format!("{key} = ");
        match range.clone().find(|&i| lines[i].starts_with(&prefix)) {
            Some(i) => lines[i] = line,
            None => {
                lines.insert(range.end, line);
                range.end += 1;
            }
        }
    }
}

/// The `key = value` lines an edit writes, values quoted as TOML strings.
fn fields(edit: &Edit) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(name) = &edit.name {
        out.push(("name", quote(name)));
    }
    if let Some(needs) = &edit.needs {
        out.push(("needs", quote(needs)));
    }
    if let Some((place, era)) = &edit.place {
        out.push(("place", quote(place)));
        out.push(("era", quote(era_word(*era))));
    }
    out
}

/// `s` as a TOML string.
pub(super) fn quote(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

fn era_word(era: Era) -> &'static str {
    match era {
        Era::Child => "child",
        Era::Adult => "adult",
    }
}

/// `text` with `hash`'s fields replaced where it has them, added at the
/// end of its entry where it doesn't.
fn update(text: &str, hash: u64, edit: &Edit) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(range) = check(text, hash) else {
        return text.to_owned();
    };
    set_lines(&mut lines, range, fields(edit));
    lines.join("\n") + if text.ends_with('\n') { "\n" } else { "" }
}

/// `text` with a new entry for `hash` at the end. A rule needs a name and
/// a need: `name` and `always` unless the edit gives them.
fn append(text: &str, hash: u64, name: &str, edit: &Edit) -> String {
    let edit = Edit {
        name: Some(edit.name.clone().unwrap_or_else(|| name.to_owned())),
        needs: Some(edit.needs.clone().unwrap_or_else(|| "always".to_owned())),
        place: edit.place.clone(),
    };
    let mut out = format!("{}\n\n[[check]]\nhash = {hash}\n", text.trim_end());
    for (key, value) in fields(&edit) {
        out += &format!("{key} = {value}\n");
    }
    out
}

/// `BottomOfTheWell` is in `bottom-of-the-well.toml`.
fn area_file(area: &str) -> String {
    let mut out = String::new();
    for (i, c) in area.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out + ".toml"
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Fake Hill.\n\n# By the gate.\n[[check]]\nhash = 18000000000000000001\nname = \"Gate chest\"\nneeds = \"always\"\n\n[[check]]\nhash = 7\nname = \"Hill pot\"\nneeds = \"child\"\n";

    #[test]
    fn an_edit_changes_its_lines_and_keeps_the_rest() {
        let edit = Edit {
            needs: Some("adult".into()),
            place: Some(("fake-hill".into(), Era::Adult)),
            ..Default::default()
        };
        let got = update(FILE, 18000000000000000001, &edit);
        assert_eq!(
            got,
            FILE.replace(
                "needs = \"always\"\n",
                "needs = \"adult\"\nplace = \"fake-hill\"\nera = \"adult\"\n"
            )
        );
        let named = Edit {
            name: Some("Hill \"pot\"".into()),
            ..Default::default()
        };
        assert!(update(FILE, 7, &named).ends_with("name = 'Hill \"pot\"'\nneeds = \"child\"\n"));
    }

    #[test]
    fn a_check_with_no_entry_gets_one_at_the_end() {
        let edit = Edit {
            needs: Some("child".into()),
            ..Default::default()
        };
        let got = append(FILE, 9, "Fake Hill Rock", &edit);
        assert!(got.starts_with(FILE));
        assert!(
            got.ends_with(
                "\n\n[[check]]\nhash = 9\nname = \"Fake Hill Rock\"\nneeds = \"child\"\n"
            )
        );
        assert!(check(&got, 9).is_some());
        assert_eq!(check(FILE, 9), None);
    }

    #[test]
    fn a_way_in_finds_its_entry_by_both_ends() {
        let text = "[[exit]]\nfrom = \"a\"\nto = \"b\"\nneeds = \"x\"\n\n[[exit]]\nfrom = \"a\"\nto = \"c\"\n\n[[inside]]\nplace = \"c\"\nneeds = \"adult\"\n";
        let exit = |to: &str| Way::Exit {
            from: "a".into(),
            to: to.into(),
        };
        assert_eq!(exit("b").find(text), Some(0..4));
        assert_eq!(exit("c").find(text), Some(5..8));
        assert_eq!(exit("d").find(text), None);
        let inside = Way::Inside { place: "c".into() };
        assert_eq!(inside.find(text), Some(9..12));
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        set_lines(&mut lines, 5..8, vec![("needs", quote("bow"))]);
        assert_eq!(lines[8], "needs = \"bow\"");
    }

    #[test]
    fn an_area_names_its_file() {
        assert_eq!(area_file("BottomOfTheWell"), "bottom-of-the-well.toml");
        assert_eq!(area_file("KokiriForest"), "kokiri-forest.toml");
    }

    #[test]
    fn a_need_that_doesnt_parse_is_refused_and_nothing_written() {
        needs_names!();
        let dir = std::env::temp_dir().join(format!("zootr-logic-set-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in FILES {
            fs::write(dir.join(name), text).unwrap();
        }
        let (name, text) = (FILES.iter())
            .find(|(_, t)| t.contains("[[check]]\nhash = "))
            .unwrap();
        let hash: u64 = text
            .split("[[check]]\nhash = ")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let bad = Edit {
            needs: Some("child and (".into()),
            ..Default::default()
        };
        let none = || unreachable!("the check has an entry");
        assert!(set(&dir, hash, &bad, none).is_err());
        assert_eq!(fs::read_to_string(dir.join(name)).unwrap(), *text);
        let good = Edit {
            needs: Some("always".into()),
            ..Default::default()
        };
        assert_eq!(set(&dir, hash, &good, none).unwrap(), dir.join(name));
        let written = fs::read_to_string(dir.join(name)).unwrap();
        assert_eq!(written.lines().count(), text.lines().count());
        fs::remove_dir_all(&dir).unwrap();
    }
}
