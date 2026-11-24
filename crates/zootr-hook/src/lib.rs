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
    BOMBCHU_BAG, BOOTS_SLOT, BUNNY_HOOD, CHILD_GEAR, Credit, DEV, FREE_BOMBCHUS, Found, HEADER,
    Header, ICE_TRAPS, Level, Levels, MAGIC, MENU_CYCLE, Offsets, Slot, Table, after_grant, fnv,
    gear_through, pick, pick_with,
};

/// Where our pieces go in `main`, for game 1.1.1. The code goes over
/// Havok's physics trace recorder, which a patch keeps from ever running,
/// the far code (link section `.far`, what rarely runs) over a function
/// only the recorder's dispatcher calls, dead with it, the table in the zero
/// padding after rodata, and the chest list in the zeroed page tail after
/// bss. The padding after text stays free: every other code mod uses it.
pub const CAVE: u32 = 0x17f_1680;
pub const FAR: u32 = 0x17f_60c0;
pub const TABLE: u32 = 0x484_c080;
/// The first byte after bss. sloperinaGZ keeps its hook and lock slots
/// in the 16 bytes from here, so the list keeps clear of them.
const BSS_END: u32 = 0x4e9_a4d0;
/// The list sits at the page tail's top end, under [`PANIC`], and moves
/// down as [`Shared`] grows.
pub const LIST: u32 = (PANIC - size_of::<Shared>() as u32) & !15;
const _: () = assert!(LIST >= BSS_END + 16, "the list reaches sloperinaGZ's slots");
/// Where the hook leaves an 8 byte mark (`PANICKED`, `BADTABLE`) and the
/// return address of its stop, in the list page's last 16 bytes, for
/// reading from outside when the game hangs.
pub const PANIC: u32 = 0x4e9_aff0;
/// Where the hook's branch slots sit, one word each right after the
/// entries: each function hook's resume, in the order the hooks are
/// listed, then the full key, handheld and joy dual controller reads. The
/// patch writes a `b` into each: past the hooked function's pacibsp, or to
/// the game's `GetNpadStates` stub for that style.
pub const SLOTS: u32 = 88;

/// The Deku Seed's name hash, the projectile an empty bow says "no Deku
/// seeds" for and the draw says "Shoot Seed" for. Every other one says
/// "no arrows" and "Shoot Arrow".
pub const DEKU_SEED: u32 = 0xdc49_1eef;

/// Where the game keeps what the hook needs in `main` 1.1.1, the same in
/// every seed, so the hook calls through them with no table yet. See
/// `docs/research/binary.md`.
pub const OFFSETS: Offsets = Offsets {
    setter: 0xfe_3fe0,
    reader: 0xbd_1c00,
    count: 0x59_63e0,
    manager: 0x4e7_8e28,
    hash: 0x17_39e0,
    delete: 0xe9_1290,
    create: 0xc1_7400,
    slot_vtable: 0x4b4_69a0,
    map_loaded: 0x108_3e20,
    chest_made: 0xd2_72f0,
    granted: 0x110_d570,
    link_update: 0x2c2_15e0,
    // `OneShotRequestOpenPickUpWindow`'s run.
    picked: 0x101_55d0,
    // Puts a pickup's content in the pouch, on every take: from the
    // pickup's own event (`OneShotPouchContentPushToPouchOwnInEvent`) or,
    // with no window, straight from the touch.
    taken: 0xc6_f620,
    // Link's nock (EquipmentUser), the drawn bow's component, its
    // `BowParam.ShootableActor`, and the copy of that into a fixed string.
    nock: 0x2df_eea0,
    drawn_bow: 0x2e0_0870,
    shootable: 0x2d0_9f60,
    name_copy: 0x2b_5480,
    // The Bow node's out of ammo handler (`ExecutePlayerBow`).
    emptied: 0x279_a7a0,
    // Link's child test, which the bow draw's guide asks to pick the
    // Slingshot's ("Shoot Seed") or the Bow's ("Shoot Arrow").
    childhood: 0x4b_6db0,
    malloc: 0x14c_6c90,
    make_arg: 0xd7_9f10,
    request: 0xd7_a820,
    set_link: 0x2b_c440,
    release_link: 0x20_b6f0,
    link_empty: 0x497_4b20,
    get_bool: 0x20_d9c0,
    set_bool: 0x68_2b20,
    get_u64: 0x57_a380,
    events: 0x4c7_e090,
    gamedata: 0x4c7_e180,
    // The slot the game's own set-time node reads. The one at 0x4c7_e878
    // also has a clock-like object at +0x510, and nothing reads that one.
    world: 0x4c7_e898,
    set_time: 0x345_22d0,
    player_vtable: 0x4b2_5e18,
    move_vtable: 0x4b4_c460,
    move_to: 0x6b_2530,
    controller_vtable: 0x4c3_4f10,
    run_setter: 0x93_52f0,
    items_built: 0xa7_13b0,
    // The Items page's vtable slot 7, run as it's left or the menu closes.
    items_left: 0xa7_6770,
    ui_update: 0x6e_6be0,
    // The Equipment page's vtable slots 5 and 7, as the Items page's.
    equipment_built: 0xa6_c800,
    equipment_left: 0xa7_0ff0,
    cell_set: 0xa7_3060,
    unfocus: 0xab_5080,
    focus: 0xab_4ec0,
    anim_frame: 0x2d_2d90,
    equipped: 0x17_b6e0,
    worn: 0xa7_1080,
    pouch_menu: 0x4c7_e0d0,
    redraw: 0x20_8350,
    icon: 0xa7_37e0,
    effects: 0x4e8_40e0,
    resident_create: 0x94_5610,
    cancel_sleep: 0x94_3550,
    request_pause: 0xd9_aec0,
    release_pause: 0xda_d2c0,
};
