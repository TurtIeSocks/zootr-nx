//! What Link wears for a spot: the Iron Boots and the Zora Tunic for a
//! holder logic puts underwater, nothing extra for the rest.

use anyhow::Result;

use crate::dev::Dev;
use crate::known::{Name, actor, item};
use crate::live;

/// The Iron Boots on (a Bool): Link sinks, and the HUD shows them. A
/// member of `PlayerStatus` no name table names, found by putting them on
/// from the menu (Mac, 2026-11-17).
const BOOTS_ON: &str = "#a6719419";
/// The tunic Link wears, by its armor's name (a String64). The Zora
/// Tunic's lets him breathe underwater, though his model may stay as it
/// was.
const TUNIC: &str = "#008d7f0c";

/// What a sinking spot gives before it puts them on.
pub(super) const UNDERWATER: [&Name; 2] = [actor::ARMOR_ADULT_HEAVY_BOOT, actor::ARMOR_ADULT_ZORA];

/// Whether a spot's holder lies underwater: its rule names the Iron Boots
/// and an adult sweeps it. Logic says so for the Water Temple's fourth
/// key chest and Hyrule Field's grotto pond crates, whose sweep rows had
/// Link swimming at the surface over them, 150 tries and never a pass.
pub(super) fn sinks(need_items: &[String], child: Option<bool>) -> bool {
    child != Some(true) && need_items.iter().any(|i| **item::HEAVY_BOOT == **i)
}

/// Put the Iron Boots on and the Zora Tunic for a spot that `sink`s, so a
/// warp stands Link on its floor and he breathes there. Boots off for every
/// other spot, or he'd sink where he should swim.
pub(super) fn wear(dev: &Dev, sink: bool) -> Result<()> {
    let (game, global) = (dev.game(), dev.global());
    if sink {
        live::set(game, global, live::hash(TUNIC)?, actor::ARMOR_ADULT_ZORA)?;
    }
    live::set(
        game,
        global,
        live::hash(BOOTS_ON)?,
        if sink { "true" } else { "false" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_adult_spot_whose_rule_names_the_iron_boots_sinks() {
        let boots = vec![item::HEAVY_BOOT.to_string(), "Item_FakeBow".to_string()];
        assert!(sinks(&boots, Some(false)));
        assert!(sinks(&boots, None));
        assert!(!sinks(&boots, Some(true)), "a child can't wear them");
        assert!(!sinks(&["Item_FakeBow".to_string()], None));
    }
}
