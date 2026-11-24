use std::collections::HashMap;

use ounce_formats::{EntryPoint, EventKind, Flowchart, Value};

use super::events::{calls, case, next, with};
use super::placements::{seller, sells};
use super::sales::{run_into, secret};
use super::spots::{Branch, RED, edits, plain};
use super::*;
use crate::checks::Drop;
use crate::flows::{
    self, ACTOR as ACTOR_PARAM, GAME_DATA, IN_POUCH, IS_BOOL, Model, Rewrites, events, param,
    reachable,
};
use crate::game::dev_dump;
use crate::known::{Name, actor, field, flag, item};

const KOKIRI: usize = 0;
const LW_STICKS: usize = 1;
const LW_NUTS: usize = 2;
const DMC_HEART: usize = 3;
const HF_HEART: usize = 4;
const HF_BOMBS: usize = 7;
/// A price no spot asks in the vanilla game, so a sale at it shows the
/// setting took.
const PRICE_SET: i32 = 25;

/// The index consts point at the spots they're named for, so a spot added
/// to the table can't move them onto another.
#[test]
fn the_test_spots_are_the_ones_named() {
    let spots = [
        (KOKIRI, flag::KOKIRI_FOREST, "sticks"),
        (LW_STICKS, flag::LOST_WOOD, "sticks"),
        (LW_NUTS, flag::LOST_WOOD, "nuts"),
        (DMC_HEART, flag::DEATH_MOUNTAIN_CRATER, "heart piece"),
        (HF_HEART, flag::HYRULE_FIELD, "heart piece"),
        (HF_BOMBS, flag::HYRULE_FIELD, "bombs"),
    ];
    for (at, area, name) in spots {
        let s = &SCRUBS[at];
        assert!(s.area == area && s.name == name, "{at}: {}", s.name);
    }
}

fn drops(at: &[(usize, impl AsRef<str>)]) -> HashMap<u64, Drop> {
    at.iter()
        .map(|(s, item)| (SCRUBS[*s].hash(), Drop::from(item.as_ref().to_owned())))
        .collect()
}

/// The pouch question the chains ask.
fn model(romfs: &ounce_formats::Romfs) -> Model {
    let flows = crate::events::Flows::load(romfs).unwrap();
    crate::gifts::levels::model(&flows).unwrap()
}

/// The flow before and after `write` with progressive items on, or `None`
/// without a dump.
fn written(at: &[(usize, impl AsRef<str>)]) -> Option<(Flowchart, Flowchart)> {
    let romfs = dev_dump()?;
    let parse = |data: &[u8]| Flowchart::parse(data).unwrap().unwrap();
    let before = parse(&romfs.read(&flows::rel(FLOW)).unwrap());
    let mut rewrites = Rewrites::new();
    let model = model(&romfs);
    let level = Level {
        model: &model,
        lines: crate::progressive::Lines::climbing(),
    };
    write(&romfs, &mut rewrites, &drops(at), PRICE_SET, level).unwrap();
    Some((before, parse(&rewrites[&flows::rel(FLOW)])))
}

fn head(flow: &Flowchart) -> u16 {
    let compares = with(flow, COMPARE, GAME_DATA, TEMP);
    let heads = with(flow, COMPARE, field::VALUE, HEAD);
    heads.into_iter().find(|e| compares.contains(e)).unwrap()
}

/// What the pouch root's chain sells `item` as: the call's item and price.
fn branch(flow: &Flowchart, item: &str) -> Option<String> {
    let compares = with(flow, COMPARE, GAME_DATA, TEMP);
    let root = reachable(flow, ROOT).unwrap();
    let hit = with(flow, COMPARE, field::VALUE, item)
        .into_iter()
        .find(|e| compares.contains(e) && root.contains(e))?;
    let at = case(flow, hit, 0).unwrap();
    let EventKind::SubFlow { params, .. } = &flow.events[at as usize].kind else {
        panic!("{item} doesn't sell")
    };
    let p = |k| param(params, k).map_or(String::new(), str::to_owned);
    let price = params.iter().find(|(k, _)| k == PRICE_PARAM).unwrap();
    assert_eq!(p(SELLER), seller(item), "{item}");
    Some(format!("{} {:?}", p(flag::ACTOR_NAME), price.1))
}

#[test]
fn a_changed_pouch_scrub_sells_its_item_ahead_of_the_vanilla_chain() {
    let Some((before, after)) = written(&[(KOKIRI, item::BOOMERANG)]) else {
        return;
    };
    let sold = branch(&after, item::BOOMERANG).unwrap();
    assert_eq!(sold, format!("{} Int([25])", item::BOOMERANG));
    assert_eq!(head(&after), head(&before));
    let root = reachable(&after, ROOT).unwrap();
    assert!(root.contains(&head(&after)), "vanilla chain cut off");
    let vanilla = with(&after, COMPARE, GAME_DATA, TEMP);
    for e in [head(&after), case(&after, head(&after), 1).unwrap()] {
        assert!(vanilla.contains(&e) && root.contains(&e));
    }
}

