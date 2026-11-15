//! Resources a chest gives only once Link has what uses them.

use crate::checks::Swap;
use crate::known::{Name, actor, item};

/// Each resource and what fires it.
pub const NEEDS: [(&Name, &Name); 5] = [
    (item::BOMB_BUNDLE_SMALL, item::BOMB_BAG_SMALL),
    (actor::T_BOX_ARROW_BUNDLE_SMALL, item::RANGE_BOW),
    (actor::T_BOX_ARROW_BUNDLE_MEDIUM, item::RANGE_BOW),
    (actor::T_BOX_ARROW_BUNDLE_LARGE, item::RANGE_BOW),
    (actor::T_BOX_DEKU_SEED_BUNDLE, item::RANGE_SLING_SHOT),
];
/// Given instead until then, as the Gerudo Training Ground's chests do.
pub const WITHOUT: &Name = actor::T_BOX_RUPEE_RED;

/// A chest holding `item` drops it once Link has what it needs.
pub fn swap(item: &str) -> Option<Swap> {
    NEEDS
        .iter()
        .find(|(i, _)| *i == item)
        .map(|&(then, check)| Swap {
            check: check.get(),
            then: then.get(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The swap turns the holder into a substitute chest, which would
    /// break a pot.
    #[test]
    fn every_vanilla_resource_sits_in_a_chest() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let held: Vec<_> = checks.iter().filter(|c| swap(&c.item).is_some()).collect();
        assert!(!held.is_empty());
        for c in held {
            assert!(
                c.holder.starts_with("Obj_TBox_A_"),
                "{}: {}",
                c.file,
                c.holder
            );
        }
    }
}
