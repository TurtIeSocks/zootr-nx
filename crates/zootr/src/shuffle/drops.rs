//! What each check's actor gets written: its drop, and which checks need
//! one at all.

use std::collections::HashMap;

use super::Placement;
use crate::checks::{Drop, ItemCheck};
use crate::progressive::Lines;
use crate::{ammo, gifts};

/// What a chest holding `item` drops. The one place a drop is decided.
/// Lines need nothing here: the hook upgrades them in the game.
pub fn drop(item: &str, lines: Lines) -> Drop {
    match ammo::swap(item, lines) {
        Some(swap) => Drop {
            item: ammo::WITHOUT.to_string(),
            swap: Some(swap),
        },
        None => Drop::from(item.to_owned()),
    }
}

/// The drops a mod has to write, shuffled or not: chests whose item
/// changed, whose drop swaps, or whose vanilla swap needs clearing.
pub fn drops(checks: &[ItemCheck], placements: &[Placement], lines: Lines) -> HashMap<u64, Drop> {
    let entry = |c: &ItemCheck, item: &str, placed: bool| {
        let d = if !gifts::is_gift(c) {
            drop(item, lines)
        } else if gifts::is_prize(c) {
            // A minigame's chest can't be a substitute chest.
            Drop::from(ammo::copy(item, lines).unwrap_or_else(|| item.to_owned()))
        } else {
            // The deal keeps swaps off gifts.
            Drop::from(item.to_owned())
        };
        // A placed trap chest turns plain even holding its own Red Rupee:
        // left a trap, it would break and never give the check.
        let trap = placed && c.holder == **crate::checks::TRAP_CHEST;
        let changed = d.swap.is_some() || item != c.item || c.substitute.is_some() || trap;
        changed.then_some((c.hash, d))
    };
    let unmoved = checks.iter().filter_map(|c| entry(c, &c.item, false));
    let placed = placements
        .iter()
        .filter_map(|p| entry(p.check, p.item, true));
    // Placements come last so they win.
    unmoved.chain(placed).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Swap;
    use crate::keys;
    use crate::known::flag;
    use crate::narrowing::tests::chest;
    use crate::settings::{self, NewGame};
    use crate::shuffle::tests::{game, holding, line, with_keys};
    use crate::shuffle::{Fixed, shuffle};

    #[test]
    fn a_key_chest_that_changes_gets_a_drop_and_one_that_doesnt_is_left() {
        needs_names!();
        let (checks, none) = (with_keys(), Fixed::new());
        let on = NewGame {
            keys: settings::DungeonKeys::Start,
            ..game(false, &[])
        };
        let p = shuffle(&checks, 7, &none, &on).unwrap();
        let d = drops(&checks, &p, Lines::default());
        for (hash, item) in [(3, keys::SMALL), (4, keys::BOSS)] {
            let now = holding(&p, hash);
            assert_eq!(d.contains_key(&hash), now != item);
        }
        let p = shuffle(&checks, 7, &none, &game(false, &[])).unwrap();
        assert!(drops(&checks, &p, Lines::default()).is_empty());
    }

    /// A minigame's chest can't turn into a substitute one, so it drops
    /// the ammo's Red Rupee copy, with no swap.
    #[test]
    fn a_prize_chest_drops_ammos_copy() {
        needs_names!();
        let prize = ItemCheck {
            hash: gifts::PRIZE_CHESTS[0],
            ..chest(1, "Item_A")
        };
        for (ammo, _) in ammo::NEEDS {
            let placed = [Placement {
                check: &prize,
                item: ammo,
            }];
            let lines = Lines::widest(false);
            let drops = drops(std::slice::from_ref(&prize), &placed, lines);
            let want = Drop::from(ammo::copy(ammo, lines).unwrap());
            assert_eq!(drops.get(&prize.hash), Some(&want), "{ammo}");
        }
    }

    #[test]
    fn a_placed_trap_chest_is_dealt_even_holding_its_own_item() {
        let trap = ItemCheck {
            holder: crate::checks::TRAP_CHEST.to_string(),
            ..chest(1, "Item_A")
        };
        let placed = [Placement {
            check: &trap,
            item: "Item_A",
        }];
        let d = drops(std::slice::from_ref(&trap), &placed, Lines::default());
        assert_eq!(d.get(&1).map(|d| d.item.as_str()), Some("Item_A"));
        // Unplaced (chests left vanilla), it stays the trap.
        assert!(drops(std::slice::from_ref(&trap), &[], Lines::default()).is_empty());
    }

    #[test]
    fn an_unmoved_line_chest_is_left_to_the_hook() {
        let ([_, (long, _)], none) = (line(0), Fixed::new());
        let race = ItemCheck {
            get_event: Some(flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX.to_string()),
            ..chest(3, long)
        };
        let checks = [chest(1, long), chest(2, "Item_B"), race];
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        assert!(p.iter().all(|p| p.check.hash != 3));
        assert!(drops(&checks, &p, Lines::default()).is_empty());
    }

    #[test]
    fn a_vanilla_substitute_chest_keeps_its_item_and_loses_its_swap() {
        needs_names!();
        let owned = ItemCheck {
            substitute: Some("Item_B".into()),
            ..chest(1, "Item_A")
        };
        let (checks, none) = ([owned, chest(2, "Item_B")], Fixed::new());
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        assert_eq!(p.len(), 2);
        let d = drops(&checks, &p, Lines::default());
        assert_eq!(d, HashMap::from([(1, Drop::from("Item_A".to_owned()))]));
    }

    #[test]
    fn only_ammo_swaps() {
        needs_names!();
        let [(hook, _), _] = line(0);
        let (bombs, bag) = ammo::NEEDS[0];
        let swap = Swap {
            check: bag,
            then: bombs,
        };
        assert_eq!(
            drop(bombs, Lines::default()),
            Drop {
                item: ammo::WITHOUT.to_string(),
                swap: Some(swap)
            }
        );
        for item in [hook, "Item_A"] {
            assert_eq!(drop(item, Lines::default()), Drop::from(item.to_owned()));
        }
    }

    #[test]
    fn an_unshuffled_bomb_chest_gives_a_rupee_until_link_has_the_bag() {
        needs_names!();
        let (bombs, none) = (ammo::NEEDS[0].0, Fixed::new());
        let checks = [chest(1, bombs), chest(2, "Item_B")];
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        let d = drops(&checks, &p, Lines::default());
        assert_eq!(d, HashMap::from([(1, drop(bombs, Lines::default()))]));
        assert_eq!(d[&1].item, ammo::WITHOUT);
    }
}
