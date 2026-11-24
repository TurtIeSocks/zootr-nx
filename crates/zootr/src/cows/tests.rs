use std::collections::{BTreeSet, HashMap};

use ounce_formats::{
    Byml, BymlDoc, BymlMap, EventKind, Flowchart, ModBuilder, Params, Value, pack_path,
};
use roead::sarc::Sarc;

use super::flow::{ARG_FLAG, ARG_ITEM, MILK, melodies, target};
use super::*;
use crate::checks::Drop;
use crate::flows::{GAME_DATA, IS_BOOL, Rewrites, SET_BOOL, param, reachable};
use crate::game::dev_dump;
use crate::gifts::levels;
use crate::known::{actor, field, flag, item};
use crate::rsdb;
use crate::standins::{Tables, actor_path, row};

fn at(actor: &str, file: &str, hash: u64, pos: [f32; 3]) -> Placement {
    Placement {
        actor: actor.into(),
        file: format!("Banc/{file}.bcett.byml.zs"),
        hash,
        pos,
    }
}

#[test]
fn the_check_is_the_lowest_child_placement_else_the_lowest() {
    needs_names!();
    let cows = group(vec![
        at(PLAIN, "Kak/FakeKak_Adult_Dynamic", 1, [0.0, 0.0, 0.0]),
        at(PLAIN, "Kak/FakeKak_Child_Dynamic", 9, [0.3, 5.0, 0.3]),
        at(PLAIN, "Kak/FakeKak_Child_Static", 5, [0.0, 0.0, 0.5]),
        at(
            "Animal_Cow_FakeNamed",
            "Fort/FakeFort_Adult_Static",
            7,
            [0.0, 0.0, 0.0],
        ),
    ]);
    assert_eq!(cows.len(), 2);
    let kak = cows.iter().find(|c| c.plain()).unwrap();
    assert_eq!(kak.placements.len(), 3);
    assert_eq!(kak.hash(), 5, "the lowest child placement, not the lowest");
    let fort = cows.iter().find(|c| !c.plain()).unwrap();
    assert_eq!(fort.hash(), 7, "no child placement: its lowest");
    // Numbered by check hash.
    assert_eq!(
        cows.iter().map(|c| (c.index, c.hash())).collect::<Vec<_>>(),
        [(0, 5), (1, 7)]
    );
}

#[test]
fn only_the_same_actor_in_one_area_within_a_unit_is_one_cow() {
    let cows = group(vec![
        at(PLAIN, "Field/FakeField_Child_Static", 1, [0.0, 0.0, 0.0]),
        at(PLAIN, "Field/FakeField_Adult_Dynamic", 2, [0.4, 0.0, 0.0]),
        at(PLAIN, "Field/FakeField_Child_Static", 3, [0.0, 0.0, 3.4]),
        at(PLAIN, "Other/FakeOther_Child_Static", 4, [0.0, 0.0, 0.0]),
        at(
            "Animal_Cow_FakeNamed",
            "Field/FakeField_Child_Static",
            5,
            [0.0, 0.0, 0.0],
        ),
        // The same placement again, as a second file can repeat it.
        at(PLAIN, "Field/FakeField_Child_Dynamic", 1, [0.0, 0.0, 0.0]),
    ]);
    let sizes: Vec<usize> = cows.iter().map(|c| c.placements.len()).collect();
    assert_eq!(sizes, [2, 1, 1, 1]);
}

#[test]
fn a_cow_holder_is_told_from_others() {
    assert!(is_holder("Cow"));
    assert!(is_holder("Grotto Cow"));
    assert!(is_holder("Lon Lon Ranch Cow 2"));
    assert!(!is_holder("Cowboy 1"));
    assert!(!is_holder("Cow Bell"));
    assert!(!is_holder("Business Scrub, sticks"));
}

