//! The out of ammo handler's hook over faked game memory: the Bow node,
//! the steps from it to Link, and his components.

use zootr_hook::OFFSETS;

use super::swap::{FAKE, Game, u64s};

/// The Bow node and each pointer the hook follows from it to Link's
/// components, past the swap's fake memory: where each one is written,
/// and what it holds.
const NODE: u64 = FAKE + 0xf100;
const OWNER: u64 = FAKE + 0xf200;
const ACTOR: u64 = FAKE + 0xf300;
const LINK: u64 = FAKE + 0xf500;
const PARTS: u64 = FAKE + 0xf800;
const USER: u64 = FAKE + 0xfc00;
/// The player component: its flags word lands at +0xe38, on the page.
const PLAYER: u64 = FAKE + 0xf000;
const STEPS: [(u64, u64); 6] = [
    (NODE + 8, OWNER),
    (OWNER + 8, ACTOR),
    (ACTOR + 0x1f0, LINK),
    (LINK + 0x270, PARTS),
    (PARTS + 0x200, USER),
    (PARTS + 0x358, PLAYER),
];
/// The player component's flags word, with no empty message raised.
const FLAGS: u64 = PLAYER + 0xe38;

#[test]
fn an_empty_bow_with_a_null_on_the_way_to_link_leaves_the_game_alone() {
    let Some(mut g) = Game::new() else {
        return;
    };
    g.write(FLAGS, &0u32.to_le_bytes());
    for broken in 0..=STEPS.len() {
        for (i, &(at, to)) in STEPS.iter().enumerate() {
            g.write(at, &u64s(&[if i == broken { 0 } else { to }]));
        }
        g.call(OFFSETS.emptied, NODE);
        assert_eq!(g.u64(FLAGS) as u32, 0, "step {broken} null");
    }
}
