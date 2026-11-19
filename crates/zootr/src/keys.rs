//! Keys Link starts with, so the chests that held them become checks. A
//! small-key door spends its dungeon's counter and a boss door reads its
//! boss key flag, so a new game that fills both opens every door.

use std::sync::LazyLock;

use anyhow::{Context, Result};

use crate::checks::ItemCheck;
use crate::key_tints;
use crate::known::{Name, actor, flag, flow, item, map};
use crate::map_copies;
use crate::rules::Need;
use crate::settings::{DungeonKeys, FortressKeys};

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

/// How many small key copies Keysanity deals dungeon `n`: one for each of
/// its keys, the chests' and the two lying in the open (Bottom of the
/// Well's and the Shadow Temple's), and logic asks for them all.
fn copies_of(n: usize) -> u32 {
    DUNGEONS[n - 1].1 as u32
}

/// The dungeon whose maps `rel`, a Banc file, is under, by its number in
/// [`DUNGEONS`], counting from 1.
pub fn dungeon_of(rel: &str) -> Option<usize> {
    let folder = rel.strip_prefix("Banc/")?.split('/').next()?;
    DUNGEONS
        .iter()
        .position(|(d, ..)| *d == folder)
        .map(|i| i + 1)
}

/// The two keys lying in the open, by placement, and the Bool their
/// room's logic module sets once Link takes what lies there. The module
/// deletes the spot's actor once it's set, whatever the seed put there.
pub const LOOSE: [(u64, &Name); 2] = [
    (
        17649154319341333603,
        flag::BOTTOM_OF_THE_WELL_LEVEL_ROOM_03_GET_SMALL_KEY,
    ),
    (
        12562389953863361839,
        flag::SHADOW_TEMPLE_LEVEL_ROOM_13_GET_KEY,
    ),
];

/// The Bool set once the loose key spot placed as `hash` is taken.
pub fn loose_taken(hash: u64) -> Option<&'static str> {
    LOOSE.iter().find(|(h, _)| *h == hash).map(|(_, f)| f.get())
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

/// Whether `item` is a ring, small key or boss key copy, the Fortress key
/// copy, or a map or compass copy, which the hook credits as Link gets it.
pub fn is_shuffled(item: &str) -> bool {
    SHUFFLED.iter().any(|s| s == item) || item == key_tints::fortress() || map_copies::is_copy(item)
}

/// With Keysanity, every small key copy, one for each key chest, and
/// every boss key.
pub static SINGLES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let n = 1..=DUNGEONS.len();
    let smalls = n.clone().flat_map(|n| {
        let ring = key_tints::ring(n);
        (0..copies_of(n)).map(move |_| ring.clone())
    });
    smalls
        .chain(n.filter(|&n| has_boss(n)).map(key_tints::boss))
        .collect()
});

/// With Fortress Keysanity, the Fortress key copy, one for each thief.
pub static FORTRESS_COPIES: LazyLock<Vec<String>> =
    LazyLock::new(|| vec![key_tints::fortress(); FORTRESS.1 as usize]);

/// Each copy `keys` and `fortress` deal takes the place of a key chest's or
/// a thief's Red Rupee, but for those `placed` already holds.
pub fn deal_copies(
    items: &mut [&str],
    keys: DungeonKeys,
    fortress: FortressKeys,
    placed: &[&str],
) -> Result<()> {
    let copies: &[String] = match keys {
        DungeonKeys::Rings => &SHUFFLED,
        DungeonKeys::Singles => &SINGLES,
        DungeonKeys::Vanilla | DungeonKeys::Start => &[],
    };
    let thieves: &[String] = match fortress.copies() {
        true => &FORTRESS_COPIES,
        false => &[],
    };
    let mut placed = placed.to_vec();
    for key in copies.iter().chain(thieves) {
        if let Some(i) = placed.iter().position(|p| *p == key.as_str()) {
            placed.swap_remove(i);
            continue;
        }
        let rupee = items.iter().position(|i| *INSTEAD == **i);
        let rupee = rupee.context("too few key chests for the dungeons' keys")?;
        items[rupee] = key;
    }
    Ok(())
}

/// Dungeon `n`'s small keys as `keys` deals them: its ring, or every copy.
fn smalls(n: usize, keys: DungeonKeys) -> Option<Need> {
    let actors = vec![key_tints::ring(n)];
    match keys {
        DungeonKeys::Rings => Some(Need::Items { actors, count: 1 }),
        DungeonKeys::Singles => Some(Need::Items {
            actors,
            count: copies_of(n),
        }),
        DungeonKeys::Vanilla | DungeonKeys::Start => None,
    }
}