#[test]
fn every_cow_has_a_name_its_area_tells_apart() {
    let Some((_, cows)) = cows() else { return };
    let mut seen = BTreeSet::new();
    for cow in &cows {
        let holder = cow.holder();
        assert!(is_holder(&holder), "{holder}");
        assert!(
            seen.insert((cow.area().to_owned(), holder.clone())),
            "{holder} twice in {}",
            cow.area()
        );
    }
    let named = |hash| cows.iter().any(|c| c.hash() == hash);
    assert!(
        NAMES.iter().all(|(hash, _)| named(*hash)),
        "a name for a cow the game doesn't place"
    );
}

/// The game's cows, or `None` without a dump.
fn cows() -> Option<(Romfs, Vec<Cow>)> {
    let romfs = dev_dump()?;
    let cows = find(&romfs).unwrap();
    Some((romfs, cows))
}

#[test]
fn eight_cows_five_plain_and_kakarikos_in_three_files() {
    let Some((_, cows)) = cows() else { return };
    assert_eq!(cows.len(), 8);
    assert_eq!(cows.iter().map(|c| c.placements.len()).sum::<usize>(), 14);
    let plain: Vec<&Cow> = cows.iter().filter(|c| c.plain()).collect();
    assert_eq!(plain.len(), 5);
    let kakariko: Vec<&Cow> = cows
        .iter()
        .filter(|c| c.area() == flag::KAKARIKO_VILLAGE)
        .collect();
    let [kakariko] = kakariko[..] else {
        panic!("{} cows in Kakariko", kakariko.len())
    };
    assert_eq!(kakariko.placements.len(), 3);
    assert!(is_child(&kakariko.check().file));
    // Each cow runs its own event.
    let events: BTreeSet<&str> = cows.iter().map(|c| c.event.as_str()).collect();
    assert_eq!(events.len(), 8);
    for c in &plain {
        assert_eq!(c.event, format!("GiveMilk_Rando_{:02}", c.index));
    }
    let checks = checks(&cows);
    assert!(checks.iter().all(is_cow));
    let hashes: BTreeSet<u64> = checks.iter().map(|c| c.hash).collect();
    assert_eq!(hashes.len(), 8);
}

#[test]
fn each_cows_flag_is_known_from_its_check_hash() {
    let Some((_, cows)) = cows() else { return };
    let hashes: Vec<u64> = cows.iter().map(Cow::hash).collect();
    assert_eq!(hashes, CHECKS);
    for c in &cows {
        assert_eq!(flag_at(c.hash()), Some(c.flag()));
    }
    assert_eq!(flag_at(1), None);
}

#[test]
fn every_cow_has_a_logic_rule_needing_eponas_song() {
    let Some((_, cows)) = cows() else { return };
    let book = crate::rules::Book::load().unwrap();
    for cow in &cows {
        let rule = book.check(cow.hash());
        let rule = rule.unwrap_or_else(|| panic!("no rule for cow {}", cow.hash()));
        assert!(rule.optional, "cow {}", cow.hash());
        assert!(format!("{:?}", rule.needs).contains("eponas-song"));
    }
}

fn parse(data: &[u8]) -> Flowchart {
    Flowchart::parse(data).unwrap().unwrap()
}

fn drops(cows: &[Cow]) -> HashMap<u64, Drop> {
    cows.iter()
        .map(|c| (c.hash(), Drop::from(format!("Item_Test_{:02}", c.index))))
        .collect()
}

fn argument(name: &str) -> Value {
    Value::Argument(name.into())
}

fn value<'a>(params: &'a Params, key: &str) -> Option<&'a Value> {
    params.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The actor and the action or query an event runs.
fn runs(flow: &Flowchart, e: u16) -> (&str, &str) {
    match &flow.events[e as usize].kind {
        EventKind::Action { actor, action, .. } => {
            let a = flow.actor_of(*actor);
            (&a.name, &a.actions[*action as usize])
        }
        EventKind::Switch { actor, query, .. } => {
            let a = flow.actor_of(*actor);
            (&a.name, &a.queries[*query as usize])
        }
        other => panic!("event {e} runs nothing: {other:?}"),
    }
}

