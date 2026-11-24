//! Every offset into `main` our code depends on, for one build of the game.
//! A [`Build`] holds them grouped by kind: [`V131072`] is game 1.1.1's,
//! and [`V65536`] game 1.0.0's, generated from it.
//! Nothing else in the repo spells one out: the hook, the patch, the build
//! script and the live reads all read them from here. See
//! `docs/research/binary.md`.
//!
//! Offsets count from the start of `main`'s image, past its NSO header.
//! Struct field offsets (an actor's position, a manager's list) aren't
//! `main` offsets and stay beside the code that reads them.

/// Where one build of `main` keeps everything the hook and the patch need.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Build {
    /// The functions the patch hooks.
    pub entries: Entries,
    /// The game's functions the hook or the host calls.
    pub funcs: Funcs,
    /// The calls the patch rewrites to branch into the hook.
    pub calls: Calls,
    /// The branches the patch rewrites.
    pub branches: Branches,
    /// The stubs of the game's imports the hook reaches.
    pub imports: Imports,
    /// The slots that hold a pointer to a manager.
    pub got: Got,
    /// What lives in bss.
    pub bss: Bss,
    /// The vtables the hook and the host recognise objects by.
    pub vtables: Vtables,
    /// Where our code, table and list go, and where the room for each ends.
    pub regions: Regions,
}

/// The hooked functions, each entered at its `pacibsp`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entries {
    /// A map's load finishing.
    pub map_loaded: u32,
    /// A chest's content made.
    pub chest_made: u32,
    /// The pouch add, which every grant goes through.
    pub granted: u32,
    /// Link's update.
    pub link_update: u32,
    /// `OneShotRequestOpenPickUpWindow`'s run.
    pub picked: u32,
    /// Puts a pickup's content in the pouch, on every take: from the
    /// pickup's own event (`OneShotPouchContentPushToPouchOwnInEvent`) or,
    /// with no window, straight from the touch.
    pub taken: u32,
    /// Link's nock (`EquipmentUser`).
    pub nock: u32,
    /// The Bow node's out of ammo handler (`ExecutePlayerBow`), which
    /// picks the empty message.
    pub emptied: u32,
    /// The Items page built (entered).
    pub items_built: u32,
    /// The Items page's vtable slot 7, run as it's left or the menu closes.
    pub items_left: u32,
    /// The UI update each frame.
    pub ui_update: u32,
    /// The Equipment page's vtable slots 5 and 7, as the Items page's:
    /// built and left.
    pub equipment_built: u32,
    pub equipment_left: u32,
    /// The resident actors' create (the manager, child), which makes the
    /// player of that age and has the other age's go, on every area load.
    /// See `docs/research/character-select.md`.
    pub resident_create: u32,
}

/// The game's functions, called from the hook or read by the host.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Funcs {
    /// The event argument setter `content_name` wraps.
    pub setter: u32,
    /// The drop reader `drop_actor` wraps.
    pub reader: u32,
    /// The pouch count query.
    pub count: u32,
    /// The name hash an `ActorHolder` keys its slots by.
    pub hash: u32,
    /// Requests a holder slot's actor be deleted.
    pub delete: u32,
    /// Creates an actor in a holder slot.
    pub create: u32,
    /// The drawn bow's component, its projectile's name
    /// (`BowParam.ShootableActor`), and the copy of that name into a fixed
    /// string.
    pub drawn_bow: u32,
    pub shootable: u32,
    pub name_copy: u32,
    /// Link's child test, which the bow draw's guide asks to pick the
    /// Slingshot's ("Shoot Seed") or the Bow's ("Shoot Arrow").
    pub childhood: u32,
    /// The game's own `malloc`.
    pub malloc: u32,
    /// Builds an event request in place.
    pub make_arg: u32,
    /// Asks the event manager to start a request.
    pub request: u32,
    /// Sets and releases the actor links a request holds.
    pub set_link: u32,
    pub release_link: u32,
    /// GameData's Bool getter and setter, and its UInt64 getter, by name
    /// hash.
    pub get_bool: u32,
    pub set_bool: u32,
    pub get_u64: u32,
    /// Sets the world clock, in seconds since midnight.
    pub set_time: u32,
    /// Moves a character controller to a point.
    pub move_to: u32,
    /// Sets the speed and direction Player asks the controller for.
    pub run_setter: u32,
    /// Sets an Items page cell's actor, and a cell button's unfocus and
    /// focus, which refresh the item detail panel.
    pub cell_set: u32,
    pub unfocus: u32,
    pub focus: u32,
    /// Seeks a layout animation to a frame, which colours an Items page
    /// cell's icon: usable, or on a button.
    pub anim_frame: u32,
    /// The item kind on a button for an age (the manager, child, 1).
    pub equipped: u32,
    /// Whether Link wears the named equipment (the pouch menu, a pointer
    /// to the name).
    pub worn: u32,
    /// Applies a cell layout's animation frames (the cell, 0).
    pub redraw: u32,
    /// Loads an Equipment cell's picture by name (its picture, &name, 0, 0).
    pub icon: u32,
    /// The resident actors' wake (the manager, &position, &3x3 rotation).
    pub cancel_sleep: u32,
    /// The request and release pause nodes' bodies (`SeqOneShotRequestPause`,
    /// `SeqOneShotReleasePause`), each taking the node.
    pub request_pause: u32,
    pub release_pause: u32,
}

