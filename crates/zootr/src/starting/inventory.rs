//! What a new game starts holding: the Inventory panel's grid, the save
//! editor's own, run on a fresh save. The setting keeps it as words
//! (`hookshot: 1`, `bombs: 20`), so neither it nor `--start` needs the
//! game's names, and an old config upgrades before any dump is open.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::bombchu_bags;
use crate::inventory::{self, Held, Pouch, Section};
use crate::known::{Name, actor, field, item};
use crate::magic::meter;
use crate::progressive;
use crate::save_editor::{LIFE, MAX_LIFE, RUPEES};
use crate::savefile::Edit;
use crate::settings::Settings;

/// Its setting's id.
pub const SETTING: &str = "start-inventory";

/// What a new game starts with while the setting is unset.
pub const DEFAULT: [&str; 1] = ["master-sword"];

/// The `--start` word for starting with nothing, not [`DEFAULT`].
pub const NONE: &str = "none";

/// The `--start` word for all of Link's gear at its top tier, full ammo,
/// every song, every trade item and twenty hearts (every heart container
/// and piece): the kit room logic is measured with.
pub const ARSENAL: &str = "arsenal";

/// What [`ARSENAL`] holds, by word and count.
#[rustfmt::skip]
const KIT: &[(&str, i32)] = &[
    ("kokiri-sword", 1), ("master-sword", 1), ("biggorons-sword", 1),
    ("deku-shield", 1), ("hylian-shield", 1), ("mirror-shield", 1),
    ("goron-tunic", 1), ("zora-tunic", 1), ("iron-boots", 1), ("hover-boots", 1),
    ("golden-gauntlets", 1), ("golden-scale", 1),
    ("bow", 1), ("biggest-quiver", 1), ("arrows", 50),
    ("fire-arrows", 1), ("ice-arrows", 1), ("light-arrows", 1),
    ("slingshot", 1), ("biggest-bullet-bag", 1), ("deku-seeds", 50),
    ("biggest-bomb-bag", 1), ("bombs", 40), ("biggest-bombchu-bag", 1), ("bombchus", 40),
    ("biggest-stick-bag", 1), ("deku-sticks", 30), ("biggest-nut-bag", 1), ("deku-nuts", 40),
    ("longshot", 1), ("boomerang", 1), ("megaton-hammer", 1), ("lens-of-truth", 1),
    ("dins-fire", 1), ("farores-wind", 1), ("nayrus-love", 1), ("double-magic", 1), ("improved-spin", 1),
    ("ocarina-of-time", 1), ("magic-beans", 10),
    ("bottle-1", 1), ("bottle-2", 1), ("bottle-3", 1), ("bottle-4", 1), ("bottle-5", 1),
    ("business-scrub-seeds", 30),
    ("keaton-mask", 1), ("skull-mask", 1), ("spooky-mask", 1), ("bunny-hood", 1),
    ("goron-mask", 1), ("zora-mask", 1), ("gerudo-mask", 1), ("mask-of-truth", 1),
    ("weird-egg", 1), ("cucco", 1), ("zeldas-letter", 1),
    ("pocket-egg", 1), ("pocket-cucco", 1), ("cojiro", 1), ("odd-mushroom", 1), ("odd-powder", 1),
    ("poachers-saw", 1), ("broken-sword", 1), ("prescription", 1), ("eyeball-frog", 1),
    ("eye-drops", 1), ("claim-check", 1),
    ("stone-of-agony", 1), ("gerudo-card", 1), ("double-defense", 1), ("gold-skulltula-tokens", 100),
    ("giants-wallet", 1), ("rupees", 1000), ("heart-containers", 8), ("heart-pieces", 36),
    ("zeldas-lullaby", 1), ("eponas-song", 1), ("sarias-song", 1), ("suns-song", 1),
    ("song-of-time", 1), ("song-of-storms", 1), ("minuet-of-forest", 1), ("bolero-of-fire", 1),
    ("serenade-of-water", 1), ("requiem-of-spirit", 1), ("nocturne-of-shadow", 1), ("prelude-of-light", 1),
];

/// The ammo a weapon's grant brings with it, which no remove takes back.
const BROUGHT: [(&Name, &Name, i32); 2] = [
    (actor::WEAPON_RANGE_BOW, item::ARROW_NORMAL, 30),
    (actor::WEAPON_RANGE_SLING_SHOT, item::DEKU_SEED, 30),
];

/// Heart containers, past the three hearts every new game has.
const HEARTS: &str = "heart-containers";
/// The hearts every new game has.
const BASE_HEARTS: i32 = 3;
/// The heart containers the game hides, one per boss.
pub(crate) const CONTAINERS: i32 = 8;
/// The heart pieces the game hides, and how many make a heart.
const PIECES: &str = "heart-pieces";
const PER_HEART: i32 = 4;
const RUPEE_WORD: &str = "rupees";

