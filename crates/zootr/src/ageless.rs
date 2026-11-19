//! Swords and shields for both ages. The pause menu's Equipment page greys
//! out the other age's swords and shields by two flags per item in its
//! table, and that's their only limit: unset, either age equips and uses
//! them. Tunics and boots stay as they are, since they dress adult Link's
//! model and do nothing on a child. See the spec,
//! `superpowers/specs/2026-11-18-equip-any-age-design.md`.

use anyhow::{Result, ensure};
use ounce_formats::{Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_file};
use roead::sarc::Sarc;

use crate::game::POUCH_TABLE;
use crate::known::{Name, field};

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

/// The table with every sword and shield usable at both ages, and how many
/// it changed.
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

/// Write the table with the Equipment page's swords and shields open to
/// both ages.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let (table, changed) = unlock(&romfs.read(POUCH_TABLE)?, |n| is_weapon(romfs, n))?;
    ensure!(
        changed > 0,
        "no sword or shield in the pause menu's table is limited to one age"
    );
    builder.write(POUCH_TABLE, &table)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (table, changed) = unlock(&vanilla, |n| is_weapon(&romfs, n)).unwrap();
        let (before, after) = (actors(&vanilla), actors(&table));
        assert_eq!(before.len(), after.len());
        let mut swords_and_shields = 0;
        for ((name, was), (_, now)) in before.iter().zip(&after) {
            match is_weapon(&romfs, name) {
                true => {
                    swords_and_shields += 1;
                    assert_eq!(*now, [true, true], "{name}");
                }
                false => assert_eq!(was, now, "{name} isn't a sword or shield"),
            }
        }
        // Biggoron's, the Giant's Knife, its broken one, Master, Kokiri,
        // and the Mirror, Hylian and Deku Shields.
        assert_eq!(swords_and_shields, 8);
        let limited = before
            .iter()
            .filter(|(n, a)| is_weapon(&romfs, n) && *a != [true, true]);
        assert_eq!(changed, limited.count());
        assert!(changed > 0);
    }

    #[test]
    fn the_rest_of_the_table_is_untouched() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (table, _) = unlock(&vanilla, |n| is_weapon(&romfs, n)).unwrap();
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
