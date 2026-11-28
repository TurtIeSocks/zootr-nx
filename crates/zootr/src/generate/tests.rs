use ounce_formats::{BymlDoc, Flowchart, Link, murmur3};

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::write::flows_alone;
use super::*;
use crate::checks::{self, Drop};
use crate::events::Flows;
use crate::flows::Rewrites;
use crate::known::{actor, flag, flow, item};
use crate::progress::{Run, Step, StepEvent};
use crate::settings::{NewGame, Settings};
use crate::shuffle::Fixed;
use crate::standins;
use crate::{bridge, cows, gamedata, gerudo, gifts, hints, keys, pickups, scrubs, shops, starting};

mod copies;
mod game_data;
mod seed;

/// A test's own scratch folder, emptied when made and gone when dropped
/// (after a failed assert too), so no two tests share one or leave it.
/// Each test passes its own `tag`.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new(tag: &str) -> Self {
        let name = format!("zootr-generate-{tag}-{}", std::process::id());
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every file under `dir`, by its path there, with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut todo = vec![dir.to_owned()];
    while let Some(at) = todo.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                todo.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                files.insert(path.strip_prefix(dir).unwrap().to_owned(), bytes);
            }
        }
    }
    files
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
    let romfs = crate::game::prepare(path, None, &run).unwrap();
    let dir = Scratch::new("steps-run");
    let settings = crate::settings::favorites::randomizer();
    generate(&romfs, &dir, 7, &settings, &Extras::default(), &run).unwrap();
    use Step::*;
    let steps = [Dump, Checks, Logic, Placing, Writing, Spoiler];
    assert_eq!(*began.lock().unwrap(), steps);
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
    let flows = |new_game: &NewGame| flows_alone(&romfs, &all, new_game, &HashMap::new());
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
    // Each grant goes on through its one-frame wait.
    let after_wait = |e: u16| {
        let [Link::Next(wait)] = both.links_to(e)[..] else {
            panic!("event {e} has one caller")
        };
        both.links_to(wait)
    };
    assert_eq!(after_wait(sword), [Link::Next(boomerang)]);
    assert_eq!(after_wait(save), [Link::Next(sword)]);
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
    let (rewrites, _) = flows_alone(&romfs, &all, &new_game, &card).unwrap();
    for name in [starting::OPENING, flow::GET_POUCH_CONTENT, flow::NPC_COMMON] {
        let data = &rewrites[&crate::flows::rel(name)];
        let flow = Flowchart::parse(data).unwrap().unwrap();
        assert_eq!(Flowchart::parse(&flow.to_bytes()).unwrap().unwrap(), flow);
    }
    // Magic apart rewrites the get window's meter check, so a plain seed
    // here keeps the game's meter.
    let plain = NewGame {
        magic_with_first_item: true,
        ..Default::default()
    };
    let (rewrites, _) = flows_alone(&romfs, &all, &plain, &HashMap::new()).unwrap();
    assert!(!rewrites.contains_key(&crate::flows::rel(flow::GET_POUCH_CONTENT)));
}

#[test]
fn scrubs_are_checks_only_with_the_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("scrubs");
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
}

#[test]
fn cows_are_checks_and_bools_only_with_the_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("cows-gen");
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
    let written = |dir: &Path| bools(&crate::game::read_built(&romfs, dir, gamedata::PATH));
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
}

#[test]
fn shop_slots_are_checks_with_shopsanity_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("shops-gen");
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
    // Each reads as its hand-written name, else as its shop's own.
    assert!((sold.iter()).all(|s| s.name() == crate::check_names::of(s.hash).unwrap_or(&s.holder)));
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
}

#[test]
fn pickups_are_checks_with_their_options_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("pickups-gen");
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
    assert_eq!(of(&report, &pickups::POTS).len(), 509);
    assert_eq!(of(&report, &pickups::FLYING_POTS).len(), 23);
    assert_eq!(of(&report, &pickups::CRATES).len(), 37);
    assert_eq!(of(&report, &pickups::ROCKS).len(), 149);
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
}

