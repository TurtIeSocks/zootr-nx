//! Room files: `logic/baseline/<dungeon>/room-NN-<name>.toml`, written by
//! `zootr logic derive`, and `logic/confirmed/<dungeon>/room-NN-<name>.toml`,
//! written from playtests. Only NN keys a file: the name is for people. A confirmed entry replaces the baseline entry
//! with its key, and one whose key the baseline lacks is an error (room
//! reach spec). One without evidence is a guess.
//!
//! A confirmed entry that names a spot on the floor (`from`, `to` or
//! `at`) isn't laid over anything here: derive reads it, checks it
//! against the level and writes what it makes into the baseline (puzzle
//! links spec).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::files::always;
use super::{Expr, parse, vocab};
use crate::entrances::DUNGEONS;

mod spots;
pub mod write;
pub use spots::{FloorClaim, Point, SpotLink, Spots};

/// Every room file under `logic/`, by its path there. `build.rs` lists them.
pub(crate) const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/room_files.rs"));

#[derive(Debug, Deserialize)]
#[serde(try_from = "spots::RawFile")]
pub struct RoomFile {
    /// The game's own `DungeonRoom` number.
    pub room: u32,
    pub name: String,
    #[serde(default)]
    pub link: Vec<Link>,
    #[serde(default)]
    pub check: Vec<Check>,
    #[serde(default)]
    pub flag: Vec<Flag>,
    #[serde(default)]
    pub knowledge: Vec<Knowledge>,
    /// The exits placed in the room.
    #[serde(default)]
    pub exit: Vec<Spot>,
    /// The spots Link lands on in the room.
    pub arrival: Vec<Spot>,
    /// Links between two spots: a puzzle, a floor that gives way.
    pub spot_link: Vec<SpotLink>,
    /// Floor no room had, claimed for this one.
    pub floor: Vec<FloorClaim>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    Walk,
    Door,
    Drop,
    Step,
    Climb,
    Crawl,
    Jump,
    Hover,
    Hookshot,
    Longshot,
    Swim,
    Dive,
    Sink,
    /// One way: the floor gives way under Link.
    Fall,
    /// A scene moves Link.
    Event,
}

impl Via {
    /// As a room file writes it.
    pub fn word(self) -> &'static str {
        match self {
            Via::Walk => "walk",
            Via::Door => "door",
            Via::Drop => "drop",
            Via::Step => "step",
            Via::Climb => "climb",
            Via::Crawl => "crawl",
            Via::Jump => "jump",
            Via::Hover => "hover",
            Via::Hookshot => "hookshot",
            Via::Longshot => "longshot",
            Via::Swim => "swim",
            Via::Dive => "dive",
            Via::Sink => "sink",
            Via::Fall => "fall",
            Via::Event => "event",
        }
    }
}

/// This room to another.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub to: u32,
    pub needs: String,
    pub via: Via,
    pub why: String,
    /// Nothing measured settles it yet.
    #[serde(default)]
    pub guess: bool,
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
    pub why: String,
    /// The superset of the tools there, since nothing settled it.
    #[serde(default)]
    pub guess: bool,
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Why a quest step gate the rule names no word for is always met
    /// by the time Link reaches it (`zootr logic quest-gates`).
    #[serde(rename = "quest-gate-ok")]
    pub quest_gate_ok: Option<String>,
    /// Laid over the baseline from a confirmed file.
    #[serde(skip)]
    pub confirmed: bool,
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
    pub why: String,
    #[serde(default)]
    pub guess: bool,
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Where it's set, for one derive can't trace (an event): derive
    /// checks the spot and writes the flag into the baseline.
    pub at: Option<Point>,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

impl Flag {
    /// How rules name it: `flag:#1a2b3c4d`.
    pub fn word(&self) -> String {
        format!("#{:08x}", self.hash)
    }
}

/// A way past the margin, for a later logic tier: a route to a room
/// (`to` and `via`), or another way to get a check (`check`). Logic
/// ignores it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub to: Option<u32>,
    pub via: Option<Via>,
    pub check: Option<u64>,
    pub why: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

