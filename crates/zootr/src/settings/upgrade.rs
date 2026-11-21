//! Settings saved by older versions, read as the current ones.

use serde_json::Value;

use super::{
    BOMBCHUS, BRIDGE, Bombchus, Bridge, SHUFFLE_ITEMS, Settings, TRIALS, Trials, choice_id,
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

/// Presets the CLI still takes by their old names, and toggles by their
/// old ids: see [`renamed`].
pub(crate) const OLD_PRESETS: [&str; 2] = [BRIDGE_UP, TRIALS_DONE];
pub(crate) const OLD_TOGGLES: [&str; 1] = [BRIDGE_MEDALLIONS];

/// The choice old preset or toggle `old` became when on, as setting id and
/// option.
fn became(old: &str) -> Option<(&'static str, String)> {
    match old {
        BRIDGE_UP => Some((BRIDGE, choice_id(Bridge::Open))),
        BRIDGE_MEDALLIONS => Some((BRIDGE, choice_id(Bridge::SixMedallions))),
        TRIALS_DONE => Some((TRIALS, choice_id(Trials::Done))),
        _ => None,
    }
}

/// What old preset or toggle `old` on is now, as the CLI takes it.
pub(crate) fn renamed(old: &str) -> Option<String> {
    became(old).map(|(id, value)| format!("--setting {id}={value}"))
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
    // The bridge preset first: it won over the toggle.
    for old in [BRIDGE_UP, BRIDGE_MEDALLIONS, TRIALS_DONE] {
        // A choice's value is a string, so only a `true` or `false` is old.
        if !settings.get(old).is_some_and(Value::is_boolean) {
            continue;
        }
        let on = settings.remove(old) == Some(true.into());
        let (id, value) = became(old).expect("an old name");
        if on {
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
