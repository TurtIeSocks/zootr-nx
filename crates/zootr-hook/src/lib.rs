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
//! add. `taken`, as any take goes into the pouch, window or not, sets the
//! flag of the twinned holder (a pot, box, rock or hive) whose drop it was.
//!
//! A seed with the Bunny Hood setting takes the run's speed request too:
//! `run_speed` makes it half again as fast while Link wears the hood.
//!
//! Link's update is hooked too. A seed with ice traps fits a trap's look
//! there and freezes Link when one springs. Dev builds also run the request
//! the app leaves in the [`Mailbox`] there: a teleport, an age switch, the
//! time of day, a quest step, an item to give or an event to start. It also
//! holds what the dev [`switches`] the app turns on keep full.

#![cfg_attr(target_os = "none", no_std, no_main)]

mod build;
pub mod entry;
#[cfg(target_os = "none")]
mod game;
pub mod keys;
pub mod look;
mod mailbox;
mod shared;
mod spots;
pub mod switches;
mod table;
pub mod traps;
pub mod ui;

pub use build::{
    Branches, Bss, Build, Calls, Entries, Funcs, Got, Imports, Offsets, Regions, V65536, V131072,
    Vtables,
};
pub use mailbox::{
    ARG_ACTOR, ARG_CHALLENGE, ARG_EQUIP, ARG_GIVE, ARG_MIXED, ARG_NOTICE, ARG_STEP, ARG_STEP_NAME,
    ARG_SUMMARY, Ask, GUARD, MAILBOX, Mailbox, NAME, Notice, Outcome, play_event,
};
pub use shared::{CHESTS, Chest, Credited, List, Shared, Swap};
pub use spots::{REACH, SPOT_KEY, Spot, nearest};
pub use switches::SWITCHES;
#[cfg(not(target_os = "none"))]
pub use table::SlotSpec;
#[cfg(not(target_os = "none"))]
pub use table::encode;
pub use table::{
    BOMBCHU_BAG, BUNNY_HOOD, Credit, DEV, FREE_BOMBCHUS, Found, HEADER, Header, ICE_TRAPS, Level,
    Levels, MAGIC, MENU_CYCLE, Slot, Table, after_grant, fnv, pick, pick_with,
};

/// The build everything here is made for: the hook is compiled against it,
/// the same in every seed, so the hook calls through its offsets with no
/// table yet, and the patch is written for it. `build.rs` compiles the
/// hook once per build, 1.0.0's with `zootr_build="v65536"`. The host's
/// tools, which run against 1.1.1, see 1.1.1's.
#[cfg(not(zootr_build = "v65536"))]
pub const OFFSETS: Build = V131072;
#[cfg(zootr_build = "v65536")]
pub const OFFSETS: Build = V65536;
/// The part of [`OFFSETS`] the running hook calls through.
const RUNTIME: Offsets = OFFSETS.offsets();

/// Where our pieces go in `main`, from [`Build::regions`].
pub const CAVE: u32 = OFFSETS.regions.cave;
pub const FAR: u32 = OFFSETS.regions.far;
pub const TABLE: u32 = OFFSETS.regions.table;
const BSS_END: u32 = OFFSETS.regions.bss_end;
pub const PANIC: u32 = OFFSETS.regions.panic;
/// The list sits at the page tail's top end, under [`PANIC`], and moves
/// down as [`Shared`] grows.
pub const LIST: u32 = list(&OFFSETS);
const _: () = assert!(LIST >= BSS_END + 16, "the list reaches sloperinaGZ's slots");
const _: () = assert!(list(&V65536) >= V65536.regions.bss_end + 16, "1.0.0's list");
const _: () = assert!(
    list(&V131072) >= V131072.regions.bss_end + 16,
    "1.1.1's list"
);

/// Where build `b`'s list sits: [`LIST`] for the build the hook is made for.
pub const fn list(b: &Build) -> u32 {
    (b.regions.panic - size_of::<Shared>() as u32) & !15
}
/// Where the hook's branch slots sit, one word each right after the
/// entries: each function hook's resume, in the order the hooks are
/// listed, then the full key, handheld and joy dual controller reads. The
/// patch writes a `b` into each: past the hooked function's pacibsp, or to
/// the game's `GetNpadStates` stub for that style.
pub const SLOTS: u32 = 84;

/// The Deku Seed's name hash, the projectile an empty bow says "no Deku
/// seeds" for and the draw says "Shoot Seed" for. Every other one says
/// "no arrows" and "Shoot Arrow".
pub const DEKU_SEED: u32 = 0xdc49_1eef;