const SONG: &Name = item::MELODY_SUN_SONG;
const MEDAL: &Name = item::MEDAL_WATER;
const STONE: &Name = item::STONE_FIRE;

fn entry_of<'a>(flow: &'a Flowchart, name: &str) -> &'a EntryPoint {
    let found = flow.entry_points.iter().find(|e| e.name == name);
    found.unwrap_or_else(|| panic!("no entry point {name}"))
}

/// An event's param `key`.
fn value<'a>(flow: &'a Flowchart, e: u16, key: &str) -> Option<&'a Value> {
    let params = match &flow.events[e as usize].kind {
        EventKind::Action { params, .. }
        | EventKind::Switch { params, .. }
        | EventKind::SubFlow { params, .. } => params,
        _ => return None,
    };
    params.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A pair of names as text, to read against what the flow runs.
fn texts<'a>(what: (&'a Name, &'a Name)) -> (&'a str, &'a str) {
    (what.0.get(), what.1.get())
}

/// What an action runs, as the actor's name and the action's.
fn action_of(flow: &Flowchart, e: u16) -> (&str, &str) {
    match &flow.events[e as usize].kind {
        EventKind::Action { actor, action, .. } => {
            let actor = flow.actor_of(*actor);
            (&actor.name, &actor.actions[*action as usize])
        }
        other => panic!("{other:?} isn't an action"),
    }
}

/// The calls `entry` makes into other flows, as `FLOW::ENTRY`.
fn called(flow: &Flowchart, entry: &str) -> Vec<String> {
    let reached = reachable(flow, entry).unwrap();
    let calls = reached
        .iter()
        .filter_map(|&e| match &flow.events[e as usize].kind {
            EventKind::SubFlow {
                flowchart,
                entry_point,
                ..
            } if !flowchart.is_empty() => Some(format!("{flowchart}::{entry_point}")),
            _ => None,
        });
    calls.collect()
}

/// Where the pouch root's compare for `item` goes on a match.
fn sale(flow: &Flowchart, item: &str) -> u16 {
    let compares = with(flow, COMPARE, GAME_DATA, TEMP);
    let root = reachable(flow, ROOT).unwrap();
    let hit = with(flow, COMPARE, field::VALUE, item)
        .into_iter()
        .find(|e| compares.contains(e) && root.contains(e))
        .unwrap();
    case(flow, hit, 0).unwrap()
}

/// Every sale call reachable from `at`, as its item and seller line.
fn sales_from(flow: &Flowchart, at: u16) -> Vec<(String, String)> {
    let reached = flows::reachable_from(flow, Some(at));
    let mut sales: Vec<_> = (reached.into_iter())
        .filter_map(|e| match &flow.events[e as usize].kind {
            EventKind::SubFlow { params, .. } => {
                let p = |k| param(params, k).map(str::to_owned);
                Some((p(flag::ACTOR_NAME)?, p(SELLER)?))
            }
            _ => None,
        })
        .collect();
    sales.sort();
    sales.dedup();
    sales
}

/// Whether `e` asks the pouch for `item`.
fn asks_for(flow: &Flowchart, e: u16, item: &str) -> bool {
    let asks = events(flow, IN_POUCH).into_iter().any(|(at, _)| at == e);
    asks && value(flow, e, ACTOR_PARAM).and_then(Value::as_str) == Some(item)
}