/// Every grid actor's word, as a player would say it. Rule-file words
/// (`rules::vocab`) are spelled the same.
#[rustfmt::skip]
pub const WORDS: &[(&str, &Name)] = &[
    ("stick-bag", item::STICK_BAG_SMALL), ("big-stick-bag", item::STICK_BAG_MEDIUM), ("biggest-stick-bag", item::STICK_BAG_LARGE),
    ("deku-sticks", item::DEKU_STICK),
    ("nut-bag", item::NUT_BAG_SMALL), ("big-nut-bag", item::NUT_BAG_MEDIUM), ("biggest-nut-bag", item::NUT_BAG_LARGE),
    ("deku-nuts", item::DEKU_NUT),
    ("bomb-bag", item::BOMB_BAG_SMALL), ("big-bomb-bag", item::BOMB_BAG_MEDIUM), ("biggest-bomb-bag", item::BOMB_BAG_LARGE),
    ("bombs", item::BOMB),
    ("bow", actor::WEAPON_RANGE_BOW), ("arrows", item::ARROW_NORMAL),
    ("fire-arrows", item::ARROW_FIRE), ("dins-fire", item::DIN_FIRE),
    ("slingshot", actor::WEAPON_RANGE_SLING_SHOT), ("deku-seeds", item::DEKU_SEED),
    ("fairy-ocarina", item::OCARINA_FAIRY), ("ocarina-of-time", item::OCARINA_TIME),
    ("bombchu-bag", bombchu_bags::SMALL), ("big-bombchu-bag", bombchu_bags::MEDIUM), ("biggest-bombchu-bag", bombchu_bags::LARGE),
    ("bombchus", item::BOMBCHU),
    ("hookshot", item::HOOKSHOT), ("longshot", item::LONGSHOT),
    ("ice-arrows", item::ARROW_ICE), ("farores-wind", item::FARORE_WIND),
    ("boomerang", item::BOOMERANG), ("lens-of-truth", actor::ARMOR_GLASS_TRUTH),
    ("magic-beans", item::MAGIC_BEAN), ("megaton-hammer", item::HAMMER),
    ("light-arrows", item::ARROW_LIGHT), ("nayrus-love", item::NAYRU_LOVE),
    ("bottle-1", field::BOTTLE_A), ("bottle-2", field::BOTTLE_B), ("bottle-3", field::BOTTLE_C), ("bottle-4", field::BOTTLE_D),
    ("bottle-5", field::BOTTLE_E), ("business-scrub-seeds", item::SCRUB_BUSINESS_SEED),
    ("weird-egg", item::WEIRD_EGG), ("cucco", item::ALARM_CUCCO), ("zeldas-letter", item::ZELDA_LETTER),
    ("keaton-mask", actor::ARMOR_CHILD_MASK_KEATON), ("skull-mask", actor::ARMOR_CHILD_MASK_SKULL),
    ("spooky-mask", actor::ARMOR_CHILD_MASK_SPOOKY), ("bunny-hood", actor::ARMOR_CHILD_MASK_BUNNY),
    ("goron-mask", actor::ARMOR_CHILD_MASK_GORON), ("zora-mask", actor::ARMOR_CHILD_MASK_ZORA),
    ("gerudo-mask", actor::ARMOR_CHILD_MASK_GERUDO), ("mask-of-truth", actor::ARMOR_CHILD_MASK_TRUTH),
    ("pocket-egg", item::POCKET_EGG), ("pocket-cucco", item::POCKET_CUCCO), ("cojiro", item::COJIRO_CUCCO),
    ("odd-mushroom", item::ODD_MUSHROOM), ("odd-powder", item::ODD_POWDER), ("poachers-saw", item::POACHER_SAW),
    ("broken-sword", item::BROKEN_SWORD), ("prescription", item::PRESCRIPTION), ("eyeball-frog", item::EYEBALL_FROG),
    ("eye-drops", item::EYE_DROP), ("claim-check", item::CLAIM_CHECK),
    ("kokiri-sword", actor::WEAPON_SWORD_KOKIRI), ("master-sword", actor::WEAPON_SWORD_MASTER),
    ("giants-knife", actor::WEAPON_SWORD_GIANT), ("biggorons-sword", actor::WEAPON_SWORD_BIGGORON),
    ("deku-shield", actor::WEAPON_SHIELD_DEKU), ("hylian-shield", actor::WEAPON_SHIELD_HYLIAN),
    ("mirror-shield", actor::WEAPON_SHIELD_MIRROR),
    ("kokiri-tunic", actor::ARMOR_ADULT_NORMAL), ("goron-tunic", actor::ARMOR_ADULT_GORON), ("zora-tunic", actor::ARMOR_ADULT_ZORA),
    ("iron-boots", actor::ARMOR_ADULT_HEAVY_BOOT), ("hover-boots", actor::ARMOR_ADULT_HOVER_BOOT),
    ("goron-bracelet", actor::ARMOR_CHILD_GAUNTLET_GORON), ("silver-gauntlets", actor::ARMOR_ADULT_GAUNTLET_SILVER),
    ("golden-gauntlets", actor::ARMOR_ADULT_GAUNTLET_GOLD),
    ("silver-scale", item::SCALE_SILVER), ("golden-scale", item::SCALE_GOLD),
    ("adults-wallet", item::WALLET_MEDIUM), ("giants-wallet", item::WALLET_LARGE),
    ("quiver", item::QUIVER_SMALL), ("big-quiver", item::QUIVER_MEDIUM), ("biggest-quiver", item::QUIVER_LARGE),
    ("bullet-bag", item::BULLET_BAG_SMALL), ("big-bullet-bag", item::BULLET_BAG_MEDIUM), ("biggest-bullet-bag", item::BULLET_BAG_LARGE),
    ("heart-pieces", actor::OBJ_HEART_PIECE),
    ("magic-meter", meter::METER), ("double-magic", item::FAIRY_BLESS_MP_DOUBLE),
    ("improved-spin", item::FAIRY_BLESS_MP), ("double-defense", item::FAIRY_BLESS_DEFENSE_DOUBLE),
    ("gerudo-card", item::TOKEN_GERUDO), ("stone-of-agony", item::AGONY_STONE),
    ("gold-skulltula-tokens", item::TOKEN_GOLD),
    ("zeldas-lullaby", item::MELODY_ZELDA_LULLABY), ("eponas-song", item::MELODY_EPONA_SONG),
    ("sarias-song", item::MELODY_SARIA_SONG), ("suns-song", item::MELODY_SUN_SONG),
    ("song-of-time", item::MELODY_TIME_SONG), ("song-of-storms", item::MELODY_STORM_SONG),
    ("minuet-of-forest", item::MELODY_FOREST_MINUET), ("bolero-of-fire", item::MELODY_FIRE_BOLERO),
    ("serenade-of-water", item::MELODY_WATER_SERENADE), ("requiem-of-spirit", item::MELODY_SPIRIT_REQUIEM),
    ("nocturne-of-shadow", item::MELODY_SHADOW_NOCTURNE), ("prelude-of-light", item::MELODY_LIGHT_PRELUDE),
    ("kokiri-emerald", item::STONE_FOREST), ("goron-ruby", item::STONE_FIRE), ("zora-sapphire", item::STONE_WATER),
    ("forest-medallion", item::MEDAL_FOREST), ("fire-medallion", item::MEDAL_FIRE), ("water-medallion", item::MEDAL_WATER),
    ("spirit-medallion", item::MEDAL_SPIRIT), ("shadow-medallion", item::MEDAL_SHADOW), ("light-medallion", item::MEDAL_LIGHT),
];

