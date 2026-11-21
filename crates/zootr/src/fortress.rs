//! The four keys the Gerudo thieves carry, as checks. Each is the placed
//! key the carpenter flow moves to its thief once beaten (`pickups`
//! swaps its actor like any holder's). The three thieves that also carry
//! their key as a drop lose it, so the one key a thief gives is the seed's.
//!
//! Her call warps the key to her as she falls, but only the key's own
//! pickup graph can warp: a stand-in left at the key's placement, under
//! the floor, never shows. So each moved key lies at the spot her call
//! names instead, on the floor beside her, and shows as she falls.
//!
//! The flow names the key actor by its name, so each call is pointed at
//! the actor now placed. A key showing a line a step at a time is one
//! placement per step, so the call is copied per step, each behind its
//! step's gate. A key that was swapped never sets the Bool the carpenters
//! read, so the flow sets it after the thief's call.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{EventKind, Flowchart, Value};

use crate::checks::{Drop, ItemCheck, TRANSLATE};
use crate::flows::{self, GAME_DATA, IS_BOOL, Rewrites, SET_BOOL, running, string};
use crate::known::{Name, field, flow};
use crate::pickups::steps;
use crate::{keys, pickups, shows};

const FLOW: &Name = flow::SAGE_OF_SPIRIT_RESCUE_CARPENTER;
/// The thieves that carry their key, by placement hash. The third carries
/// none: its key is only the one the flow moves.
pub const CARRYING: [u64; 3] = [
    3350876166368957251,
    3233425834948791513,
    13730570727502202323,
];
/// The call's argument naming the key's actor.
const KEY: &Name = flow::OBJ_SMALL_KEY;
/// The call's argument naming the spot on the floor beside her.
const SPOT: &Name = flow::KEY_POSITION;
/// The query reading a BoolExp. Case 0 is true.
const IS_EXP: &Name = flow::EVENT_QUERY_IS_GAME_DATA_BOOL_EXP;

/// Whether `check` is a thief's own drop, which a seed with Fortress keys
/// clears and checks the placed key for instead.
pub fn is_carried(check: &ItemCheck) -> bool {
    CARRYING.contains(&check.hash) && keys::is_key(&check.item)
}

/// Each thief's call, by the number of the key it carries (01 first).
const THIEVES: [&Name; 4] = [
    flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF01,
    flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF02,
    flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF03,
    flow::GERUDO_FORTRESS_DESTROY_GERUDO_THIEF04,
];

/// The Bool the seed sets once the thief carrying key `n` (01 first) gives
/// another item. The key's own Bool can't say so: the key, placed
/// elsewhere, sets it there.
fn gave(n: &str) -> String {
    format!("Rando_Thief_{n}")
}

/// The Bools a new game with Fortress keys adds, one per thief.
pub fn flags() -> Vec<String> {
    (1..=THIEVES.len())
        .map(|i| gave(&format!("{i:02}")))
        .collect()
}

/// The Bool set once the thief whose key is `vanilla` gives `item`: the
/// key's own while it holds its key, else the seed's.
pub fn got(vanilla: &str, item: &str) -> Option<String> {
    let (_, key) = thief_of(vanilla).ok()?;
    let n = vanilla.strip_prefix(keys::FORTRESS_KEY)?;
    Some(if item == vanilla { key } else { gave(n) })
}

/// The entry that runs the thief's call, and the Bool the key sets, for
/// the key actor `key`.
fn thief_of(key: &str) -> Result<(String, String)> {
    let n = key.strip_prefix(keys::FORTRESS_KEY);
    let n = n.with_context(|| format!("{key} isn't a Fortress key"))?;
    let thief = n
        .parse::<usize>()
        .ok()
        .and_then(|i| THIEVES.get(i.checked_sub(1)?));
    let thief = thief.with_context(|| format!("{key} isn't a thief's key"))?;
    Ok((
        thief.to_string(),
        format!(
            "{}.{}{n}",
            field::IS_GET_SMALL_KEY_GERUDO_FORTRESS,
            field::SMALL_KEY
        ),
    ))
}

