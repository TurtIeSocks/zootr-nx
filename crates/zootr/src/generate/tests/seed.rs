use std::collections::{HashMap, HashSet};

use crate::{
    events::Flows,
    generate::{
        Extras, Spot, generate, shuffle,
        tests::{Scratch, tree},
        write::flows_alone,
    },
    gifts, icons,
    known::{actor, flag, flow, item, scene},
    progress::Run,
    settings::{NewGame, Settings},
    shuffle::Fixed,
    starting,
};

#[test]
fn same_seed_and_settings_give_the_same_mod() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("gen");
    let settings = crate::settings::favorites::randomizer_and(Settings::from([(
        "open-forest".into(),
        true.into(),
    )]));
    let extras = Extras::default();
    let [a, b] = ["a", "b"]
        .map(|to| generate(&romfs, &dir.join(to), 7, &settings, &extras, &Run::silent()).unwrap());
    assert_eq!(a, b);
    let (a_files, b_files) = (tree(&dir.join("a")), tree(&dir.join("b")));
    assert_eq!(
        a_files.keys().collect::<Vec<_>>(),
        b_files.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in &a_files {
        assert!(b_files[path] == *bytes, "{} differs", path.display());
    }
    assert!(a.changed() > 0 && !a.written.defaults.is_empty());
    let spoiler = std::fs::read_to_string(dir.join("a/spoiler.txt")).unwrap();
    assert_eq!(spoiler, a.spoiler(&icons::icons(7, &settings, &extras)));
}

#[test]
fn every_seed_can_call_navi_without_the_deku_tree() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("navi");
    let report = generate(
        &romfs,
        &dir,
        1,
        &Settings::new(),
        &Extras::default(),
        &Run::silent(),
    )
    .unwrap();
    assert!(report.written.defaults.contains(&format!(
        "{}.Navi: false -> true",
        flag::ACTION_GUIDE_IS_END
    )));
}

#[test]
fn a_seed_with_only_quest_steps_still_counts_as_a_test() {
    assert!(Extras::default().is_empty());
    let extras = Extras {
        progress: vec![format!("{}=Step02", flow::SAGE_OF_SPIRIT)],
        ..Default::default()
    };
    assert!(!extras.is_empty());
    let json = serde_json::to_string(&extras).unwrap();
    assert_eq!(serde_json::from_str::<Extras>(&json).unwrap(), extras);
    assert_eq!(serde_json::to_string(&Extras::default()).unwrap(), "{}");
}

#[test]
fn every_seed_gets_the_dev_branches() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let all = Flows::load(&romfs).unwrap();
    let banc = crate::flows::rel(scene::SY_BANC_CHANGE_GO);
    let (rewrites, _) = flows_alone(&romfs, &all, &NewGame::default(), &HashMap::new()).unwrap();
    let give = zootr_hook::ARG_GIVE;
    assert!(
        rewrites[&banc].windows(give.len()).any(|w| w == give),
        "every seed checks the card there"
    );
}

#[test]
fn a_bad_quest_step_stops_the_seed_before_anything_is_written() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("steps-bad");
    let extras = Extras {
        progress: vec![format!("{}=Step99", flow::SAGE_OF_SPIRIT)],
        ..Default::default()
    };
    let err = generate(&romfs, &dir, 7, &Settings::new(), &extras, &Run::silent()).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("{}=Step99", flow::SAGE_OF_SPIRIT)),
        "{err:#}"
    );
    assert!(!dir.exists());
}

#[test]
fn a_logic_seed_keeps_each_spots_round_hero_and_rule() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("logic");
    // The game's own bridge and Bombchus' default cap (for the Spirit
    // trial), which a hand-built new game holds no word for.
    let bridge = crate::settings::value_word(crate::settings::BRIDGE, "vanilla");
    let chus = crate::settings::value_word(crate::settings::BOMBCHUS, "free");
    let new_game = NewGame {
        on: [bridge, chus].into(),
        ..Default::default()
    };
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let rounds: HashSet<u16> = (report.spots.iter().filter_map(|s| s.round))
        .chain(report.events.iter().map(|e| e.round))
        .collect();
    let last = rounds.iter().copied().max().unwrap();
    assert_eq!(
        rounds,
        (1..=last).collect(),
        "rounds count from 1 without gaps"
    );
    let heroes: Vec<&Spot> = report.spots.iter().filter(|s| s.hero).collect();
    assert!(!heroes.is_empty() && heroes.iter().all(|s| s.round.is_some()));
    assert!(report.spots.iter().any(|s| !s.need_items.is_empty()));
    for s in report.spots.iter().filter(|s| s.round.is_some()) {
        assert!(!s.need.is_empty(), "{} has no rule", s.name());
        assert!(s.need_items.iter().all(|i| s.need.contains(i.as_str())));
    }
    let text = report.spoiler(&[]);
    assert!(text.contains("\nWay of the Hero\n") && text.contains("\nPlaythrough\n  Round 1\n"));
}

