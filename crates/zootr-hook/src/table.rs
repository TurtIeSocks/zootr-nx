//! The table the seed writes into the game: its header, the offsets of what
//! the hook needs, the progressive lines, how a level is picked, the
//! flags an item sets as it's found, and the menu slot Link cycles.

use crate::ui::MultiSlot;

pub const MAGIC: &[u8; 4] = b"ZLT6";

/// The header's flag for a seed whose bombchus need no bomb bag.
pub const FREE_BOMBCHUS: u32 = 1;
/// The header's flag for a seed where Link runs faster in the Bunny Hood.
pub const BUNNY_HOOD: u32 = 2;
/// The header's flag for a seed where Link picks what the trade slot shows.
pub const TRADE_CYCLE: u32 = 4;
pub const HEADER: usize = size_of::<Header>();

/// The table's start, little endian like the game.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub magic: [u8; 4],
    pub offsets: Offsets,
    /// The seed's settings the hook acts on, like [`FREE_BOMBCHUS`].
    pub flags: u32,
    /// The size of the lines that follow.
    pub size: u32,
    /// The size of the credits after them.
    pub credits: u32,
    /// The size of the [`Slot`] after them, 0 for none.
    pub slot: u32,
}

const _: () = assert!(cfg!(target_endian = "little"));

impl Header {
    /// A copy of the header at the start of `bytes`.
    pub fn read(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= HEADER);
        // SAFETY: the header is plain u32s and bytes, all of it in `bytes`.
        unsafe { bytes.as_ptr().cast::<Self>().read_unaligned() }
    }
}

