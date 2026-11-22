//! One-handed swords and shields for both ages. The pause menu's
//! Equipment page greys out the other age's swords and shields by two
//! flags per item in its table, and that's their only limit: unset, either
//! age equips and uses them. Tunics and boots stay as they are, since they dress adult Link's
//! model and do nothing on a child, and so do the two-handed swords: a
//! child swings them only with the clips items for both ages lends him,
//! so that setting opens them ([`two_handed_opened`]). See the spec,
//! `superpowers/specs/2026-11-18-equip-any-age-design.md`.

use std::collections::HashSet;

use anyhow::{Result, ensure};
use ounce_formats::{Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_file};
use roead::sarc::Sarc;

use crate::game::POUCH_TABLE;
use crate::known::{Name, field};
use crate::standins::{POUCH_ACTOR_INFO, row_id};

/// The flags, each age's.
const AGES: [&Name; 2] = [field::IS_USABLE_CHILD, field::IS_USABLE_ADULT];
/// The components that make an actor a sword or a shield.
const WEAPONS: [&Name; 2] = [field::WEAPON_PARAM, field::SHIELD_PARAM];

/// Whether `actor` is usable at each age, as `AGES` lists them. A missing
/// flag is false.
fn ages(actor: &BymlMap) -> [bool; 2] {
    AGES.map(|flag| actor.get(flag.get()).and_then(|f| f.as_bool().ok()) == Some(true))
}

/// The actor's name, as the table gives it.
fn name(actor: &BymlMap) -> Option<&str> {
    actor
        .get(field::ACTOR_SHORT.get())?
        .as_string()
        .ok()
        .map(|s| s.as_str())
}

/// Whether the actor named `name` is a sword or a shield: its pack carries
/// one of `WEAPONS`.
fn is_weapon(romfs: &Romfs, name: &str) -> bool {
    let Ok(pack) = romfs.read(&pack_file(name)) else {
        return false;
    };
    let Ok(sarc) = Sarc::new(pack) else {
        return false;
    };
    let dirs = WEAPONS.map(|w| format!("Component/{w}/"));
    sarc.files().any(|f| {
        f.name()
            .is_some_and(|n| dirs.iter().any(|d| n.starts_with(d.as_str())))
    })
}

/// The two-handed swords' `WeaponType`. A child's own animations refuse
/// their swing (`research/two-handed-swords-child.md`).
const TWO_HANDED: &Name = &crate::n!(0xac8f0c8c2fcaf11d);

/// Each actor whose pouch row makes it a two-handed sword.
fn two_handed(romfs: &Romfs) -> Result<HashSet<String>> {
    let doc = BymlDoc::parse(&romfs.read(POUCH_ACTOR_INFO)?)?;
    let is = |row: &&Byml| {
        let kind = row
            .as_map()
            .ok()
            .and_then(|m| m.get(field::WEAPON_TYPE.get()));
        kind.and_then(|k| k.as_string().ok())
            .is_some_and(|k| k.as_str() == TWO_HANDED.get())
    };
    let names: HashSet<_> = doc
        .root
        .as_array()?
        .iter()
        .filter(is)
        .map(|r| row_id(r).to_owned())
        .collect();
    ensure!(
        !names.is_empty(),
        "no two-handed sword in {POUCH_ACTOR_INFO}"
    );
    Ok(names)
}

/// Whether the actor named `name` opens to both ages: a sword or shield,
/// but not a two-handed sword.
fn shared(romfs: &Romfs) -> Result<impl Fn(&str) -> bool + '_> {
    let two_handed = two_handed(romfs)?;
    Ok(move |name: &str| is_weapon(romfs, name) && !two_handed.contains(name))
}

/// Each actor on the Equipment page that `weapon` says is a sword or a
/// shield.
fn weapons<'a>(root: &'a mut Byml, weapon: &impl Fn(&str) -> bool) -> Result<Vec<&'a mut BymlMap>> {
    let Some(Byml::Array(slots)) = root.as_mut_map()?.get_mut(field::EQUIPMENT_ARRAY.get()) else {
        return Ok(vec![]);
    };
    let actors = slots.iter_mut().filter_map(|slot| {
        match slot.as_mut_map().ok()?.get_mut(field::ACTOR.get()) {
            Some(Byml::Array(actors)) => Some(actors),
            _ => None,
        }
    });
    Ok(actors
        .flatten()
        .filter_map(|a| a.as_mut_map().ok())
        .filter(|a| name(a).is_some_and(weapon))
        .collect())
}

/// The table with every actor `weapon` names usable at both ages, and how
/// many it changed.
fn unlock(table: &[u8], weapon: impl Fn(&str) -> bool) -> Result<(Vec<u8>, usize)> {
    let mut doc = BymlDoc::parse(table)?;
    let mut changed = 0;
    for actor in weapons(&mut doc.root, &weapon)? {
        changed += usize::from(ages(actor) != [true, true]);
        for flag in AGES {
            actor.insert(flag.get().into(), Byml::Bool(true));
        }
    }
    Ok((doc.to_binary(), changed))
}

