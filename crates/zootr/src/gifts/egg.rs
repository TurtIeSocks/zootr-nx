//! Malon's Weird Egg ✅ with NPC gifts on. The egg hatches by place and
//! quest step, never by the egg itself: a trigger by the moat hatches it
//! once Malon's talk has moved the castle quest to Step03, whatever she
//! gave. So her check holds the cucco in the egg's place ([`own`]) and
//! nothing hatches: her talk no longer moves the quest, and a new game
//! marks the hatch played, which deletes its trigger. A cucco found first
//! wakes Talon before her talk, and she leaves for the ranch once he's up,
//! so his wake hands over her check instead ([`from_talon`]). With gifts
//! off it all stays the game's own.
//! Spec: the cucco takes the Weird Egg's place (2026-11-25).

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use ounce_formats::{Flowchart, Romfs};

use super::{GIFTS, Gift, Rewrites, later};
use crate::checks::ItemCheck;
use crate::cutscenes::played_flag;
use crate::events::Flows;
use crate::flows;
use crate::known::{Name, flow, item, scene};
use crate::settings::NPC_GIFTS;
use crate::starting::Start;

pub(super) const FLOW: &Name = flow::GOTO_HYRULE_CASTLE;
/// The castle quest's step Malon's talk moves it to, which the hatch waits on.
const HATCH_STEP: &str = "Step03";
/// The starting words for the egg and the cucco.
const EGG: &str = "weird-egg";
const CUCCO: &str = "cucco";

fn gift() -> &'static Gift {
    (GIFTS.iter().find(|g| g.item == item::WEIRD_EGG)).expect("Malon's egg is a gift")
}

/// The item `check` holds in play when it's Weird Egg ✅: the cucco. Her
/// check is only in play with NPC gifts on, where nothing hatches.
pub fn own(check: &ItemCheck) -> Option<&'static str> {
    (check.hash == gift().hash()).then(|| item::ALARM_CUCCO.get())
}

/// Step over Malon's move of the castle quest to the hatch's step.
pub(super) fn unhatched(flow: &mut Flowchart) -> Result<()> {
    let moves = crate::steps::rewinds(flow, FLOW.get(), &[HATCH_STEP.into()]);
    let [one] = moves[..] else {
        anyhow::bail!(
            "{FLOW} moves to {HATCH_STEP} {} times, not once",
            moves.len()
        );
    };
    flows::step_over(flow, one)
}

/// The new-game edit marking the hatch played, with NPC gifts on: its
/// trigger deletes itself as the castle loads.
pub fn hatched(on: &BTreeSet<String>) -> Option<String> {
    let hatch = flow::GOTO_HYRULE_CASTLE_EGGHATCH_AREABOOT;
    (on.contains(NPC_GIFTS)).then(|| format!("{}=true", played_flag(hatch)))
}

/// Whether `gift` is Weird Egg ✅.
pub(super) fn is(gift: &Gift) -> bool {
    gift.item == item::WEIRD_EGG
}

/// Have Talon's wake hand over Weird Egg ✅'s `item` unless Malon already
/// did, whether his scene plays or a skip marks it played.
pub(super) fn from_talon(
    romfs: &Romfs,
    flows: &Flows,
    rewrites: &mut Rewrites,
    item: &str,
) -> Result<()> {
    let given = gift().found().context("Malon's check sets no Bool")?;
    let wake = scene::HRL_CASTLE_0020.get();
    flows::edit(romfs, rewrites, FLOW, |f| {
        later::grant_after_unless(f, flows, wake, item, Some(&given))
    })
}

