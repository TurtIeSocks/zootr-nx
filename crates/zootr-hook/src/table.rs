//! The table the seed writes into the game: its header, the progressive
//! lines, how a level is picked, the flags an item sets as it's found, the
//! menu slots Link cycles, and how many twinned holders ([`Spot`]) the seed
//! wrote. The hook only reads it: a cheat that writes it many
//! times a second would put back anything the hook wrote.

use crate::look::Look;
#[cfg(not(target_os = "none"))]
use crate::look::LookSpec;
#[cfg(not(target_os = "none"))]
use crate::spots::Spot;
use crate::ui::{MAX_SLOTS, MultiSlot, Page};

pub const MAGIC: &[u8; 4] = b"ZLTA";

/// The header's flag for a seed whose bombchus need no bomb bag.
pub const FREE_BOMBCHUS: u32 = 1;
/// The header's flag for a seed where Link runs faster in the Bunny Hood.
pub const BUNNY_HOOD: u32 = 2;
/// The header's flag for a seed where Link picks what each shared menu
/// slot (trade, masks) shows.
pub const MENU_CYCLE: u32 = 4;
/// The header's flag for a seed with ice traps, which spring on Link's
/// update.
pub const ICE_TRAPS: u32 = 8;
/// The header's flag for a seed carrying the mailbox: the app's requests
/// and the dev switches, held on Link's update.
pub const DEV: u32 = 16;
/// The header's flag for a seed whose bombchus need a Bombchu Bag, which
/// sets their own expansion value.
pub const BOMBCHU_BAG: u32 = 32;
/// The header's flag for a seed where child Link wears the boots: the
/// equipment slots' child test lets their slot through ([`gear_through`]).
pub const CHILD_GEAR: u32 = 64;
/// The equipment slot the boots go in.
pub const BOOTS_SLOT: u32 = 9;

/// Whether the equipment slots' child test lets `slot` through for a
/// child in a seed with `flags`: the game refuses a child the armor, boots
/// and hand armor slots (8 to 10), so nothing of theirs is made on him.
/// [`CHILD_GEAR`] lets the boots through.
pub fn gear_through(slot: u32, flags: u32) -> bool {
    (slot == BOOTS_SLOT || slot == 8) && flags & CHILD_GEAR != 0
}
pub const HEADER: usize = size_of::<Header>();

/// The table's start, little endian like the game.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub magic: [u8; 4],
    /// The seed's settings the hook acts on, like [`FREE_BOMBCHUS`].
    pub flags: u32,
    /// The size of the lines that follow.
    pub size: u32,
    /// The size of the credits after them.
    pub credits: u32,
    /// The size of the [`Slot`]s after them, 0 for none.
    pub slots: u32,
    /// How many [`Spot`]s the seed wrote to GameData.
    pub spots: u32,
    /// The size of the [`Look`] after the slots, 0 for none.
    pub look: u32,
}

const _: () = assert!(cfg!(target_endian = "little"));

/// What the header's magic says is at `TABLE`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Found {
    /// Zero: no table written yet, so every hook leaves the game as it is.
    Nothing,
    Table,
    /// Anything else: a broken install.
    Broken,
}

impl Header {
    /// What the magic says is here.
    pub fn found(&self) -> Found {
        match self.magic {
            [0, 0, 0, 0] => Found::Nothing,
            m if m == *MAGIC => Found::Table,
            _ => Found::Broken,
        }
    }

    /// A copy of the header at the start of `bytes`.
    pub fn read(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= HEADER);
        // SAFETY: the header is plain u32s and bytes, all of it in `bytes`.
        unsafe { bytes.as_ptr().cast::<Self>().read_unaligned() }
    }
}

