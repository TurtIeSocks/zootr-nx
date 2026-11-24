//! The scrub spots: which placements are scrubs, what each one sells in the
//! vanilla game, and which of them a seed edits.

use std::collections::HashMap;

use super::{FLOW, HOLDER};
use crate::checks::{Drop, ItemCheck};
use crate::flows;
use crate::gifts;
use crate::known::{Name, actor, flag, flow, item};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Branch {
    /// Sells once through the pouch root, picked by the placement's item.
    Pouch,
    /// Hyrule Field's heart piece, sold at the end of its riddle talk.
    Secret,
    /// Sold consumables on repeat. As a check, it runs its own entry into
    /// the pouch root `ROOT`.
    Shop,
}

pub struct Scrub {
    /// The entry point the placement's vanilla talk runs, for a one-time
    /// scrub.
    pub talk: &'static Name,
    /// Banc folder, for the spoiler.
    pub area: &'static Name,
    /// Tells the scrubs of one area apart.
    pub name: &'static str,
    /// Every placement at the spot, child first. The first is the check's id.
    pub placements: &'static [u64],
    pub branch: Branch,
    pub item: &'static Name,
    /// The Bool the vanilla sale sets, which spots may share. A repeat spot
    /// sells forever and has none.
    pub sold: Option<&'static Name>,
    /// Whether the placements run their talk entry when hit, too.
    pub attacked: bool,
}

/// The empty name, which no entry point has: a scrub that talks to no one.
const NO_TALK: &Name = &crate::n!(0xcbf29ce484222325);
/// What a repeat spot brings to the pool, since its ammo can't sit in a
/// chest unguarded.
pub(super) const RED: &Name = actor::T_BOX_RUPEE_RED;

const fn shop(
    area: &'static Name,
    name: &'static str,
    placements: &'static [u64],
    attacked: bool,
) -> Scrub {
    Scrub {
        talk: NO_TALK,
        area,
        name,
        placements,
        branch: Branch::Shop,
        item: RED,
        sold: None,
        attacked,
    }
}

const fn pouch(
    area: &'static Name,
    name: &'static str,
    placements: &'static [u64],
    talk: &'static Name,
    item: &'static Name,
    sold: &'static Name,
    attacked: bool,
) -> Scrub {
    Scrub {
        talk,
        area,
        name,
        placements,
        branch: Branch::Pouch,
        item,
        sold: Some(sold),
        attacked,
    }
}

pub const SCRUBS: [Scrub; 15] = [
    pouch(
        flag::KOKIRI_FOREST,
        "sticks",
        &[182031956235860994, 7488448334396031424],
        flag::ENEMY_SCRUB_BUSINESS_DEKU_STICK_POUCH_01,
        item::STICK_BAG_MEDIUM,
        flag::LOST_WOOD_ENEMY_SCRUB_BUSINESS_DEKU_STICK_POUCH_01,
        false,
    ),
    pouch(
        flag::LOST_WOOD,
        "sticks",
        &[5406759280757349546, 11192313919554167152],
        flag::ENEMY_SCRUB_BUSINESS_DEKU_STICK_POUCH_01,
        item::STICK_BAG_MEDIUM,
        flag::LOST_WOOD_ENEMY_SCRUB_BUSINESS_DEKU_STICK_POUCH_01,
        false,
    ),
    pouch(
        flag::LOST_WOOD,
        "nuts",
        &[4328741615941752705, 5764461665617243979],
        flag::ENEMY_SCRUB_BUSINESS_DEKU_NUTS_POUCH_01,
        item::NUT_BAG_MEDIUM,
        flag::LOSTWOOD_ENEMY_SCRUB_BUSINESS_DEKU_NUTS_POUCH_01,
        false,
    ),
    pouch(
        flag::DEATH_MOUNTAIN_CRATER,
        "heart piece",
        &[2518138296199711695],
        actor::ENEMY_SCRUB_BUSINESS_HEART_02,
        actor::OBJ_HEART_PIECE,
        flag::DEATH_MOUNTAIN_ENEMY_SCRUB_BUSINESS_HEART_PIECE_01,
        true,
    ),
    Scrub {
        talk: NO_TALK,
        area: flag::HYRULE_FIELD,
        name: "heart piece",
        placements: &[10364468093017215295, 1888007325063123154],
        branch: Branch::Secret,
        item: actor::OBJ_HEART_PIECE,
        sold: Some(flag::HYRULE_FIELD_ENEMY_SCRUB_BUSINESS_HEART_PIECE_01),
        attacked: false,
    },
    shop(flag::HYRULE_FIELD, "seeds", &[12901354557574090016], false),
    shop(flag::HYRULE_FIELD, "nuts", &[17418042722045679657], false),
    shop(flag::HYRULE_FIELD, "bombs", &[16672855544778411393], false),
    shop(
        flag::LOST_WOOD,
        "rock grotto",
        &[2214083465643671502, 14619830301496284497],
        false,
    ),
    shop(
        flag::LOST_WOOD,
        "potions",
        &[15048088372638595204, 11385057993574159532],
        true,
    ),
    shop(flag::LOST_WOOD, "bug buyer", &[2569517939889411425], true),
    shop(flag::DODONGO_CAVERN, "", &[7388739057158296701], true),
    shop(
        flag::INSIDE_JABU_JABU_BELLY,
        "",
        &[9496373148524757782],
        true,
    ),
    shop(flow::BOTTOM_OF_THE_WELL, "", &[15945446340598299551], false),
    shop(flag::SHADOW_TEMPLE, "", &[8721287453021850502], false),
];