macro_rules! offsets {
    ($($(#[doc = $doc:literal])* $field:ident,)*) => {
        /// Where the game keeps what the hook needs, relative to `main`.
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Offsets {
            $($(#[doc = $doc])* pub $field: u32,)*
        }

        #[cfg(not(target_os = "none"))]
        impl Offsets {
            const COUNT: usize = [$(stringify!($field)),*].len();

            fn words(self) -> [u32; Self::COUNT] {
                [$(self.$field),*]
            }

            #[cfg(test)]
            fn from_words([$($field),*]: [u32; Self::COUNT]) -> Self {
                Self { $($field),* }
            }
        }
    };
}

offsets! {
    /// The event argument setter `content_name` wraps.
    setter,
    /// The drop reader `drop_actor` wraps.
    reader,
    /// The pouch count query.
    count,
    /// The pointer to the pouch manager.
    manager,
    /// The name hash an `ActorHolder` keys its slots by.
    hash,
    /// Requests a holder slot's actor be deleted.
    delete,
    /// Creates an actor in a holder slot.
    create,
    /// A holder slot's vtable.
    slot_vtable,
    /// The hooked functions: map loaded, chest content made, pouch add,
    /// Link's update, a pickup window opening.
    map_loaded,
    chest_made,
    granted,
    link_update,
    picked,
    /// The game's own `malloc`.
    malloc,
    /// Builds an event request in place.
    make_arg,
    /// Asks the event manager to start a request.
    request,
    /// Sets and releases the actor links a request holds.
    set_link,
    release_link,
    /// What an empty actor link points at.
    link_empty,
    /// GameData's Bool getter and setter, by name hash.
    get_bool,
    set_bool,
    /// The pointers to the event manager, GameData and the world.
    events,
    gamedata,
    world,
    /// Sets the world clock, in seconds since midnight.
    set_time,
    /// The player actor's vtable, and its movement component's.
    player_vtable,
    move_vtable,
    /// Moves a character controller to a point, and the vtable of Link's
    /// controller, which holds it.
    move_to,
    controller_vtable,
    /// Sets the speed and direction Player asks the controller for.
    run_setter,
    /// The game's stubs for `nn::hid::GetNpadStates`, by style: full key,
    /// handheld, joy dual.
    npad_full,
    npad_handheld,
    npad_dual,
    /// The hooked functions: the Items page built (entered), the Items page
    /// left, and the UI update each frame.
    items_built,
    items_left,
    ui_update,
    /// Sets an Items page cell's actor, and a cell button's unfocus and
    /// focus, which refresh the item detail panel.
    cell_set,
    unfocus,
    focus,
    /// Seeks a layout animation to a frame, which colours an Items page
    /// cell's icon: usable, or on a button.
    anim_frame,
    /// The item kind on a button for an age (the manager, child, 1).
    equipped,
}

/// The table at `TABLE`. Each line is a level count, then each
/// level lowest first: the actor it drops, then the pouch name it's owned
/// as, each a length byte and the name with a NUL.
///
/// The credits follow: each a name the same way, then the [`Credit`] its
/// find sets. A chest's flow credits a dungeon item by its exact name, so a
/// copy of one is credited here.
pub struct Table<'a> {
    pub offsets: &'a Offsets,
    pub flags: u32,
    lines: &'a [u8],
    credits: &'a [u8],
    pub slot: Option<Slot<'a>>,
}

impl<'a> Table<'a> {
    /// The table `header` starts, `body` being what follows it.
    pub fn parse(header: &'a Header, body: &'a [u8]) -> Option<Self> {
        let (lines, rest) = body.split_at_checked(header.size as usize)?;
        let (credits, rest) = rest.split_at_checked(header.credits as usize)?;
        let slot = rest.get(..header.slot as usize)?;
        (header.magic == *MAGIC).then_some(Self {
            offsets: &header.offsets,
            flags: header.flags,
            lines,
            credits,
            slot: Slot::parse(slot),
        })
    }

    /// What finding `name` sets.
    pub fn credit(&self, name: &[u8]) -> Option<Credit> {
        let mut rest = self.credits;
        while let Some((&len, after)) = rest.split_first() {
            let found = after.get(..len as usize)?;
            let entry = after.get(len as usize + 1..)?.first_chunk()?;
            if found == name {
                return Credit::read(entry);
            }
            rest = &after[len as usize + 1 + Credit::SIZE..];
        }
        None
    }

    /// The level that drops or is owned as `name`, with its line's number
    /// and levels.
    pub fn find(&self, name: &[u8]) -> Option<(u16, Levels<'a>, Level<'a>)> {
        let mut rest = self.lines;
        for i in 0.. {
            let (&n, after) = rest.split_first()?;
            let line = Levels {
                rest: after,
                left: n,
            };
            if let Some(level) = line.clone().find(|l| l.drop == name || l.pouch == name) {
                return Some((i, line, level));
            }
            rest = line.skip_all()?;
        }
        None
    }
}

/// What finding an item sets in GameData, by name hash. In the table: a
/// kind byte, the hash, then the value, little endian.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Credit {
    /// A Bool, set true.
    Bool(u32),
    /// An Int and its saved copy, set to the value.
    Int(u32, i32),
    /// An Int and its saved copy, the value added: a single small key
    /// counts one more for its own dungeon, wherever Link is.
    Add(u32, i32),
}

impl Credit {
    const SIZE: usize = 9;

    fn read(&[kind, h0, h1, h2, h3, v0, v1, v2, v3]: &[u8; Self::SIZE]) -> Option<Self> {
        let hash = u32::from_le_bytes([h0, h1, h2, h3]);
        match kind {
            0 => Some(Self::Bool(hash)),
            1 => Some(Self::Int(hash, i32::from_le_bytes([v0, v1, v2, v3]))),
            2 => Some(Self::Add(hash, i32::from_le_bytes([v0, v1, v2, v3]))),
            _ => None,
        }
    }

    #[cfg(not(target_os = "none"))]
    fn bytes(self) -> [u8; Self::SIZE] {
        let (kind, hash, value) = match self {
            Self::Bool(hash) => (0, hash, 0),
            Self::Int(hash, value) => (1, hash, value),
            Self::Add(hash, value) => (2, hash, value),
        };
        let mut out = [kind; Self::SIZE];
        out[1..5].copy_from_slice(&hash.to_le_bytes());
        out[5..].copy_from_slice(&value.to_le_bytes());
        out
    }
}

/// A line's levels, lowest first.
#[derive(Clone)]
pub struct Levels<'a> {
    rest: &'a [u8],
    left: u8,
}

impl<'a> Levels<'a> {
    /// What follows the line, or `None` if it runs past the table.
    fn skip_all(mut self) -> Option<&'a [u8]> {
        while self.left > 0 {
            self.next()?;
        }
        Some(self.rest)
    }
}

/// One level. Both names are followed by a NUL in the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level<'a> {
    pub drop: &'a [u8],
    pub pouch: &'a [u8],
}

