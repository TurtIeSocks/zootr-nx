//! The names the Archipelago world shows players. Ours, never the game's
//! text: the classic words where the remake's differ.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;

use crate::checks::ItemCheck;
use crate::generate::Spot;
use crate::known::{Name, actor, flag, flow, item, map};
use crate::progressive::{Level, Lines};
use crate::{check_names, cows, gifts, goron_pot, ice_traps, pickups, places, scrubs, shops};

/// Each area by its folder.
const AREAS: &[(&Name, &str)] = &[
    (flow::BOTTOM_OF_THE_WELL, "Bottom of the Well"),
    (flag::DEATH_MOUNTAIN_CRATER, "Death Mountain Crater"),
    (flag::DEATH_MOUNTAIN_TRAIL, "Death Mountain Trail"),
    (flag::DESERT_COLOSSUS, "Desert Colossus"),
    (flag::DODONGO_CAVERN, "Dodongo's Cavern"),
    (map::DREAM_WORLD, "Dream World"),
    (map::FIRE_TEMPLE, "Fire Temple"),
    (map::FOREST_TEMPLE, "Forest Temple"),
    (flag::GERUDO_FORTRESS, "Gerudo's Fortress"),
    (map::GERUDO_TRAINING_GROUND, "Gerudo Training Ground"),
    (flow::GERUDO_VALLEY, "Gerudo Valley"),
    (flag::GORON_CITY, "Goron City"),
    (flag::GRAVEYARD, "Graveyard"),
    (flag::HOUNTED_WASTELAND, "Haunted Wasteland"),
    (map::GANON_CASTLE, "Outside Ganon's Castle"),
    (flag::HYRULE_CASTLE, "Hyrule Castle"),
    (flag::HYRULE_FIELD, "Hyrule Field"),
    (map::ICE_CAVERN, "Ice Cavern"),
    (map::INSIDE_GANON_CASTLE, "Ganon's Castle"),
    (map::INSIDE_JABU_JABU_BELLY, "Jabu-Jabu's Belly"),
    (map::INSIDE_THE_DEKU_TREE, "Deku Tree"),
    (flag::KAKARIKO_VILLAGE, "Kakariko Village"),
    (flag::KOKIRI_FOREST, "Kokiri Forest"),
    (flag::LOST_WOOD, "Lost Woods"),
    (flag::MARKET, "Market"),
    (flow::SACRED_FOREST_MEADOW, "Sacred Forest Meadow"),
    (map::SHADOW_TEMPLE, "Shadow Temple"),
    (map::SPIRIT_TEMPLE, "Spirit Temple"),
    (map::WATER_TEMPLE, "Water Temple"),
    (flag::ZORA_DOMAIN, "Zora's Domain"),
    (flag::ZORA_FOUNTAIN, "Zora's Fountain"),
    (flag::ZORA_RIVER, "Zora's River"),
];