/// The `bl`s the patch rewrites, each into a hook entry.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calls {
    /// The event request's string argument calls, where an opening chest
    /// sets its `ContentName`.
    pub content1: u32,
    pub content2: u32,
    /// The drop reader's calls, where a loading room reads a chest's
    /// placement.
    pub drop1: u32,
    pub drop2: u32,
    /// The bow draw's child test, which picks its guide.
    pub seed_guide: u32,
    /// The run node's call to the speed setter.
    pub run: u32,
    /// The controller device's three `GetNpadStates` calls: full key,
    /// handheld and joy dual.
    pub npad_full: u32,
    pub npad_handheld: u32,
    pub npad_dual: u32,
}

/// The branches the patch rewrites into `b`s: each where it is, and where
/// it skips to.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Branches {
    /// The physics step's `cbz` over the call into Havok's trace recorder,
    /// and where it skips to. Made a `b`, the recorder never runs and holds
    /// the cave.
    pub gate: u32,
    pub gate_to: u32,
    /// Link's "can he catch the Boomerang" check's `tbz` into its child
    /// test (the age override, then `PlayerGameParam.IsChild`), and where it
    /// skips to: whether the Boomerang is back. Made a `b`, the adult
    /// catches it as the child does, and every selector that asks gets the
    /// same answer.
    pub catch: u32,
    pub catch_to: u32,
    /// Where the `tbz` at `catch` goes in the game's own code, which the
    /// patch's checks read the original's word against.
    pub catch_orig: u32,
}

/// The game's stubs for `nn::hid::GetNpadStates` the `npad_*` calls go to.
/// The hook reaches them through its slots.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Imports {
    /// Full key, handheld and joy dual.
    pub npad_full: u32,
    pub npad_handheld: u32,
    pub npad_dual: u32,
}

/// The slots in the GOT that hold the address of a manager.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Got {
    /// The pointers to the event manager, GameData and the world.
    pub events: u32,
    pub gamedata: u32,
    pub world: u32,
    /// The pointer to the pouch menu.
    pub pouch_menu: u32,
}

/// Globals in bss.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bss {
    /// The pointer to the pouch manager.
    pub manager: u32,
    /// The pointer to the effect system, which holds the effect file the
    /// ice's `Look` sizes.
    pub effects: u32,
    /// The live actor table's header pointer and its slot count, which the
    /// host reads.
    pub actors_table: u32,
    pub actors_capacity: u32,
}

/// The vtables objects are recognised by.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vtables {
    /// A holder slot's vtable.
    pub slot: u32,
    /// What an empty actor link points at.
    pub link_empty: u32,
    /// The player actor's vtable, and its movement component's.
    pub player: u32,
    pub movement: u32,
    /// The vtable of Link's character controller, which holds `move_to`.
    pub controller: u32,
    /// A Life component's, which the host reads.
    pub life: u32,
}