#[test]
fn a_plain_item_sells_through_the_rupee_deduction_and_the_plain_grant() {
    for item in [SONG, MEDAL, STONE] {
        let Some((before, after)) = written(&[(LW_NUTS, item)]) else {
            return;
        };
        let call = sale(&after, item);
        let EventKind::SubFlow { entry_point, .. } = &after.events[call as usize].kind else {
            panic!("{item} doesn't sell")
        };
        assert_eq!(entry_point, PLAIN_ONE_TIME);
        let named = |key| value(&after, call, key).and_then(Value::as_str);
        assert_eq!(named(flag::ACTOR_NAME), Some(item.get()));
        assert_eq!(named(PRICE_PARAM), None);
        assert_eq!(named(SELLER), Some(seller(item).as_str()));

        // The copy takes the rupees, waits for the count, then grants by
        // the sale's own `ActorName`, and never shows the item in hand.
        let show = format!("{SHOW_FLOW}::{SHOW}");
        assert_eq!(
            called(&after, PLAIN_ONE_TIME),
            [format!("{}::{}", GRANT.0, GRANT.1)]
        );
        assert!(called(&after, ONE_TIME).contains(&show));
        let reached = reachable(&after, PLAIN_ONE_TIME).unwrap();
        let pay = events(&after, PAY);
        let pay: Vec<_> = pay.iter().filter(|(e, _)| reached.contains(e)).collect();
        let [(pay, _)] = pay[..] else {
            panic!("{pay:?}")
        };
        let wait = next(&after, *pay).unwrap();
        let grant = next(&after, wait).unwrap();
        assert_eq!(action_of(&after, wait), texts(WAIT_PAID));
        let by = value(&after, grant, flag::ACTOR_NAME);
        assert_eq!(by, Some(&Value::Argument(flag::ACTOR_NAME.to_string())));
        assert_eq!(
            value(&after, *pay, GAME_DATA).and_then(Value::as_str),
            Some(PRICE.get())
        );

        // The vanilla sale is untouched. The copy has its own argument
        // actor, so its events never talk through the original's.
        assert_eq!(entry_of(&after, ONE_TIME), entry_of(&before, ONE_TIME));
        let at = |name: &str| after.entry_points.iter().position(|e| e.name == name);
        let argument = |name: &str| {
            let at = at(name).map(|i| i as u16);
            after
                .actors
                .iter()
                .position(|a| a.argument_entry == at && at.is_some())
        };
        let (old, new) = (
            argument(ONE_TIME).unwrap(),
            argument(PLAIN_ONE_TIME).unwrap(),
        );
        assert_ne!(old, new);
        for e in reached {
            if let EventKind::Action { actor, .. } | EventKind::Switch { actor, .. } =
                &after.events[e as usize].kind
            {
                assert_ne!(*actor as usize, old, "event {e}");
            }
        }
    }
}

#[test]
fn a_plain_pouch_spot_names_the_item_after_reading_the_placement() {
    let Some((before, after)) = written(&[(LW_NUTS, SONG)]) else {
        return;
    };
    let nuts = &SCRUBS[LW_NUTS];
    // The placement keeps the item it came with and runs its own entry,
    // so the talk starts the way every other scrub's does.
    let fields = fields(&drops(&[(LW_NUTS, SONG)]));
    for h in nuts.placements {
        assert!(fields.contains(&format!("{h}.Dynamic.{ITEM}={}", nuts.item)));
        let entry = sells(nuts, SONG);
        assert!(fields.contains(&format!("{h}.Dynamic.{TALK}={entry}")));
        assert!(
            !fields
                .iter()
                .any(|f| f.starts_with(&format!("{h}.Dynamic.{ATTACKED}")))
        );
    }
    // The entry sets the item, then runs the vanilla entry's call, then
    // the spot's own sold flag.
    let main = entry_of(&after, &sells(nuts, SONG)).main.unwrap();
    assert_eq!(action_of(&after, main), texts(SET_STRING));
    assert_eq!(
        value(&after, main, GAME_DATA).and_then(Value::as_str),
        Some(SELECTED.get())
    );
    assert_eq!(
        value(&after, main, field::VALUE).and_then(Value::as_str),
        Some(SONG.get())
    );
    let call = next(&after, main).unwrap();
    let vanilla = entry_of(&before, nuts.talk).main.unwrap();
    let EventKind::SubFlow { entry_point, .. } = &after.events[call as usize].kind else {
        panic!("no call")
    };
    assert_eq!(entry_point, ROOT);
    assert_eq!(
        value(&after, call, "Greeting_00"),
        value(&before, vanilla, "Greeting_00")
    );
    assert_eq!(sold_flag(&after, call), nuts.flag());
    // After the talk reads the placement, the item set there goes into
    // `TEMP`, and is cleared. With none set the talk carries on as before.
    let reads = events(&before, WRITE_ITEM);
    let read = reads
        .iter()
        .find(|(e, _)| reachable(&before, ROOT).unwrap().contains(e));
    let read = read.unwrap().0;
    let carry_on = next(&before, read).unwrap();
    let compare = next(&after, read).unwrap();
    assert_eq!(
        value(&after, compare, GAME_DATA).and_then(Value::as_str),
        Some(SELECTED.get())
    );
    assert_eq!(
        value(&after, compare, field::VALUE).and_then(Value::as_str),
        Some(SONG.get())
    );
    assert_eq!(case(&after, compare, 1).unwrap(), carry_on);
    let set = case(&after, compare, 0).unwrap();
    assert_eq!(action_of(&after, set), texts(SET_STRING));
    assert_eq!(
        value(&after, set, GAME_DATA).and_then(Value::as_str),
        Some(TEMP.get())
    );
    assert_eq!(
        value(&after, set, field::VALUE).and_then(Value::as_str),
        Some(SONG.get())
    );
    let clear = next(&after, set).unwrap();
    assert_eq!(
        value(&after, clear, GAME_DATA).and_then(Value::as_str),
        Some(SELECTED.get())
    );
    assert_eq!(
        value(&after, clear, field::VALUE).and_then(Value::as_str),
        Some("")
    );
    assert_eq!(next(&after, clear), Some(carry_on));
}

