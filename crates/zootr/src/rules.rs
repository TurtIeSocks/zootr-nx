//! The rule language: short sentences saying what Link needs, like
//! `bombs or (adult and item:megaton-hammer)`. Rule files live in `logic/`
//! and are built into the binary. They speak in friendly words that
//! [`vocab`] maps to the game's names.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail, ensure};

use crate::entrances::Doors;
use crate::settings::{AdultJabu, DungeonEntrances, DungeonKeys, FortressKeys, NewGame};
use crate::world::Era;

mod files;
mod item;
mod lang;
mod set;
pub mod vocab;

pub use set::{Edit, set};

pub use files::{ExitRule, PlaceRule, SourceRule, settings_read};
pub use lang::{Expr, Have, Need, parse};

/// What a seed's rules resolve against.
pub struct Ctx {
    pub progressive: bool,
    /// The levels the seed's upgrade lines hold.
    pub lines: crate::progressive::Lines,
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
    /// Where each dungeon door leads, while dungeon entrances shuffle.
    pub doors: Option<Doors>,
    /// Link's age on a new file.
    pub age: Era,
    /// When Jabu-Jabu waits in the adult fountain, read by the word
    /// `adult-jabu-jabu`.
    pub adult_jabu: AdultJabu,
}

impl Ctx {
    /// Toggle `id`: met when on, unmet when off, left to the player when
    /// open.
    pub fn setting(&self, id: &str) -> Need {
        if self.open.contains(id) {
            Need::Setting(id.into())
        } else if self.on.contains(id) {
            Need::Always
        } else {
            Need::Never
        }
    }