#[test]
fn a_logic_seed_lists_what_link_starts_with() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("start");
    let sword = item::SWORD_KOKIRI.get();
    let new_game = NewGame {
        start: starting::Start::of(&["kokiri-sword"]),
        on: ["open-forest", "no-pickup-texts"].map(String::from).into(),
        defaults: crate::presets::expand(&["no-pickup-texts".into()], &[]).unwrap(),
        ..NewGame::default()
    };
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let held = |item: &str| report.start.items.iter().find(|h| h.item == item);
    let sword_held = held(sword).expect("the starting item is listed");
    assert!(
        sword_held.count == 1 && !sword_held.name.starts_with("Item_"),
        "{sword_held:?}"
    );
    // It names the check whose slot it left: the sword's own, here.
    let [from] = sword_held.from[..] else {
        panic!("{sword_held:?}")
    };
    let spot = report.spots.iter().find(|s| s.hash == from).unwrap();
    assert_eq!((spot.vanilla.as_str(), &sword_held.via), (sword, &None));
    let heart = held(actor::OBJ_HEART.get()).expect("a preset's pickup is listed");
    assert!(heart.from.is_empty(), "{heart:?}");
    assert_eq!(heart.via.as_deref(), Some("No Pickup Texts"));
    assert!(report.start.settings.iter().any(|s| s.contains("orest")));
}

/// Every bag in the pool, with the carpet merchant's sale a check holding
/// his own Bombchus, and not a check, where his shop sells them: each whole
/// seed writes.
#[test]
fn a_seed_with_every_starter_bag_writes() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("bags");
    let carpet = (gifts::GIFTS.iter())
        .find(|g| g.flow == actor::NPC_HYLIA_044_ADULT)
        .unwrap();
    let new_game = NewGame {
        bombchus: crate::settings::Bombchus::Bag,
        shuffle_nut_bag: true,
        shuffle_stick_bag: true,
        on: carpet.setting.into_iter().map(String::from).collect(),
        ..Default::default()
    };
    let own = Fixed::from([(carpet.hash(), carpet.item.to_string())]);
    let unsold = NewGame {
        on: Default::default(),
        ..new_game.clone()
    };
    shuffle(&romfs, &dir, 7, &Fixed::new(), &unsold, &Run::silent()).unwrap();
    let report = shuffle(&romfs, &dir, 7, &own, &new_game, &Run::silent()).unwrap();
    let dealt = |item: &str| report.spots.iter().any(|s| s.item == item);
    for bag in crate::bombchu_bags::BAGS {
        assert!(dealt(bag), "{bag}");
    }
    assert!(dealt(item::NUT_BAG_SMALL.get()) && dealt(item::STICK_BAG_SMALL.get()));
}

#[test]
fn a_met_zelda_seed_marks_her_three_checks_given_at_start() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::{MET_ZELDA, NPC_GIFTS, SONGS, Settings};
    let dir = Scratch::new("met-zelda");
    let settings = Settings::from([MET_ZELDA, NPC_GIFTS, SONGS].map(|id| (id.into(), true.into())));
    let new_game = NewGame::from_settings(&settings, &[]).unwrap();
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let given: Vec<&Spot> = report.spots.iter().filter(|s| s.given).collect();
    assert_eq!(given.len(), 3, "{given:?}");
    let letter = item::ZELDA_LETTER.get();
    assert_eq!(given.iter().filter(|s| s.item == letter).count(), 1);
    assert_eq!(report.spots.iter().filter(|s| s.item == letter).count(), 1);
    assert!(report.spots.iter().all(|s| s.item != item::WEIRD_EGG.get()));
    assert!(report.spoiler(&[]).contains(" (given at start)\n"));
}
