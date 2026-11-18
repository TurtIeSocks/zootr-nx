//! Dampe's race gives its two chests as checks. The race decides what it is
//! from what Link holds: every talk, timer and step asks whether he has the
//! Hookshot, and the second prize whether he took the heart piece. With
//! those items anywhere in the seed, the race would follow what Link found
//! rather than the race. So it asks two saved Bools instead, set when each
//! chest opens, and keeps the vanilla order: first chest, then second.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Flowchart, Link, Romfs, Value};

use crate::checks::Drop;
use crate::flows::{
    self, ACTOR, GAME_DATA, IN_POUCH, IS_BOOL, Model, Rewrites, SET_BOOL, param, running, set,
    string, switch,
};
use crate::known::{Name, field, flag, flow, item};

/// The chest that holds the Hookshot, then the one with the heart piece.
pub const HOOKSHOT_CHEST: u64 = 16041366920820397198;
pub const HEART_CHEST: u64 = 5534919361313859554;
/// The Blue and Red Rupee chests, the race's repeat prizes. They stay
/// vanilla, the maintainer's call (EIE spec, row 8).
pub const REPEAT_CHESTS: [u64; 2] = [4322670163630771947, 234006393181394602];
/// Set when the first chest opens, then the second.
pub const FIRST: &str = "Randomizer_DampeRace_First";
pub const SECOND: &str = "Randomizer_DampeRace_Second";

const FLOW: &Name = flow::MINI_GAME_DAMPE_RACE;
const SET_ENUM: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_TRIGGER_SET_GAME_DATA_ENUM,
);
/// What the race asks for the Hookshot: in the pouch, whose case 1 is held,
/// and whether it was ever got, whose case 0 is.
const EVER_GOT: (&Name, &Name) = (field::GAME_DATA_ACTOR, flow::EVENT_QUERY_CHECK_IS_GET);
const GOT: &Name = flag::ACTOR_NAME;
const HOOKSHOT: &Name = item::HOOKSHOT;
/// The Bool the game sets when Link takes the second prize.
const HEART: &Name = flag::UNDER_THE_GRAVE_DAMPE_RACE_HEART_GETED;
/// The state the race keeps for its chest, and the value each chest's
/// opening sets.
const STATE: &Name = flag::MINIGAME_DAMPE_RACE_TREASURE_BOX_STATE;
const OPENED: [(&Name, &str); 2] = [
    (field::OPENED_HOOK_SHOT, FIRST),
    (field::OPENED_HEART_PIECE, SECOND),
];

/// The Bools the race asks, when the seed moves either chest's item.
pub fn flags(drops: &HashMap<u64, Drop>) -> Vec<String> {
    match [HOOKSHOT_CHEST, HEART_CHEST]
        .iter()
        .any(|h| drops.contains_key(h))
    {
        true => vec![FIRST.into(), SECOND.into()],
        false => Vec::new(),
    }
}

/// Make the race ask its own Bools, when `drops` moves either chest.
pub fn give(romfs: &Romfs, rewrites: &mut Rewrites, drops: &HashMap<u64, Drop>) -> Result<()> {
    if flags(drops).is_empty() {
        return Ok(());
    }
    flows::edit(romfs, rewrites, FLOW, |flow| {
        let asks = switch(flow, IS_BOOL).with_context(|| format!("{FLOW} asks no Bool"))?;
        let sets = running(flow, SET_BOOL).first().copied();
        let sets = sets.with_context(|| format!("{FLOW} sets no Bool"))?;
        let is_bool = flows::model(flow, IS_BOOL, asks)?;
        let set_bool = flows::model(flow, SET_BOOL, sets)?;
        read(flow, &is_bool)?;
        set_when_opened(flow, &set_bool)
    })
}

/// Every question about the Hookshot or the heart piece reads a Bool.
fn read(flow: &mut Flowchart, is_bool: &Model) -> Result<()> {
    let (actor, query) = flow.query(&is_bool.actor, is_bool.name);
    let (mut hookshot, mut heart) = (0, 0);
    for i in 0..flow.events.len() {
        let EventKind::Switch {
            actor: a,
            query: q,
            params,
            ..
        } = &flow.events[i].kind
        else {
            continue;
        };
        let asked = flow.actor_of(*a);
        let is =
            |actor: &str, query: &str| asked.name == actor && asked.queries[*q as usize] == query;
        let held = |key| param(params, key).is_some_and(|v| v == HOOKSHOT);
        // The flag, and whether the question's cases are the Bool's reversed.
        let (flag, reversed) = if is(IN_POUCH.0, IN_POUCH.1) && held(ACTOR) {
            (FIRST, false)
        } else if is(EVER_GOT.0, EVER_GOT.1) && held(GOT) {
            (FIRST, true)
        } else if is(IS_BOOL.0, IS_BOOL.1) && param(params, GAME_DATA).is_some_and(|v| v == HEART) {
            (SECOND, false)
        } else {
            continue;
        };
        match flag == FIRST {
            true => hookshot += 1,
            false => heart += 1,
        }
        let mut params = is_bool.params.clone();
        set(&mut params, GAME_DATA, string(flag))?;
        let EventKind::Switch {
            actor: a,
            query: q,
            params: p,
            cases,
        } = &mut flow.events[i].kind
        else {
            unreachable!("matched as a switch above");
        };
        (*a, *q, *p) = (actor, query, params);
        if reversed {
            cases.iter_mut().for_each(|c| c.0 = 1 - c.0);
        }
    }
    ensure!(
        hookshot > 0 && heart > 0,
        "{} asks for the Hookshot {hookshot} times and the heart {heart}",
        flow.name
    );
    Ok(())
}

