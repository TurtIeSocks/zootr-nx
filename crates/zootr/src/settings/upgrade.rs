//! Settings saved by older versions, read as the current ones.

use super::{SHUFFLE_ITEMS, Settings};
use crate::starting::{self, Start};

/// The toggle that left every item in place, before it became
/// [`SHUFFLE_ITEMS`] with the opposite sense.
const VANILLA_CHESTS: &str = "vanilla-chests";

/// Toggles that started a new game with items, before the starting
/// inventory, and the words each became.
const STARTING: [(&str, &[&str]); 9] = [
    ("start-kokiri-sword", &["kokiri-sword"]),
    ("start-deku-shield", &["deku-shield"]),
    ("start-slingshot", &["slingshot"]),
    ("start-bomb-bag", &["bomb-bag"]),
    ("start-fairy-ocarina", &["fairy-ocarina"]),
    (
        "warp-kit",
        &[
            "fairy-ocarina",
            "minuet-of-forest",
            "bolero-of-fire",
            "serenade-of-water",
            "requiem-of-spirit",
            "nocturne-of-shadow",
            "prelude-of-light",
        ],
    ),
    ("nut-bag", &["nut-bag"]),
    ("stick-bag", &["stick-bag"]),
    ("magic", &["magic-meter"]),
];

/// Brings saved `settings` (the config, a history entry, a share code) up
/// to date, so an old seed builds as it did.
pub(crate) fn upgrade(settings: &mut Settings) {
    if let Some(vanilla) = settings.remove(VANILLA_CHESTS) {
        let shuffled = vanilla != true;
        settings
            .entry(SHUFFLE_ITEMS.into())
            .or_insert(shuffled.into());
    }
    let on: Vec<&[&str]> = (STARTING.iter())
        .filter_map(|(id, words)| (settings.remove(*id)? == true).then_some(*words))
        .collect();
    if !on.is_empty() {
        let mut start = Start::from_settings(settings).unwrap_or_default();
        for word in on.into_iter().flatten() {
            let held = start.0.get(*word).copied().unwrap_or(0);
            start.0.insert((*word).into(), held.max(1));
        }
        let value = serde_json::to_value(start).expect("a map");
        settings.insert(starting::SETTING.into(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_chests_become_shuffle_items_the_other_way_round() {
        let up = |vanilla: bool| {
            let mut s = Settings::from([(VANILLA_CHESTS.into(), vanilla.into())]);
            upgrade(&mut s);
            s
        };
        assert_eq!(
            up(true),
            Settings::from([(SHUFFLE_ITEMS.into(), false.into())])
        );
        assert_eq!(
            up(false),
            Settings::from([(SHUFFLE_ITEMS.into(), true.into())])
        );
        let mut current = Settings::from([(SHUFFLE_ITEMS.into(), true.into())]);
        upgrade(&mut current);
        assert_eq!(
            current,
            Settings::from([(SHUFFLE_ITEMS.into(), true.into())])
        );
    }

    #[test]
    fn starting_toggles_become_the_starting_inventory() {
        let mut s = Settings::from([
            ("start-kokiri-sword".into(), true.into()),
            ("start-slingshot".into(), false.into()),
            ("warp-kit".into(), true.into()),
            ("magic".into(), true.into()),
            (starting::SETTING.into(), serde_json::json!({ "bombs": 20 })),
        ]);
        upgrade(&mut s);
        let start = Start::from_settings(&s).unwrap();
        let words: Vec<_> = start.0.keys().map(String::as_str).collect();
        assert_eq!(words.len(), 10, "{words:?}");
        for w in [
            "kokiri-sword",
            "fairy-ocarina",
            "prelude-of-light",
            "magic-meter",
        ] {
            assert_eq!(start.0[w], 1, "{w}");
        }
        assert_eq!(start.0["bombs"], 20);
        assert!(!words.contains(&"slingshot"));
        assert_eq!(s.len(), 1, "{s:?}");
    }
}
