//! Where a seed is beaten: each exit into Ganondorf's stage with what it
//! needs, the win condition and the tower's keys included, and whether
//! Link gets there.

use anyhow::{Result, ensure};

use super::{Scan, World, eras_of};
use crate::keys;
use crate::known::{Name, map};
use crate::rules::{Book, Ctx, Have, Need};
use crate::world::Era;

/// Where the seed is beaten: Ganondorf's stage.
pub const GOAL: &Name = map::GANONDORF_STAGE;

/// Each exit into [`GOAL`], for `ctx`'s seed: its place, which `at` finds,
/// and what it needs.
pub(super) fn exits(
    book: &Book,
    ctx: &Ctx,
    scan: &Scan,
    at: &impl Fn(&str, Era) -> Result<usize>,
) -> Result<Vec<(usize, Need)>> {
    let mut goal = Vec::new();
    for rule in book.exits.iter().filter(|r| r.to == GOAL) {
        let mut need = book.need(&rule.expr, ctx)?;
        // The tower's boss key door, as on the exit's own edge.
        if let Some(keys) = keys::boss_door_need(&rule.to, &ctx.keys) {
            need = Need::All(vec![need, keys]);
        }
        for era in eras_of(&scan.places, &rule.from) {
            goal.push((at(&rule.from, era)?, need.clone()));
        }
    }
    ensure!(!goal.is_empty(), "no exit into {GOAL}");
    Ok(goal)
}

impl World {
    /// Each exit into `GOAL`: its place and what it needs.
    pub fn goal(&self) -> &[(usize, Need)] {
        &self.goal
    }

    /// Whether Link reaches Ganondorf with `have`.
    pub fn beaten(&self, have: &Have) -> bool {
        let seen = self.seen(have);
        let era = |p: usize| self.places[p].1;
        self.goal
            .iter()
            .any(|(p, need)| seen[*p] && need.met(have, era(*p)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{adult_edges, world};
    use super::*;
    use crate::checks;
    use crate::known::flag;

    /// `words` held, by their rule words.
    fn holding(words: &[&str]) -> Have {
        let mut have = Have::default();
        for word in words {
            have.add(&crate::rules::vocab::id(word).unwrap());
        }
        have
    }

    #[test]
    fn the_bridge_needs_what_its_setting_says() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let bosses = holding(&["beat:bongo-bongo", "beat:twinrova"]);
        let medallions = holding(&[
            "item:forest-medallion",
            "item:fire-medallion",
            "item:water-medallion",
            "item:shadow-medallion",
            "item:spirit-medallion",
            "item:light-medallion",
        ]);
        let up = |bridge: &str, have: &Have| {
            let on = crate::settings::value_word(crate::settings::BRIDGE, bridge);
            let world = world(&romfs, &[&on]);
            let edges = adult_edges(&world, map::MARKET_ADULT, map::GANON_CASTLE);
            assert!(!edges.is_empty());
            edges.iter().any(|e| e.need.met(have, Era::Adult))
        };
        let none = Have::default();
        assert!(up("vanilla", &bosses));
        assert!(!up("vanilla", &medallions) && !up("vanilla", &none));
        assert!(up("six-medallions", &medallions));
        assert!(!up("six-medallions", &bosses) && !up("six-medallions", &none));
        assert!(up("open", &none));
    }

    #[test]
    fn all_six_trials_need_each_trials_items_and_done_only_the_climb() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let climb = [
            "item:magic-meter",
            "item:bow",
            "item:light-arrows",
            "item:master-sword",
        ];
        // One way through each trial: Forest and Shadow share the Hover
        // Boots, Shadow, Fire and Light the Golden Gauntlets, Spirit and
        // Light the Hookshot. The Forest wants Din's Fire or Fire Arrows.
        let trials = [
            "item:hover-boots",
            "item:dins-fire",
            "item:bottle",
            "item:megaton-hammer",
            "item:golden-gauntlets",
            "item:lens-of-truth",
            "item:hookshot",
            "item:mirror-shield",
            "item:bombchus-small",
            "item:longshot",
        ];
        // Bombchus at their default cap, as a new game's settings have it.
        let chus = crate::settings::value_word(crate::settings::BOMBCHUS, "free");
        // Holding every dungeon key, as keys from the start give: the tower's
        // boss door asks for Ganon's Castle's.
        let tower = |trials: &str, have: &Have| {
            let mut have = have.clone();
            crate::keys::SINGLES.iter().for_each(|k| have.add(k));
            let have = &have;
            let on = crate::settings::value_word(crate::settings::TRIALS, trials);
            let world = world(&romfs, &[&on, &chus]);
            let edges = adult_edges(&world, map::INSIDE_GANON_CASTLE, GOAL);
            assert!(!edges.is_empty());
            let goal = world.goal().iter().any(|(_, n)| n.met(have, Era::Adult));
            (edges.iter().any(|e| e.need.met(have, Era::Adult)), goal)
        };
        assert_eq!(tower("done", &holding(&climb)), (true, true));
        assert_eq!(tower("all-six", &holding(&climb)), (false, false));
        let all = [&climb[..], &trials[..]].concat();
        assert_eq!(tower("all-six", &holding(&all)), (true, true));
        // The Longshot is a Hookshot too, so the Hookshot goes with it.
        let hooks = ["item:hookshot", "item:longshot"];
        let drops = (trials.iter().filter(|i| **i != hooks[0])).map(std::slice::from_ref);
        for drop in drops.chain([&hooks[..]]) {
            let less: Vec<&str> = all.iter().copied().filter(|i| !drop.contains(i)).collect();
            assert_eq!(
                tower("all-six", &holding(&less)),
                (false, false),
                "{drop:?}"
            );
        }
    }

    #[test]
    fn ganondorf_waits_for_ganons_castle_boss_key() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        // The trials done, so the climb is all the rest the goal needs.
        let done = crate::settings::value_word(crate::settings::TRIALS, "done");
        let new_game = crate::settings::NewGame {
            on: [done].into(),
            ..crate::settings::NewGame::rings()
        };
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        let n = keys::dungeon_in(flag::INSIDE_GANON_CASTLE).unwrap();
        let climb = || {
            holding(&[
                "item:magic-meter",
                "item:bow",
                "item:light-arrows",
                "item:master-sword",
            ])
        };
        let beaten = |keys: &[String]| {
            let mut have = climb();
            keys.iter().for_each(|k| have.add(k));
            world.goal().iter().any(|(_, n)| n.met(&have, Era::Adult))
        };
        let (boss, ring) = (crate::key_tints::boss(n), crate::key_tints::ring(n));
        assert!(beaten(&[boss.clone(), ring.clone()]));
        assert!(!beaten(&[ring]), "the tower's boss key door");
        // The castle's one small key, for the Light trial's door.
        assert!(!beaten(&[boss]), "the Light trial's small key door");
    }
}