/// Each chest's opening also sets its Bool, right where the race notes the
/// chest opened.
fn set_when_opened(flow: &mut Flowchart, set_bool: &Model) -> Result<()> {
    let (actor, action) = flow.action(&set_bool.actor, set_bool.name);
    for (opened, flag) in OPENED {
        let notes: Vec<u16> = running(flow, SET_ENUM)
            .into_iter()
            .filter(|&e| {
                let EventKind::Action { params, .. } = &flow.events[e as usize].kind else {
                    return false;
                };
                param(params, GAME_DATA) == Some(STATE) && param(params, "Value") == Some(opened)
            })
            .collect();
        ensure!(
            !notes.is_empty(),
            "{} never notes {opened} in {STATE}",
            flow.name
        );
        for note in notes {
            let EventKind::Action { next, .. } = flow.events[note as usize].kind else {
                unreachable!("running finds actions");
            };
            let mut params = set_bool.params.clone();
            set(&mut params, GAME_DATA, string(flag))?;
            set(&mut params, "Value", Value::Bool(vec![true]))?;
            let sets = flow.push(EventKind::Action {
                next,
                actor,
                action,
                params,
            });
            flow.link(Link::Next(note), Some(sets))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::item;

    fn moved(hash: u64) -> HashMap<u64, Drop> {
        HashMap::from([(hash, Drop::from(item::BOTTLE.to_string()))])
    }

    fn written(drops: &HashMap<u64, Drop>) -> Flowchart {
        let romfs = dev_dump().unwrap();
        let mut rewrites = Rewrites::new();
        give(&romfs, &mut rewrites, drops).unwrap();
        match rewrites.get(&flows::rel(FLOW)) {
            Some(data) => Flowchart::parse(data).unwrap().unwrap(),
            None => flows::load(&romfs, FLOW).unwrap().unwrap(),
        }
    }

    /// An event asking a GameData entry, with its name and cases.
    type Asked<'a> = (u16, &'a str, &'a [(u32, u16)]);

    /// The events of `flow` asking `what`.
    fn asks<'a>(flow: &'a Flowchart, what: (&str, &str)) -> Vec<Asked<'a>> {
        (0u16..)
            .zip(&flow.events)
            .filter_map(|(i, e)| match &e.kind {
                EventKind::Switch {
                    actor,
                    query,
                    params,
                    cases,
                } => {
                    let a = flow.actor_of(*actor);
                    let is = (a.name.as_str(), a.queries[*query as usize].as_str()) == what;
                    let name = param(params, GAME_DATA)?;
                    is.then_some((i, name, cases.as_slice()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_race_is_left_alone_unless_a_chest_moves() {
        assert!(flags(&HashMap::new()).is_empty());
        assert!(flags(&moved(1)).is_empty());
        for chest in [HOOKSHOT_CHEST, HEART_CHEST] {
            assert_eq!(flags(&moved(chest)), [FIRST, SECOND]);
        }
        let Some(romfs) = dev_dump() else { return };
        let vanilla = flows::load(&romfs, FLOW).unwrap().unwrap();
        assert_eq!(written(&HashMap::new()), vanilla);
    }

    #[test]
    fn a_moved_chest_leaves_the_race_asking_its_own_bools_in_the_same_order() {
        if dev_dump().is_none() {
            return;
        }
        let flow = written(&moved(HOOKSHOT_CHEST));
        let held = |flag: &str| -> Vec<_> {
            let bools = asks(&flow, flows::said(IS_BOOL)).into_iter();
            bools.filter(|a| a.1 == flag).collect()
        };
        assert_eq!(held(FIRST).len(), 9);
        assert_eq!(held(SECOND).len(), 5);
        assert_eq!(held(HEART).len(), 0);
        let strings = flows::strings(&flow);
        assert!(
            !strings.contains(&&**HOOKSHOT),
            "still asks for the Hookshot"
        );
        // The pouch question keeps its cases, the ever-got one reverses
        // them: it said "yes" with case 0.
        let cases = |at: u16| held(FIRST).iter().find(|a| a.0 == at).unwrap().2.to_vec();
        assert_eq!(cases(39), [(0, 156), (1, 3)]);
        assert_eq!(cases(90), [(1, 92), (0, 183)]);
        // Each chest's opening sets its Bool before the race goes on.
        for (opened, flag) in OPENED {
            let notes: Vec<_> = running(&flow, SET_ENUM)
                .into_iter()
                .filter(|&e| {
                    let EventKind::Action { params, .. } = &flow.events[e as usize].kind else {
                        return false;
                    };
                    param(params, "Value") == Some(opened)
                })
                .collect();
            assert!(!notes.is_empty(), "{opened}");
            for note in notes {
                let EventKind::Action { next, .. } = flow.events[note as usize].kind else {
                    unreachable!();
                };
                let EventKind::Action { params, next, .. } =
                    &flow.events[next.unwrap() as usize].kind
                else {
                    panic!("{opened} goes on without setting {flag}");
                };
                assert_eq!(param(params, GAME_DATA), Some(flag));
                assert_eq!(
                    params.iter().find(|p| p.0 == "Value").map(|p| &p.1),
                    Some(&Value::Bool(vec![true]))
                );
                assert!(next.is_some());
            }
        }
    }
}
