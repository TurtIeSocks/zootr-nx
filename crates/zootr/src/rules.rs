//! The rule language: short sentences saying what Link needs, like
//! `bombs or (adult and item:megaton-hammer)`. Rule files live in `logic/`
//! and are built into the binary. They speak in friendly words that
//! [`vocab`] maps to the game's names.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::progressive::LINES;
use crate::settings::{self, DungeonKeys, FortressKeys, NewGame};
use crate::world::Era;

mod lang;
mod set;
pub mod vocab;

pub use set::{Edit, set};

pub use lang::{Expr, Have, Need, parse};

const ABILITIES: &str = "abilities.toml";

/// Every rule file, by name. A test keeps this in step with `logic/`.
const FILES: &[(&str, &str)] = &[
    (ABILITIES, include_str!("../logic/abilities.toml")),
    ("exits.toml", include_str!("../logic/exits.toml")),
    ("gifts.toml", include_str!("../logic/gifts.toml")),
    (
        "bottom-of-the-well.toml",
        include_str!("../logic/bottom-of-the-well.toml"),
    ),
    ("deku-tree.toml", include_str!("../logic/deku-tree.toml")),
    (
        "kokiri-forest.toml",
        include_str!("../logic/kokiri-forest.toml"),
    ),
    (
        "hyrule-field.toml",
        include_str!("../logic/hyrule-field.toml"),
    ),
    ("lost-woods.toml", include_str!("../logic/lost-woods.toml")),
    ("vanilla.toml", include_str!("../logic/vanilla.toml")),
    ("bosses.toml", include_str!("../logic/bosses.toml")),
    ("cows.toml", include_str!("../logic/cows.toml")),
    ("beans.toml", include_str!("../logic/beans.toml")),
    ("graveyard.toml", include_str!("../logic/graveyard.toml")),
    (
        "water-temple.toml",
        include_str!("../logic/water-temple.toml"),
    ),
    (
        "spirit-temple.toml",
        include_str!("../logic/spirit-temple.toml"),
    ),
    (
        "ganons-castle.toml",
        include_str!("../logic/ganons-castle.toml"),
    ),
    ("grottos.toml", include_str!("../logic/grottos.toml")),
];

/// The game's own words naming an item actor start with one of these.
/// Rule files use `item:` words, so only code writes these.
const ITEM_PREFIXES: &[&str] = &["Item_", "TBox_", "Armor_", "Obj_"];
/// A boss defeat's id starts with this. Only a vanilla source gives one,
/// and Link holds it like an item.
const BEAT: &str = "Beat_";

/// What a seed's rules resolve against.
pub struct Ctx {
    pub progressive: bool,
    /// Toggles that are on.
    pub on: BTreeSet<String>,
    /// For what's rolled per seed, like shop prices.
    pub seed: u64,
    /// How dungeon keys are dealt: as copies, logic asks for them.
    pub keys: DungeonKeys,
    /// How the thieves' keys are dealt: as copies, the card's check asks
    /// for them.
    pub fortress: FortressKeys,
    /// Toggles left open, each a [`Need::Setting`] instead of always or
    /// never. Only the Archipelago export opens any.
    pub open: BTreeSet<String>,
}

impl Ctx {
    pub fn new(new_game: &NewGame) -> Self {
        Self {
            progressive: new_game.progressive,
            on: new_game.on.clone(),
            seed: new_game.seed,
            keys: new_game.keys,
            fortress: new_game.fortress_keys,
            open: BTreeSet::new(),
        }
    }
}

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
    alternative: Vec<AlternativeRule>,
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

/// What a check needs beyond reaching its place.
pub struct CheckEntry {
    pub name: String,
    pub needs: Expr,
    /// Set for checks that aren't placed actors.
    pub place: Option<(String, Era)>,
    /// Dealt only with a setting on.
    pub optional: bool,
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

/// What makes the game load a map's `_Alternative` copy in its place. The
/// game's own gate is a flag logic can't evaluate.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeRule {
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

/// Every rule file, parsed and checked.
pub struct Book {
    abilities: BTreeMap<String, Expr>,
    checks: BTreeMap<u64, CheckEntry>,
    pub exits: Vec<ExitRule>,
    pub alternatives: Vec<AlternativeRule>,
    pub sources: Vec<SourceRule>,
    toggles: BTreeSet<String>,
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

