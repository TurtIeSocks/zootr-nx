//! Keys Link starts with, so the chests that held them become checks. A
//! small-key door spends its dungeon's counter and a boss door reads its
//! boss key flag, so a new game that fills both opens every door.

use std::sync::LazyLock;

use anyhow::{Context, Result};

use crate::checks::ItemCheck;
use crate::key_tints;
use crate::known::{Name, actor, flag, flow, item, map};
use crate::rules::Need;

pub const SMALL: &Name = item::DUNGEON_SMALL_KEY;
pub const BOSS: &Name = item::DUNGEON_BOSS_KEY;
/// What a key chest brings to the pool: the key itself stays out of it.
pub const INSTEAD: &Name = actor::T_BOX_RUPEE_RED;

/// Each dungeon with keys, as its `DungeonInfo` member and Banc folder
/// name: how many small keys it has, which is how many small-key doors,
/// and the boss room its boss key opens, if it has one.
pub const DUNGEONS: [(&Name, i32, Option<&Name>); 8] = [
    (flow::BOTTOM_OF_THE_WELL, 2, None),
    (flag::FOREST_TEMPLE, 5, Some(map::PHANTOM_GANON_STAGE)),
    (flag::FIRE_TEMPLE, 8, Some(map::VOLVAGIA_STAGE)),
    (flag::WATER_TEMPLE, 4, Some(map::MORPHA_STAGE)),
    (flag::SHADOW_TEMPLE, 5, Some(map::BONGO_BONGO_STAGE)),
    (flag::SPIRIT_TEMPLE, 5, Some(map::TWINROVA_STAGE)),
    (flag::GERUDO_TRAINING_GROUND, 9, None),
    (flag::INSIDE_GANON_CASTLE, 1, Some(map::GANONDORF_STAGE)),
];

/// The dungeon whose maps `rel`, a Banc file, is under, by its number in
/// [`DUNGEONS`], counting from 1.
pub fn dungeon_of(rel: &str) -> Option<usize> {
    let folder = rel.strip_prefix("Banc/")?.split('/').next()?;
    DUNGEONS
        .iter()
        .position(|(d, ..)| *d == folder)
        .map(|i| i + 1)
}

/// Whether dungeon `n` has a boss key.
pub fn has_boss(n: usize) -> bool {
    DUNGEONS[n - 1].2.is_some()
}

/// With key rings, every ring and boss key, each dealt once.
pub static SHUFFLED: LazyLock<Vec<String>> = LazyLock::new(|| {
    let n = 1..=DUNGEONS.len();
    let rings = n.clone().map(key_tints::ring);
    rings
        .chain(n.filter(|&n| has_boss(n)).map(key_tints::boss))
        .collect()
});

/// Whether `item` is a ring or boss key copy, which only a chest can give:
/// the hook credits them as a chest opens.
pub fn is_shuffled(item: &str) -> bool {
    SHUFFLED.iter().any(|s| s == item)
}

/// Each ring and boss key takes the place of a key chest's Red Rupee.
pub fn deal_rings(items: &mut [&str]) -> Result<()> {
    for key in SHUFFLED.iter() {
        let rupee = items.iter().position(|i| *INSTEAD == **i);
        let rupee = rupee.context("too few key chests for the rings and boss keys")?;
        items[rupee] = key;
    }
    Ok(())
}

/// With key rings, what logic adds to a check in `rel`, a Banc file: its
/// dungeon's ring. Stricter than the game, which asks for it only past
/// the first locked door.
pub fn ring_need(rel: &str) -> Option<Need> {
    dungeon_of(rel).map(|n| holds(key_tints::ring(n)))
}

/// With key rings, what logic adds to the way into `room`, a boss room:
/// its dungeon's boss key and ring.
pub fn boss_door_need(room: &str) -> Option<Need> {
    let i = DUNGEONS
        .iter()
        .position(|(.., r)| r.is_some_and(|r| *r == *room))?;
    let keys = [key_tints::boss(i + 1), key_tints::ring(i + 1)];
    Some(Need::All(keys.map(holds).to_vec()))
}