impl<'a> Levels<'a> {
    fn name(&mut self) -> Option<&'a [u8]> {
        take_name(&mut self.rest)
    }
}

/// The name at the start of `rest`, a length byte, the name and a NUL,
/// moving `rest` past it.
fn take_name<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let (&len, after) = rest.split_first()?;
    let name = after.get(..len as usize)?;
    *rest = after.get(len as usize + 1..)?;
    Some(name)
}

/// A menu slot several actors share, as the seed writes it: the age flag's
/// hash, the slot's cell on the Items page, the actor count, each actor's
/// age bits, then each one's item kind, then each one's name the way the
/// lines write it. Actors are in the game's table order.
#[derive(Clone, Copy, Debug)]
pub struct Slot<'a> {
    /// The GameData Bool true while Link is a child.
    pub age_flag: u32,
    /// Where the slot's cell is in the Items page's list.
    pub cell: u8,
    pub actors: MultiSlot<'a>,
    /// What the cell setter takes with each actor: its kind in the game's
    /// item kind list, less one.
    kinds: &'a [u8],
    names: &'a [u8],
}

impl<'a> Slot<'a> {
    fn parse(bytes: &'a [u8]) -> Option<Self> {
        let (hash, rest) = bytes.split_first_chunk()?;
        let (&[cell, n], rest) = rest.split_first_chunk()?;
        let (ages, rest) = rest.split_at_checked(n as usize)?;
        let (kinds, names) = rest.split_at_checked(n as usize)?;
        Some(Self {
            age_flag: u32::from_le_bytes(*hash),
            cell,
            actors: MultiSlot { ages },
            kinds,
            names,
        })
    }

    /// Actor `i`'s name, a NUL after it in the table, and its kind.
    pub fn actor(&self, i: usize) -> Option<(&'a [u8], u8)> {
        let mut rest = self.names;
        for _ in 0..i {
            take_name(&mut rest)?;
        }
        Some((take_name(&mut rest)?, *self.kinds.get(i)?))
    }

    /// The actor named `name`.
    pub fn index(&self, name: &[u8]) -> Option<usize> {
        let mut rest = self.names;
        (0..self.kinds.len()).find(|_| take_name(&mut rest) == Some(name))
    }
}

/// A [`Slot`] for [`encode`]: each actor as its name, age bits and kind.
#[cfg(not(target_os = "none"))]
pub struct SlotSpec<'a> {
    pub age_flag: u32,
    pub cell: u8,
    pub actors: &'a [(&'a str, u8, u8)],
}

impl<'a> Iterator for Levels<'a> {
    type Item = Level<'a>;

    fn next(&mut self) -> Option<Level<'a>> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        Some(Level {
            drop: self.name()?,
            pouch: self.name()?,
        })
    }
}

/// What to drop: the level after the highest one Link owns, or the top once
/// he owns it. `owns` gets a pouch name.
pub fn pick<'a>(levels: Levels<'a>, owns: impl Fn(&[u8]) -> bool) -> Option<&'a [u8]> {
    let (mut give, mut after) = (None, true);
    for l in levels {
        if after || owns(l.pouch) {
            give = Some(l.drop);
        }
        after = owns(l.pouch);
    }
    give
}

/// `pick`, with the pouch name `also` owned too.
pub fn pick_with<'a>(
    levels: Levels<'a>,
    also: &[u8],
    owns: &impl Fn(&[u8]) -> bool,
) -> Option<&'a [u8]> {
    pick(levels, |p| p == also || owns(p))
}

/// The line `added` is a level of and what its chests give once it's
/// owned, as the line's number and the drop name.
pub fn after_grant<'a>(
    table: &Table<'a>,
    added: &[u8],
    owns: &impl Fn(&[u8]) -> bool,
) -> Option<(u16, &'a [u8])> {
    let (line, levels, level) = table.find(added)?;
    Some((line, pick_with(levels, level.pouch, owns)?))
}

