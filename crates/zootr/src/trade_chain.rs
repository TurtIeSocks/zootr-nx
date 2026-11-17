//! Biggoron's trade chain. Every seed cuts its three countdowns (the
//! mushroom, the frog and the eye drops): the trade slot makes holding
//! several trade items normal, and a countdown punishes that. See
//! `docs/superpowers/specs/2026-11-17-trade-checks-design.md`.

use anyhow::{Context, Result};
use ounce_formats::{Byml, EventKind, Flowchart, Link, Romfs};

use crate::flows::{self, Rewrites};
use crate::gamedata;
use crate::known::{Name, flag, flow};

/// The flow holding the chain's shared entries, the countdowns among them.
const CHAIN: &Name = flow::SUB_EVENT_GET_SWORD_BIG_GORON;
/// The entries Grog, King Zora and the scientist call to start a countdown.
const TIMERS: [&Name; 3] = [
    flow::BIG_GORON_TIMER_STEP04,
    flow::BIG_GORON_TIMER_STEP09,
    flow::BIG_GORON_TIMER_STEP10,
];
/// Whether a countdown runs, read from the quest step and the item held,
/// not the timer. Warping, sleeping and changing age read it, and when
/// it's true Navi warns and the trade item is taken.
const COUNTING: &Name = flag::SUB_EVENT_GET_SWORD_BIG_GORON_TIMER_COUNTING_STEP;

/// Each countdown entry runs only the short wait after its timer setting,
/// then ends: no setting, no timer on screen, no start.
pub fn cut_timers(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, CHAIN, |flow| {
        for timer in TIMERS {
            let entry = (flow.entry_points.iter())
                .position(|e| e.name == **timer)
                .with_context(|| format!("no entry {timer}"))?;
            let wait = wait_after(flow, flow.entry_points[entry].main)
                .with_context(|| format!("{timer} doesn't wait after its setting"))?;
            flow.link(Link::Entry(entry), Some(wait))?;
            flow.link(Link::Next(wait), None)?;
        }
        Ok(())
    })
}

/// The wait right after event `setting`, reached from nowhere else.
fn wait_after(flow: &Flowchart, setting: Option<u16>) -> Option<u16> {
    let next = |e: u16| match &flow.events[e as usize].kind {
        EventKind::Action { next, .. } => *next,
        _ => None,
    };
    let wait = next(setting?)?;
    let is_wait = match &flow.events[wait as usize].kind {
        EventKind::Action { actor, action, .. } => {
            flow.actor_of(*actor).actions[*action as usize] == **flow::EVENT_WAIT
        }
        _ => false,
    };
    (is_wait && flow.links_to(wait).len() == 1).then_some(wait)
}

/// No countdown ever runs, so none can cost Link his trade item.
pub fn never_counting(root: &mut Byml) -> Result<()> {
    gamedata::never(root, COUNTING)
}

#[cfg(test)]
mod tests {
    use ounce_formats::BymlDoc;

    use super::*;
    use crate::game::dev_dump;
    use crate::gamedata::GameData;

    /// Every event an entry runs, by action name.
    fn runs(flow: &Flowchart, entry: &Name) -> Vec<String> {
        let reached = flows::reachable(flow, entry).unwrap();
        (reached.into_iter())
            .filter_map(|e| match &flow.events[e as usize].kind {
                EventKind::Action { actor, action, .. } => {
                    Some(flow.actor_of(*actor).actions[*action as usize].clone())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_countdown_starts() {
        let Some(romfs) = dev_dump() else { return };
        let before = flows::load(&romfs, CHAIN).unwrap().unwrap();
        let mut rewrites = Rewrites::new();
        cut_timers(&romfs, &mut rewrites).unwrap();
        let after = Flowchart::parse(&rewrites[&flows::rel(CHAIN)])
            .unwrap()
            .unwrap();
        for timer in TIMERS {
            let ran = runs(&before, timer);
            assert!(ran.iter().any(|a| a.contains("Timer")), "{timer}: {ran:?}");
            assert_eq!(
                runs(&after, timer),
                [flow::EVENT_WAIT.to_string()],
                "{timer}"
            );
        }
    }

    #[test]
    fn nothing_reads_a_countdown_as_running() {
        let Some(romfs) = dev_dump() else { return };
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        let gd = GameData::load(&romfs).unwrap();
        let before = gd.expr(COUNTING).unwrap().unwrap();
        assert!(before.contains("== Step04"), "{before}");
        never_counting(&mut doc.root).unwrap();
        let hash = ounce_formats::murmur3(COUNTING.as_bytes());
        let table = &doc.root["Data"][flag::BOOL_EXP.get()];
        let entry = (table.as_array().unwrap().iter())
            .find(|e| {
                e["Hash"]
                    .as_int::<i64>()
                    .is_ok_and(|h| h == i64::from(hash))
            })
            .unwrap();
        assert_eq!(
            gd.render(entry).unwrap(),
            format!("({before}) && !({before})")
        );
    }
}