impl Scrub {
    pub fn hash(&self) -> u64 {
        self.placements[0]
    }

    /// The Bool set once Link buys here, shared by every copy of the spot.
    pub(super) fn flag(&self) -> String {
        format!("Rando_ScrubSold_{}", self.hash())
    }

    /// The Bool set once Link buys `item` here: the spot's own when the seed
    /// edits its sale, else the vanilla sale's.
    pub fn sold(&self, item: &str) -> Option<String> {
        match self.branch != Branch::Secret && self.edited(item) {
            true => Some(self.flag()),
            false => self.sold.map(|f| f.to_string()),
        }
    }

    fn holder(&self) -> String {
        match self.name {
            "" => HOLDER.into(),
            name => format!("{HOLDER}, {name}"),
        }
    }

    /// Whether the seed's `item` means editing it: always for a repeat
    /// spot, which mustn't sell its ammo.
    fn edited(&self, item: &str) -> bool {
        self.branch == Branch::Shop || item != self.item
    }
}

/// One check per scrub.
pub fn checks() -> Vec<ItemCheck> {
    SCRUBS
        .iter()
        .map(|s| gifts::check(flows::rel(FLOW), s.area, s.hash(), s.holder(), s.item))
        .collect()
}

pub fn of(check: &ItemCheck) -> Option<&'static Scrub> {
    at(check.hash)
}

/// The scrub whose check is `hash`.
pub fn at(hash: u64) -> Option<&'static Scrub> {
    SCRUBS.iter().find(|s| s.hash() == hash)
}

/// The families whose items have no get animation: songs, medallions and
/// spiritual stones. The vanilla sale shows the item in Link's hands before
/// handing it over, and the talk of a scrub holding one of these never
/// started, so they take the plain grant and a talk that never names them
/// in the placement.
const PLAIN: [&str; 3] = ["Item_Melody_", "Item_Medal_", "Item_Stone_"];

/// Whether `item` sells through the plain grant.
pub fn plain(item: &str) -> bool {
    PLAIN.iter().any(|family| item.starts_with(family))
}

/// The scrubs to edit, each with the item it sells.
pub(super) fn edits(drops: &HashMap<u64, Drop>) -> impl Iterator<Item = (&'static Scrub, &str)> {
    SCRUBS
        .iter()
        .map(|s| (s, gifts::item_at(drops, s.hash(), s.item)))
        .filter(|(s, item)| s.edited(item))
}
