//! A seed's icons: five pictures standing in for its number. They cover the
//! seed, settings and extras together, so the same icons mean the same mod.
//! Colours aside: the same icons mean the same shuffle.

use ounce_formats::murmur3;

use crate::generate::Extras;
use crate::settings::Settings;

/// Exactly 32, five bits each. Reordering or renaming changes every seed's
/// icons, and the app draws each by this name.
pub const ICONS: [&str; 32] = [
    "Sword",
    "Shield",
    "Heart",
    "Bomb",
    "Gem",
    "Key",
    "Crown",
    "Flame",
    "Droplet",
    "Leaf",
    "Feather",
    "Music",
    "Skull",
    "Ghost",
    "Moon",
    "Sun",
    "Star",
    "Hourglass",
    "Compass",
    "Map",
    "Fish",
    "Bird",
    "Egg",
    "Mountain",
    "Anchor",
    "Axe",
    "Hammer",
    "Wand",
    "Snowflake",
    "Zap",
    "Shell",
    "Clover",
];
const COUNT: usize = 5;

pub fn icons(seed: u64, settings: &Settings, extras: &Extras) -> Vec<String> {
    // Both maps serialize in key order, so the bytes are stable. Colours
    // change no placement, so they leave the icons be.
    let extras = extras.cosmetic_free();
    let bytes = serde_json::to_vec(&(seed, settings, extras)).expect("plain data serializes");
    let hash = murmur3(&bytes);
    (0..COUNT)
        .map(|i| ICONS[(hash >> (i * 5)) as usize % ICONS.len()].to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_inputs_same_icons_and_settings_count() {
        let none = Settings::new();
        let a = icons(7, &none, &Extras::default());
        assert_eq!(a.len(), COUNT);
        assert_eq!(a, icons(7, &none, &Extras::default()));
        assert_ne!(a, icons(8, &none, &Extras::default()));
        let forest = Settings::from([("open-forest".into(), true.into())]);
        assert_ne!(a, icons(7, &forest, &Extras::default()));
    }
}
