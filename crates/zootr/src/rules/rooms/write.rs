//! `zootr logic room-set` and `room-add`: confirmed entries written into a
//! room's file line by line, so comments and order stay. Nothing is
//! written unless every rule and room file still loads. Spots are checked
//! against the floor only by derive, so a write loads no geometry.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{RoomFile, folder};
use crate::entrances::Dungeon;
use crate::rules::Book;
use crate::rules::files::pairs;
use crate::rules::set::{block, quote, rule_files};

/// A spot entry `room-add` writes. Spots are `x,y,z`.
pub enum Add {
    Link {
        from: String,
        to: String,
        via: String,
        needs: String,
        why: String,
    },
    Flag {
        hash: u32,
        name: Option<String>,
        at: String,
        needs: String,
        why: String,
    },
    Floor {
        at: String,
        why: String,
    },
}

/// Every room file under `dir`'s `baseline/` and `confirmed/`, by its path
/// there, sorted as the build lists them.
pub fn room_files(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut paths = Vec::new();
    for tier in ["baseline", "confirmed"] {
        collect(&dir.join(tier), &mut paths)?;
    }
    paths.sort();
    (paths.into_iter())
        .map(|p| {
            let rel = p.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
            let text = fs::read_to_string(&p).with_context(|| p.display().to_string())?;
            Ok((rel, text))
        })
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "toml") {
            out.push(path);
        }
    }
    Ok(())
}

/// A dungeon's rooms under `dir`, by number, with their names.
pub fn rooms(dir: &Path, dungeon: &Dungeon) -> Result<Vec<(u32, String)>> {
    let book = Book::load_from(dir)?;
    Ok((book.rooms.iter())
        .filter(|r| r.node == dungeon.node.get())
        .map(|r| (r.file.room, r.file.name.clone()))
        .collect())
}

/// Sets a check's confirmed rule in `room`: its need and why, with
/// `evidence` added to what the entry had. Without any evidence the
/// entry is a guess. Returns the file written.
pub fn set_check(
    dir: &Path,
    dungeon: &Dungeon,
    room: u32,
    hash: u64,
    needs: &str,
    why: &str,
    evidence: Option<&str>,
) -> Result<PathBuf> {
    write(dir, dungeon, room, |base, confirmed, text| {
        let name = (base.check.iter())
            .find(|c| c.hash == hash)
            .map(|c| c.name.clone())
            .with_context(|| format!("room {room} has no check {hash}"))?;
        let old = confirmed.and_then(|f| f.check.iter().find(|c| c.hash == hash));
        let mut lines = vec![
            format!("hash = {hash}"),
            format!("name = {}", quote(&name)),
            format!("needs = {}", quote(needs)),
            format!("why = {}", quote(why)),
        ];
        if let Some(ok) = old.and_then(|c| c.quest_gate_ok.as_deref()) {
            lines.push(format!("quest-gate-ok = {}", quote(ok)));
        }
        let kept = old.map(|c| c.evidence.as_slice()).unwrap_or_default();
        lines.extend(evidence_lines(kept, evidence));
        Ok(put(text, "[[check]]", &lines))
    })
}

/// Adds a spot entry to `room`. A flag replaces the confirmed flag with
/// its hash, keeping that one's evidence; its name defaults to the
/// baseline's. Returns the file written.
pub fn add(
    dir: &Path,
    dungeon: &Dungeon,
    room: u32,
    entry: &Add,
    evidence: Option<&str>,
) -> Result<PathBuf> {
    write(dir, dungeon, room, |base, confirmed, text| {
        let (table, mut lines, kept) = match entry {
            Add::Link {
                from,
                to,
                via,
                needs,
                why,
            } => (
                "[[link]]",
                vec![
                    format!("from = {}", quote(from)),
                    format!("to = {}", quote(to)),
                    format!("via = {}", quote(via)),
                    format!("needs = {}", quote(needs)),
                    format!("why = {}", quote(why)),
                ],
                &[][..],
            ),
            Add::Flag {
                hash,
                name,
                at,
                needs,
                why,
            } => {
                let old = confirmed.and_then(|f| f.flag.iter().find(|f| f.hash == *hash));
                let name = (name.clone())
                    .or_else(|| old.map(|f| f.name.clone()))
                    .or_else(|| {
                        base.flag
                            .iter()
                            .find(|f| f.hash == *hash)
                            .map(|f| f.name.clone())
                    })
                    .with_context(|| format!("flag #{hash:08x} has no name here: give --name"))?;
                (
                    "[[flag]]",
                    vec![
                        format!("hash = 0x{hash:08x}"),
                        format!("name = {}", quote(&name)),
                        format!("at = {}", quote(at)),
                        format!("needs = {}", quote(needs)),
                        format!("why = {}", quote(why)),
                    ],
                    old.map(|f| f.evidence.as_slice()).unwrap_or_default(),
                )
            }
            Add::Floor { at, why } => (
                "[[floor]]",
                vec![
                    format!("at = {}", quote(at)),
                    format!("why = {}", quote(why)),
                ],
                &[][..],
            ),
        };
        lines.extend(evidence_lines(kept, evidence));
        Ok(match entry {
            Add::Flag { .. } => put(text, table, &lines),
            _ => append(text, table, &lines),
        })
    })
}

