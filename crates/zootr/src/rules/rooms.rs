//! Room files: `logic/baseline/<dungeon>/room-NN.toml`, written by
//! `zootr logic derive`, and `logic/confirmed/<dungeon>/room-NN.toml`,
//! written from playtests. A confirmed entry replaces the baseline entry
//! with its key, and one whose key the baseline lacks is an error (room
//! reach spec).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::files::always;
use super::{Expr, parse, vocab};
use crate::entrances::DUNGEONS;

/// Every room file under `logic/`, by its path there. `build.rs` lists them.
pub(super) const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/room_files.rs"));

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomFile {
    /// The game's own `DungeonRoom` number.
    pub room: u32,
    #[expect(dead_code, reason = "logic audit reads it (room reach build step 4)")]
    pub name: String,
    #[serde(default)]
    pub link: Vec<Link>,
    #[serde(default)]
    pub check: Vec<Check>,
    #[serde(default)]
    pub flag: Vec<Flag>,
    #[serde(default)]
    pub knowledge: Vec<Knowledge>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    Walk,
    Door,
    Drop,
    Step,
    Climb,
    Jump,
    Hover,
    Hookshot,
    Longshot,
    Swim,
    Dive,
    Sink,
}

/// This room to another.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub to: u32,
    pub needs: String,
    pub via: Via,
    #[expect(dead_code, reason = "logic audit reads it (room reach build step 4)")]
    pub why: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub hash: u64,
    pub name: String,
    pub needs: String,
    #[expect(dead_code, reason = "logic audit reads it (room reach build step 4)")]
    pub why: String,
    /// The superset of the tools there, since nothing settled it.
    #[serde(default)]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "logic audit reads it (room reach build step 4)")
    )]
    pub guess: bool,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

/// A GameData flag Link sets once he reaches the room and meets `needs`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flag {
    pub hash: u32,
    pub name: String,
    pub needs: String,
    #[expect(dead_code, reason = "logic audit reads it (room reach build step 4)")]
    pub why: String,
    #[serde(default)]
    #[expect(dead_code, reason = "logic audit reads it (room reach build step 4)")]
    pub guess: bool,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

impl Flag {
    /// How rules name it: `flag:#1a2b3c4d`.
    pub fn word(&self) -> String {
        format!("#{:08x}", self.hash)
    }
}

/// A route between the margin and the limit, for a later logic tier.
/// Logic ignores it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub to: u32,
    pub via: Via,
    #[expect(dead_code, reason = "logic audit reads it (room reach build step 4)")]
    pub why: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// An entry a confirmed file can replace, by its key.
pub(super) trait Entry {
    fn key(&self) -> String;
    fn evidence(&self) -> &[String];
}