/// Each item by its actor. An upgrade line's levels are named one by one;
/// [`PROGRESSIVE`] names the line.
const ITEMS: &[(&Name, &str)] = &[
    (item::AGONY_STONE, "Stone of Agony"),
    (item::ALARM_CUCCO, "Cucco"),
    (item::ARROW_FIRE, "Fire Arrows"),
    (item::ARROW_ICE, "Ice Arrows"),
    (item::ARROW_LIGHT, "Light Arrows"),
    (item::BOMB_BAG_SMALL, "Bomb Bag"),
    (item::BOMB_BAG_MEDIUM, "Big Bomb Bag"),
    (item::BOMB_BAG_LARGE, "Biggest Bomb Bag"),
    (item::BOMB_BUNDLE_SMALL, "Bombs (5)"),
    (item::BOMBCHU_BUNDLE_SMALL, "Bombchus (5)"),
    (item::BOMBCHU_BUNDLE_MEDIUM, "Bombchus (10)"),
    (item::BOMBCHU_BUNDLE_LARGE, "Bombchus (20)"),
    (crate::bombchu_bags::SMALL, "Bombchu Bag (20)"),
    (crate::bombchu_bags::MEDIUM, "Bombchu Bag (30)"),
    (crate::bombchu_bags::LARGE, "Bombchu Bag (40)"),
    (item::BOOMERANG, "Boomerang"),
    (item::BOTTLE, "Bottle"),
    (item::BOTTLE_LETTER, "Ruto's Letter"),
    (item::BOTTLE_MILK, "Bottle with Milk"),
    (item::BOTTLE_RED_POTION, "Bottle with Red Potion"),
    (item::BOTTLE_GREEN_POTION, "Bottle with Green Potion"),
    (item::BOTTLE_BLUE_POTION, "Bottle with Blue Potion"),
    (item::BOTTLE_FAIRY, "Bottle with Fairy"),
    (item::BOTTLE_FISH, "Bottle with Fish"),
    (item::BOTTLE_BUG, "Bottle with Bugs"),
    (item::BOTTLE_POE, "Bottle with Poe"),
    (item::BOTTLE_BIG_POE, "Bottle with Big Poe"),
    (actor::ITEM_BOTTLE_BLUE_FIRE, "Bottle with Blue Fire"),
    (item::BROKEN_SWORD, "Broken Goron's Sword"),
    (item::BULLET_BAG_MEDIUM, "Big Bullet Bag"),
    (item::BULLET_BAG_LARGE, "Biggest Bullet Bag"),
    (item::CLAIM_CHECK, "Claim Check"),
    (item::COJIRO_CUCCO, "Cojiro"),
    (item::DEKU_STICK_BUNDLE_SMALL, "Deku Sticks (3)"),
    (item::DIN_FIRE, "Din's Fire"),
    (item::DUNGEON_BOSS_KEY, "Boss Key"),
    (item::DUNGEON_COMPASS, "Compass"),
    (item::DUNGEON_MAP, "Map"),
    (item::DUNGEON_SMALL_KEY, "Small Key"),
    (item::EYE_DROP, "Eye Drops"),
    (item::EYEBALL_FROG, "Eyeball Frog"),
    (item::FAIRY_BLESS_DEFENSE_DOUBLE, "Double Defense"),
    (item::FAIRY_BLESS_MP, "Improved Spin"),
    (item::FAIRY_BLESS_MP_DOUBLE, "Double Magic"),
    (item::FARORE_WIND, "Farore's Wind"),
    (item::GAUNTLET_GORON, "Goron's Bracelet"),
    (item::GAUNTLET_SILVER, "Silver Gauntlets"),
    (item::GAUNTLET_GOLD, "Golden Gauntlets"),
    (item::GLASS_TRUTH, "Lens of Truth"),
    (item::HAMMER, "Megaton Hammer"),
    (item::HEAVY_BOOT, "Iron Boots"),
    (item::HOOKSHOT, "Hookshot"),
    (item::HOVER_BOOT, "Hover Boots"),
    (item::LONGSHOT, "Longshot"),
    (item::MAGIC_BEAN, "Magic Bean"),
    (item::MASK_KEATON, "Keaton Mask"),
    (item::MASK_SKULL, "Skull Mask"),
    (item::MASK_SPOOKY, "Spooky Mask"),
    (item::MASK_BUNNY, "Bunny Hood"),
    (item::MASK_GORON, "Goron Mask"),
    (item::MASK_ZORA, "Zora Mask"),
    (item::MASK_GERUDO, "Gerudo Mask"),
    (item::MASK_TRUTH, "Mask of Truth"),
    (item::MEDAL_FIRE, "Fire Medallion"),
    (item::MEDAL_FOREST, "Forest Medallion"),
    (item::MEDAL_LIGHT, "Light Medallion"),
    (item::MEDAL_SHADOW, "Shadow Medallion"),
    (item::MEDAL_SPIRIT, "Spirit Medallion"),
    (item::MEDAL_WATER, "Water Medallion"),
    (crate::magic::meter::METER, "Magic Meter"),
    (item::MELODY_ZELDA_LULLABY, "Zelda's Lullaby"),
    (item::MELODY_EPONA_SONG, "Epona's Song"),
    (item::MELODY_SARIA_SONG, "Saria's Song"),
    (item::MELODY_SUN_SONG, "Sun's Song"),
    (item::MELODY_TIME_SONG, "Song of Time"),
    (item::MELODY_STORM_SONG, "Song of Storms"),
    (item::MELODY_FOREST_MINUET, "Minuet of Forest"),
    (item::MELODY_FIRE_BOLERO, "Bolero of Fire"),
    (item::MELODY_WATER_SERENADE, "Serenade of Water"),
    (item::MELODY_SPIRIT_REQUIEM, "Requiem of Spirit"),
    (item::MELODY_SHADOW_NOCTURNE, "Nocturne of Shadow"),
    (item::MELODY_LIGHT_PRELUDE, "Prelude of Light"),
    (item::NAYRU_LOVE, "Nayru's Love"),
    (item::NUT_BAG_SMALL, "Deku Nut Bag (20)"),
    (item::NUT_BAG_MEDIUM, "Deku Nut Bag (30)"),
    (item::NUT_BAG_LARGE, "Deku Nut Bag (40)"),
    (item::OCARINA_FAIRY, "Fairy Ocarina"),
    (item::ODD_MUSHROOM, "Odd Mushroom"),
    (item::ODD_POWDER, "Odd Potion"),
    (item::POACHER_SAW, "Poacher's Saw"),
    (item::POCKET_EGG, "Pocket Egg"),
    (item::PRESCRIPTION, "Prescription"),
    (item::OCARINA_TIME, "Ocarina of Time"),
    (item::QUIVER_MEDIUM, "Big Quiver"),
    (item::QUIVER_LARGE, "Biggest Quiver"),
    (item::RANGE_BOW, "Fairy Bow"),
    (item::RANGE_SLING_SHOT, "Fairy Slingshot"),
    (item::SCALE_SILVER, "Silver Scale"),
    (item::SCALE_GOLD, "Golden Scale"),
    (item::SCRUB_BUSINESS_SEED, "Business Scrub Seed"),
    (item::SHIELD_MIRROR, "Mirror Shield"),
    (item::STICK_BAG_SMALL, "Deku Stick Bag (10)"),
    (item::STICK_BAG_MEDIUM, "Deku Stick Bag (20)"),
    (item::STONE_FIRE, "Goron's Ruby"),
    (item::STONE_FOREST, "Kokiri's Emerald"),
    (item::STONE_WATER, "Zora's Sapphire"),
    (item::STICK_BAG_LARGE, "Deku Stick Bag (30)"),
    (item::SWORD_BIGGORON, "Biggoron's Sword"),
    (item::SWORD_GIANT, "Giant's Knife"),
    (item::SWORD_KOKIRI, "Kokiri Sword"),
    (item::TOKEN_GERUDO, "Gerudo Membership Card"),
    (item::TOKEN_GOLD, "Gold Skulltula Token"),
    (item::WALLET_MEDIUM, "Adult's Wallet"),
    (item::WALLET_LARGE, "Giant's Wallet"),
    (item::WEIRD_EGG, "Weird Egg"),
    (item::ZELDA_LETTER, "Zelda's Letter"),
    (actor::OBJ_ARMOR_GORON, "Goron Tunic"),
    (actor::OBJ_ARMOR_ZORA, "Zora Tunic"),
    (actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_01, "Small Key"),
    (actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_02, "Small Key"),
    (actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_03, "Small Key"),
    (actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_04, "Small Key"),
    (actor::OBJ_HEART_CONTAINER, "Heart Container"),
    (actor::OBJ_HEART_PIECE, "Piece of Heart"),
    (actor::TBOX_MAGIC_JAR_LARGE, "Magic Jar"),
    (actor::OBJ_MAGIC_JAR_SMALL, "Small Magic Jar"),
    (actor::OBJ_HEART, "Recovery Heart"),
    (actor::OBJ_FAIRY, "Fairy"),
    (actor::OBJ_BOMB, "Bomb"),
    (actor::OBJ_DEKU_STICK, "Deku Stick"),
    (actor::OBJ_DEKU_NUT, "Deku Nut"),
    (actor::OBJ_DEKU_SEED, "Deku Seeds (3)"),
    (actor::T_BOX_ARROW_BUNDLE_SMALL, "Arrows (3)"),
    (actor::T_BOX_ARROW_BUNDLE_MEDIUM, "Arrows (5)"),
    (actor::T_BOX_ARROW_BUNDLE_LARGE, "Arrows (10)"),
    (actor::T_BOX_DEKU_NUT_BUNDLE, "Deku Nuts (3)"),
    (actor::T_BOX_DEKU_SEED_BUNDLE, "Deku Seeds (5)"),
    (actor::T_BOX_RUPEE_GREEN, "Green Rupee (1)"),
    (actor::T_BOX_RUPEE_BLUE, "Blue Rupee (5)"),
    (actor::T_BOX_RUPEE_RED, "Red Rupee (20)"),
    (actor::T_BOX_RUPEE_PURPLE, "Purple Rupee (50)"),
    (actor::T_BOX_RUPEE_GOLD, "Gold Rupee (200)"),
    (actor::T_BOX_SHIELD_DEKU, "Deku Shield"),
    (actor::T_BOX_SHIELD_HYLIAN, "Hylian Shield"),
    (actor::WEAPON_SWORD_MASTER, "Master Sword"),
];

