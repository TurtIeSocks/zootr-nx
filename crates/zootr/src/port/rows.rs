//! Every offset in a [`Build`], each with how the matcher finds it, and the
//! source of a generated build.

use zootr_hook::Build;

/// How a row's offset is found in the other build.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Code: a function's entry or a word in it, by a masked window.
    Code,
    /// Where the branch at the named row goes, read from its matched word.
    Target(&'static str),
    /// The end of the last function before it, with its padding: where the next
    /// one starts.
    End,
    /// An import's stub, by the import's name.
    Import,
    /// A GOT slot or a bss global, by the code that loads it.
    Global,
    /// A vtable, slot by slot.
    Vtable,
    /// From the NSO header and MOD0.
    Header(Rule),
}

/// A place the NSO header and MOD0 give.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rule {
    /// Text's end.
    TextEnd,
    /// Rodata's end, rounded up to 16 bytes.
    RodataEnd,
    /// Data's start.
    DataStart,
    /// bss's end, rounded up to 16 bytes.
    BssEnd,
    /// bss's end, rounded up to its page.
    ListEnd,
    /// 16 bytes short of that.
    Panic,
}

/// One offset of a [`Build`].
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub group: &'static str,
    pub field: &'static str,
    pub kind: Kind,
}

impl Row {
    pub fn name(&self) -> String {
        format!("{}.{}", self.group, self.field)
    }
}

/// Lists every field of [`Build`] once, in order, with its kind: the rows,
/// a build's values in that order, and a build from them. A field left out
/// doesn't compile.
macro_rules! rows {
    ($($group:ident: $ty:ident { $($field:ident: $kind:expr,)* })*) => {
        pub const ROWS: &[Row] = &[$($(Row {
            group: stringify!($group),
            field: stringify!($field),
            kind: $kind,
        },)*)*];

        /// `b`'s offsets, in [`ROWS`]' order.
        pub fn values(b: &Build) -> Vec<u32> {
            vec![$($(b.$group.$field,)*)*]
        }

        /// The build holding `values`, in [`ROWS`]' order.
        pub fn assemble(values: &[u32]) -> Build {
            assert_eq!(values.len(), ROWS.len());
            let mut v = values.iter().copied();
            Build {$($group: zootr_hook::$ty {$($field: v.next().unwrap(),)*},)*}
        }

        /// A group's type, as the source names it.
        fn type_name(group: &str) -> &'static str {
            match group {
                $(stringify!($group) => stringify!($ty),)*
                _ => unreachable!("no group {group}"),
            }
        }
    };
}

use Kind::{Code, End, Global, Header, Import, Target, Vtable};

