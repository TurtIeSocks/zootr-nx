//! The rule files: which there are, what each may hold, and loading them
//! into a [`Book`], checked once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::{Book, CheckEntry, Ctx, Expr, kinds, parse, rooms, vocab};
use crate::settings;
use crate::world::Era;

pub(super) const ABILITIES: &str = "abilities.toml";

/// Every rule file, by name. A test keeps this in step with `logic/`.
pub(super) const FILES: &[(&str, &str)] = &[
    (ABILITIES, include_str!("../../logic/abilities.toml")),
    ("exits.toml", include_str!("../../logic/exits.toml")),
    ("gifts.toml", include_str!("../../logic/gifts.toml")),
    (
        "bottom-of-the-well.toml",
        include_str!("../../logic/bottom-of-the-well.toml"),
    ),
    ("deku-tree.toml", include_str!("../../logic/deku-tree.toml")),
    (
        "kokiri-forest.toml",
        include_str!("../../logic/kokiri-forest.toml"),
    ),
    (
        "hyrule-field.toml",
        include_str!("../../logic/hyrule-field.toml"),
    ),
    (
        "lost-woods.toml",
        include_str!("../../logic/lost-woods.toml"),
    ),
    ("vanilla.toml", include_str!("../../logic/vanilla.toml")),
    ("bosses.toml", include_str!("../../logic/bosses.toml")),
    ("cows.toml", include_str!("../../logic/cows.toml")),
    ("beans.toml", include_str!("../../logic/beans.toml")),
    (
        "goron-city.toml",
        include_str!("../../logic/goron-city.toml"),
    ),
    ("graveyard.toml", include_str!("../../logic/graveyard.toml")),
    (
        "water-temple.toml",
        include_str!("../../logic/water-temple.toml"),
    ),
    (
        "spirit-temple.toml",
        include_str!("../../logic/spirit-temple.toml"),
    ),
    (
        "ganons-castle.toml",
        include_str!("../../logic/ganons-castle.toml"),
    ),
    ("grottos.toml", include_str!("../../logic/grottos.toml")),
    ("forest-temple.toml", include_str!("../../logic/forest-temple.toml")),
    ("shadow-temple.toml", include_str!("../../logic/shadow-temple.toml")),
    ("fire-temple.toml", include_str!("../../logic/fire-temple.toml")),
    ("ice-cavern.toml", include_str!("../../logic/ice-cavern.toml")),
    ("gerudo-training-ground.toml", include_str!("../../logic/gerudo-training-ground.toml")),
    ("jabu-jabu.toml", include_str!("../../logic/jabu-jabu.toml")),
    ("dodongos-cavern.toml", include_str!("../../logic/dodongos-cavern.toml")),
];

/// Any rule file but `abilities.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    #[serde(default)]
    check: Vec<CheckRule>,
    #[serde(default)]
    exit: Vec<ExitRule>,
    #[serde(default)]
    source: Vec<SourceRule>,
    #[serde(default)]
    alternative: Vec<PlaceRule>,
    #[serde(default)]
    inside: Vec<PlaceRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckRule {
    hash: u64,
    /// Shown in place of the generated name (`Spot::name`).
    name: String,
    needs: String,
    /// Where a check with no placement is, like a gift, or one whose
    /// placement isn't where Link gets it.
    place: Option<String>,
    era: Option<Era>,
    /// The setting that deals the check, so it may not be dealt.
    setting: Option<String>,
    /// Why a quest step gate the rule names no word for is always met
    /// by the time Link reaches it (`zootr logic quest-gates`).
    #[serde(rename = "quest-gate-ok")]
    quest_gate_ok: Option<String>,
}

/// What an exit between two maps needs. One the game's data doesn't have,
/// like a boss door, says it's `added`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitRule {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub added: bool,
    pub needs: String,
    /// Why a quest step gate the rule names no word for is always met
    /// by the time Link reaches it (`zootr logic quest-gates`).
    #[serde(rename = "quest-gate-ok")]
    pub quest_gate_ok: Option<String>,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

/// A rule about one place: what makes the game load its `_Alternative`
/// copy (the game's own gate is a flag logic can't evaluate), or what a
/// dungeon needs inside.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceRule {
    pub place: String,
    pub needs: String,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

/// Where Link gets an item that isn't shuffled yet.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRule {
    pub name: String,
    pub place: String,
    pub era: Era,
    pub item: String,
    needs: String,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

pub(super) fn always() -> Expr {
    Expr::Word("always".into())
}

/// The settings the rule files read, by id: `setting:bombchus=free` reads
/// `bombchus`.
pub fn settings_read() -> BTreeSet<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '=');
    FILES
        .iter()
        .flat_map(|(_, text)| text.split(move |c: char| !word(c)))
        .filter_map(|w| w.strip_prefix("setting:"))
        .map(|w| w.split_once('=').map_or(w, |(id, _)| id))
        .filter(|id| !id.is_empty())
        .map(String::from)
        .collect()
}