/// Each upgrade line's name with progressive items on, by the level whose
/// id the name keeps: its first before the starter bags and the Magic
/// Meter's setting joined AP.
const PROGRESSIVE: &[(&Name, &str)] = &[
    (item::HOOKSHOT, "Progressive Hookshot"),
    (item::GAUNTLET_GORON, "Progressive Strength Upgrade"),
    (item::OCARINA_FAIRY, "Progressive Ocarina"),
    (item::SCALE_SILVER, "Progressive Scale"),
    (item::WALLET_MEDIUM, "Progressive Wallet"),
    (item::BOMB_BAG_SMALL, "Progressive Bomb Bag"),
    (item::RANGE_BOW, "Progressive Bow"),
    (item::RANGE_SLING_SHOT, "Progressive Slingshot"),
    (item::STICK_BAG_MEDIUM, "Progressive Deku Stick Bag"),
    (item::NUT_BAG_MEDIUM, "Progressive Deku Nut Bag"),
    (crate::bombchu_bags::SMALL, "Progressive Bombchu Bag"),
    (crate::magic::meter::METER, "Progressive Magic Meter"),
];

fn find(table: &[(&Name, &'static str)], id: &str) -> Option<&'static str> {
    table.iter().find(|(n, _)| n.get() == id).map(|(_, w)| *w)
}

/// The area's name.
pub fn area(folder: &str) -> Result<&'static str> {
    find(AREAS, folder).with_context(|| format!("no Archipelago name for area {folder}"))
}

/// Items AP counts as another, which gives the same: a gift's rupee pays
/// what a chest's does.
const SAME: &[(&Name, &Name)] = &[
    (actor::OBJ_RUPEE_GREEN, actor::T_BOX_RUPEE_GREEN),
    (actor::OBJ_RUPEE_BLUE, actor::T_BOX_RUPEE_BLUE),
    (actor::OBJ_RUPEE_RED, actor::T_BOX_RUPEE_RED),
    (actor::OBJ_RUPEE_PURPLE, actor::T_BOX_RUPEE_PURPLE),
    (actor::OBJ_RUPEE_GOLD, actor::T_BOX_RUPEE_GOLD),
    (actor::OBJ_HEART_IN_WATER, actor::OBJ_HEART),
    (
        actor::OBJ_ARROW_BUNDLE_SMALL,
        actor::T_BOX_ARROW_BUNDLE_SMALL,
    ),
];

/// The item AP counts `actor` as.
pub fn same(actor: &str) -> &str {
    SAME.iter()
        .find(|(a, _)| a.get() == actor)
        .map_or(actor, |(_, to)| to.get())
}

/// Each copy the hook credits (a dungeon's map or compass, a Fortress
/// key), by its actor, named for its dungeon.
static COPIES: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    let mut out = BTreeMap::new();
    for copy in crate::map_copies::copies() {
        let name = crate::map_copies::copy_name(&copy, |f| area(f).unwrap_or(f).to_owned());
        out.insert(copy, name.expect("a map copy names itself"));
    }
    out.insert(
        crate::key_tints::fortress(),
        "Gerudo's Fortress Small Key".into(),
    );
    out
});