    fn from_files(files: &[(&str, &str)]) -> Result<Self> {
        let mut book = Book {
            abilities: BTreeMap::new(),
            checks: BTreeMap::new(),
            exits: Vec::new(),
            alternatives: Vec::new(),
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
        for mut alt in file.alternative {
            alt.place = vocab::node(&alt.place)?;
            alt.expr = parse(&alt.needs)?;
            vocab::words(&mut alt.expr)?;
            ensure!(
                !self.alternatives.iter().any(|a| a.place == alt.place),
                "{} has two alternative rules",
                alt.place
            );
            self.alternatives.push(alt);
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
        for source in &self.sources {
            self.need(&source.expr, &ctx)
                .with_context(|| format!("source {}", source.name))?;
        }
        Ok(())
    }

    /// What loads `map`'s alternative, resolved. Every alternative the
    /// game has needs a rule: its own flag is out of logic's reach.
    pub fn alternative(&self, map: &str, ctx: &Ctx) -> Result<Need> {
        let rule = self
            .alternatives
            .iter()
            .find(|a| a.place == map)
            .with_context(|| format!("{map} has an alternative but no rule for it"))?;
        self.need(&rule.expr, ctx)
    }

    /// A check's name in the rule files, if it has an entry.
    pub fn name(hash: u64) -> Option<&'static str> {
        static NAMES: OnceLock<BTreeMap<u64, String>> = OnceLock::new();
        // The tests load every rule file, so an error here never ships.
        let names = NAMES.get_or_init(|| {
            Self::load()
                .map(|b| b.checks.into_iter().map(|(h, e)| (h, e.name)).collect())
                .unwrap_or_default()
        });
        names.get(&hash).map(String::as_str)
    }

    /// The hand-written rule for a check, if it has one.
    pub fn check(&self, hash: u64) -> Option<&CheckEntry> {
        self.checks.get(&hash)
    }

    pub fn checks(&self) -> impl Iterator<Item = (u64, &CheckEntry)> {
        self.checks.iter().map(|(h, e)| (*h, e))
    }

    /// `text` parsed and resolved, for rules from outside the files.
    pub fn resolve(&self, text: &str, ctx: &Ctx) -> Result<Need> {
        self.need(&parse(text)?, ctx)
    }

    pub fn need(&self, expr: &Expr, ctx: &Ctx) -> Result<Need> {
        self.expand(expr, ctx, &mut vec![])
    }

    fn expand<'a>(&'a self, expr: &'a Expr, ctx: &Ctx, path: &mut Vec<&'a str>) -> Result<Need> {
        let many = |exprs: &'a [Expr], path: &mut Vec<&'a str>| -> Result<Vec<Need>> {
            exprs.iter().map(|e| self.expand(e, ctx, path)).collect()
        };
        Ok(match expr {
            Expr::All(exprs) => Need::All(many(exprs, path)?),
            Expr::Any(exprs) => Need::Any(many(exprs, path)?),
            Expr::Word(word) => {
                if let Some(inner) = self.abilities.get(word) {
                    ensure!(!path.contains(&word.as_str()), "`{word}` uses itself");
                    path.push(word);
                    let need = self.expand(inner, ctx, path)?;
                    path.pop();
                    need
                } else {
                    self.word(word, ctx)?
                }
            }
        })
    }

