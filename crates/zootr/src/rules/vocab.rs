//! The words the rule files use for the game's own names. Each word maps
//! to a `known` const, so no rule file spells a name out: `item:kokiri-sword`
//! is an item, `beat:gohma` a boss defeat, and a place is its node's word.
//! A word the table lacks is an error naming the word.
//!
//! Words are lowercase and hyphenated, as a player would say them. A place
//! keeps its era suffix, so the graph has the nodes the game's data does.

use anyhow::{Result, bail};

use super::Expr;
use super::item::{BEAT, ITEM_PREFIXES};
use crate::known::{Name, actor, field, item, map};

/// `item:<word>`: an actor Link holds, or a pickup that stands for one.
pub const ITEMS: &[(&str, &Name)] = &[
    ("bolero-of-fire", item::MELODY_FIRE_BOLERO),
    ("bomb-bag", item::BOMB_BAG_SMALL),
    ("bombchu-bag", crate::bombchu_bags::SMALL),
    ("bombchus-large", item::BOMBCHU_BUNDLE_LARGE),
    ("bombchus-medium", item::BOMBCHU_BUNDLE_MEDIUM),
    ("bombchus-small", item::BOMBCHU_BUNDLE_SMALL),
    ("biggorons-sword", item::SWORD_BIGGORON),
    ("boomerang", item::BOOMERANG),
    ("bottle", item::BOTTLE),
    ("bow", item::RANGE_BOW),
    ("broken-sword", item::BROKEN_SWORD),
    ("claim-check", item::CLAIM_CHECK),
    ("cojiro", item::COJIRO_CUCCO),
    ("deku-shield-chest", actor::T_BOX_SHIELD_DEKU),
    ("dins-fire", item::DIN_FIRE),
    ("eponas-song", item::MELODY_EPONA_SONG),
    ("eye-drops", item::EYE_DROP),
    ("eyeball-frog", item::EYEBALL_FROG),
    ("fairy-ocarina", item::OCARINA_FAIRY),
    ("fire-arrows", item::ARROW_FIRE),
    ("fire-medallion", item::MEDAL_FIRE),
    ("forest-medallion", item::MEDAL_FOREST),
    ("gerudo-card", item::TOKEN_GERUDO),
    ("gold-skulltula-token", item::TOKEN_GOLD),
    ("golden-gauntlets", item::GAUNTLET_GOLD),
    ("goron-bracelet", item::GAUNTLET_GORON),
    ("goron-ruby", item::STONE_FIRE),
    ("goron-tunic", actor::ARMOR_ADULT_GORON),
    ("goron-tunic-pickup", actor::OBJ_ARMOR_GORON),
    ("hookshot", item::HOOKSHOT),
    ("hover-boots", item::HOVER_BOOT),
    ("hylian-shield-chest", actor::T_BOX_SHIELD_HYLIAN),
    ("iron-boots", item::HEAVY_BOOT),
    ("kokiri-emerald", item::STONE_FOREST),
    ("kokiri-sword", item::SWORD_KOKIRI),
    ("lens-of-truth", item::GLASS_TRUTH),
    ("light-arrows", item::ARROW_LIGHT),
    ("light-medallion", item::MEDAL_LIGHT),
    ("longshot", item::LONGSHOT),
    ("magic-bean", item::MAGIC_BEAN),
    ("magic-meter", item::FAIRY_BLESS_MP),
    ("master-sword", actor::WEAPON_SWORD_MASTER),
    ("megaton-hammer", item::HAMMER),
    ("milk", item::BOTTLE_MILK),
    ("minuet-of-forest", item::MELODY_FOREST_MINUET),
    ("mirror-shield", item::SHIELD_MIRROR),
    ("nocturne-of-shadow", item::MELODY_SHADOW_NOCTURNE),
    ("ocarina-of-time", item::OCARINA_TIME),
    ("odd-mushroom", item::ODD_MUSHROOM),
    ("odd-powder", item::ODD_POWDER),
    ("poachers-saw", item::POACHER_SAW),
    ("pocket-egg", item::POCKET_EGG),
    ("prelude-of-light", item::MELODY_LIGHT_PRELUDE),
    ("prescription", item::PRESCRIPTION),
    ("requiem-of-spirit", item::MELODY_SPIRIT_REQUIEM),
    ("rutos-letter", item::BOTTLE_LETTER),
    ("sarias-song", item::MELODY_SARIA_SONG),
    ("scarecrows-song", item::MELODY_SCARECROW_SONG),
    ("serenade-of-water", item::MELODY_WATER_SERENADE),
    ("shadow-medallion", item::MEDAL_SHADOW),
    ("silver-gauntlets", item::GAUNTLET_SILVER),
    ("silver-scale", item::SCALE_SILVER),
    ("slingshot", item::RANGE_SLING_SHOT),
    ("song-of-storms", item::MELODY_STORM_SONG),
    ("song-of-time", item::MELODY_TIME_SONG),
    ("spirit-medallion", item::MEDAL_SPIRIT),
    ("stick-bag", item::STICK_BAG_SMALL),
    ("suns-song", item::MELODY_SUN_SONG),
    ("water-medallion", item::MEDAL_WATER),
    ("weird-egg", item::WEIRD_EGG),
    ("zeldas-letter", item::ZELDA_LETTER),
    ("zeldas-lullaby", item::MELODY_ZELDA_LULLABY),
    ("zora-sapphire", item::STONE_WATER),
    ("zora-tunic", actor::ARMOR_ADULT_ZORA),
    ("zora-tunic-pickup", actor::OBJ_ARMOR_ZORA),
];