/// Every actor the world names: any of ours another slot's seed can hold.
pub fn named() -> impl Iterator<Item = &'static str> {
    let tables = ITEMS.iter().chain(PROGRESSIVE).map(|(n, _)| n.get());
    let trap = std::iter::once(ice_traps::POOLED);
    tables.chain(COPIES.keys().map(String::as_str)).chain(trap)
}

/// The item's name: with progressive items on, its line's at any level,
/// else its own.
pub fn item(actor: &str, progressive: bool) -> Result<&'static str> {
    let actor = same(actor);
    if let Some(name) = COPIES.get(actor) {
        return Ok(name);
    }
    if actor == ice_traps::POOLED {
        // Not a [`Name`]: no dump holds it.
        return Ok(ice_traps::NAME);
    }
    let name = match progressive.then(|| key(actor)).flatten() {
        Some(key) => find(PROGRESSIVE, key),
        None => find(ITEMS, actor),
    };
    name.with_context(|| format!("no Archipelago name for item {actor}"))
}

/// The level [`PROGRESSIVE`] names the line `actor` is any level of by,
/// if it climbs.
fn key(actor: &str) -> Option<&'static str> {
    let line = lines().find(|l| l.iter().any(|level| level.0.get() == actor))?;
    let mut levels = line.iter().map(|level| level.0.get());
    levels.find(|a| find(PROGRESSIVE, a).is_some())
}

