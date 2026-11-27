//! Seeds that move keys, maps and compasses: the checks they open and the
//! copies they deal.

use super::*;

#[test]
fn key_chests_are_checks_and_hold_no_key_only_when_keys_start() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("keys-gen");
    let held = |r: &Report| {
        let spots = r.spots.iter().filter(|s| keys::is_key(&s.vanilla));
        spots.map(|s| s.hash).collect::<HashSet<_>>()
    };
    let on = NewGame {
        keys: crate::settings::DungeonKeys::Start,
        defaults: keys::defaults(true, false),
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
    // The 37 small and 6 boss key chests, and the 2 loose keys.
    assert_eq!(held(&report).len(), 45);
    assert!(report.spots.iter().all(|s| !keys::is_key(&s.item)));
    // Logic reads no key: the chests sit behind the dungeon's own obstacles.
    for s in report.spots.iter().filter(|s| keys::is_key(&s.vanilla)) {
        assert!(!s.need.contains("small-key"), "{}", s.need);
        assert!(s.need_items.iter().all(|i| !keys::is_key(i)), "{}", s.need);
    }
    let forest = report.written.defaults.iter();
    let forest: Vec<_> = forest
        .filter(|d| d.contains(&**flag::FOREST_TEMPLE))
        .collect();
    assert_eq!(forest.len(), 2, "{forest:?}");
    assert!(forest[0].ends_with("-> 5") && forest[1].ends_with("-> true"));
    let off = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    assert!(held(&off).is_empty());
    assert!(
        !off.written
            .defaults
            .iter()
            .any(|d| d.contains("DungeonInfo"))
    );
}

#[test]
fn a_mixed_seed_deals_one_copy_for_its_ring_dungeons_and_one_for_every_other_key() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("keys-mixed");
    let mixed = NewGame {
        keys: crate::settings::DungeonKeys::Mixed,
        ..NewGame::default()
    };
    let report = shuffle(
        &romfs,
        &dir.join("mixed"),
        7,
        &Fixed::new(),
        &mixed,
        &Run::silent(),
    )
    .unwrap();
    let modes = crate::keys::KeyModes::roll(crate::settings::DungeonKeys::Mixed, 7);
    // The seed mixes: some dungeons get a ring, the rest their keys.
    let rings = modes.rings().len();
    assert!(
        rings > 0 && rings < keys::DUNGEONS.len(),
        "seed 7 rolls one way"
    );
    assert_eq!(report.key_rings.len(), rings, "{:?}", report.key_rings);
    // Every key chest is a check, and none of them holds a key the game
    // counts.
    let held = report.spots.iter().filter(|s| keys::is_key(&s.vanilla));
    assert_eq!(held.count(), 45);
    assert!(report.spots.iter().all(|s| !keys::is_key(&s.item)));
    // Each dungeon holds as many copies as its mode deals.
    for (i, d) in keys::DUNGEONS.iter().enumerate() {
        let n = i + 1;
        let copy = crate::key_tints::ring(n);
        let dealt = report.spots.iter().filter(|s| s.item == copy).count() as u32;
        let want = match modes.mode(n) {
            crate::keys::KeyMode::Rings => 1,
            crate::keys::KeyMode::Singles => keys::copies_of(n),
        };
        assert_eq!(
            dealt,
            want,
            "{}: {}",
            d.name,
            modes.mode(n) == crate::keys::KeyMode::Rings
        );
    }
}

#[test]
fn thief_keys_are_checks_and_the_carried_ones_are_cleared_when_fortress_keys_start() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("fortress-gen");
    let thieves = |r: &Report| {
        let spots = r.spots.iter().filter(|s| s.holder == pickups::THIEF);
        spots.cloned().collect::<Vec<_>>()
    };
    // The thieves still carrying a key, in the written fortress map or the
    // game's own where the mod leaves it alone.
    let carrying = |dir: &Path| {
        let rel = &format!(
            "Banc/{}/{}_Static.bcett.byml.zs",
            flag::GERUDO_FORTRESS,
            crate::known::map::GERUDO_FORTRESS_ADULT
        );
        let data = match std::fs::read(dir.join("romfs").join(rel)) {
            Ok(raw) => romfs.dicts().decompress(rel, &raw).unwrap(),
            Err(_) => romfs.read(rel).unwrap(),
        };
        let doc = BymlDoc::parse(&data).unwrap();
        let held = |a: &&ounce_formats::Byml| {
            let dynamic = a.as_map().unwrap().get(crate::checks::DYNAMIC);
            let drop = dynamic.and_then(|d| checks::str_field(d, checks::DROP_ACTOR));
            drop.is_some_and(keys::is_key)
        };
        checks::actors(&doc.root)
            .unwrap()
            .iter()
            .filter(held)
            .count()
    };
    let on = NewGame {
        fortress_keys: crate::settings::FortressKeys::Start,
        defaults: keys::defaults(false, true),
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
    let spots = thieves(&report);
    assert_eq!(spots.len(), 4);
    for s in &spots {
        assert!(
            keys::is_key(&s.vanilla) && !keys::is_key(&s.item),
            "{}",
            s.item
        );
        assert!(s.need.contains("adult"), "{}", s.need);
    }
    assert_eq!(
        report
            .spots
            .iter()
            .filter(|s| keys::is_key(&s.vanilla))
            .count(),
        4
    );
    assert_eq!(carrying(&dir.join("on")), 0);
    let rel = crate::flows::rel(flow::SAGE_OF_SPIRIT_RESCUE_CARPENTER);
    assert!(dir.join("on/romfs").join(&rel).is_file());
    let counter = "DungeonInfo.GerudoFortress.SmallKeyNum: 0 -> 4";
    assert!(report.written.defaults.contains(&counter.to_owned()));
    let off = shuffle(
        &romfs,
        &dir.join("off"),
        7,
        &Fixed::new(),
        &NewGame::default(),
        &Run::silent(),
    )
    .unwrap();
    assert!(thieves(&off).is_empty());
    assert_eq!(carrying(&dir.join("off")), 3);
}

