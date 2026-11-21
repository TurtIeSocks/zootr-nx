//! Settings saved by older versions, read as the current ones.

use serde_json::Value;

use super::{
    BOMBCHUS, BRIDGE, Bombchus, Bridge, DOOR_OF_TIME, DoorOfTime, GERUDO_FORTRESS, GerudoFortress,
    SHUFFLE_ITEMS, Settings, TRIALS, Trials, choice_id,
};
use crate::starting::{self, Start};

/// The toggle that left every item in place, before it became
/// [`SHUFFLE_ITEMS`] with the opposite sense.
const VANILLA_CHESTS: &str = "vanilla-chests";

/// The toggle that gave bombchus their own cap, before [`BOMBCHUS`] chose
/// between that, the bomb bag's and Bombchu Bags.
const FREE_BOMBCHUS: &str = "free-bombchus";

/// The toggle that raised the bridge at six medallions, before [`BRIDGE`]
/// chose how the bridge rises.
const BRIDGE_MEDALLIONS: &str = "bridge-medallions";

/// The preset that put the bridge up, saved as `true` under the id
/// [`BRIDGE`] took over.
const BRIDGE_UP: &str = BRIDGE;

/// The preset that brought every barrier down, before [`TRIALS`].
const TRIALS_DONE: &str = "trials-done";

/// The preset that opened the Door of Time and gave the stones, the
/// Ocarina of Time and the Song of Time, saved as `true` under the id
/// [`DOOR_OF_TIME`] took over. Its gifts are dropped.
const DOOR_UP: &str = DOOR_OF_TIME;

/// The toggle that opened the Door of Time, on by default, before
/// [`DOOR_OF_TIME`] chose Open or Closed.
const OPEN_DOOR: &str = "open-door-of-time";

/// The presets that freed the four carpenters, and that counted the first
/// as all four, before [`GERUDO_FORTRESS`].
const CARPENTERS_FREED: &str = "carpenters-freed";
const CARPENTERS_FAST: &str = "carpenters-fast";

/// Presets the CLI still takes by their old names, and toggles by their
/// old ids: see [`renamed`].
pub(crate) const OLD_PRESETS: [&str; 5] = [
    BRIDGE_UP,
    TRIALS_DONE,
    DOOR_UP,
    CARPENTERS_FREED,
    CARPENTERS_FAST,
];
pub(crate) const OLD_TOGGLES: [&str; 2] = [BRIDGE_MEDALLIONS, OPEN_DOOR];

/// Each old preset and toggle, in the order the upgrade reads them: where
/// two set one choice, the first wins.
const OLD: [&str; 7] = [
    BRIDGE_UP,
    BRIDGE_MEDALLIONS,
    TRIALS_DONE,
    DOOR_UP,
    OPEN_DOOR,
    CARPENTERS_FREED,
    CARPENTERS_FAST,
];

/// The choice old preset or toggle `old` became, as setting id and option,
/// when it was `on`. Most leave nothing when off.
fn became(old: &str, on: bool) -> Option<(&'static str, String)> {
    let door = |open| match open {
        true => DoorOfTime::Open,
        false => DoorOfTime::Closed,
    };
    match (old, on) {
        (BRIDGE_UP, true) => Some((BRIDGE, choice_id(Bridge::Open))),
        (BRIDGE_MEDALLIONS, true) => Some((BRIDGE, choice_id(Bridge::SixMedallions))),
        (TRIALS_DONE, true) => Some((TRIALS, choice_id(Trials::Done))),
        (DOOR_UP, true) => Some((DOOR_OF_TIME, choice_id(DoorOfTime::Open))),
        (OPEN_DOOR, on) => Some((DOOR_OF_TIME, choice_id(door(on)))),
        (CARPENTERS_FREED, true) => Some((GERUDO_FORTRESS, choice_id(GerudoFortress::Freed))),
        (CARPENTERS_FAST, true) => Some((GERUDO_FORTRESS, choice_id(GerudoFortress::Fast))),
        _ => None,
    }
}

/// What old preset or toggle `old` on is now, as the CLI takes it.
pub(crate) fn renamed(old: &str) -> Option<String> {
    renamed_as(old, true)
}

/// What old preset or toggle `old` set to `on` is now, as the CLI takes it.
pub(crate) fn renamed_as(old: &str, on: bool) -> Option<String> {
    became(old, on).map(|(id, value)| format!("--setting {id}={value}"))
}