/// Reads `room`'s baseline and confirmed files under `dir`, has `edit`
/// make the confirmed file's new text, then writes it if every file
/// still loads with it.
fn write(
    dir: &Path,
    dungeon: &Dungeon,
    room: u32,
    edit: impl FnOnce(&RoomFile, Option<&RoomFile>, &str) -> Result<String>,
) -> Result<PathBuf> {
    let mut files = room_files(dir)?;
    let base_folder = format!("baseline/{}/", folder(dungeon.node.get()));
    let (base_path, base_text) = (files.iter())
        .find(|(p, _)| p.starts_with(&base_folder) && number(p) == Some(room))
        .with_context(|| format!("{} has no room {room}", dungeon.label))?;
    let base: RoomFile = toml::from_str(base_text).with_context(|| base_path.clone())?;
    let path = base_path.replacen("baseline/", "confirmed/", 1);
    let at = files.iter().position(|(p, _)| *p == path);
    let text = match at {
        Some(i) => files[i].1.clone(),
        None => format!(
            "# Settled by playtest. Overrides baseline/ by key.\nroom = {room}\nname = {}\n",
            quote(&base.name)
        ),
    };
    let confirmed: Option<RoomFile> = (at.is_some())
        .then(|| toml::from_str(&text))
        .transpose()
        .with_context(|| path.clone())?;
    let new = edit(&base, confirmed.as_ref(), &text)?;
    match at {
        Some(i) => files[i].1 = new.clone(),
        None => files.push((path.clone(), new.clone())),
    }
    Book::from_parts(&pairs(&rule_files(dir)?), &pairs(&files))
        .context("the edited room files don't load, nothing written")?;
    let out = dir.join(&path);
    fs::create_dir_all(out.parent().context("a room file's folder")?)?;
    fs::write(&out, new)?;
    Ok(out)
}

/// `room-NN-<name>.toml`'s NN.
fn number(path: &str) -> Option<u32> {
    let file = path.rsplit('/').next()?;
    file.strip_prefix("room-")?
        .split(['-', '.'])
        .next()?
        .parse()
        .ok()
}

/// `kept` and then `new` as an `evidence` array, or nothing at all (a
/// guess). A blank `new` adds nothing.
fn evidence_lines(kept: &[String], new: Option<&str>) -> Vec<String> {
    let new = new.map(str::trim).filter(|e| !e.is_empty());
    let all: Vec<&str> = (kept.iter().map(String::as_str)).chain(new).collect();
    if all.is_empty() {
        return vec![];
    }
    let mut out = vec!["evidence = [".to_owned()];
    out.extend(all.iter().map(|e| format!("    {},", quote(e))));
    out.push("]".to_owned());
    out
}

/// `text` with the `table` entry keyed by `lines[0]` replaced by `lines`,
/// or added at the end when it has none.
fn put(text: &str, table: &str, lines: &[String]) -> String {
    match block(text, table, &lines[0]) {
        Some(range) => {
            let mut all: Vec<String> = text.lines().map(String::from).collect();
            let block = std::iter::once(table.to_owned()).chain(lines.iter().cloned());
            all.splice(range, block);
            all.join("\n") + "\n"
        }
        None => append(text, table, lines),
    }
}