/// The table for `lines`, each lowest level first as (drop, pouch name),
/// with the seed's `flags`, `credits` by name and the `slot` Link cycles.
#[cfg(not(target_os = "none"))]
pub fn encode(
    offsets: Offsets,
    flags: u32,
    lines: &[&[(&str, &str)]],
    credits: &[(&str, Credit)],
    slot: Option<&SlotSpec>,
) -> Vec<u8> {
    fn push_name(out: &mut Vec<u8>, name: &str) {
        out.push(u8::try_from(name.len()).expect("a name is under 256 bytes"));
        out.extend(name.bytes().chain([0]));
    }
    let mut body = Vec::new();
    for line in lines {
        body.push(u8::try_from(line.len()).expect("a line has under 256 levels"));
        for name in line.iter().flat_map(|&(drop, pouch)| [drop, pouch]) {
            push_name(&mut body, name);
        }
    }
    let size = body.len();
    for (name, credit) in credits {
        push_name(&mut body, name);
        body.extend(credit.bytes());
    }
    let credits = body.len() - size;
    if let Some(slot) = slot {
        let n = u8::try_from(slot.actors.len()).expect("a slot has under 256 actors");
        body.extend(slot.age_flag.to_le_bytes());
        body.extend([slot.cell, n]);
        body.extend(slot.actors.iter().map(|a| a.1));
        body.extend(slot.actors.iter().map(|a| a.2));
        for (name, ..) in slot.actors {
            push_name(&mut body, name);
        }
    }
    let words = [size, credits, body.len() - size - credits]
        .map(|n| u32::try_from(n).expect("table under 4 GB"));
    let mut out = MAGIC.to_vec();
    for v in offsets.words().into_iter().chain([flags]).chain(words) {
        out.extend(v.to_le_bytes());
    }
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offsets() -> Offsets {
        Offsets::from_words(core::array::from_fn(|i| i as u32 + 1))
    }
    const BAGS: &[(&str, &str)] = &[("Bag_S", "Bag_S"), ("Bag_M", "Bag_M"), ("Bag_L", "Bag_L")];
    const HOOK: &[(&str, &str)] = &[("Hook", "Hook"), ("Long", "Long")];
    /// Owned under another name.
    const GLOVES: &[(&str, &str)] = &[("Glove_S", "Arm_S"), ("Glove_G", "Arm_G")];
    const CREDITS: &[(&str, Credit)] = &[
        ("Key_01", Credit::Bool(0x1234_5678)),
        ("Key_02", Credit::Int(0xdead_beef, -3)),
        ("Key_03", Credit::Int(0x0bad_f00d, 9)),
        ("Key_04", Credit::Add(0xfeed_face, 1)),
    ];

    fn with_table<T>(f: impl FnOnce(Table) -> T) -> T {
        let bytes = encode(offsets(), 0, &[HOOK, BAGS, GLOVES], CREDITS, Some(&SLOT));
        let header = Header::read(&bytes);
        assert_eq!(
            (header.size + header.credits + header.slot) as usize,
            bytes.len() - HEADER
        );
        f(Table::parse(&header, &bytes[HEADER..]).unwrap())
    }

    fn give(owned: &[&str], name: &str) -> Option<String> {
        with_table(|t| {
            let (_, line, _) = t.find(name.as_bytes())?;
            let owns = |l: &[u8]| owned.iter().any(|o| o.as_bytes() == l);
            pick(line, owns).map(|l| String::from_utf8(l.to_vec()).unwrap())
        })
    }

    #[test]
    fn the_table_round_trips() {
        with_table(|t| {
            assert_eq!(*t.offsets, offsets());
            let (_, gloves, _) = t.find(b"Glove_G").unwrap();
            let gloves: Vec<_> = gloves.collect();
            let level = |drop, pouch| Level { drop, pouch };
            assert_eq!(
                gloves,
                [
                    level(&b"Glove_S"[..], &b"Arm_S"[..]),
                    level(b"Glove_G", b"Arm_G")
                ]
            );
            assert!(t.find(b"Bag").is_none());
            assert_eq!(t.find(b"Arm_S").unwrap().2, gloves[0]);
        });
    }

    #[test]
    fn levels_come_in_order_whatever_the_chest_names() {
        for &(chest, _) in BAGS {
            assert_eq!(give(&[], chest).unwrap(), "Bag_S");
            assert_eq!(give(&["Bag_S"], chest).unwrap(), "Bag_M");
            assert_eq!(give(&["Bag_S", "Bag_M"], chest).unwrap(), "Bag_L");
            assert_eq!(give(&["Bag_S", "Bag_M", "Bag_L"], chest).unwrap(), "Bag_L");
        }
    }

    #[test]
    fn the_highest_owned_level_decides_even_with_a_gap() {
        assert_eq!(give(&["Bag_M"], "Bag_S").unwrap(), "Bag_L");
        assert_eq!(give(&["Bag_S", "Bag_L"], "Bag_S").unwrap(), "Bag_L");
        assert_eq!(give(&["Hook"], "Hook").unwrap(), "Long");
    }

    #[test]
    fn ownership_goes_by_pouch_name() {
        assert_eq!(give(&["Glove_S"], "Glove_S").unwrap(), "Glove_S");
        assert_eq!(give(&["Arm_S"], "Glove_S").unwrap(), "Glove_G");
    }

    #[test]
    fn names_outside_every_line_are_left_alone() {
        assert_eq!(give(&[], "Fake_Arrow_Fire"), None);
    }

    fn granted(owned: &[&str], added: &str) -> Option<(u16, String)> {
        with_table(|t| {
            let owns = |l: &[u8]| owned.iter().any(|o| o.as_bytes() == l);
            let (line, next) = after_grant(&t, added.as_bytes(), &owns)?;
            Some((line, String::from_utf8(next.to_vec()).unwrap()))
        })
    }

    #[test]
    fn a_grant_counts_the_added_level_as_owned() {
        assert_eq!(granted(&[], "Bag_S").unwrap(), (1, "Bag_M".into()));
        assert_eq!(granted(&["Bag_S"], "Bag_M").unwrap(), (1, "Bag_L".into()));
        assert_eq!(granted(&[], "Long").unwrap(), (0, "Long".into()));
        assert_eq!(granted(&[], "Arrow"), None);
    }

    #[test]
    fn a_grant_matches_the_drop_or_the_pouch_name() {
        assert_eq!(granted(&[], "Glove_S").unwrap(), (2, "Glove_G".into()));
        assert_eq!(granted(&[], "Arm_S").unwrap(), (2, "Glove_G".into()));
    }

    #[test]
    fn a_wrong_magic_is_no_table() {
        let mut bytes = encode(offsets(), 0, &[HOOK], &[], None);
        bytes[0] = b'X';
        assert!(Table::parse(&Header::read(&bytes), &bytes[HEADER..]).is_none());
    }

    #[test]
    fn a_credit_reads_back_with_its_kind_and_value() {
        with_table(|t| {
            for &(name, credit) in CREDITS {
                assert_eq!(t.credit(name.as_bytes()), Some(credit));
            }
            assert_eq!(t.credit(b"Key_0"), None);
            assert_eq!(t.credit(b"Hook"), None);
        });
    }

    #[test]
    fn credits_stay_out_of_the_lines() {
        with_table(|t| assert!(t.find(b"Key_01").is_none()));
    }

    #[test]
    fn a_table_cut_short_is_no_table() {
        let bytes = encode(offsets(), 0, &[HOOK], CREDITS, Some(&SLOT));
        let cut = &bytes[HEADER..bytes.len() - 1];
        assert!(Table::parse(&Header::read(&bytes), cut).is_none());
    }

    const SLOT: SlotSpec = SlotSpec {
        age_flag: 0xc0ff_ee00,
        cell: 22,
        actors: &[("FakeCheck", 2, 44), ("FakeSaw", 2, 39), ("FakeEgg", 1, 31)],
    };

    #[test]
    fn the_slot_reads_back_by_index_and_name() {
        with_table(|t| {
            let slot = t.slot.unwrap();
            assert_eq!((slot.age_flag, slot.cell), (0xc0ff_ee00, 22));
            assert_eq!(slot.actors.ages, [2, 2, 1]);
            assert_eq!(slot.actor(1), Some((&b"FakeSaw"[..], 39)));
            assert_eq!(slot.actor(2), Some((&b"FakeEgg"[..], 31)));
            assert_eq!(slot.actor(3), None);
            assert_eq!(slot.index(b"FakeEgg"), Some(2));
            assert_eq!(slot.index(b"FakeSa"), None);
        });
    }

    #[test]
    fn a_table_without_a_slot_has_none() {
        let bytes = encode(offsets(), 0, &[HOOK], CREDITS, None);
        let header = Header::read(&bytes);
        assert!(
            Table::parse(&header, &bytes[HEADER..])
                .unwrap()
                .slot
                .is_none()
        );
    }
}