/// Toggles that started a new game with items, before the starting
/// inventory, and the words each became, as `--start` takes them.
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
    if let Some(free) = settings.remove(FREE_BOMBCHUS) {
        let chosen = match free == true {
            true => Bombchus::Free,
            false => Bombchus::BombBag,
        };
        (settings.entry(BOMBCHUS.into())).or_insert(choice_id(chosen).into());
    }
    // A preset first: it won over the toggle.
    for old in OLD {
        // A choice's value is a string, so only a `true` or `false` is old.
        if !settings.get(old).is_some_and(Value::is_boolean) {
            continue;
        }
        let on = settings.remove(old) == Some(true.into());
        if let Some((id, value)) = became(old, on) {
            settings.entry(id.into()).or_insert(value.into());
        }
    }
    let on: Vec<&[&str]> = (STARTING.iter())
        .filter_map(|(id, words)| (settings.remove(*id)? == true).then_some(*words))
        .collect();
    if !on.is_empty() {
        let mut start = Start::from_settings(settings).unwrap_or_default();
        for arg in on.into_iter().flatten() {
            let (word, n) = starting::parse(arg).expect("a starting word");
            let held = start.0.get(&word).copied().unwrap_or(0);
            start.0.insert(word, held.max(n));
        }
        let value = serde_json::to_value(start).expect("a map");
        settings.insert(starting::SETTING.into(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_toggle_and_the_quest_presets_become_the_win_condition() {
        let up = |old: &[(&str, bool)]| {
            let mut s: Settings = (old.iter())
                .map(|&(id, on)| (id.to_owned(), on.into()))
                .collect();
            upgrade(&mut s);
            s
        };
        let is = |new: &[(&str, &str)]| -> Settings {
            (new.iter())
                .map(|&(id, v)| (id.to_owned(), v.into()))
                .collect()
        };
        let six = (BRIDGE, "six-medallions");
        let open = (BRIDGE, "open");
        let done = (TRIALS, "done");
        assert_eq!(up(&[(BRIDGE_MEDALLIONS, true)]), is(&[six]));
        assert_eq!(up(&[(BRIDGE, true)]), is(&[open]));
        // The preset went after the toggle's edits, so it won.
        assert_eq!(
            up(&[(BRIDGE, true), (BRIDGE_MEDALLIONS, true)]),
            is(&[open])
        );
        // The trials leave the bridge as it was: the game's own if unset.
        assert_eq!(up(&[(TRIALS_DONE, true)]), is(&[done]));
        let both = up(&[(TRIALS_DONE, true), (BRIDGE_MEDALLIONS, true)]);
        assert_eq!(both, is(&[six, done]));
        let off = [
            (BRIDGE_MEDALLIONS, false),
            (BRIDGE, false),
            (TRIALS_DONE, false),
        ];
        assert_eq!(up(&off), Settings::new());
        let mut current = is(&[open, done]);
        upgrade(&mut current);
        assert_eq!(current, is(&[open, done]));
        assert_eq!(
            renamed(BRIDGE_MEDALLIONS).unwrap(),
            "--setting rainbow-bridge=six-medallions"
        );
        assert_eq!(renamed(BRIDGE).unwrap(), "--setting rainbow-bridge=open");
        assert_eq!(
            renamed(TRIALS_DONE).unwrap(),
            "--setting ganons-trials=done"
        );
        assert_eq!(renamed("open-forest"), None);
    }

    #[test]
    fn the_door_toggle_and_preset_become_the_door_of_time() {
        let up = |old: &[(&str, bool)]| {
            let mut s: Settings = (old.iter())
                .map(|&(id, on)| (id.to_owned(), on.into()))
                .collect();
            upgrade(&mut s);
            s
        };
        let door = |v: &str| Settings::from([(DOOR_OF_TIME.into(), v.into())]);
        assert_eq!(up(&[(OPEN_DOOR, true)]), door("open"));
        assert_eq!(up(&[(OPEN_DOOR, false)]), door("closed"));
        assert_eq!(up(&[(DOOR_UP, true)]), door("open"));
        // The preset opened the door whatever the toggle said.
        assert_eq!(up(&[(DOOR_UP, true), (OPEN_DOOR, false)]), door("open"));
        assert_eq!(up(&[(DOOR_UP, false), (OPEN_DOOR, false)]), door("closed"));
        assert_eq!(up(&[(DOOR_UP, false)]), Settings::new());
        let mut current = door("closed");
        upgrade(&mut current);
        assert_eq!(current, door("closed"));
        assert_eq!(
            renamed_as(OPEN_DOOR, false).unwrap(),
            "--setting door-of-time=closed"
        );
        assert_eq!(renamed(DOOR_UP).unwrap(), "--setting door-of-time=open");
    }

    #[test]
    fn the_carpenter_presets_become_the_gerudo_fortress() {
        let up = |old: &[(&str, bool)]| {
            let mut s: Settings = (old.iter())
                .map(|&(id, on)| (id.to_owned(), on.into()))
                .collect();
            upgrade(&mut s);
            s
        };
        let fortress = |v: &str| Settings::from([(GERUDO_FORTRESS.into(), v.into())]);
        assert_eq!(up(&[(CARPENTERS_FREED, true)]), fortress("freed"));
        assert_eq!(up(&[(CARPENTERS_FAST, true)]), fortress("fast"));
        // Freed wins over Fast.
        let both = up(&[(CARPENTERS_FAST, true), (CARPENTERS_FREED, true)]);
        assert_eq!(both, fortress("freed"));
        let off = up(&[(CARPENTERS_FAST, false), (CARPENTERS_FREED, false)]);
        assert_eq!(off, Settings::new());
        assert_eq!(
            renamed(CARPENTERS_FREED).unwrap(),
            "--setting gerudo-fortress=freed"
        );
    }

    #[test]
    fn free_bombchus_becomes_the_bombchus_choice() {
        let up = |free: bool| {
            let mut s = Settings::from([(FREE_BOMBCHUS.into(), free.into())]);
            upgrade(&mut s);
            s
        };
        let chosen = |v: &str| Settings::from([(BOMBCHUS.into(), v.into())]);
        assert_eq!(up(true), chosen("free"));
        assert_eq!(up(false), chosen("bomb-bag"));
    }

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
            ("start-bomb-bag".into(), true.into()),
            ("warp-kit".into(), true.into()),
            ("magic".into(), true.into()),
            (starting::SETTING.into(), serde_json::json!({ "bombs": 20 })),
        ]);
        upgrade(&mut s);
        let start = Start::from_settings(&s).unwrap();
        let words: Vec<_> = start.0.keys().map(String::as_str).collect();
        assert_eq!(words.len(), 11, "{words:?}");
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
