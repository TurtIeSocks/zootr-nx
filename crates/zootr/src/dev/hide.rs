//! Hiding a live actor's model and showing it again, with no reload: our
//! bit in the model's hide mask (`live::actors::hide`). Collision stays.
//! A map load makes every actor again, so a hide lasts until then.

use std::collections::BTreeSet;
use std::fmt;

use anyhow::Result;
use serde::Serialize;

use super::{Dev, Lines, distance};
use crate::live::Memory;
use crate::live::actors::{self, Actor, Hidden};

/// A live actor some placement made, and how far it is from Link.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Near {
    pub hash: u64,
    pub name: String,
    pub at: [f32; 3],
    pub distance: f32,
    pub hidden: Hidden,
}

impl fmt::Display for Near {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Near {
            hash,
            name,
            distance,
            hidden,
            ..
        } = self;
        write!(f, "{distance:>6.1} m  {name}  {hash}")?;
        match hidden {
            Hidden::Shown => Ok(()),
            Hidden::Ours => f.write_str("  hidden"),
            Hidden::Game => f.write_str("  hidden by the game"),
        }
    }
}

/// The actors in `all` a placement made, nearest `link` first.
fn near(all: Vec<Actor>, link: [f32; 3]) -> Vec<Near> {
    let mut found: Vec<Near> = (all.into_iter())
        .filter_map(|a| {
            Some(Near {
                hash: a.hash?,
                distance: distance(link, a.pos),
                name: a.name,
                at: a.pos,
                hidden: a.hidden,
            })
        })
        .collect();
    found.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    found
}

/// The actors in `all` of the placements in `hashes`.
fn picked<'a>(all: &'a [Actor], hashes: &BTreeSet<u64>) -> Vec<&'a Actor> {
    (all.iter())
        .filter(|a| a.hash.is_some_and(|h| hashes.contains(&h)))
        .collect()
}

impl<M: Memory> Dev<M> {
    /// The live actors placements made, nearest Link first.
    pub fn nearby(&self) -> Result<Lines<Near>> {
        let link = self.link()?;
        Ok(Lines(near(actors::actors(&self.game, self.main)?, link)))
    }

    /// Hide the actors of these placements, or show them again. Says which
    /// placements had a live actor: the rest may not have loaded yet. One
    /// with no model refuses before anything is written.
    pub fn hide(&self, hashes: &[u64], on: bool) -> Result<Vec<u64>> {
        let all = actors::actors(&self.game, self.main)?;
        let wanted: BTreeSet<u64> = hashes.iter().copied().collect();
        let picked = picked(&all, &wanted);
        if let Some(a) = picked.iter().find(|a| a.model.is_none()) {
            anyhow::bail!("{} has no model to hide", a.name);
        }
        for a in &picked {
            actors::hide(&self.game, a, on)?;
        }
        let found: BTreeSet<u64> = picked.iter().filter_map(|a| a.hash).collect();
        Ok(hashes
            .iter()
            .copied()
            .filter(|h| found.contains(h))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str, x: f32, hash: Option<u64>) -> Actor {
        Actor {
            at: 0,
            name: name.into(),
            pos: [x, 0.0, 0.0],
            hash,
            model: Some(0x10),
            hidden: Hidden::Shown,
        }
    }

    #[test]
    fn nearby_keeps_placed_actors_nearest_first() {
        let all = vec![
            actor("Obj_FakeFar", 9.0, Some(1)),
            actor("FakeEffect", 0.5, None),
            actor("Obj_FakeNear", -2.0, Some(2)),
        ];
        let found = near(all, [0.0; 3]);
        let named: Vec<_> = found
            .iter()
            .map(|n| (n.name.as_str(), n.distance))
            .collect();
        assert_eq!(named, [("Obj_FakeNear", 2.0), ("Obj_FakeFar", 9.0)]);
    }

    #[test]
    fn a_hide_picks_every_actor_of_its_placements() {
        let all = vec![
            actor("Obj_FakeA", 0.0, Some(1)),
            actor("Obj_FakeB", 0.0, Some(2)),
            actor("Obj_FakeA", 1.0, Some(1)),
            actor("FakeEffect", 0.0, None),
        ];
        let picked = picked(&all, &BTreeSet::from([1, 3]));
        assert_eq!(picked.len(), 2);
        assert!(picked.iter().all(|a| a.hash == Some(1)));
    }

    #[test]
    fn a_line_says_who_hid_it() {
        let mut n = near(vec![actor("Obj_FakeA", 3.0, Some(7))], [0.0; 3]).remove(0);
        assert_eq!(n.to_string(), "   3.0 m  Obj_FakeA  7");
        n.hidden = Hidden::Ours;
        assert!(n.to_string().ends_with("  hidden"));
        n.hidden = Hidden::Game;
        assert!(n.to_string().ends_with("hidden by the game"));
    }
}