/// Where the game keeps what the hook needs, relative to `main`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offsets {
    /// The event argument setter `content_name` wraps.
    pub setter: u32,
    /// The drop reader `drop_actor` wraps.
    pub reader: u32,
    /// The pouch count query.
    pub count: u32,
    /// The pointer to the pouch manager.
    pub manager: u32,
    /// The name hash an `ActorHolder` keys its slots by.
    pub hash: u32,
    /// Requests a holder slot's actor be deleted.
    pub delete: u32,
    /// Creates an actor in a holder slot.
    pub create: u32,
    /// A holder slot's vtable.
    pub slot_vtable: u32,
    /// The hooked functions: map loaded, chest content made, pouch add,
    /// Link's update, a pickup window opening, and a pickup going into
    /// the pouch, with a window or without.
    pub map_loaded: u32,
    pub chest_made: u32,
    pub granted: u32,
    pub link_update: u32,
    pub picked: u32,
    pub taken: u32,
    /// The hooked nock, then the drawn bow's component, its projectile's
    /// name, and the copy of that name into a fixed string.
    pub nock: u32,
    pub drawn_bow: u32,
    pub shootable: u32,
    pub name_copy: u32,
    /// The hooked out of ammo handler, which picks the empty message.
    pub emptied: u32,
    /// Link's child test, which the bow draw's guide asks.
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
    /// What an empty actor link points at.
    pub link_empty: u32,
    /// GameData's Bool getter and setter, and its UInt64 getter, by name
    /// hash.
    pub get_bool: u32,
    pub set_bool: u32,
    pub get_u64: u32,
    /// The pointers to the event manager, GameData and the world.
    pub events: u32,
    pub gamedata: u32,
    pub world: u32,
    /// Sets the world clock, in seconds since midnight.
    pub set_time: u32,
    /// The player actor's vtable, and its movement component's.
    pub player_vtable: u32,
    pub move_vtable: u32,
    /// Moves a character controller to a point, and the vtable of Link's
    /// controller, which holds it.
    pub move_to: u32,
    pub controller_vtable: u32,
    /// Sets the speed and direction Player asks the controller for.
    pub run_setter: u32,
    /// The hooked functions: the Items page built (entered), the Items page
    /// left, the UI update each frame, and the Equipment page built and
    /// left.
    pub items_built: u32,
    pub items_left: u32,
    pub ui_update: u32,
    pub equipment_built: u32,
    pub equipment_left: u32,
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
    /// to the name), and the pointer to the pouch menu.
    pub worn: u32,
    pub pouch_menu: u32,
    /// Applies a cell layout's animation frames (the cell, 0).
    pub redraw: u32,
    /// Loads an Equipment cell's picture by name (its picture, &name, 0, 0).
    pub icon: u32,
    /// The pointer to the effect system, which holds the effect file
    /// the ice's [`Look`] sizes.
    pub effects: u32,
    /// The resident actors' create (the manager, child), which makes the
    /// player of that age and has the other age's go, on every area load,
    /// and their wake (the manager, &position, &3x3 rotation). See
    /// `docs/research/character-select.md`.
    pub resident_create: u32,
    pub cancel_sleep: u32,
    /// The request and release pause nodes' bodies (`SeqOneShotRequestPause`,
    /// `SeqOneShotReleasePause`), each taking the node.
    pub request_pause: u32,
    pub release_pause: u32,
}

/// The table at `TABLE`. Each line is a level count, [`KEYED`] or not,
/// then each level lowest first: the actor it drops, then the pouch name
/// it's owned as, each a length byte and the name with a NUL.
///
/// The credits follow: each a name the same way, then the [`Credit`] its
/// find sets. A chest's flow credits a dungeon item by its exact name, so a
/// copy of one is credited here.
pub struct Table<'a> {
    pub offsets: &'static Offsets,
    pub flags: u32,
    lines: &'a [u8],
    credits: &'a [u8],
    /// The slots Link cycles, in the order the seed wrote them.
    pub slots: [Option<Slot<'a>>; MAX_SLOTS],
    /// How many [`Spot`]s the seed wrote to GameData.
    pub spots: u32,
    pub look: Option<Look<'a>>,
}

