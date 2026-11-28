//! The Water Temple's mouth at the bottom of Lake Hylia.
//!
//! An air wall stands over it while the Water sage's quest is short of past a
//! step, which is how vanilla keeps Link out until he's done Ice Cavern. A
//! seed that deals the Hookshot before that leaves the mouth walled while
//! logic already promises it to `iron-boots and zora-tunic and hookshot`
//! (`logic/exits.toml`), so the temple can't be opened at all: the shutter
//! behind the wall opens only on the chain demo, and the demo needs the switch
//! Link hooks, which he can't reach through the wall (issue #560).
//!
//! So the wall goes, gated on a Bool nothing sets, the way a quiet enemy is
//! (`crate::enemies`). The shutter stays shut until Link hooks the switch, and
//! the switch takes his control only while he wears the Iron Boots, so the
//! entrance keeps exactly the need its logic rule names. The quest is left
//! alone: fifteen other edges read it, from the King Zora talk to the Ice
//! Cavern arrival.
//!
//! The child lake places the same wall over the same spot. It stays: there's
//! no door to reach there, and no Iron Boots for a child to swim down with.

use anyhow::Result;
use ounce_formats::Byml;

use crate::checks::{actors_mut, gate, hash_of};

/// The Bool the wall waits on. Nothing sets it.
const FLAG: &str = "ZootrWaterTempleWall";
/// The wall over the mouth, in the adult lake.
const WALL: u64 = 2935529029587700987;

/// Take the wall out of `root`, a map's placements. Returns whether this map
/// held it.
pub fn open(root: &mut Byml) -> Result<bool> {
    for actor in actors_mut(root)? {
        if hash_of(actor) != Some(WALL) {
            continue;
        }
        // Whatever it stood on before, it never stands now.
        actor.as_mut_map()?.remove("Presence");
        gate(actor, FLAG, false)?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use ounce_formats::Romfs;

    use super::*;
    use crate::checks::{actors, banc_files, load_banc};
    use crate::game::dev_dump;
    use crate::known::{Name, flag, map};
    use crate::world::presence_of;

    /// The lake's map file for `era` (`Adult`, `Child`).
    fn lake(romfs: &Romfs, era: &Name) -> String {
        let name = format!("{}_Dynamic", era.get());
        (banc_files(romfs).unwrap().into_iter())
            .find(|rel| rel.contains(&name))
            .unwrap()
    }

    /// The wall's gate in `root`, shown as the game reads it, or `None` where
    /// this map has no wall of ours.
    fn wall(root: &Byml) -> Option<String> {
        let actor = actors(root)
            .unwrap()
            .iter()
            .find(|a| hash_of(a) == Some(WALL))?;
        Some(presence_of(actor).unwrap().to_string())
    }

    #[test]
    fn the_wall_over_the_adult_mouth_goes() {
        let Some(romfs) = dev_dump() else { return };
        let rel = lake(&romfs, map::HYRULE_FIELD_ADULT);
        let mut root = load_banc(&romfs, &rel).unwrap().root;
        assert_eq!(
            wall(&root),
            Some(format!("!{}", flag::SAGE_OF_WATER_IS_AFTER_STEP03_EXP)),
            "{rel}"
        );
        assert!(open(&mut root).unwrap());
        assert_eq!(wall(&root), Some(FLAG.into()));
    }

    #[test]
    fn the_child_lakes_copy_of_the_wall_stays() {
        let Some(romfs) = dev_dump() else { return };
        let rel = lake(&romfs, map::HYRULE_FIELD_CHILD);
        let mut root = load_banc(&romfs, &rel).unwrap().root;
        assert!(!open(&mut root).unwrap(), "{rel}");
    }
}
