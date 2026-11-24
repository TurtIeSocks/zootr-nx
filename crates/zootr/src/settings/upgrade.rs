//! Settings saved by older versions, read as the current ones.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    BOMBCHUS, BRIDGE, Bombchus, Bridge, DOOR_OF_TIME, DoorOfTime, GERUDO_FORTRESS, GerudoFortress,
    MET_ZELDA, SHUFFLE_ITEMS, Settings, TRIALS, Trials, choice_id,
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

/// The preset that started the trade quest at its last step with the
/// Claim Check in hand, for early debugging. Dropped, as the pool holds
/// the Claim Check.
const CLAIM_CHECK: &str = "claim-check";

/// The preset that started with Zelda met, before [`MET_ZELDA`].
const ZELDA_DONE: &str = "zelda-done";

/// The toggles that opened items to both ages by kind, before one toggle
/// per item, and the Both Ages toggles each became.
const SPLIT: [(&str, &[&str]); 3] = [
    (
        "ageless-weapons",
        &[
            "both-ages-kokiri-sword",
            "both-ages-master-sword",
            "both-ages-deku-shield",
            "both-ages-mirror-shield",
        ],
    ),
    (
        "ageless-items",
        &[
            "both-ages-bow",
            "both-ages-slingshot",
            "both-ages-hammer",
            "both-ages-hookshot",
            "both-ages-longshot",
            "both-ages-boomerang",
            "both-ages-deku-sticks",
            "both-ages-giants-knife",
            "both-ages-biggoron-sword",
        ],
    ),
    (
        "ageless-gear",
        &[
            "both-ages-iron-boots",
            "both-ages-hover-boots",
            "both-ages-zora-tunic",
            "both-ages-goron-tunic",
        ],
    ),
];

/// Presets the CLI still takes by their old names, and toggles by their
/// old ids: see [`renamed`] and [`dropped`].
pub(crate) const OLD_PRESETS: [&str; 7] = [
    BRIDGE_UP,
    ZELDA_DONE,
    TRIALS_DONE,
    DOOR_UP,
    CARPENTERS_FREED,
    CARPENTERS_FAST,
    CLAIM_CHECK,
];
pub(crate) const OLD_TOGGLES: [&str; 5] = [
    BRIDGE_MEDALLIONS,
    OPEN_DOOR,
    SPLIT[0].0,
    SPLIT[1].0,
    SPLIT[2].0,
];

/// Each old preset and toggle, in the order the upgrade reads them: where
/// two set one choice, the first wins.
const OLD: [&str; 9] = [
    BRIDGE_UP,
    ZELDA_DONE,
    BRIDGE_MEDALLIONS,
    TRIALS_DONE,
    DOOR_UP,
    OPEN_DOOR,
    CARPENTERS_FREED,
    CARPENTERS_FAST,
    CLAIM_CHECK,
];

/// The setting old preset or toggle `old` became, as setting id and value,
/// when it was `on`. Most leave nothing when off.
fn became(old: &str, on: bool) -> Option<(&'static str, Value)> {
    let door = |open| match open {
        true => DoorOfTime::Open,
        false => DoorOfTime::Closed,
    };
    let chose = |id, option: String| Some((id, Value::from(option)));
    match (old, on) {
        (ZELDA_DONE, true) => Some((MET_ZELDA, true.into())),
        (BRIDGE_UP, true) => chose(BRIDGE, choice_id(Bridge::Open)),
        (BRIDGE_MEDALLIONS, true) => chose(BRIDGE, choice_id(Bridge::SixMedallions)),
        (TRIALS_DONE, true) => chose(TRIALS, choice_id(Trials::Done)),
        (DOOR_UP, true) => chose(DOOR_OF_TIME, choice_id(DoorOfTime::Open)),
        (OPEN_DOOR, on) => chose(DOOR_OF_TIME, choice_id(door(on))),
        (CARPENTERS_FREED, true) => chose(GERUDO_FORTRESS, choice_id(GerudoFortress::Freed)),
        (CARPENTERS_FAST, true) => chose(GERUDO_FORTRESS, choice_id(GerudoFortress::Fast)),
        _ => None,
    }
}

/// Why old preset `old` became nothing, for the one line the CLI prints.
pub(crate) fn dropped(old: &str) -> Option<&'static str> {
    (old == CLAIM_CHECK).then_some("the Claim Check comes from the pool, so nothing replaces it")
}

/// What old preset or toggle `old` on is now, as the CLI takes it.
pub(crate) fn renamed(old: &str) -> Option<String> {
    renamed_as(old, true)
}