/// Where our pieces go in `main`, and where the room for each ends. The
/// code goes over Havok's physics trace recorder, which a patch keeps from
/// ever running, the far code (link section `.far`, what rarely runs) over
/// a function only the recorder's dispatcher calls, dead with it, the table
/// in the zero padding after rodata, and the chest list in the zeroed page
/// tail after bss. The padding after text stays free: every other code mod
/// uses it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Regions {
    /// The hook's code, and where the recorder's room ends.
    pub cave: u32,
    pub cave_end: u32,
    /// The far code, and where its room ends: the first word past it the
    /// Mac's game rewrites, which a cheat can't write over.
    pub far: u32,
    pub far_end: u32,
    /// The seed's table, and where its room ends: the start of data.
    pub table: u32,
    pub table_end: u32,
    /// The first byte after bss. sloperinaGZ keeps its hook and lock slots
    /// in the 16 bytes from here, so the list keeps clear of them.
    pub bss_end: u32,
    /// Where the hook leaves an 8 byte mark (`PANICKED`, `BADTABLE`) and the
    /// return address of its stop, in the list page's last 16 bytes, for
    /// reading from outside when the game hangs.
    pub panic: u32,
    /// The end of the list's page.
    pub list_end: u32,
    /// The end of `main`'s text, every instruction the game has.
    pub text_end: u32,
}

/// What the running hook calls through, flat, as the table the hook reads
/// at run time ([`Table::offsets`](crate::Table)): a copy of the build's own
/// offsets, made by [`Build::offsets`]. Its order is the order of the hook's
/// loads, so it keeps the order the hook was first laid out in, and this one
/// list names each field and where in the build it comes from.
macro_rules! offsets {
    ($($field:ident = $group:ident.$from:ident,)*) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Offsets {
            $(pub $field: u32,)*
        }

        impl Build {
            /// The hook's view of this build.
            pub const fn offsets(&self) -> Offsets {
                Offsets {
                    $($field: self.$group.$from,)*
                }
            }
        }
    };
}

offsets! {
    setter = funcs.setter,
    reader = funcs.reader,
    count = funcs.count,
    manager = bss.manager,
    hash = funcs.hash,
    delete = funcs.delete,
    create = funcs.create,
    slot_vtable = vtables.slot,
    map_loaded = entries.map_loaded,
    chest_made = entries.chest_made,
    granted = entries.granted,
    link_update = entries.link_update,
    picked = entries.picked,
    taken = entries.taken,
    nock = entries.nock,
    drawn_bow = funcs.drawn_bow,
    shootable = funcs.shootable,
    name_copy = funcs.name_copy,
    emptied = entries.emptied,
    childhood = funcs.childhood,
    malloc = funcs.malloc,
    make_arg = funcs.make_arg,
    request = funcs.request,
    set_link = funcs.set_link,
    release_link = funcs.release_link,
    link_empty = vtables.link_empty,
    get_bool = funcs.get_bool,
    set_bool = funcs.set_bool,
    get_u64 = funcs.get_u64,
    events = got.events,
    gamedata = got.gamedata,
    world = got.world,
    set_time = funcs.set_time,
    player_vtable = vtables.player,
    move_vtable = vtables.movement,
    move_to = funcs.move_to,
    controller_vtable = vtables.controller,
    run_setter = funcs.run_setter,
    items_built = entries.items_built,
    items_left = entries.items_left,
    ui_update = entries.ui_update,
    equipment_built = entries.equipment_built,
    equipment_left = entries.equipment_left,
    cell_set = funcs.cell_set,
    unfocus = funcs.unfocus,
    focus = funcs.focus,
    anim_frame = funcs.anim_frame,
    equipped = funcs.equipped,
    worn = funcs.worn,
    pouch_menu = got.pouch_menu,
    redraw = funcs.redraw,
    icon = funcs.icon,
    effects = bss.effects,
    resident_create = entries.resident_create,
    cancel_sleep = funcs.cancel_sleep,
    request_pause = funcs.request_pause,
    release_pause = funcs.release_pause,
}