/// The upgrade lines AP names, at their longest: both starter bags, the
/// Bombchu Bags and the Magic Meter. One name stands for a line at every
/// level, and a seed keeps the levels its settings do (`pool.json`
/// `levels`, each with its `when`).
fn lines() -> impl Iterator<Item = &'static [Level]> {
    Lines::widest(false).all().filter(|l| l.len() > 1)
}

/// A line's name and its levels, lowest first: each level's actor and own
/// name.
pub type Levels = (&'static str, Vec<(&'static str, &'static str)>);

/// Each line's [`Levels`].
pub fn levels() -> Result<Vec<Levels>> {
    let named = |l: &[Level]| -> Result<_> {
        let steps = l
            .iter()
            .map(|level| Ok((level.0.get(), item(level.0.get(), false)?)));
        Ok((item(l[0].0.get(), true)?, steps.collect::<Result<_>>()?))
    };
    lines().map(named).collect()
}

/// The name of the line `actor` is any level of, if it climbs, and the
/// level whose id the name keeps: with progressive items on, the world
/// gives the line's name to each check's own item, which the pool names.
pub fn line(actor: &str) -> Result<Option<(&'static str, &'static str)>> {
    let key = key(same(actor));
    key.map(|k| Ok((item(k, true)?, k))).transpose()
}

/// Each check's location name, by hash: its hand-written name
/// (`check_names`), else the area, then where (from `places::fill`), then
/// the vanilla item, then the holder. Names two checks would share get
/// where each stands, then a number.
pub fn locations(romfs: &Romfs, checks: &[ItemCheck]) -> Result<BTreeMap<u64, String>> {
    let mut spots = Vec::new();
    for c in checks {
        spots.push(Spot {
            hash: c.hash,
            area: c.area.clone(),
            holder: c.holder.clone(),
            vanilla: c.item.clone(),
            chest_name: item(&c.item, false)?.to_owned(),
            ..Default::default()
        });
    }
    places::fill(romfs, &mut spots)?;
    let mut out = BTreeMap::new();
    for (c, s) in checks.iter().zip(&spots) {
        let area = area(&c.area)?;
        let name = if let Some(name) = check_names::of(c.hash) {
            name.to_owned()
        } else if scrubs::of(c).is_some() {
            match c.holder.split_once(", ") {
                Some((_, what)) => format!("{area} Business Scrub ({})", title(what)),
                None => format!("{area} Business Scrub"),
            }
        } else if gifts::of(c).is_some() {
            format!("{area} {}", title(beside(area, &c.holder)))
        } else if c.holder == actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02.get() {
            format!("{area} Treasure Chest Game Prize")
        } else if cows::is_holder(&c.holder) || shops::is_holder(&c.holder) {
            // A shop slot or cow names its own place: "Market Bazaar 1".
            format!("{area} {}", beside(area, &c.holder))
        } else if c.hash == goron_pot::HASH {
            format!("{area} Spinning Pot {}", item(&c.item, false)?)
        } else if let Some(kind) = breakable(c) {
            format!("{area} {kind}")
        } else if pickups::is_pickup(c) {
            format!("{area} {}", item(&c.item, false)?)
        } else if !s.place.is_empty() {
            match s.place.rsplit_once(' ') {
                Some((at, n)) if n.parse::<u32>().is_ok() => format!("{area} {at} Chest {n}"),
                _ => format!("{area} {} Chest", s.place),
            }
        } else {
            bail!("no Archipelago name rule for check {} in {area}", c.hash);
        };
        out.insert(c.hash, name);
    }
    apart(romfs, checks, &mut out)?;
    let unique: std::collections::BTreeSet<&String> = out.values().collect();
    ensure!(
        unique.len() == out.len(),
        "two checks share a location name"
    );
    Ok(out)
}

/// What breakable holds `check`, as its location names it: a pot, not
/// the drop it held in the vanilla game.
fn breakable(check: &ItemCheck) -> Option<&'static str> {
    use pickups::{CRATES, FLYING_POTS, GRASS, HIVES, POTS, ROCKS};
    let kinds = [
        (&POTS, "Pot"),
        (&FLYING_POTS, "Flying Pot"),
        (&CRATES, "Crate"),
        (&ROCKS, "Rock"),
        (&GRASS, "Grass"),
        (&HIVES, "Beehive"),
    ];
    kinds
        .into_iter()
        .find(|(k, _)| k.holds(check))
        .map(|(_, name)| name)
}

