//! What Link wears under water: the Iron Boots to sink, and the Zora
//! Tunic to breathe there (without it he drowns in about 20 s). Put on and
//! taken off from the menu.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, ensure};

use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::known::{Name, actor};
use crate::live::{self, Memory};
use crate::play::Input;

/// The tunic Link wears, by its armor's name (a String64).
const TUNIC: &str = "#008d7f0c";

/// The buttons, each with the wait after it, that put a just-given item
/// on: the menu opens on the newest item, and A puts a ZR item on ZR or
/// gear on or off. The item ring never selected a bottle (12 pushes).
pub const EQUIP: [(&str, Duration); 3] = [
    ("Plus", Duration::from_millis(2500)),
    ("A", Duration::from_millis(500)),
    ("Plus", Duration::from_secs(3)),
];

/// What Link wears now: whether the Iron Boots are on, and the tunic.
pub fn worn<M: Memory>(dev: &Dev<M>) -> Result<(bool, String)> {
    let (_, at, len) = live::locate(dev.game(), dev.global(), live::hash(TUNIC)?)?;
    let tunic = dev.game().read(at, len)?;
    let tunic = tunic.split(|&b| b == 0).next().unwrap_or_default();
    Ok((dev.boots()?, String::from_utf8_lossy(tunic).into_owned()))
}

/// The gear to put on or take off from the menu to `sink` or not, given
/// what Link wears. A in the menu toggles the item under the cursor, and
/// a give moves the cursor to it, so each one listed is a give and one
/// press (other machine, 2026-11-17). Writing what he wears reads back
/// set but changes nothing. The tunic goes first: a give at the floor,
/// boots on, can fail. The Zora Tunic stays on after, as nothing needs
/// it off.
pub fn toggles(sink: bool, (boots, tunic): &(bool, String)) -> Vec<&'static Name> {
    let mut toggle = Vec::new();
    if sink && *tunic != **actor::ARMOR_ADULT_ZORA {
        toggle.push(actor::ARMOR_ADULT_ZORA);
    }
    if sink != *boots {
        toggle.push(actor::ARMOR_ADULT_HEAVY_BOOT);
    }
    toggle
}

/// Give `item` and press it in the menu: a ZR item goes on ZR, and gear
/// goes on or comes off.
pub fn equip<M: Memory, I: Input>(
    dev: &Dev<M>,
    gd: &GameData,
    input: &mut I,
    item: &Name,
) -> Result<()> {
    dev.give(gd, item, 1)?;
    for (button, wait) in EQUIP {
        input.press(button)?;
        sleep(wait);
    }
    Ok(())
}

/// Put the Iron Boots on, with the Zora Tunic, to `sink`, else take them
/// off.
pub fn wear<M: Memory, I: Input>(
    dev: &Dev<M>,
    gd: &GameData,
    input: &mut I,
    sink: bool,
) -> Result<()> {
    for armor in toggles(sink, &worn(dev)?) {
        equip(dev, gd, input, armor)?;
    }
    let (boots, _) = worn(dev)?;
    let stayed = if boots { "on" } else { "off" };
    ensure!(boots == sink, "the Iron Boots stayed {stayed}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