    pub fn new(new_game: &NewGame) -> Self {
        Self {
            progressive: new_game.progressive,
            lines: crate::progressive::Lines::of(new_game),
            on: new_game.on.clone(),
            seed: new_game.seed,
            keys: new_game.keys,
            fortress: new_game.fortress_keys,
            open: BTreeSet::new(),
            doors: (new_game.dungeon_entrances != DungeonEntrances::Off)
                .then(|| new_game.doors.clone()),
            age: new_game.age,
            adult_jabu: new_game.adult_jabu,
        }
    }
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

/// Every rule file, parsed and checked.
pub struct Book {
    abilities: BTreeMap<String, Expr>,
    checks: BTreeMap<u64, CheckEntry>,
    pub exits: Vec<ExitRule>,
    pub alternatives: Vec<PlaceRule>,
    insides: Vec<PlaceRule>,
    pub sources: Vec<SourceRule>,
    toggles: BTreeSet<String>,
}

impl Book {
    /// What being inside the dungeon at `node` needs, resolved. Every
    /// dungeon has a rule: a moved door carries it along.
    pub fn inside(&self, node: &str, ctx: &Ctx) -> Result<Need> {
        let rule = (self.insides.iter())
            .find(|r| r.place == node)
            .with_context(|| format!("{node} has no inside rule"))?;
        self.need(&rule.expr, ctx)
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
        if let Some(need) = item::word(word, ctx.progressive, ctx.lines) {
            return need;
        }
        Ok(match word {
            "always" => Need::Always,
            "child" => Need::Era(Era::Child),
            "adult" => Need::Era(Era::Adult),
            "adult-jabu-jabu" => match ctx.adult_jabu {
                AdultJabu::Off => Need::Never,
                AdultJabu::Always => Need::Always,
                AdultJabu::AfterMorpha => return self.resolve(&vocab::id("beat:morpha")?, ctx),
            },
            _ => match word.split_once(':') {
                Some(("setting", id)) => {
                    ensure!(self.toggles.contains(id), "no toggle `{id}`");
                    ctx.setting(id)
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

#[cfg(test)]
mod tests {
    use super::files::ABILITIES;
    use super::*;
    use crate::known::{actor, item, map};
    use crate::settings;

    /// The id of a boss defeat, by its word.
    fn beat(word: &str) -> String {
        vocab::id(&format!("beat:{word}")).unwrap()
    }

    fn ctx(progressive: bool, on: &[&str]) -> Ctx {
        Ctx {
            progressive,
            lines: Default::default(),
            on: on.iter().map(|s| s.to_string()).collect(),
            seed: 0,
            keys: DungeonKeys::Vanilla,
            fortress: FortressKeys::Vanilla,
            open: BTreeSet::new(),
            doors: None,
            age: Era::Child,
            adult_jabu: AdultJabu::Off,
        }
    }

    fn have(items: &[&str]) -> Have {
        let mut have = Have::default();
        items.iter().for_each(|i| have.add(i));
        have
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
    fn adult_jabu_jabu_reads_its_mode() {
        needs_names!();
        let book = Book::load().unwrap();
        let word = |mode| {
            let ctx = Ctx {
                adult_jabu: mode,
                ..ctx(true, &[])
            };
            book.resolve("adult-jabu-jabu", &ctx).unwrap()
        };
        assert!(matches!(word(AdultJabu::Off), Need::Never));
        assert!(matches!(word(AdultJabu::Always), Need::Always));
        let morpha = beat("morpha");
        assert!(word(AdultJabu::AfterMorpha).met(&have(&[&morpha]), Era::Adult));
        assert!(!word(AdultJabu::AfterMorpha).met(&have(&[]), Era::Adult));
    }

    #[test]
    fn settings_switch_rules() {
        needs_names!();
        let book = Book::load().unwrap();
        let chus: &[&str] = &[item::BOMBCHU_BUNDLE_SMALL];
        let bag = &[chus[0], crate::bombchu_bags::SMALL.get()];
        let bombs = &[chus[0], item::BOMB_BAG_SMALL.get()];
        let chus_in = |mode: &str| book.resolve("bombchus", &ctx(true, &[mode])).unwrap();
        let (bomb_bag, free, bags) = (
            chus_in("bombchus=bomb-bag"),
            chus_in("bombchus=free"),
            chus_in("bombchus=bag"),
        );
        assert!(!bomb_bag.met(&have(chus), Era::Child));
        assert!(bomb_bag.met(&have(bombs), Era::Child));
        assert!(free.met(&have(chus), Era::Child));
        assert!(!bags.met(&have(bombs), Era::Child));
        assert!(bags.met(&have(bag), Era::Child));
        let sticks = |on: &[&str], items: &[&str]| {
            let need = book.resolve("sticks", &ctx(true, on)).unwrap();
            need.met(&have(items), Era::Child)
        };
        let bag = item::STICK_BAG_SMALL.get();
        assert!(sticks(&["shuffle-stick-bag=off"], &[]));
        assert!(!sticks(&["shuffle-stick-bag=on"], &[]));
        assert!(sticks(&["shuffle-stick-bag=on"], &[bag]));
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
    fn ageless_weapons_let_the_other_age_use_swords_and_shields() {
        needs_names!();
        let book = Book::load().unwrap();
        // Whether the other age may use it with the setting on. Live: the
        // tunics and boots do nothing on a child, and a child can't aim
        // the Mirror Shield's reflection.
        let cases = [
            ("mirror-shield", "mirror-shield", Era::Adult, false),
            ("kokiri-sword", "kokiri-sword", Era::Child, true),
            ("deku-shield", "deku-shield-chest", Era::Child, true),
            ("hover-boots", "hover-boots", Era::Adult, false),
            ("iron-boots", "iron-boots", Era::Adult, false),
            ("zora-tunic", "zora-tunic", Era::Adult, false),
            ("goron-tunic", "goron-tunic", Era::Adult, false),
        ];
        for (word, item, home, shared) in cases {
            let other = match home {
                Era::Adult => Era::Child,
                Era::Child => Era::Adult,
            };
            let held = have(&[&vocab::id(&format!("item:{item}")).unwrap()]);
            let met =
                |on: &[&str], era| book.resolve(word, &ctx(true, on)).unwrap().met(&held, era);
            assert!(met(&[], home) && !met(&[], other), "{word} off");
            let on = [settings::AGELESS_WEAPONS];
            assert!(met(&on, home), "{word} on, at its own age");
            assert_eq!(met(&on, other), shared, "{word} on, at the other age");
        }
    }

    #[test]
    fn light_needs_an_adult_with_the_mirror_shield_even_with_ageless_weapons() {
        needs_names!();
        let book = Book::load().unwrap();
        let held = have(&[item::SHIELD_MIRROR]);
        let met = |on: &[&str], era| {
            book.resolve("light", &ctx(true, on))
                .unwrap()
                .met(&held, era)
        };
        assert!(met(&[], Era::Adult) && !met(&[], Era::Child));
        assert!(!met(&[settings::AGELESS_WEAPONS], Era::Child));
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
        let c = ctx(true, &["bombchus=bomb-bag"]);
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
        let lens: [&str; 2] = [crate::magic::meter::NAME, item::GLASS_TRUTH];
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
    fn each_zora_preset_opens_its_way_without_the_item() {
        needs_names!();
        let book = Book::load().unwrap();
        let exit = |from: &str, to: &str| {
            let (from, to) = (vocab::node(from).unwrap(), vocab::node(to).unwrap());
            book.exits.iter().find(|e| e.from == from && e.to == to)
        };
        let lullaby: &[&str] = &[item::OCARINA_FAIRY, item::MELODY_ZELDA_LULLABY];
        let ways: [(&str, &str, &str, &[&str], Era); 4] = [
            (
                "zora-domain-child",
                "zora-fountain-child",
                "king-zora-moved",
                &[item::BOTTLE_LETTER],
                Era::Child,
            ),
            (
                "zora-fountain-child",
                "jabu-jabu",
                "jabu-open",
                &[item::BOTTLE],
                Era::Child,
            ),
            (
                "zora-river-child",
                "zora-domain-child",
                "open-waterfall",
                lullaby,
                Era::Child,
            ),
            (
                "zora-river-adult",
                "zora-domain-adult",
                "open-waterfall",
                lullaby,
                Era::Adult,
            ),
        ];
        for (from, to, setting, item, era) in ways {
            let expr = &exit(from, to).unwrap().expr;
            let need = |on: &[&str]| book.need(expr, &ctx(true, on)).unwrap();
            assert!(need(&[setting]).met(&have(&[]), era), "{setting}");
            assert!(!need(&[]).met(&have(&[]), era), "{from} without {setting}");
            assert!(need(&[]).met(&have(item), era), "{from} with {item:?}");
        }
        // Only the child map walls King Zora off.
        let fountain = map::ZORA_FOUNTAIN_ADULT.get();
        assert!(
            !book.exits.iter().any(|e| e.to == fountain),
            "adult fountain"
        );
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
            .chain(crate::known::ours().map(String::from))
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
            .filter(|a| !known.contains(**a) && !a.starts_with(super::item::BEAT))
            .collect();
        assert!(missing.is_empty(), "not items: {missing:?}");
    }
}
