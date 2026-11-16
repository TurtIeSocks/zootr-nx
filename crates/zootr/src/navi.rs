//! Quiet Navi: the talks Navi starts on her own, stepped over. See
//! `docs/superpowers/specs/2026-11-11-quiet-navi-design.md`.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use ounce_formats::{EventKind, Flowchart, Link, Romfs};

use crate::flows::{self, Rewrites};
use crate::known::{Name, actor, field, flag, flow, scene};

/// Entry points that make Navi talk without Link asking, by flow.
pub const FORCED: [(&Name, &[&Name]); 14] = [
    (
        scene::SY_BANC_CHANGE_ARRIVE,
        &[
            flag::GET_STONE_OF_FIRE_ARRIVE_FROM_KOKIRI,
            // The ocarina guide on the first walk into Hyrule Field.
            scene::SY_BANC_CHANGE_ARRIVE_HYRULE_FIELD,
        ],
    ),
    (
        flow::NAVI_HINT,
        &[
            flow::HINT_DUNGEON_TUTORIAL_EVENT,
            flag::HINT_TRIAL,
            flag::NAVI_HINT_SHIELD_HYLIAN,
            flag::NAVI_HINT_SHIELD_HYLIAN_BUY_SHOP,
            flow::NAVI_HINT_SHIELD_HYLIAN_PLAY_CHECK,
            flow::NAVI_HINT_SHIELD_HYLIAN_PLAY_CHECK_BUY_SHOP,
        ],
    ),
    (
        flow::ALL_SAGE,
        &[
            field::NAVI_GUIDE_SHEIK_SPIRIT_TEMPLE,
            field::NAVI_GUIDE_SHEIK_UNDER_WELL,
        ],
    ),
    (
        flow::DOOR_OF_TIME,
        &[
            field::DOOR_OF_TIME_GET_OCARINA_OF_TIME_AREA_OUT,
            flow::DOOR_OF_TIME_NAVI_REPEAT,
            field::DOOR_OF_TIME_ESCAPE,
        ],
    ),
    (
        flow::GATHERING_TOKEN_GOLD,
        &[
            field::NAVI_GUIDE_GATHERING_TOKEN_GOLD,
            flag::NAVI_GUIDE_TOKEN_GOLD_OVER10,
        ],
    ),
    (
        flow::GET_STONE_OF_WATER,
        &[field::STONE_OF_WATER_ELEVATED_FLOOR_NAVI_TALK],
    ),
    (
        flow::SAGE_OF_SPIRIT,
        &[
            field::SAGE_OF_SPIRIT_NAVI_CRAWL_TUNNEL,
            flag::NAVI_LEAVE_DUNGEON,
        ],
    ),
    (
        flow::SAGE_OF_FIRE,
        &[
            flow::SAGE_OF_FIRE_NAVI_SCORCHING,
            flag::SAGE_OF_FIRE_NAVI_CRATER_TALK,
        ],
    ),
    (
        flow::HOUNTED_WASTELAND_FIELD_RESTART,
        &[
            flow::HOUNTED_WASTE_LAND_RESTART_NAVI,
            field::HOUNTED_WASTELAND_FIELD_RESTART_TO_ENTRANCE,
            field::HOUNTED_WASTELAND_FIELD_RESTART_TO_CHECKPOINT,
        ],
    ),
    (
        flow::SAGE_OF_SPIRIT_RESCUE_CARPENTER,
        &[field::WINDOWHINT, field::WINDOWHINT_1, field::WINDOWHINT_4],
    ),
    (
        flow::SAGE_OF_WATER,
        &[
            field::WATER_TEMPLE_WATER_LEVEL_CHANGE_NAVI,
            scene::MELODY_GET_WATER_AFTER,
        ],
    ),
    (
        flow::BOTTOM_OF_THE_WELL,
        &[
            flow::SUB_EVENT_GET_GLASS_TRUTH_NAVI_REPEAT,
            field::BOTTOM_OF_THE_WELL_GET_GLASS_TRUTH,
        ],
    ),
    (
        flow::MINI_GAME_DAMPE_RACE,
        &[
            flow::MINI_GAME_DAMPE_RACE_NAVI_REPEAT,
            flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX,
        ],
    ),
    (
        flow::FOREST_TEMPLE_GIMMICK,
        &[
            flow::FIRST_TIME_BLOCK,
            field::FOREST_TEMPLE_ROOM25_FIRST_TIME_BLOCK,
            field::SACRED_FOREST_FIRST_TIME_BLOCK,
        ],
    ),
];