/// Grid sections a new game can't start holding: too much of the game
/// reads the stones and medallions as quest progress.
pub const BARRED: [Section; 2] = [Section::Stones, Section::Medallions];

/// Grid actors whose chest item has another name: what a new game is
/// granted, what leaves the pool and what logic counts. The upgrade lines
/// (`progressive::LINES`) name theirs too.
const DROPS: &[(&Name, &Name)] = &[
    (actor::WEAPON_SWORD_KOKIRI, item::SWORD_KOKIRI),
    (actor::WEAPON_SHIELD_DEKU, actor::T_BOX_SHIELD_DEKU),
    (actor::WEAPON_SHIELD_MIRROR, item::SHIELD_MIRROR),
    (actor::ARMOR_GLASS_TRUTH, item::GLASS_TRUTH),
    (actor::ARMOR_ADULT_HEAVY_BOOT, item::HEAVY_BOOT),
    (actor::ARMOR_ADULT_HOVER_BOOT, item::HOVER_BOOT),
    (actor::ARMOR_CHILD_MASK_KEATON, item::MASK_KEATON),
    (actor::ARMOR_CHILD_MASK_SKULL, item::MASK_SKULL),
    (actor::ARMOR_CHILD_MASK_SPOOKY, item::MASK_SPOOKY),
    (actor::ARMOR_CHILD_MASK_BUNNY, item::MASK_BUNNY),
    (actor::ARMOR_CHILD_MASK_GORON, item::MASK_GORON),
    (actor::ARMOR_CHILD_MASK_ZORA, item::MASK_ZORA),
    (actor::ARMOR_CHILD_MASK_GERUDO, item::MASK_GERUDO),
    (actor::ARMOR_CHILD_MASK_TRUTH, item::MASK_TRUTH),
    (field::BOTTLE_A, item::BOTTLE),
    (field::BOTTLE_B, item::BOTTLE),
    (field::BOTTLE_C, item::BOTTLE),
    (field::BOTTLE_D, item::BOTTLE),
    (field::BOTTLE_E, item::BOTTLE),
];

/// Ammo whose first pickup also gives its bag, with that bag's tiers. A
/// bag held from the start marks its ammo got, or the menu hides the slot
/// and the first pickup shows its first-time text, as though no bag were
/// held.
const BAGGED: [(&Name, [&Name; 3]); 2] = [
    (
        item::DEKU_NUT,
        [
            item::NUT_BAG_SMALL,
            item::NUT_BAG_MEDIUM,
            item::NUT_BAG_LARGE,
        ],
    ),
    (
        item::DEKU_STICK,
        [
            item::STICK_BAG_SMALL,
            item::STICK_BAG_MEDIUM,
            item::STICK_BAG_LARGE,
        ],
    ),
];

/// Granted equipped: the sword and shield a child carries.
const EQUIPPED: [&Name; 2] = [item::SWORD_KOKIRI, actor::T_BOX_SHIELD_DEKU];

/// Granted right after it, e.g. the Slingshot's bullet bag, which the
/// game's own pickup adds but a quiet grant doesn't.
const WITH: [(&Name, &Name); 1] = [(item::RANGE_SLING_SHOT, item::BULLET_BAG_SMALL)];