/// The pause menu's table at `table` with the two-handed swords open to
/// both ages too.
pub fn two_handed_opened(romfs: &Romfs, table: &[u8]) -> Result<Vec<u8>> {
    let two_handed = two_handed(romfs)?;
    let (table, changed) = unlock(table, |name| two_handed.contains(name))?;
    ensure!(
        changed > 0,
        "no two-handed sword in the pause menu's table is limited to one age"
    );
    Ok(table)
}

/// Write the table with the Equipment page's swords and shields open to
/// both ages, all but the two-handed swords.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let (table, changed) = unlock(&builder.read(POUCH_TABLE)?, shared(romfs)?)?;
    ensure!(
        changed > 0,
        "no sword or shield in the pause menu's table is limited to one age"
    );
    builder.write(POUCH_TABLE, &table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    /// Each Equipment page actor in the table at `bytes`: its name and
    /// ages.
    fn actors(bytes: &[u8]) -> Vec<(String, [bool; 2])> {
        let root = BymlDoc::parse(bytes).unwrap().root;
        let slots = root.as_map().unwrap()[field::EQUIPMENT_ARRAY.get()]
            .as_array()
            .unwrap();
        slots
            .iter()
            .flat_map(|s| s.as_map().unwrap()[field::ACTOR.get()].as_array().unwrap())
            .map(|a| {
                let a = a.as_map().unwrap();
                (name(a).unwrap().to_owned(), ages(a))
            })
            .collect()
    }

    #[test]
    fn swords_and_shields_open_to_both_ages_and_nothing_else_moves() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let shared = shared(&romfs).unwrap();
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (table, changed) = unlock(&vanilla, &shared).unwrap();
        let (before, after) = (actors(&vanilla), actors(&table));
        assert_eq!(before.len(), after.len());
        let mut opened = 0;
        for ((name, was), (_, now)) in before.iter().zip(&after) {
            match shared(name) {
                true => {
                    opened += 1;
                    assert_eq!(*now, [true, true], "{name}");
                }
                false => assert_eq!(was, now, "{name} isn't shared"),
            }
        }
        // Master, Kokiri, and the Mirror, Hylian and Deku Shields.
        assert_eq!(opened, 5);
        let limited = before
            .iter()
            .filter(|(n, a)| shared(n) && *a != [true, true]);
        assert_eq!(changed, limited.count());
        assert!(changed > 0);
    }

    #[test]
    fn the_two_handed_swords_stay_adult() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (table, _) =
            unlock(&romfs.read(POUCH_TABLE).unwrap(), shared(&romfs).unwrap()).unwrap();
        let two_handed = [
            actor::WEAPON_SWORD_BIGGORON,
            actor::WEAPON_SWORD_GIANT,
            actor::WEAPON_SWORD_BROKEN_GIANT,
        ];
        for sword in two_handed {
            let ages = actors(&table).into_iter().find(|(n, _)| n == sword.get());
            assert_eq!(ages, Some((sword.to_string(), [false, true])), "{sword}");
        }
    }

    #[test]
    fn items_for_both_ages_opens_the_two_handed_swords_alone_and_in_either_order() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let two_handed = two_handed(&romfs).unwrap();
        let alone = two_handed_opened(&romfs, &vanilla).unwrap();
        let mut opened = 0;
        for ((name, was), (_, now)) in actors(&vanilla).iter().zip(&actors(&alone)) {
            match two_handed.contains(name) {
                true => {
                    opened += 1;
                    assert_eq!(*now, [true, true], "{name}");
                }
                false => assert_eq!(was, now, "{name} isn't two-handed"),
            }
        }
        // Biggoron's, the Giant's Knife and the broken knife.
        assert_eq!(opened, 3);
        let shared = shared(&romfs).unwrap();
        let weapons_first =
            two_handed_opened(&romfs, &unlock(&vanilla, &shared).unwrap().0).unwrap();
        let items_first = unlock(&alone, &shared).unwrap().0;
        assert_eq!(weapons_first, items_first);
    }

    #[test]
    fn the_rest_of_the_table_is_untouched() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (table, _) = unlock(&vanilla, shared(&romfs).unwrap()).unwrap();
        let mut was = BymlDoc::parse(&vanilla).unwrap().root;
        let mut now = BymlDoc::parse(&table).unwrap().root;
        for root in [&mut was, &mut now] {
            root.as_mut_map()
                .unwrap()
                .remove(field::EQUIPMENT_ARRAY.get());
        }
        assert_eq!(was, now);
    }
}
