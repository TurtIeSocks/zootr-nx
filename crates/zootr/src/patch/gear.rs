//! The game's own age checks that keep boots off a child, rewritten so
//! the Iron and Hover Boots reach him. A child wears boots only in a seed
//! whose table unlocks them, so every seed carries these.

use super::Patch;

/// The boot state's `tbz` past its child test's early return, and where it
/// skips to: reading which boots Link wears. Made a `b`, a child in the
/// Iron or Hover Boots gets their state as the adult does.
pub(super) const BOOTS: (u32, u32) = (0x4b_6ed8, 0x4b_6ef4);

/// The equipment slots' child refusals, each `(at, to)`: three words that
/// return early for a child equipping the armor (8), boots (9) or hand
/// armor (10) slot, slot in `w20`, and where the slot carries on. The
/// first is in the equip request, the second in the actor it creates.
pub(super) const GATES: [(u32, u32); 2] = [(0x2df_6d88, 0x2df_6e88), (0x101_4018, 0x101_4034)];

/// The words each gate holds in 1.1.1: `sub w8, w20, #8`, `cmp w8, #2`
/// or `#3`, then `b.hi` or `b.hs` past the refusal.
#[cfg(test)]
const VANILLA: [[u32; 3]; 2] = [
    [0x5100_2288, 0x7100_091f, 0x5400_07c8],
    [0x5100_2288, 0x7100_0d1f, 0x5400_00a2],
];

/// The gate at `at` refusing only the armor and hand armor slots: `cmp
/// w20, #8`, `ccmp w20, #10, #4, ne` (equal when either), `b.ne to`.
fn boots_pass(at: u32, to: u32) -> Patch {
    let delta = (to - (at + 8)) / 4;
    let words = [0x7100_229f, 0x7a4a_1a84, 0x5400_0001 | delta << 5];
    Patch {
        at,
        bytes: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    }
}

/// Every gate opened to the boots slot.
pub(super) fn gates() -> impl Iterator<Item = Patch> {
    GATES.into_iter().map(|(at, to)| boots_pass(at, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boot_state_skips_the_child_return() {
        let p = super::super::patches(&crate::progressive::LINES, &[], 0, &[], &[], None).unwrap();
        let boots = p.iter().find(|p| p.at == BOOTS.0).unwrap();
        // b 0x4b6ef4, over tbz w0, #0, 0x4b6ef4
        assert_eq!(boots.bytes, 0x1400_0007u32.to_le_bytes());
    }

    #[test]
    fn each_gate_still_refuses_the_armor_and_hand_armor_slots() {
        let words = |p: &Patch| -> Vec<u32> {
            (p.bytes.chunks(4))
                .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
                .collect()
        };
        let [equip, create] = [0, 1].map(|i| words(&gates().nth(i).unwrap()));
        // cmp w20, #8; ccmp w20, #10, #4, ne; b.ne 0x2df6e88
        assert_eq!(equip, [0x7100_229f, 0x7a4a_1a84, 0x5400_07c1]);
        // b.ne 0x1014034, from 0x1014020
        assert_eq!(create[2], 0x5400_00a1);
    }

    #[test]
    fn each_gate_holds_its_vanilla_refusal() {
        let Ok(main) = std::fs::read(super::super::tests::PC_MAIN) else {
            return;
        };
        let word = |at: u32| u32::from_le_bytes(main[at as usize..][..4].try_into().unwrap());
        assert_eq!(word(BOOTS.0), 0x3600_00e0, "tbz w0, #0, 0x4b6ef4");
        for ((at, _), vanilla) in GATES.into_iter().zip(VANILLA) {
            assert_eq!([0, 4, 8].map(|i| word(at + i)), vanilla, "{at:#x}");
        }
    }
}