/// `beat:<word>`: a boss defeat, held like an item. The const names the
/// boss, and the item is `BEAT` and that name.
pub const BEATS: &[(&str, &Name)] = &[
    ("barinade", field::BARINADE),
    ("bongo-bongo", field::BONGO_BONGO),
    ("gohma", field::GOHMA),
    ("king-dodongo", field::KING_DODONGO),
    ("morpha", field::MORPHA),
    ("phantom-ganon", field::PHANTOM_GANON),
    ("twinrova", field::TWINROVA),
    ("volvagia", field::VOLVAGIA),
];

/// A place node. A word with no era is a dungeon or a boss room.
pub const PLACES: &[(&str, &Name)] = &[
    ("barinade-room", map::BARINADE_STAGE),
    ("bongo-bongo-room", map::BONGO_BONGO_STAGE),
    ("bottom-of-the-well", map::BOTTOM_OF_THE_WELL),
    (
        "death-mountain-crater-adult",
        map::DEATH_MOUNTAIN_CRATER_ADULT,
    ),
    (
        "death-mountain-crater-child",
        map::DEATH_MOUNTAIN_CRATER_CHILD,
    ),
    (
        "death-mountain-trail-adult",
        map::DEATH_MOUNTAIN_TRAIL_ADULT,
    ),
    (
        "death-mountain-trail-child",
        map::DEATH_MOUNTAIN_TRAIL_CHILD,
    ),
    ("deku-tree", map::INSIDE_THE_DEKU_TREE),
    ("desert-colossus-adult", map::DESERT_COLOSSUS_ADULT),
    ("desert-colossus-child", map::DESERT_COLOSSUS_CHILD),
    ("dodongos-cavern", map::DODONGO_CAVERN),
    ("fire-temple", map::FIRE_TEMPLE),
    ("forest-temple", map::FOREST_TEMPLE),
    ("ganondorf-room", map::GANONDORF_STAGE),
    ("ganons-castle", map::GANON_CASTLE),
    ("gerudo-fortress-adult", map::GERUDO_FORTRESS_ADULT),
    ("gerudo-training-ground", map::GERUDO_TRAINING_GROUND),
    ("gerudo-valley-adult", map::GERUDO_VALLEY_ADULT),
    ("gohma-room", map::GOHMA_STAGE),
    ("goron-city-adult", map::GORON_CITY_ADULT),
    ("goron-city-child", map::GORON_CITY_CHILD),
    ("graveyard-adult", map::GRAVEYARD_ADULT),
    ("graveyard-child", map::GRAVEYARD_CHILD),
    ("haunted-wasteland-adult", map::HOUNTED_WASTELAND_ADULT),
    ("haunted-wasteland-child", map::HOUNTED_WASTELAND_CHILD),
    ("hyrule-castle", map::HYRULE_CASTLE),
    ("hyrule-field-adult", map::HYRULE_FIELD_ADULT),
    ("hyrule-field-child", map::HYRULE_FIELD_CHILD),
    ("ice-cavern", map::ICE_CAVERN),
    ("inside-ganons-castle", map::INSIDE_GANON_CASTLE),
    ("jabu-jabu", map::INSIDE_JABU_JABU_BELLY),
    ("kakariko-village-adult", map::KAKARIKO_VILLAGE_ADULT),
    (
        "kakariko-village-adult-alternative",
        map::KAKARIKO_VILLAGE_ADULT_ALTERNATIVE,
    ),
    ("kakariko-village-child", map::KAKARIKO_VILLAGE_CHILD),
    ("king-dodongo-room", map::DODONGO_STAGE),
    ("kokiri-forest-adult", map::KOKIRI_FOREST_ADULT),
    ("kokiri-forest-child", map::KOKIRI_FOREST_CHILD),
    ("lost-woods-adult", map::LOST_WOOD_ADULT),
    ("lost-woods-child", map::LOST_WOOD_CHILD),
    ("market-adult", map::MARKET_ADULT),
    ("market-child", map::MARKET_CHILD),
    ("morpha-room", map::MORPHA_STAGE),
    ("phantom-ganon-room", map::PHANTOM_GANON_STAGE),
    (
        "sacred-forest-meadow-adult",
        map::SACRED_FOREST_MEADOW_ADULT,
    ),
    (
        "sacred-forest-meadow-child",
        map::SACRED_FOREST_MEADOW_CHILD,
    ),
    ("shadow-temple", map::SHADOW_TEMPLE),
    ("spirit-temple", map::SPIRIT_TEMPLE),
    ("twinrova-room", map::TWINROVA_STAGE),
    ("volvagia-room", map::VOLVAGIA_STAGE),
    ("water-temple", map::WATER_TEMPLE),
    ("zora-domain-adult", map::ZORA_DOMAIN_ADULT),
    (
        "zora-domain-adult-alternative",
        map::ZORA_DOMAIN_ADULT_ALTERNATIVE,
    ),
    ("zora-domain-child", map::ZORA_DOMAIN_CHILD),
    ("zora-fountain-adult", map::ZORA_FOUNTAIN_ADULT),
    ("zora-fountain-child", map::ZORA_FOUNTAIN_CHILD),
    ("zora-river-adult", map::ZORA_RIVER_ADULT),
    ("zora-river-child", map::ZORA_RIVER_CHILD),
];