impl<'a> Table<'a> {
    /// The table `header` starts, `body` being what follows it.
    pub fn parse(header: &'a Header, body: &'a [u8]) -> Option<Self> {
        let (lines, rest) = body.split_at_checked(header.size as usize)?;
        let (credits, rest) = rest.split_at_checked(header.credits as usize)?;
        let (slots, rest) = rest.split_at_checked(header.slots as usize)?;
        let look = rest.get(..header.look as usize)?;
        (header.magic == *MAGIC).then_some(Self {
            offsets: &crate::OFFSETS,
            flags: header.flags,
            lines,
            credits,
            slots: Slot::parse_all(slots),
            spots: header.spots,
            look: Look::parse(look),
        })
    }

    /// What finding `name` sets.
    pub fn credit(&self, name: &[u8]) -> Option<Credit> {
        if self.flags & ICE_TRAPS != 0 && name.starts_with(crate::traps::ITEM.as_bytes()) {
            return Some(Credit::Freeze);
        }
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

    /// Each line: its number, whether it's [`KEYED`], and its levels.
    fn lines(&self) -> impl Iterator<Item = (u16, bool, Levels<'a>)> {
        let mut rest = self.lines;
        (0..).map_while(move |i| {
            let (&n, after) = rest.split_first()?;
            let line = Levels {
                rest: after,
                left: n & !KEYED,
            };
            rest = line.clone().skip_all().unwrap_or_default();
            Some((i, n & KEYED != 0, line))
        })
    }

    /// The level that drops or is owned as `name`, with its line's number
    /// and levels. A keyed line answers only to its bottom level's drop.
    pub fn find(&self, name: &[u8]) -> Option<(u16, Levels<'a>, Level<'a>)> {
        self.lines().find_map(|(i, keyed, line)| {
            let mut levels = line.clone();
            let level = match keyed {
                true => levels.next().filter(|l| l.drop == name),
                false => levels.find(|l| l.drop == name || l.pouch == name),
            }?;
            Some((i, line, level))
        })
    }
}

/// A line count's bit for a line that acts only on its bottom level's drop,
/// a name nothing but its own holders carry. A grant reaches it by the
/// pouch names above that. A prize chest's ammo is one: a Red Rupee copy,
/// then the ammo once Link owns what fires it.
pub const KEYED: u8 = 0x80;

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
    /// An ice trap: Link freezes once he's free to.
    Freeze,
}

impl Credit {
    const SIZE: usize = 9;

    fn read(&[kind, h0, h1, h2, h3, v0, v1, v2, v3]: &[u8; Self::SIZE]) -> Option<Self> {
        let hash = u32::from_le_bytes([h0, h1, h2, h3]);
        match kind {
            0 => Some(Self::Bool(hash)),
            1 => Some(Self::Int(hash, i32::from_le_bytes([v0, v1, v2, v3]))),
            2 => Some(Self::Add(hash, i32::from_le_bytes([v0, v1, v2, v3]))),
            3 => Some(Self::Freeze),
            _ => None,
        }
    }

    #[cfg(not(target_os = "none"))]
    fn bytes(self) -> [u8; Self::SIZE] {
        let (kind, hash, value) = match self {
            Self::Bool(hash) => (0, hash, 0),
            Self::Int(hash, value) => (1, hash, value),
            Self::Add(hash, value) => (2, hash, value),
            Self::Freeze => (3, 0, 0),
        };
        let mut out = [kind; Self::SIZE];
        out[1..5].copy_from_slice(&hash.to_le_bytes());
        out[5..].copy_from_slice(&value.to_le_bytes());
        out
    }
}

