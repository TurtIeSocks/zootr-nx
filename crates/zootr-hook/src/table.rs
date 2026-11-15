//! The table the seed writes into the game: its header, the offsets of what
//! the hook needs, the progressive lines, and how a level is picked.

pub const MAGIC: &[u8; 4] = b"ZLT2";

/// The header's flag for a seed whose bombchus need no bomb bag.
pub const FREE_BOMBCHUS: u32 = 1;
/// The header's flag for a seed where Link runs faster in the Bunny Hood.
pub const BUNNY_HOOD: u32 = 2;
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
    /// Link's update.
    map_loaded,
    chest_made,
    granted,
    link_update,
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
}

/// The table at `TABLE`. Each line is a level count, then each
/// level lowest first: the actor it drops, then the pouch name it's owned
/// as, each a length byte and the name with a NUL.
pub struct Table<'a> {
    pub offsets: &'a Offsets,
    pub flags: u32,
    lines: &'a [u8],
}

impl<'a> Table<'a> {
    pub fn parse(header: &'a Header, lines: &'a [u8]) -> Option<Self> {
        (header.magic == *MAGIC).then_some(Self {
            offsets: &header.offsets,
            flags: header.flags,
            lines,
        })
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
        let (&len, after) = self.rest.split_first()?;
        let name = after.get(..len as usize)?;
        self.rest = after.get(len as usize + 1..)?;
        Some(name)
    }
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
/// with the seed's `flags`.
#[cfg(not(target_os = "none"))]
pub fn encode(offsets: Offsets, flags: u32, lines: &[&[(&str, &str)]]) -> Vec<u8> {
    let mut body = Vec::new();
    for line in lines {
        body.push(u8::try_from(line.len()).expect("a line has under 256 levels"));
        for name in line.iter().flat_map(|&(drop, pouch)| [drop, pouch]) {
            body.push(u8::try_from(name.len()).expect("a name is under 256 bytes"));
            body.extend(name.bytes().chain([0]));
        }
    }
    let size = u32::try_from(body.len()).expect("table under 4 GB");
    let mut out = MAGIC.to_vec();
    for v in offsets.words().into_iter().chain([flags, size]) {
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

    fn with_table<T>(f: impl FnOnce(Table) -> T) -> T {
        let bytes = encode(offsets(), 0, &[HOOK, BAGS, GLOVES]);
        let header = Header::read(&bytes);
        assert_eq!(header.size as usize, bytes.len() - HEADER);
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
        assert_eq!(give(&[], "Item_Arrow_Fire"), None);
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
        let mut bytes = encode(offsets(), 0, &[HOOK]);
        bytes[0] = b'X';
        assert!(Table::parse(&Header::read(&bytes), &bytes[HEADER..]).is_none());
    }
}