#[test]
fn every_spot_can_sell_a_plain_item_at_once() {
    let items = [SONG, MEDAL, STONE, item::MEDAL_FIRE];
    let at: Vec<_> = (0..SCRUBS.len()).map(|s| (s, items[s % 4])).collect();
    let Some((_, after)) = written(&at) else {
        return;
    };
    for &(s, item) in &at {
        let spot = &SCRUBS[s];
        if spot.branch != Branch::Secret {
            let main = entry_of(&after, &sells(spot, item)).main.unwrap();
            assert_eq!(
                value(&after, main, field::VALUE).and_then(Value::as_str),
                Some(item.get())
            );
        }
    }
    // One entry per spot, and one plain sale for all of them.
    let entries = after.entry_points.iter();
    assert_eq!(
        entries.filter(|e| e.name.contains("Sells")).count(),
        SCRUBS.len() - 1
    );
    let plain = after
        .entry_points
        .iter()
        .filter(|e| e.name == PLAIN_ONE_TIME);
    assert_eq!(plain.count(), 1);
}

#[test]
fn a_heart_piece_sale_beside_a_plain_item_writes() {
    // Selling the vanilla chain's own head item adds a second compare
    // for it ahead of the chain.
    let Some((_, after)) = written(&[(KOKIRI, HEART), (LW_NUTS, SONG)]) else {
        return;
    };
    let main = entry_of(&after, &sells(&SCRUBS[LW_NUTS], SONG)).main;
    assert!(main.is_some());
}

