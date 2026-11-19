//! Equipment for both ages. The pause menu's Equipment page greys out the
//! other age's gear by two flags per item in its table, and that's the
//! only limit: unset, either age equips and uses it. See the spec,
//! `superpowers/specs/2026-11-18-equip-any-age-design.md`.

use anyhow::{Result, ensure};
use ounce_formats::{Byml, BymlDoc, BymlMap, ModBuilder, Romfs};

use crate::game::POUCH_TABLE;
use crate::known::{Name, field};

/// The flags, each age's.
const AGES: [&Name; 2] = [field::IS_USABLE_CHILD, field::IS_USABLE_ADULT];

/// Whether `actor` is usable at each age, as `AGES` lists them. A missing
/// flag is false.
fn ages(actor: &Byml) -> [bool; 2] {
    AGES.map(|flag| {
        let flag = actor.as_map().ok().and_then(|m| m.get(flag.get()));
        flag.and_then(|f| f.as_bool().ok()) == Some(true)
    })
}

/// A slot that holds one actor per age, like the Kokiri tunic: unlocked,
/// the menu would show whichever comes first, age-blind.
fn age_pair(actors: &[Byml]) -> bool {
    let has = |want: [bool; 2]| actors.iter().any(|a| ages(a) == want);
    has([true, false]) && has([false, true])
}

/// Each actor of the Equipment page's slots, past the age pairs.
fn unlockable(root: &mut Byml) -> Result<Vec<&mut BymlMap>> {
    let Some(Byml::Array(slots)) = root.as_mut_map()?.get_mut(field::EQUIPMENT_ARRAY.get()) else {
        return Ok(vec![]);
    };
    let actors = slots.iter_mut().filter_map(|slot| {
        let Ok(slot) = slot.as_mut_map() else {
            return None;
        };
        match slot.get_mut(field::ACTOR.get()) {
            Some(Byml::Array(actors)) if !age_pair(actors) => Some(actors),
            _ => None,
        }
    });
    Ok(actors
        .flatten()
        .filter_map(|a| a.as_mut_map().ok())
        .collect())
}

/// The table with every unlockable actor usable at both ages, and how
/// many it changed.
fn unlock(table: &[u8]) -> Result<(Vec<u8>, usize)> {
    let mut doc = BymlDoc::parse(table)?;
    let mut changed = 0;
    for actor in unlockable(&mut doc.root)? {
        let before = ages(&Byml::Map(actor.clone()));
        for flag in AGES {
            actor.insert(flag.get().into(), Byml::Bool(true));
        }
        changed += usize::from(before != [true, true]);
    }
    Ok((doc.to_binary(), changed))
}

/// Write the table with the Equipment page open to both ages.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let (table, changed) = unlock(&romfs.read(POUCH_TABLE)?)?;
    ensure!(
        changed > 0,
        "no Equipment page item in the pause menu's table is limited to one age"
    );
    builder.write(POUCH_TABLE, &table)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Equipment page's slots, as the table at `bytes` has them.
    fn slots(bytes: &[u8]) -> Vec<Byml> {
        let root = BymlDoc::parse(bytes).unwrap().root;
        let slots = root.as_map().unwrap()[field::EQUIPMENT_ARRAY.get()].clone();
        slots.as_array().unwrap().to_vec()
    }

    fn actors(slot: &Byml) -> &[Byml] {
        slot.as_map().unwrap()[field::ACTOR.get()]
            .as_array()
            .unwrap()
    }

    #[test]
    fn every_slot_but_the_age_pairs_opens_to_both_ages() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (table, changed) = unlock(&vanilla).unwrap();
        let (before, after) = (slots(&vanilla), slots(&table));
        assert_eq!(before.len(), after.len());
        let pairs = before.iter().filter(|s| age_pair(actors(s))).count();
        assert_eq!(pairs, 1, "the Kokiri tunic is the page's one age pair");
        let mut limited = 0;
        for (was, now) in before.iter().zip(&after) {
            if age_pair(actors(was)) {
                assert_eq!(was, now, "an age pair is left as it was");
                continue;
            }
            limited += actors(was)
                .iter()
                .filter(|a| ages(a) != [true, true])
                .count();
            assert!(actors(now).iter().all(|a| ages(a) == [true, true]));
        }
        assert_eq!(changed, limited);
        assert!(changed > 0);
    }

    #[test]
    fn the_rest_of_the_table_is_untouched() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (table, _) = unlock(&vanilla).unwrap();
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
