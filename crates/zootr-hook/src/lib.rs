//! Code that runs inside the game. Built for `aarch64-unknown-none` by
//! `zootr`'s build script and written into `main` by a cheat file, with a
//! table the seed writes. On the host it builds as a plain library, so
//! the table format and level picking are tested here and shared with the
//! writer.
//!
//! Two hooks pick the next level of a progressive line Link doesn't have.
//! As a room loads, `drop_actor` writes it over a chest's placement, which
//! decides the model and the get event's extra grants. As the chest opens,
//! `content_name` sets it as the get event's `ContentName`, which decides
//! the text and the item.
//!
//! A map's chests are all made as it loads, so three more keep a second
//! chest of a line in step: `map_loaded` empties the chest list,
//! `chest_made` fills it, and `granted` recreates each closed chest's
//! content as the level it now gives. `granted` also credits key copies
//! from every holder that adds to the pouch, and `picked`, as a pickup
//! window opens, from the floor, whose take stocks the pouch without the
//! add.
//!
//! A seed with the Bunny Hood setting takes the run's speed request too:
//! `run_speed` makes it half again as fast while Link wears the hood.
//!
//! Dev builds hook Link's update too, to run the request the app leaves in
//! the [`Mailbox`]: a teleport, an age switch, the time of day, a quest
//! step, an item to give or an event to start. It also holds what the dev
//! [`switches`] the app turns on keep full.

#![cfg_attr(target_os = "none", no_std, no_main)]

#[cfg(target_os = "none")]
mod game;
mod mailbox;
mod shared;
pub mod switches;
mod table;

pub use mailbox::{
    ARG_ACTOR, ARG_CHALLENGE, ARG_EQUIP, ARG_GIVE, ARG_STEP, ARG_STEP_NAME, Ask, GUARD, MAILBOX,
    Mailbox, NAME, Outcome, play_event,
};
pub use shared::{CHESTS, Chest, Credited, List, Shared};
pub use switches::SWITCHES;
#[cfg(not(target_os = "none"))]
pub use table::encode;
pub use table::{
    BUNNY_HOOD, Credit, FREE_BOMBCHUS, HEADER, Header, Level, Levels, MAGIC, Offsets, Table,
    after_grant, pick, pick_with,
};

/// Where our pieces go in `main`, for game 1.1.0. The code goes over
/// Havok's physics trace recorder, which a patch keeps from ever running,
/// the table in the zero padding after rodata, and the chest list in the
/// zeroed page tail after bss.
pub const CAVE: u32 = 0x17f_1680;
pub const TABLE: u32 = 0x484_c080;
pub const LIST: u32 = 0x4e9_a4d0;
/// Where a panic in the hook leaves its line and file, in the list page's
/// last 16 bytes, for reading from outside when the game hangs.
pub const PANIC: u32 = 0x4e9_aff0;
/// Where each function hook's resume slot sits in the hook, one word each
/// in the order the hooks are listed, right after the entries. The patch
/// writes a `b` past the hooked function's pacibsp into each.
pub const RESUMES: u32 = 32;
