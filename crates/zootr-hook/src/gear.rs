//! Boots and tunics for a child, in a seed with [`CHILD_GEAR`]: which of
//! the equipment slots' child refusals let him through, and which armor he
//! wears for each tunic.

use core::ffi::CStr;

use crate::table::fnv;

/// The header's flag for a seed where child Link wears the boots and
/// tunics: the equipment slots' child test lets their slots through
/// ([`gear_through`]) and he wears the seed's armor for a tunic
/// ([`child_armor`]).
pub const CHILD_GEAR: u32 = 64;
/// The equipment slot the tunics go in.
pub const ARMOR_SLOT: u32 = 8;
/// The equipment slot the boots go in.
pub const BOOTS_SLOT: u32 = 9;

/// The armor a child wears for each tunic, each as the [`fnv`] of the name
/// the tunic's slot holds and the actor the seed makes for him. The Zora
/// and Goron Tunics' slots name the adult's armor, a whole adult upper
/// body: his copies show his own cloth in the tunic's color. His Kokiri
/// Tunic's slot names its pickup model, no armor at all, so it never took
/// a tunic off: his copy of the adult's empty armor does.
pub static CHILD_ARMOR: [Armor; 3] = [
    Armor::new(0xf673_35a2, "Rando_Armor_Zora"),
    Armor::new(0x71ce_9e29, "Rando_Armor_Goron"),
    Armor::new(0xa433_3f57, "Rando_Armor_Kokiri"),
];

/// The [`fnv`] of the adult's Kokiri Tunic, his empty armor, which the
/// armor slot holds after an adult turns child in it.
pub const ADULT_KOKIRI: u32 = 0xedda_5997;

/// A tunic's armor for a child. Its name sits in the table itself, padded,
/// since a table of pointers would need relocating, which the hook can't.
pub struct Armor {
    /// The [`fnv`] of the name the tunic's slot holds.
    pub worn: u32,
    name: [u8; NAME],
}

/// The room for an armor's name and its NUL.
const NAME: usize = 24;

impl Armor {
    const fn new(worn: u32, name: &str) -> Self {
        let (from, mut to) = (name.as_bytes(), [0; NAME]);
        assert!(from.len() < NAME);
        let mut i = 0;
        while i < from.len() {
            to[i] = from[i];
            i += 1;
        }
        Self { worn, name: to }
    }

    /// The actor's name.
    pub fn name(&self) -> &CStr {
        // `new` leaves at least one NUL.
        CStr::from_bytes_until_nul(&self.name).unwrap_or_default()
    }
}

/// Whether the equipment slots' child test lets `slot` through for a
/// child in a seed with `flags`: the game refuses a child the armor, boots
/// and hand armor slots (8 to 10), so nothing of theirs is made on him.
/// [`CHILD_GEAR`] lets the tunics and boots through.
pub fn gear_through(slot: u32, flags: u32) -> bool {
    matches!(slot, ARMOR_SLOT | BOOTS_SLOT) && child_gear(flags)
}

/// The armor a child wears in place of `worn`, the name his tunic's slot
/// holds, in a seed with `flags`: its index in [`CHILD_ARMOR`], or `None`
/// to keep the game's.
pub fn child_armor(worn: &[u8], flags: u32) -> Option<usize> {
    let hash = fnv(worn);
    let found = CHILD_ARMOR.iter().position(|a| a.worn == hash);
    found.filter(|_| child_gear(flags))
}

/// Whether the equipment menu marks every Kokiri Tunic cell as worn,
/// rather than testing each cell, for no armor worn (`empty`) and the
/// game's `child` answer, in a seed with `flags`. The game does for a
/// child, who wore nothing else, and for an adult with no armor, whom it
/// then dresses in the Kokiri Tunic. With [`CHILD_GEAR`] only for the
/// adult: each cell's test, with [`kokiri_worn`], marks what a child wears.
#[inline(always)]
pub fn every_kokiri_worn(empty: bool, child: bool, flags: u32) -> bool {
    if child_gear(flags) {
        empty && !child
    } else {
        empty || child
    }
}

/// Whether the equipment menu marks the cell named `cell` as worn where
/// its own test (the cell's name against the armor slot's, `worn`)
/// doesn't, in a seed with `flags`. With [`CHILD_GEAR`] the Kokiri Tunic
/// is worn when the slot holds either age's name for it, or nothing: an
/// age switch keeps the other age's name, and a new game starts empty.
#[inline(always)]
pub fn kokiri_worn(cell: &[u8], worn: &[u8], flags: u32) -> bool {
    child_gear(flags) && kokiri(cell) && (worn.is_empty() || kokiri(worn))
}

/// Whether `name` is either age's Kokiri Tunic.
#[inline(always)]
fn kokiri(name: &[u8]) -> bool {
    let hash = fnv(name);
    hash == CHILD_ARMOR[2].worn || hash == ADULT_KOKIRI
}

/// Whether the D-pad's Down icon takes Link for a child, from the HUD
/// status's own answer (`child`): it hides the boots icon from a child.
/// Not in a seed with [`CHILD_GEAR`], where he wears them too.
pub fn hud_child(child: bool, flags: u32) -> bool {
    child && !child_gear(flags)
}

fn child_gear(flags: u32) -> bool {
    flags & CHILD_GEAR != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_child_gear_flag_lets_only_the_tunics_and_boots_through() {
        for slot in [ARMOR_SLOT, BOOTS_SLOT, 10] {
            assert!(!gear_through(slot, 0), "{slot}");
            assert!(!gear_through(slot, !CHILD_GEAR), "{slot}");
        }
        assert!(gear_through(ARMOR_SLOT, CHILD_GEAR));
        assert!(gear_through(BOOTS_SLOT, CHILD_GEAR));
        assert!(!gear_through(10, !0));
    }

    #[test]
    fn the_child_gear_flag_shows_a_child_the_boots_icon() {
        assert!(hud_child(true, 0));
        assert!(hud_child(true, !CHILD_GEAR));
        assert!(!hud_child(true, CHILD_GEAR));
        assert!(!hud_child(false, 0));
        assert!(!hud_child(false, CHILD_GEAR));
    }

    #[test]
    fn the_menu_marks_every_kokiri_cell_as_the_game_does_without_the_flag() {
        for child in [false, true] {
            assert!(every_kokiri_worn(true, child, !CHILD_GEAR));
            assert_eq!(every_kokiri_worn(false, child, !CHILD_GEAR), child);
        }
    }

    #[test]
    fn with_the_flag_only_an_adult_with_no_armor_marks_every_kokiri_cell() {
        assert!(every_kokiri_worn(true, false, CHILD_GEAR));
        assert!(!every_kokiri_worn(true, true, CHILD_GEAR));
        assert!(!every_kokiri_worn(false, true, CHILD_GEAR));
        assert!(!every_kokiri_worn(false, false, CHILD_GEAR));
    }

    #[test]
    fn another_cell_keeps_the_games_test() {
        // The Kokiri Tunics' names are checked against the dump's in `zootr`.
        assert!(!kokiri_worn(b"FakeArmor", b"", CHILD_GEAR));
    }

    #[test]
    fn a_child_keeps_any_other_armor() {
        // The tunics' names are checked against the dump's in `zootr`.
        assert_eq!(child_armor(b"FakeArmor", CHILD_GEAR), None);
    }
}
