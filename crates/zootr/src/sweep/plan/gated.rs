//! What a gated holder waits on: the values that raise it, the NPCs whose
//! flags move them out of its way, and what Link wields at it first.

use anyhow::Result;

use super::{NPC, Planner, Raise, Ready, in_map, lazy, quests};
use crate::events::{Effect, Flows};
use crate::flags;
use crate::gates;
use crate::generate::Spot;
use crate::known::{Name, field, flow};
use crate::sweep::wield::{self, Wield};
use crate::warp;

/// The actors `expr` reads, by name: a switch, an enemy beaten.
fn drivers(expr: &gates::Expr) -> Vec<Name> {
    let actor = |a: &gates::Atom| match a {
        gates::Atom::Actor { name, .. } => Some(Name(crate::known::hash(name))),
        _ => None,
    };
    expr.atoms().into_iter().filter_map(actor).collect()
}

impl Planner<'_> {
    /// The GameData values and quest steps that raise the chest or boss
    /// heart container with placement hash `hash`, and the age it rises
    /// for when only one can open it.
    pub(super) fn appear(&mut self, hash: u64) -> Result<Raise> {
        if self.appear.is_none() {
            let gated = [gates::trace(self.romfs)?, gates::hearts(self.romfs)?].concat();
            self.drivers = (gated.iter())
                .map(|g| (g.hash, drivers(&g.expr)))
                .filter(|(_, d)| !d.is_empty())
                .collect();
            self.foes = (gated.iter())
                .map(|g| (g.hash, g.expr.foes()))
                .filter(|(_, f)| !f.is_empty())
                .collect();
            let gd = self.gamedata()?;
            let raise = |g: gates::Gate| {
                let steps = quests(gd, &g.expr);
                (g.hash, (g.expr.opening(), steps, g.expr.age()))
            };
            self.appear = Some(gated.into_iter().map(raise).collect());
        }
        Ok((self.appear.as_ref())
            .and_then(|a| a.get(&hash).cloned())
            .unwrap_or_default())
    }

    /// What Link wields before he uses the holder of `spot` at `face` in
    /// `map`: a pour on red ice it's frozen in, else a pound on a pillar
    /// pinning down a switch its gate reads. Needs [`Planner::appear`] run
    /// first, for the switches.
    pub(super) fn wield(&self, spot: &Spot, map: &str, face: [f32; 3]) -> Result<Option<Wield>> {
        let of = |by| in_map(by, &spot.area, map);
        if let Some(melt) = wield::melt(of(&self.ice), face) {
            return Ok(Some(melt));
        }
        let Some(drivers) = self.drivers.get(&spot.hash) else {
            return Ok(None);
        };
        let drivers: Vec<&Name> = drivers.iter().collect();
        let switches = warp::spots_in(self.romfs, &spot.area, &drivers)?;
        let switches = switches.get(map).map_or(&[][..], Vec::as_slice);
        Ok(wield::pound(of(&self.pillars), switches))
    }

    /// The actor delete flags play sets along with `ready`'s, through the
    /// flows of the NPCs that set them. A jailed Fire Temple Goron's talk
    /// sets his delete flag, which unlocks the chest behind him, and
    /// his rescue sets another. Without that one
    /// he still sits in front of the chest after a load.
    pub(super) fn leaving(&mut self, ready: &[String]) -> Result<Ready> {
        if ready.is_empty() {
            return Ok(vec![]);
        }
        let flows = lazy(&mut self.flows, || Flows::load(self.romfs))?;
        let mut out: Ready = vec![];
        for flag in ready {
            let setters = (flows.charts.iter()).filter(|(name, chart)| {
                name.starts_with(NPC) && flags::set_in(chart).any(|f| f == flag)
            });
            for (setter, _) in setters {
                let deletes = flows.effects(setter).unwrap_or_default();
                for gone in deletes.iter().filter_map(deleted) {
                    if !out.contains(&gone) && !ready.contains(&gone) {
                        out.push(gone);
                    }
                }
            }
        }
        Ok(out)
    }
}

