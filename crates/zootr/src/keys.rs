//! Keys Link starts with, so the chests that held them become checks. A
//! small-key door spends its dungeon's counter and a boss door reads its
//! boss key flag, so a new game that fills both opens every door.

use crate::checks::ItemCheck;
use crate::known::{Name, actor, flag, flow, item};

pub const SMALL: &Name = item::DUNGEON_SMALL_KEY;
pub const BOSS: &Name = item::DUNGEON_BOSS_KEY;
/// What a key chest brings to the pool: the key itself stays out of it.
pub const INSTEAD: &Name = actor::T_BOX_RUPEE_RED;

/// Each dungeon with keys, as its `DungeonInfo` member and Banc folder
/// name: how many small keys it has, which is how many small-key doors,
/// and whether it has a boss key.
pub const DUNGEONS: [(&Name, i32, bool); 8] = [
    (flow::BOTTOM_OF_THE_WELL, 2, false),
    (flag::FOREST_TEMPLE, 5, true),
    (flag::FIRE_TEMPLE, 8, true),
    (flag::WATER_TEMPLE, 4, true),
    (flag::SHADOW_TEMPLE, 5, true),
    (flag::SPIRIT_TEMPLE, 5, true),
    (flag::GERUDO_TRAINING_GROUND, 9, false),
    (flag::INSIDE_GANON_CASTLE, 1, true),
];

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

/// The new-game defaults that fill the counters and boss key flags of
/// every dungeon, and the Fortress's counter when `fortress`.
pub fn defaults(dungeons: bool, fortress: bool) -> Vec<String> {
    let mut edits = Vec::new();
    for (name, small, boss) in DUNGEONS.iter().filter(|_| dungeons) {
        edits.push(format!(
            "DungeonInfo.{name}.{}={small}",
            flag::SMALL_KEY_NUM
        ));
        if *boss {
            edits.push(format!("DungeonInfo.{name}.IsGetBossKey=true"));
        }
    }
    if fortress {
        let (name, small) = FORTRESS;
        edits.push(format!(
            "DungeonInfo.{name}.{}={small}",
            flag::SMALL_KEY_NUM
        ));
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;
    use crate::known::{actor, flag, flow, item};
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
    fn a_key_chest_brings_a_red_rupee() {
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
            assert_eq!(chests(BOSS), i32::from(boss), "{name}");
        }
        assert_eq!(held.len(), 43);
    }
}