/// The chest item a grid actor stands for.
pub fn drop(actor: &'static Name) -> &'static Name {
    let lines = (progressive::LINES.iter().flat_map(|l| l.iter())).map(|&(d, p)| (p, d));
    (lines.chain(DROPS.iter().copied()))
        .find(|&(pouch, _)| pouch == actor)
        .map_or(actor, |(_, d)| d)
}

/// One grant in the opening.
#[derive(Clone, Debug, PartialEq)]
pub struct Grant {
    pub actor: String,
    pub num: i32,
    pub equip: bool,
}

impl Grant {
    fn new(actor: &str, num: i32) -> Self {
        Self {
            actor: actor.to_owned(),
            num,
            equip: EQUIPPED.iter().any(|e| e.get() == actor),
        }
    }
}

/// What giving `actor` grants: the actor, equipped if a child carries it,
/// then what comes with it.
pub fn granted(actor: &str, num: i32) -> Vec<Grant> {
    let with = WITH.iter().filter(|(a, _)| a.get() == actor);
    let mut out = vec![Grant::new(actor, num)];
    out.extend(with.map(|(_, w)| Grant::new(w, 1)));
    out
}

/// What every new game holds already, so no word: the child's wallet, which
/// has no pack to grant and no place in the pool.
const ALWAYS: [&Name; 1] = [item::WALLET_SMALL];

/// What a new game starts holding, by word.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent, rename = "StartInventory")]
pub struct Start(pub BTreeMap<String, i32>);

impl Start {
    /// The setting's value, [`DEFAULT`] when unset.
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let start: Self = match settings.get(SETTING) {
            Some(v) => {
                serde_json::from_value(v.clone()).context("reading the starting inventory")?
            }
            None => Self::of(&DEFAULT),
        };
        start.check()?;
        Ok(start)
    }

    /// A new game holding each of `words` once. Each must be a word, as in
    /// a const list.
    pub fn of(words: &[&str]) -> Self {
        let mut start = Self::default();
        (words.iter()).for_each(|w| start.add(w, 1).expect("a starting word"));
        start
    }

    /// Fails on a word that isn't an item, a heart count or rupees, or
    /// names a stone or medallion.
    pub fn check(&self) -> Result<()> {
        self.0.keys().try_for_each(|w| actor_of(w).map(|_| ()))
    }

    /// Whether `word` is held at all.
    pub fn holds(&self, word: &str) -> bool {
        self.0.get(word).is_some_and(|&n| n > 0)
    }

    /// `word` held `n` times: every tier below it too, as a click does.
    pub fn add(&mut self, word: &str, n: i32) -> Result<()> {
        if word == ARSENAL {
            return KIT.iter().try_for_each(|&(w, n)| self.add(w, n));
        }
        if word == HEARTS && !(0..=CONTAINERS).contains(&n) {
            bail!("a new game starts with 0 to {CONTAINERS} heart containers, not {n}");
        }
        let Some(actor) = actor_of(word)? else {
            self.0.insert(word.into(), n);
            return Ok(());
        };
        for lower in inventory::below(actor)
            .into_iter()
            .filter(|a| !ALWAYS.contains(a))
        {
            let w = word_of(lower)?;
            let held = self.0.entry(w.into()).or_default();
            *held = (*held).max(1);
        }
        self.0.insert(word.into(), n);
        Ok(())
    }

    /// The hearts it starts with: a new game's three, one per heart
    /// container and one per four pieces.
    pub fn hearts(&self) -> i32 {
        let n = |w: &str| self.0.get(w).copied().unwrap_or(0);
        BASE_HEARTS + n(HEARTS) + n(PIECES) / PER_HEART
    }

    /// How many of grid actor `a` it holds.
    pub(crate) fn count(&self, a: &Name) -> i32 {
        let w = WORDS.iter().find(|(_, n)| *n == a).map(|(w, _)| *w);
        w.and_then(|w| self.0.get(w)).copied().unwrap_or(0)
    }

    /// Each grid actor held and its count, in grid order.
    fn held(&self) -> Vec<(Held, i32)> {
        (inventory::held().into_iter())
            .map(|h| (h, self.count(h.actor)))
            .filter(|&(_, n)| n > 0)
            .collect()
    }

    /// The opening's grants. Items in grid order, so the Hookshot comes
    /// before the Longshot. Then the ammo, past what its weapon and bags
    /// bring. Then a heart container per heart past a new game's, one
    /// grant each: a heart container's grant adds one whatever its count,
    /// and a granted heart piece adds none (live, 2026-11-23: eight pieces
    /// left three hearts), so the pieces' hearts come as containers too.
    pub fn grants(&self, pouch: &Pouch) -> Vec<Grant> {
        let mut out = self.items();
        for (h, n) in self.held().into_iter().filter(|(h, _)| h.ammo) {
            let n = n - self.brought(h.actor, pouch);
            out.extend((n > 0).then(|| Grant::new(h.actor.get(), n)));
        }
        let container = || Grant::new(actor::OBJ_HEART_CONTAINER.get(), 1);
        out.extend((BASE_HEARTS..self.hearts()).map(|_| container()));
        out
    }

    /// The grants of everything but ammo, in grid order. What comes with an
    /// item comes only when the grid doesn't hold it already.
    fn items(&self) -> Vec<Grant> {
        let held: Vec<_> = self.held().into_iter().filter(|(h, _)| !h.ammo).collect();
        let drops: Vec<&str> = held.iter().map(|(h, _)| drop(h.actor).get()).collect();
        let mut out = Vec::new();
        for ((_, n), d) in held.iter().zip(&drops) {
            let mut grants = granted(d, *n).into_iter();
            out.extend(grants.next());
            out.extend(grants.filter(|w| !drops.contains(&w.actor.as_str())));
        }
        out
    }

    /// The ammo `a` its held weapon and bags bring: the weapon its own, a
    /// bag past the first fills it.
    fn brought(&self, a: &Name, pouch: &Pouch) -> i32 {
        let items = self.items();
        let bags: Vec<&str> = items.iter().map(|g| g.actor.as_str()).collect();
        let weapon = (BROUGHT.iter())
            .filter(|(weapon, ammo, _)| *ammo == a && self.count(weapon) > 0)
            .map(|(_, _, n)| *n);
        weapon
            .chain([pouch.filled(a.get(), &bags)])
            .max()
            .unwrap_or(0)
    }

    /// It holding at least the ammo its weapons and bags bring, as the
    /// game gives it.
    fn floored(mut self, pouch: &Pouch) -> Self {
        for (word, a) in WORDS {
            let n = self.brought(a, pouch);
            if n > 0 {
                let held = self.0.entry((*word).to_owned()).or_default();
                *held = (*held).max(n);
            }
        }
        self
    }

    /// Fit it to a seed whose pouch table is `pouch` (`Pouch::seeded`):
    /// hold each smallest bag its ammo brings, as every new game then does,
    /// drop a Bombchu Bag the seed doesn't have, and bring each ammo
    /// down to the room its bags leave. Returns what came down or went, as
    /// `word: was -> now`.
    pub fn settle(&mut self, pouch: &Pouch) -> Vec<String> {
        for bag in pouch.brought_bags() {
            let word = WORDS.iter().find(|(_, a)| a.get() == bag);
            if let Some((w, _)) = word {
                self.0.entry((*w).to_owned()).or_insert(1);
            }
        }
        let mut notes = Vec::new();
        let mut note = |w: &str, was: i32, now: i32| notes.push(format!("{w}: {was} -> {now}"));
        for (w, a) in WORDS {
            let lacked = bombchu_bags::BAGS.contains(&a.get()) && pouch.bag_cap(a.get()).is_none();
            if lacked && let Some(was) = self.0.remove(*w) {
                note(w, was, 0);
            }
        }
        let held = self.held();
        let bags: Vec<&str> = (held.iter().map(|(h, _)| h.actor.get()))
            .filter(|a| pouch.bag_cap(a).is_some())
            .collect();
        for (h, n) in &held {
            let room = pouch.room(h.actor.get(), &bags).unwrap_or(*n);
            if *n > room {
                let w = word_of(h.actor).expect("a grid actor");
                match room {
                    0 => self.0.remove(w),
                    r => self.0.insert(w.to_owned(), r),
                };
                note(w, *n, room);
            }
        }
        notes
    }

    /// What Link owns from the start, once per unit: what leaves the pool
    /// and what logic counts. Ammo is neither.
    pub fn owned(&self) -> Vec<&'static str> {
        let containers = self.0.get(HEARTS).copied().unwrap_or(0).max(0);
        (self.held().into_iter())
            .filter(|(h, _)| !h.ammo)
            .map(|(h, n)| (drop(h.actor), n))
            .chain([(actor::OBJ_HEART_CONTAINER, containers)])
            .flat_map(|(d, n)| std::iter::repeat_n(d.get(), n as usize))
            .collect()
    }

    /// GameData default edits for a seed: the rupees, and the ammo a held
    /// bag marks got (`BAGGED`). A new game sets its own health, so heart
    /// containers are granted.
    pub fn defaults(&self) -> Vec<String> {
        let rupees = self.0.get(RUPEE_WORD);
        let bagged = (BAGGED.iter())
            .filter(|(_, bags)| bags.iter().any(|b| self.count(b) > 0))
            .map(|(ammo, _)| format!("IsGet.{ammo}=true"));
        (rupees.iter())
            .map(|n| format!("#{RUPEES:08x}={n}"))
            .chain(bagged)
            .collect()
    }

    /// The Status edits the panel's save takes: its hearts past a new
    /// game's, with health full, and the rupees.
    fn status(&self) -> Vec<Edit> {
        let quarters = self.hearts() * 4;
        let hearts =
            (self.hearts() > BASE_HEARTS).then_some([(MAX_LIFE, quarters), (LIFE, quarters)]);
        let rupees = self.0.get(RUPEE_WORD).map(|&n| (RUPEES, n));
        (hearts.into_iter().flatten().chain(rupees))
            .map(|(hash, n)| Edit {
                hash,
                index: 0,
                value: n.to_string(),
            })
            .collect()
    }
}

