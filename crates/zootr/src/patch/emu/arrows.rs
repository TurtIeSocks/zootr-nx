//! The out of ammo handler's hook over faked game memory: the Bow node,
//! the steps from it to Link, and his components.

use zootr_hook::OFFSETS;

use super::swap::{FAKE, Game, u64s};

/// Where the test's own pages are mapped: past the swap's fake memory, so
/// nothing of the swap's shares them.
const PAGES: u64 = FAKE + 0x1_0000;
const PAGES_LEN: u64 = 0x2000;
/// The player component: its flags word lands at +0xe38, on the page.
const PLAYER: u64 = PAGES;
/// The Bow node and each pointer the hook follows from it to Link's
/// components: where each one is written, and what it holds.
const NODE: u64 = PAGES + 0x1100;
const OWNER: u64 = PAGES + 0x1200;
const ACTOR: u64 = PAGES + 0x1300;
const LINK: u64 = PAGES + 0x1500;
const PARTS: u64 = PAGES + 0x1800;
const USER: u64 = PAGES + 0x1c00;
const STEPS: [(u64, u64); 6] = [
    (NODE + 8, OWNER),
    (OWNER + 8, ACTOR),
    (ACTOR + 0x1f0, LINK),
    (LINK + 0x270, PARTS),
    (PARTS + 0x200, USER),
    (PARTS + 0x358, PLAYER),
];
/// The player component's flags word, and the no arrows message in it.
const FLAGS: u64 = PLAYER + 0xe38;
const NO_ARROWS: u32 = 1 << 27;

/// With the message raised, the hook would rewrite the word if it got
/// past a null, so the word staying put shows the guard stopped it.
#[test]
fn an_empty_bow_with_a_null_on_the_way_to_link_leaves_the_game_alone() {
    let Some(mut g) = Game::new() else {
        return;
    };
    g.map(PAGES, PAGES_LEN);
    for broken in 0..STEPS.len() {
        for (i, &(at, to)) in STEPS.iter().enumerate() {
            g.write(at, &u64s(&[if i == broken { 0 } else { to }]));
        }
        g.write(FLAGS, &NO_ARROWS.to_le_bytes());
        g.call(OFFSETS.emptied, NODE);
        assert_eq!(g.u64(FLAGS) as u32, NO_ARROWS, "step {broken} null");
    }
}
