//! Maps and compasses as copies, one each per dungeon, so either can land
//! anywhere (EIE #3). The game's own map credits the dungeon Link stands in,
//! so found anywhere else it would mark the wrong one. A copy answers to its
//! own name, its pouch row a plain item, and sets its dungeon's flag as it's
//! found: a chest's own event sets it (`chest_levels`), the hook does from
//! any other holder.

use std::sync::LazyLock;

use anyhow::{Result, ensure};
use ounce_formats::{ActorPack, ModBuilder, Romfs};

use crate::checks::ItemCheck;
use crate::key_tints::{self, MADE, Sets, TEXT_PARTS};
use crate::known::{Name, flag, flow, item};
use crate::standins::Tables;
use crate::text::{CopyLine, Named, Says};

/// The dungeons with a map and compass. Each name is both the area its
/// chests are in and its `DungeonInfo` member.
pub const DUNGEONS: [&Name; 10] = [
    flag::INSIDE_THE_DEKU_TREE,
    flag::DODONGO_CAVERN,
    flag::INSIDE_JABU_JABU_BELLY,
    flag::FOREST_TEMPLE,
    flag::FIRE_TEMPLE,
    flag::WATER_TEMPLE,
    flag::SHADOW_TEMPLE,
    flag::SPIRIT_TEMPLE,
    flow::BOTTOM_OF_THE_WELL,
    flag::ICE_CAVERN,
];

/// A map or a compass: the game's item it copies, the copy's name part and
/// the `DungeonInfo` field it sets.
#[derive(Clone, Copy)]
struct Kind {
    template: &'static Name,
    part: &'static str,
    field: &'static str,
    text: CopyText,
}

type CopyText = [(&'static str, bool); 3];

const KINDS: [Kind; 2] = [
    Kind {
        template: item::DUNGEON_MAP,
        part: "Map",
        field: "IsGetMap",
        text: [
            ("{} Map", true),
            ("You found {the} {} map!", true),
            ("It shows every room of {the} {}.", false),
        ],
    },
    Kind {
        template: item::DUNGEON_COMPASS,
        part: "Compass",
        field: "IsGetCompass",
        text: [
            ("{} Compass", true),
            ("You found {the} {} compass!", true),
            ("It finds the treasure chests of {the} {}.", false),
        ],
    },
];

/// Dungeon `n`'s copy of `kind`, counting from 1.
fn name(kind: &Kind, n: usize) -> String {
    format!("{MADE}{}{n:02}", kind.part)
}

/// Every copy, with its kind and dungeon number.
static ALL: LazyLock<Vec<(String, Kind, usize)>> = LazyLock::new(|| {
    let n = 1..=DUNGEONS.len();
    (n.flat_map(|n| KINDS.map(|k| (name(&k, n), k, n)))).collect()
});

/// Every copy, by actor.
pub fn copies() -> impl Iterator<Item = String> {
    ALL.iter().map(|(c, ..)| c.clone())
}

/// Whether `item` is a map or compass copy.
pub fn is_copy(item: &str) -> bool {
    ALL.iter().any(|(c, ..)| c == item)
}

/// The copy `check`, a map or compass chest, brings to the pool: its own
/// dungeon's. `None` for any other check.
pub fn pooled(check: &ItemCheck) -> Option<&'static str> {
    let n = DUNGEONS.iter().position(|d| **d == *check.area)? + 1;
    let kind = KINDS.iter().position(|k| *k.template == *check.item)?;
    ALL.iter()
        .find(|(_, k, m)| *m == n && k.part == KINDS[kind].part)
        .map(|(c, ..)| c.as_str())
}

/// The game's item `item` copies, if it's a copy.
pub fn template(item: &str) -> Option<&'static Name> {
    ALL.iter()
        .find(|(c, ..)| c == item)
        .map(|(_, k, _)| k.template)
}

/// What each copy sets as it's found: its dungeon's flag.
pub fn sets() -> Vec<(String, Sets)> {
    (ALL.iter())
        .map(|(c, k, n)| {
            let flag = format!("DungeonInfo.{}.{}", DUNGEONS[n - 1].get(), k.field);
            (c.clone(), Sets::Bool(flag))
        })
        .collect()
}

/// A copy's name after its dungeon's, `area` naming one by its id.
pub fn copy_name(item: &str, area: impl Fn(&str) -> String) -> Option<String> {
    let (_, k, n) = ALL.iter().find(|(c, ..)| c == item)?;
    Some(format!("{} {}", area(DUNGEONS[n - 1]), k.part))
}

/// The label naming `item`, if it's a copy.
pub fn name_label(item: &str) -> Option<String> {
    is_copy(item).then(|| format!("{item}{}", TEXT_PARTS[0]))
}

/// The lines each copy's get shows, the game's map or compass lines
/// naming its dungeon.
pub fn captions() -> Vec<CopyLine> {
    (ALL.iter())
        .flat_map(|(copy, k, n)| {
            let place = DUNGEONS[n - 1].get();
            TEXT_PARTS
                .iter()
                .zip(k.text)
                .map(move |(part, (english, bracketed))| CopyLine {
                    label: format!("{copy}{part}"),
                    sources: vec![format!("{}{part}", k.template.get())],
                    says: Says::Named(Named {
                        place,
                        english,
                        bracketed,
                    }),
                })
        })
        .collect()
}

/// The pack a copy is built as, as [`write`] writes it, for a holder that
/// stands in for it. `None` for any other item.
pub fn copy_pack(romfs: &Romfs, item: &str) -> Result<Option<ActorPack>> {
    let Some(template) = template(item) else {
        return Ok(None);
    };
    let template = ActorPack::load(romfs, template)?;
    key_tints::own_pack(&template, item, &template.name).map(Some)
}

/// Writes every copy, each showing its template's model, its pouch row a
/// plain item: the template's keeps a pickup from handing it over.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    for kind in KINDS {
        let template = ActorPack::load(romfs, kind.template)?;
        for n in 1..=DUNGEONS.len() {
            key_tints::copy(
                builder,
                tables,
                &template,
                &name(&kind, n),
                &template.name,
                true,
            )?;
        }
    }
    let copies: Vec<String> = ALL.iter().map(|(c, ..)| c.clone()).collect();
    let changed = tables.plain_deal(&copies)?;
    ensure!(
        changed == copies.len(),
        "{changed} copy rows of {}",
        copies.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_copy_sets_a_flag_the_game_keeps() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = crate::gamedata::GameData::load(&romfs).unwrap();
        for (copy, set) in sets() {
            let Sets::Bool(flag) = set else {
                panic!("{copy} sets no Bool");
            };
            gd.info(&flag).unwrap().check(&flag, "true").unwrap();
        }
    }

    #[test]
    fn each_dungeon_has_its_own_map_and_compass() {
        assert_eq!(ALL.len(), 20);
        let [map, compass] = KINDS;
        assert!(is_copy(&name(&map, 1)) && is_copy(&name(&compass, 10)));
        assert!(!is_copy(&name(&map, 11)));
    }
}