#[test]
fn the_milk_entries_are_the_ones_asking_the_melody() {
    let Some(romfs) = dev_dump() else { return };
    let flow = flows::load(&romfs, FLOW).unwrap().unwrap();
    let asks = melodies(&flow);
    assert_eq!(asks.len(), 2);
    for at in asks {
        let by = MILK
            .iter()
            .filter(|m| reachable(&flow, m).unwrap().contains(&at));
        assert_eq!(by.count(), 1, "event {at}");
    }
}

#[test]
fn after_the_right_melody_the_milk_asks_the_bool_its_caller_passes() {
    let Some((romfs, cows)) = cows() else { return };
    let before = flows::load(&romfs, FLOW).unwrap().unwrap();
    let mut rewrites = Rewrites::new();
    give_all(&romfs, &mut rewrites, &cows, &drops(&cows), false);
    let flow = parse(&rewrites[&flows::rel(FLOW)]);
    assert_eq!(parse(&flow.to_bytes()), flow);
    let cases = |flow: &Flowchart, at: u16| match &flow.events[at as usize].kind {
        EventKind::Switch { cases, params, .. } => (cases.clone(), params.clone()),
        other => panic!("event {at} isn't a question: {other:?}"),
    };
    let next = |e: u16| match &flow.events[e as usize].kind {
        EventKind::Action { next, params, .. } => (*next, params.clone()),
        other => panic!("event {e} isn't an action: {other:?}"),
    };
    for at in melodies(&before) {
        let (vanilla, _) = cases(&before, at);
        let right = vanilla.iter().find(|c| c.0 == 0).unwrap().1;
        let (now, _) = cases(&flow, at);
        let [(0, gate), (1, wrong)] = now[..] else {
            panic!("{now:?}")
        };
        assert_eq!(vanilla.iter().find(|c| c.0 == 1).unwrap().1, wrong);
        assert_eq!(runs(&flow, gate), flows::said(IS_BOOL));
        let (asked, params) = cases(&flow, gate);
        assert_eq!(value(&params, GAME_DATA), Some(&argument(ARG_FLAG)));
        // Set: the milk as before.
        let [(0, set), (1, unset)] = asked[..] else {
            panic!("{asked:?}")
        };
        assert_eq!(set, right);
        // Unset: set it, then grant the item, then the get window, then the end.
        assert_eq!(runs(&flow, unset), flows::said(SET_BOOL));
        let (grant, params) = next(unset);
        assert_eq!(value(&params, GAME_DATA), Some(&argument(ARG_FLAG)));
        assert!(params.contains(&(field::VALUE.get().into(), Value::Bool(vec![true]))));
        let grant = grant.unwrap();
        assert_eq!(runs(&flow, grant), flows::said(flows::GRANT));
        let (mut line, params) = next(grant);
        assert_eq!(value(&params, flows::ACTOR), Some(&argument(ARG_ITEM)));
        for id in crate::grant::GET_LINES.iter() {
            let (after, params) = next(line.unwrap());
            assert_eq!(param(&params, field::MESSAGE_ID), Some(id.as_str()));
            line = after;
        }
        assert_eq!(line, None);
    }
}

