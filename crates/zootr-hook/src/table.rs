//! The table the seed writes into the game: its header, the progressive
//! lines, how a level is picked, the flags an item sets as it's found, the
//! menu slot Link cycles, and the twinned holders whose drop sets a flag as
//! Link takes it. The hook only reads it: a cheat that writes it many
//! times a second would put back anything the hook wrote.

use crate::look::Look;
#[cfg(not(target_os = "none"))]
use crate::look::LookSpec;
use crate::ui::MultiSlot;

pub const MAGIC: &[u8; 4] = b"ZLTA";

/// The header's flag for a seed whose bombchus need no bomb bag.
pub const FREE_BOMBCHUS: u32 = 1;
/// The header's flag for a seed where Link runs faster in the Bunny Hood.
pub const BUNNY_HOOD: u32 = 2;
/// The header's flag for a seed where Link picks what the trade slot shows.
pub const TRADE_CYCLE: u32 = 4;
/// The header's flag for a seed with ice traps, which spring on Link's
/// update.
pub const ICE_TRAPS: u32 = 8;
/// The header's flag for a seed carrying the mailbox: the app's requests
/// and the dev switches, held on Link's update.
pub const DEV: u32 = 16;
/// The header's flag for a seed whose bombchus need a Bombchu Bag, which
/// sets their own expansion value.
pub const BOMBCHU_BAG: u32 = 32;
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
    /// The size of the [`Slot`] after them, 0 for none.
    pub slot: u32,
    /// How many [`Spot`]s the seed wrote to GameData.
    pub spots: u32,
    /// The size of the [`Look`] after the slot, 0 for none.
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
    /// left, and the UI update each frame.
    pub items_built: u32,
    pub items_left: u32,
    pub ui_update: u32,
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
    /// The pointer to the effect system, which holds the effect file
    /// the ice's [`Look`] sizes.
    pub effects: u32,
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
    pub slot: Option<Slot<'a>>,
    /// How many [`Spot`]s the seed wrote to GameData.
    pub spots: u32,
    pub look: Option<Look<'a>>,
}

