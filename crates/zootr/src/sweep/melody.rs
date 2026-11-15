//! Each song's notes on the Switch buttons, for a song lesson's "Play the
//! ..." prompt, which only the song itself passes.

use crate::known::{Name, item};

/// The ocarina's notes are L, R, A, Y and X (the N64's A, C-down, C-right,
/// C-left and C-up), named by the song item.
const SONGS: [(&Name, &str); 12] = [
    (item::MELODY_ZELDA_LULLABY, "Y X A Y X A"),
    (item::MELODY_EPONA_SONG, "X Y A X Y A"),
    (item::MELODY_SARIA_SONG, "R A Y R A Y"),
    (item::MELODY_SUN_SONG, "A R X A R X"),
    (item::MELODY_TIME_SONG, "A L R A L R"),
    (item::MELODY_STORM_SONG, "L R X L R X"),
    (item::MELODY_FOREST_MINUET, "L X Y A Y A"),
    (item::MELODY_FIRE_BOLERO, "R L R L A R A R"),
    (item::MELODY_WATER_SERENADE, "L R A A Y"),
    (item::MELODY_SPIRIT_REQUIEM, "L R L A R L"),
    (item::MELODY_SHADOW_NOCTURNE, "Y A A L Y A R"),
    (item::MELODY_LIGHT_PRELUDE, "X A X A Y X"),
];

/// The notes of the song `item` teaches, as in `item::MELODY_FIRE_BOLERO`.
pub fn notes(item: &str) -> Option<impl Iterator<Item = &'static str>> {
    let (_, notes) = SONGS.iter().find(|(name, _)| *name == item)?;
    Some(notes.split(' '))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;

    #[test]
    fn a_song_item_names_its_notes() {
        if crate::game::no_names() {
            return;
        }
        let nocturne: Vec<_> = notes(item::MELODY_SHADOW_NOCTURNE).unwrap().collect();
        assert_eq!(nocturne, ["Y", "A", "A", "L", "Y", "A", "R"]);
        assert!(notes("Item_A").is_none());
        for song in crate::logic::WARP_SONGS {
            assert!(
                notes(&format!("{}{song}", item::MELODY_FIRE_BOLERO.stem())).is_some(),
                "{song}"
            );
        }
    }
}
