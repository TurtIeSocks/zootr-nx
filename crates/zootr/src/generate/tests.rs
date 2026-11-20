use ounce_formats::{BymlDoc, Flowchart, Link};

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::write::flows;
use super::*;
use crate::checks::{self, Drop};
use crate::events::Flows;
use crate::flows::Rewrites;
use crate::known::{actor, flag, flow, item, scene};
use crate::progress::{Run, Step, StepEvent};
use crate::settings::{NewGame, Settings};
use crate::shuffle::Fixed;
use crate::standins;
use crate::{
    bridge, cows, gamedata, gerudo, gifts, hints, icons, keys, pickups, scrubs, shops, starting,
};

mod copies;

#[test]
fn same_seed_and_settings_give_the_same_mod() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-gen-{}", std::process::id()));
    let settings = Settings::from([("open-forest".into(), true.into())]);
    let extras = Extras::default();
    let a = generate(
        &romfs,
        &dir.join("a"),
        7,
        &settings,
        &extras,
        &Run::silent(),
    )
    .unwrap();
    let b = generate(
        &romfs,
        &dir.join("a"),
        7,
        &settings,
        &extras,
        &Run::silent(),
    )
    .unwrap();
    assert_eq!(a, b);
    assert!(a.changed() > 0 && !a.written.defaults.is_empty());
    let spoiler = std::fs::read_to_string(dir.join("a/spoiler.txt")).unwrap();
    assert_eq!(spoiler, a.spoiler(&icons::icons(7, &settings, &extras)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_run_reports_every_step_in_order() {
    let path = Path::new(crate::game::DEV_DUMP);
    if !path.exists() {
        return;
    }
    let began = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let heard = began.clone();
    let run = Run::silent().with(move |e| {
        if let StepEvent::Began { step } = e {
            heard.lock().unwrap().push(*step);
        }
    });
    let romfs = crate::game::prepare(path, &run).unwrap();
    let dir = std::env::temp_dir().join(format!("zootr-steps-{}", std::process::id()));
    generate(&romfs, &dir, 7, &Settings::new(), &Extras::default(), &run).unwrap();
    use Step::*;
    let steps = [Dump, Checks, Logic, Placing, Writing, Spoiler];
    assert_eq!(*began.lock().unwrap(), steps);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_seed_can_call_navi_without_the_deku_tree() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-navi-{}", std::process::id()));
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
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn grants_land_on_top_of_the_intro_skip() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let mut new_game = NewGame {
        skips: crate::cutscenes::Skips {
            intro: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let rel = crate::flows::rel(starting::OPENING);
    let parse = |r: &Rewrites| Flowchart::parse(&r[&rel]).unwrap().unwrap();
    let all = Flows::load(&romfs).unwrap();
    let flows = |new_game: &NewGame| flows(&romfs, &all, new_game, &HashMap::new());
    let skipped = parse(&flows(&new_game).unwrap().0);
    assert_ne!(
        skipped,
        crate::flows::load(&romfs, starting::OPENING)
            .unwrap()
            .unwrap()
    );
    new_game.start = starting::Start::of(&["kokiri-sword", "boomerang"]);
    let both = parse(&flows(&new_game).unwrap().0);
    let save = crate::flows::only(&both, starting::AUTOSAVE).unwrap();
    let callers = skipped.links_to(save);
    // The grid's order: the Boomerang's cell comes before the sword's.
    let [sword, boomerang] = crate::flows::running(&both, crate::flows::GRANT)[..] else {
        panic!("two grants")
    };
    assert_eq!(both.links_to(boomerang), callers);
    assert_eq!(both.links_to(sword), [Link::Next(boomerang)]);
    assert_eq!(both.links_to(save), [Link::Next(sword)]);
    assert_eq!(both.entry_points, skipped.entry_points);
    for (i, e) in (0u16..).zip(&skipped.events) {
        let runs_save = callers
            .iter()
            .any(|l| matches!(l, Link::Next(e) | Link::Branch(e, _) | Link::Join(e) if *e == i));
        if !runs_save {
            assert_eq!(
                both.events[i as usize], *e,
                "the skip left {} alone",
                e.name
            );
        }
    }
}

#[test]
fn a_moved_card_and_a_started_quest_write_their_flows_with_the_rest() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let new_game = NewGame {
        start: starting::Start::of(&["kokiri-sword"]),
        progress: vec![(flow::SAGE_OF_SPIRIT.to_string(), "Step02".into())],
        ..Default::default()
    };
    let card = HashMap::from([(1, Drop::from(gerudo::CARD.to_string()))]);
    let all = Flows::load(&romfs).unwrap();
    let (rewrites, _) = flows(&romfs, &all, &new_game, &card).unwrap();
    for name in [starting::OPENING, flow::GET_POUCH_CONTENT, flow::NPC_COMMON] {
        let data = &rewrites[&crate::flows::rel(name)];
        let flow = Flowchart::parse(data).unwrap().unwrap();
        assert_eq!(Flowchart::parse(&flow.to_bytes()).unwrap().unwrap(), flow);
    }
    let (rewrites, _) = flows(&romfs, &all, &NewGame::default(), &HashMap::new()).unwrap();
    assert!(!rewrites.contains_key(&crate::flows::rel(flow::GET_POUCH_CONTENT)));
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
    let (rewrites, _) = flows(&romfs, &all, &NewGame::default(), &HashMap::new()).unwrap();
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
    let dir = std::env::temp_dir().join(format!("zootr-steps-{}", std::process::id()));
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
fn scrubs_are_checks_only_with_the_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-scrubs-{}", std::process::id()));
    let flow = crate::flows::rel(scrubs::FLOW);
    let scrubs = |r: &Report| {
        let of = |s: &&Spot| s.holder.starts_with(scrubs::HOLDER);
        r.spots.iter().filter(of).count()
    };
    let on = NewGame {
        scrubs: true,
        ..Default::default()
    };
    let report = shuffle(
        &romfs,
        &dir.join("on"),
        7,
        &Fixed::new(),
        &on,
        &Run::silent(),
    )
    .unwrap();
    assert_eq!(scrubs(&report), scrubs::SCRUBS.len());
    assert!(dir.join("on/romfs").join(&flow).is_file());
    let report = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    );
    assert_eq!(scrubs(&report.unwrap()), 0);
    assert!(!dir.join("off/romfs").join(&flow).is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cows_are_checks_and_bools_only_with_the_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-cows-gen-{}", std::process::id()));
    let flow = crate::flows::rel(actor::ANIMAL_COW);
    let cows = |r: &Report| {
        r.spots
            .iter()
            .filter(|s| cows::is_holder(&s.holder))
            .count()
    };
    let bools = |data: &[u8]| {
        let doc = BymlDoc::parse(data).unwrap();
        let root = doc.root.as_map().unwrap();
        root["Data"].as_map().unwrap()["Bool"]
            .as_array()
            .unwrap()
            .len()
    };
    let written = |dir: &Path| {
        let raw = std::fs::read(dir.join("romfs").join(gamedata::PATH)).unwrap();
        bools(&romfs.dicts().decompress(gamedata::PATH, &raw).unwrap())
    };
    let clone = |dir: &Path| dir.join("romfs/Pack/Actor/Animal_Cow_Rando_00.pack.zs");
    let on = NewGame {
        cows: true,
        ..Default::default()
    };
    let report = shuffle(
        &romfs,
        &dir.join("on"),
        7,
        &Fixed::new(),
        &on,
        &Run::silent(),
    )
    .unwrap();
    assert_eq!(cows(&report), 8);
    assert!(dir.join("on/romfs").join(&flow).is_file());
    assert!(clone(&dir.join("on")).is_file());
    let off = &dir.join("off");
    let report = shuffle(
        &romfs,
        off,
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    assert_eq!(cows(&report), 0);
    // Against the seed without cows, since every seed adds Bools of its
    // own (Dampe's race, boss key copies).
    assert_eq!(written(&dir.join("on")), written(off) + 8);
    assert!(!off.join("romfs").join(&flow).is_file());
    assert!(!clone(off).is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shop_slots_are_checks_with_shopsanity_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-shops-gen-{}", std::process::id()));
    let shops = |r: &Report| -> Vec<Spot> {
        let of = |s: &&Spot| shops::is_holder(&s.holder);
        r.spots.iter().filter(of).cloned().collect()
    };
    let on = NewGame {
        shop_slots: 4,
        progressive: true,
        ..Default::default()
    };
    let report = shuffle(
        &romfs,
        &dir.join("on"),
        7,
        &Fixed::new(),
        &on,
        &Run::silent(),
    )
    .unwrap();
    let sold = shops(&report);
    assert_eq!(sold.len(), 32);
    assert!(sold.iter().all(|s| s.name() == s.holder));
    let pack = |r: &str| dir.join("on/romfs/Pack/Actor").join(format!("{r}.pack.zs"));
    // A slot dealt a line's level shows each step with a stand-in of its
    // own (`shops::steps`).
    let lines = crate::progressive::LINES.concat();
    let upgrade = |s: &Spot| lines.iter().any(|(actor, _)| **actor == s.item);
    assert!(sold.iter().any(upgrade), "seed 7 deals a shop an upgrade");
    let steps = std::fs::read_dir(dir.join("on/romfs/Pack/Actor")).unwrap();
    let names: Vec<_> = steps
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("Shop_Rando_") && n.ends_with("_1.pack.zs"))
    );
    assert!(pack("Shop_Rando_00").is_file());
    assert!(!pack("Shop_Rando_04").is_file());
    let off = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    );
    assert!(shops(&off.unwrap()).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pickups_are_checks_with_their_options_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-pickups-gen-{}", std::process::id()));
    let of = |r: &Report, kind: &pickups::Kind| -> HashSet<u64> {
        let checks = kind.checks(&romfs).unwrap();
        let held = |s: &&Spot| checks.iter().any(|c| c.hash == s.hash);
        r.spots.iter().filter(held).map(|s| s.hash).collect()
    };
    let on = NewGame {
        freestanding: true,
        skulltulas: true,
        boss_hearts: true,
        pots: true,
        crates: true,
        rocks: true,
        grass: true,
        hives: true,
        ..Default::default()
    };
    let report = shuffle(
        &romfs,
        &dir.join("on"),
        7,
        &Fixed::new(),
        &on,
        &Run::silent(),
    )
    .unwrap();
    assert_eq!(of(&report, &pickups::POTS).len(), 530);
    assert_eq!(of(&report, &pickups::FLYING_POTS).len(), 23);
    assert_eq!(of(&report, &pickups::CRATES).len(), 44);
    assert_eq!(of(&report, &pickups::ROCKS).len(), 167);
    assert_eq!(of(&report, &pickups::GRASS).len(), 160);
    assert_eq!(of(&report, &pickups::HIVES).len(), 21);
    assert_eq!(of(&report, &pickups::FREESTANDING).len(), 14);
    assert_eq!(of(&report, &pickups::SKULLTULAS).len(), 100);
    assert_eq!(of(&report, &pickups::BOSS_HEARTS).len(), 8);
    let tokens = report.spots.iter().filter(|s| s.item == item::TOKEN_GOLD);
    assert_eq!(tokens.count(), 100);
    let off = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    );
    let off = off.unwrap();
    assert!(of(&off, &pickups::FREESTANDING).is_empty());
    assert!(of(&off, &pickups::SKULLTULAS).is_empty());
    assert!(of(&off, &pickups::POTS).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_lake_pickups_are_checks_only_with_npc_gifts_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::NPC_GIFTS;
    let dir = std::env::temp_dir().join(format!("zootr-lake-gen-{}", std::process::id()));
    let held = |r: &Report| -> usize {
        let hashes = [&pickups::FIRE_ARROWS, &pickups::RUTOS_LETTER]
            .map(|k| k.checks(&romfs).unwrap()[0].hash);
        r.spots.iter().filter(|s| hashes.contains(&s.hash)).count()
    };
    let on = NewGame {
        on: [NPC_GIFTS.into()].into(),
        ..Default::default()
    };
    let report = shuffle(
        &romfs,
        &dir.join("on"),
        7,
        &Fixed::new(),
        &on,
        &Run::silent(),
    )
    .unwrap();
    assert_eq!(held(&report), 2);
    let off = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    );
    assert_eq!(held(&off.unwrap()), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_merchants_sell_their_checks_only_with_merchants_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::MERCHANTS;
    let dir = std::env::temp_dir().join(format!("zootr-merchants-gen-{}", std::process::id()));
    let sales: Vec<_> = gifts::GIFTS.iter().filter(|g| g.is_sale()).collect();
    assert_eq!(sales.len(), 3);
    let fixed: Fixed = sales
        .iter()
        .map(|g| (g.hash(), item::HOOKSHOT.to_string()))
        .collect();
    let held = |r: &Report| {
        let spots = |g: &&&gifts::Gift| r.spots.iter().any(|s| s.hash == g.hash());
        sales.iter().filter(spots).count()
    };
    let written = |dir: &Path, rel: &str| dir.join("romfs").join(rel).is_file();
    let rels = [
        &**flow::SIMPLE_SHOP,
        sales[0].flow,
        sales[1].flow,
        sales[2].flow,
    ]
    .map(crate::flows::rel);
    let on = NewGame {
        on: [MERCHANTS.into()].into(),
        ..Default::default()
    };
    let report = shuffle(&romfs, &dir.join("on"), 7, &fixed, &on, &Run::silent()).unwrap();
    assert_eq!(held(&report), 3);
    assert!(rels.iter().all(|r| written(&dir.join("on"), r)));
    assert!(written(&dir.join("on"), crate::gamedata::PATH));
    let off = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    );
    let report = off.unwrap();
    assert_eq!(held(&report), 0);
    assert!(rels.iter().all(|r| !written(&dir.join("off"), r)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn gift_flows_are_written_when_they_give_something() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-gift-{}", std::process::id()));
    let [saria, zelda] = [0, 1].map(|i| crate::flows::rel(gifts::GIFTS[i].flow));
    let written = |dir: &Path, rel: &str| dir.join("romfs").join(rel).is_file();
    let hook = Fixed::from([(gifts::GIFTS[0].hash(), item::HOOKSHOT.to_string())]);
    let report = shuffle(
        &romfs,
        &dir.join("a"),
        7,
        &hook,
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    assert!(written(&dir.join("a"), &saria) && written(&dir.join("a"), &zelda));
    assert!(
        report
            .spots
            .iter()
            .any(|s| s.hash == gifts::GIFTS[0].hash())
    );
    let vanilla = NewGame {
        vanilla: true,
        ..Default::default()
    };
    shuffle(
        &romfs,
        &dir.join("b"),
        7,
        &Fixed::new(),
        &vanilla,
        &Run::silent(),
    )
    .unwrap();
    assert!(!written(&dir.join("b"), &saria) && written(&dir.join("b"), &zelda));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dealt_gifts_are_checks_only_with_their_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::{DUNGEON_REWARDS, HUNDRED_TOKENS, NPC_GIFTS, SONGS, TRADE_CHAIN};
    for setting in [NPC_GIFTS, DUNGEON_REWARDS, SONGS, TRADE_CHAIN] {
        let dir = std::env::temp_dir().join(format!("zootr-{setting}-{}", std::process::id()));
        let ours: Vec<&gifts::Gift> = gifts::GIFTS
            .iter()
            .filter(|g| g.setting() == Some(setting))
            .collect();
        let dealt = |r: &Report| {
            let of = |s: &&Spot| ours.iter().any(|g| g.hash() == s.hash);
            r.spots.iter().filter(of).count()
        };
        // An adult item, so an adult-only first gift (Anju's egg) can hold it.
        let first = Fixed::from([(ours[0].hash(), item::HOOKSHOT.to_string())]);
        // With the 100-token reward on, as it is unless turned off.
        let on = NewGame {
            on: [setting, HUNDRED_TOKENS].map(String::from).into(),
            ..Default::default()
        };
        let report = shuffle(&romfs, &dir.join("on"), 7, &first, &on, &Run::silent()).unwrap();
        assert_eq!(dealt(&report), ours.len(), "{setting}");
        let flow = dir.join("on/romfs").join(crate::flows::rel(ours[0].flow));
        assert!(flow.is_file(), "{setting}");
        let off = shuffle(
            &romfs,
            &dir.join("off"),
            7,
            &Fixed::new(),
            &NewGame::default(),
            &Run::silent(),
        );
        assert_eq!(dealt(&off.unwrap()), 0, "{setting}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn the_medallion_bridge_starts_armed_and_keeps_its_step() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-bridge-gen-{}", std::process::id()));
    let on = NewGame {
        bridge_medallions: true,
        ..Default::default()
    };
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &on, &Run::silent()).unwrap();
    let step = &report.written.defaults;
    assert!(step.iter().any(|d| {
        d.starts_with(&format!("{}: ", crate::steps::saved(flow::DEFEAT_GANON)))
            && d.ends_with("-> Step01")
    }));
    assert!(
        dir.join("romfs")
            .join(crate::flows::rel(flow::ALL_SAGE))
            .is_file()
    );
    let raw = std::fs::read(dir.join("romfs").join(gamedata::PATH)).unwrap();
    let data = romfs.dicts().decompress(gamedata::PATH, &raw).unwrap();
    let mut doc = BymlDoc::parse(&data).unwrap();
    assert!(
        bridge::need_medallions(&mut doc.root).is_err(),
        "already edited"
    );
}

#[test]
fn hints_fill_every_stone_only_with_the_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-hints-{}", std::process::id()));
    let stones = |dir: &Path| {
        let rel = crate::names::TEXT;
        let raw = std::fs::read(dir.join("romfs").join(rel)).ok()?;
        let data = romfs.dicts().decompress(rel, &raw).unwrap();
        let sarc = roead::sarc::Sarc::new(&data).unwrap();
        let file = sarc.get_data(&format!("{}.msbt", hints::FILE)).unwrap();
        let lines = ounce_formats::messages(file).unwrap();
        let mut out: Vec<String> = lines
            .into_iter()
            .filter(|(l, _)| l.starts_with("GOSSIP_") && !l.contains("Rumor"))
            .map(|(_, t)| t)
            .collect();
        out.sort();
        Some(out)
    };
    let on = NewGame {
        hints: true,
        ..Default::default()
    };
    shuffle(
        &romfs,
        &dir.join("a"),
        7,
        &Fixed::new(),
        &on,
        &Run::silent(),
    )
    .unwrap();
    let read = stones(&dir.join("a")).unwrap();
    assert_eq!(read.len(), hints::STONES);
    assert!(
        read.iter().all(|l| l.starts_with("They say that ")),
        "{read:?}"
    );
    assert!(read.windows(2).any(|w| w[0] != w[1]));
    let flow = dir
        .join("a/romfs")
        .join(crate::flows::rel(flow::GOSSIP_STONE));
    assert!(flow.is_file());
    shuffle(
        &romfs,
        &dir.join("b"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    let vanilla = stones(&dir.join("b")).unwrap_or_default();
    assert!(!vanilla.iter().any(|l| l.starts_with("They say that ")));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn line_chests_hold_their_first_level_and_no_swap() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::progressive::{first, in_line};
    let checks = checks::scan(&romfs).unwrap();
    let dir = std::env::temp_dir().join(format!("zootr-prog-{}", std::process::id()));
    let new_game = NewGame {
        vanilla: true,
        progressive: true,
        ..Default::default()
    };
    shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let mut moved = 0;
    for chest in checks.iter().filter(|c| in_line(&c.item)) {
        let doc = match std::fs::read(dir.join("romfs").join(&chest.file)) {
            Ok(raw) => {
                let raw = romfs.dicts().decompress(&chest.file, &raw).unwrap();
                BymlDoc::parse(&raw).unwrap()
            }
            Err(_) => checks::load_banc(&romfs, &chest.file).unwrap(),
        };
        let actors = checks::actors(&doc.root).unwrap();
        let actor = actors
            .iter()
            .find(|a| checks::hash_of(a) == Some(chest.hash))
            .unwrap();
        let dynamic = &actor.as_map().unwrap()["Dynamic"];
        let fields = [checks::DROP_ACTOR, checks::CHECK_ACTOR, checks::SUBSTITUTE];
        let [drop, check, substitute] = fields.map(|k| checks::str_field(dynamic, k));
        assert_eq!((check, substitute), (None, None), "{}", chest.file);
        // Unshuffleable chests keep the level they name.
        let level = drop.unwrap();
        assert!(level == first(&chest.item) || level == chest.item);
        moved += usize::from(level != chest.item);
    }
    // The Water Temple's Longshot chest holds the Hookshot.
    assert!(moved > 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_pickup_holding_a_major_item_drops_its_stand_in() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-standin-{}", std::process::id()));
    // Free bombchus edit the pouch table too, and once wrote it over the
    // stand-ins' rows: the get window opened, but nothing reached the pouch
    // (Mac, 2026-11-15).
    let new_game = NewGame {
        pots: true,
        skulltulas: true,
        free_bombchus: true,
        ..Default::default()
    };
    let pot = &pickups::POTS.checks(&romfs).unwrap()[0];
    let skulltula = &pickups::SKULLTULAS.checks(&romfs).unwrap()[0];
    let fixed = Fixed::from([
        (pot.hash, item::HOOKSHOT.to_string()),
        (skulltula.hash, item::BOOMERANG.to_string()),
    ]);
    shuffle(&romfs, &dir, 7, &fixed, &new_game, &Run::silent()).unwrap();
    for check in [pot, skulltula] {
        let raw = std::fs::read(dir.join("romfs").join(&check.file)).unwrap();
        let raw = romfs.dicts().decompress(&check.file, &raw).unwrap();
        let doc = BymlDoc::parse(&raw).unwrap();
        let actors = checks::actors(&doc.root).unwrap();
        let actor = actors
            .iter()
            .find(|a| checks::hash_of(a) == Some(check.hash))
            .unwrap();
        let dynamic = &actor.as_map().unwrap()["Dynamic"];
        let drop = checks::str_field(dynamic, checks::DROP_ACTOR).unwrap();
        assert!(drop.starts_with("Obj_Rando_"), "{}: {drop}", check.holder);
        let rel = standins::POUCH_ACTOR_INFO;
        let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
        let pouch = BymlDoc::parse(&romfs.dicts().decompress(rel, &raw).unwrap()).unwrap();
        standins::row(&pouch, drop).unwrap();
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_logic_seed_keeps_each_spots_round_hero_and_rule() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("zootr-logic-{}", std::process::id()));
    let report = shuffle(
        &romfs,
        &dir,
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
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
    let dir = std::env::temp_dir().join(format!("zootr-start-{}", std::process::id()));
    let sword = item::SWORD_KOKIRI.get();
    let new_game = NewGame {
        start: starting::Start::of(&["kokiri-sword"]),
        on: ["open-forest".to_owned()].into(),
        ..NewGame::default()
    };
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    let held = report.start.items.iter().find(|h| h.item == sword);
    let held = held.expect("the starting item is listed");
    assert!(
        held.count == 1 && !held.name.starts_with("Item_"),
        "{held:?}"
    );
    assert!(report.start.settings.iter().any(|s| s.contains("orest")));
}