#[test]
fn every_cow_passes_its_own_bool_and_item_to_its_milk() {
    let Some((romfs, cows)) = cows() else { return };
    let mut rewrites = Rewrites::new();
    give_all(&romfs, &mut rewrites, &cows, &drops(&cows), false);
    let events = BymlDoc::parse(&romfs.read(&rsdb::path(rsdb::EVENTS)).unwrap()).unwrap();
    let (plain_flow, _) = target(&events, actor::ANIMAL_COW_GIVE_MILK).unwrap();
    let mut flags = BTreeSet::new();
    for cow in &cows {
        let (name, entry) = match cow.plain() {
            true => (plain_flow.clone(), cow.event.clone()),
            false => target(&events, &cow.event).unwrap(),
        };
        let flow = parse(&rewrites[&flows::rel(&name)]);
        let calls = milk_calls(&flow, &entry);
        assert!(!calls.is_empty(), "{entry} calls no milk");
        let item = format!("Item_Test_{:02}", cow.index);
        for params in calls {
            assert_eq!(
                param(params, ARG_FLAG),
                Some(cow.flag().as_str()),
                "{entry}"
            );
            assert_eq!(param(params, ARG_ITEM), Some(item.as_str()), "{entry}");
            if cow.plain() {
                // Its clone stands in for the plain cow.
                let clone = cow.clone_name();
                assert_eq!(param(params, "Cow"), Some(clone.as_str()));
                let actor = Value::ActorId {
                    name: clone.clone(),
                    secondary: String::new(),
                };
                assert_eq!(value(params, flows::ACTOR), Some(&actor));
                assert!(flow.actors.iter().any(|a| a.name == clone));
            }
        }
        assert!(flags.insert(cow.flag()), "{} shared", cow.flag());
    }
    assert_eq!(flags.len(), 8);
}

/// The params of every milk call `entry` of `flow` reaches.
fn milk_calls<'a>(flow: &'a Flowchart, entry: &str) -> Vec<&'a Params> {
    (reachable(flow, entry).unwrap().into_iter())
        .filter_map(|e| match &flow.events[e as usize].kind {
            EventKind::SubFlow {
                entry_point,
                params,
                ..
            } if MILK.iter().any(|m| entry_point == m) => Some(params),
            _ => None,
        })
        .collect()
}

/// [`give`] with the pouch question, progressive items on or off.
fn give_all(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    cows: &[Cow],
    drops: &HashMap<u64, Drop>,
    progressive: bool,
) {
    let model = levels::model(&crate::events::Flows::load(romfs).unwrap()).unwrap();
    let level = levels::Level {
        model: &model,
        lines: crate::progressive::Lines {
            progressive,
            ..Default::default()
        },
    };
    give(romfs, rewrites, cows, drops, level).unwrap();
}

/// Every cow dealt `dealt` passes each of `passes` to its milk, by what
/// Link owns.
fn every_cow_dealt_passes(dealt: &str, progressive: bool, passes: &[&str]) {
    let Some((romfs, cows)) = cows() else { return };
    let dealt = |c: &Cow| (c.hash(), Drop::from(dealt.to_string()));
    let drops: HashMap<u64, Drop> = cows.iter().map(dealt).collect();
    let mut rewrites = Rewrites::new();
    give_all(&romfs, &mut rewrites, &cows, &drops, progressive);
    let events = BymlDoc::parse(&romfs.read(&rsdb::path(rsdb::EVENTS)).unwrap()).unwrap();
    let (plain_flow, _) = target(&events, actor::ANIMAL_COW_GIVE_MILK).unwrap();
    for cow in &cows {
        let (name, entry) = match cow.plain() {
            true => (plain_flow.clone(), cow.event.clone()),
            false => target(&events, &cow.event).unwrap(),
        };
        let flow = parse(&rewrites[&flows::rel(&name)]);
        assert_eq!(parse(&flow.to_bytes()), flow);
        let passed: BTreeSet<_> = (milk_calls(&flow, &entry).into_iter())
            .inspect(|p| assert_eq!(param(p, ARG_FLAG), Some(cow.flag().as_str())))
            .map(|p| param(p, ARG_ITEM).unwrap().to_string())
            .collect();
        let passes = passes.iter().map(|i| i.to_string()).collect();
        assert_eq!(passed, passes, "{entry}");
    }
}

#[test]
fn a_cow_dealt_a_line_item_passes_the_level_link_needs() {
    every_cow_dealt_passes(item::HOOKSHOT, true, &[item::HOOKSHOT, item::LONGSHOT]);
}

#[test]
fn a_cow_dealt_arrows_passes_a_red_rupee_until_link_has_the_bow() {
    let arrows = actor::T_BOX_ARROW_BUNDLE_SMALL;
    every_cow_dealt_passes(arrows, false, &[crate::ammo::WITHOUT, arrows]);
}