/// Gives each check whose name another shares where it stands in its area:
/// "Hyrule Field Gold Skulltula Token" becomes "Hyrule Field Southwest Gold
/// Skulltula Token", numbered when that clashes too.
fn apart(romfs: &Romfs, checks: &[ItemCheck], names: &mut BTreeMap<u64, String>) -> Result<()> {
    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    for n in names.values() {
        *seen.entry(n).or_default() += 1;
    }
    let shared: Vec<&ItemCheck> = (checks.iter())
        .filter(|c| seen.get(names[&c.hash].as_str()) > Some(&1))
        .collect();
    let mut spots = Vec::new();
    for c in &shared {
        let what = names[&c.hash]
            .strip_prefix(area(&c.area)?)
            .unwrap_or_default();
        spots.push(Spot {
            hash: c.hash,
            area: c.area.clone(),
            chest_name: what.trim().to_owned(),
            ..Default::default()
        });
    }
    places::fill_any(romfs, &mut spots)?;
    for (c, s) in shared.iter().zip(&spots) {
        ensure!(
            !s.place.is_empty(),
            "{} shares its name and isn't placed",
            names[&c.hash]
        );
        names.insert(c.hash, format!("{} {}", area(&c.area)?, s.place));
    }
    Ok(())
}

/// `name` with the words that repeat `area` dropped, for a name that
/// follows it: the area's own words at its start ("Kakariko shooting
/// gallery" in Kakariko Village, "Zora Shop 1" in Zora's Domain) and an
/// "in (the) ..." or "at (the) ..." of the area's words at its end
/// ("Sheik in the Crater" in Death Mountain Crater).
fn beside<'a>(area: &str, name: &'a str) -> &'a str {
    let area: Vec<String> = area.split(' ').map(str::to_lowercase).collect();
    let words: Vec<&str> = name.split(' ').collect();
    let lead = (words.iter().zip(&area))
        .take_while(|(w, a)| a.starts_with(&w.to_lowercase()))
        .count();
    let lead = lead.min(words.len() - 1);
    let tail = match words.iter().rposition(|w| ["in", "at"].contains(w)) {
        Some(i) if i > lead => {
            let rest = &words[i + 1..];
            let rest = rest.strip_prefix(&["the"]).unwrap_or(rest);
            let joined = rest.join(" ").to_lowercase();
            let theirs = area.join(" ");
            let ours =
                !rest.is_empty() && (theirs.ends_with(&joined) || theirs.starts_with(&joined));
            if ours { words.len() - i } else { 0 }
        }
        _ => 0,
    };
    let start: usize = words[..lead].iter().map(|w| w.len() + 1).sum();
    let end = name.len()
        - words[words.len() - tail..]
            .iter()
            .map(|w| w.len() + 1)
            .sum::<usize>();
    &name[start..end.max(start)]
}