    fn word(&self, word: &str, ctx: &Ctx) -> Result<Need> {
        if ITEM_PREFIXES.iter().any(|p| word.starts_with(p)) {
            return item(word, ctx.progressive);
        }
        if word.len() > BEAT.len() && word.starts_with(BEAT) {
            return item(word, false);
        }
        Ok(match word {
            "always" => Need::Always,
            "child" => Need::Era(Era::Child),
            "adult" => Need::Era(Era::Adult),
            _ => match word.split_once(':') {
                Some(("setting", id)) => {
                    ensure!(self.toggles.contains(id), "no toggle `{id}`");
                    if ctx.open.contains(id) {
                        Need::Setting(id.into())
                    } else if ctx.on.contains(id) {
                        Need::Always
                    } else {
                        Need::Never
                    }
                }
                Some(("flag", name)) if !name.is_empty() => Need::Flag(name.into()),
                // Something logic can't read yet, or a GameData enum it
                // doesn't track (a race's prize), so nothing proves it.
                Some(("todo" | "enum", what)) if !what.is_empty() => Need::Never,
                _ => bail!("unknown word `{word}`"),
            },
        })
    }
}

/// An item's name or `name*n`. In an upgrade line, `n` counts levels from
/// the name: with progressive items any of the line's items count, without
/// them it names the level itself.
fn item(word: &str, progressive: bool) -> Result<Need> {
    let (actor, n) = match word.split_once('*') {
        Some((actor, n)) => (actor, n.parse::<u32>().ok().filter(|&n| n > 0)),
        None => (word, Some(1)),
    };
    let n = n.with_context(|| format!("bad count in `{word}`"))?;
    let Some((line, at)) = LINES.iter().find_map(|l| {
        l.iter()
            .position(|&(drop, _)| drop == actor)
            .map(|i| (l, i))
    }) else {
        return Ok(Need::Items {
            actors: vec![actor.into()],
            count: n,
        });
    };
    let level = at + n as usize;
    ensure!(level <= line.len(), "`{word}` is past the end of its line");
    let (levels, count) = match progressive {
        true => (&line[..], level),
        // Owning a higher level does everything a lower one does.
        false => (&line[level - 1..], 1),
    };
    Ok(Need::Items {
        actors: levels.iter().map(|&(drop, _)| drop.to_string()).collect(),
        count: count as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item, map};

    /// The id of a boss defeat, by its word.
    fn beat(word: &str) -> String {
        vocab::id(&format!("beat:{word}")).unwrap()
    }

    fn ctx(progressive: bool, on: &[&str]) -> Ctx {
        Ctx {
            progressive,
            on: on.iter().map(|s| s.to_string()).collect(),
            seed: 0,
            keys: DungeonKeys::Vanilla,
            fortress: FortressKeys::Vanilla,
            open: BTreeSet::new(),
        }
    }

    fn have(items: &[&str]) -> Have {
        let mut have = Have::default();
        items.iter().for_each(|i| have.add(i));
        have
    }

    #[test]
    fn line_counts_follow_progressive_items() {
        needs_names!();
        let book = Book::load().unwrap();
        let on = ctx(true, &[]);
        let off = ctx(false, &[]);
        let longshot = |c: &Ctx| book.resolve(&format!("{}*2", item::HOOKSHOT), c).unwrap();

        assert!(!longshot(&on).met(&have(&[item::HOOKSHOT]), Era::Adult));
        assert!(longshot(&on).met(&have(&[item::HOOKSHOT, item::LONGSHOT]), Era::Adult));
        assert!(longshot(&on).met(&have(&[item::LONGSHOT, item::LONGSHOT]), Era::Adult));
        assert_eq!(longshot(&on), book.resolve(item::LONGSHOT, &on).unwrap());

        assert!(!longshot(&off).met(&have(&[item::HOOKSHOT, item::HOOKSHOT]), Era::Adult));
        assert!(longshot(&off).met(&have(&[item::LONGSHOT]), Era::Adult));

        assert!(book.resolve(&format!("{}*3", item::HOOKSHOT), &on).is_err());
        assert!(book.resolve(&format!("{}*0", item::HOOKSHOT), &on).is_err());
    }

    #[test]
    fn abilities_expand_with_era() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let hammer = book.resolve("explosives or hammer", &c).unwrap();
        assert!(hammer.met(&have(&[item::HAMMER]), Era::Adult));
        assert!(!hammer.met(&have(&[item::HAMMER]), Era::Child));
        assert!(hammer.met(&have(&[item::BOMB_BAG_SMALL]), Era::Child));
        assert!(!hammer.met(&have(&[]), Era::Adult));
    }

    #[test]
    fn a_beehive_falls_to_what_reaches_and_hurts_it() {
        needs_names!();
        // Its DamageRates give the slingshot seed 0, and a bomb's blast
        // falls short of the overhang (Mac, 2026-11-15).
        let book = Book::load().unwrap();
        let knock = book.resolve("knock-down", &ctx(true, &[])).unwrap();
        assert!(knock.met(&have(&[item::BOOMERANG]), Era::Child));
        assert!(!knock.met(&have(&[item::RANGE_SLING_SHOT]), Era::Child));
        assert!(!knock.met(&have(&[item::BOMB_BAG_SMALL]), Era::Child));
    }

    #[test]
    fn a_milk_is_a_bottle() {
        needs_names!();
        let book = Book::load().unwrap();
        let bottle = book.resolve("bottle", &ctx(true, &[])).unwrap();
        assert!(bottle.met(&have(&[item::BOTTLE_MILK]), Era::Child));
        assert!(!bottle.met(&have(&[]), Era::Child));
    }

    #[test]
    fn settings_switch_rules() {
        needs_names!();
        let book = Book::load().unwrap();
        let chus: &[&str] = &[item::BOMBCHU_BUNDLE_SMALL];
        let off = book.resolve("bombchus", &ctx(true, &[])).unwrap();
        let on = book
            .resolve("bombchus", &ctx(true, &["free-bombchus"]))
            .unwrap();
        assert!(!off.met(&have(chus), Era::Child));
        assert!(on.met(&have(chus), Era::Child));
        assert!(
            book.resolve("setting:no-such-toggle", &ctx(true, &[]))
                .is_err()
        );
    }

    #[test]
    fn flags_and_unknown_words() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let flag = book.resolve("flag:Fake_Flag", &c).unwrap();
        let mut h = have(&[]);
        assert!(!flag.met(&h, Era::Child));
        h.flags.insert("Fake_Flag".into());
        assert!(flag.met(&h, Era::Child));
        for bad in ["strength", "flag:", "switch:Obj_A"] {
            assert!(book.resolve(bad, &c).is_err(), "{bad}");
        }
    }

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
    }

    #[test]
    fn an_alternative_takes_its_one_rule() {
        let ok = (ABILITIES, "");
        let rule = "[[alternative]]\nplace = \"zora-domain-adult\"\nneeds = \"beat:morpha\"\n";
        let book = Book::from_files(&[ok, ("a.toml", rule)]).unwrap();
        let c = ctx(true, &[]);
        let need = book.alternative(map::ZORA_DOMAIN_ADULT, &c).unwrap();
        assert_eq!(
            need.to_string(),
            book.resolve(&beat("morpha"), &c).unwrap().to_string()
        );
        // The game swapping a map with no rule would hide a check from logic.
        assert!(book.alternative(map::KAKARIKO_VILLAGE_ADULT, &c).is_err());
        assert!(Book::from_files(&[ok, ("a.toml", rule), ("b.toml", rule)]).is_err());
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
    fn equip_any_age_lets_the_other_age_use_the_gear() {
        needs_names!();
        let book = Book::load().unwrap();
        let cases = [
            ("hover-boots", "hover-boots", Era::Adult),
            ("iron-boots", "iron-boots", Era::Adult),
            ("zora-tunic", "zora-tunic", Era::Adult),
            ("goron-tunic", "goron-tunic", Era::Adult),
            ("mirror-shield", "mirror-shield", Era::Adult),
            ("kokiri-sword", "kokiri-sword", Era::Child),
            ("deku-shield", "deku-shield-chest", Era::Child),
        ];
        for (word, item, home) in cases {
            let other = match home {
                Era::Adult => Era::Child,
                Era::Child => Era::Adult,
            };
            let held = have(&[&vocab::id(&format!("item:{item}")).unwrap()]);
            let met =
                |on: &[&str], era| book.resolve(word, &ctx(true, on)).unwrap().met(&held, era);
            assert!(met(&[], home) && !met(&[], other), "{word} off");
            let on = [settings::EQUIP_ANY_AGE];
            assert!(met(&on, home) && met(&on, other), "{word} on");
        }
    }

    #[test]
    fn light_needs_an_adult_mirror_shield_without_equip_any_age() {
        needs_names!();
        let book = Book::load().unwrap();
        let held = have(&[item::SHIELD_MIRROR]);
        let met = |on: &[&str], era| {
            book.resolve("light", &ctx(true, on))
                .unwrap()
                .met(&held, era)
        };
        assert!(met(&[], Era::Adult) && !met(&[], Era::Child));
        assert!(met(&[settings::EQUIP_ANY_AGE], Era::Child));
    }

    #[test]
    fn the_well_drains_with_zeldas_lullaby() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let lullaby: &[&str] = &[item::OCARINA_FAIRY, item::MELODY_ZELDA_LULLABY];
        for hash in [12988877782812797760, 5429815628949128345] {
            let needs = book.need(&book.check(hash).unwrap().needs, &c).unwrap();
            assert!(!needs.met(&have(&[]), Era::Child), "{hash}");
            assert!(needs.met(&have(lullaby), Era::Child), "{hash}");
        }
    }

    #[test]
    fn the_water_temples_sunken_key_wants_boots_tunic_bow_and_longshot() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(false, &[]);
        let needs = book
            .need(&book.check(3693930347660122883).unwrap().needs, &c)
            .unwrap();
        let all: [&str; 4] = [
            item::RANGE_BOW,
            item::LONGSHOT,
            item::HEAVY_BOOT,
            actor::ARMOR_ADULT_ZORA,
        ];
        assert!(needs.met(&have(&all), Era::Adult));
        assert!(!needs.met(&have(&all), Era::Child));
        for missing in all {
            let some: Vec<_> = all.into_iter().filter(|i| *i != missing).collect();
            assert!(!needs.met(&have(&some), Era::Adult), "without {missing}");
        }
    }

    #[test]
    fn spirit_temple_chests_each_take_their_own_side() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let ages = |hash: u64| {
            let needs = book.need(&book.check(hash).unwrap().needs, &c).unwrap();
            (needs.possible(Era::Child), needs.possible(Era::Adult))
        };
        assert_eq!(ages(5309180490719879013), (true, false), "the bridge");
        assert_eq!(ages(1501288453962537671), (true, true), "the climb");
        let adult_side = [
            12909857529499414969,
            9143177002861777924,
            14040210993541550542,
            13828480906010430010,
        ];
        for hash in adult_side {
            assert_eq!(ages(hash), (false, true), "{hash}");
        }
    }

    #[test]
    fn sheik_in_the_crater_is_past_the_gap_and_the_heat() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let needs = book
            .need(&book.check(7517995748820283814).unwrap().needs, &c)
            .unwrap();
        let met = |items: &[&str]| needs.met(&have(items), Era::Adult);
        let tunic = actor::ARMOR_ADULT_GORON;
        assert!(!met(&[tunic]), "the tunic alone doesn't cross the gap");
        assert!(!met(&[item::HOOKSHOT]), "the hookshot alone burns");
        assert!(met(&[tunic, item::HOOKSHOT]));
        assert!(met(&[tunic, item::HOVER_BOOT]));
        assert!(met(&[tunic, item::OCARINA_FAIRY, item::MELODY_FIRE_BOLERO]));
    }

    #[test]
    fn the_tunic_and_bag_givers_need_what_stops_or_thaws_them() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let gift = |giver: &str| {
            let gift = crate::gifts::GIFTS.iter().find(|g| g.giver == giver);
            let needs = &book.check(gift.unwrap().hash()).unwrap().needs;
            book.need(needs, &c).unwrap()
        };
        let cases: [(&str, Era, &[&str]); 3] = [
            ("Link the Goron", Era::Adult, &[item::BOMB_BAG_SMALL]),
            ("King Zora", Era::Adult, &[item::BOTTLE]),
            ("Rolling Goron", Era::Child, &[item::BOMB_BAG_SMALL]),
        ];
        for (giver, era, items) in cases {
            let needs = gift(giver);
            assert!(!needs.met(&have(&[]), era), "{giver} with nothing");
            assert!(needs.met(&have(items), era), "{giver} with {items:?}");
        }
        let sheik = book.check(7517995748820283814).unwrap();
        let sheik = book.need(&sheik.needs, &c).unwrap();
        let given = have(&[actor::OBJ_ARMOR_GORON, item::HOOKSHOT]);
        assert!(sheik.met(&given, Era::Adult), "the given tunic counts");
    }

    #[test]
    fn each_minigame_prize_needs_what_its_game_does() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let (bag, bow) = (item::BOMBCHU_BUNDLE_SMALL, item::RANGE_BOW);
        let (sling, gerudo) = (item::RANGE_SLING_SHOT, item::TOKEN_GERUDO);
        let (letter, saria) = (item::ZELDA_LETTER, item::MELODY_SARIA_SONG);
        let ocarina = item::OCARINA_FAIRY;
        let epona = item::MELODY_EPONA_SONG;
        let cases: Vec<(&str, Era, Vec<&str>)> = vec![
            (
                "Bombchu bowling: second prize",
                Era::Child,
                vec![bag, item::BOMB_BAG_SMALL],
            ),
            (
                "Horseback archery: quiver",
                Era::Adult,
                vec![bow, ocarina, epona, gerudo],
            ),
            ("Market shooting gallery", Era::Child, vec![]),
            ("Kakariko shooting gallery", Era::Adult, vec![bow]),
            ("Lost Woods target", Era::Child, vec![sling]),
            ("Talon's cuccos", Era::Child, vec![item::WEIRD_EGG]),
            ("Deku Theater: Skull Mask", Era::Child, vec![letter]),
            (
                "Deku Theater: Mask of Truth",
                Era::Child,
                vec![letter, ocarina, saria],
            ),
        ];
        for (giver, era, items) in cases {
            let gift = crate::gifts::GIFTS.iter().find(|g| g.giver == giver);
            let needs = &book.check(gift.unwrap().hash()).unwrap().needs;
            let needs = book.need(needs, &c).unwrap();
            assert!(needs.met(&have(&items), era), "{giver} with {items:?}");
            for gone in 0..items.len() {
                let mut fewer = items.clone();
                fewer.remove(gone);
                assert!(
                    !needs.met(&have(&fewer), era),
                    "{giver} without {}",
                    items[gone]
                );
            }
        }
    }

    #[test]
    fn the_minigame_chests_need_the_lens_and_the_races_nothing() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let chest = |hash| book.need(&book.check(hash).unwrap().needs, &c).unwrap();
        let [treasure, hookshot, heart] = crate::gifts::PRIZE_CHESTS.map(chest);
        let lens: [&str; 2] = [item::FAIRY_BLESS_MP, item::GLASS_TRUTH];
        assert!(treasure.met(&have(&lens), Era::Child));
        assert!(!treasure.met(&have(&lens[..1]), Era::Child));
        assert!(!treasure.met(&have(&[]), Era::Child));
        for race in [hookshot, heart] {
            assert!(race.met(&have(&[]), Era::Adult));
        }
    }

    #[test]
    fn the_new_givers_need_their_rules_and_nothing_less() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        // Placed pickups have no giver, so these are by hash.
        let pickups = [
            ("Fire Arrows", 4598657944866228526),
            ("Ruto's letter", 474136620438465634),
        ];
        let gift = |giver: &str| {
            let hash = match pickups.iter().find(|p| p.0 == giver) {
                Some(p) => p.1,
                None => {
                    let gift = crate::gifts::GIFTS.iter().find(|g| g.giver == giver);
                    gift.unwrap().hash()
                }
            };
            let needs = &book.check(hash).unwrap().needs;
            book.need(needs, &c).unwrap()
        };
        let poes: [&str; 4] = [
            item::BOTTLE,
            item::RANGE_BOW,
            item::OCARINA_FAIRY,
            item::MELODY_EPONA_SONG,
        ];
        let (bongo, twinrova, morpha) = (beat("bongo-bongo"), beat("twinrova"), beat("morpha"));
        let cases: [(&str, Era, &[&str]); 6] = [
            (
                "Light Arrows",
                Era::Adult,
                &[bongo.as_str(), twinrova.as_str()][..],
            ),
            ("Poe collector", Era::Adult, &poes[..]),
            (
                "Fire Arrows",
                Era::Adult,
                &[item::RANGE_BOW, morpha.as_str()][..],
            ),
            ("Ruto's letter", Era::Child, &[&**item::SCALE_SILVER][..]),
            ("Weird Egg", Era::Child, &[][..]),
            ("Potion shop bottle", Era::Child, &[][..]),
        ];
        for (giver, era, items) in cases {
            let needs = gift(giver);
            assert!(needs.met(&have(items), era), "{giver} with {items:?}");
            for (i, left) in items.iter().enumerate() {
                let mut rest = items.to_vec();
                rest.remove(i);
                assert!(!needs.met(&have(&rest), era), "{giver} without {left}");
            }
        }
        assert!(!gift("Poe collector").met(&have(&poes), Era::Child));
        let knife = gift("Medigoron");
        for blast in [item::BOMB_BAG_SMALL, item::HAMMER] {
            assert!(knife.met(&have(&[blast]), Era::Adult), "{blast}");
        }
        assert!(!knife.met(&have(&[]), Era::Adult));
        assert!(!knife.met(&have(&[item::HAMMER]), Era::Child));
        // The bean seller sits past the boulders at Zora River's mouth.
        let beans = gift("Bean seller");
        assert!(!beans.met(&have(&[]), Era::Child));
        for past in [item::BOMB_BAG_SMALL, item::SCALE_SILVER] {
            assert!(beans.met(&have(&[past]), Era::Child), "{past}");
        }
        let letter = have(
            &poes[1..3]
                .iter()
                .copied()
                .chain([&**item::BOTTLE_LETTER, &**item::MELODY_EPONA_SONG])
                .collect::<Vec<_>>(),
        );
        assert!(
            gift("Poe collector").met(&letter, Era::Adult),
            "Ruto's letter is a bottle"
        );
        // Talon sleeps by the courtyard's crates until the egg's cucco
        // wakes him.
        for giver in ["Zelda's letter", "Impa"] {
            assert!(!gift(giver).met(&have(&[]), Era::Child), "{giver}");
            assert!(
                gift(giver).met(&have(&[item::WEIRD_EGG]), Era::Child),
                "{giver}"
            );
        }
    }

    #[test]
    fn the_sword_wants_the_claim_check_which_the_chain_gives() {
        needs_names!();
        let book = Book::load().unwrap();
        let c = ctx(false, &[]);
        let biggoron = crate::gifts::GIFTS.iter().find(|g| g.giver == "Biggoron");
        let sword = &book.check(biggoron.unwrap().hash()).unwrap().needs;
        let sword = book.need(sword, &c).unwrap();
        let check: [&str; 2] = [item::CLAIM_CHECK, item::BOMB_BAG_SMALL];
        assert!(sword.met(&have(&check), Era::Adult));
        assert!(!sword.met(&have(&check[1..]), Era::Adult));
        let chain = book.sources.iter().find(|s| s.name == "Claim Check");
        let chain = book.need(&chain.unwrap().expr, &c).unwrap();
        let all: [&str; 5] = [
            item::OCARINA_FAIRY,
            item::MELODY_ZELDA_LULLABY,
            item::BOTTLE,
            item::LONGSHOT,
            item::BOMB_BAG_SMALL,
        ];
        assert!(chain.met(&have(&all), Era::Adult));
        for (i, left) in all.iter().enumerate() {
            let mut rest = all.to_vec();
            rest.remove(i);
            assert!(!chain.met(&have(&rest), Era::Adult), "without {left}");
        }
        // The lake's tunnel into the domain, and Morpha thawing the King.
        let morpha = beat("morpha");
        let other: [&str; 4] = [item::SCALE_SILVER, &morpha, item::LONGSHOT, item::HAMMER];
        assert!(chain.met(&have(&other), Era::Adult));
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
        let built: BTreeSet<String> = FILES.iter().map(|f| f.0.to_owned()).collect();
        assert_eq!(on_disk, built);
    }

    #[test]
    fn every_item_word_is_an_item() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let known: BTreeSet<String> = crate::items::scan(&romfs)
            .unwrap()
            .into_iter()
            .map(|i| i.actor)
            .collect();
        let book = Book::load().unwrap();
        let c = ctx(true, &[]);
        let mut used = BTreeSet::new();
        let needs: Vec<Need> = book
            .abilities
            .values()
            .chain(book.checks.values().map(|c| &c.needs))
            .chain(book.exits.iter().map(|e| &e.expr))
            .chain(book.sources.iter().map(|s| &s.expr))
            .map(|e| book.need(e, &c).unwrap())
            .collect();
        needs.iter().for_each(|n| n.items(&mut used));
        let missing: Vec<_> = used
            .iter()
            .filter(|a| !known.contains(**a) && !a.starts_with(BEAT))
            .collect();
        assert!(missing.is_empty(), "not items: {missing:?}");
    }
}
