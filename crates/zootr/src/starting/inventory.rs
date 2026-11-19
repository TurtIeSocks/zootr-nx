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

/// Heart containers, whole hearts. Health starts full.
const HEARTS: &str = "heart-containers";
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
    ("child-wallet", item::WALLET_SMALL), ("adults-wallet", item::WALLET_MEDIUM), ("giants-wallet", item::WALLET_LARGE),
    ("quiver", item::QUIVER_SMALL), ("big-quiver", item::QUIVER_MEDIUM), ("biggest-quiver", item::QUIVER_LARGE),
    ("bullet-bag", item::BULLET_BAG_SMALL), ("big-bullet-bag", item::BULLET_BAG_MEDIUM), ("biggest-bullet-bag", item::BULLET_BAG_LARGE),
    ("heart-pieces", actor::OBJ_HEART_PIECE),
    ("magic-meter", item::FAIRY_BLESS_MP), ("double-magic", item::FAIRY_BLESS_MP_DOUBLE),
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

/// What a new game starts holding, by word.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent, rename = "StartInventory")]
pub struct Start(pub BTreeMap<String, i32>);

impl Start {
    /// The setting's value, empty when unset.
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let start: Self = match settings.get(SETTING) {
            Some(v) => {
                serde_json::from_value(v.clone()).context("reading the starting inventory")?
            }
            None => Self::default(),
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

    /// `word` held `n` times: every tier below it too, as a click does.
    pub fn add(&mut self, word: &str, n: i32) -> Result<()> {
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
        for lower in inventory::below(actor) {
            let w = word_of(lower)?;
            let held = self.0.entry(w.into()).or_default();
            *held = (*held).max(1);
        }
        self.0.insert(word.into(), n);
        Ok(())
    }

    /// Each grid actor held and its count, in grid order.
    fn held(&self) -> Vec<(Held, i32)> {
        let count = |a: &Name| {
            let w = WORDS.iter().find(|(_, n)| *n == a).map(|(w, _)| *w);
            w.and_then(|w| self.0.get(w)).copied().unwrap_or(0)
        };
        (inventory::held().into_iter())
            .map(|h| (h, count(h.actor)))
            .filter(|&(_, n)| n > 0)
            .collect()
    }

    /// The opening's grants, in grid order so a bag comes before its ammo
    /// and the Hookshot before the Longshot. What comes with an item comes
    /// only when the grid doesn't hold it already.
    pub fn grants(&self) -> Vec<Grant> {
        let held = self.held();
        let drops: Vec<&str> = held.iter().map(|(h, _)| drop(h.actor).get()).collect();
        let mut out = Vec::new();
        for ((_, n), d) in held.iter().zip(&drops) {
            let mut grants = granted(d, *n).into_iter();
            out.extend(grants.next());
            out.extend(grants.filter(|w| !drops.contains(&w.actor.as_str())));
        }
        out
    }

    /// What Link owns from the start, once per unit: what leaves the pool
    /// and what logic counts. Ammo is neither.
    pub fn owned(&self) -> Vec<&'static str> {
        (self.held().into_iter())
            .filter(|(h, _)| !h.ammo)
            .flat_map(|(h, n)| std::iter::repeat_n(drop(h.actor).get(), n as usize))
            .collect()
    }

    /// GameData default edits: heart containers, with health full, and
    /// rupees.
    pub fn defaults(&self) -> Vec<String> {
        let set = |hash: u32, n: i32| format!("#{hash:08x}={n}");
        let mut out = Vec::new();
        if let Some(&hearts) = self.0.get(HEARTS) {
            out.extend([set(MAX_LIFE, hearts * 4), set(LIFE, hearts * 4)]);
        }
        if let Some(&rupees) = self.0.get(RUPEE_WORD) {
            out.push(set(RUPEES, rupees));
        }
        out
    }
}

/// `--start`'s `WORD[=N]`: a word it takes, and how many, 1 unless said.
pub fn parse(arg: &str) -> Result<(String, i32)> {
    let (word, n) = match arg.split_once('=') {
        Some((w, n)) => (w, n.parse().with_context(|| format!("{n} isn't a count"))?),
        None => (arg, 1),
    };
    Start::default().add(word, n)?;
    Ok((word.to_owned(), n))
}

/// Every word `--start` takes.
pub fn words() -> String {
    let all = WORDS.iter().map(|(w, _)| *w).chain([HEARTS, RUPEE_WORD]);
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
    let held: Vec<(&Name, i32)> = (start.held().into_iter())
        .map(|(h, n)| (h.actor, n))
        .collect();
    savefile::apply(gd, &mut edited, &inv.holding(pouch, &held)?)?;
    let status = (start.defaults().iter())
        .map(|e| Edit::parse(e))
        .collect::<Result<Vec<_>>>()?;
    savefile::apply(gd, &mut edited, &status)?;
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
    reread(gd, inv, opened, &edits)
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
    reread(gd, inv, opened, &[edit])
}

/// What `opened` holds after `edits`, as words: only what differs from a
/// fresh save.
fn reread(gd: &GameData, inv: &Inventory, mut opened: Opened, edits: &[Edit]) -> Result<Start> {
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
    Ok(Start(out))
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
            assert_eq!(n, 1, "{:?}", h.actor);
        }
        let grid: Vec<_> = inventory::held().into_iter().map(|h| h.actor).collect();
        for (w, a) in WORDS {
            assert!(grid.contains(a), "{w}");
            assert_eq!(WORDS.iter().filter(|(o, _)| o == w).count(), 1, "{w}");
        }
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
            Start::default()
        );
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
    fn grants_follow_the_grid_with_bags_before_their_ammo() {
        needs_names!();
        let s = start(&[
            ("bombs", 20),
            ("bomb-bag", 1),
            ("longshot", 1),
            ("hookshot", 1),
            ("kokiri-sword", 1),
        ]);
        let got: Vec<_> = s
            .grants()
            .into_iter()
            .map(|g| (g.actor, g.num, g.equip))
            .collect();
        let want = [
            (item::BOMB_BAG_SMALL.get(), 1, false),
            (item::BOMB.get(), 20, false),
            (item::HOOKSHOT.get(), 1, false),
            (item::LONGSHOT.get(), 1, false),
            (item::SWORD_KOKIRI.get(), 1, true),
        ];
        assert_eq!(got, want.map(|(a, n, e)| (a.to_owned(), n, e)));
    }

    #[test]
    fn the_slingshot_brings_its_bag_unless_the_grid_has_one() {
        needs_names!();
        let actors = |s: Start| s.grants().into_iter().map(|g| g.actor).collect::<Vec<_>>();
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
    fn hearts_and_rupees_are_defaults_by_hash() {
        let s = start(&[(HEARTS, 5), (RUPEE_WORD, 99)]);
        assert_eq!(
            s.defaults(),
            [
                format!("#{MAX_LIFE:08x}=20"),
                format!("#{LIFE:08x}=20"),
                format!("#{RUPEES:08x}=99"),
            ]
        );
        assert_eq!(savefile::hash_of(&format!("#{RUPEES:08x}")), RUPEES);
    }

    #[test]
    fn grid_drops_are_pouch_items_a_grant_can_give() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for h in inventory::held() {
            let d = drop(h.actor);
            if d == item::WALLET_SMALL {
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
