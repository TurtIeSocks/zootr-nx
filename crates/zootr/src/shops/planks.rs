//! How high each shop slot's shelf lies above the slot's placement.
//!
//! The goods on a shelf don't rest on it: each floats or sinks by its own
//! pose, so a stand-in can't take its height from the goods it replaces.
//! These were measured in the game instead. A red rupee was lifted 0.4 m
//! over every slot, and the gap down to the plank was read against the
//! rupee's own height on screen. Good to about 3 cm.
//!
//! A slot hidden at the time (behind a sold-out board or the keeper's
//! hand) takes its row's other planks.
//!
//! The table holds each measured rise plus the rupee's floor in its
//! `ActorInfo` row. That row's floor is the rupee in its own pose, which
//! lifts it about 0.2 m. On the shelf it plays another pose, so [`plank`]
//! swaps the row's floor for the one it has there.

use super::SHOPS;
use super::floors::floor;
use crate::known::actor;

/// The rupee's floor in its `ActorInfo` row, which the table carries.
const ROW_FLOOR: f32 = 0.0072;

/// Each shop's planks, metres above the placement, by slot number from 1.
const PLANKS: [[f32; 8]; SHOPS.len()] = [
    // Kokiri Shop
    [0.17, 0.17, 0.16, 0.18, 0.18, 0.19, 0.17, 0.16],
    // Market Bazaar (7 hidden)
    [0.16, 0.17, 0.20, 0.18, 0.15, 0.14, 0.17, 0.15],
    // Market Potion Shop (7 and 8 hidden)
    [0.10, 0.09, 0.14, 0.09, 0.15, 0.10, 0.11, 0.11],
    // Bombchu Shop
    [0.18, 0.11, 0.18, 0.11, 0.21, 0.16, 0.19, 0.18],
    // Kakariko Bazaar
    [0.19, 0.16, 0.18, 0.13, 0.16, 0.18, 0.12, 0.18],
    // Kakariko Potion Shop (4 hidden)
    [0.15, 0.17, 0.14, 0.17, 0.13, 0.17, 0.15, 0.17],
    // Goron Shop
    [0.19, 0.27, 0.27, 0.18, 0.18, 0.25, 0.18, 0.20],
    // Zora Shop (3 hidden behind the keeper's hand)
    [0.21, 0.25, 0.19, 0.16, 0.25, 0.21, 0.25, 0.18],
];

/// How high the shelf under slot `number` of `shop` lies above the slot's
/// placement. `None` for a shop or slot the table doesn't hold.
pub fn plank(shop: &str, number: usize) -> Option<f32> {
    let at = SHOPS.iter().position(|s| *s == shop)?;
    let measured = PLANKS[at].get(number.checked_sub(1)?)?;
    Some(measured - ROW_FLOOR + floor(actor::T_BOX_RUPEE_RED)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_finds_its_shops_plank_and_the_rupee_keeps_its_measured_rise() {
        needs_names!();
        let red = floor(actor::T_BOX_RUPEE_RED).unwrap();
        let rise = |shop, number| plank(shop, number).unwrap() - red;
        assert!((rise("Goron Shop", 2) - (0.27 - ROW_FLOOR)).abs() < 1e-6);
        assert!((rise("Kokiri Shop", 8) - (0.16 - ROW_FLOOR)).abs() < 1e-6);
        assert_eq!(plank("Kokiri Shop", 0), None);
        assert_eq!(plank("Kokiri Shop", 9), None);
        assert_eq!(plank("Fake Shop", 1), None);
    }
}
