//! Seeds' edits to GameData.

use super::*;
use crate::settings::{BREAKABLES_RESPAWN, POTS, favorites};

/// How many placement keys a built seed resets each way, by reset type.
fn resets(romfs: &ounce_formats::Romfs, dir: &Path) -> HashMap<i64, usize> {
    let data = crate::game::read_built(romfs, dir, gamedata::PATH);
    let doc = BymlDoc::parse(&data).unwrap();
    let keys = doc.root["Data"][ounce_formats::KEYED].as_array().unwrap();
    let mut count = HashMap::new();
    for key in keys {
        let reset = key.as_map().unwrap()[&**crate::known::field::RESET_TYPE_VALUE].as_int::<i64>();
        *count.entry(reset.unwrap()).or_default() += 1;
    }
    count
}

#[test]
fn breakables_and_their_twins_respawn_on_load_only_with_the_setting() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("breakables-respawn");
    let build = |respawn: bool, to: &str| {
        let settings = favorites::randomizer_and(Settings::from([
            (POTS.into(), true.into()),
            (BREAKABLES_RESPAWN.into(), respawn.into()),
        ]));
        let new_game = NewGame::from_settings(&settings, &[]).unwrap();
        shuffle(
            &romfs,
            &dir.join(to),
            7,
            &Fixed::new(),
            &new_game,
            &Run::silent(),
        )
        .unwrap();
        resets(&romfs, &dir.join(to))
    };
    let off = build(false, "off");
    let on = build(true, "on");
    // The game's 2444 breakables, then the shuffled pots' twins.
    assert!(off[&88] > 2444, "{off:?}");
    assert!(!off.contains_key(&89));
    assert_eq!(on.get(&89), off.get(&88));
    assert!(!on.contains_key(&88));
    assert_eq!(on[&24], off[&24]);
}