/// Steps over every Navi talk the `FORCED` entries reach, and the camera
/// moves that frame them.
pub fn quiet(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    for (flow, entries) in FORCED {
        flows::edit(romfs, rewrites, flow, |f| silence(f, entries))?;
    }
    Ok(())
}

fn silence(flow: &mut Flowchart, entries: &[&Name]) -> Result<()> {
    let others: Vec<&str> = flow
        .entry_points
        .iter()
        .map(|ep| ep.name.as_str())
        .filter(|n| !entries.iter().any(|e| *e == *n))
        .collect();
    let kept = reached(flow, &others)?;
    let talks: BTreeSet<u16> = reached(flow, entries)?
        .into_iter()
        .filter(|&e| is_talk(flow, e) || (is_framing(flow, e) && !kept.contains(&e)))
        .collect();
    let end = flows::ending(flow);
    for &at in &talks {
        let to = past(flow, at, &talks)?;
        for link in flow.links_to(at) {
            match (link, to) {
                // Another talk, itself stepped over.
                (Link::Next(i), _) if talks.contains(&i) => {}
                (Link::Branch(..), None) => {
                    let end = end.context("a case runs a talk last, and nothing ends quietly")?;
                    flow.link(link, Some(end))?;
                }
                _ => flow.link(link, to)?,
            }
        }
    }
    Ok(())
}

