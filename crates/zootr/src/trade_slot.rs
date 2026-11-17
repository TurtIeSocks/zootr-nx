//! The pause menu's trade slot, as the hook cycles it: the Items page's
//! `ItemArray` entry whose `SpecialDeal` is Biggoron's sword challenge
//! (`UI/Pouch/PouchTable`). Its items are in the table's own order, each
//! with the age that uses it. See `docs/research/in-game-ui.md`.

use ounce_formats::murmur3;
use zootr_hook::SlotSpec;
use zootr_hook::ui::Age;

use crate::known::{Name, flag, item};

/// The pause menu's table, which the tests check the slot against.
pub const FILES: [&str; 1] = ["UI/Pouch/PouchTable/PouchTable.game__ui__PouchTable.bgyml"];
/// The slot's cell in the Items page's list.
const CELL: u8 = 22;
const CHILD: u8 = Age::Child as u8;
const ADULT: u8 = Age::Adult as u8;
/// Latest in each chain first, as the table lists them.
const ITEMS: [(&Name, u8); 14] = [
    (item::CLAIM_CHECK, ADULT),
    (item::EYE_DROP, ADULT),
    (item::EYEBALL_FROG, ADULT),
    (item::PRESCRIPTION, ADULT),
    (item::BROKEN_SWORD, ADULT),
    (item::POACHER_SAW, ADULT),
    (item::ODD_POWDER, ADULT),
    (item::ODD_MUSHROOM, ADULT),
    (item::COJIRO_CUCCO, ADULT),
    (item::POCKET_CUCCO, ADULT),
    (item::POCKET_EGG, ADULT),
    (item::ZELDA_LETTER, CHILD),
    (item::ALARM_CUCCO, CHILD),
    (item::WEIRD_EGG, CHILD),
];

/// What the cell setter takes with the item at table position `i`. The
/// game's item kind list runs Weird Egg to Claim Check, the table's
/// reverse, and the setter takes a kind less one.
fn kind(i: usize) -> u8 {
    44 - i as u8
}

/// Each item as the table writes it: its name, ages and kind.
pub fn actors() -> Vec<(&'static str, u8, u8)> {
    (ITEMS.iter().enumerate())
        .map(|(i, &(name, ages))| (name.get(), ages, kind(i)))
        .collect()
}

/// The slot for the table, borrowing `actors`.
pub fn spec<'a>(actors: &'a [(&'a str, u8, u8)]) -> SlotSpec<'a> {
    SlotSpec {
        age_flag: murmur3(flag::SEQUENCE_IS_CHILDHOOD.as_bytes()),
        cell: CELL,
        actors,
    }
}

#[cfg(test)]
mod tests {
    use ounce_formats::{Byml, BymlDoc};

    use super::*;
    use crate::known::field;

    #[test]
    fn the_items_and_ages_are_the_game_s_in_its_order() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let doc = BymlDoc::parse(&romfs.read(FILES[0]).unwrap()).unwrap();
        let get = |m: &'_ Byml, key: &Name| m.as_map().unwrap().get(&**key).cloned();
        let slots = get(&doc.root, field::ITEM_ARRAY).unwrap();
        let deal = |s: &&Byml| {
            get(s, field::SPECIAL_DEAL)
                == Some(Byml::String(field::SWORD_BIG_GORON_CHALLENGE.get().into()))
        };
        let slots = slots.as_array().unwrap();
        let cell = slots.iter().position(|s| deal(&s)).unwrap();
        assert_eq!(cell, usize::from(CELL));
        let usable = |a: &Byml, f: &Name| get(a, f) == Some(Byml::Bool(true));
        let game: Vec<_> = get(&slots[cell], field::ACTOR)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                let name = get(a, field::ACTOR_SHORT).unwrap();
                let ages = [
                    (field::IS_USABLE_CHILD, CHILD),
                    (field::IS_USABLE_ADULT, ADULT),
                ]
                .into_iter()
                .filter(|&(f, _)| usable(a, f))
                .fold(0, |ages, (_, age)| ages | age);
                (name.as_string().unwrap().to_string(), ages)
            })
            .collect();
        let ours: Vec<_> = actors()
            .iter()
            .map(|&(n, a, _)| (n.to_string(), a))
            .collect();
        assert_eq!(game, ours);
    }

    #[test]
    fn kinds_run_from_the_claim_check_down_to_the_weird_egg() {
        let kinds: Vec<_> = (0..ITEMS.len()).map(kind).collect();
        assert_eq!((kinds[0], kinds[13]), (44, 31));
    }
}