/// What old preset or toggle `old` set to `on` is now, as the CLI takes it.
pub(crate) fn renamed_as(old: &str, on: bool) -> Option<String> {
    if let Some((_, new)) = SPLIT.iter().find(|(id, _)| *id == old) {
        let each = new.iter().map(|id| format!("--setting {id}={on}"));
        return Some(each.collect::<Vec<_>>().join(" "));
    }
    became(old, on).map(|(id, value)| match value.as_str() {
        Some(option) => format!("--setting {id}={option}"),
        None => format!("--setting {id}={value}"),
    })
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

/// Which defaults a file's settings were saved under, kept in the file
/// beside them. A file without one is from before the defaults went
/// vanilla.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SettingsVersion(u32);

impl SettingsVersion {
    /// The randomizer's defaults: the Randomizer favorite's values.
    pub const RANDOMIZER: Self = Self(1);
    /// Every setting at the game's own behavior.
    pub const VANILLA: Self = Self(2);
    pub const CURRENT: Self = Self::VANILLA;

    /// What a file that names no version was saved under.
    pub fn unwritten() -> Self {
        Self::RANDOMIZER
    }
}

/// A new file's settings are current.
impl Default for SettingsVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

/// Brings saved `settings` (the config, a history entry, a share code),
/// saved under `from`'s defaults, up to date, so an old seed builds as it
/// did.
pub(crate) fn upgrade(settings: &mut Settings, from: SettingsVersion) {
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
            settings.entry(id.into()).or_insert(value);
        }
    }
    for (old, new) in SPLIT {
        if settings.remove(old) == Some(true.into()) {
            for id in new {
                settings.entry((*id).into()).or_insert(true.into());
            }
        }
    }
    if let Some(start) = settings.get_mut(starting::SETTING) {
        starting::upgrade(start);
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
    // A setting left at the randomizer's default keeps it.
    if from < SettingsVersion::VANILLA {
        for (id, value) in super::favorites::randomizer() {
            settings.entry(id).or_insert(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `old`'s toggles, upgraded.
    fn up(old: &[(&str, bool)]) -> Settings {
        let mut s: Settings = (old.iter())
            .map(|&(id, on)| (id.to_owned(), on.into()))
            .collect();
        upgrade(&mut s, SettingsVersion::CURRENT);
        s
    }

    /// Each old kind toggle turns on its items, and between them every
    /// Both Ages toggle once.
    #[test]
    fn the_kind_toggles_become_one_toggle_per_item() {
        let mut every: Vec<&str> = SPLIT
            .iter()
            .flat_map(|(_, new)| new.iter().copied())
            .collect();
        every.sort_unstable();
        let mut toggles = crate::ageless::toggles::TOGGLES.map(|t| t.id);
        toggles.sort_unstable();
        assert_eq!(every, toggles);
        for (old, new) in SPLIT {
            let on: Settings = new
                .iter()
                .map(|id| ((*id).to_owned(), true.into()))
                .collect();
            assert_eq!(up(&[(old, true)]), on, "{old}");
            assert_eq!(up(&[(old, false)]), Settings::new(), "{old} off");
        }
        // A toggle saved off since stays off.
        let kept = up(&[(SPLIT[2].0, true), ("both-ages-goron-tunic", false)]);
        assert_eq!(kept["both-ages-goron-tunic"], false);
        assert_eq!(kept["both-ages-zora-tunic"], true);
    }

    /// Every word an old starting toggle became is one `--start` takes.
    #[test]
    fn every_starting_word_parses() {
        for word in STARTING.iter().flat_map(|(_, words)| *words) {
            assert!(starting::parse(word).is_ok(), "{word}");
        }
    }

    #[test]
    fn the_bridge_toggle_and_the_quest_presets_become_the_win_condition() {
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
        upgrade(&mut current, SettingsVersion::CURRENT);
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
        let door = |v: &str| Settings::from([(DOOR_OF_TIME.into(), v.into())]);
        assert_eq!(up(&[(OPEN_DOOR, true)]), door("open"));
        assert_eq!(up(&[(OPEN_DOOR, false)]), door("closed"));
        assert_eq!(up(&[(DOOR_UP, true)]), door("open"));
        // The preset opened the door whatever the toggle said.
        assert_eq!(up(&[(DOOR_UP, true), (OPEN_DOOR, false)]), door("open"));
        assert_eq!(up(&[(DOOR_UP, false), (OPEN_DOOR, false)]), door("closed"));
        assert_eq!(up(&[(DOOR_UP, false)]), Settings::new());
        let mut current = door("closed");
        upgrade(&mut current, SettingsVersion::CURRENT);
        assert_eq!(current, door("closed"));
        assert_eq!(
            renamed_as(OPEN_DOOR, false).unwrap(),
            "--setting door-of-time=closed"
        );
        assert_eq!(renamed(DOOR_UP).unwrap(), "--setting door-of-time=open");
    }

    #[test]
    fn the_carpenter_presets_become_the_gerudo_fortress() {
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
    fn the_zelda_preset_becomes_met_zelda() {
        let mut s = Settings::from([(ZELDA_DONE.into(), true.into())]);
        upgrade(&mut s, SettingsVersion::CURRENT);
        assert_eq!(s, Settings::from([(MET_ZELDA.into(), true.into())]));
        let mut off = Settings::from([(ZELDA_DONE.into(), false.into())]);
        upgrade(&mut off, SettingsVersion::CURRENT);
        assert_eq!(off, Settings::new());
        assert_eq!(renamed(ZELDA_DONE).unwrap(), "--setting met-zelda=true");
    }

    #[test]
    fn the_claim_check_preset_goes_with_a_note() {
        for on in [true, false] {
            let mut s = Settings::from([(CLAIM_CHECK.into(), on.into())]);
            upgrade(&mut s, SettingsVersion::CURRENT);
            assert_eq!(s, Settings::new());
        }
        assert_eq!(renamed(CLAIM_CHECK), None);
        assert!(dropped(CLAIM_CHECK).unwrap().contains("pool"));
        assert_eq!(dropped(BRIDGE_UP), None);
    }

    #[test]
    fn free_bombchus_becomes_the_bombchus_choice() {
        let up = |free: bool| {
            let mut s = Settings::from([(FREE_BOMBCHUS.into(), free.into())]);
            upgrade(&mut s, SettingsVersion::CURRENT);
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
            upgrade(&mut s, SettingsVersion::CURRENT);
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
        upgrade(&mut current, SettingsVersion::CURRENT);
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
        upgrade(&mut s, SettingsVersion::CURRENT);
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
