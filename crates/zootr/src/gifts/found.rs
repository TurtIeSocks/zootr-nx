//! What marks a gift given: the Bool the tracker reads for it, and the one
//! the seed adds and sets for a gift the game marks with nothing.

use std::collections::BTreeSet;

use anyhow::Result;
use ounce_formats::{Romfs, Value};

use super::*;
use crate::flows::Model;

impl Gift {
    /// The GameData Bool set once it's given: a cutscene's played flag, an
    /// NPC's own, or one the seed adds. Biggoron sets none.
    pub fn found(&self) -> Option<String> {
        if let Some((.., own)) = OWN.iter().find(|(flow, ..)| *flow == self.flow) {
            return Some((*own).into());
        }
        if self.is_pedestal() {
            return Some(pedestal::GOT.into());
        }
        if let Some(added) = self.added() {
            return Some(added);
        }
        let flag = |found: &Name| match found == SCENE_PLAYED {
            true => played_flag(self.flow),
            false => found.to_string(),
        };
        match self.how {
            How::Calls { found, .. } | How::Sale { found, .. } => found.map(flag),
            How::Later { found, .. } => Some(flag(found)),
            How::Added { .. } => None,
            _ => Some(flag(SCENE_PLAYED)),
        }
    }

    /// The Bool the seed adds and sets right after its grant, for a gift
    /// the game marks with nothing of its own.
    fn added(&self) -> Option<String> {
        let own = OWN.iter().any(|(flow, ..)| *flow == self.flow);
        let none = matches!(self.how, How::Calls { found: None, .. });
        (none && !own).then(|| format!("Rando_Gift_{:016x}", self.hash()))
    }
}

/// The Bools the gifts' edits add, each set once its gift is given: the
/// gift's flow, its setting, the Bool.
const OWN: [(&Name, &str, &str); 3] = [
    (MEDIGORON, MERCHANTS, MEDIGORON_SOLD),
    (flow::SAGE_OF_WATER, NPC_GIFTS, zora::GIVEN),
    (
        actor::NPC_HYLIA_TALON_ADULT_KAKARIKO,
        TRADE_CHAIN,
        crate::trade_chain::TALON_GAVE,
    ),
];

/// The Bools the gifts' edits set, which a new game adds to its save data.
pub fn flags(on: &BTreeSet<String>) -> Vec<String> {
    let own = (OWN.iter())
        .filter(|(_, setting, _)| on.contains(*setting))
        .map(|(.., flag)| (*flag).into());
    let added = GIFTS.iter().filter(|g| g.dealt(on)).filter_map(Gift::added);
    own.chain(added).chain(pedestal::flag(on)).collect()
}

/// Sets each dealt gift's added Bool right after its grants, sharing one
/// Bool setter.
#[derive(Default)]
pub(super) struct Marks(Option<Model>);

impl Marks {
    /// Sets `gift`'s added Bool after each event of `took`, if a new game
    /// with `on` deals it.
    pub(super) fn mark(
        &mut self,
        romfs: &Romfs,
        rewrites: &mut Rewrites,
        gift: &Gift,
        took: &[u16],
        on: &BTreeSet<String>,
    ) -> Result<()> {
        let Some(flag) = gift.added().filter(|_| gift.dealt(on)) else {
            return Ok(());
        };
        let set_bool = match &self.0 {
            Some(s) => s,
            None => self.0.insert(crate::chest_levels::setter(romfs)?),
        };
        flows::edit(romfs, rewrites, gift.flow, |f| {
            (took.iter()).try_for_each(|&at| {
                flows::set_after(f, set_bool, at, &flag, Value::Bool(vec![true]))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::gifts::tests::written;

    #[test]
    fn each_dealt_gift_the_game_marks_with_nothing_sets_its_own_bool() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let on: BTreeSet<String> = [NPC_GIFTS, MERCHANTS, TRADE_CHAIN].map(String::from).into();
        let added: Vec<&Gift> = (GIFTS.iter())
            .filter(|g| g.dealt(&on) && g.added().is_some())
            .collect();
        assert!(added.len() > 10, "{} gifts", added.len());
        // As dealt, then each holding the next one's item, chained by level.
        let next = |i: usize| (added[(i + 1) % added.len()].hash(), added[i].item);
        let rotated = (0..added.len()).map(|i| (next(i).0, Drop::from(next(i).1.to_string())));
        for (drops, progressive) in [(HashMap::new(), false), (rotated.collect(), true)] {
            let mut rewrites = Rewrites::default();
            give_all(
                &romfs,
                &all,
                &mut rewrites,
                &drops,
                crate::progressive::Lines {
                    progressive,
                    ..Default::default()
                },
                &on,
            )
            .unwrap();
            for gift in &added {
                let flag = gift.added().unwrap();
                let flow = flows::render(&written(&rewrites, gift.flow));
                assert!(flow.contains(&flag), "{} sets no {flag}", gift.giver);
            }
        }
        let mut rewrites = Rewrites::default();
        give_all(
            &romfs,
            &all,
            &mut rewrites,
            &HashMap::new(),
            crate::progressive::Lines::default(),
            &Default::default(),
        )
        .unwrap();
        let flag = added[0].added().unwrap();
        let untouched = rewrites.get(&flows::rel(added[0].flow));
        assert!(untouched.is_none_or(|b| {
            !flows::render(&Flowchart::parse(b).unwrap().unwrap()).contains(&flag)
        }));
    }
}