/// FNV-1a, the hook's name hash.
pub fn fnv(name: &[u8]) -> u32 {
    name.iter().fold(0x811c_9dc5_u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
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

/// A menu slot several actors share, as the seed writes it: the page,
/// the age flag's hash, the slot's cell on its page, the actor count, each
/// actor's age bits, then each one's kind, then the size of the names and
/// each one's name the way the lines write it. Actors are in the game's
/// table order.
#[derive(Clone, Copy, Debug)]
pub struct Slot<'a> {
    pub page: Page,
    /// The GameData Bool true while Link is a child.
    pub age_flag: u32,
    /// Where the slot's cell is in its page's list.
    pub cell: u8,
    pub actors: MultiSlot<'a>,
    /// What the page draws each actor by. On the Items page, what the cell
    /// setter takes: its kind in the game's item kind list, less one. On
    /// the Equipment page, its icon's frame.
    kinds: &'a [u8],
    names: &'a [u8],
}

impl<'a> Slot<'a> {
    /// Each slot in `bytes`, up to [`MAX_SLOTS`].
    fn parse_all(mut bytes: &'a [u8]) -> [Option<Self>; MAX_SLOTS] {
        let mut slots = [None; MAX_SLOTS];
        for slot in &mut slots {
            let Some((read, rest)) = Self::parse(bytes) else {
                break;
            };
            *slot = Some(read);
            bytes = rest;
        }
        slots
    }

    /// The slot `bytes` starts with, and what follows it.
    fn parse(bytes: &'a [u8]) -> Option<(Self, &'a [u8])> {
        let (&[page], rest) = bytes.split_first_chunk()?;
        let (hash, rest) = rest.split_first_chunk()?;
        let (&[cell, n], rest) = rest.split_first_chunk()?;
        let (ages, rest) = rest.split_at_checked(n as usize)?;
        let (kinds, rest) = rest.split_at_checked(n as usize)?;
        let (size, rest) = rest.split_first_chunk()?;
        let (names, rest) = rest.split_at_checked(u16::from_le_bytes(*size).into())?;
        let slot = Self {
            page: Page::from_u8(page)?,
            age_flag: u32::from_le_bytes(*hash),
            cell,
            actors: MultiSlot { ages },
            kinds,
            names,
        };
        Some((slot, rest))
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
    pub page: Page,
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

/// Each line `added` is a level of and what its chests give once it's
/// owned, as the line's number and the drop name. A keyed line counts it
/// only as a pouch name above its bottom.
pub fn after_grant<'a>(
    table: &Table<'a>,
    added: &[u8],
    owns: &impl Fn(&[u8]) -> bool,
) -> impl Iterator<Item = (u16, &'a [u8])> {
    table.lines().filter_map(move |(i, keyed, line)| {
        let mut levels = line.clone().skip(keyed.into());
        let level = levels.find(|l| match keyed {
            true => l.pouch == added,
            false => l.drop == added || l.pouch == added,
        })?;
        Some((i, pick_with(line, level.pouch, owns)?))
    })
}

/// The table for `lines`, then the [`KEYED`] lines `keyed`, each lowest
/// level first as (drop, pouch name), with the seed's `flags`, `credits`
/// by name, the `slots` Link cycles and how many holders' `spots` the seed
/// writes to GameData.
#[cfg(not(target_os = "none"))]
#[allow(clippy::too_many_arguments)]
pub fn encode(
    flags: u32,
    lines: &[&[(&str, &str)]],
    keyed: &[&[(&str, &str)]],
    credits: &[(&str, Credit)],
    slots: &[SlotSpec],
    spots: &[Spot],
    look: Option<&LookSpec>,
) -> Vec<u8> {
    fn push_name(out: &mut Vec<u8>, name: &str) {
        out.push(u8::try_from(name.len()).expect("a name is under 256 bytes"));
        out.extend(name.bytes().chain([0]));
    }
    let mut body = Vec::new();
    let marked = lines
        .iter()
        .map(|l| (l, 0))
        .chain(keyed.iter().map(|l| (l, KEYED)));
    for (line, mark) in marked {
        let n = u8::try_from(line.len()).ok().filter(|&n| n & KEYED == 0);
        body.push(n.expect("a line has under 128 levels") | mark);
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
    assert!(
        slots.len() <= MAX_SLOTS,
        "the hook cycles {MAX_SLOTS} slots"
    );
    for slot in slots {
        let n = u8::try_from(slot.actors.len()).expect("a slot has under 256 actors");
        body.push(slot.page as u8);
        body.extend(slot.age_flag.to_le_bytes());
        body.extend([slot.cell, n]);
        body.extend(slot.actors.iter().map(|a| a.1));
        body.extend(slot.actors.iter().map(|a| a.2));
        let mut names = Vec::new();
        for (name, ..) in slot.actors {
            push_name(&mut names, name);
        }
        let len = u16::try_from(names.len()).expect("a slot's names are under 64 KB");
        body.extend(len.to_le_bytes());
        body.extend(names);
    }
    let slot = body.len() - size - credits;
    body.extend(look.map(LookSpec::bytes).unwrap_or_default());
    let look = body.len() - size - credits - slot;
    let words = [size, credits, slot, spots.len(), look]
        .map(|n| u32::try_from(n).expect("table under 4 GB"));
    let mut out = MAGIC.to_vec();
    for v in [flags].into_iter().chain(words) {
        out.extend(v.to_le_bytes());
    }
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_child_gear_flag_lets_only_the_boots_through() {
        for slot in [8, BOOTS_SLOT, 10] {
            assert!(!gear_through(slot, 0), "{slot}");
            assert!(!gear_through(slot, !CHILD_GEAR), "{slot}");
        }
        assert!(gear_through(BOOTS_SLOT, CHILD_GEAR));
        assert!(!gear_through(8, !0));
        assert!(!gear_through(10, !0));
    }
    use crate::spots::tests::SPOTS;

    const BAGS: &[(&str, &str)] = &[("Bag_S", "Bag_S"), ("Bag_M", "Bag_M"), ("Bag_L", "Bag_L")];
    const HOOK: &[(&str, &str)] = &[("Hook", "Hook"), ("Long", "Long")];
    /// Owned under another name.
    const GLOVES: &[(&str, &str)] = &[("Glove_S", "Arm_S"), ("Glove_G", "Arm_G")];
    /// Keyed: a rupee only its holders drop, then seeds once Link owns any
    /// bag.
    const SEEDS: &[(&str, &str)] = &[
        ("Rupee_FakeCopy", "Rupee_FakeCopy"),
        ("Seed", "Bag_S"),
        ("Seed", "Bag_M"),
        ("Seed", "Bag_L"),
    ];
    const CREDITS: &[(&str, Credit)] = &[
        ("Key_01", Credit::Bool(0x1234_5678)),
        ("Key_02", Credit::Int(0xdead_beef, -3)),
        ("Key_03", Credit::Int(0x0bad_f00d, 9)),
        ("Key_04", Credit::Add(0xfeed_face, 1)),
        ("FakeTrap", Credit::Freeze),
    ];

    fn with_table<T>(f: impl FnOnce(Table) -> T) -> T {
        let bytes = encode(
            0,
            &[HOOK, BAGS, GLOVES],
            &[SEEDS],
            CREDITS,
            &[SLOT, MASKS],
            SPOTS,
            Some(&look()),
        );
        let header = Header::read(&bytes);
        assert_eq!(
            (header.size + header.credits + header.slots + header.look) as usize,
            bytes.len() - HEADER
        );
        assert_eq!(header.spots as usize, SPOTS.len());
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
            let (line, next) = after_grant(&t, added.as_bytes(), &owns).next()?;
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
    fn a_keyed_line_answers_only_to_its_bottom_drop() {
        assert_eq!(give(&[], "Rupee_FakeCopy").unwrap(), "Rupee_FakeCopy");
        assert_eq!(give(&["Bag_M"], "Rupee_FakeCopy").unwrap(), "Seed");
        // The bags' own line, never the keyed one.
        assert_eq!(give(&[], "Bag_S").unwrap(), "Bag_S");
        assert_eq!(give(&["Bag_S"], "Seed"), None);
    }

    #[test]
    fn a_grant_reaches_every_line_it_owns_a_level_of() {
        let all = |owned: &[&str], added: &str| {
            with_table(|t| {
                let owns = |l: &[u8]| owned.iter().any(|o| o.as_bytes() == l);
                (after_grant(&t, added.as_bytes(), &owns))
                    .map(|(line, next)| (line, String::from_utf8(next.to_vec()).unwrap()))
                    .collect::<Vec<_>>()
            })
        };
        let seeds = (3, "Seed".to_owned());
        assert_eq!(all(&[], "Bag_S"), [(1, "Bag_M".into()), seeds.clone()]);
        assert_eq!(all(&["Bag_S"], "Bag_L"), [(1, "Bag_L".into()), seeds]);
        // Its own copy and its ammo don't reach it.
        assert_eq!(all(&[], "Rupee_FakeCopy"), []);
        assert_eq!(all(&[], "Seed"), []);
    }

    #[test]
    fn a_wrong_magic_is_no_table() {
        let mut bytes = encode(0, &[HOOK], &[], &[], &[], &[], None);
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
    fn every_trap_freezes_without_an_entry_while_ice_traps_is_on() {
        let trap = format!("{}042", crate::traps::ITEM);
        let credit = |flags| {
            let bytes = encode(flags, &[HOOK], &[], CREDITS, &[], &[], None);
            let header = Header::read(&bytes);
            let t = Table::parse(&header, &bytes[HEADER..]).unwrap();
            (t.credit(trap.as_bytes()), t.credit(b"Key_01"))
        };
        let key = Some(Credit::Bool(0x1234_5678));
        assert_eq!(credit(ICE_TRAPS), (Some(Credit::Freeze), key));
        assert_eq!(credit(0), (None, key));
    }

    #[test]
    fn credits_stay_out_of_the_lines() {
        with_table(|t| assert!(t.find(b"Key_01").is_none()));
    }

    #[test]
    fn a_zero_magic_is_nothing_ours_a_table_and_any_other_broken() {
        let mut bytes = encode(0, &[HOOK], &[], &[], &[], &[], None);
        assert_eq!(Header::read(&bytes).found(), Found::Table);
        bytes[..4].copy_from_slice(&[0; 4]);
        assert_eq!(Header::read(&bytes).found(), Found::Nothing);
        bytes[0] = b'X';
        assert_eq!(Header::read(&bytes).found(), Found::Broken);
    }

    #[test]
    fn a_table_cut_short_is_no_table() {
        let bytes = encode(0, &[HOOK], &[], CREDITS, &[SLOT], SPOTS, None);
        let header = Header::read(&bytes);
        let cut = &bytes[HEADER..bytes.len() - 1];
        assert!(Table::parse(&header, cut).is_none());
    }

    fn look() -> LookSpec {
        let sizes = crate::look::Sizes {
            masks: [1 << 12 | 3, 1 << 13 | 3],
            values: [0x0100_0040, 0x6b],
            value_at: 16,
            count: 2,
        };
        LookSpec {
            sizes,
            records: vec![0x10, 0x30],
        }
    }

    #[test]
    fn the_look_reads_back_after_the_slot() {
        with_table(|t| {
            let read = t.look.unwrap();
            assert_eq!(read.sizes, look().sizes);
            assert_eq!(read.records().collect::<Vec<_>>(), [0x10, 0x30]);
        });
    }

    const SLOT: SlotSpec = SlotSpec {
        page: Page::Items,
        age_flag: 0xc0ff_ee00,
        cell: 22,
        actors: &[("FakeCheck", 2, 44), ("FakeSaw", 2, 39), ("FakeEgg", 1, 31)],
    };

    const MASKS: SlotSpec = SlotSpec {
        page: Page::Equipment,
        age_flag: 0xc0ff_ee00,
        cell: 12,
        actors: &[("FakeTruth", 3, 16), ("FakeKeaton", 3, 12)],
    };

    #[test]
    fn the_slots_read_back_by_index_and_name() {
        with_table(|t| {
            let [Some(slot), Some(masks)] = t.slots else {
                panic!("two slots");
            };
            assert_eq!((masks.page, masks.cell), (Page::Equipment, 12));
            assert_eq!(masks.actor(1), Some((&b"FakeKeaton"[..], 12)));
            assert_eq!(masks.index(b"FakeTruth"), Some(0));
            assert_eq!(slot.page, Page::Items);
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
        let bytes = encode(0, &[HOOK], &[], CREDITS, &[], &[], None);
        let header = Header::read(&bytes);
        assert!(
            Table::parse(&header, &bytes[HEADER..])
                .unwrap()
                .slots
                .iter()
                .all(Option::is_none)
        );
    }
}
