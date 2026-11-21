//! The rule files: which there are, what each may hold, and loading them
//! into a [`Book`], checked once.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::{Book, CheckEntry, Ctx, Expr, parse, vocab};
use crate::settings::{self, DungeonKeys, FortressKeys};
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
    needs: String,
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

fn always() -> Expr {
    Expr::Word("always".into())
}

/// The settings the rule files read, by id.
pub fn settings_read() -> BTreeSet<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == ':';
    FILES
        .iter()
        .flat_map(|(_, text)| text.split(move |c: char| !word(c)))
        .filter_map(|w| w.strip_prefix("setting:"))
        .filter(|id| !id.is_empty())
        .map(String::from)
        .collect()
}

impl Book {
    pub fn load() -> Result<Self> {
        Self::from_files(FILES)
    }

    pub(super) fn from_files(files: &[(&str, &str)]) -> Result<Self> {
        let mut book = Book {
            abilities: BTreeMap::new(),
            checks: BTreeMap::new(),
            exits: Vec::new(),
            alternatives: Vec::new(),
            insides: Vec::new(),
            sources: Vec::new(),
            toggles: settings::toggles(),
        };
        for &(name, text) in files {
            book.add(name, text)
                .with_context(|| format!("logic/{name}"))?;
        }
        book.validate()?;
        Ok(book)
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
        let ctx = Ctx {
            progressive: true,
            on: BTreeSet::new(),
            seed: 0,
            keys: DungeonKeys::Vanilla,
            fortress: FortressKeys::Vanilla,
            open: BTreeSet::new(),
            doors: None,
            age: crate::world::Era::Child,
        };
        for (word, expr) in &self.abilities {
            self.need(expr, &ctx)
                .with_context(|| format!("ability `{word}`"))?;
        }
        for (hash, entry) in &self.checks {
            self.need(&entry.needs, &ctx)
                .with_context(|| format!("check {hash}"))?;
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
    fn the_settings_rules_read_are_toggles() {
        let read = settings_read();
        assert!(read.contains("open-forest") && read.contains("kakariko-gate"));
        assert!(read.is_subset(&settings::toggles()), "{read:?}");
    }

    #[test]
    fn every_file_in_logic_is_built_in() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/logic");
        let on_disk: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        let mut built: BTreeSet<String> = FILES.iter().map(|f| f.0.to_owned()).collect();
        built.insert(crate::key_rooms::FILE.into());
        assert_eq!(on_disk, built);
    }
}