/// Brings a saved starting inventory up to date: the stones and
/// medallions it may hold, which no new game starts with now, are dropped.
/// A heart count past [`CONTAINERS`] is from when the word counted every
/// heart, and becomes the containers and pieces that make as many.
pub fn upgrade(start: &mut serde_json::Value) {
    let Some(words) = start.as_object_mut() else {
        return;
    };
    words.retain(|w, _| {
        let actor = WORDS.iter().find(|(k, _)| k == w);
        !actor.is_some_and(|(_, a)| barred(a))
    });
    let old = words.get(HEARTS).and_then(serde_json::Value::as_i64);
    if let Some(hearts) = old.filter(|&h| h > i64::from(CONTAINERS)) {
        let past = hearts as i32 - BASE_HEARTS;
        let pieces = (past - CONTAINERS) * PER_HEART;
        let held = words.get(PIECES).and_then(serde_json::Value::as_i64);
        let pieces = (held.unwrap_or(0) as i32).max(pieces).min(36);
        words.insert(HEARTS.into(), CONTAINERS.into());
        words.insert(PIECES.into(), pieces.into());
    }
}

/// `--start`'s `WORD[=N]`: a word it takes, and how many, 1 unless said.
pub fn parse(arg: &str) -> Result<(String, i32)> {
    if arg == NONE {
        return Ok((NONE.into(), 0));
    }
    let (word, n) = match arg.split_once('=') {
        Some((w, n)) => (w, n.parse().with_context(|| format!("{n} isn't a count"))?),
        None => (arg, 1),
    };
    Start::default().add(word, n)?;
    Ok((word.to_owned(), n))
}