impl<'a> Table<'a> {
    /// The table `header` starts, `body` being what follows it.
    pub fn parse(header: &'a Header, body: &'a [u8]) -> Option<Self> {
        let (lines, rest) = body.split_at_checked(header.size as usize)?;
        let (credits, rest) = rest.split_at_checked(header.credits as usize)?;
        let (slot, rest) = rest.split_at_checked(header.slot as usize)?;
        let look = rest.get(..header.look as usize)?;
        (header.magic == *MAGIC).then_some(Self {
            offsets: &crate::OFFSETS,
            flags: header.flags,
            lines,
            credits,
            slot: Slot::parse(slot),
            spots: header.spots,
            look: Look::parse(look),
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

/// How far a drop may lie from its holder, in metres: a pot thrown breaks
/// where it lands.
pub const REACH: f32 = 8.0;

/// The flag of the holder whose drop Link just took: `name`, lying `at`.
/// The nearest of `spots` within [`REACH`] that drops that name, of those
/// not yet `picked`. Two holders dropping the same name that close may
/// swap ticks, which leaves Link the same item either way.
pub fn nearest(
    spots: impl Iterator<Item = Spot>,
    name: &[u8],
    at: [f32; 3],
    picked: impl Fn(u32) -> bool,
) -> Option<u32> {
    let name = Spot::name_hash(name);
    let mut best: Option<(f32, u32)> = None;
    for s in spots.filter(|s| s.name == name) {
        let d = s.distance2(at);
        let nearer = best.is_none_or(|(b, _)| d < b);
        if nearer && d <= REACH * REACH && !picked(s.flag) {
            best = Some((d, s.flag));
        }
    }
    best.map(|(_, flag)| flag)
}

/// Where the spots' GameData hashes start, two to a spot: murmur3 of
/// `Rando_Spot`, so the hook needs no name hashing.
pub const SPOT_KEY: u32 = 0xa5b7_e07e;

/// A twinned holder (a pot, box, rock or hive): the Bool its drop sets
/// once taken, the drop's name by [`Spot::name_hash`], and where the holder
/// stands, in whole metres. The seed writes each as two unsaved UInt64s in
/// GameData, at [`Spot::key`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub flag: u32,
    pub name: u16,
    pub at: [i16; 3],
}

impl Spot {
    /// The GameData hash of spot `i`'s `word`: 0 its flag and name, 1
    /// where it stands.
    pub fn key(i: u32, word: u32) -> u32 {
        SPOT_KEY.wrapping_add(i.wrapping_mul(2)).wrapping_add(word)
    }

    /// The low half of a name's FNV-1a.
    pub fn name_hash(name: &[u8]) -> u16 {
        fnv(name) as u16
    }

    /// The spot from its two GameData words.
    pub fn from_words([who, at]: [u64; 2]) -> Self {
        Self {
            flag: who as u32,
            name: (who >> 32) as u16,
            at: [0, 16, 32].map(|shift| (at >> shift) as u16 as i16),
        }
    }

    /// Its two GameData words: the flag and name, then where it stands.
    #[cfg(not(target_os = "none"))]
    pub fn words(self) -> [u64; 2] {
        let who = u64::from(self.flag) | u64::from(self.name) << 32;
        let at = (self.at.iter().zip([0, 16, 32]))
            .fold(0, |at, (&v, shift)| at | u64::from(v as u16) << shift);
        [who, at]
    }

    fn distance2(&self, at: [f32; 3]) -> f32 {
        at.iter()
            .zip(self.at)
            .map(|(a, s)| a - f32::from(s))
            .map(|d| d * d)
            .sum()
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
/// by name, the `slot` Link cycles and how many holders' `spots` the seed
/// writes to GameData.
#[cfg(not(target_os = "none"))]
#[allow(clippy::too_many_arguments)]
pub fn encode(
    flags: u32,
    lines: &[&[(&str, &str)]],
    keyed: &[&[(&str, &str)]],
    credits: &[(&str, Credit)],
    slot: Option<&SlotSpec>,
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

    /// Two pots dropping a green rupee 3 m apart, one a stand-in further
    /// on, and one far off.
    const SPOTS: &[Spot] = &[
        spot(1, "Obj_FakeGreen", [10, 0, 10]),
        spot(2, "Obj_FakeGreen", [13, 0, 10]),
        spot(3, "Obj_FakeStandIn_001", [16, 0, 10]),
        spot(4, "Obj_FakeGreen", [100, 0, 10]),
    ];

    const fn spot(flag: u32, name: &str, at: [i16; 3]) -> Spot {
        // FNV-1a's low half, by hand, since `fnv` isn't const.
        let (b, mut h, mut i) = (name.as_bytes(), 0x811c_9dc5_u32, 0);
        while i < b.len() {
            h = (h ^ b[i] as u32).wrapping_mul(0x0100_0193);
            i += 1;
        }
        Spot {
            flag,
            name: h as u16,
            at,
        }
    }

    fn with_table<T>(f: impl FnOnce(Table) -> T) -> T {
        let bytes = encode(
            0,
            &[HOOK, BAGS, GLOVES],
            &[SEEDS],
            CREDITS,
            Some(&SLOT),
            SPOTS,
            Some(&look()),
        );
        let header = Header::read(&bytes);
        assert_eq!(
            (header.size + header.credits + header.slot + header.look) as usize,
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
        let mut bytes = encode(0, &[HOOK], &[], &[], None, &[], None);
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
    fn a_zero_magic_is_nothing_ours_a_table_and_any_other_broken() {
        let mut bytes = encode(0, &[HOOK], &[], &[], None, &[], None);
        assert_eq!(Header::read(&bytes).found(), Found::Table);
        bytes[..4].copy_from_slice(&[0; 4]);
        assert_eq!(Header::read(&bytes).found(), Found::Nothing);
        bytes[0] = b'X';
        assert_eq!(Header::read(&bytes).found(), Found::Broken);
    }

    #[test]
    fn a_table_cut_short_is_no_table() {
        let bytes = encode(0, &[HOOK], &[], CREDITS, Some(&SLOT), SPOTS, None);
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
        let bytes = encode(0, &[HOOK], &[], CREDITS, None, &[], None);
        let header = Header::read(&bytes);
        assert!(
            Table::parse(&header, &bytes[HEADER..])
                .unwrap()
                .slot
                .is_none()
        );
    }

    fn take(name: &str, at: [f32; 3], picked: &[u32]) -> Option<u32> {
        let spots = SPOTS.iter().map(|s| Spot::from_words(s.words()));
        nearest(spots, name.as_bytes(), at, |f| picked.contains(&f))
    }

    #[test]
    fn a_take_ticks_the_nearest_holder_dropping_that_name() {
        assert_eq!(take("Obj_FakeGreen", [10.4, 0.2, 9.0], &[]), Some(1));
        assert_eq!(take("Obj_FakeGreen", [12.0, 0.0, 10.0], &[]), Some(2));
        assert_eq!(take("Obj_FakeStandIn_001", [10.0, 0.0, 10.0], &[]), Some(3));
    }

    #[test]
    fn a_picked_holder_passes_the_tick_to_the_next() {
        assert_eq!(take("Obj_FakeGreen", [10.0, 0.0, 10.0], &[1]), Some(2));
        assert_eq!(take("Obj_FakeGreen", [10.0, 0.0, 10.0], &[1, 2]), None);
    }

    #[test]
    fn a_take_far_from_every_holder_or_of_another_name_ticks_none() {
        assert_eq!(take("Obj_FakeGreen", [50.0, 0.0, 10.0], &[]), None);
        assert_eq!(take("Obj_FakeGreen", [10.0, 9.0, 10.0], &[]), None);
        assert_eq!(take("Obj_FakeBlue", [10.0, 0.0, 10.0], &[]), None);
        assert_eq!(take("Obj_FakeGreen", [100.0, 7.0, 13.0], &[]), Some(4));
    }

    #[test]
    fn a_spot_round_trips_through_its_words_and_far_off_spots_keep_their_sign() {
        let far = spot(u32::MAX, "Obj_FakeGreen", [-32768, -1, 32767]);
        assert_eq!(Spot::from_words(far.words()), far);
        assert_eq!(
            (Spot::key(0, 0), Spot::key(0, 1), Spot::key(1, 0)),
            (SPOT_KEY, SPOT_KEY + 1, SPOT_KEY + 2)
        );
    }

    #[test]
    fn the_name_hash_is_fnvs_low_half() {
        assert_eq!(Spot::name_hash(b"Obj_FakeGreen"), SPOTS[0].name);
        assert_eq!(fnv(b""), 0x811c_9dc5);
    }
}
