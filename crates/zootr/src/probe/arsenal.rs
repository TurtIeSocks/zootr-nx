//! `probe arsenal`: the numbers the game keeps for how Link moves and how
//! far his gear reaches, adult beside child, for the room logic.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Romfs};

use crate::items::show;
use crate::known::{Name, actor, item};

/// Link's own files, by the folder they sit in.
const LINK: &[&str] = &["PlayerGameParam", "CctComponentPlayerClimbParam"];

/// Each piece of gear: what to call it, the pack, and the folder of the
/// file that holds its numbers.
const GEAR: &[(&str, &Name, &str)] = &[
    ("Hookshot", actor::WEAPON_RANGE_HOOKSHOT, "HookshotParam"),
    ("Hookshot tip", actor::SHOOTABLE_HOOK, "ShootableParam"),
    ("Longshot", actor::WEAPON_RANGE_LONGSHOT, "HookshotParam"),
    ("Longshot tip", actor::SHOOTABLE_HOOK_LONG, "ShootableParam"),
    ("Hover Boots", item::HOVER_BOOT, "PlayerEquipmentActor"),
    ("Iron Boots", item::HEAVY_BOOT, "PlayerEquipmentActor"),
    ("Bow", actor::WEAPON_RANGE_BOW, "BowParam"),
    ("Slingshot", actor::WEAPON_RANGE_SLING_SHOT, "BowParam"),
    ("Boomerang", item::BOOMERANG, "Boomerang"),
    ("Bomb", item::BOMB, "AttackParam"),
    ("Bombchu", item::BOMBCHU, "BombchuParam"),
    ("Silver Scale", item::SCALE_SILVER, "ExpansionAbilityActor"),
    ("Golden Scale", item::SCALE_GOLD, "ExpansionAbilityActor"),
    (
        "Goron Bracelet",
        item::GAUNTLET_GORON,
        "ExpansionAbilityActor",
    ),
    (
        "Silver Gauntlets",
        item::GAUNTLET_SILVER,
        "ExpansionAbilityActor",
    ),
    (
        "Golden Gauntlets",
        item::GAUNTLET_GOLD,
        "ExpansionAbilityActor",
    ),
    ("Magic", item::DIN_FIRE, "ExpansionAbilityActor"),
    ("Bunny Hood", actor::ARMOR_CHILD_MASK_BUNNY, "MaskParam"),
    ("Lens of Truth", actor::ARMOR_GLASS_TRUTH, "MaskParam"),
    ("Hylian Shield", actor::WEAPON_SHIELD_HYLIAN, "ShieldParam"),
    ("Mirror Shield", actor::WEAPON_SHIELD_MIRROR, "ShieldParam"),
];

/// The file in `pack` under `folder`, its own when the folder holds a
/// base file too.
pub(super) fn file(pack: &ActorPack, name: &str, folder: &str) -> Option<String> {
    let under = format!("{folder}/");
    let hits: Vec<String> = (pack.files().into_iter())
        .filter(|f| f.contains(&under))
        .collect();
    let own = format!("/{name}.");
    let mine = hits.iter().find(|f| f.contains(&own));
    mine.or(hits.first()).cloned()
}

/// `file`'s values in `pack`, key and shown value, keys sorted.
fn values(pack: &ActorPack, name: &str, folder: &str) -> Result<Vec<(String, String)>> {
    let Some(path) = file(pack, name, folder) else {
        return Ok(vec![]);
    };
    let map = pack.resolve(&path)?.unwrap_or_default();
    let mut out: Vec<_> = map.iter().map(|(k, v)| (k.to_string(), show(v))).collect();
    out.sort();
    Ok(out)
}

fn wanted(key: &str, grep: Option<&str>) -> bool {
    grep.is_none_or(|g| key.to_lowercase().contains(&g.to_lowercase()))
}

pub(super) fn arsenal(romfs: &Romfs, grep: Option<&str>) -> Result<()> {
    let (adult, child) = (actor::PLAYER_ADULT, actor::PLAYER_CHILD);
    let packs = [adult, child].map(|a| ActorPack::load(romfs, a).with_context(|| a.to_string()));
    let [adult_pack, child_pack] = packs;
    let (adult_pack, child_pack) = (adult_pack?, child_pack?);
    for folder in LINK {
        println!("== Link: {folder} (adult, child; = where the same)");
        let a = values(&adult_pack, adult, folder)?;
        let c = values(&child_pack, child, folder)?;
        let keys: BTreeSet<&String> = a.iter().chain(&c).map(|(k, _)| k).collect();
        let of = |vs: &[(String, String)], k: &str| {
            let hit = vs.iter().find(|(key, _)| key == k);
            hit.map_or("-".to_string(), |(_, v)| v.clone())
        };
        for k in keys.into_iter().filter(|k| wanted(k, grep)) {
            let (av, cv) = (of(&a, k), of(&c, k));
            let cv = if av == cv { "=".to_string() } else { cv };
            println!("  {k}\t{av}\t{cv}");
        }
    }
    for (label, name, folder) in GEAR {
        let pack = ActorPack::load(romfs, name).with_context(|| name.to_string())?;
        let shown: Vec<_> = (values(&pack, name, folder)?.into_iter())
            .filter(|(k, _)| wanted(k, grep))
            .collect();
        if shown.is_empty() && grep.is_some() {
            continue;
        }
        println!("== {label} ({name}, {folder})");
        for (k, v) in shown {
            println!("  {k}\t{v}");
        }
    }
    Ok(())
}