/// Every event `entries` can run, through calls into the flow's own
/// entries.
fn reached(flow: &Flowchart, entries: &[impl AsRef<str>]) -> Result<BTreeSet<u16>> {
    let main = |name: &str| {
        let ep = flow.entry_points.iter().find(|ep| ep.name == name);
        ep.map(|ep| ep.main)
            .with_context(|| format!("no entry {name} in {}", flow.name))
    };
    let mut stack = entries
        .iter()
        .map(|e| main(e.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    let mut seen = BTreeSet::new();
    while let Some(at) = stack.pop() {
        let Some(at) = at else { continue };
        if !seen.insert(at) {
            continue;
        }
        match &flow.events[at as usize].kind {
            EventKind::Action { next, .. } | EventKind::Join { next } => stack.push(*next),
            EventKind::Switch { cases, .. } => stack.extend(cases.iter().map(|c| Some(c.1))),
            EventKind::Fork { join, branches } => {
                stack.push(Some(*join));
                stack.extend(branches.iter().copied().map(Some));
            }
            EventKind::SubFlow {
                next,
                flowchart,
                entry_point,
                ..
            } => {
                stack.push(*next);
                if flowchart.is_empty() {
                    stack.push(main(entry_point)?);
                }
            }
            EventKind::SubFlowSwitch {
                flowchart,
                entry_point,
                cases,
                ..
            } => {
                stack.extend(cases.iter().map(|c| Some(c.1)));
                if flowchart.is_empty() {
                    stack.push(main(entry_point)?);
                }
            }
            EventKind::Return { .. } => {}
        }
    }
    Ok(seen)
}

/// Whether `entry` is one of `talk`'s Navi entries (`talk` then `Navi`).
fn navis(entry: &str, talk: &str) -> bool {
    entry
        .strip_prefix(talk)
        .is_some_and(|rest| rest.starts_with("Navi"))
}

/// Navi saying something, flying in to say it, or the tutorial screen
/// after.
fn is_talk(flow: &Flowchart, e: u16) -> bool {
    match &flow.events[e as usize].kind {
        EventKind::SubFlow {
            flowchart,
            entry_point,
            ..
        } => {
            (flowchart == flow::NAVI_EVENT_COMMON && navis(entry_point, "EventTalk_"))
                || navis(entry_point, "InitTalk_")
                || entry_point == flow::INIT_TALK_NAVI_HELP
                || flowchart == flow::ACTION_GUIDE
        }
        EventKind::Action { actor, action, .. } => {
            let actor = flow.actor_of(*actor);
            actor.name == actor::NPC_NAVI
                && actor.actions[*action as usize].starts_with(&**flow::EVENT_TALK)
        }
        _ => false,
    }
}

/// A camera move that frames Navi's talk, short of the flow's end. Kept
/// where an entry left alone reaches it too.
fn is_framing(flow: &Flowchart, e: u16) -> bool {
    let EventKind::Action {
        actor,
        action,
        next: Some(_),
        ..
    } = &flow.events[e as usize].kind
    else {
        return false;
    };
    let actor = flow.actor_of(*actor);
    actor.name == flow::EVENT_CAMERA
        && actor.actions[*action as usize] == flow::EVENT_MOVE_TO_TARGET_POS_CAMERA
}

/// Where to go instead of talk `at`: its next event, past any further
/// talks. `None` is the end of the flow.
fn past(flow: &Flowchart, mut at: u16, talks: &BTreeSet<u16>) -> Result<Option<u16>> {
    for _ in 0..flow.events.len() {
        let next = match &flow.events[at as usize].kind {
            EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } => *next,
            _ => bail!("talk {at} has no next"),
        };
        match next {
            Some(n) if talks.contains(&n) => at = n,
            _ => return Ok(next),
        }
    }
    bail!("talks loop back on themselves")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, flow, scene};

    fn quieted() -> Option<(Romfs, Rewrites)> {
        let romfs = crate::game::dev_dump()?;
        let mut rewrites = Rewrites::new();
        quiet(&romfs, &mut rewrites).unwrap();
        Some((romfs, rewrites))
    }

    fn parse(rewrites: &Rewrites, flow: &str) -> Flowchart {
        Flowchart::parse(&rewrites[&flows::rel(flow)])
            .unwrap()
            .unwrap()
    }

    fn talks(flow: &Flowchart, entries: &[impl AsRef<str>]) -> usize {
        let reached = reached(flow, entries).unwrap();
        reached.into_iter().filter(|&e| is_talk(flow, e)).count()
    }

    #[test]
    fn forced_entries_reach_no_navi_talk() {
        let Some((romfs, rewrites)) = quieted() else {
            return;
        };
        for (name, entries) in FORCED {
            let before = flows::load(&romfs, name).unwrap().unwrap();
            assert!(talks(&before, entries) > 0, "{name} had nothing to quiet");
            assert_eq!(talks(&parse(&rewrites, name), entries), 0, "{name}");
        }
    }

    #[test]
    fn kept_entries_still_talk() {
        let Some((romfs, rewrites)) = quieted() else {
            return;
        };
        let door = parse(&rewrites, flow::DOOR_OF_TIME);
        assert!(talks(&door, &[flag::CHECK_SPIRIT_STONE_BASE]) > 0);
        assert!(!rewrites.contains_key(&flows::rel(flow::FIELD_PLANT_MAGIC_BEAN)));
        let soil = flows::load(&romfs, flow::FIELD_PLANT_MAGIC_BEAN)
            .unwrap()
            .unwrap();
        assert!(talks(&soil, &[field::CHECK_SOIL]) > 0);
    }

    #[test]
    fn the_deku_tree_arrival_keeps_the_camera_still() {
        let Some((_, rewrites)) = quieted() else {
            return;
        };
        let flow = parse(&rewrites, flow::NAVI_HINT);
        let reached = reached(&flow, &[flow::HINT_DUNGEON_TUTORIAL_EVENT]).unwrap();
        assert!(!reached.into_iter().any(|e| is_framing(&flow, e)));
    }

    #[test]
    fn every_forced_entry_exists() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (name, entries) in FORCED {
            let flow = flows::load(&romfs, name).unwrap().unwrap();
            for e in entries {
                assert!(
                    flow.entry_points.iter().any(|ep| *ep.name == *e),
                    "{name}::{e}"
                );
            }
        }
    }

    #[test]
    fn the_lost_woods_arrival_keeps_its_flags() {
        let Some((_, rewrites)) = quieted() else {
            return;
        };
        const FLAG: &Name =
            flag::HAVE_TALKED_SPECIAL_DM_SY_BANC_CHANGE_ARRIVE_GET_STONE_OF_FIRE_ARRIVE_FROM_KOKIRI_EVENT312;
        let flow = parse(&rewrites, scene::SY_BANC_CHANGE_ARRIVE);
        let sets = reached(&flow, &[flag::GET_STONE_OF_FIRE_ARRIVE_FROM_KOKIRI])
            .unwrap()
            .into_iter()
            .any(|e| match &flow.events[e as usize].kind {
                EventKind::Action { params, .. } => params.iter().any(|(k, v)| {
                    k == field::GAME_DATA_NAME && v.as_str().is_some_and(|v| v == FLAG)
                }),
                _ => false,
            });
        assert!(sets);
    }
}