/// Every word `--start` takes.
pub fn words() -> String {
    let items = WORDS.iter().filter(|(_, a)| !barred(a)).map(|(w, _)| *w);
    let all = items.chain([HEARTS, RUPEE_WORD, NONE, ARSENAL]);
    all.collect::<Vec<_>>().join(", ")
}

/// Whether grid actor `a` is a stone or medallion, which no new game
/// starts with.
fn barred(a: &Name) -> bool {
    inventory::section(a).is_some_and(|s| BARRED.contains(&s))
}

/// The grid actor `word` names, none for a heart or rupee count. Fails on
/// a word `--start` doesn't take.
fn actor_of(word: &str) -> Result<Option<&'static Name>> {
    if word == HEARTS || word == RUPEE_WORD {
        return Ok(None);
    }
    let found = WORDS.iter().find(|(w, _)| *w == word);
    match found {
        Some((_, a)) if !barred(a) => Ok(Some(*a)),
        _ => bail!(
            "a new game can't start with {word}. `--start` takes {}",
            words()
        ),
    }
}

fn word_of(actor: &Name) -> Result<&'static str> {
    let w = WORDS.iter().find(|(_, a)| *a == actor);
    w.map(|(w, _)| *w)
        .with_context(|| format!("{actor} has no word"))
}

