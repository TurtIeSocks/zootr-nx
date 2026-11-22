//! The names the Archipelago world shows players. Ours, never the game's
//! text: the classic words where the remake's differ.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;

use crate::checks::ItemCheck;
use crate::generate::Spot;
use crate::known::{Name, actor, flag, flow, item, map};
use crate::progressive::Lines;
use crate::{gifts, pickups, places, scrubs};

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
    (item::BOOMERANG, "Boomerang"),
    (item::BOTTLE, "Bottle"),
    (item::BOTTLE_LETTER, "Ruto's Letter"),
    (item::BOTTLE_MILK, "Bottle with Milk"),
    (item::BULLET_BAG_MEDIUM, "Big Bullet Bag"),
    (item::BULLET_BAG_LARGE, "Biggest Bullet Bag"),
    (item::DEKU_STICK_BUNDLE_SMALL, "Deku Sticks (3)"),
    (item::DIN_FIRE, "Din's Fire"),
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
    (item::NUT_BAG_MEDIUM, "Deku Nut Bag (30)"),
    (item::NUT_BAG_LARGE, "Deku Nut Bag (40)"),
    (item::OCARINA_FAIRY, "Fairy Ocarina"),
    (item::OCARINA_TIME, "Ocarina of Time"),
    (item::QUIVER_MEDIUM, "Big Quiver"),
    (item::QUIVER_LARGE, "Biggest Quiver"),
    (item::RANGE_BOW, "Fairy Bow"),
    (item::RANGE_SLING_SHOT, "Fairy Slingshot"),
    (item::SCALE_SILVER, "Silver Scale"),
    (item::SCALE_GOLD, "Golden Scale"),
    (item::SCRUB_BUSINESS_SEED, "Business Scrub Seed"),
    (item::SHIELD_MIRROR, "Mirror Shield"),
    (item::STICK_BAG_MEDIUM, "Deku Stick Bag (20)"),
    (item::STICK_BAG_LARGE, "Deku Stick Bag (30)"),
    (item::SWORD_BIGGORON, "Biggoron's Sword"),
    (item::SWORD_KOKIRI, "Kokiri Sword"),
    (item::TOKEN_GERUDO, "Gerudo Membership Card"),
    (item::WALLET_MEDIUM, "Adult's Wallet"),
    (item::WALLET_LARGE, "Giant's Wallet"),
    (item::WEIRD_EGG, "Weird Egg"),
    (item::ZELDA_LETTER, "Zelda's Letter"),
    (actor::OBJ_ARMOR_GORON, "Goron Tunic"),
    (actor::OBJ_ARMOR_ZORA, "Zora Tunic"),
    (actor::OBJ_HEART_PIECE, "Piece of Heart"),
    (actor::TBOX_MAGIC_JAR_LARGE, "Magic Jar"),
    (actor::T_BOX_ARROW_BUNDLE_SMALL, "Arrows (3)"),
    (actor::T_BOX_ARROW_BUNDLE_MEDIUM, "Arrows (5)"),
    (actor::T_BOX_ARROW_BUNDLE_LARGE, "Arrows (10)"),
    (actor::T_BOX_DEKU_NUT_BUNDLE, "Deku Nuts (3)"),
    (actor::T_BOX_RUPEE_GREEN, "Green Rupee (1)"),
    (actor::T_BOX_RUPEE_BLUE, "Blue Rupee (5)"),
    (actor::T_BOX_RUPEE_RED, "Red Rupee (20)"),
    (actor::T_BOX_RUPEE_PURPLE, "Purple Rupee (50)"),
    (actor::T_BOX_RUPEE_GOLD, "Gold Rupee (200)"),
    (actor::T_BOX_SHIELD_DEKU, "Deku Shield"),
    (actor::T_BOX_SHIELD_HYLIAN, "Hylian Shield"),
];

/// Each upgrade line by its first level, with progressive items on.
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
    (actor::OBJ_RUPEE_BLUE, actor::T_BOX_RUPEE_BLUE),
    (actor::OBJ_RUPEE_PURPLE, actor::T_BOX_RUPEE_PURPLE),
    (actor::OBJ_RUPEE_GOLD, actor::T_BOX_RUPEE_GOLD),
];

/// The item AP counts `actor` as.
pub fn same(actor: &str) -> &str {
    SAME.iter()
        .find(|(a, _)| a.get() == actor)
        .map_or(actor, |(_, to)| to.get())
}

/// The item's name: its line's with progressive items on, else its own.
pub fn item(actor: &str, progressive: bool) -> Result<&'static str> {
    let actor = same(actor);
    // The world never shuffles a starter bag, so its lines are the game's.
    let line = progressive
        .then(|| Lines::default().all().find(|l| l[0].0.get() == actor))
        .flatten();
    let name = match line {
        Some(_) => find(PROGRESSIVE, actor),
        None => find(ITEMS, actor),
    };
    name.with_context(|| format!("no Archipelago name for item {actor}"))
}

/// Each check's location name, by hash: the area, then where (from
/// `places::fill`), then the vanilla item, then the holder.
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
        let name = if scrubs::of(c).is_some() {
            match c.holder.split_once(", ") {
                Some((_, what)) => format!("{area} Business Scrub ({})", title(what)),
                None => format!("{area} Business Scrub"),
            }
        } else if gifts::of(c).is_some() {
            format!("{area} {}'s Gift", c.holder)
        } else if c.holder == actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02.get() {
            format!("{area} Treasure Chest Game Prize")
        } else if pickups::is_pickup(c) {
            // One lone pickup per area so far: the lake's Fire Arrows, the
            // letter in the bottle.
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
    let unique: std::collections::BTreeSet<&String> = out.values().collect();
    ensure!(
        unique.len() == out.len(),
        "two checks share a location name"
    );
    Ok(out)
}

/// Each word capitalized.
fn title(text: &str) -> String {
    let word = |w: &str| {
        let mut cs = w.chars();
        cs.next()
            .map(|c| c.to_uppercase().chain(cs).collect())
            .unwrap_or_default()
    };
    text.split(' ').map(word).collect::<Vec<String>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