#[test]
fn fortress_keysanity_writes_its_copy_and_deals_one_for_every_thief() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("fortress-singles");
    let settings = crate::settings::favorites::randomizer_and(Settings::from([(
        crate::settings::FORTRESS_KEYS.into(),
        "singles".into(),
    )]));
    let new_game = NewGame::from_settings(&settings, &[]).unwrap();
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let copy = crate::key_tints::fortress();
    let dealt = report.spots.iter().filter(|s| s.item == copy).count();
    assert_eq!(dealt, 4);
    let thieves = report.spots.iter().filter(|s| s.holder == pickups::THIEF);
    assert_eq!(thieves.count(), 4);
    let pack = ounce_formats::pack_file(&copy);
    assert!(dir.join("romfs").join(pack).is_file());
    // No key at the start: the copies fill the counter.
    let counter = "DungeonInfo.GerudoFortress.SmallKeyNum";
    assert!(!report.written.defaults.iter().any(|d| d.contains(counter)));
}

#[test]
fn freed_carpenters_keep_the_thieves_and_their_keys_with_fortress_keysanity() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("fortress-freed");
    let settings = crate::settings::favorites::randomizer_and(Settings::from([
        (crate::settings::FORTRESS_KEYS.into(), "singles".into()),
        (crate::settings::GERUDO_FORTRESS.into(), "freed".into()),
    ]));
    let new_game = NewGame::from_settings(&settings, &[]).unwrap();
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let thieves = report.spots.iter().filter(|s| s.holder == pickups::THIEF);
    assert_eq!(thieves.count(), 4);
    let copy = crate::key_tints::fortress();
    assert_eq!(report.spots.iter().filter(|s| s.item == copy).count(), 4);
    let beaten = |d: &&String| d.contains("DestroyGerudoThief");
    assert!(!report.written.defaults.iter().any(|d| beaten(&d)));
}

#[test]
fn maps_and_compasses_deal_their_dungeon_s_copies_anywhere_and_name_it() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    let dir = Scratch::new("map-copies");
    let settings = crate::settings::favorites::randomizer_and(Settings::from([(
        crate::settings::MAPS_COMPASSES.into(),
        true.into(),
    )]));
    let new_game = NewGame::from_settings(&settings, &[]).unwrap();
    let report = shuffle(&romfs, &dir, 7, &Fixed::new(), &new_game, &Run::silent()).unwrap();
    let copies: Vec<_> = (report.spots.iter())
        .filter(|s| crate::map_copies::is_copy(&s.item))
        .collect();
    assert_eq!(copies.len(), 20);
    let vanilla = crate::narrowing::MAPS_COMPASSES;
    assert!(
        !report
            .spots
            .iter()
            .any(|s| vanilla.iter().any(|m| **m == *s.item))
    );
    let mut away = 0;
    for s in &copies {
        assert!(
            dir.join("romfs")
                .join(ounce_formats::pack_file(&s.item))
                .is_file()
        );
        // Named after its own dungeon, wherever it lies.
        assert!(
            s.item_name.ends_with(" Map") || s.item_name.ends_with(" Compass"),
            "{}",
            s.item_name
        );
        let own = crate::map_copies::copy_name(&s.item, |id| id.to_owned()).unwrap();
        away += usize::from(!own.starts_with(&s.area));
    }
    assert!(away > 0, "every copy stayed in its own dungeon");
}

#[test]
fn a_pot_dealt_a_compass_drops_the_compass_copy_s_stand_in() {
    let Some(romfs) = crate::game::dev_dump() else {
        return;
    };
    // A Kokiri Forest pot, its vanilla drop a Blue Rupee.
    const POT: u64 = 5133775918408786238;
    let dir = Scratch::new("pot-compass");
    let settings = crate::settings::favorites::randomizer_and(Settings::from([
        (crate::settings::MAPS_COMPASSES.into(), true.into()),
        (crate::settings::POTS.into(), true.into()),
    ]));
    let new_game = NewGame::from_settings(&settings, &[]).unwrap();
    let copies = crate::map_copies::sets().into_iter().map(|(c, _)| c);
    let compass = copies.into_iter().find(|c| c.contains("Compass")).unwrap();
    let fixed = Fixed::from([(POT, compass.clone())]);
    let report = shuffle(&romfs, &dir, 7, &fixed, &new_game, &Run::silent()).unwrap();
    let spot = report.spots.iter().find(|s| s.hash == POT).unwrap();
    assert_eq!(spot.item, compass);
    let rel = &format!(
        "Banc/{0}/{1}_Dynamic.bcett.byml.zs",
        flag::KOKIRI_FOREST,
        crate::known::map::KOKIRI_FOREST_CHILD
    );
    let doc = BymlDoc::parse(&crate::game::read_built(&romfs, &dir, rel)).unwrap();
    let actors = checks::actors(&doc.root).unwrap();
    let pot = actors
        .iter()
        .find(|a| checks::hash_of(a) == Some(POT))
        .unwrap();
    let dynamic = pot.as_map().unwrap().get(crate::checks::DYNAMIC).unwrap();
    let drop = checks::str_field(dynamic, checks::DROP_ACTOR).unwrap();
    assert!(
        dir.join("romfs")
            .join(ounce_formats::pack_file(drop))
            .is_file(),
        "{drop}"
    );
}