fn find(table: &[(&str, &'static Name)], word: &str) -> Option<&'static Name> {
    table.iter().find(|(w, _)| *w == word).map(|(_, n)| *n)
}

/// The game's name for a place word, as the world graph spells its node.
pub fn node(word: &str) -> Result<String> {
    match find(PLACES, word) {
        Some(name) => Ok(name.get().to_owned()),
        None => bail!("unknown place `{word}`"),
    }
}

/// The place word for a node, if the table has one.
pub fn place_word(node: &str) -> Option<&'static str> {
    PLACES.iter().find(|(_, n)| **n == *node).map(|(w, _)| *w)
}

/// What an `item:` or `beat:` word stands for, any `*n` count kept. The
/// item behind a beat is `BEAT` and the boss's name.
pub fn id(word: &str) -> Result<String> {
    let (base, count) = match word.split_once('*') {
        Some((base, n)) => (base, format!("*{n}")),
        None => (word, String::new()),
    };
    let id = match base.split_once(':') {
        Some(("item", w)) => find(ITEMS, w).map(|n| n.get().to_owned()),
        Some(("beat", w)) => find(BEATS, w).map(|n| format!("{BEAT}{}", n.get())),
        _ => None,
    };
    match id {
        Some(id) => Ok(format!("{id}{count}")),
        None => bail!("unknown word `{word}`"),
    }
}

/// Swaps each friendly word in `expr` for the game's name, and refuses a
/// name written out in a rule file.
pub fn words(expr: &mut Expr) -> Result<()> {
    match expr {
        Expr::All(exprs) | Expr::Any(exprs) => exprs.iter_mut().try_for_each(words),
        Expr::Word(word) => {
            let friendly = word.starts_with("item:") || word.starts_with("beat:");
            let written_out = ITEM_PREFIXES.iter().any(|p| word.starts_with(p))
                || (word.len() > BEAT.len() && word.starts_with(BEAT));
            if written_out {
                bail!("`{word}` names the game: use an `item:` or `beat:` word");
            }
            if friendly {
                *word = id(word)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::rules::files::FILES;

    type Table = &'static [(&'static str, &'static Name)];

    fn every_table() -> [(&'static str, Table); 3] {
        [("item", ITEMS), ("beat", BEATS), ("place", PLACES)]
    }

    #[test]
    fn every_entry_resolves_against_the_dump() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        let missing: Vec<_> = every_table()
            .into_iter()
            .flat_map(|(kind, table)| table.iter().map(move |(w, n)| (kind, *w, *n)))
            .filter(|(_, _, n)| n.get().starts_with('#'))
            .map(|(kind, word, _)| format!("{kind}:{word}"))
            .collect();
        assert!(missing.is_empty(), "not in the dump: {missing:?}");
    }

    #[test]
    fn no_two_words_share_a_const() {
        for (kind, table) in every_table() {
            let mut words = BTreeSet::new();
            let mut consts = BTreeSet::new();
            for (word, name) in table {
                assert!(words.insert(*word), "{kind} word `{word}` twice");
                assert!(
                    consts.insert(name.0),
                    "{kind} word `{word}` repeats a const"
                );
            }
        }
    }

    /// The words rule files use, comments left out.
    fn used() -> BTreeSet<String> {
        let word = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "-:*".contains(c);
        FILES
            .iter()
            .flat_map(|(_, text)| text.lines())
            .map(|line| line.split('#').next().unwrap_or_default())
            .flat_map(|line| line.split(|c: char| !word(c)))
            .map(|w| w.split('*').next().unwrap_or_default().to_owned())
            .collect()
    }

    #[test]
    fn every_word_is_used_by_a_rule_file() {
        let used = used();
        let unused: Vec<_> = every_table()
            .into_iter()
            .flat_map(|(kind, table)| table.iter().map(move |(w, _)| (kind, *w)))
            .filter(|&(kind, word)| match kind {
                "place" => !used.contains(word),
                _ => !used.contains(&format!("{kind}:{word}")),
            })
            .collect();
        assert!(unused.is_empty(), "no rule file uses {unused:?}");
    }

    #[test]
    fn a_place_word_and_its_node_round_trip() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        for (word, _) in PLACES {
            assert_eq!(place_word(&node(word).unwrap()), Some(*word));
        }
    }

    #[test]
    fn friendly_words_become_names_and_unknown_ones_fail() {
        assert_eq!(id("item:hookshot").unwrap(), item::HOOKSHOT.get());
        assert_eq!(
            id("item:silver-scale*2").unwrap(),
            format!("{}*2", item::SCALE_SILVER.get())
        );
        assert_eq!(id("beat:gohma").unwrap(), format!("{BEAT}{}", field::GOHMA));
        for bad in ["item:no-such", "beat:", "hookshot"] {
            assert!(id(bad).is_err(), "{bad}");
        }
        assert!(node("no-such-place").is_err());
        let mut written = Expr::Word("Item_A".into());
        assert!(words(&mut written).is_err());
        let mut friendly = Expr::Any(vec![Expr::Word("item:no-such".into())]);
        assert!(format!("{:#}", words(&mut friendly).unwrap_err()).contains("item:no-such"));
    }
}