/// A ring's or boss key copy's name, after its dungeon's: `area` names one
/// by its id.
pub fn copy_name(actor: &str, area: impl Fn(&str) -> String) -> Option<String> {
    DUNGEONS.iter().enumerate().find_map(|(i, (dungeon, ..))| {
        let n = i + 1;
        let kind = match actor {
            a if a == key_tints::ring(n) => "Key Ring",
            a if a == key_tints::boss(n) => "Boss Key",
            _ => return None,
        };
        Some(format!("{} {kind}", area(dungeon)))
    })
}

fn holds(actor: String) -> Need {
    Need::Items {
        actors: vec![actor],
        count: 1,
    }
}

/// The Gerudo Fortress's keys, one for each thief, each its own actor.
pub const FORTRESS_KEY: &str = "Obj_Dungeon_SmallKey_GerudoFortress_";

/// Whether `item` is a dungeon's small or boss key.
pub fn is_dungeon_key(item: &str) -> bool {
    item == SMALL || item == BOSS
}

/// Whether `item` is any key a new game can start with.
pub fn is_key(item: &str) -> bool {
    is_dungeon_key(item) || item.starts_with(FORTRESS_KEY)
}

/// What `check` brings to the pool: its item, or `INSTEAD` for a key.
pub fn pooled(check: &ItemCheck) -> &str {
    match is_key(&check.item) {
        true => INSTEAD,
        false => &check.item,
    }
}

/// The Gerudo Fortress counts keys like a dungeon, one for each thief.
pub const FORTRESS: (&Name, i32) = (flag::GERUDO_FORTRESS, 4);

/// The Bool `dungeon`'s boss key sets.
pub fn boss_flag(dungeon: &Name) -> String {
    format!("DungeonInfo.{dungeon}.IsGetBossKey")
}

/// The Int counting `dungeon`'s small keys.
pub fn small_flag(dungeon: &Name) -> String {
    format!("DungeonInfo.{dungeon}.{}", flag::SMALL_KEY_NUM)
}

