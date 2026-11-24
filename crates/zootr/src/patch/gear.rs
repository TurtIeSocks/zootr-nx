//! The game's own age checks that keep boots and tunics off a child,
//! opened so the Iron and Hover Boots and the Zora and Goron Tunics reach
//! him in a seed with ageless gear.

use zootr_hook::entry::{GEAR_CHILD, GEAR_CREATE, GEAR_HUD, GEAR_MENU};

/// The boot state's `tbz` past its child test's early return, and where it
/// skips to: reading which boots Link wears. Made a `b`, a child in the
/// Iron or Hover Boots gets their state as the adult does. Unconditional:
/// the boots only reach a child through the calls below.
pub(super) const BOOTS: (u32, u32) = (0x4b_6ed8, 0x4b_6ef4);

/// The equipment slots' child tests, `bl` to Link's child test with the
/// slot in `w20`: in the equip request, then in the actor it makes. A
/// child is refused the armor, boots and hand armor slots there, so
/// nothing of theirs is made on him. Each goes to the hook's `gear_child`
///, which lets the boots and armor slots through in a seed with
/// `CHILD_GEAR`, and the tunics too. Without it, an adult in boots who
/// turns child keeps them on, and the child has no clips to walk in them.
pub(super) const CALLS: [(u32, u32); 2] = [(0x2df_6d84, GEAR_CHILD), (0x101_4010, GEAR_CHILD)];

/// The D-pad boots icon's read of whether Link is a child, from the HUD
/// status (`ldrb w8, [x8, #0x5c0]`), and the hook entry its `bl` goes to
/// in its place: `gear_hud`. A child's icon hides there, so in
/// a seed with `CHILD_GEAR` it answers that he isn't one. Only the boots
/// icon reads it here: the masks share the cross with their own test.
pub(super) const HUD: (u32, u32) = (0x2d_4998, GEAR_HUD);

/// The actor creator, which the equipment code hands an actor's name.
#[cfg(test)]
const CREATOR: u32 = 0x37_2a70;

/// The equipment code's calls to the creator: in the actor maker, the
/// pouch's maker and the spawner's three. Each goes to `gear_create`,
/// which makes a child's tunic the seed's armor for him in a seed with
/// `CHILD_GEAR`. The pause menu puts a tunic on through these, not the
/// equip refresh.
pub(super) const CREATE: [(u32, u32); 5] = [
    (0x101_47c0, GEAR_CREATE),
    (0x2df_8c20, GEAR_CREATE),
    (0x2e0_2208, GEAR_CREATE),
    (0x2e0_2934, GEAR_CREATE),
    (0x2e0_3014, GEAR_CREATE),
];

/// The pause menu's read of whether Link is a child (`ldrb w25, [x9,
/// #0x5c0]`), which greys his tunics out, and the hook entry its `bl` goes
/// to in its place: `gear_menu`, which answers as [`HUD`]'s does.
pub(super) const MENU: (u32, u32) = (0x33b_a4a8, GEAR_MENU);

/// Every site above that a `bl` goes over.
pub(super) const SITES: [(u32, u32); 9] = [
    CALLS[0], CALLS[1], HUD, MENU, CREATE[0], CREATE[1], CREATE[2], CREATE[3], CREATE[4],
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Link's child test.
    const CHILDHOOD: u32 = 0x4b_6db0;

    /// `bl to` from `at`.
    fn call(at: u32, to: u32) -> u32 {
        0x9400_0000 | ((to.wrapping_sub(at) / 4) & 0x3ff_ffff)
    }

    #[test]
    fn each_call_and_skip_holds_its_vanilla_word() {
        let Ok(main) = std::fs::read(super::super::tests::PC_MAIN) else {
            return;
        };
        let word = |at: u32| u32::from_le_bytes(main[at as usize..][..4].try_into().unwrap());
        assert_eq!(word(BOOTS.0), 0x3600_00e0, "tbz w0, #0, 0x4b6ef4");
        assert_eq!(word(HUD.0), 0x3957_0108, "ldrb w8, [x8, #0x5c0]");
        assert_eq!(word(MENU.0), 0x3957_0139, "ldrb w25, [x9, #0x5c0]");
        for (at, _) in CREATE {
            assert_eq!(word(at), call(at, CREATOR), "{at:#x}");
        }
        for (at, _) in CALLS {
            assert_eq!(word(at), call(at, CHILDHOOD), "{at:#x}");
        }
    }

    #[test]
    fn the_boot_state_skips_the_child_return() {
        let p = super::super::patches(&crate::progressive::LINES, &[], 0, &[], &[], None).unwrap();
        let boots = p.iter().find(|p| p.at == BOOTS.0).unwrap();
        // b 0x4b6ef4, over tbz w0, #0, 0x4b6ef4
        assert_eq!(boots.bytes, 0x1400_0007u32.to_le_bytes());
    }
}