/// Point each moved key's call at the actor now placed there, one call
/// per step when it shows a line a step at a time, and set the key's Bool
/// after it, for the Fortress keys of `pieces` that `drops` changes.
pub fn give(
    romfs: &ounce_formats::Romfs,
    rewrites: &mut Rewrites,
    pieces: &[ItemCheck],
    drops: &HashMap<u64, Drop>,
    progressive: crate::progressive::Lines,
) -> Result<()> {
    // Each key's steps, holder's own first.
    let mut moved: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    for m in pickups::moved(pieces, drops, progressive) {
        if m.vanilla.starts_with(keys::FORTRESS_KEY) {
            moved.entry(m.vanilla).or_default().push(m);
        }
    }
    if moved.is_empty() {
        return Ok(());
    }
    flows::edit(romfs, rewrites, FLOW, |flow| {
        let sets = *running(flow, SET_BOOL)
            .first()
            .with_context(|| format!("{FLOW} sets no Bool"))?;
        let set_bool = flows::model(flow, SET_BOOL, sets)?;
        let asks = flows::switch(flow, IS_BOOL).with_context(|| format!("{FLOW} asks no Bool"))?;
        let is_bool = flows::model(flow, IS_BOOL, asks)?;
        for (vanilla, placed) in &moved {
            let (entry, flag) = thief_of(vanilla)?;
            let call = flow.entry_points.iter().find(|e| e.name == entry);
            let call = call.and_then(|e| e.main);
            let call = call.with_context(|| format!("{FLOW} has no {entry}"))?;
            ensure!(
                matches!(flow.events[call as usize].kind, EventKind::SubFlow { .. }),
                "{entry} doesn't start with a call"
            );
            let into = flow.links_to(call);
            rename(flow, call, &placed[0].actor, 0)?;
            let n = vanilla.strip_prefix(keys::FORTRESS_KEY).unwrap_or_default();
            // Each lands right after the call, so the key's Bool runs first.
            for flag in [gave(n), flag] {
                flows::set_after(flow, &set_bool, call, &flag, Value::Bool(vec![true]))?;
            }
            // Each step past the holder's own calls with its key, joining
            // the holder's call at its Bools.
            let mut head = call;
            for m in &placed[1..] {
                let copy = flow.push(flow.events[call as usize].kind.clone());
                let k = m.step.as_ref().map_or(0, |s| s.k);
                rename(flow, copy, &m.actor, k)?;
                let mut params = is_bool.params.clone();
                flows::set(&mut params, GAME_DATA, string(&steps::gate(m.hash)))?;
                let (actor, query) = flow.query(&is_bool.actor, IS_EXP);
                head = flow.push(EventKind::Switch {
                    actor,
                    query,
                    params,
                    cases: vec![(0, copy), (1, head)],
                });
            }
            for link in into {
                flow.link(link, Some(head))?;
            }
        }
        Ok(())
    })
}

/// The placement fields that lay each moved key, one placement per step,
/// at the spot her call names, for the Fortress keys of `pieces` that
/// `drops` changes (Mac, 2026-11-19).
pub fn fields(
    romfs: &ounce_formats::Romfs,
    pieces: &[ItemCheck],
    drops: &HashMap<u64, Drop>,
    progressive: crate::progressive::Lines,
) -> Result<Vec<String>> {
    let flow = flows::load(romfs, FLOW)?.with_context(|| format!("{FLOW} isn't a flow"))?;
    let mut fields = Vec::new();
    for m in pickups::moved(pieces, drops, progressive) {
        if !m.vanilla.starts_with(keys::FORTRESS_KEY) {
            continue;
        }
        let spot = spot(&flow, m.vanilla)?;
        let at = spot.iter().enumerate();
        fields.extend(at.map(|(i, v)| format!("{}.{TRANSLATE}.{i}={v}", m.hash)));
    }
    Ok(fields)
}

/// The spot her call names for the key actor `key`.
fn spot(flow: &Flowchart, key: &str) -> Result<[f32; 3]> {
    let (entry, _) = thief_of(key)?;
    let call = flow.entry_points.iter().find(|e| e.name == entry);
    let call = call.and_then(|e| e.main);
    let call = call.with_context(|| format!("{FLOW} has no {entry}"))?;
    let EventKind::SubFlow { params, .. } = &flow.events[call as usize].kind else {
        bail!("{entry} doesn't start with a call");
    };
    match params.iter().find(|(k, _)| k == SPOT) {
        Some((_, Value::Float(v))) => <[f32; 3]>::try_from(v.as_slice())
            .with_context(|| format!("{entry}'s {SPOT} isn't a point")),
        _ => bail!("{entry}'s call names no {SPOT}"),
    }
}