/// The new-game defaults that fill the counters and boss key flags of
/// every dungeon, and the Fortress's counter when `fortress`.
pub fn defaults(dungeons: bool, fortress: bool) -> Vec<String> {
    let mut edits = Vec::new();
    for (name, small, boss) in DUNGEONS.iter().filter(|_| dungeons) {
        edits.push(format!("{}={small}", small_flag(name)));
        if boss.is_some() {
            edits.push(format!("{}=true", boss_flag(name)));
        }
    }
    if fortress {
        let (name, small) = FORTRESS;
        edits.push(format!("{}={small}", small_flag(name)));
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;
    use crate::known::{actor, flag, flow, item, map};
    use crate::presets;

    #[test]
    fn the_defaults_name_each_dungeon_with_its_count() {
        let edits = defaults(true, false);
        assert_eq!(edits.len(), 8 + 6);
        for line in [
            &format!(
                "DungeonInfo.{}.{}=5",
                flag::FOREST_TEMPLE,
                flag::SMALL_KEY_NUM
            ),
            &format!("DungeonInfo.{}.IsGetBossKey=true", flag::FOREST_TEMPLE),
            &format!(
                "DungeonInfo.{}.{}=1",
                flag::INSIDE_GANON_CASTLE,
                flag::SMALL_KEY_NUM
            ),
            &format!(
                "DungeonInfo.{}.IsGetBossKey=true",
                flag::INSIDE_GANON_CASTLE
            ),
            &format!(
                "DungeonInfo.{}.{}=9",
                flag::GERUDO_TRAINING_GROUND,
                flag::SMALL_KEY_NUM
            ),
        ] {
            assert!(edits.contains(&line.to_owned()), "{line}");
        }
        assert!(
            !edits
                .iter()
                .any(|e| e.contains(&format!("{}.IsGet", flow::BOTTOM_OF_THE_WELL)))
        );
        assert!(!edits.iter().any(|e| e.contains("Fortress")));
        assert_eq!(
            defaults(false, true),
            [format!(
                "DungeonInfo.{}.{}=4",
                flag::GERUDO_FORTRESS,
                flag::SMALL_KEY_NUM
            )]
        );
        assert!(defaults(false, false).is_empty());
    }

    #[test]
    fn each_ring_and_boss_key_takes_a_red_rupee_s_place() {
        needs_names!();
        let red = INSTEAD.get();
        let mut items = vec![red; SHUFFLED.len() + 1];
        items.push(item::HOOKSHOT.get());
        deal_rings(&mut items).unwrap();
        assert_eq!(items.iter().filter(|i| **i == red).count(), 1);
        assert!(SHUFFLED.iter().all(|k| items.contains(&k.as_str())));
        assert_eq!(SHUFFLED.len(), DUNGEONS.len() + 6);
        assert!(deal_rings(&mut [red; 3]).is_err());
    }

    #[test]
    fn logic_asks_a_dungeon_s_checks_for_its_ring_and_its_boss_door_for_both() {
        needs_names!();
        let forest = format!("Banc/{}/Fake.bcett.byml.zs", flag::FOREST_TEMPLE);
        let n = dungeon_of(&forest).unwrap();
        assert_eq!(ring_need(&forest).unwrap().to_string(), key_tints::ring(n));
        assert!(ring_need("Banc/FakeField/Fake.bcett.byml.zs").is_none());
        let door = boss_door_need(map::PHANTOM_GANON_STAGE)
            .unwrap()
            .to_string();
        assert!(door.contains(&key_tints::boss(n)) && door.contains(&key_tints::ring(n)));
        assert!(boss_door_need(map::GOHMA_STAGE).is_none());
    }

    #[test]
    fn a_copy_is_named_after_its_dungeon() {
        let area = |id: &str| format!("<{id}>");
        let forest = DUNGEONS.iter().position(|d| d.0 == flag::FOREST_TEMPLE);
        let n = forest.unwrap() + 1;
        let named = |a: String| copy_name(&a, area);
        assert_eq!(
            named(key_tints::ring(n)).unwrap(),
            format!("<{}> Key Ring", flag::FOREST_TEMPLE)
        );
        assert_eq!(
            named(key_tints::boss(n)).unwrap(),
            format!("<{}> Boss Key", flag::FOREST_TEMPLE)
        );
        assert!(named(item::HOOKSHOT.to_string()).is_none());
    }

    #[test]
    fn a_key_chest_brings_a_red_rupee() {
        needs_names!();
        let chest = |item: &str| ItemCheck {
            item: item.into(),
            ..crate::gifts::check(
                String::new(),
                "A",
                1,
                actor::OBJ_T_BOX_A_02.to_string(),
                item,
            )
        };
        assert_eq!(pooled(&chest(SMALL)), INSTEAD);
        assert_eq!(pooled(&chest(BOSS)), INSTEAD);
        assert_eq!(pooled(&chest(item::HOOKSHOT)), item::HOOKSHOT);
    }

    #[test]
    fn the_defaults_apply_and_match_the_games_keys() {
        use crate::checks::{GYAML, actors, banc_files, hash_of, load_banc, str_field};
        use std::collections::BTreeSet;
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let mut doc =
            ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        presets::apply(&gd, &mut doc.root, &defaults(true, true)).unwrap();
        let mut held = BTreeSet::new();
        let mut lying = BTreeSet::new();
        for rel in banc_files(&romfs).unwrap() {
            let area = rel.split('/').nth(1).unwrap().to_owned();
            let doc = load_banc(&romfs, &rel).unwrap();
            for actor in actors(&doc.root).unwrap() {
                let hash = hash_of(actor).unwrap();
                if str_field(actor, GYAML) == Some(actor::OBJ_DUNGEON_SMALL_KEY) {
                    lying.insert((area.clone(), hash));
                }
                let item = actor.as_map().unwrap().get("Dynamic");
                let item = item.and_then(|d| str_field(d, crate::checks::DROP_ACTOR));
                if let Some(item) = item.filter(|i| is_dungeon_key(i)) {
                    held.insert((area.clone(), item.to_owned(), hash));
                }
            }
        }
        for (name, small, boss) in DUNGEONS {
            let chests = |key: &str| {
                let n = held.iter().filter(|h| h.0 == name && h.1 == key).count();
                n as i32
            };
            let lying = lying.iter().filter(|l| l.0 == name).count() as i32;
            assert_eq!(chests(SMALL) + lying, small, "{name}");
            assert_eq!(chests(BOSS), i32::from(boss.is_some()), "{name}");
        }
        assert_eq!(held.len(), 43);
    }
}
