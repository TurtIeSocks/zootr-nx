//! Malon's Weird Egg ✅ with NPC gifts on. The egg hatches by place and
//! quest step, never by the egg itself: a trigger by the moat hatches it
//! once Malon's talk has moved the castle quest to Step03, whatever she
//! gave. So her check holds the cucco in the egg's place ([`own`]) and
//! nothing hatches: her talk no longer moves the quest, and a new game
//! marks the hatch played, which deletes its trigger. Talon woken first
//! moves the quest past the step she stood until, so she stands until her
//! check is given instead. With gifts off it all stays the game's own.
//! Spec: the cucco takes the Weird Egg's place (2026-11-25).

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, Flowchart};

use super::{GIFTS, Gift};
use crate::checks::{ItemCheck, actors_mut, gate, hash_of};
use crate::flows;
use crate::cutscenes::played_flag;
use crate::known::{Name, flow, item};
use crate::settings::NPC_GIFTS;
use crate::starting::Start;

pub(super) const FLOW: &Name = flow::GOTO_HYRULE_CASTLE;
/// The castle quest's step Malon's talk moves it to, which the hatch waits on.
const HATCH_STEP: &str = "Step03";
/// Malon at the castle wall.
const MALON: u64 = 3576280271882277111;
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

/// Gate Malon at the castle wall on her check's Bool, with NPC gifts on, so
/// she stands until it's given at any step of the quest. Not with Met
/// Zelda: the opening gives her check and starts the quest past her.
/// Returns the hashes changed.
pub fn malon(root: &mut Byml, on: &BTreeSet<String>) -> Result<Vec<u64>> {
    let gift = gift();
    if !gift.dealt(on) || gift.pocket(on) {
        return Ok(Vec::new());
    }
    let given = gift.found().context("Malon's check sets no Bool")?;
    let mut changed = Vec::new();
    for actor in actors_mut(root)?.iter_mut() {
        if hash_of(actor) != Some(MALON) {
            continue;
        }
        ensure!(changed.is_empty(), "Malon is placed twice");
        actor.as_mut_map()?.remove("Presence");
        gate(actor, &given, true)?;
        changed.push(MALON);
    }
    Ok(changed)
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
    use crate::checks::{actors, load_banc};
    use crate::events::Flows;
    use crate::known::{field, map};
    use crate::settings::MET_ZELDA;

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
    fn only_gifts_mark_the_hatch_played() {
        needs_names!();
        let played = hatched(&on(&[NPC_GIFTS])).unwrap();
        assert!(played.ends_with("_Areaboot=true"), "{played}");
        assert_eq!(hatched(&on(&[])), None);
    }

    #[test]
    fn with_gifts_malon_stands_until_her_check_is_given() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rel = format!(
            "Banc/{0}/{0}_Static.bcett.byml.zs",
            map::HYRULE_CASTLE.get()
        );
        let gate_of = |root: &Byml| {
            let malon = actors(root)
                .unwrap()
                .iter()
                .find(|a| hash_of(a) == Some(MALON));
            let p = malon.unwrap().as_map().unwrap()["Presence"]
                .as_map()
                .unwrap();
            (
                p[field::FLAG_NAME.get()].as_string().unwrap().to_string(),
                p[field::IS_NEGATION.get()].as_bool().unwrap(),
            )
        };
        let vanilla = load_banc(&romfs, &rel).unwrap().root;
        for off in [on(&[]), on(&[NPC_GIFTS, MET_ZELDA])] {
            let mut root = vanilla.clone();
            assert!(malon(&mut root, &off).unwrap().is_empty());
            assert_eq!(gate_of(&root), gate_of(&vanilla));
        }
        let mut root = vanilla.clone();
        assert_eq!(malon(&mut root, &on(&[NPC_GIFTS])).unwrap(), [MALON]);
        assert_eq!(gate_of(&root), (gift().found().unwrap(), true));
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
