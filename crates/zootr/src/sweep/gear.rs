//! What Link wears for a spot: the Iron Boots and the Zora Tunic for a
//! holder logic puts underwater, nothing extra for the rest.

use anyhow::Result;

use crate::dev::Dev;
use crate::known::{Name, actor, item};
use crate::live::{self, Memory};

/// The Iron Boots on (a Bool): Link sinks, and the HUD shows them. A
/// member of `PlayerStatus` no name table names, found by putting them on
/// from the menu (Mac, 2026-11-17).
const BOOTS_ON: &str = "#a6719419";
/// The tunic Link wears, by its armor's name (a String64).
const TUNIC: &str = "#008d7f0c";

/// Whether a spot's holder lies underwater: its rule names the Iron Boots
/// and an adult sweeps it. Logic says so for the Water Temple's fourth
/// key chest and Hyrule Field's grotto pond crates, whose sweep rows had
/// Link swimming at the surface over them, 150 tries and never a pass.
pub(super) fn sinks(need_items: &[String], child: Option<bool>) -> bool {
    child != Some(true) && need_items.iter().any(|i| **item::HEAVY_BOOT == **i)
}

/// What Link wears now: whether the Iron Boots are on, and the tunic.
pub(super) fn worn(dev: &Dev) -> Result<(bool, String)> {
    let read = |name| -> Result<Vec<u8>> {
        let (_, at, len) = live::locate(dev.game(), dev.global(), live::hash(name)?)?;
        dev.game().read(at, len)
    };
    let boots = read(BOOTS_ON)?.first().is_some_and(|b| b & 1 == 1);
    let tunic = read(TUNIC)?;
    let tunic = tunic.split(|&b| b == 0).next().unwrap_or_default();
    Ok((boots, String::from_utf8_lossy(tunic).into_owned()))
}

/// The gear to put on or take off from the menu for a spot that does or
/// doesn't `sink`, given what Link wears. A in the menu toggles the item
/// under the cursor, and a give moves the cursor to it, so each one
/// listed is a give and one press (other machine, 2026-11-17). Writing
/// what he wears reads back set but changes nothing. The tunic goes
/// first: a give at the floor, boots on, can fail. The Zora Tunic stays
/// on after, as no spot needs it off.
pub(super) fn toggles(sink: bool, (boots, tunic): &(bool, String)) -> Vec<&'static Name> {
    let mut toggle = Vec::new();
    if sink && *tunic != **actor::ARMOR_ADULT_ZORA {
        toggle.push(actor::ARMOR_ADULT_ZORA);
    }
    if sink != *boots {
        toggle.push(actor::ARMOR_ADULT_HEAVY_BOOT);
    }
    toggle
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

    #[test]
    fn only_gear_that_must_change_is_toggled_tunic_first() {
        let zora = actor::ARMOR_ADULT_ZORA.to_string();
        let plain = "Armor_Adult_FakeTunic".to_string();
        let both = vec![actor::ARMOR_ADULT_ZORA, actor::ARMOR_ADULT_HEAVY_BOOT];
        assert_eq!(toggles(true, &(false, plain.clone())), both);
        assert_eq!(
            toggles(true, &(false, zora.clone())),
            [actor::ARMOR_ADULT_HEAVY_BOOT]
        );
        assert!(
            toggles(true, &(true, zora.clone())).is_empty(),
            "a second A would take them off"
        );
        assert_eq!(
            toggles(false, &(true, zora.clone())),
            [actor::ARMOR_ADULT_HEAVY_BOOT]
        );
        assert!(toggles(false, &(false, plain)).is_empty());
        assert!(
            toggles(false, &(false, zora)).is_empty(),
            "the tunic stays on"
        );
    }
}