#[test]
fn a_freestanding_gold_skulltula_token_clears_the_floor() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("freestanding-token");
    let check = pickups::FREESTANDING
        .checks(&romfs)
        .unwrap()
        .into_iter()
        .find(|check| check.hash == 13389209437182306002)
        .unwrap();
    let fixed = Fixed::from([(check.hash, item::TOKEN_GOLD.to_string())]);
    let new_game = NewGame {
        freestanding: true,
        skulltulas: true,
        ..Default::default()
    };
    shuffle(&romfs, &dir, 7, &fixed, &new_game, &Run::silent()).unwrap();

    let position = |doc: &BymlDoc| {
        let placement = checks::actors(&doc.root)
            .unwrap()
            .iter()
            .find(|placement| checks::hash_of(placement) == Some(check.hash))
            .unwrap();
        checks::vec3(placement, checks::TRANSLATE).unwrap()
    };
    let vanilla = checks::load_banc(&romfs, &check.file).unwrap();
    let built = crate::game::read_built(&romfs, &dir, &check.file);
    let [x, y, z] = position(&vanilla);
    let [new_x, new_y, new_z] = position(&BymlDoc::parse(&built).unwrap());
    assert_eq!((new_x, new_z), (x, z));
    assert!(
        (new_y - y - 0.192_871_1).abs() < 0.000_1,
        "height: {y} -> {new_y}",
    );
}

#[test]
fn the_lake_pickups_are_checks_only_with_npc_gifts_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::NPC_GIFTS;
    let dir = Scratch::new("lake-gen");
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
}

#[test]
fn the_merchants_sell_their_checks_only_with_merchants_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::MERCHANTS;
    let dir = Scratch::new("merchants-gen");
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
}

#[test]
fn gift_flows_are_written_when_they_give_something() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("gift");
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
}

#[test]
fn dealt_gifts_are_checks_only_with_their_option_on() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::settings::{DUNGEON_REWARDS, HUNDRED_TOKENS, NPC_GIFTS, SONGS, TRADE_CHAIN};
    // With every option off, one seed answers for each setting.
    let off_dir = Scratch::new("gifts-off");
    let off = shuffle(
        &romfs,
        &off_dir,
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    // Each setting's seed builds on its own, so they build at once.
    let settings = [NPC_GIFTS, DUNGEON_REWARDS, SONGS, TRADE_CHAIN];
    ounce_formats::par_map(&settings, |&setting| {
        let dir = Scratch::new(&format!("gifts-{setting}"));
        let ours: Vec<&gifts::Gift> = gifts::GIFTS
            .iter()
            .filter(|g| g.setting == Some(setting))
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
        let report = shuffle(&romfs, &dir, 7, &first, &on, &Run::silent()).unwrap();
        assert_eq!(dealt(&report), ours.len(), "{setting}");
        let flow = dir.join("romfs").join(crate::flows::rel(ours[0].flow));
        assert!(flow.is_file(), "{setting}");
        assert_eq!(dealt(&off), 0, "{setting}");
    });
}

#[test]
fn the_medallion_bridge_starts_armed_and_keeps_its_step() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("bridge-gen");
    let on = NewGame {
        bridge: crate::settings::Bridge::SixMedallions,
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
    let data = crate::game::read_built(&romfs, &dir, gamedata::PATH);
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
    let dir = Scratch::new("hints");
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
    // An AP seed's stones name only the hints it was given.
    let given = |hints: Vec<hints::Hint>, out: &str| {
        let new_game = NewGame {
            given_hints: Some(hints),
            ..on.clone()
        };
        shuffle(
            &romfs,
            &dir.join(out),
            7,
            &Fixed::new(),
            &new_game,
            &Run::silent(),
        )
        .unwrap();
        stones(&dir.join(out)).unwrap_or_default()
    };
    let words = hints::Place::Words("Fake Pot 3 (Fake Player)".into());
    let hint = hints::Hint {
        place: words,
        item: item::HAMMER.to_string(),
    };
    let read = given(vec![hint], "c");
    assert_eq!(read.len(), hints::STONES);
    let line = "They say that Fake Pot 3 (Fake Player)\nhides Megaton Hammer.";
    assert!(read.iter().all(|l| l == line), "{read:?}");
    let none = given(vec![], "d");
    assert!(!none.iter().any(|l| l.starts_with("They say that ")));
}

#[test]
fn line_chests_hold_their_first_level_and_no_swap() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    use crate::progressive::{first, in_line};
    let checks = checks::scan(&romfs).unwrap();
    let dir = Scratch::new("prog");
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
        let dynamic = &actor.as_map().unwrap()[crate::checks::DYNAMIC];
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
}