/// `text` with a new `table` entry at the end.
fn append(text: &str, table: &str, lines: &[String]) -> String {
    format!("{}\n\n{table}\n{}\n", text.trim_end(), lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Settled by playtest. Overrides baseline/ by key.\nroom = 1\nname = \"Fake Hall\"\n\n# A note.\n[[check]]\nhash = 18000000000000000001\nname = \"Gate chest\"\nneeds = \"always\"\nwhy = \"open\"\nevidence = [\n    \"playtest, the maintainer: one\",\n]\n\n[[flag]]\nhash = 0x0000000a\nname = \"Fake flag\"\nneeds = \"always\"\nwhy = \"set\"\n";

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn put_replaces_a_whole_entry_evidence_and_all() {
        let new = lines(&[
            "hash = 18000000000000000001",
            "name = \"Gate chest\"",
            "needs = \"bombs\"",
            "why = \"blast it\"",
        ]);
        let got = put(FILE, "[[check]]", &new);
        assert!(got.contains("# A note.\n[[check]]\nhash = 18000000000000000001\nname = \"Gate chest\"\nneeds = \"bombs\"\nwhy = \"blast it\"\n\n[[flag]]"));
        assert!(!got.contains("evidence"));
        let _: RoomFile = toml::from_str(&got).unwrap();
    }

    #[test]
    fn put_adds_an_entry_it_doesnt_find() {
        let new = lines(&["hash = 0x0000000b", "name = \"Other\""]);
        let got = put(FILE, "[[flag]]", &new);
        assert!(got.starts_with(FILE.trim_end()));
        assert!(got.ends_with("\n\n[[flag]]\nhash = 0x0000000b\nname = \"Other\"\n"));
    }

    #[test]
    fn evidence_keeps_the_old_and_adds_the_new() {
        let kept = vec!["one".to_owned()];
        assert_eq!(
            evidence_lines(&kept, Some(" two ")),
            lines(&["evidence = [", "    \"one\",", "    \"two\",", "]"])
        );
        assert!(evidence_lines(&[], Some("  ")).is_empty());
        assert!(evidence_lines(&[], None).is_empty());
    }

    #[test]
    fn a_room_file_names_its_number() {
        assert_eq!(number("baseline/fake-hall/room-03-slingshot.toml"), Some(3));
        assert_eq!(number("baseline/fake-hall/room-12.toml"), Some(12));
        assert_eq!(number("baseline/fake-hall/kinds.toml"), None);
    }

    /// The Deku Tree's logic folder copied to a scratch one.
    fn scratch(tag: &str) -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("logic");
        let dir =
            std::env::temp_dir().join(format!("zootr-room-write-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (name, text) in rule_files(&src).unwrap() {
            let p = dir.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        for (name, text) in room_files(&src).unwrap() {
            let p = dir.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        dir
    }

    fn deku() -> &'static Dungeon {
        (crate::entrances::DUNGEONS.iter())
            .find(|d| d.id == "deku-tree")
            .unwrap()
    }

    #[test]
    fn a_check_is_set_in_a_new_confirmed_file_and_a_bad_need_writes_nothing() {
        needs_names!();
        let dir = scratch("check");
        let book = Book::load_from(&dir).unwrap();
        let room = (book.rooms.iter())
            .filter(|r| r.node == deku().node.get())
            .find(|r| !r.file.check.is_empty() && !dir.join(confirmed_path(r)).exists())
            .unwrap();
        let (number, hash) = (room.file.room, room.file.check[0].hash);
        let path = dir.join(confirmed_path(room));
        assert!(set_check(&dir, deku(), number, hash, "child and (", "x", None).is_err());
        assert!(!path.exists());
        let wrote = set_check(&dir, deku(), number, hash, "always", "open", None).unwrap();
        assert_eq!(wrote, path);
        let book = Book::load_from(&dir).unwrap();
        let check = (book.rooms.iter())
            .find(|r| r.node == deku().node.get() && r.file.room == number)
            .and_then(|r| r.file.check.iter().find(|c| c.hash == hash))
            .unwrap();
        assert_eq!((check.needs.as_str(), check.guess), ("always", true));
        set_check(
            &dir,
            deku(),
            number,
            hash,
            "always",
            "open",
            Some("playtest, the maintainer: seen"),
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("[[check]]").count(), 1, "{text}");
        assert!(text.contains("the maintainer: seen"));
        fs::remove_dir_all(&dir).unwrap();
    }

    fn confirmed_path(room: &super::super::Room) -> String {
        let base = room_files(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("logic")
                .as_path(),
        )
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .find(|p| {
            p.starts_with(&format!("baseline/{}/", folder(&room.node)))
                && number(p) == Some(room.file.room)
        })
        .unwrap();
        base.replacen("baseline/", "confirmed/", 1)
    }

    #[test]
    fn a_floor_claim_is_added_and_a_bad_via_refused() {
        needs_names!();
        let dir = scratch("add");
        let floor = Add::Floor {
            at: "1,2,3".into(),
            why: "a ledge".into(),
        };
        let path = add(&dir, deku(), 0, &floor, None).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.ends_with("[[floor]]\nat = \"1,2,3\"\nwhy = \"a ledge\"\n"),
            "{text}"
        );
        let link = Add::Link {
            from: "1,2,3".into(),
            to: "4,5,6".into(),
            via: "teleport".into(),
            needs: "always".into(),
            why: "x".into(),
        };
        assert!(add(&dir, deku(), 0, &link, None).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        assert!(add(&dir, deku(), 99, &floor, None).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
