//! What the shuffle may deal, and where: the checks it leaves out and the
//! items a check refuses, each with its reason. The shuffle only asks
//! whether there's a reason. The ledger reports which, numbered as in the
//! EIE spec's list of narrowings.

use crate::checks::ItemCheck;
use crate::gifts::levels;
use crate::known::{Name, item};
use crate::settings::{self, NewGame};
use crate::{cows, dampe, gifts, keys, pickups, scrubs, shops};

/// One narrowing from the EIE spec's list: #1, #4 and #8.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Narrowing {
    /// A key copy at a holder whose get the hook can't credit.
    Uncredited,
    /// The holder can't ask the pouch which level of a line, or which ammo,
    /// to give.
    AsksPouch,
    /// A minigame's own event picks what it gives.
    Minigame,
}

impl Narrowing {
    /// Its number in the EIE spec's list.
    pub fn number(self) -> u8 {
        match self {
            Self::Uncredited => 1,
            Self::AsksPouch => 4,
            Self::Minigame => 8,
        }
    }
}

/// Why a check stays out of the shuffle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeftOut {
    /// A narrowing: debt, by the EIE spec.
    Narrowed(Narrowing),
    /// Not a chest or a gift, so no holder the shuffle deals.
    NotAHolder,
    /// A map, compass or key that its setting keeps where it is.
    Setting,
    /// The maintainer's call, written in a spec.
    Maintainer,
}

/// The game's maps and compasses, which move as their dungeon's copies
/// (`map_copies`).
pub(crate) const MAPS_COMPASSES: [&Name; 2] = [item::DUNGEON_MAP, item::DUNGEON_COMPASS];

/// Whether `new_game` deals maps and compasses, as copies, to any check.
pub(crate) fn maps_move(new_game: &NewGame) -> bool {
    !new_game.vanilla && !new_game.keep_chests && new_game.on.contains(settings::MAPS_COMPASSES)
}

/// What `shuffleable` lets in beyond the plain chests.
#[derive(Clone, Copy)]
pub(crate) struct Let {
    /// Maps and compasses.
    pub(crate) maps: bool,
    /// The chests of keys Link starts with.
    pub(crate) keys: bool,
}

impl Let {
    pub(crate) fn of(new_game: &NewGame) -> Self {
        Self {
            maps: maps_move(new_game),
            keys: new_game.keys.moved(),
        }
    }
}

/// Whether the shuffle may deal `c`: true when [`left_out`] finds no reason
/// to keep it vanilla.
pub(crate) fn shuffleable(c: &ItemCheck, open: Let) -> bool {
    left_out(c, open).is_none()
}

/// Why `c` stays out of the shuffle, if it does.
pub(crate) fn left_out(c: &ItemCheck, open: Let) -> Option<LeftOut> {
    // Not the Treasure Chest Game's shop chests, but its prize.
    if !(c.holder.starts_with("Obj_TBox_") || gifts::is_gift(c)) {
        return Some(LeftOut::NotAHolder);
    }
    // Keys moved around can soft-lock, since logic doesn't count them.
    // Keys Link starts with aren't moved: their chests bring a rupee.
    let moves = open.maps && MAPS_COMPASSES.iter().any(|d| **d == *c.item)
        || open.keys && keys::is_dungeon_key(&c.item);
    if c.item.starts_with("Item_Dungeon_") && !moves {
        return Some(LeftOut::Setting);
    }
    if dampe::REPEAT_CHESTS.contains(&c.hash) {
        return Some(LeftOut::Maintainer);
    }
    // Minigame events pick the outcome themselves, bar the prize chests.
    let minigame = c
        .get_event
        .as_deref()
        .is_some_and(|e| e.starts_with("MiniGame_"));
    (minigame && !gifts::is_prize(c)).then_some(LeftOut::Narrowed(Narrowing::Minigame))
}

/// Whether `item` asks on `check`, which a gift can't answer unless it
/// asks the pouch first. Nothing asks of an NPC gift, a scrub or a cow:
/// each puts the pouch questions in front of its grant (`gifts::levels`).
/// A prize chest is a chest to the hook, which gives a line's level there
/// and swaps a Red Rupee copy for ammo (`ammo::line`). A shop slot shows
/// each step on a shelf of its own (`shops::steps`), and a pickup in a
/// placement of its own (`pickups::steps`). The Goron pot shows a line's
/// first step until the hook picks it (`goron_pot`): refusing it there
/// would narrow EIE.
pub(crate) fn asks_at(check: &ItemCheck, item: &str, progressive: bool) -> bool {
    let picks = gifts::of(check).is_some()
        || scrubs::of(check).is_some()
        || cows::is_cow(check)
        || shops::is_shop(check)
        || gifts::is_prize(check)
        || pickups::is_pickup(check)
        || crate::goron_pot::is_pot(check);
    !picks && levels::asks(item, crate::progressive::Lines::widest(progressive))
}

