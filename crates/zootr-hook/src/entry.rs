//! Where each hook entry sits in the hook's branch table, as an offset
//! from its start (`CAVE`): one `b` a word, in the order `game.rs` lays
//! them out under `entries:`. The patch points each call site and hooked
//! function at one of these. A test holds them to that layout.

/// The event request's string argument calls, as an opening chest sets
/// its `ContentName`.
pub const CONTENT_NAME: u32 = 0;
/// The drop reader's calls, as a loading room reads a chest's placement.
pub const DROP_ACTOR: u32 = 4;
pub const MAP_LOADED: u32 = 8;
pub const CHEST_MADE: u32 = 12;
pub const GRANTED: u32 = 16;
pub const LINK_UPDATE: u32 = 20;
/// The run node's call to the speed setter.
pub const RUN_SPEED: u32 = 24;
pub const PICKED: u32 = 28;
/// The controller reads: full key, handheld, joy dual.
pub const NPAD_FULL: u32 = 32;
pub const NPAD_HANDHELD: u32 = 36;
pub const NPAD_DUAL: u32 = 40;
pub const ITEMS_BUILT: u32 = 44;
pub const ITEMS_LEFT: u32 = 48;
pub const UI_UPDATE: u32 = 52;
pub const TAKEN: u32 = 56;
pub const NOCKED: u32 = 60;
pub const EMPTIED: u32 = 64;
/// The bow draw's child test.
pub const SEED_GUIDE: u32 = 68;
pub const EQUIPMENT_BUILT: u32 = 72;
pub const EQUIPMENT_LEFT: u32 = 76;
pub const RESIDENT_MADE: u32 = 80;

#[cfg(test)]
mod tests {
    use super::*;

    /// Each entry's branch target in `game.rs`, and its offset here.
    const ENTRIES: [(&str, u32); 21] = [
        ("content_name_entry", CONTENT_NAME),
        ("drop_actor", DROP_ACTOR),
        ("map_loaded_entry", MAP_LOADED),
        ("chest_made_entry", CHEST_MADE),
        ("granted_entry", GRANTED),
        ("link_update_entry", LINK_UPDATE),
        ("run_speed", RUN_SPEED),
        ("picked_entry", PICKED),
        ("npad_full_entry", NPAD_FULL),
        ("npad_handheld_entry", NPAD_HANDHELD),
        ("npad_dual_entry", NPAD_DUAL),
        ("items_built_entry", ITEMS_BUILT),
        ("items_left_entry", ITEMS_LEFT),
        ("ui_update_entry", UI_UPDATE),
        ("taken_entry", TAKEN),
        ("nocked_entry", NOCKED),
        ("emptied_entry", EMPTIED),
        ("seed_guide", SEED_GUIDE),
        ("equipment_built_entry", EQUIPMENT_BUILT),
        ("equipment_left_entry", EQUIPMENT_LEFT),
        ("resident_made_entry", RESIDENT_MADE),
    ];

    #[test]
    fn each_entry_is_where_the_branch_table_puts_it() {
        let asm = include_str!("game.rs");
        let table = asm.split("\"entries:\",").nth(1).unwrap();
        let table = table.split("\"entries_end:\",").next().unwrap();
        let targets: Vec<&str> = table
            .lines()
            .map(|l| l.trim().trim_end_matches(','))
            .filter(|l| !l.is_empty())
            .map(|l| l.strip_prefix("\"b ").unwrap().strip_suffix('"').unwrap())
            .collect();
        let named: Vec<&str> = ENTRIES.iter().map(|&(name, _)| name).collect();
        assert_eq!(targets, named);
        for (i, &(name, offset)) in ENTRIES.iter().enumerate() {
            assert_eq!(offset as usize, i * 4, "{name}");
        }
    }
}