/// Each word capitalized, but the small ones inside a name.
fn title(text: &str) -> String {
    const SMALL: &[&str] = &["a", "an", "as", "at", "in", "of", "on", "the", "to"];
    let word = |(i, w): (usize, &str)| {
        if i > 0 && SMALL.contains(&w) {
            return w.to_owned();
        }
        let mut cs = w.chars();
        cs.next()
            .map(|c| c.to_uppercase().chain(cs).collect())
            .unwrap_or_default()
    };
    text.split(' ')
        .enumerate()
        .map(word)
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every breakable reads as what holds it, never its drop: "Kakariko
    /// Village East Pot 3".
    #[test]
    fn a_breakable_is_named_by_its_kind() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let on = ["pots", "crates", "rocks", "grass", "hives"];
        let settings = on.map(|id| (id.to_owned(), true.into())).into();
        let new_game = crate::settings::NewGame::from_settings(&settings, &[]).unwrap();
        let mut checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        checks.retain(|c| breakable(c).is_some());
        let names = locations(&romfs, &checks).unwrap();
        for c in &checks {
            let (kind, name) = (breakable(c).unwrap(), &names[&c.hash]);
            let bare = name.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == ' ');
            assert!(bare.ends_with(kind), "{name} isn't a {kind}");
        }
        assert!(checks.len() > 900, "{} breakables", checks.len());
    }

    /// A gift reads as its giver, without the area twice.
    #[test]
    fn a_name_beside_its_area_drops_the_area_s_words() {
        let cases = [
            (
                "Kakariko Village",
                "Kakariko shooting gallery",
                "Kakariko Village Shooting Gallery",
            ),
            ("Lost Woods", "Lost Woods target", "Lost Woods Target"),
            ("Ice Cavern", "Sheik in the Ice Cavern", "Ice Cavern Sheik"),
            (
                "Death Mountain Crater",
                "Sheik in the Crater",
                "Death Mountain Crater Sheik",
            ),
            (
                "Kakariko Village",
                "Sheik in Kakariko",
                "Kakariko Village Sheik",
            ),
            (
                "Desert Colossus",
                "Sheik at the Colossus",
                "Desert Colossus Sheik",
            ),
            ("Zora's Domain", "Zora Shop 8", "Zora's Domain Shop 8"),
            (
                "Market",
                "Sheik at the Temple of Time",
                "Market Sheik at the Temple of Time",
            ),
            (
                "Zora's Fountain",
                "Great Fairy of Farore",
                "Zora's Fountain Great Fairy of Farore",
            ),
            ("Hyrule Field", "Zelda's song", "Hyrule Field Zelda's Song"),
            ("Market", "Market", "Market Market"),
        ];
        for (area, giver, want) in cases {
            assert_eq!(format!("{area} {}", title(beside(area, giver))), want);
        }
    }

    /// The starter bags and the Magic Meter included, so a level a
    /// setting adds never goes by a name of its own.
    #[test]
    fn a_line_goes_by_one_name_at_every_level() {
        for l in lines() {
            let name = item(l[0].0.get(), true).unwrap();
            assert!(name.starts_with("Progressive "), "{name}");
            for level in l {
                assert_eq!(item(level.0.get(), true).unwrap(), name);
            }
        }
        let nuts = item(item::NUT_BAG_SMALL.get(), true).unwrap();
        assert_eq!(nuts, "Progressive Deku Nut Bag");
    }

    #[test]
    fn the_running_man_s_purple_rupee_is_a_chest_s_to_ap() {
        let race = item(actor::OBJ_RUPEE_PURPLE, false).unwrap();
        assert_eq!(race, item(actor::T_BOX_RUPEE_PURPLE, false).unwrap());
        assert_eq!(
            same(actor::OBJ_RUPEE_PURPLE),
            actor::T_BOX_RUPEE_PURPLE.get()
        );
    }
}