#[test]
fn no_cows_change_no_flow() {
    let Some(romfs) = dev_dump() else { return };
    let mut rewrites = Rewrites::new();
    give_all(&romfs, &mut rewrites, &[], &HashMap::new(), false);
    assert!(rewrites.is_empty());
}

#[test]
fn every_placement_of_a_plain_cow_is_swapped_to_its_clone() {
    let Some((_, cows)) = cows() else { return };
    let fields = fields(&cows);
    let plain = cows.iter().filter(|c| c.plain());
    let swapped: usize = plain.clone().map(|c| c.placements.len()).sum();
    assert_eq!(fields.len(), swapped);
    for cow in plain {
        for p in &cow.placements {
            let want = format!("{}.Gyaml={}", p.hash, cow.clone_name());
            assert!(fields.contains(&want), "{want}");
        }
    }
    assert_eq!(flags(&cows).len(), 8);
}

#[test]
fn each_plain_cow_gets_a_pack_rows_and_an_event_of_its_own() {
    let Some((romfs, cows)) = cows() else { return };
    let dir = std::env::temp_dir().join(format!("zootr-cows-{}", std::process::id()));
    let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
    let mut tables = Tables::load(&romfs).unwrap();
    let mut events = rsdb::Events::load(&romfs).unwrap();
    write(&romfs, &mut builder, &mut tables, &mut events, &cows).unwrap();
    tables.write(&mut builder).unwrap();
    events.write(&mut builder).unwrap();
    builder.finish().unwrap();
    let read = |rel: &str| {
        let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
        romfs.dicts().decompress(rel, &raw).unwrap()
    };
    let events = BymlDoc::parse(&read(&rsdb::path(rsdb::EVENTS))).unwrap();
    let template = row(&events, actor::ANIMAL_COW_GIVE_MILK).unwrap();
    let analyzed = BymlDoc::parse(&read(&rsdb::path(rsdb::ANALYZED))).unwrap();
    let mut info = row(&analyzed, actor::ANIMAL_COW_GIVE_MILK).unwrap();
    let mut names = BTreeSet::new();
    for cow in cows.iter().filter(|c| c.plain()) {
        let name = cow.clone_name();
        assert!(names.insert(name.clone()));
        row(&tables.actor, &name).unwrap();
        row(&tables.game, &name).unwrap();
        // Its event: the plain one's, running its own entry.
        let own = row(&events, &cow.event).unwrap();
        assert_eq!(
            own.get(field::ENTRY_POINT_NAME.get()),
            Some(&Byml::String(cow.event.as_str().into()))
        );
        assert_eq!(
            own.get(field::FLOWCHART_FILE_NAME.get()),
            template.get(field::FLOWCHART_FILE_NAME.get())
        );
        // The game knows it ahead, as it knows the plain one.
        info.insert("__RowId".into(), Byml::String(cow.event.as_str().into()));
        assert_eq!(row(&analyzed, &cow.event).unwrap(), info);
        // Its pack names that event through its own table.
        let rel = format!("Pack/Actor/{name}.pack.zs");
        let sarc = Sarc::new(read(&rel)).unwrap();
        let param = |path: &str| -> BymlMap {
            let data = sarc
                .get_data(path)
                .unwrap_or_else(|| panic!("{name} has no {path}"));
            Byml::from_binary(data).unwrap().as_map().unwrap().clone()
        };
        let actor = param(&actor_path(&name));
        let table = pack_path(&component(&actor, TABLE).unwrap());
        assert!(table.contains(&name), "{table}");
        let replace = pack_path(&component(&param(&table), REPLACE).unwrap());
        assert!(replace.contains(&name), "{replace}");
        let event = param(&replace)[&**EVENT_NAME]
            .as_string()
            .unwrap()
            .to_string();
        assert!(event.contains(&format!("/{}.", cow.event)), "{event}");
        // And keeps the plain actor beside it.
        param(&actor_path(PLAIN));
    }
    assert_eq!(names.len(), 5);
    std::fs::remove_dir_all(dir).unwrap();
}