/// Name `actor` in the key argument of the call at `at`, by step `k`'s
/// instance name (`shows::instance`).
fn rename(flow: &mut Flowchart, at: u16, actor: &str, k: usize) -> Result<()> {
    flows::rename_actor(flow, at, KEY, actor, |name| shows::instance(name, k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::scan;
    use crate::game::dev_dump;
    use crate::known::{actor, field, flag, item};

    #[test]
    fn four_keys_are_checks_and_three_thieves_carry_one() {
        let Some(romfs) = dev_dump() else { return };
        let found = pickups::FORTRESS_KEYS.checks(&romfs).unwrap();
        assert_eq!(found.len(), 4);
        for c in &found {
            assert_eq!(c.holder, pickups::THIEF);
            assert_eq!(c.area, flag::GERUDO_FORTRESS);
            assert!(c.presence.is_none() && keys::is_key(&c.item));
            assert!(pickups::is_pickup(c));
        }
        let mut items: Vec<_> = found.iter().map(|c| c.item.as_str()).collect();
        items.sort();
        items.dedup();
        assert_eq!(items.len(), 4);
        let carried: Vec<_> = scan(&romfs)
            .unwrap()
            .into_iter()
            .filter(is_carried)
            .collect();
        let mut hashes: Vec<_> = carried.iter().map(|c| c.hash).collect();
        hashes.sort();
        let mut all = CARRYING;
        all.sort();
        assert_eq!(hashes, all);
        assert!(
            carried
                .iter()
                .all(|c| c.holder.contains(&**flag::GERUDO_THIEF))
        );
    }

    #[test]
    fn a_moved_key_is_called_by_its_new_actor_and_sets_its_bool() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::FORTRESS_KEYS.checks(&romfs).unwrap();
        let key = pieces.iter().find(|c| c.item.ends_with("_01")).unwrap();
        let drops = HashMap::from([(key.hash, Drop::from(keys::INSTEAD.to_string()))]);
        let mut rewrites = Rewrites::new();
        give(&romfs, &mut rewrites, &pieces, &drops, Default::default()).unwrap();
        let actor = pickups::moved(&pieces, &drops, Default::default())
            .remove(0)
            .actor;
        let flow = Flowchart::parse(&rewrites[&flows::rel(FLOW)])
            .unwrap()
            .unwrap();
        let entry = |thief: &Name| {
            let main = flow.entry_points.iter().find(|e| e.name == thief.get());
            &flow.events[main.unwrap().main.unwrap() as usize].kind
        };
        let EventKind::SubFlow { params, next, .. } = entry(THIEVES[0]) else {
            panic!("a call")
        };
        let Some((_, Value::ActorId { name, secondary })) = params.iter().find(|p| p.0 == KEY)
        else {
            panic!("an actor")
        };
        assert_eq!(name.as_str(), actor.as_str());
        assert_eq!(secondary, field::SMALL_KEY_FOR_EVENT_01);
        let set = |at: Option<u16>| {
            let EventKind::Action { params, next, .. } = &flow.events[at.unwrap() as usize].kind
            else {
                panic!("a Bool")
            };
            (
                flows::param(params, flows::GAME_DATA).unwrap().to_owned(),
                *next,
            )
        };
        let (key, next) = set(*next);
        assert_eq!(
            key,
            flag::IS_GET_SMALL_KEY_GERUDO_FORTRESS_SMALL_KEY01.get()
        );
        let (thief, next) = set(next);
        assert_eq!(Some(thief), got(&key_actor(1), &actor));
        assert!(next.is_none());
        let vanilla = flows::load(&romfs, FLOW).unwrap().unwrap();
        let sets = |f: &Flowchart| flows::running(f, SET_BOOL).len();
        assert_eq!(sets(&flow), sets(&vanilla) + 2);
        let EventKind::SubFlow { params, next, .. } = entry(THIEVES[1]) else {
            panic!("a call")
        };
        assert!(next.is_none());
        let Some((_, Value::ActorId { name, .. })) = params.iter().find(|p| p.0 == KEY) else {
            panic!("an actor")
        };
        assert_eq!(name, actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_02);
        assert!(
            flows::undeclared(&flow).is_empty(),
            "{:?}",
            flows::undeclared(&flow)
        );
    }

    #[test]
    fn a_stepping_key_is_called_by_each_steps_actor_behind_its_gate() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::FORTRESS_KEYS.checks(&romfs).unwrap();
        let key = pieces.iter().find(|c| c.item.ends_with("_01")).unwrap();
        let drops = HashMap::from([(key.hash, Drop::from(item::HOOKSHOT.to_string()))]);
        let mut rewrites = Rewrites::new();
        give(
            &romfs,
            &mut rewrites,
            &pieces,
            &drops,
            crate::progressive::Lines::climbing(),
        )
        .unwrap();
        let placed = pickups::moved(&pieces, &drops, crate::progressive::Lines::climbing());
        assert_eq!(placed.len(), 2, "the Hookshot, then the Longshot");
        let flow = Flowchart::parse(&rewrites[&flows::rel(FLOW)])
            .unwrap()
            .unwrap();
        let main = flow
            .entry_points
            .iter()
            .find(|e| e.name == THIEVES[0].get());
        let at = |e: u16| &flow.events[e as usize].kind;
        let EventKind::Switch { params, cases, .. } = at(main.unwrap().main.unwrap()) else {
            panic!("a gate")
        };
        let gate = flows::param(params, flows::GAME_DATA).unwrap();
        assert_eq!(gate, steps::gate(placed[1].hash));
        let calls = |e: u16| {
            let EventKind::SubFlow { params, next, .. } = at(e) else {
                panic!("a call")
            };
            let Some((_, Value::ActorId { name, secondary })) = params.iter().find(|p| p.0 == KEY)
            else {
                panic!("an actor")
            };
            ((name.clone(), secondary.clone()), *next)
        };
        let ((shown, shown_as), joins) = calls(cases[0].1);
        let ((own, own_as), next) = calls(cases[1].1);
        assert_eq!(shown, placed[1].actor);
        // Each step's call names its own instance, as its placement does.
        assert_eq!(shown_as, shows::instance(&own_as, 1));
        assert_eq!(own, placed[0].actor);
        assert!(next.is_some());
        assert_eq!(joins, next, "both run the key's Bools");
        let undeclared = flows::undeclared(&flow);
        assert!(undeclared.is_empty(), "{undeclared:?}");
    }

    #[test]
    fn each_step_of_a_moved_key_lies_where_she_falls() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::FORTRESS_KEYS.checks(&romfs).unwrap();
        let key = pieces.iter().find(|c| c.item.ends_with("_01")).unwrap();
        let drops = HashMap::from([(key.hash, Drop::from(item::HOOKSHOT.to_string()))]);
        let laid = fields(
            &romfs,
            &pieces,
            &drops,
            crate::progressive::Lines::climbing(),
        )
        .unwrap();
        let placed = pickups::moved(&pieces, &drops, crate::progressive::Lines::climbing());
        // Her call's spot for the first key, on the floor at 102.5.
        let spot = ["26.111", "102.5", "-67.422"];
        let want: Vec<String> = placed
            .iter()
            .flat_map(|m| (0..3).map(move |i| format!("{}.{TRANSLATE}.{i}={}", m.hash, spot[i])))
            .collect();
        assert_eq!(laid, want, "both steps");
        let none = fields(
            &romfs,
            &pieces,
            &HashMap::new(),
            crate::progressive::Lines::climbing(),
        )
        .unwrap();
        assert!(none.is_empty(), "a key left in place stays put");
    }

    fn key_actor(n: usize) -> String {
        format!("{}{n:02}", keys::FORTRESS_KEY)
    }

    #[test]
    fn a_thief_ticks_by_its_keys_bool_only_while_it_holds_its_key() {
        needs_names!();
        let key = key_actor(3);
        let own = got(&key, &key).unwrap();
        assert!(own.ends_with("03"), "{own}");
        assert!(!own.starts_with("Rando_"), "{own}");
        assert_eq!(
            got(&key, "Item_FakeOther").as_deref(),
            Some("Rando_Thief_03")
        );
        assert!(flags().contains(&"Rando_Thief_03".to_string()));
        assert_eq!(got("Obj_FakeChest", "Item_FakeOther"), None);
    }

    #[test]
    fn nothing_changes_while_the_keys_are_where_they_were() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::FORTRESS_KEYS.checks(&romfs).unwrap();
        let mut rewrites = Rewrites::new();
        give(
            &romfs,
            &mut rewrites,
            &pieces,
            &HashMap::new(),
            crate::progressive::Lines::climbing(),
        )
        .unwrap();
        assert!(rewrites.is_empty());
    }
}