impl Book {
    pub fn load() -> Result<Self> {
        Self::from_parts(FILES, rooms::FILES)
    }

    /// The book from the rule and room files under `dir` as they are on
    /// disk now (a worktree's edits), not the ones built in.
    pub fn load_from(dir: &Path) -> Result<Self> {
        let rules = super::set::rule_files(dir)?;
        let rooms = rooms::write::room_files(dir)?;
        Self::from_parts(&pairs(&rules), &pairs(&rooms))
    }

    pub(super) fn from_files(files: &[(&str, &str)]) -> Result<Self> {
        Self::from_parts(files, &[])
    }

    /// The book from rule files and room files. A live room file's check
    /// takes the place of a hand rule for it, which is kept for the audit.
    pub(super) fn from_parts(files: &[(&str, &str)], room_files: &[(&str, &str)]) -> Result<Self> {
        let mut book = Book {
            abilities: BTreeMap::new(),
            checks: BTreeMap::new(),
            exits: Vec::new(),
            alternatives: Vec::new(),
            insides: Vec::new(),
            sources: Vec::new(),
            toggles: settings::toggles(),
            rooms: Vec::new(),
            kinds: Vec::new(),
            hand: BTreeMap::new(),
        };
        for &(name, text) in files {
            book.add(name, text)
                .with_context(|| format!("logic/{name}"))?;
        }
        book.rooms = rooms::load(room_files)?;
        book.kinds = kinds::load(room_files)?;
        book.take_room_checks();
        book.validate()?;
        Ok(book)
    }

    /// Each live room's check in place of the hand rule for it, which is
    /// kept for the audit.
    fn take_room_checks(&mut self) {
        let live = self.rooms.iter().filter(|r| r.live);
        for check in live.flat_map(|r| &r.file.check) {
            let entry = CheckEntry {
                name: check.name.clone(),
                needs: check.expr.clone(),
                place: None,
                // A setting may leave it undealt, like a gold skulltula. Its
                // hash is a placement's: the fresh derive test keeps it so.
                optional: true,
                room: true,
                quest_gate_ok: check.quest_gate_ok.clone(),
            };
            if let Some(hand) = self.checks.insert(check.hash, entry) {
                self.hand.entry(check.hash).or_insert(hand);
            }
        }
    }

    /// The book with the dungeon in `folder` live, as if it were in
    /// [`rooms::LIVE`].
    #[cfg(test)]
    pub(crate) fn with_live(mut self, folder: &str) -> Self {
        for room in self
            .rooms
            .iter_mut()
            .filter(|r| rooms::folder(&r.node) == folder)
        {
            room.live = true;
        }
        self.take_room_checks();
        self
    }

    fn add(&mut self, name: &str, text: &str) -> Result<()> {
        if name == ABILITIES {
            let table: BTreeMap<String, String> = toml::from_str(text)?;
            for (word, rule) in table {
                let mut expr = parse(&rule)?;
                vocab::words(&mut expr)?;
                self.abilities.insert(word, expr);
            }
            return Ok(());
        }
        let file: RuleFile = toml::from_str(text)?;
        for rule in file.check {
            let place = match (rule.place, rule.era) {
                (Some(place), Some(era)) => Some((vocab::node(&place)?, era)),
                (None, None) => None,
                _ => bail!(
                    "check {} needs both a place and an era, or neither",
                    rule.hash
                ),
            };
            let mut needs = parse(&rule.needs)?;
            vocab::words(&mut needs)?;
            let entry = CheckEntry {
                name: rule.name,
                needs,
                place,
                optional: rule.setting.is_some(),
                room: false,
                quest_gate_ok: rule.quest_gate_ok,
            };
            ensure!(
                self.checks.insert(rule.hash, entry).is_none(),
                "check {} has two rules",
                rule.hash
            );
        }
        for mut exit in file.exit {
            exit.from = vocab::node(&exit.from)?;
            exit.to = vocab::node(&exit.to)?;
            exit.expr = parse(&exit.needs)?;
            vocab::words(&mut exit.expr)?;
            self.exits.push(exit);
        }
        for (rules, list, kind) in [
            (file.alternative, &mut self.alternatives, "alternative"),
            (file.inside, &mut self.insides, "inside"),
        ] {
            for mut rule in rules {
                rule.place = vocab::node(&rule.place)?;
                rule.expr = parse(&rule.needs)?;
                vocab::words(&mut rule.expr)?;
                ensure!(
                    !list.iter().any(|a| a.place == rule.place),
                    "{} has two {kind} rules",
                    rule.place
                );
                list.push(rule);
            }
        }
        for mut source in file.source {
            source.place = vocab::node(&source.place)?;
            source.item = vocab::id(&source.item)?;
            source.expr = parse(&source.needs)?;
            vocab::words(&mut source.expr)?;
            self.sources.push(source);
        }
        Ok(())
    }

