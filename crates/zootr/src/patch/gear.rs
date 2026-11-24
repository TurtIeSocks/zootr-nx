//! The game's own age checks that keep boots off a child, opened so the
//! Iron and Hover Boots reach him in a seed with ageless gear.

use zootr_hook::Build;
use zootr_hook::entry::{GEAR_CHILD, GEAR_HUD};

/// The boot state's `tbz` past its child test's early return, and where it
/// skips to: reading which boots Link wears. Made a `b`, a child in the
/// Iron or Hover Boots gets their state as the adult does. Unconditional:
/// the boots only reach a child through the calls below.
pub(super) const fn boots(b: &Build) -> (u32, u32) {
    (b.branches.boots, b.branches.boots_to)
}

/// The equipment slots' child tests, `bl` to Link's child test with the
/// slot in `w20`: in the equip request, then in the actor it makes. A
/// child is refused the armor, boots and hand armor slots there, so
/// nothing of theirs is made on him. Each goes to the hook's `gear_child`,
/// which lets the boots slot through in a seed with `CHILD_GEAR`. Without
/// it, an adult in boots who turns child keeps them on, and the child has
/// no clips to walk in them.
pub(super) const fn calls(b: &Build) -> [(u32, u32); 2] {
    [
        (b.calls.gear_request, GEAR_CHILD),
        (b.calls.gear_actor, GEAR_CHILD),
    ]
}

/// The D-pad boots icon's read of whether Link is a child, from the HUD
/// status (`ldrb w8, [x8, #0x5c0]`), and the hook entry its `bl` goes to
/// in its place: `gear_hud`. A child's icon hides there, so in a seed with
/// `CHILD_GEAR` it answers that he isn't one. Only the boots icon reads it
/// here: the masks share the cross with their own test.
pub(super) const fn hud(b: &Build) -> (u32, u32) {
    (b.calls.gear_hud, GEAR_HUD)
}

#[cfg(test)]
mod tests {
    use super::super::{Hook, branch};
    use super::*;

    #[test]
    fn the_boot_state_skips_the_child_return() {
        for h in Hook::all() {
            let (at, to) = boots(&h.build);
            let p = h.patches(&crate::progressive::LINES, &[], 0, &[], &[], None);
            let p = p.unwrap().into_iter().find(|p| p.at == at).unwrap();
            assert_eq!(p.bytes, branch(at, to, false), "{}", h.release.name());
        }
    }
}