/// The actor delete flag `effect` sets, if it sets one: a name ending
/// `_Delete`, set true.
fn deleted(effect: &Effect) -> Option<String> {
    let set = effect.action == flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL
        && effect.params.contains("Value=true");
    let (_, rest) = effect
        .params
        .split_once(&format!("{}=\"", field::GAME_DATA_NAME))?;
    let (name, _) = rest.split_once('"')?;
    (set && name.ends_with("_Delete")).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::tests::seed;
    use super::super::{Trigger, plan};
    use super::*;
    use crate::known::{actor, flag};

    #[test]
    fn a_chest_that_rises_is_planned_with_the_flag_that_raises_it() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        // Dodongo's Cavern's Red Rupee chest, risen once the Lizalfos fall:
        // they're beaten for real, and the flag is held back for a kill
        // that doesn't take.
        let dodongo = planned.iter().find(|p| p.spot.hash == 7764016096998075927);
        let Some(Trigger::Open {
            ready, foes, latch, ..
        }) = dodongo.map(|p| &p.trigger)
        else {
            panic!("{dodongo:?}");
        };
        assert_eq!(latch, &[flag::GIMMICK_DEFEAT_ENEMY_OPEN_DODONGO_CAVE_01]);
        assert!(ready.is_empty() && !foes.is_empty(), "{foes:?}");
        let plain = planned.iter().filter_map(|p| match &p.trigger {
            Trigger::Open { ready, .. } => Some(ready.is_empty()),
            _ => None,
        });
        assert!(
            plain.filter(|&p| p).count() > 50,
            "most chests just sit there"
        );
    }

    #[test]
    fn the_fire_temple_boss_key_waits_for_its_goron_to_leave() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // The boss key chest's gate holds his talk's flag. His rescue takes
        // him out of the way.
        let mut planner = Planner::new(&romfs, BTreeSet::new());
        let gate = [flag::NPC_GORON_010_DELETE.to_string()];
        let leaving = planner.leaving(&gate).unwrap();
        assert_eq!(leaving, [flag::NPC_GORON_010_ADULT_FIRE_TEMPLE_DELETE]);
    }

    #[test]
    fn a_boss_heart_container_waits_for_its_quest_past_the_boss() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut planner = Planner::new(&romfs, BTreeSet::new());
        let mut raised = |hash| planner.appear(hash).unwrap();
        // Read live from the quest system: past Step07_2 is Step08.
        let water = raised(14426685265732353862);
        assert_eq!(
            water.1,
            [(flow::SAGE_OF_WATER.to_string(), "Step08".to_owned())]
        );
        // Through a BoolExp, the Spirit sage's "after step 9" one.
        let spirit = raised(4720088048159616469);
        assert_eq!(
            spirit.1,
            [(flow::SAGE_OF_SPIRIT.to_string(), "Step10".to_owned())]
        );
        // A plain flag.
        let fire = raised(9478330070473364248);
        assert_eq!(
            (fire.0, fire.1),
            (
                vec![flag::FIRE_TEMPLE_BOSS_GROUND_CHANGE.to_string()],
                vec![]
            )
        );
        for g in gates::hearts(&romfs).unwrap() {
            let (ready, steps, _) = raised(g.hash);
            assert!(
                !ready.is_empty() || !steps.is_empty(),
                "{} raises on nothing",
                g.area
            );
        }
    }

    #[test]
    fn holders_frozen_in_red_ice_melt_it_first() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let frozen: BTreeSet<u64> = (planned.iter())
            .filter(|p| matches!(&p.trigger, Trigger::Open { wield: Some(w), .. } if w.item == actor::ITEM_BOTTLE_BLUE_FIRE))
            .map(|p| p.spot.hash)
            .collect();
        // The Ice Cavern's two pots and its map chest.
        let cavern = [
            3105481948394149361,
            13545259034644162992,
            11498802739257120175,
        ];
        assert!(cavern.iter().all(|h| frozen.contains(h)), "{frozen:?}");
        assert!(frozen.len() < 10, "{frozen:?}");
    }

    #[test]
    fn a_chest_whose_switch_a_pillar_pins_has_the_hammer_pound_it_first() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let chest = |hash| Spot {
            hash,
            area: crate::known::map::GERUDO_TRAINING_GROUND.to_string(),
            holder: actor::OBJ_T_BOX_A_01.to_string(),
            ..Default::default()
        };
        // The Spirit Trial's chest ringed by flames, and one an eye
        // switch opens.
        let planned = plan(
            &romfs,
            &[chest(4372363658229299619), chest(2436665354602612279)],
        );
        let wields: Vec<_> = (planned.unwrap().into_iter())
            .map(|p| match p.trigger {
                Trigger::Open { wield, .. } => wield.map(|w| (w.item, w.on.map(f32::round))),
                other => panic!("{other:?}"),
            })
            .collect();
        let pillar = [52.0, 98.0, -86.0];
        assert_eq!(wields, [Some((crate::known::item::HAMMER, pillar)), None]);
    }
}
