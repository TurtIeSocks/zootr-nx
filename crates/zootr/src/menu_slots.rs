//! The pause menu's shared slots, as the hook cycles them: the trade slot,
//! the Items page's `ItemArray` entry whose `SpecialDeal` is Biggoron's
//! sword challenge, and the mask slot, the Equipment page's
//! `EquipmentArray` entry whose `SpecialDeal` is `Mask`
//! (`UI/Pouch/PouchTable`). Each slot's items are in the table's own
//! order, each with the ages that use it. See `docs/research/in-game-ui.md`.

use ounce_formats::murmur3;
use zootr_hook::SlotSpec;
use zootr_hook::ui::{Age, Page};

use crate::known::{Name, actor, field, flag, item};

const CHILD: u8 = Age::Child as u8;
const ADULT: u8 = Age::Adult as u8;
const BOTH: u8 = CHILD | ADULT;

/// A shared slot: its page, its entry in the page's list, its items with
/// their ages, and what the page draws each one by.
pub struct MenuSlot {
    page: Page,
    /// The page's list in the pouch table, and the slot's `SpecialDeal`.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the dump test checks the slot by them")
    )]
    list: &'static Name,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the dump test checks the slot by them")
    )]
    deal: &'static Name,
    /// The slot's cell in the page's list.
    cell: u8,
    items: &'static [(&'static Name, u8)],
    /// What the page draws the item at table position `i` by, see
    /// [`zootr_hook::Slot`].
    kind: fn(usize) -> u8,
}

/// The trade slot, latest in each chain first, as the table lists them.
/// The game's item kind list runs Weird Egg to Claim Check, the table's
/// reverse, and the cell setter takes a kind less one.
const TRADE: MenuSlot = MenuSlot {
    page: Page::Items,
    list: field::ITEM_ARRAY,
    deal: field::SWORD_BIG_GORON_CHALLENGE,
    cell: 22,
    items: &[
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
    ],
    kind: |i| 44 - i as u8,
};

/// The mask slot. Each mask's icon frame is its place in the Equipment
/// page's icon list, Keaton 12 to Gerudo 19.
const MASKS: MenuSlot = MenuSlot {
    page: Page::Equipment,
    list: field::EQUIPMENT_ARRAY,
    deal: field::MASK,
    cell: 12,
    items: &[
        (actor::ARMOR_CHILD_MASK_TRUTH, BOTH),
        (actor::ARMOR_CHILD_MASK_GORON, BOTH),
        (actor::ARMOR_CHILD_MASK_ZORA, BOTH),
        (actor::ARMOR_CHILD_MASK_GERUDO, BOTH),
        (actor::ARMOR_CHILD_MASK_BUNNY, BOTH),
        (actor::ARMOR_CHILD_MASK_SPOOKY, BOTH),
        (actor::ARMOR_CHILD_MASK_SKULL, BOTH),
        (actor::ARMOR_CHILD_MASK_KEATON, BOTH),
    ],
    kind: |i| [16, 17, 18, 19, 15, 14, 13, 12][i],
};

/// Every shared slot, in the table's order.
pub const ALL: [&MenuSlot; 2] = [&TRADE, &MASKS];

impl MenuSlot {
    /// Each item as the table writes it: its name, ages and kind.
    pub fn actors(&self) -> Vec<(&'static str, u8, u8)> {
        (self.items.iter().enumerate())
            .map(|(i, &(name, ages))| (name.get(), ages, (self.kind)(i)))
            .collect()
    }

    /// The slot for the table, borrowing `actors`.
    pub fn spec<'a>(&self, actors: &'a [(&'a str, u8, u8)]) -> SlotSpec<'a> {
        SlotSpec {
            page: self.page,
            age_flag: murmur3(flag::SEQUENCE_IS_CHILDHOOD.as_bytes()),
            cell: self.cell,
            actors,
        }
    }
}

#[cfg(test)]
mod tests {
    use ounce_formats::{Byml, BymlDoc};

    use super::*;

    #[test]
    fn the_items_and_ages_are_the_game_s_in_its_order() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let doc = BymlDoc::parse(&romfs.read(crate::game::POUCH_TABLE).unwrap()).unwrap();
        let get = |m: &'_ Byml, key: &Name| m.as_map().unwrap().get(&**key).cloned();
        for slot in ALL {
            let entries = get(&doc.root, slot.list).unwrap();
            let entries = entries.as_array().unwrap();
            let deal = |s: &&Byml| {
                get(s, field::SPECIAL_DEAL) == Some(Byml::String(slot.deal.get().into()))
            };
            let cell = entries.iter().position(|s| deal(&s)).unwrap();
            assert_eq!(cell, usize::from(slot.cell), "{}", slot.deal.get());
            let usable = |a: &Byml, f: &Name| get(a, f) == Some(Byml::Bool(true));
            let game: Vec<_> = get(&entries[cell], field::ACTOR)
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
            let ours: Vec<_> = (slot.actors().iter())
                .map(|&(n, a, _)| (n.to_string(), a))
                .collect();
            assert_eq!(game, ours);
        }
    }

    #[test]
    fn trade_kinds_run_from_the_claim_check_down_to_the_weird_egg() {
        let kinds: Vec<_> = TRADE.actors().iter().map(|a| a.2).collect();
        assert_eq!((kinds[0], kinds[13]), (44, 31));
    }

    #[test]
    fn each_mask_has_its_own_icon_frame() {
        let mut frames: Vec<_> = MASKS.actors().iter().map(|a| a.2).collect();
        frames.sort();
        assert_eq!(frames, (12..20).collect::<Vec<_>>());
    }
}