mod v65536;
pub use v65536::V65536;

/// Game 1.1.1's (package v131072).
pub const V131072: Build = Build {
    entries: Entries {
        map_loaded: 0x108_3e20,
        chest_made: 0xd2_72f0,
        granted: 0x110_d570,
        link_update: 0x2c2_15e0,
        picked: 0x101_55d0,
        taken: 0xc6_f620,
        nock: 0x2df_eea0,
        emptied: 0x279_a7a0,
        items_built: 0xa7_13b0,
        items_left: 0xa7_6770,
        ui_update: 0x6e_6be0,
        equipment_built: 0xa6_c800,
        equipment_left: 0xa7_0ff0,
        resident_create: 0x94_5610,
    },
    funcs: Funcs {
        setter: 0xfe_3fe0,
        reader: 0xbd_1c00,
        count: 0x59_63e0,
        hash: 0x17_39e0,
        delete: 0xe9_1290,
        create: 0xc1_7400,
        drawn_bow: 0x2e0_0870,
        shootable: 0x2d0_9f60,
        name_copy: 0x2b_5480,
        childhood: 0x4b_6db0,
        malloc: 0x14c_6c90,
        make_arg: 0xd7_9f10,
        request: 0xd7_a820,
        set_link: 0x2b_c440,
        release_link: 0x20_b6f0,
        get_bool: 0x20_d9c0,
        set_bool: 0x68_2b20,
        get_u64: 0x57_a380,
        set_time: 0x345_22d0,
        move_to: 0x6b_2530,
        run_setter: 0x93_52f0,
        cell_set: 0xa7_3060,
        unfocus: 0xab_5080,
        focus: 0xab_4ec0,
        anim_frame: 0x2d_2d90,
        equipped: 0x17_b6e0,
        worn: 0xa7_1080,
        redraw: 0x20_8350,
        icon: 0xa7_37e0,
        cancel_sleep: 0x94_3550,
        request_pause: 0xd9_aec0,
        release_pause: 0xda_d2c0,
    },
    calls: Calls {
        content1: 0x2ae_0828,
        content2: 0x2ae_08cc,
        drop1: 0x2ef_6560,
        drop2: 0x2ef_4888,
        seed_guide: 0x4b_01a4,
        run: 0x93_4990,
        npad_full: 0x346_966c,
        npad_handheld: 0x346_9698,
        npad_dual: 0x346_94f8,
    },
    branches: Branches {
        gate: 0x17d_59bc,
        gate_to: 0x17d_59c4,
        catch: 0x41_0250,
        catch_to: 0x41_0258,
        catch_orig: 0x41_02f0,
    },
    imports: Imports {
        npad_full: 0x3c4_2800,
        npad_handheld: 0x3c4_2830,
        npad_dual: 0x3c4_27f0,
    },
    got: Got {
        events: 0x4c7_e090,
        gamedata: 0x4c7_e180,
        // The slot the game's own set-time node reads. The one at 0x4c7_e878
        // also has a clock-like object at +0x510, and nothing reads that one.
        world: 0x4c7_e898,
        pouch_menu: 0x4c7_e0d0,
    },
    bss: Bss {
        manager: 0x4e7_8e28,
        effects: 0x4e8_40e0,
        actors_table: 0x4e9_0da8,
        actors_capacity: 0x4e9_0da0,
    },
    vtables: Vtables {
        slot: 0x4b4_69a0,
        link_empty: 0x497_4b20,
        player: 0x4b2_5e18,
        movement: 0x4b4_c460,
        controller: 0x4c3_4f10,
        life: 0x4b4_86a0,
    },
    regions: Regions {
        cave: 0x17f_1680,
        cave_end: 0x17f_58e0,
        far: 0x17f_60c0,
        far_end: 0x17f_8a1c,
        table: 0x484_c080,
        table_end: 0x484_d000,
        bss_end: 0x4e9_a4d0,
        panic: 0x4e9_aff0,
        list_end: 0x4e9_b000,
        text_end: 0x3c4_61d0,
    },
};