#[test]
fn a_mod_with_plain_items_at_scrubs_generates() {
    let Some(romfs) = dev_dump() else { return };
    let at = [
        (LW_NUTS, SONG),
        (HF_HEART, MEDAL),
        (HF_BOMBS, STONE),
        (KOKIRI, item::HAMMER),
    ];
    let dir = std::env::temp_dir().join(format!("zootr-plain-scrubs-{}", std::process::id()));
    let new_game = crate::settings::NewGame {
        scrubs: true,
        ..Default::default()
    };
    crate::generate::write_mod(
        &romfs,
        &dir,
        &drops(&at),
        &new_game,
        &[],
        &crate::progress::Run::silent(),
    )
    .unwrap();
    let rel = flows::rel(FLOW);
    let raw = std::fs::read(dir.join("romfs").join(&rel)).unwrap();
    let data = romfs.dicts().decompress(&rel, &raw).unwrap();
    let flow = Flowchart::parse(&data).unwrap().unwrap();
    assert!(flow.entry_points.iter().any(|e| e.name == PLAIN_ONE_TIME));
    assert!(!reachable(&flow, ROOT).unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_edited_spot_with_an_attack_entry_takes_it_over_too() {
    needs_names!();
    let dm = SCRUBS
        .iter()
        .position(|s| s.talk == actor::ENEMY_SCRUB_BUSINESS_HEART_02)
        .unwrap();
    for item in [MEDAL, item::HAMMER] {
        let fields = fields(&drops(&[(dm, item)]));
        let entry = sells(&SCRUBS[dm], item);
        for h in SCRUBS[dm].placements {
            assert!(fields.contains(&format!("{h}.Dynamic.{ATTACKED}={entry}")));
        }
    }
}

#[test]
fn a_plain_item_at_a_shop_spot_keeps_the_spot_s_stock_and_runs_its_own_entry() {
    let Some((_, after)) = written(&[(HF_BOMBS, STONE)]) else {
        return;
    };
    let bombs = &SCRUBS[HF_BOMBS];
    let fields = fields(&drops(&[(HF_BOMBS, STONE)]));
    let h = bombs.hash();
    assert!(fields.contains(&format!("{h}.Dynamic.{ITEM}={RED}")));
    assert!(fields.contains(&format!("{h}.Dynamic.{TALK}={}", sells(bombs, STONE))));
    let main = entry_of(&after, &sells(bombs, STONE)).main.unwrap();
    let call = next(&after, main).unwrap();
    assert_eq!(sold_flag(&after, call), bombs.flag());
    let sale = sale(&after, STONE);
    let EventKind::SubFlow { entry_point, .. } = &after.events[sale as usize].kind else {
        panic!("{STONE} doesn't sell")
    };
    assert_eq!(entry_point, PLAIN_ONE_TIME);
}

#[test]
fn hyrule_fields_riddle_scrub_sells_a_plain_item_through_the_plain_sale() {
    let Some((_, after)) = written(&[(HF_HEART, SONG)]) else {
        return;
    };
    let secret = reachable(&after, SECRET).unwrap();
    let sales: Vec<_> = calls(&after, PLAIN_ONE_TIME)
        .into_iter()
        .filter(|e| secret.contains(e))
        .collect();
    let [sale] = sales[..] else {
        panic!("{sales:?}")
    };
    assert_eq!(
        value(&after, sale, flag::ACTOR_NAME).and_then(Value::as_str),
        Some(SONG.get())
    );
    let hf = SCRUBS[HF_HEART].hash().to_string();
    assert!(after.entry_points.iter().all(|e| !e.name.contains(&hf)));
}

#[test]
fn items_with_a_get_animation_keep_the_vanilla_sale() {
    let Some((_, after)) = written(&[(KOKIRI, item::BOOMERANG)]) else {
        return;
    };
    assert!(after.entry_points.iter().all(|e| e.name != PLAIN_ONE_TIME));
    let call = sale(&after, item::BOOMERANG);
    let EventKind::SubFlow { entry_point, .. } = &after.events[call as usize].kind else {
        panic!("no sale")
    };
    assert_eq!(entry_point, ONE_TIME);
}

#[test]
fn songs_medallions_and_stones_are_plain() {
    needs_names!();
    let songs = [
        "ZeldaLullaby",
        flag::EPONA_SONG,
        flag::SARIA_SONG,
        flag::SUN_SONG,
        "TimeSong",
        "StormSong",
        flag::FOREST_MINUET,
        flag::FIRE_BOLERO,
        flag::WATER_SERENADE,
        flag::SPIRIT_REQUIEM,
        flag::SHADOW_NOCTURNE,
        flag::LIGHT_PRELUDE,
        "ScarecrowSong",
    ];
    let medals = ["Forest", "Fire", "Water", "Spirit", "Shadow", "Light"];
    let stones = ["Forest", "Fire", "Water"];
    let items = songs
        .map(|s| format!("Item_Melody_{s}"))
        .into_iter()
        .chain(medals.map(|m| format!("Item_Medal_{m}")))
        .chain(stones.map(|s| format!("Item_Stone_{s}")));
    for item in items {
        assert!(plain(&item), "{item}");
    }
    for item in [
        item::HAMMER,
        "Item_L",
        actor::WEAPON_SHIELD_DEKU,
        actor::OBJ_HEART_PIECE,
    ] {
        assert!(!plain(item), "{item}");
    }
}

#[test]
fn a_shop_spot_runs_its_own_entry_into_the_pouch_root() {
    let Some((_, after)) = written(&[(HF_BOMBS, item::DIN_FIRE)]) else {
        return;
    };
    assert_eq!(
        branch(&after, item::DIN_FIRE).unwrap(),
        format!("{} Int([25])", item::DIN_FIRE)
    );
    assert!(branch(&after, RED).is_some());
    let entry = reachable(&after, &sells(&SCRUBS[HF_BOMBS], item::DIN_FIRE)).unwrap();
    assert!(entry.contains(&head(&after)));
}

#[test]
fn an_unchanged_seed_edits_only_the_shop_spots() {
    needs_names!();
    let edited: Vec<_> = edits(&HashMap::new()).map(|(s, _)| s.branch).collect();
    assert_eq!(edited, [Branch::Shop; 10]);
    let fields = fields(&HashMap::new());
    let hf = SCRUBS[HF_BOMBS].hash();
    let talk = sells(&SCRUBS[HF_BOMBS], RED);
    assert!(fields.contains(&format!("{hf}.Dynamic.{TALK}={talk}")));
    assert!(fields.contains(&format!("{hf}.Dynamic.{ITEM}={RED}")));
    assert!(
        !fields
            .iter()
            .any(|f| f.starts_with(&SCRUBS[KOKIRI].hash().to_string()))
    );
}

#[test]
fn every_copy_of_a_spot_takes_the_item_its_entry_and_its_sold_flag() {
    let kokiri = &SCRUBS[KOKIRI];
    let drops = drops(&[(KOKIRI, item::BOOMERANG)]);
    let fields = fields(&drops);
    let gone = gone(&drops);
    for h in kokiri.placements {
        assert!(fields.contains(&format!("{h}.Dynamic.{ITEM}={}", item::BOOMERANG)));
        let talk = sells(kokiri, item::BOOMERANG);
        assert!(fields.contains(&format!("{h}.Dynamic.{TALK}={talk}")));
        assert!(gone.contains(&(*h, kokiri.flag())));
    }
}

/// The riddle scrub's flow dealt `item`, or `None` without a dump.
fn riddle(item: &str) -> Option<Flowchart> {
    let romfs = dev_dump()?;
    let data = romfs.read(&flows::rel(FLOW)).unwrap();
    let mut flow = Flowchart::parse(&data).unwrap().unwrap();
    let model = model(&romfs);
    let level = Level {
        model: &model,
        lines: crate::progressive::Lines::climbing(),
    };
    secret(&mut flow, item, PRICE_SET, level).unwrap();
    Some(flow)
}

/// Each sale the riddle talk reaches: what it sells, what its shop opening
/// shows, and whether that opening shows the hearts.
fn riddle_sales(flow: &Flowchart) -> Vec<(String, Vec<String>, bool)> {
    let talk = reachable(flow, SECRET).unwrap();
    let gauges: Vec<u16> = events(flow, HEART_GAUGE)
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    calls(flow, ONE_TIME)
        .into_iter()
        .filter(|e| talk.contains(e))
        .map(|sale| {
            let opens = run_into(flow, sale);
            let shown = (opens.iter())
                .filter(|e| with(flow, SET_STRING, GAME_DATA, SELECTED).contains(e))
                .map(|&e| sold(flow, e, field::VALUE))
                .collect();
            let hearts = opens.iter().any(|e| gauges.contains(e));
            (sold(flow, sale, flag::ACTOR_NAME), shown, hearts)
        })
        .collect()
}

fn sold(flow: &Flowchart, e: u16, key: &str) -> String {
    let kind = &flow.events[e as usize].kind;
    let (EventKind::Action { params, .. } | EventKind::SubFlow { params, .. }) = kind else {
        panic!("event {e} has no params")
    };
    param(params, key).unwrap().to_owned()
}

#[test]
fn the_riddle_scrub_opens_its_shop_naming_what_it_sells() {
    let Some(flow) = riddle(item::HAMMER) else {
        return;
    };
    let sales = riddle_sales(&flow);
    let hammer = item::HAMMER.to_string();
    assert_eq!(sales, [(hammer.clone(), vec![hammer], false)]);
    let fields = fields(&drops(&[(HF_HEART, item::HAMMER)]));
    let hf = SCRUBS[HF_HEART].placements;
    assert!(
        fields
            .iter()
            .all(|f| hf.iter().all(|h| !f.starts_with(&h.to_string())))
    );
}

#[test]
fn the_riddle_scrub_dealt_the_hookshot_opens_a_shop_per_step() {
    let Some(flow) = riddle(item::HOOKSHOT) else {
        return;
    };
    let mut sales = riddle_sales(&flow);
    sales.sort();
    // Link owning either shot gets the Longshot.
    sales.dedup();
    let step = |i: &str| (i.to_string(), vec![i.to_string()], false);
    assert_eq!(sales, [step(item::HOOKSHOT), step(item::LONGSHOT)]);
}

#[test]
fn the_riddle_scrub_dealt_a_heart_still_shows_the_hearts() {
    let Some(flow) = riddle(HEART) else { return };
    let heart = HEART.to_string();
    assert_eq!(riddle_sales(&flow), [(heart.clone(), vec![heart], true)]);
}

#[test]
fn two_scrubs_selling_one_item_share_a_branch() {
    let Some((_, after)) = written(&[(KOKIRI, "Item_L"), (LW_STICKS, "Item_L")]) else {
        return;
    };
    let compares = with(&after, COMPARE, field::VALUE, "Item_L");
    assert_eq!(compares.len(), 1);
}

#[test]
fn each_item_sold_gets_one_line() {
    needs_names!();
    let at = [
        (KOKIRI, "Item_L"),
        (LW_STICKS, "Item_L"),
        (HF_HEART, item::HAMMER),
    ];
    let drops = drops(&at);
    let lines = lines(&drops, Default::default());
    let items: Vec<&str> = lines.new.iter().map(|(_, i)| *i).collect();
    assert_eq!(items, ["Item_L", item::HAMMER, RED]);
    assert_eq!(
        format!("{}:{}", *TEXT, lines.new[1].0),
        seller(item::HAMMER)
    );
    assert!(
        super::lines(&HashMap::new(), Default::default())
            .new
            .iter()
            .all(|(_, i)| *i == RED)
    );
}

#[test]
fn scrubs_are_placed_where_the_table_says() {
    let Some(romfs) = dev_dump() else { return };
    let placed = crate::checks::banc_files(&romfs).unwrap();
    let mut found = std::collections::HashSet::new();
    for rel in placed {
        let doc = crate::checks::load_banc(&romfs, &rel).unwrap();
        let fields: Vec<String> = SCRUBS
            .iter()
            .flat_map(|s| s.placements.iter().map(|h| format!("{h}.Dynamic.{TALK}=x")))
            .collect();
        let mut root = doc.root;
        found.extend(crate::checks::set_fields(&mut root, &fields).unwrap());
    }
    for s in &SCRUBS {
        for h in s.placements {
            assert!(found.contains(h), "{} {} {h}", s.area, s.name);
        }
    }
}

/// Every talk a seed points a scrub at has its event rows, or the game
/// starts nothing when Link talks to it (found live on 2026-11-13).
#[test]
fn every_talk_a_scrub_runs_has_its_event_rows() {
    let Some(romfs) = dev_dump() else { return };
    let drops = drops(&[
        (HF_BOMBS, item::BOOMERANG),
        (LW_STICKS, SONG),
        (HF_HEART, MEDAL),
    ]);
    let talks: Vec<String> = fields(&drops)
        .iter()
        .filter_map(|f| f.split_once(&format!(".Dynamic.{TALK}=")))
        .map(|(_, t)| t.to_owned())
        .collect();
    assert!(
        talks.contains(&sells(&SCRUBS[HF_BOMBS], item::BOOMERANG)),
        "{talks:?}"
    );
    assert!(
        talks.contains(&sells(&SCRUBS[LW_STICKS], SONG)),
        "{talks:?}"
    );
    let mut tables = crate::rsdb::Events::load(&romfs).unwrap();
    crate::scrubs::events(&drops, &mut tables).unwrap();
    for t in &talks {
        let setting = crate::standins::row(&tables.setting, t).unwrap();
        let entry = setting
            .get(field::ENTRY_POINT_NAME.get())
            .unwrap()
            .as_string()
            .unwrap();
        assert_eq!(entry.as_str(), t);
        crate::standins::row(&tables.analyzed, t).unwrap();
    }
}

/// Sales are picked by item, so two scrubs selling one item share a sale.
/// Its price is the setting's, never the vanilla price of whichever spot came
/// first (the Death Mountain heart scrub asked 100, then took 100 for a
/// Blue Rupee in a Hyrule Field grotto that said 40).
#[test]
fn every_sale_asks_the_set_price() {
    let Some((_, after)) = written(&[(DMC_HEART, item::BOOMERANG), (HF_BOMBS, item::BOOMERANG)])
    else {
        return;
    };
    let prices: Vec<String> = calls(&after, ONE_TIME)
        .into_iter()
        .filter_map(|e| match &after.events[e as usize].kind {
            EventKind::SubFlow { params, .. } => Some(format!(
                "{:?}",
                params.iter().find(|(k, _)| k == PRICE_PARAM)?.1
            )),
            _ => None,
        })
        .collect();
    assert!(prices.contains(&"Int([25])".to_owned()), "{prices:?}");
    assert_eq!(
        branch(&after, item::BOOMERANG).unwrap(),
        format!("{} Int([25])", item::BOOMERANG)
    );
}

/// The Bool a root call `call` sets once Link pays.
fn sold_flag(flow: &Flowchart, call: u16) -> String {
    let ask = next(flow, call).expect("nothing asks whether Link paid");
    let paid = events(flow, IS_BOOL).into_iter().any(|(e, _)| e == ask);
    assert!(paid, "{ask} isn't a Bool check");
    assert_eq!(
        value(flow, ask, GAME_DATA).and_then(Value::as_str),
        Some(BOUGHT.get())
    );
    let set = case(flow, ask, 0).unwrap();
    assert_eq!(next(flow, set), None);
    let flag = value(flow, set, GAME_DATA).and_then(Value::as_str);
    flag.unwrap().to_owned()
}

#[test]
fn every_edited_spot_sets_its_own_bool_once_paid() {
    let at: Vec<_> = (0..SCRUBS.len())
        .filter(|&s| SCRUBS[s].branch != Branch::Secret)
        .map(|s| (s, [item::BOOMERANG, SONG][s % 2]))
        .collect();
    let Some((_, after)) = written(&at) else {
        return;
    };
    let drops = drops(&at);
    let mut flags = flags(&drops);
    for &(s, item) in &at {
        let spot = &SCRUBS[s];
        let main = entry_of(&after, &sells(spot, item)).main.unwrap();
        let call = match plain(item) {
            true => next(&after, main).unwrap(),
            false => main,
        };
        assert_eq!(sold_flag(&after, call), spot.flag(), "{}", spot.name);
    }
    // One per spot, and never the Hyrule Field riddle scrub's.
    let all = flags.len();
    flags.sort();
    flags.dedup();
    assert_eq!(flags.len(), all);
    assert_eq!(all, SCRUBS.len() - 1);
    let placed: usize = at.iter().map(|(s, _)| SCRUBS[*s].placements.len()).sum();
    assert_eq!(gone(&drops).len(), placed);
}

#[test]
fn a_scrub_dealt_the_hookshot_offers_and_sells_the_step_link_owns() {
    needs_names!();
    let Some((_, after)) = written(&[(KOKIRI, item::HOOKSHOT)]) else {
        return;
    };
    // Highest first: the Longshot, then the Hookshot.
    let ask = sale(&after, item::HOOKSHOT);
    assert!(asks_for(&after, ask, item::LONGSHOT));
    let asked = flows::reachable_from(&after, Some(ask));
    assert!(asked.iter().any(|&e| asks_for(&after, e, item::HOOKSHOT)));
    let both = [item::HOOKSHOT, item::LONGSHOT].map(|i| (i.to_string(), seller(i)));
    assert_eq!(sales_from(&after, ask), both);
    let dealt = drops(&[(KOKIRI, item::HOOKSHOT)]);
    let lines = lines(&dealt, Default::default());
    for i in [item::HOOKSHOT, item::LONGSHOT] {
        assert!(lines.new.iter().any(|(_, l)| *l == i.get()), "{i}");
    }
}

#[test]
fn a_scrub_dealt_arrows_offers_a_red_rupee_without_the_bow() {
    needs_names!();
    let arrows = actor::T_BOX_ARROW_BUNDLE_SMALL;
    let Some((_, after)) = written(&[(KOKIRI, arrows)]) else {
        return;
    };
    let ask = sale(&after, arrows);
    let sales = sales_from(&after, ask);
    let named = |i: &str| (i.to_string(), seller(i));
    assert!(sales.contains(&named(crate::ammo::WITHOUT)), "{sales:?}");
    assert!(sales.contains(&named(arrows)), "{sales:?}");
    let asked = flows::reachable_from(&after, Some(ask));
    let bow = actor::WEAPON_RANGE_BOW;
    assert!(asked.iter().any(|&e| asks_for(&after, e, bow)));
}

#[test]
fn every_scrub_spot_takes_the_leaves_it_stands_in() {
    let Some(romfs) = dev_dump() else { return };
    let gone: Vec<_> = SCRUBS
        .iter()
        .flat_map(|s| s.placements.iter().map(|h| (*h, s.flag())))
        .collect();
    let mut leaves = Vec::new();
    for (_, doc) in crate::checks::bancs(&romfs).unwrap() {
        leaves.extend(super::leaves(&doc.unwrap().root, &gone).unwrap());
    }
    for s in &SCRUBS {
        let n = leaves.iter().filter(|(_, f)| *f == s.flag()).count();
        assert!(n >= s.placements.len(), "{} stands in {n} leaves", s.name);
    }
}

#[test]
fn the_riddle_scrub_as_an_adult_names_what_it_was_dealt() {
    let says = |flow: &Flowchart, label: &str| {
        let id = format!("{}:{label}", *TEXT);
        let talks = flow.events.iter().filter(|e| match &e.kind {
            EventKind::Action { params, .. } => param(params, field::MESSAGE_ID) == Some(&id),
            _ => false,
        });
        talks.count()
    };
    let (Some(hookshot), Some(heart)) = (riddle(item::HOOKSHOT), riddle(HEART)) else {
        return;
    };
    for like in SOLD_OUT {
        let named = super::placements::naming(like, item::HOOKSHOT);
        assert!(
            says(&hookshot, &named) > 0 && says(&hookshot, like) == 0,
            "{like}"
        );
        assert!(says(&heart, like) > 0, "{like}");
    }
}

#[test]
fn a_spot_is_sold_by_the_flag_its_seed_registers() {
    let other = actor::OBJ_HEART_PIECE.to_string();
    for (i, s) in SCRUBS.iter().enumerate() {
        let item = match s.item == other.as_str() {
            true => item::STICK_BAG_MEDIUM.to_string(),
            false => other.clone(),
        };
        let registered = flags(&drops(&[(i, &item)]));
        let sold = s.sold(&item).unwrap();
        let own = s.branch != Branch::Secret;
        assert_eq!(registered.contains(&sold), own, "{}", s.name);
    }
}

#[test]
fn an_unedited_spot_keeps_its_vanilla_sold_flag() {
    needs_names!();
    for s in SCRUBS.iter().filter(|s| s.branch != Branch::Shop) {
        let vanilla = s.sold.map(|f| f.to_string());
        assert!(vanilla.is_some(), "{}", s.name);
        assert_eq!(s.sold(s.item.as_ref()), vanilla, "{}", s.name);
    }
    let hf = &SCRUBS[HF_HEART];
    let other = item::STICK_BAG_MEDIUM.to_string();
    assert_eq!(
        hf.sold(&other),
        Some(flag::HYRULE_FIELD_ENEMY_SCRUB_BUSINESS_HEART_PIECE_01.to_string())
    );
    assert!(
        SCRUBS
            .iter()
            .filter(|s| s.branch == Branch::Shop)
            .all(|s| s.sold.is_none())
    );
}