impl Knowledge {
    /// What it leads to, as the audit says it.
    pub fn what(&self) -> String {
        match (self.to, self.via, self.check) {
            (Some(to), Some(via), None) => format!("to {to} ({})", via.word()),
            (None, None, Some(check)) => format!("check {check}"),
            _ => "a route (to, via) or a check, not both".into(),
        }
    }

    fn shaped(&self) -> Result<()> {
        let route = self.to.is_some() && self.via.is_some() && self.check.is_none();
        let check = self.to.is_none() && self.via.is_none() && self.check.is_some();
        ensure!(route || check, "knowledge needs `to` and `via`, or `check`");
        Ok(())
    }
}

/// An exit or a spot Link lands on, keyed by [`spot`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spot {
    pub hash: u32,
    #[expect(dead_code, reason = "for people: logic keys a spot by its hash")]
    pub name: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// The key of a spot in a map, an exit's by where it leads: the same for
/// an exit and the spot it lands on.
pub fn spot(map: &str, spot: &str) -> u32 {
    ounce_formats::murmur3(format!("{map}/{spot}").as_bytes())
}

/// An entry a confirmed file can replace, by its key.
pub(super) trait Entry {
    fn key(&self) -> String;
    fn evidence(&self) -> &[String];
    /// A confirmed entry with no evidence: a guess where it can be one.
    fn unproven(&mut self) -> Result<()> {
        bail!("confirmed {} has no evidence", self.key())
    }
}