/// With NPC gifts on, a starting egg starts as the cucco it would hatch
/// into, since nothing hatches it.
pub fn start_hatched(start: &mut Start, gifts: bool) -> Result<()> {
    if gifts && start.0.remove(EGG).is_some_and(|n| n > 0) && !start.holds(CUCCO) {
        start.add(CUCCO, 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::tests::written;
    use super::super::{Rewrites, give_all};
    use super::*;
    use crate::events::Flows;
    use crate::known::actor;

    fn on(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The flows `give_all` writes with Malon's check dealt `item`, or none.
    fn dealt(romfs: &ounce_formats::Romfs, item: Option<&Name>) -> Rewrites {
        let drops = (item.iter())
            .map(|i| (gift().hash(), crate::checks::Drop::from(i.to_string())))
            .collect::<HashMap<_, _>>();
        let mut rewrites = Rewrites::default();
        let on = match item {
            Some(_) => on(&[NPC_GIFTS]),
            None => on(&[]),
        };
        give_all(
            romfs,
            &Flows::load(romfs).unwrap(),
            &mut rewrites,
            &drops,
            crate::progressive::Lines::default(),
            &on,
        )
        .unwrap();
        rewrites
    }

    #[test]
    fn a_dealt_cucco_is_handed_over_and_the_quest_never_reaches_the_hatch() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = flows::load(&romfs, FLOW).unwrap().unwrap();
        let moves = |f: &Flowchart| crate::steps::rewinds(f, FLOW.get(), &[HATCH_STEP.into()]);
        assert_eq!(moves(&vanilla).len(), 1);
        let rewrites = dealt(&romfs, Some(item::ALARM_CUCCO));
        let flow = written(&rewrites, FLOW);
        let text = flows::render(&flow);
        assert!(
            text.contains(&format!("\"{}\"", item::ALARM_CUCCO)),
            "{text}"
        );
        // Nothing leads to her move of the quest any more.
        assert!(moves(&flow).iter().all(|&e| flow.links_to(e).is_empty()));
        // Her check's own Bool is set after the hand-over.
        assert!(text.contains(&gift().found().unwrap()));
        // Gifts off, her flow is the game's own.
        assert!(!dealt(&romfs, None).contains_key(&flows::rel(FLOW)));
    }

    #[test]
    fn talon_s_wake_hands_over_her_check_unless_malon_did() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rewrites = dealt(&romfs, Some(actor::OBJ_HEART_PIECE));
        let flow = written(&rewrites, FLOW);
        let text = flows::render(&flow);
        let lines: Vec<&str> = text.lines().collect();
        let event = |n: &str| {
            lines
                .iter()
                .find(|l| l.starts_with(&format!("{n} ")))
                .unwrap()
        };
        // The wake's transition leads to a question on her check's Bool.
        let wake = (lines.iter())
            .find(|l| l.contains(&format!("\"{}\"", scene::HRL_CASTLE_0020)))
            .unwrap();
        let asks = event(wake.rsplit_once(" -> ").unwrap().1);
        let given = gift().found().unwrap();
        assert!(
            asks.contains("IsGameDataBool") && asks.contains(&given),
            "{asks}"
        );
        // Given, it only sets the Bool again. Not given, the grant runs first.
        let (_, cases) = asks.rsplit_once("? ").unwrap();
        let [set, grant] = [" 0:", " 1:"].map(|c| {
            let at = format!(" {cases}")
                .split(c)
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .to_owned();
            *event(&at)
        });
        assert!(
            set.contains("SetGameDataBool") && set.contains(&given),
            "{set}"
        );
        assert!(!grant.contains("SetGameDataBool"), "{grant}");
        assert_eq!(
            text.matches(&super::super::tests::granting(actor::OBJ_HEART_PIECE.get()))
                .count(),
            1
        );
    }

    #[test]
    fn only_gifts_mark_the_hatch_played() {
        needs_names!();
        let played = hatched(&on(&[NPC_GIFTS])).unwrap();
        assert!(played.ends_with("_Areaboot=true"), "{played}");
        assert_eq!(hatched(&on(&[])), None);
    }

    #[test]
    fn a_starting_egg_starts_as_the_cucco_with_gifts_only() {
        needs_names!();
        let egg = || Start([(EGG.to_owned(), 1)].into());
        let mut start = egg();
        start_hatched(&mut start, false).unwrap();
        assert!(start.holds(EGG) && !start.holds(CUCCO));
        let mut start = egg();
        start_hatched(&mut start, true).unwrap();
        assert!(!start.holds(EGG) && start.holds(CUCCO));
    }
}
