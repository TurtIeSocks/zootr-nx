//! What a new game starts holding: the Inventory panel's grid, the save
//! editor's own, run on a fresh save. The setting keeps it as words
//! (`hookshot: 1`, `bombs: 20`), so neither it nor `--start` needs the
//! game's names, and an old config upgrades before any dump is open.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use ounce_formats::Save;
use serde::{Deserialize, Serialize};

use crate::gamedata::GameData;
use crate::inventory::{self, Held, Intent, Inventory, Pouch};
use crate::known::{Name, actor, field, item};
use crate::progressive;
use crate::save_editor::{self, ItemsView, LIFE, MAX_LIFE, Opened, RUPEES};
use crate::savefile::{self, Edit};
use crate::settings::Settings;
use crate::value;

/// Its setting's id.
pub const SETTING: &str = "start-inventory";

/// What a new game starts with while the setting is unset.
pub const DEFAULT: [&str; 1] = ["master-sword"];

/// The `--start` word for starting with nothing, not [`DEFAULT`].
pub const NONE: &str = "none";

/// The `--start` word for all of Link's gear at its top tier, full ammo,
/// every song and twenty hearts: the kit room logic is measured with.
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
    ("biggest-bomb-bag", 1), ("bombs", 40), ("bombchus", 20),
    ("biggest-stick-bag", 1), ("deku-sticks", 30), ("biggest-nut-bag", 1), ("deku-nuts", 40),
    ("longshot", 1), ("boomerang", 1), ("megaton-hammer", 1), ("lens-of-truth", 1),
    ("dins-fire", 1), ("farores-wind", 1), ("nayrus-love", 1), ("double-magic", 1),
    ("ocarina-of-time", 1), ("magic-beans", 10),
    ("bottle-1", 1), ("bottle-2", 1), ("bottle-3", 1), ("bottle-4", 1),
    ("bunny-hood", 1), ("mask-of-truth", 1),
    ("stone-of-agony", 1), ("gerudo-card", 1), ("double-defense", 1),
    ("giants-wallet", 1), ("rupees", 500), ("heart-containers", 20),
    ("zeldas-lullaby", 1), ("eponas-song", 1), ("sarias-song", 1), ("suns-song", 1),
    ("song-of-time", 1), ("song-of-storms", 1), ("minuet-of-forest", 1), ("bolero-of-fire", 1),
    ("serenade-of-water", 1), ("requiem-of-spirit", 1), ("nocturne-of-shadow", 1), ("prelude-of-light", 1),
];

/// The ammo a weapon's grant brings with it, which no remove takes back.
const BROUGHT: [(&Name, &Name, i32); 2] = [
    (actor::WEAPON_RANGE_BOW, item::ARROW_NORMAL, 30),
    (actor::WEAPON_RANGE_SLING_SHOT, item::DEKU_SEED, 30),
];