impl Entry for Link {
    fn unproven(&mut self) -> Result<()> {
        self.guess = true;
        Ok(())
    }
    fn key(&self) -> String {
        format!("link to {} by {:?}", self.to, self.via)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Check {
    fn unproven(&mut self) -> Result<()> {
        self.guess = true;
        Ok(())
    }
    fn key(&self) -> String {
        format!("check {}", self.hash)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Flag {
    fn unproven(&mut self) -> Result<()> {
        self.guess = true;
        Ok(())
    }
    fn key(&self) -> String {
        format!("flag {}", self.word())
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Spot {
    fn key(&self) -> String {
        format!("spot #{:08x}", self.hash)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl Entry for Knowledge {
    fn key(&self) -> String {
        format!("knowledge {}", self.what())
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

/// The dungeons whose room files drive logic, by folder. A dungeon joins
/// once room places are in the solver and its pilot has settled its audit
/// (room reach spec, build steps 4 and 6). Until then its room files are
/// derived, loaded and checked, and logic keeps its hand rules. Their
/// freshness tests are ignored until then (each derives a whole dungeon,
/// minutes of CI for files no seed reads): a dungeon that joins takes the
/// ignore off its own.
pub const LIVE: &[&str] = &[];

/// One room, its confirmed entries laid over its baseline.
#[derive(Debug)]
pub struct Room {
    /// The dungeon's node in the world graph.
    pub node: String,
    pub file: RoomFile,
    /// Whether its dungeon is in [`LIVE`].
    pub live: bool,
    /// Its confirmed entries that name a spot, for derive.
    pub spots: Spots,
}

/// A dungeon's folder name: its map's name in kebab case.
pub fn folder(node: &str) -> String {
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

/// `tier/dungeon/room-NN-<name>.toml` (or `room-NN.toml`) as its tier,
/// the dungeon's node and NN.
fn path(path: &str) -> Result<(&str, String, u32)> {
    let parts: Vec<&str> = path.split('/').collect();
    let [tier @ ("baseline" | "confirmed"), dungeon, file] = parts[..] else {
        bail!("not tier/dungeon/room-NN-<name>.toml");
    };
    let node = (DUNGEONS.iter())
        .map(|d| d.node.get())
        .find(|n| folder(n) == dungeon)
        .with_context(|| format!("no dungeon `{dungeon}`"))?;
    let number = (file.strip_prefix("room-"))
        .and_then(|f| f.strip_suffix(".toml"))
        .and_then(|f| f.split('-').next())
        .and_then(|n| n.parse().ok())
        .context("not room-NN-<name>.toml")?;
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
    for mut c in confirmed {
        if c.evidence().is_empty() {
            c.unproven()?;
        }
        let slot = (base.iter_mut().find(|b| b.key() == c.key()))
            .with_context(|| format!("confirmed {} has no baseline entry", c.key()))?;
        *slot = c;
    }
    Ok(())
}

/// Knowledge only comes from playtests, so a confirmed file adds it.
fn add(base: &mut Vec<Knowledge>, confirmed: Vec<Knowledge>) -> Result<()> {
    for k in confirmed {
        ensure!(
            !k.evidence.is_empty(),
            "confirmed {} has no evidence",
            k.key()
        );
        ensure!(
            base.iter().all(|b| b.key() != k.key()),
            "two entries for {}",
            k.key()
        );
        base.push(k);
    }
    Ok(())
}

pub(super) fn rule(needs: &str) -> Result<Expr> {
    let mut expr = parse(needs)?;
    vocab::words(&mut expr)?;
    Ok(expr)
}

/// The flags `expr` names, by hash.
fn flags_named(expr: &Expr, out: &mut Vec<u32>) {
    match expr {
        Expr::All(exprs) | Expr::Any(exprs) => exprs.iter().for_each(|e| flags_named(e, out)),
        Expr::Word(w) => out.extend(w.strip_prefix("flag:").and_then(super::lang::flag_hash)),
    }
}

/// Every room, confirmed over baseline, each rule parsed.
pub(super) fn load(files: &[(&str, &str)]) -> Result<Vec<Room>> {
    let mut rooms: BTreeMap<(String, u32), (RoomFile, Spots)> = BTreeMap::new();
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
            unique(&file.spot_link)?;
            unique(&file.check)?;
            unique(&file.flag)?;
            unique(&file.knowledge)?;
            for k in &file.knowledge {
                k.shaped().with_context(|| k.key())?;
            }
            for l in &mut file.link {
                l.expr = rule(&l.needs).with_context(|| l.key())?;
            }
            for l in &mut file.spot_link {
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
                let spotted = !file.spot_link.is_empty()
                    || !file.floor.is_empty()
                    || file.flag.iter().any(|f| f.at.is_some());
                ensure!(
                    !spotted,
                    "{}: a spot entry belongs in confirmed/, derive writes what it makes",
                    at()
                );
                let twice = rooms
                    .insert((node, number), (file, Spots::default()))
                    .is_some();
                ensure!(!twice, "{}: a second file for room {number}", at());
            }
            _ => confirmed.push((name, node, file)),
        }
    }
    // Each flag a confirmed entry names, to check something sets it.
    let mut named: Vec<(String, String, u32)> = vec![];
    for (name, node, mut file) in confirmed {
        let at = || format!("logic/{name}");
        let (base, spots) = (rooms.get_mut(&(node.clone(), file.room)))
            .with_context(|| format!("{}: no baseline room", at()))?;
        let mut name_flags = |key: String, expr: &Expr| {
            let mut out = vec![];
            flags_named(expr, &mut out);
            named.extend(
                out.into_iter()
                    .map(|h| (node.clone(), format!("{}: {key}", at()), h)),
            );
        };
        for l in &file.link {
            name_flags(l.key(), &l.expr);
        }
        for l in &file.spot_link {
            name_flags(l.key(), &l.expr);
        }
        for c in &file.check {
            name_flags(c.key(), &c.expr);
        }
        for f in &file.flag {
            name_flags(f.key(), &f.expr);
        }
        let (placed, flags): (Vec<Flag>, Vec<Flag>) =
            file.flag.into_iter().partition(|f| f.at.is_some());
        file.flag = flags;
        let lay = || -> Result<()> {
            for mut l in std::mem::take(&mut file.spot_link) {
                if l.evidence.is_empty() {
                    l.unproven()?;
                }
                spots.link.push(l);
            }
            for mut f in placed {
                if f.evidence.is_empty() {
                    f.unproven()?;
                }
                spots.flag.push(f);
            }
            spots.floor.append(&mut file.floor);
            overlay(&mut base.link, file.link)?;
            file.check.iter_mut().for_each(|c| c.confirmed = true);
            overlay(&mut base.check, file.check)?;
            overlay(&mut base.flag, file.flag)?;
            add(&mut base.knowledge, file.knowledge)?;
            overlay(&mut base.exit, file.exit)?;
            overlay(&mut base.arrival, file.arrival)
        };
        lay().with_context(at)?;
    }
    for (node, what, hash) in named {
        let set = (rooms.range((node.clone(), 0)..=(node, u32::MAX)))
            .flat_map(|(_, (file, spots))| file.flag.iter().chain(&spots.flag))
            .any(|f| f.hash == hash);
        ensure!(
            set,
            "{what} needs flag #{hash:08x}, which no [[flag]] in its dungeon sets"
        );
    }
    let rooms: Vec<Room> = (rooms.into_iter())
        .map(|((node, _), (file, spots))| Room {
            live: LIVE.contains(&folder(&node).as_str()),
            node,
            file,
            spots,
        })
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

    /// The maintainer's call (puzzle links spec): an entry no playtest
    /// backs is a guess until one does.
    #[test]
    fn a_confirmed_entry_without_evidence_is_a_guess() {
        let bare = BASE.replace("guess = true", "");
        let (base, conf) = (at("baseline"), at("confirmed"));
        let rooms = load(&[(&base, BASE), (&conf, &bare)]).unwrap();
        let file = &rooms[0].file;
        assert!(file.link[0].guess && file.check[0].guess && file.flag[0].guess);
        // An exit or arrival has nothing to guess: it still needs evidence.
        let spot = format!("{BASE}\n[[exit]]\nhash = 7\nname = \"Fake Exit\"\n");
        let err = load(&[(&base, &spot), (&conf, &spot)]).unwrap_err();
        assert!(format!("{err:#}").contains("no evidence"), "{err:#}");
    }

    const SPOTS: &str = r#"
        room = 4
        name = "Room04"
        [[link]]
        from = "-33.5,109.8,10.7"
        to = "-45,104.9,11"
        needs = "always"
        via = "fall"
        why = "the plank breaks"
        evidence = ["fake-playtest"]
        [[link]]
        from = "-45,104.9,11"
        to = "-31.5,109.8,10.2"
        needs = "flag:#1a2b3c4d"
        via = "climb"
        why = "the ladder falls"
        [[flag]]
        hash = 0x5e5e5e5e
        name = "Fake Room Cleared"
        at = "1,2,3"
        needs = "always"
        why = "a fake fight"
        [[floor]]
        at = "4,5,6"
        why = "fake pit"
    "#;

    #[test]
    fn spot_entries_go_to_derive_not_over_the_baseline() {
        let (base, conf) = (at("baseline"), at("confirmed"));
        let rooms = load(&[(&base, BASE), (&conf, SPOTS)]).unwrap();
        let room = &rooms[0];
        assert_eq!((room.file.link.len(), room.file.flag.len()), (1, 1));
        let [fall, ladder] = &room.spots.link[..] else {
            panic!("{:?}", room.spots)
        };
        assert_eq!((fall.via, fall.guess), (Via::Fall, false));
        assert_eq!(fall.from, Point([-33.5, 109.8, 10.7]));
        assert!(ladder.guess, "no evidence");
        assert_eq!(room.spots.flag[0].at, Some(Point([1.0, 2.0, 3.0])));
        assert!(room.spots.flag[0].guess);
        assert_eq!(room.spots.floor[0].at, Point([4.0, 5.0, 6.0]));
        let err = load(&[(&base, SPOTS)]).unwrap_err();
        assert!(
            format!("{err:#}").contains("belongs in confirmed"),
            "{err:#}"
        );
    }

    #[test]
    fn a_spot_link_names_two_spots() {
        let (base, conf) = (at("baseline"), at("confirmed"));
        let bad = |from: &str, to: &str| {
            let text = SPOTS
                .replacen("from = \"-33.5,109.8,10.7\"", from, 1)
                .replacen("to = \"-45,104.9,11\"", to, 1);
            format!("{:#}", load(&[(&base, BASE), (&conf, &text)]).unwrap_err())
        };
        assert!(bad("", "to = \"1,2,3\"").contains("no `from`"));
        assert!(bad("from = \"1,2,3\"", "to = 0").contains("names a spot `to`"));
        assert!(bad("from = \"1,2\"", "to = \"1,2,3\"").contains("isn't x,y,z"));
    }

    #[test]
    fn a_confirmed_entry_names_only_flags_its_dungeon_sets() {
        let (base, conf) = (at("baseline"), at("confirmed"));
        let unset = SPOTS.replace("flag:#1a2b3c4d", "flag:#0badf1a6");
        let err = load(&[(&base, BASE), (&conf, &unset)]).unwrap_err();
        assert!(
            format!("{err:#}").contains("no [[flag]] in its dungeon sets"),
            "{err:#}"
        );
        // A flag a confirmed entry places counts as set.
        let placed = SPOTS.replace("flag:#1a2b3c4d", "flag:#5e5e5e5e");
        load(&[(&base, BASE), (&conf, &placed)]).unwrap();
    }

    #[test]
    fn confirmed_knowledge_adds_a_route_or_a_check_with_evidence() {
        let known = r#"
            room = 4
            name = "Room04"
            [[knowledge]]
            check = 1
            why = "fake bombchus up the wall"
            evidence = ["fake-playtest"]
            [[knowledge]]
            to = 0
            via = "jump"
            why = "fake long jump"
            evidence = ["fake-trace"]
        "#;
        let (base, conf) = (at("baseline"), at("confirmed"));
        let rooms = load(&[(&base, BASE), (&conf, known)]).unwrap();
        let what: Vec<String> = rooms[0]
            .file
            .knowledge
            .iter()
            .map(Knowledge::what)
            .collect();
        assert_eq!(what, ["check 1", "to 0 (jump)"]);
        let bare = known.replace("evidence = [\"fake-playtest\"]", "");
        let err = load(&[(&base, BASE), (&conf, &bare)]).unwrap_err();
        assert!(format!("{err:#}").contains("no evidence"), "{err:#}");
        let both = known.replace("check = 1", "check = 1\nto = 0");
        let err = load(&[(&base, BASE), (&conf, &both)]).unwrap_err();
        assert!(format!("{err:#}").contains("or `check`"), "{err:#}");
    }

    #[test]
    fn a_file_must_name_its_own_room_and_a_real_dungeon() {
        let wrong = format!("baseline/{}/room-05.toml", deku());
        assert!(load(&[(&wrong, BASE)]).is_err());
        assert!(load(&[("baseline/fake-cave/room-04.toml", BASE)]).is_err());
    }

    /// Renaming a room keeps its confirmed entries: only NN keys a file.
    #[test]
    fn a_room_name_in_the_file_name_is_only_for_people() {
        let named = format!("baseline/{}/room-04-fake-basement.toml", deku());
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
        let rooms = load(&[(&named, BASE), (&at("confirmed"), confirmed)]).unwrap();
        assert_eq!(rooms[0].file.room, 4);
        let twice = load(&[(&named, BASE), (&at("baseline"), BASE)]).unwrap_err();
        assert!(format!("{twice:#}").contains("second file"), "{twice:#}");
    }

    #[test]
    fn a_room_check_waits_for_its_dungeon_to_go_live() {
        needs_names!();
        let book =
            super::super::Book::from_parts(super::super::files::FILES, &[(&at("baseline"), BASE)])
                .unwrap();
        assert!(!book.rooms[0].live, "no dungeon is live yet");
        let hand = super::super::Book::from_files(super::super::files::FILES).unwrap();
        assert_eq!(
            book.checks().map(|(h, _)| h).collect::<Vec<_>>(),
            hand.checks().map(|(h, _)| h).collect::<Vec<_>>(),
        );
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
