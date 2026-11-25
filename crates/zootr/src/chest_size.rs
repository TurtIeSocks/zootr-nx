//! What size each chest takes. Matches contents makes a chest big when it
//! holds an item the seed needs, the boss key chest when it holds a boss
//! key, and small otherwise. Keys and tokens stay small whatever logic
//! says: a seed holds many of each, and big chests for all of them would
//! hide the hint.

use crate::checks::{self, Size};
use crate::foreign::Owner;
use crate::keys;
use crate::known::item;
use crate::settings::{ChestSize, NewGame};

/// The size a chest of `size` dropping `drop` takes in `new_game`'s seed.
pub fn resized(new_game: &NewGame, size: Size, drop: &str) -> Size {
    match new_game.chest_size {
        ChestSize::Vanilla => size,
        ChestSize::Small => checks::shrunk(size, drop),
        ChestSize::Matches => of(new_game, drop),
        ChestSize::Big => match size {
            Size::Small => Size::Big,
            other => other,
        },
    }
}

/// The size a chest holding `item` takes when its size matches it. A trap
/// takes the size of what it passes for, and another player's item is big
/// when Archipelago classes it progression.
pub fn of(new_game: &NewGame, item: &str) -> Size {
    if let Some(copy) = new_game.foreign.iter().find(|f| f.actor == item) {
        return match &copy.owner {
            Owner::Trap { .. } => of(new_game, &copy.shows),
            Owner::Player { progression, .. } => big_if(*progression),
        };
    }
    if keys::is_boss(item) {
        Size::Boss
    } else if keys::is_small(item) || item == item::TOKEN_GOLD {
        Size::Small
    } else {
        big_if(new_game.major.contains(item))
    }
}

fn big_if(major: bool) -> Size {
    match major {
        true => Size::Big,
        false => Size::Small,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foreign::Foreign;
    use crate::key_tints;
    use crate::known::actor;

    fn copy(actor: &str, shows: &str, owner: Owner) -> Foreign {
        Foreign {
            actor: actor.into(),
            shows: shows.into(),
            model: None,
            owner,
        }
    }

    fn player(progression: bool) -> Owner {
        Owner::Player {
            item: "Fake Item".into(),
            player: "FakePlayer".into(),
            progression,
        }
    }

    fn seed() -> NewGame {
        let needed = [item::HOOKSHOT, item::TOKEN_GOLD, keys::SMALL, keys::BOSS];
        let trap = Owner::Trap {
            label: "Fake".into(),
        };
        NewGame {
            chest_size: ChestSize::Matches,
            major: needed.iter().map(|i| i.to_string()).collect(),
            foreign: vec![
                copy("FakeTrap_Hook", item::HOOKSHOT, trap.clone()),
                copy("FakeTrap_Rupee", actor::T_BOX_RUPEE_RED, trap),
                copy("FakeCopy_Prog", "", player(true)),
                copy("FakeCopy_Fill", "", player(false)),
            ],
            ..NewGame::default()
        }
    }

    #[test]
    fn each_class_takes_its_size() {
        needs_names!();
        let seed = seed();
        let size = |i: &str| of(&seed, i);
        assert_eq!(size(item::HOOKSHOT), Size::Big, "needed");
        assert_eq!(size(actor::T_BOX_RUPEE_RED), Size::Small, "not needed");
        assert_eq!(
            size(item::TOKEN_GOLD),
            Size::Small,
            "a token, though needed"
        );
        assert_eq!(size(keys::SMALL), Size::Small, "a small key, though needed");
        assert_eq!(size(&key_tints::ring(1)), Size::Small, "a key ring");
        assert_eq!(
            size(&key_tints::fortress()),
            Size::Small,
            "a Fortress key copy"
        );
        assert_eq!(size(keys::BOSS), Size::Boss);
        assert_eq!(size(&key_tints::boss(1)), Size::Boss, "a boss key copy");
    }

    #[test]
    fn a_trap_takes_its_disguise_s_size_and_ap_items_their_class_s() {
        needs_names!();
        let seed = seed();
        assert_eq!(of(&seed, "FakeTrap_Hook"), Size::Big);
        assert_eq!(of(&seed, "FakeTrap_Rupee"), Size::Small);
        assert_eq!(of(&seed, "FakeCopy_Prog"), Size::Big);
        assert_eq!(of(&seed, "FakeCopy_Fill"), Size::Small);
    }

    #[test]
    fn only_matches_contents_reads_the_drop() {
        needs_names!();
        let mut seed = seed();
        let hookshot = item::HOOKSHOT.get();
        assert_eq!(resized(&seed, Size::Small, hookshot), Size::Big);
        seed.chest_size = ChestSize::Small;
        assert_eq!(resized(&seed, Size::Big, hookshot), Size::Small);
        assert_eq!(resized(&seed, Size::Boss, hookshot), Size::Boss);
        seed.chest_size = ChestSize::Big;
        assert_eq!(resized(&seed, Size::Small, hookshot), Size::Big);
        assert_eq!(resized(&seed, Size::Boss, hookshot), Size::Boss);
        seed.chest_size = ChestSize::Vanilla;
        assert_eq!(resized(&seed, Size::Small, hookshot), Size::Small);
    }
}