/// Whether `check` may hold `item`: a gift can't hold one that asks. A
/// key copy goes in any chest or gift
/// (NPC, shop, scrub, cow, prize chest or pickup), whose gets the hook
/// credits as Link gets them (the pouch add, or the pickup window). Each
/// kind was proven live (stage 1 of the every-item-everywhere spec).
pub(crate) fn allowed(check: &ItemCheck, item: &str, progressive: bool) -> bool {
    refusal(check, item, progressive).is_none()
}

/// Why `check` may not hold `item`, if it may not.
pub(crate) fn refusal(check: &ItemCheck, item: &str, progressive: bool) -> Option<Narrowing> {
    let gift = gifts::is_gift(check);
    if gift && asks_at(check, item, progressive) {
        return Some(Narrowing::AsksPouch);
    }
    let credited = gift || check.holder.starts_with("Obj_TBox_");
    (!credited && keys::is_shuffled(item)).then_some(Narrowing::Uncredited)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::known::{actor, flag};
    use crate::{ammo, progressive};

    pub(crate) fn chest(hash: u64, item: &str) -> ItemCheck {
        ItemCheck {
            file: "Banc/Area/Map.bcett.byml.zs".into(),
            area: "Area".into(),
            hash,
            holder: actor::OBJ_T_BOX_A_02.to_string(),
            item: item.into(),
            get_event: None,
            substitute: None,
            presence: None,
            watched: crate::checks::Watched::No,
            room: None,
        }
    }

    /// Nothing beyond plain chests.
    pub(crate) fn no() -> Let {
        Let {
            maps: false,
            keys: false,
        }
    }

    #[test]
    fn a_cow_holds_a_line_item_and_ammo() {
        needs_names!();
        let cow = ItemCheck {
            holder: cows::HOLDER.into(),
            ..chest(1, "Item_A")
        };
        assert!(allowed(&cow, item::HOOKSHOT, true));
        assert!(allowed(&cow, item::BOMB_BUNDLE_SMALL, true));
    }

    /// The hook levels a prize chest's line as it does any chest's, and
    /// swaps its Red Rupee copy for ammo.
    #[test]
    fn a_prize_chest_holds_every_line_level_and_ammo() {
        needs_names!();
        for hash in gifts::PRIZE_CHESTS {
            let prize = ItemCheck {
                hash,
                ..chest(1, "Item_A")
            };
            for line in progressive::LINES {
                for &(level, _) in line.iter() {
                    assert!(allowed(&prize, level, true), "{level}");
                }
            }
            for (ammo, _) in ammo::NEEDS {
                assert!(allowed(&prize, ammo, true), "{ammo}");
                assert!(allowed(&prize, ammo, false), "{ammo}");
            }
        }
    }

    #[test]
    fn a_scrub_holds_every_lines_first_level_and_ammo() {
        needs_names!();
        let scrub = &crate::scrubs::checks()[0];
        for on in [false, true] {
            for line in progressive::LINES {
                assert!(allowed(scrub, line[0].0, on), "{}", line[0].0);
            }
            for (ammo, _) in ammo::NEEDS {
                assert!(allowed(scrub, ammo, on), "{ammo}");
            }
        }
    }

    #[test]
    fn a_shop_slot_holds_every_line_level_and_ammo() {
        needs_names!();
        let slot = ItemCheck {
            file: shops::FILE.into(),
            ..chest(1, "Item_A")
        };
        for on in [false, true] {
            for line in progressive::LINES {
                for &(level, _) in line.iter() {
                    assert!(allowed(&slot, level, on), "{level}");
                }
            }
            for (ammo, _) in ammo::NEEDS {
                assert!(allowed(&slot, ammo, on), "{ammo}");
            }
        }
    }

    #[test]
    fn a_pickup_holds_every_line_level_and_ammo_even_one_logic_watches() {
        needs_names!();
        let pot = ItemCheck {
            holder: actor::OBJ_POT_A_01.to_string(),
            ..chest(1, "Item_A")
        };
        let watched = ItemCheck {
            watched: crate::checks::Watched::Loud,
            ..pot.clone()
        };
        for on in [false, true] {
            for line in progressive::LINES {
                for &(level, _) in line.iter() {
                    assert!(allowed(&pot, level, on), "{level}");
                    assert!(allowed(&watched, level, on), "{level}");
                }
            }
            for (ammo, _) in ammo::NEEDS {
                assert!(allowed(&pot, ammo, on), "{ammo}");
                assert!(allowed(&watched, ammo, on), "{ammo}");
            }
        }
        assert!(allowed(&watched, item::HOOKSHOT, true));
    }

    #[test]
    fn key_copies_go_in_chests_and_every_gift() {
        needs_names!();
        let ring = crate::key_tints::ring(1);
        let pot = ItemCheck {
            holder: actor::OBJ_POT_A_01.to_string(),
            ..chest(1, "Item_A")
        };
        let other = ItemCheck {
            holder: "Obj_P".into(),
            ..chest(1, "Item_A")
        };
        let saria = ItemCheck {
            hash: gifts::GIFTS[0].hash(),
            ..chest(1, "Item_A")
        };
        assert!(allowed(&chest(1, "Item_A"), &ring, false));
        assert!(allowed(&saria, &ring, false));
        assert!(allowed(&pot, &ring, false));
        let prize = ItemCheck {
            hash: gifts::PRIZE_CHESTS[0],
            ..chest(1, "Item_A")
        };
        let cow = ItemCheck {
            holder: cows::HOLDER.to_string(),
            ..chest(1, "Item_A")
        };
        assert!(allowed(&prize, &ring, false));
        assert!(allowed(&cow, &ring, false));
        assert!(!allowed(&other, &ring, false));
        assert!(allowed(&other, "Item_A", false));
        assert_eq!(refusal(&other, &ring, false), Some(Narrowing::Uncredited));
    }

    #[test]
    fn each_left_out_check_says_why() {
        needs_names!();
        let why = |c: &ItemCheck| left_out(c, no());
        let pot = ItemCheck {
            holder: actor::OBJ_POT_A_01.to_string(),
            ..chest(1, "Item_A")
        };
        let other = ItemCheck {
            holder: "Obj_P".into(),
            ..chest(1, "Item_A")
        };
        let game = ItemCheck {
            get_event: Some(flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX.to_string()),
            ..chest(2, "Item_A")
        };
        assert_eq!(why(&chest(1, "Item_A")), None);
        assert_eq!(why(&pot), None);
        assert_eq!(why(&other), Some(LeftOut::NotAHolder));
        assert_eq!(
            why(&chest(1, item::DUNGEON_MAP.get())),
            Some(LeftOut::Setting)
        );
        // Nothing reads the Business Scrub Seed, so its chests are dealt.
        assert_eq!(why(&chest(1, item::SCRUB_BUSINESS_SEED.get())), None);
        assert_eq!(why(&game), Some(LeftOut::Narrowed(Narrowing::Minigame)));
        for hash in dampe::REPEAT_CHESTS {
            let repeat = ItemCheck {
                hash,
                ..game.clone()
            };
            assert_eq!(why(&repeat), Some(LeftOut::Maintainer));
        }
    }

    #[test]
    fn only_the_three_prize_chests_of_the_minigames_are_dealt() {
        let minigame = |hash, holder: &str| ItemCheck {
            holder: holder.into(),
            get_event: Some(flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX.to_string()),
            ..chest(hash, "Item_A")
        };
        for hash in gifts::PRIZE_CHESTS {
            assert!(shuffleable(
                &minigame(hash, actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02),
                no()
            ));
        }
        assert!(!shuffleable(&minigame(3, actor::OBJ_T_BOX_A_02), no()));
        assert!(!shuffleable(
            &minigame(4, actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02),
            no()
        ));
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let games = |c: &&ItemCheck| {
            c.get_event
                .as_deref()
                .is_some_and(|e| e.starts_with("MiniGame_"))
        };
        let dealt: BTreeSet<u64> = (checks.iter().filter(games))
            .filter(|c| shuffleable(c, no()))
            .map(|c| c.hash)
            .collect();
        assert_eq!(dealt, BTreeSet::from(gifts::PRIZE_CHESTS));
    }

    #[test]
    fn every_vanilla_substitute_chest_is_shuffleable() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let owned: std::collections::BTreeSet<_> = checks
            .iter()
            .filter(|c| c.substitute.is_some())
            .inspect(|c| assert!(shuffleable(c, no()), "{} {}", c.file, c.hash))
            .map(|c| c.hash)
            .collect();
        assert_eq!(owned.len(), 7);
    }
}