impl Entry for Link {
    fn key(&self) -> String {
        format!("link to {} by {:?}", self.to, self.via)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Check {
    fn key(&self) -> String {
        format!("check {}", self.hash)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Flag {
    fn key(&self) -> String {
        format!("flag {}", self.word())
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Knowledge {
    fn key(&self) -> String {
        format!("knowledge to {} by {:?}", self.to, self.via)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

/// One room, its confirmed entries laid over its baseline.
#[derive(Debug)]
pub struct Room {
    /// The dungeon's node in the world graph.
    pub node: String,
    pub file: RoomFile,
}

/// A dungeon's folder name: its map's name in kebab case.
fn folder(node: &str) -> String {
    let mut out = String::new();
    for c in node.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        match c {
            '_' if !out.ends_with('-') => out.push('-'),
            '_' => {}
            c => out.push(c.to_ascii_lowercase()),
        }
    }
    out
}

/// `tier/dungeon/room-NN.toml` as its tier, the dungeon's node and NN.
fn path(path: &str) -> Result<(&str, String, u32)> {
    let parts: Vec<&str> = path.split('/').collect();
    let [tier @ ("baseline" | "confirmed"), dungeon, file] = parts[..] else {
        bail!("not tier/dungeon/room-NN.toml");
    };
    let node = (DUNGEONS.iter())
        .map(|d| d.node.get())
        .find(|n| folder(n) == dungeon)
        .with_context(|| format!("no dungeon `{dungeon}`"))?;
    let number = (file.strip_prefix("room-"))
        .and_then(|f| f.strip_suffix(".toml"))
        .and_then(|n| n.parse().ok())
        .context("not room-NN.toml")?;
    Ok((tier, node.to_owned(), number))
}

pub(super) fn unique<T: Entry>(list: &[T]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for e in list {
        ensure!(seen.insert(e.key()), "two entries for {}", e.key());
    }
    Ok(())
}

/// Each confirmed entry in place of the baseline entry with its key.
pub(super) fn overlay<T: Entry>(base: &mut [T], confirmed: Vec<T>) -> Result<()> {
    unique(&confirmed)?;
    for c in confirmed {
        ensure!(
            !c.evidence().is_empty(),
            "confirmed {} has no evidence",
            c.key()
        );
        let slot = (base.iter_mut().find(|b| b.key() == c.key()))
            .with_context(|| format!("confirmed {} has no baseline entry", c.key()))?;
        *slot = c;
    }
    Ok(())
}

pub(super) fn rule(needs: &str) -> Result<Expr> {
    let mut expr = parse(needs)?;
    vocab::words(&mut expr)?;
    Ok(expr)
}

/// Every room, confirmed over baseline, each rule parsed.
pub(super) fn load(files: &[(&str, &str)]) -> Result<Vec<Room>> {
    let mut rooms: BTreeMap<(String, u32), RoomFile> = BTreeMap::new();
    let mut confirmed = Vec::new();
    for &(name, text) in files
        .iter()
        .filter(|(n, _)| !n.ends_with(super::kinds::FILE))
    {
        let at = || format!("logic/{name}");
        let (tier, node, number) = path(name).with_context(at)?;
        let mut file: RoomFile = toml::from_str(text).with_context(at)?;
        ensure!(file.room == number, "{}: says room {}", at(), file.room);
        let mut parsed = || -> Result<()> {
            unique(&file.link)?;
            unique(&file.check)?;
            unique(&file.flag)?;
            unique(&file.knowledge)?;
            for l in &mut file.link {
                l.expr = rule(&l.needs).with_context(|| l.key())?;
            }
            for c in &mut file.check {
                c.expr = rule(&c.needs).with_context(|| c.key())?;
            }
            for f in &mut file.flag {
                f.expr = rule(&f.needs).with_context(|| f.key())?;
            }
            Ok(())
        };
        parsed().with_context(at)?;
        match tier {
            "baseline" => {
                rooms.insert((node, number), file);
            }
            _ => confirmed.push((name, node, file)),
        }
    }
    for (name, node, file) in confirmed {
        let at = || format!("logic/{name}");
        let base = (rooms.get_mut(&(node, file.room)))
            .with_context(|| format!("{}: no baseline room", at()))?;
        let lay = || -> Result<()> {
            overlay(&mut base.link, file.link)?;
            overlay(&mut base.check, file.check)?;
            overlay(&mut base.flag, file.flag)?;
            overlay(&mut base.knowledge, file.knowledge)
        };
        lay().with_context(at)?;
    }
    let rooms: Vec<Room> = (rooms.into_iter())
        .map(|((node, _), file)| Room { node, file })
        .collect();
    let mut checks = BTreeSet::new();
    for c in rooms.iter().flat_map(|r| &r.file.check) {
        ensure!(checks.insert(c.hash), "check {} is in two rooms", c.hash);
    }
    Ok(rooms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map;

    const DEKU: &str = "deku";

    fn deku() -> String {
        folder(map::INSIDE_THE_DEKU_TREE.get())
    }

    fn at(tier: &str) -> String {
        format!("{tier}/{}/room-04.toml", deku())
    }

    const BASE: &str = r#"
        room = 4
        name = "Room04"
        [[link]]
        to = 0
        needs = "always"
        via = "climb"
        why = "climb wall"
        [[check]]
        hash = 1
        name = "Fake Chest"
        needs = "slingshot"
        why = "superset"
        guess = true
        [[flag]]
        hash = 0x1a2b3c4d
        name = "Fake Room Unlocked"
        needs = "slingshot"
        why = "eye switch writes it"
    "#;

    #[test]
    fn a_folder_is_the_map_name_in_kebab_case() {
        assert_eq!(folder("FakeTemple"), "fake-temple");
        assert_eq!(folder("Fake_Cave"), "fake-cave");
        needs_names!();
        assert!(deku().contains(DEKU));
    }

    #[test]
    fn a_confirmed_entry_replaces_the_baseline_entry_with_its_key() {
        let confirmed = r#"
            room = 4
            name = "Room04"
            [[check]]
            hash = 1
            name = "Fake Chest"
            needs = "bombs"
            why = "web: Bomb 100"
            evidence = ["fake-trace"]
        "#;
        let (base, conf) = (at("baseline"), at("confirmed"));
        let rooms = load(&[(&base, BASE), (&conf, confirmed)]).unwrap();
        let [room] = &rooms[..] else {
            panic!("{rooms:?}")
        };
        assert_eq!(room.node, map::INSIDE_THE_DEKU_TREE.get());
        let [check] = &room.file.check[..] else {
            panic!()
        };
        assert_eq!((check.needs.as_str(), check.guess), ("bombs", false));
        // What the confirmed file left out stays as the baseline has it.
        assert_eq!(room.file.link.len(), 1);
        assert_eq!(room.file.flag[0].word(), "#1a2b3c4d");
    }

    #[test]
    fn a_confirmed_entry_the_baseline_lacks_is_an_error() {
        let orphan = r#"
            room = 4
            name = "Room04"
            [[check]]
            hash = 2
            name = "Fake Gone"
            needs = "bombs"
            why = "gone"
            evidence = ["fake-trace"]
        "#;
        let (base, conf) = (at("baseline"), at("confirmed"));
        let err = load(&[(&base, BASE), (&conf, orphan)]).unwrap_err();
        assert!(format!("{err:#}").contains("no baseline entry"), "{err:#}");
    }

    #[test]
    fn a_confirmed_entry_needs_evidence() {
        let bare = BASE.replace("guess = true", "");
        let (base, conf) = (at("baseline"), at("confirmed"));
        let err = load(&[(&base, BASE), (&conf, &bare)]).unwrap_err();
        assert!(format!("{err:#}").contains("no evidence"), "{err:#}");
    }

    #[test]
    fn a_file_must_name_its_own_room_and_a_real_dungeon() {
        let wrong = format!("baseline/{}/room-05.toml", deku());
        assert!(load(&[(&wrong, BASE)]).is_err());
        assert!(load(&[("baseline/fake-cave/room-04.toml", BASE)]).is_err());
    }

    #[test]
    fn a_bad_rule_fails_at_load() {
        needs_names!();
        let bad = BASE.replace(
            "needs = \"slingshot\"\n        why = \"superset\"",
            "needs = \"fake-word\"\n        why = \"superset\"",
        );
        // Words resolve in the book, which checks every room rule at load.
        let book = |text: &str| {
            super::super::Book::from_parts(super::super::files::FILES, &[(&at("baseline"), text)])
        };
        assert!(book(BASE).is_ok());
        assert!(book(&bad).is_err());
    }
}