/// Heart containers, whole hearts.
const HEARTS: &str = "heart-containers";
/// The heart containers every new game has. The rest are granted.
const BASE_HEARTS: i32 = 3;
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
    ("bombchus", item::BOMBCHU),
    ("hookshot", item::HOOKSHOT), ("longshot", item::LONGSHOT),
    ("ice-arrows", item::ARROW_ICE), ("farores-wind", item::FARORE_WIND),
    ("boomerang", item::BOOMERANG), ("lens-of-truth", actor::ARMOR_GLASS_TRUTH),
    ("magic-beans", item::MAGIC_BEAN), ("megaton-hammer", item::HAMMER),
    ("light-arrows", item::ARROW_LIGHT), ("nayrus-love", item::NAYRU_LOVE),
    ("bottle-1", field::BOTTLE_A), ("bottle-2", field::BOTTLE_B), ("bottle-3", field::BOTTLE_C), ("bottle-4", field::BOTTLE_D),
    ("weird-egg", item::WEIRD_EGG), ("cucco", item::ALARM_CUCCO), ("zeldas-letter", item::ZELDA_LETTER),
    ("keaton-mask", actor::ARMOR_CHILD_MASK_KEATON), ("skull-mask", actor::ARMOR_CHILD_MASK_SKULL),
    ("spooky-mask", actor::ARMOR_CHILD_MASK_SPOOKY), ("bunny-hood", actor::ARMOR_CHILD_MASK_BUNNY),
    ("mask-of-truth", actor::ARMOR_CHILD_MASK_TRUTH),
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
    ("magic-meter", crate::magic::meter::METER), ("double-magic", item::FAIRY_BLESS_MP_DOUBLE),
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
    (field::BOTTLE_A, item::BOTTLE),
    (field::BOTTLE_B, item::BOTTLE),
    (field::BOTTLE_C, item::BOTTLE),
    (field::BOTTLE_D, item::BOTTLE),
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

    /// Fails on a word that isn't an item, a heart count or rupees.
    pub fn check(&self) -> Result<()> {
        let known = |w: &str| w == HEARTS || w == RUPEE_WORD || WORDS.iter().any(|(k, _)| *k == w);
        if let Some(w) = self.0.keys().find(|w| !known(w)) {
            bail!(
                "a new game can't start with {w}. `--start` takes {}",
                words()
            );
        }
        Ok(())
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
        if word == HEARTS || word == RUPEE_WORD {
            self.0.insert(word.into(), n);
            return Ok(());
        }
        let &(_, actor) = (WORDS.iter().find(|(w, _)| *w == word)).with_context(|| {
            format!(
                "a new game can't start with {word}. `--start` takes {}",
                words()
            )
        })?;
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

    /// How many of grid actor `a` it holds.
    fn count(&self, a: &Name) -> i32 {
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
    /// bring. Then the heart containers past a new game's, one grant each: a
    /// heart container's grant adds one whatever its count.
    pub fn grants(&self, pouch: &Pouch) -> Vec<Grant> {
        let mut out = self.items();
        for (h, n) in self.held().into_iter().filter(|(h, _)| h.ammo) {
            let n = n - self.brought(h.actor, pouch);
            out.extend((n > 0).then(|| Grant::new(h.actor.get(), n)));
        }
        let hearts = self.0.get(HEARTS).copied().unwrap_or(BASE_HEARTS);
        let container = || Grant::new(actor::OBJ_HEART_CONTAINER.get(), 1);
        out.extend((BASE_HEARTS..hearts).map(|_| container()));
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

    /// What Link owns from the start, once per unit: what leaves the pool
    /// and what logic counts. Ammo is neither.
    pub fn owned(&self) -> Vec<&'static str> {
        (self.held().into_iter())
            .filter(|(h, _)| !h.ammo)
            .flat_map(|(h, n)| std::iter::repeat_n(drop(h.actor).get(), n as usize))
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

    /// The Status edits the panel's save takes: heart containers, with
    /// health full, and the rupees.
    fn status(&self) -> Vec<Edit> {
        let hearts = self
            .0
            .get(HEARTS)
            .map(|&h| [(MAX_LIFE, h * 4), (LIFE, h * 4)]);
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
    let all = (WORDS.iter().map(|(w, _)| *w)).chain([HEARTS, RUPEE_WORD, NONE, ARSENAL]);
    all.collect::<Vec<_>>().join(", ")
}

fn word_of(actor: &Name) -> Result<&'static str> {
    let w = WORDS.iter().find(|(_, a)| *a == actor);
    w.map(|(w, _)| *w)
        .with_context(|| format!("{actor} has no word"))
}

/// A new game's save, as far as the grid and the kept Status fields go:
/// every value they read, at its GameData default.
fn fresh(gd: &GameData, inv: &Inventory, pouch: &Pouch) -> Result<Save> {
    let mut seen = HashSet::new();
    let hashes = (inv.cell_hashes(pouch).into_iter().flatten()).chain([MAX_LIFE, LIFE, RUPEES]);
    let mut entries = Vec::new();
    for hash in hashes.filter(|h| seen.insert(*h)) {
        let (Some(kind), Some(entry)) = (gd.kind(hash), gd.entry(hash)) else {
            continue;
        };
        let Some(default) = entry.as_map()?.get(&**field::DEFAULT_VALUE) else {
            continue;
        };
        let Some(width) = ounce_formats::save_width(kind) else {
            continue;
        };
        let bytes = match (savefile::raw(default), default.as_string()) {
            (Some(raw), _) => raw.to_le_bytes().to_vec(),
            (None, Ok(text)) => value::encode(kind, text, width)?,
            _ => continue,
        };
        entries.push((hash, kind, bytes));
    }
    Save::build(&entries)
}

/// The editor's view of a new game holding `start`: a fresh save as
/// `original`, and it holding `start` as `edited`.
pub fn open(gd: &GameData, inv: &Inventory, pouch: &Pouch, start: &Start) -> Result<Opened> {
    let original = fresh(gd, inv, pouch)?;
    let mut edited = Save::parse(original.bytes().to_vec())?;
    let start = &start.clone().floored(pouch);
    let held: Vec<(&Name, i32)> = (start.held().into_iter())
        .map(|(h, n)| (h.actor, n))
        .collect();
    savefile::apply(gd, &mut edited, &inv.holding(pouch, &held)?)?;
    savefile::apply(gd, &mut edited, &start.status())?;
    Ok(Opened {
        path: PathBuf::new(),
        stamp: 0,
        original,
        edited,
    })
}

/// The Status fields the panel keeps: the rest follow from the grid.
const STATUS: [u32; 2] = [MAX_LIFE, RUPEES];

/// The Inventory panel: the grid without its Dungeons, whose keys belong to
/// the Dungeon keys setting, and the kept Status fields.
pub fn items(gd: &GameData, inv: &Inventory, pouch: &Pouch, start: &Start) -> Result<ItemsView> {
    let mut view = save_editor::items(gd, inv, pouch, &open(gd, inv, pouch, start)?);
    (view.sections.iter_mut())
        .filter(|p| p.section == inventory::Section::Dungeons)
        .for_each(|p| p.shown = false);
    view.status.retain(|f| STATUS.contains(&f.hash));
    Ok(view)
}

/// `start` after a click on cell `at`.
pub fn click(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    at: usize,
    intent: Intent,
) -> Result<Start> {
    let opened = open(gd, inv, pouch, start)?;
    let edits = save_editor::click(inv, pouch, &opened, at, intent)?;
    reread(gd, inv, pouch, opened, &edits)
}

/// `start` after a Status field takes `edit`. Health follows the heart
/// containers.
pub fn set(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    edit: Edit,
) -> Result<Start> {
    if !STATUS.contains(&edit.hash) {
        bail!("a new game only starts with heart containers and rupees set");
    }
    let opened = open(gd, inv, pouch, start)?;
    reread(gd, inv, pouch, opened, &[edit])
}

/// What `opened` holds after `edits`, as words: only what differs from a
/// fresh save.
fn reread(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    mut opened: Opened,
    edits: &[Edit],
) -> Result<Start> {
    savefile::apply(gd, &mut opened.edited, edits)?;
    let read = |save: &Save| {
        let mut inv = inv.clone();
        inv.load(inventory::in_save(save));
        inv.values()
    };
    let (was, now) = (read(&opened.original), read(&opened.edited));
    let mut out = BTreeMap::new();
    for (w, a) in WORDS {
        let n = now.get(a).copied().unwrap_or(0);
        if n > was.get(a).copied().unwrap_or(0) {
            out.insert((*w).to_owned(), n);
        }
    }
    let int = |save: &Save, hash| {
        save.get(hash)
            .and_then(|e| save.scalar(e))
            .map(|r| r as i32)
    };
    for (word, hash, per) in [(HEARTS, MAX_LIFE, 4), (RUPEE_WORD, RUPEES, 1)] {
        let n = int(&opened.edited, hash);
        if n.is_some() && n != int(&opened.original, hash) {
            out.insert(word.into(), n.unwrap_or(0) / per);
        }
    }
    Ok(Start(out).floored(pouch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(words: &[(&str, i32)]) -> Start {
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
            (HEARTS, 5),
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
            (item::BOMB.get(), 20, false),
            (actor::OBJ_HEART_CONTAINER.get(), 1, false),
            (actor::OBJ_HEART_CONTAINER.get(), 1, false),
        ];
        assert_eq!(got, want.map(|(a, n, e)| (a.to_owned(), n, e)));
        let none = start(&[(HEARTS, 3)]).grants(&Pouch::default());
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
    fn owned_counts_each_unit_but_no_ammo() {
        needs_names!();
        let s = start(&[
            ("bombs", 20),
            ("bomb-bag", 1),
            ("heart-pieces", 3),
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
        want.sort();
        assert_eq!(owned, want);
    }

    #[test]
    fn rupees_are_a_default_by_hash() {
        let s = start(&[(HEARTS, 5), (RUPEE_WORD, 99)]);
        assert_eq!(s.defaults(), [format!("#{RUPEES:08x}=99")]);
        assert_eq!(savefile::hash_of(&format!("#{RUPEES:08x}")), RUPEES);
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

    /// The grid and pouch table from the dump, with GameData.
    fn layout() -> Option<(GameData, Inventory, Pouch)> {
        let romfs = crate::game::dev_dump()?;
        let gd = GameData::load(&romfs).unwrap();
        let inv = Inventory::new(&gd, &romfs).unwrap();
        Some((gd, inv, Pouch::load(&romfs).unwrap()))
    }

    #[test]
    fn clicks_on_a_fresh_save_come_back_as_words() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let cells = inv.cells();
        let at = |label: &str| cells.iter().position(|c| c.label == label).unwrap();
        let once = click(
            &gd,
            &inv,
            &pouch,
            &Start::default(),
            at("Hookshot"),
            Intent::Up,
        )
        .unwrap();
        assert_eq!(once, start(&[("hookshot", 1)]));
        let twice = click(&gd, &inv, &pouch, &once, at("Hookshot"), Intent::Up).unwrap();
        assert_eq!(twice, start(&[("hookshot", 1), ("longshot", 1)]));
        let back = click(&gd, &inv, &pouch, &twice, at("Hookshot"), Intent::Up).unwrap();
        assert_eq!(back, Start::default());
        let view = items(&gd, &inv, &pouch, &twice).unwrap();
        assert!(view.staged[at("Hookshot")]);
        let fields: Vec<_> = view.status.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(fields, ["Heart containers", "Rupees"]);
        let dungeons = view
            .sections
            .iter()
            .find(|p| p.section == inventory::Section::Dungeons);
        assert!(!dungeons.unwrap().shown);
    }

    #[test]
    fn status_edits_come_back_as_hearts_and_rupees() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let s = set(
            &gd,
            &inv,
            &pouch,
            &Start::default(),
            Edit::new(MAX_LIFE, "20"),
        )
        .unwrap();
        assert_eq!(s, start(&[(HEARTS, 5)]));
        let s = set(&gd, &inv, &pouch, &s, Edit::new(RUPEES, "50")).unwrap();
        assert_eq!(s, start(&[(HEARTS, 5), (RUPEE_WORD, 50)]));
        assert!(set(&gd, &inv, &pouch, &s, Edit::new(LIFE, "4")).is_err());
    }
}