mod panel;
pub use panel::{click, items, onto};

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn start(words: &[(&str, i32)]) -> Start {
        Start(words.iter().map(|&(w, n)| (w.to_owned(), n)).collect())
    }

    #[test]
    fn every_grid_actor_has_one_word_and_every_word_one_actor() {
        for h in inventory::held() {
            let n = WORDS.iter().filter(|(_, a)| *a == h.actor).count();
            let want = if ALWAYS.contains(&h.actor) { 0 } else { 1 };
            assert_eq!(n, want, "{:?}", h.actor);
        }
        let grid: Vec<_> = inventory::held().into_iter().map(|h| h.actor).collect();
        for (w, a) in WORDS {
            assert!(grid.contains(a), "{w}");
            assert_eq!(WORDS.iter().filter(|(o, _)| o == w).count(), 1, "{w}");
        }
    }

    #[test]
    fn a_bigger_wallet_leaves_out_the_one_every_game_has() {
        needs_names!();
        let mut s = Start::default();
        s.add("giants-wallet", 1).unwrap();
        assert_eq!(s, start(&[("adults-wallet", 1), ("giants-wallet", 1)]));
        assert!(!s.owned().contains(&item::WALLET_SMALL.get()));
        assert!(Start::default().add("child-wallet", 1).is_err());
    }

    #[test]
    fn the_arsenal_holds_every_kit_word_and_no_word_of_its_own() {
        needs_names!();
        let mut s = Start::default();
        s.add(ARSENAL, 1).unwrap();
        s.check().unwrap();
        assert!(!s.holds(ARSENAL));
        for &(w, n) in KIT {
            assert_eq!(s.0.get(w), Some(&n), "{w}");
        }
        // A top tier brings the tiers below it.
        for w in [
            "hookshot",
            "silver-gauntlets",
            "goron-bracelet",
            "silver-scale",
        ] {
            assert!(s.holds(w), "{w}");
        }
        assert_eq!(parse(ARSENAL).unwrap(), (ARSENAL.into(), 1));
    }

    #[test]
    fn unknown_words_are_refused() {
        assert!(start(&[("hookshot", 1), (HEARTS, 5)]).check().is_ok());
        assert!(Start::default().add(HEARTS, CONTAINERS + 1).is_err());
        let err = start(&[("hookshoot", 1)]).check().unwrap_err();
        assert!(err.to_string().contains("hookshoot"));
        let settings = Settings::from([(SETTING.into(), serde_json::json!({ "nope": 1 }))]);
        assert!(Start::from_settings(&settings).is_err());
        assert_eq!(
            Start::from_settings(&Settings::new()).unwrap(),
            Start::of(&DEFAULT)
        );
        let none = Settings::from([(SETTING.into(), serde_json::json!({}))]);
        assert_eq!(Start::from_settings(&none).unwrap(), Start::default());
    }

    #[test]
    fn no_new_game_starts_with_a_stone_or_medallion() {
        for w in ["kokiri-emerald", "forest-medallion", "light-medallion"] {
            assert!(parse(w).is_err(), "{w}");
            assert!(start(&[(w, 1)]).check().is_err(), "{w}");
            assert!(!words().split(", ").any(|k| k == w), "{w}");
        }
        let mut old = serde_json::json!({ "goron-ruby": 1, "spirit-medallion": 1, "hookshot": 1 });
        upgrade(&mut old);
        assert_eq!(old, serde_json::json!({ "hookshot": 1 }));
    }

    #[test]
    fn a_word_brings_the_tiers_below_it() {
        let mut s = Start::default();
        s.add("longshot", 1).unwrap();
        s.add("gold-skulltula-tokens", 10).unwrap();
        assert_eq!(
            s,
            start(&[
                ("gold-skulltula-tokens", 10),
                ("hookshot", 1),
                ("longshot", 1)
            ])
        );
        assert!(s.add("nope", 1).is_err());
    }

    #[test]
    fn grants_follow_the_grid_then_the_ammo_then_the_hearts() {
        needs_names!();
        let s = start(&[
            ("bombs", 20),
            ("bomb-bag", 1),
            ("big-bomb-bag", 1),
            ("longshot", 1),
            ("hookshot", 1),
            ("kokiri-sword", 1),
            (HEARTS, 1),
            (PIECES, 5),
        ]);
        let got: Vec<_> = (s.grants(&Pouch::default()).into_iter())
            .map(|g| (g.actor, g.num, g.equip))
            .collect();
        let want = [
            (item::BOMB_BAG_SMALL.get(), 1, false),
            (item::BOMB_BAG_MEDIUM.get(), 1, false),
            (item::HOOKSHOT.get(), 1, false),
            (item::LONGSHOT.get(), 1, false),
            (item::SWORD_KOKIRI.get(), 1, true),
            (actor::OBJ_HEART_PIECE.get(), 5, false),
            (item::BOMB.get(), 20, false),
            (actor::OBJ_HEART_CONTAINER.get(), 1, false),
            (actor::OBJ_HEART_CONTAINER.get(), 1, false),
        ];
        assert_eq!(got, want.map(|(a, n, e)| (a.to_owned(), n, e)));
        let none = start(&[(HEARTS, 0)]).grants(&Pouch::default());
        assert!(none.is_empty(), "{none:?}");
    }

    /// A pouch table with the quivers' group, for tests.
    fn quivers() -> Pouch {
        let bags = [
            (item::QUIVER_SMALL.get(), "Arrow", 30),
            (item::QUIVER_MEDIUM.get(), "Arrow", 40),
        ];
        Pouch::of(&bags, &[(item::ARROW_NORMAL.get(), "Arrow")])
    }

    #[test]
    fn ammo_is_granted_past_what_its_weapon_and_bags_bring() {
        needs_names!();
        let arrows = |words: &[(&str, i32)]| {
            (start(words).grants(&quivers()).into_iter())
                .filter(|g| g.actor == item::ARROW_NORMAL.get())
                .map(|g| g.num)
                .collect::<Vec<_>>()
        };
        // The Bow brings 30, a quiver past the first fills to its cap.
        assert_eq!(arrows(&[("bow", 1), ("quiver", 1), ("arrows", 30)]), [0; 0]);
        assert_eq!(arrows(&[("bow", 1), ("quiver", 1), ("arrows", 40)]), [10]);
        let big = [("bow", 1), ("quiver", 1), ("big-quiver", 1), ("arrows", 40)];
        assert_eq!(arrows(&big), [0; 0]);
        assert_eq!(arrows(&[("quiver", 1), ("arrows", 20)]), [20]);
    }

    #[test]
    fn the_grid_shows_the_ammo_its_weapons_and_bags_bring() {
        needs_names!();
        let floored = |words: &[(&str, i32)]| start(words).floored(&quivers()).0;
        let s = floored(&[("slingshot", 1), ("deku-seeds", 10), ("bow", 1)]);
        assert_eq!((s["deku-seeds"], s["arrows"]), (30, 30));
        assert_eq!(floored(&[("bow", 1), ("arrows", 35)])["arrows"], 35);
        let big = floored(&[("quiver", 1), ("big-quiver", 1)]);
        assert_eq!(big["arrows"], 40);
        assert!(!floored(&[("quiver", 1)]).contains_key("arrows"));
    }

    #[test]
    fn the_slingshot_brings_its_bag_unless_the_grid_has_one() {
        needs_names!();
        let actors = |s: Start| {
            s.grants(&Pouch::default())
                .into_iter()
                .map(|g| g.actor)
                .collect::<Vec<_>>()
        };
        let alone = actors(start(&[("slingshot", 1)]));
        assert_eq!(
            alone,
            [item::RANGE_SLING_SHOT.get(), item::BULLET_BAG_SMALL.get()]
        );
        let bagged = actors(start(&[
            ("slingshot", 1),
            ("bullet-bag", 1),
            ("big-bullet-bag", 1),
        ]));
        let bags = bagged
            .iter()
            .filter(|a| *a == item::BULLET_BAG_SMALL.get())
            .count();
        assert_eq!(bags, 1);
        assert!(bagged.contains(&item::BULLET_BAG_MEDIUM.to_string()));
    }

    #[test]
    fn hearts_are_three_and_each_container_and_each_four_pieces() {
        assert_eq!(Start::default().hearts(), 3);
        assert_eq!(start(&[(HEARTS, 2), (PIECES, 7)]).hearts(), 6);
        assert_eq!(Start::of(&[ARSENAL]).hearts(), 20);
    }

    #[test]
    fn an_old_heart_count_becomes_containers_and_pieces() {
        let up = |old: serde_json::Value| {
            let mut v = old;
            upgrade(&mut v);
            v
        };
        let twenty = serde_json::json!({ "heart-containers": 8, "heart-pieces": 36 });
        assert_eq!(up(serde_json::json!({ "heart-containers": 20 })), twenty);
        let twelve = serde_json::json!({ "heart-containers": 8, "heart-pieces": 4 });
        assert_eq!(up(serde_json::json!({ "heart-containers": 12 })), twelve);
        let now = serde_json::json!({ "heart-containers": 5 });
        assert_eq!(up(now.clone()), now);
    }

    #[test]
    fn settling_fits_the_start_to_the_seed_s_bags() {
        use crate::settings::{Bombchus, NewGame};
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let table = Pouch::load(&romfs).unwrap();
        let settle = |new_game: NewGame, words: &[(&str, i32)]| {
            let mut s = start(words);
            let notes = s.settle(&table.clone().seeded(&new_game));
            (s.0, notes)
        };
        let bags = || NewGame {
            bombchus: Bombchus::Bag,
            ..Default::default()
        };
        // Unshuffled bags come with every new game.
        let (held, notes) = settle(NewGame::default(), &[]);
        assert_eq!(held, start(&[("nut-bag", 1), ("stick-bag", 1)]).0);
        assert!(notes.is_empty());
        // Free bombchus hold 40 with no bag, and no Bombchu Bag is dealt.
        let (held, notes) = settle(NewGame::default(), &[("bombchus", 50), ("bombchu-bag", 1)]);
        assert_eq!(held.get("bombchus"), Some(&40));
        assert!(!held.contains_key("bombchu-bag"));
        assert_eq!(notes, ["bombchu-bag: 1 -> 0", "bombchus: 50 -> 40"]);
        // With Bombchu Bags, none fit before the first, and the bag's size after.
        let (held, notes) = settle(bags(), &[("bombchus", 20)]);
        assert!(!held.contains_key("bombchus"));
        assert_eq!(notes, ["bombchus: 20 -> 0"]);
        let (held, _) = settle(
            bags(),
            &[("bombchus", 40), ("bombchu-bag", 1), ("big-bombchu-bag", 1)],
        );
        assert_eq!(held.get("bombchus"), Some(&30));
        // A shuffled bag doesn't come with the game: nuts need one.
        let shuffled = NewGame {
            shuffle_nut_bag: true,
            ..Default::default()
        };
        let (held, _) = settle(shuffled, &[("deku-nuts", 20)]);
        assert!(!held.contains_key("nut-bag") && !held.contains_key("deku-nuts"));
        assert_eq!(held.get("stick-bag"), Some(&1));
    }

    #[test]
    fn a_starting_bombchu_bag_leaves_the_pool_and_is_granted() {
        needs_names!();
        let s = start(&[("bombchu-bag", 1), ("big-bombchu-bag", 1)]);
        let bags = [bombchu_bags::SMALL.get(), bombchu_bags::MEDIUM.get()];
        assert_eq!(s.owned(), bags);
        let granted: Vec<_> = s
            .grants(&Pouch::default())
            .into_iter()
            .map(|g| g.actor)
            .collect();
        assert_eq!(granted, bags);
    }

    #[test]
    fn owned_counts_each_unit_but_no_ammo() {
        needs_names!();
        let s = start(&[
            ("bombs", 20),
            ("bomb-bag", 1),
            ("heart-pieces", 3),
            (HEARTS, 2),
            ("bottle-1", 1),
            ("bottle-2", 1),
        ]);
        let mut owned = s.owned();
        owned.sort();
        let mut want = vec![
            item::BOMB_BAG_SMALL.get(),
            item::BOTTLE.get(),
            item::BOTTLE.get(),
        ];
        want.extend([actor::OBJ_HEART_PIECE.get(); 3]);
        want.extend([actor::OBJ_HEART_CONTAINER.get(); 2]);
        want.sort();
        assert_eq!(owned, want);
    }

    #[test]
    fn rupees_are_a_default_by_hash() {
        let s = start(&[(HEARTS, 5), (RUPEE_WORD, 99)]);
        assert_eq!(s.defaults(), [format!("#{RUPEES:08x}=99")]);
        assert_eq!(crate::savefile::hash_of(&format!("#{RUPEES:08x}")), RUPEES);
    }

    #[test]
    fn a_starting_nut_or_stick_bag_marks_its_ammo_got() {
        let got = |a: &Name| format!("IsGet.{a}=true");
        assert!(
            start(&[("nut-bag", 1)])
                .defaults()
                .contains(&got(item::DEKU_NUT))
        );
        let big = start(&[("big-stick-bag", 1)]).defaults();
        assert!(big.contains(&got(item::DEKU_STICK)));
        assert!(!big.contains(&got(item::DEKU_NUT)));
        assert!(start(&[("bomb-bag", 1)]).defaults().is_empty());
    }

    #[test]
    fn grid_drops_are_pouch_items_a_grant_can_give() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for h in inventory::held() {
            let d = drop(h.actor);
            if ALWAYS.contains(&d) || crate::known::is_ours(d) {
                continue;
            }
            assert!(crate::items::get(&romfs, d).unwrap().is_some(), "{d}");
        }
    }
}
