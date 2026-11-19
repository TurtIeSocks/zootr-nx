//! The four keys the Gerudo thieves carry, as checks. Each is the placed
//! key the carpenter flow moves to its thief once beaten (`pickups`
//! swaps its actor like any holder's). The three thieves that also carry
//! their key as a drop lose it, so the one key a thief gives is the seed's.
//!
//! The flow names the key actor by its name, so each call is pointed at
//! the actor now placed. A key that was swapped never sets the Bool the
//! carpenters read, so the flow sets it after the thief's call.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Actor, EventKind, Flowchart, Value};

use crate::checks::{Drop, ItemCheck};
use crate::flows::SET_BOOL;
use crate::flows::{self, Rewrites, params_mut, running};
use crate::known::{Name, field, flow};
use crate::{keys, pickups};

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

/// Point each moved key's call at the actor now placed there, and set the
/// key's Bool after it, for the Fortress keys of `pieces` that `drops`
/// changes.
pub fn give(
    romfs: &ounce_formats::Romfs,
    rewrites: &mut Rewrites,
    pieces: &[ItemCheck],
    drops: &HashMap<u64, Drop>,
) -> Result<()> {
    // A thief's key never shows a step at a time, whatever the setting.
    let moved: Vec<_> = pickups::moved(pieces, drops, false)
        .into_iter()
        .filter(|m| m.vanilla.starts_with(keys::FORTRESS_KEY))
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    flows::edit(romfs, rewrites, FLOW, |flow| {
        let sets = *running(flow, SET_BOOL)
            .first()
            .with_context(|| format!("{FLOW} sets no Bool"))?;
        let set_bool = flows::model(flow, SET_BOOL, sets)?;
        for m in &moved {
            let (entry, flag) = thief_of(m.vanilla)?;
            let call = flow.entry_points.iter().find(|e| e.name == entry);
            let call = call.and_then(|e| e.main);
            let call = call.with_context(|| format!("{FLOW} has no {entry}"))?;
            ensure!(
                matches!(flow.events[call as usize].kind, EventKind::SubFlow { .. }),
                "{entry} doesn't start with a call"
            );
            rename(flow, call, &m.actor)?;
            let n = m
                .vanilla
                .strip_prefix(keys::FORTRESS_KEY)
                .unwrap_or_default();
            // Each lands right after the call, so the key's Bool runs first.
            for flag in [gave(n), flag] {
                flows::set_after(flow, &set_bool, call, &flag, Value::Bool(vec![true]))?;
            }
        }
        Ok(())
    })
}

/// Name `actor` in the key argument of the call at `at`, keeping the
/// instance name the placement still has.
fn rename(flow: &mut Flowchart, at: u16, actor: &str) -> Result<()> {
    let params = params_mut(&mut flow.events[at as usize].kind);
    let slot = params.iter_mut().find(|(k, _)| k == KEY);
    let slot = slot.with_context(|| format!("the call has no {KEY}"))?;
    let Value::ActorId { name, secondary } = &mut slot.1 else {
        anyhow::bail!("{KEY} isn't an actor");
    };
    let old = std::mem::replace(name, actor.to_owned());
    let secondary = secondary.clone();
    // The game finds a call's actor in the flow's table by name: a name the
    // table lacks indexes past it and crashes as the flow loads.
    let declared = flow
        .actors
        .iter()
        .find(|a| a.name == old && a.secondary_name == secondary);
    let template = declared.with_context(|| format!("{} doesn't declare {old}", flow.name))?;
    let template = Actor {
        name: actor.to_owned(),
        ..template.clone()
    };
    flow.actor_like(&template);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::scan;
    use crate::game::dev_dump;
    use crate::known::{actor, field, flag};

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
        give(&romfs, &mut rewrites, &pieces, &drops).unwrap();
        let actor = pickups::moved(&pieces, &drops, false).remove(0).actor;
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
        all_declared(&flow);
    }

    /// Every call naming a key actor names one the flow declares.
    fn all_declared(flow: &Flowchart) {
        for e in &flow.events {
            let EventKind::SubFlow { params, .. } = &e.kind else {
                continue;
            };
            let Some((_, Value::ActorId { name, secondary })) = params.iter().find(|p| p.0 == KEY)
            else {
                continue;
            };
            let same = |a: &Actor| &a.name == name && &a.secondary_name == secondary;
            assert!(
                flow.actors.iter().any(same),
                "<{name} {secondary}> undeclared"
            );
        }
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
        give(&romfs, &mut rewrites, &pieces, &HashMap::new()).unwrap();
        assert!(rewrites.is_empty());
    }
}