#[test]
fn a_pickup_holding_a_major_item_drops_its_stand_in() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("standin");
    // Free bombchus edit the pouch table too, and once wrote it over the
    // stand-ins' rows: the get window opened, but nothing reached the pouch
    // (Mac, 2026-11-15).
    let new_game = NewGame {
        pots: true,
        skulltulas: true,
        bombchus: crate::settings::Bombchus::Free,
        ..Default::default()
    };
    let pot = &pickups::POTS.checks(&romfs).unwrap()[0];
    let skulltula = &pickups::SKULLTULAS.checks(&romfs).unwrap()[0];
    let fixed = Fixed::from([
        (pot.hash, item::HOOKSHOT.to_string()),
        (skulltula.hash, item::BOOMERANG.to_string()),
    ]);
    let report = shuffle(&romfs, &dir, 7, &fixed, &new_game, &Run::silent()).unwrap();
    let rel = crate::gamedata::PATH;
    let gd = crate::gamedata::GameData::parse(&crate::game::read_built(&romfs, &dir, rel)).unwrap();
    for check in [pot, skulltula] {
        let raw = crate::game::read_built(&romfs, &dir, &check.file);
        let doc = BymlDoc::parse(&raw).unwrap();
        let actors = checks::actors(&doc.root).unwrap();
        let actor = actors
            .iter()
            .find(|a| checks::hash_of(a) == Some(check.hash))
            .unwrap();
        let dynamic = &actor.as_map().unwrap()[crate::checks::DYNAMIC];
        let drop = checks::str_field(dynamic, checks::DROP_ACTOR).unwrap();
        assert!(drop.starts_with("Obj_Rando_"), "{}: {drop}", check.holder);
        let rel = standins::POUCH_ACTOR_INFO;
        let pouch = BymlDoc::parse(&crate::game::read_built(&romfs, &dir, rel)).unwrap();
        standins::row(&pouch, drop).unwrap();
        // Its take ticks it without the hook: the stand-in's Bool, read by
        // every gate through the holder's BoolExp, and by the tracker.
        let taken = pickups::taken::flag(drop);
        let spot = report.spots.iter().find(|s| s.hash == check.hash).unwrap();
        assert_eq!(spot.taken, std::slice::from_ref(&taken));
        let got = crate::picked::got(check.hash);
        let named = |n: &str| gd.name(murmur3(n.as_bytes()));
        let picked = named(&crate::picked::flag(check.hash));
        assert_eq!(
            gd.expr(&got).unwrap(),
            Some(format!("{picked} || {}", named(&taken)))
        );
        let gate = crate::world::presence_of(actor).unwrap();
        assert!(
            gate.flag == got || gate.flag == crate::picked::shown(check.hash),
            "{}",
            gate.flag
        );
    }
}

#[test]
fn blue_warps_keep_age_with_the_adult_deku_tree() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let all = Flows::load(&romfs).unwrap();
    let rel = crate::flows::rel(crate::blue_warps::FLOW);
    let warp = |new_game: &NewGame| {
        let (rewrites, _) = flows_alone(&romfs, &all, new_game, &HashMap::new()).unwrap();
        rewrites.get(&rel).cloned()
    };
    let mut new_game = NewGame::default();
    let off = warp(&new_game);
    new_game.on.insert(crate::presets::ADULT_DEKU_TREE.into());
    assert_ne!(warp(&new_game), off);
}

#[test]
fn the_pool_uses_one_check_per_linked_rock_and_excludes_missable_holders() {
    let Some(romfs) = crate::game::dev_dump() else { return };
    let game = NewGame {
        rocks: true,
        pots: true,
        crates: true,
        ..NewGame::default()
    };
    let checks = crate::checks::dealt(&romfs, &game).unwrap();
    let fixed = Fixed::new();
    let deal = crate::shuffle::deal(&checks, &fixed, &game).unwrap();
    let hashes: std::collections::HashSet<_> = deal.free.iter().map(|c| c.hash).collect();
    for hash in [
        16780156705177292663, // Child Rock 2
        13817169663978116369, // Early adult Rock 2
        10656348214587939196, // Late adult Rock 1
        6087670250388905572,  // Late signpost rock
        8668774813841012997,  // Adult shortcut boulder
        14654205574115276454, // Marsh grotto crate
        1337254747199514230,  // Escape pot
    ] {
        assert!(!hashes.contains(&hash), "extra check {hash}");
    }
    for hash in [
        17186151219195818834, // Child Rock 1
        2661037609348760452,  // Rock 7
        11138816988756551783, // Early signpost rock
        8438942300162055419,  // Child shortcut boulder
        13727300571360846696, // Barrier pot
        1256062463004045470,  // Tower pot before Ganondorf
    ] {
        assert!(hashes.contains(&hash), "missing check {hash}");
    }
}