    /// Resolves every rule once, so an unknown word or a loop fails at load.
    fn validate(&self) -> Result<()> {
        let ctx = Ctx::for_checks();
        for (word, expr) in &self.abilities {
            self.need(expr, &ctx)
                .with_context(|| format!("ability `{word}`"))?;
        }
        for (hash, entry) in &self.checks {
            self.need(&entry.needs, &ctx)
                .with_context(|| format!("check {hash}"))?;
        }
        for kind in &self.kinds {
            self.need(&kind.expr, &ctx)
                .with_context(|| format!("kind {}", kind.name))?;
        }
        for room in &self.rooms {
            let flags = room.file.flag.iter().map(|f| (&f.expr, f.name.as_str()));
            let links = room.file.link.iter().map(|l| (&l.expr, "a link"));
            let checks = room.file.check.iter().map(|c| (&c.expr, c.name.as_str()));
            for (expr, what) in flags.chain(links).chain(checks) {
                self.need(expr, &ctx)
                    .with_context(|| format!("room {} of {}: {what}", room.file.room, room.node))?;
            }
        }
        for exit in &self.exits {
            self.need(&exit.expr, &ctx)
                .with_context(|| format!("exit {} to {}", exit.from, exit.to))?;
        }
        for alt in &self.alternatives {
            self.need(&alt.expr, &ctx)
                .with_context(|| format!("alternative to {}", alt.place))?;
        }
        for inside in &self.insides {
            self.need(&inside.expr, &ctx)
                .with_context(|| format!("inside {}", inside.place))?;
        }
        for source in &self.sources {
            self.need(&source.expr, &ctx)
                .with_context(|| format!("source {}", source.name))?;
        }
        Ok(())
    }
}

/// Borrowed `(name, text)` pairs, as the loaders take them.
pub(super) fn pairs<N: AsRef<str>>(files: &[(N, String)]) -> Vec<(&str, &str)> {
    (files.iter())
        .map(|(n, t)| (n.as_ref(), t.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_fail_loudly() {
        let abilities = (ABILITIES, "a = \"b\"\nb = \"a\"");
        assert!(Book::from_files(&[abilities]).is_err());
        let unknown = (ABILITIES, "a = \"item:hookshot or strength\"");
        assert!(Book::from_files(&[unknown]).is_err());
        let written = (ABILITIES, "a = \"Item_A\"");
        assert!(Book::from_files(&[written]).is_err());
        let twice = "[[check]]\nhash = 1\nname = \"x\"\nneeds = \"child\"\n";
        let ok = (ABILITIES, "");
        assert!(Book::from_files(&[ok, ("A.toml", twice), ("B.toml", twice)]).is_err());
        assert!(Book::from_files(&[ok, ("A.toml", "[[check]]\nhash = 1\n")]).is_err());
        assert!(Book::from_files(&[ok, ("A.toml", twice)]).is_ok());
        let inside = "[[inside]]\nplace = \"deku-tree\"\nneeds = \"child\"\n";
        assert!(Book::from_files(&[ok, ("A.toml", inside)]).is_ok());
        assert!(Book::from_files(&[ok, ("A.toml", inside), ("B.toml", inside)]).is_err());
    }

    #[test]
    fn an_unknown_word_names_itself_and_its_file() {
        let ok = (ABILITIES, "");
        let bad_item = (ABILITIES, "a = \"item:no-such\"");
        let bad_place = "[[exit]]\nfrom = \"nowhere\"\nto = \"deku-tree\"\nneeds = \"child\"\n";
        let bad_source = "[[source]]\nname = \"x\"\nplace = \"deku-tree\"\nera = \"child\"\nitem = \"beat:no-such\"\nneeds = \"always\"\n";
        let cases = [
            (vec![bad_item], "item:no-such", "logic/abilities.toml"),
            (vec![ok, ("a.toml", bad_place)], "nowhere", "logic/a.toml"),
            (
                vec![ok, ("b.toml", bad_source)],
                "beat:no-such",
                "logic/b.toml",
            ),
        ];
        for (files, word, file) in cases {
            let err = Book::from_files(&files).err().unwrap();
            let err = format!("{err:#}");
            assert!(err.contains(word) && err.contains(file), "{err}");
        }
    }

    #[test]
    fn the_settings_rules_read_are_settings() {
        let read = settings_read();
        assert!(read.contains("open-forest") && read.contains("kakariko-gate"));
        assert!(read.contains("bombchus"), "a choice is read by its id");
        let words = settings::toggles();
        let known = |id: &String| {
            words
                .iter()
                .any(|w| w == id || w.starts_with(&format!("{id}=")))
        };
        assert!(read.iter().all(known), "{read:?}");
    }

    #[test]
    fn every_file_in_logic_is_built_in() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/logic");
        let on_disk: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap())
            // The room folders, which build.rs lists whole.
            .filter(|e| !e.file_type().unwrap().is_dir())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        let mut built: BTreeSet<String> = FILES.iter().map(|f| f.0.to_owned()).collect();
        built.insert(crate::key_rooms::FILE.into());
        built.insert(crate::nav::limits::FILE.into());
        built.insert(crate::derive::rooms::NAMES_FILE.into());
        assert_eq!(on_disk, built);
    }
}