rows! {
    entries: Entries {
        map_loaded: Code,
        chest_made: Code,
        granted: Code,
        link_update: Code,
        picked: Code,
        taken: Code,
        nock: Code,
        emptied: Code,
        items_built: Code,
        items_left: Code,
        ui_update: Code,
        equipment_built: Code,
        equipment_left: Code,
        resident_create: Code,
        ocarina_input: Code,
        select_drops: Code,
    }
    funcs: Funcs {
        setter: Code,
        reader: Code,
        count: Code,
        hash: Code,
        delete: Code,
        create: Code,
        drawn_bow: Code,
        shootable: Code,
        name_copy: Code,
        childhood: Code,
        make_actor: Code,
        malloc: Code,
        make_arg: Code,
        request: Code,
        set_link: Code,
        release_link: Code,
        get_bool: Code,
        set_bool: Code,
        get_u64: Code,
        set_time: Code,
        move_to: Code,
        run_setter: Code,
        cell_set: Code,
        unfocus: Code,
        focus: Code,
        anim_frame: Code,
        equipped: Code,
        worn: Code,
        redraw: Code,
        icon: Code,
        cancel_sleep: Code,
        request_pause: Code,
        release_pause: Code,
    }
    calls: Calls {
        content1: Code,
        content2: Code,
        drop1: Code,
        drop2: Code,
        seed_guide: Code,
        run: Code,
        npad_full: Code,
        npad_handheld: Code,
        npad_dual: Code,
        gear_request: Code,
        gear_actor: Code,
        gear_hud: Code,
        gear_menu: Code,
        gear_worn: Code,
        gear_mark: Code,
        gear_create1: Code,
        gear_create2: Code,
        gear_create3: Code,
        gear_create4: Code,
        gear_create5: Code,
    }
    branches: Branches {
        gate: Code,
        gate_to: Code,
        catch: Code,
        catch_to: Code,
        catch_orig: Target("branches.catch"),
        boots: Code,
        boots_to: Target("branches.boots"),
    }
    imports: Imports {
        npad_full: Import,
        npad_handheld: Import,
        npad_dual: Import,
    }
    got: Got {
        events: Global,
        gamedata: Global,
        world: Global,
        pouch_menu: Global,
    }
    bss: Bss {
        manager: Global,
        effects: Global,
        actors_table: Global,
        actors_capacity: Global,
    }
    vtables: Vtables {
        slot: Vtable,
        link_empty: Vtable,
        player: Vtable,
        movement: Vtable,
        controller: Vtable,
        life: Vtable,
    }
    regions: Regions {
        cave: Code,
        cave_end: End,
        far: Code,
        far_end: Code,
        table: Header(Rule::RodataEnd),
        table_end: Header(Rule::DataStart),
        bss_end: Header(Rule::BssEnd),
        panic: Header(Rule::Panic),
        list_end: Header(Rule::ListEnd),
        text_end: Header(Rule::TextEnd),
    }
}

/// Where the row named `name` is in [`ROWS`].
pub fn position(name: &str) -> Option<usize> {
    ROWS.iter().position(|r| r.name() == name)
}

/// `v` in hex as the source writes offsets: groups of four digits.
pub fn hex(v: u32) -> String {
    let digits = format!("{v:x}");
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(4) {
            out.push('_');
        }
        out.push(c);
    }
    format!("0x{out}")
}

/// The source of a generated build module: `name`, a doc line `about`,
/// and a header naming `command`.
pub fn source(b: &Build, name: &str, about: &str, command: &str) -> String {
    let mut out = format!(
        "//! Generated by `{command}`: don't edit, run it again.\n\n\
         use super::*;\n\n/// {about}\npub const {name}: Build = Build {{\n"
    );
    let values = values(b);
    let mut group = "";
    for (row, v) in ROWS.iter().zip(values) {
        if row.group != group {
            if !group.is_empty() {
                out += "    },\n";
            }
            group = row.group;
            out += &format!("    {group}: {} {{\n", type_name(group));
        }
        out += &format!("        {}: {},\n", row.field, hex(v));
    }
    out + "    },\n};\n"
}

#[cfg(test)]
mod tests {
    use zootr_hook::V131072;

    use super::*;

    #[test]
    fn the_rows_are_every_field_once() {
        assert_eq!(ROWS.len() * 4, size_of::<Build>());
        let values = values(&V131072);
        assert_eq!(assemble(&values), V131072);
        let mut names: Vec<_> = ROWS.iter().map(Row::name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ROWS.len());
        for row in ROWS {
            if let Kind::Target(of) = row.kind {
                assert!(position(of).is_some(), "{of}");
            }
        }
    }

    #[test]
    fn offsets_print_as_the_source_writes_them() {
        assert_eq!(hex(0x108_3e20), "0x108_3e20");
        assert_eq!(hex(0x4e9_a4d0), "0x4e9_a4d0");
        assert_eq!(hex(0x3e20), "0x3e20");
        assert_eq!(hex(0), "0x0");
    }

    #[test]
    fn the_source_holds_every_value() {
        let text = source(&V131072, "V1", "Doc.", "zootr probe port");
        assert!(text.starts_with("//! Generated by `zootr probe port`"));
        assert!(text.contains("    entries: Entries {\n        map_loaded: 0x108_3e20,\n"));
        assert!(text.ends_with("        text_end: 0x3c4_61d0,\n    },\n};\n"));
    }
}