/// With keys dealt as copies, what logic adds to a check in `rel`, a Banc
/// file: all its dungeon's small keys. Stricter than the game, which asks
/// for them door by door, but safe in any order Link spends them.
pub fn dungeon_need(rel: &str, keys: DungeonKeys) -> Option<Need> {
    smalls(dungeon_of(rel)?, keys)
}

/// With the Fortress keys dealt as copies, what logic adds to a check whose
/// vanilla item is `item`: all four for the Gerudo membership card, given
/// once the fourth carpenter is free.
pub fn fortress_need(item: &str, fortress: FortressKeys) -> Option<Need> {
    (fortress.copies() && item == crate::gerudo::CARD).then(|| Need::Items {
        actors: vec![key_tints::fortress()],
        count: FORTRESS.1 as u32,
    })
}

/// With keys dealt as copies, what logic adds to the way into `room`, a
/// boss room: its dungeon's boss key and small keys.
pub fn boss_door_need(room: &str, keys: DungeonKeys) -> Option<Need> {
    let i = DUNGEONS
        .iter()
        .position(|(.., r)| r.is_some_and(|r| *r == *room))?;
    let small = smalls(i + 1, keys)?;
    Some(Need::All(vec![holds(key_tints::boss(i + 1)), small]))
}

/// A ring's, small key's or boss key copy's name, after its dungeon's:
/// `area` names one by its id. The ring copy is a single small key with
/// Keysanity.
pub fn copy_name(actor: &str, area: impl Fn(&str) -> String, keys: DungeonKeys) -> Option<String> {
    if let Some(name) = map_copies::copy_name(actor, &area) {
        return Some(name);
    }
    if actor == key_tints::fortress() {
        return Some(format!("{} Small Key", area(FORTRESS.0)));
    }
    DUNGEONS.iter().enumerate().find_map(|(i, (dungeon, ..))| {
        let n = i + 1;
        let kind = match actor {
            a if a == key_tints::ring(n) && keys == DungeonKeys::Singles => "Small Key",
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

/// What `check` brings to the pool: its item, `INSTEAD` for a key, or for
/// a map or compass its dungeon's copy.
pub fn pooled(check: &ItemCheck) -> &str {
    match is_key(&check.item) {
        true => INSTEAD,
        false => map_copies::pooled(check).unwrap_or(&check.item),
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
        deal_copies(&mut items, DungeonKeys::Rings, FortressKeys::Vanilla, &[]).unwrap();
        assert_eq!(items.iter().filter(|i| **i == red).count(), 1);
        assert!(SHUFFLED.iter().all(|k| items.contains(&k.as_str())));
        assert_eq!(SHUFFLED.len(), DUNGEONS.len() + 6);
        assert!(
            deal_copies(
                &mut [red; 3],
                DungeonKeys::Rings,
                FortressKeys::Vanilla,
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn keysanity_deals_a_copy_for_every_key_chest() {
        needs_names!();
        let red = INSTEAD.get();
        // The 37 small key chests, the 2 loose keys and the 6 boss key chests.
        assert_eq!(SINGLES.len(), 39 + 6);
        let mut items = vec![red; SINGLES.len()];
        deal_copies(&mut items, DungeonKeys::Singles, FortressKeys::Vanilla, &[]).unwrap();
        assert!(!items.contains(&red));
        let forest = DUNGEONS
            .iter()
            .position(|d| d.0 == flag::FOREST_TEMPLE)
            .unwrap();
        let ring = key_tints::ring(forest + 1);
        let held = items.iter().filter(|i| **i == ring).count() as u32;
        assert_eq!(held, copies_of(forest + 1));
        let mut start = vec![red; 2];
        deal_copies(&mut start, DungeonKeys::Start, FortressKeys::Start, &[]).unwrap();
        assert_eq!(start, [red; 2]);
    }

    #[test]
    fn loose_keys_are_the_two_lying_and_tick_by_what_their_module_sets() {
        needs_names!();
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let found = crate::pickups::LOOSE_KEYS.checks(&romfs).unwrap();
        let found: std::collections::BTreeSet<u64> = found.iter().map(|c| c.hash).collect();
        let listed = LOOSE.iter().map(|l| l.0).collect();
        assert_eq!(found, listed);
        let creators = crate::gates::creators(&romfs).unwrap();
        for (hash, taken) in LOOSE {
            let module = creators.iter().find(|c| c.hash == hash).unwrap();
            let rel = format!("Logic/{}.ainb", module.graph);
            let patch = ounce_formats::ainb::Patch::new(romfs.read(&rel).unwrap()).unwrap();
            let sets = patch.graph().nodes.iter().any(|n| {
                n.name == crate::known::field::TRIGGER_GAME_DATA_SET_BOOL_TRUE.get()
                    && n.str(crate::known::field::GAME_DATA_NAME_BOOL) == Some(taken.get())
            });
            assert!(sets, "{} never sets {}", module.graph, taken.get());
            assert_eq!(loose_taken(hash), Some(taken.get()));
        }
    }

    #[test]
    fn fortress_keysanity_deals_a_copy_for_every_thief_and_the_card_asks_for_all() {
        needs_names!();
        let red = INSTEAD.get();
        let mut items = vec![red; 5];
        deal_copies(&mut items, DungeonKeys::Vanilla, FortressKeys::Singles, &[]).unwrap();
        let copy = key_tints::fortress();
        assert_eq!(items.iter().filter(|i| **i == copy).count(), 4);
        assert_eq!(items.iter().filter(|i| **i == red).count(), 1);
        // A copy placed by hand is one of the four.
        let mut items = vec![red; 5];
        let placed = [copy.as_str()];
        deal_copies(
            &mut items,
            DungeonKeys::Vanilla,
            FortressKeys::Singles,
            &placed,
        )
        .unwrap();
        assert_eq!(items.iter().filter(|i| **i == copy).count(), 3);
        assert!(is_shuffled(&copy));
        let need = Need::Items {
            actors: vec![copy],
            count: 4,
        };
        let card = crate::gerudo::CARD.get();
        assert_eq!(fortress_need(card, FortressKeys::Singles), Some(need));
        assert_eq!(fortress_need(card, FortressKeys::Start), None);
        assert_eq!(
            fortress_need(item::HOOKSHOT.get(), FortressKeys::Singles),
            None
        );
        let area = |id: &str| format!("<{id}>");
        let name = copy_name(&key_tints::fortress(), area, DungeonKeys::Vanilla);
        assert_eq!(name, Some(format!("<{}> Small Key", FORTRESS.0)));
    }

    #[test]
    fn each_dungeon_s_key_checks_match_its_copies() {
        needs_names!();
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut checks = crate::checks::all(&romfs).unwrap();
        checks.extend(crate::pickups::LOOSE_KEYS.checks(&romfs).unwrap());
        let mut seen = std::collections::HashSet::new();
        let mut counts = [0u32; 8];
        for c in checks.iter().filter(|c| c.item == SMALL.get()) {
            if let Some(n) = dungeon_of(&c.file).filter(|_| seen.insert(c.hash)) {
                counts[n - 1] += 1;
            }
        }
        assert_eq!(counts, std::array::from_fn(|i| copies_of(i + 1)));
    }

    #[test]
    fn logic_asks_a_dungeon_s_checks_for_its_ring_and_its_boss_door_for_both() {
        needs_names!();
        let forest = format!("Banc/{}/Fake.bcett.byml.zs", flag::FOREST_TEMPLE);
        let n = dungeon_of(&forest).unwrap();
        let rings = DungeonKeys::Rings;
        let need = dungeon_need(&forest, rings).unwrap().to_string();
        assert_eq!(need, key_tints::ring(n));
        assert!(dungeon_need("Banc/FakeField/Fake.bcett.byml.zs", rings).is_none());
        assert!(dungeon_need(&forest, DungeonKeys::Start).is_none());
        let door = boss_door_need(map::PHANTOM_GANON_STAGE, rings)
            .unwrap()
            .to_string();
        assert!(door.contains(&key_tints::boss(n)) && door.contains(&key_tints::ring(n)));
        assert!(boss_door_need(map::GOHMA_STAGE, rings).is_none());
        // Keysanity asks for every copy the dungeon's chests hold.
        let all = dungeon_need(&forest, DungeonKeys::Singles).unwrap();
        let Need::Items { actors, count } = all else {
            panic!("{all}")
        };
        assert_eq!((actors, count), (vec![key_tints::ring(n)], copies_of(n)));
    }

    #[test]
    fn a_copy_is_named_after_its_dungeon() {
        let area = |id: &str| format!("<{id}>");
        let forest = DUNGEONS.iter().position(|d| d.0 == flag::FOREST_TEMPLE);
        let n = forest.unwrap() + 1;
        let named = |a: String| copy_name(&a, area, DungeonKeys::Rings);
        assert_eq!(
            named(key_tints::ring(n)).unwrap(),
            format!("<{}> Key Ring", flag::FOREST_TEMPLE)
        );
        assert_eq!(
            named(key_tints::boss(n)).unwrap(),
            format!("<{}> Boss Key", flag::FOREST_TEMPLE)
        );
        assert!(named(item::HOOKSHOT.to_string()).is_none());
        assert_eq!(
            copy_name(&key_tints::ring(n), area, DungeonKeys::Singles).unwrap(),
            format!("<{}> Small Key", flag::FOREST_TEMPLE)
        );
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
