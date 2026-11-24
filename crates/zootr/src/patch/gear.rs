//! The game's own age checks that keep boots and tunics off a child,
//! opened so they reach him in a seed with a boots or tunic toggle on.

use zootr_hook::Build;
use zootr_hook::entry::{GEAR_CHILD, GEAR_CREATE, GEAR_HUD, GEAR_MARK, GEAR_MENU, GEAR_WORN};

/// The boot state's `tbz` past its child test's early return, and where it
/// skips to: reading which boots Link wears. Made a `b`, a child in the
/// Iron or Hover Boots gets their state as the adult does. Unconditional:
/// the boots only reach a child through the sites below.
pub(super) const fn boots(b: &Build) -> (u32, u32) {
    (b.branches.boots, b.branches.boots_to)
}

/// Every gear site a `bl` goes over, and the hook entry it goes to:
///
/// - The equipment slots' child tests, `bl` to Link's child test with the
///   slot in `w20`, in the equip request and in the actor it makes, go to
///   `gear_child`. A child is refused the armor, boots and hand armor
///   slots there, so nothing of theirs is made on him. It lets the boots
///   through in a seed with `CHILD_GEAR`. Without it, an adult in boots
///   who turns child keeps them on, and the child has no clips to walk in
///   them.
/// - The D-pad boots icon's read of whether Link is a child goes to
///   `gear_hud`. A child's icon hides there, so with `CHILD_GEAR` it
///   answers that he isn't one. Only the boots icon reads it here: the
///   masks share the cross with their own test.
/// - The pause menu's read of the same, which greys his tunics out, goes
///   to `gear_menu`, which answers as `gear_hud` does.
/// - The equipment cells' child test for the armor's worn mark goes to
///   `gear_worn`. For a child every Kokiri Tunic cell shows as worn,
///   without reading what he wears, so with `CHILD_GEAR` his cells take
///   the worn test instead. That test's call goes to `gear_mark`, which
///   with `CHILD_GEAR` also takes either age's Kokiri Tunic, or none, for
///   the Kokiri Tunic.
/// - The equipment code's calls to the actor creator go to `gear_create`,
///   which makes a child's tunic the seed's armor for him with
///   `CHILD_GEAR`. The pause menu puts a tunic on through these, not the
///   equip refresh.
pub(super) const fn sites(b: &Build) -> [(u32, u32); 11] {
    let c = &b.calls;
    [
        (c.gear_request, GEAR_CHILD),
        (c.gear_actor, GEAR_CHILD),
        (c.gear_hud, GEAR_HUD),
        (c.gear_menu, GEAR_MENU),
        (c.gear_worn, GEAR_WORN),
        (c.gear_mark, GEAR_MARK),
        (c.gear_create1, GEAR_CREATE),
        (c.gear_create2, GEAR_CREATE),
        (c.gear_create3, GEAR_CREATE),
        (c.gear_create4, GEAR_CREATE),
        (c.gear_create5, GEAR_CREATE),
    ]
}

/// Checks each gear site in `main` holds the word the patch replaces,
/// for build `b`: the calls go where the table says, the loads and the
/// `ccmp` are the words the hook entries stand in for.
#[cfg(test)]
pub(super) fn words_hold_what_we_expect(main: &[u8], b: &Build) {
    let word = |at: u32| u32::from_le_bytes(main[at as usize..][..4].try_into().unwrap());
    let call = |at: u32, to: u32| u32::from_le_bytes(super::branch(at, to, true));
    let (at, to) = boots(b);
    let tbz = 0x3600_0000 | (((to - at) / 4) << 5);
    assert_eq!(word(at), tbz, "tbz w0, #0, boots_to");
    let (c, f) = (&b.calls, &b.funcs);
    assert_eq!(word(c.gear_hud), 0x3957_0108, "ldrb w8, [x8, #0x5c0]");
    assert_eq!(word(c.gear_menu), 0x3957_0139, "ldrb w25, [x9, #0x5c0]");
    assert_eq!(word(c.gear_worn), 0x7a40_1b80, "ccmp w28, #0x0, #0x0, ne");
    let calls = [
        (c.gear_request, f.childhood),
        (c.gear_actor, f.childhood),
        (c.gear_mark, f.worn),
        (c.gear_create1, f.make_actor),
        (c.gear_create2, f.make_actor),
        (c.gear_create3, f.make_actor),
        (c.gear_create4, f.make_actor),
        (c.gear_create5, f.make_actor),
    ];
    for (at, to) in calls {
        assert_eq!(word(at), call(at, to), "{at:#x}");
    }
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
