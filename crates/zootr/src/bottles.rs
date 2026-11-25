//! The pool's bottles. A bottled item is a new bottle, so it needs a bottle
//! slot Link doesn't own: with all five owned, its get shows and Link gets
//! nothing. The pool deals exactly as many as the starting bottles leave
//! room for. Ruto's Letter is one while King Zora hasn't moved; every other
//! is a random pick from Ship's list of normal bottles. Cows bring a rupee.

use anyhow::{Result, ensure};

use crate::known::{Name, actor, field, item};
use crate::rng::Rng;
use crate::starting;

/// The bottle slots, `bottle-1` to `bottle-5` as starting words name them.
pub const FIELDS: [&Name; 5] = [
    field::BOTTLE_A,
    field::BOTTLE_B,
    field::BOTTLE_C,
    field::BOTTLE_D,
    field::BOTTLE_E,
];

/// What a slot bottle may hold: Ship's normal bottles.
pub const CONTENTS: [&Name; 11] = [
    item::BOTTLE,
    item::BOTTLE_MILK,
    item::BOTTLE_RED_POTION,
    item::BOTTLE_GREEN_POTION,
    item::BOTTLE_BLUE_POTION,
    item::BOTTLE_FAIRY,
    item::BOTTLE_FISH,
    item::BOTTLE_BUG,
    item::BOTTLE_POE,
    item::BOTTLE_BIG_POE,
    actor::ITEM_BOTTLE_BLUE_FIRE,
];

/// What a cow brings to the pool in place of its milk.
pub const COW: &Name = crate::ammo::WITHOUT;

/// Salt for the contents' roll, so it doesn't follow the deal's.
const SALT: u64 = 0xb0771e5;

/// Five starting bottles leave the letter no slot, so they need King
/// Zora moved, when the letter isn't dealt.
pub fn check_start(started: usize, king_zora_moved: bool) -> Result<()> {
    ensure!(
        started < FIELDS.len() || king_zora_moved,
        "starting with {started} bottles needs King Zora Moved: Ruto's Letter needs a bottle slot of its own"
    );
    Ok(())
}

/// Whether `item` takes a bottle slot: an empty or filled bottle, or the
/// letter.
pub fn is_bottle(item: &str) -> bool {
    item.starts_with(&**item::BOTTLE)
}

/// How the pool's bottles are dealt.
pub struct Deal {
    /// Bottles Link starts with.
    pub started: usize,
    /// Whether the letter must be dealt: King Zora hasn't moved.
    pub letter: bool,
    /// Whether to roll each bottle's contents (off in vanilla mode).
    pub roll: Option<u64>,
}

impl Deal {
    /// Deal `items` (one per free check, `cow` true at a cow's) so they
    /// hold exactly the bottles the slots leave room for after the start
    /// and `placed` (plando and kept checks). A cow's milk becomes [`COW`];
    /// extras past the room become the starting filler, other bottles
    /// before the letter. Fails when `placed` alone needs more slots than
    /// the start leaves.
    pub fn apply<'a>(
        &self,
        items: &mut [&'a str],
        cow: impl Fn(usize) -> bool,
        placed: impl Iterator<Item = &'a str>,
    ) -> Result<()> {
        for (i, item) in items.iter_mut().enumerate() {
            if cow(i) && is_bottle(item) {
                *item = COW;
            }
        }
        let room = FIELDS.len().saturating_sub(self.started);
        let fixed = placed.filter(|i| is_bottle(i)).count();
        ensure!(
            fixed <= room,
            "{fixed} bottles are placed, but starting with {} leaves room for {room}",
            self.started
        );
        let mut room = room - fixed;
        let letter = |i: &str| self.letter && i == &**item::BOTTLE_LETTER;
        // The letter claims its slot first, then the others in pool order.
        let order: Vec<usize> = (0..items.len())
            .filter(|&i| letter(items[i]))
            .chain((0..items.len()).filter(|&i| is_bottle(items[i]) && !letter(items[i])))
            .collect();
        let mut rng = self.roll.map(|seed| Rng::new(seed ^ SALT));
        for i in order {
            match room.checked_sub(1) {
                Some(left) => room = left,
                None => {
                    items[i] = starting::FILLER;
                    continue;
                }
            }
            if let (Some(rng), false) = (rng.as_mut(), letter(items[i])) {
                items[i] = CONTENTS[rng.below(CONTENTS.len() as u64) as usize];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTHER: &str = "Item_FakeOther";

    fn count(items: &[&str], f: impl Fn(&str) -> bool) -> usize {
        items.iter().filter(|i| f(i)).count()
    }

    /// A full shuffle's slot items: three bottles, the letter, Talon's
    /// milk, then three cows' milk and some others.
    fn pool() -> Vec<&'static str> {
        let mut p = vec![item::BOTTLE.get(); 3];
        p.push(item::BOTTLE_LETTER.get());
        p.push(item::BOTTLE_MILK.get());
        p.extend([item::BOTTLE_MILK.get(); 3]);
        p.extend([OTHER; 4]);
        p
    }

    /// The pool's cows, by index.
    fn cow(i: usize) -> bool {
        (5..8).contains(&i)
    }

    fn deal(started: usize, letter: bool, roll: Option<u64>) -> Vec<&'static str> {
        let mut items = pool();
        let d = Deal {
            started,
            letter,
            roll,
        };
        d.apply(&mut items, cow, std::iter::empty()).unwrap();
        items
    }

    #[test]
    fn the_pool_holds_five_less_the_starting_bottles() {
        needs_names!();
        for n in 0..=5 {
            for letter in [true, false] {
                let items = deal(n, letter, Some(7));
                assert_eq!(
                    count(&items, is_bottle),
                    5 - n,
                    "{n} started, letter {letter}"
                );
            }
        }
    }

    #[test]
    fn the_letter_stays_while_king_zora_waits() {
        needs_names!();
        for n in 0..=4 {
            let items = deal(n, true, Some(7));
            assert_eq!(count(&items, |i| i == item::BOTTLE_LETTER.get()), 1, "{n}");
        }
    }

    #[test]
    fn cows_bring_a_rupee() {
        needs_names!();
        let items = deal(0, true, None);
        assert!((5..8).all(|i| items[i] == COW.get()));
        assert_eq!(items[4], item::BOTTLE_MILK.get(), "Talon's milk stays");
    }

    #[test]
    fn rolled_bottles_hold_a_normal_content_and_vanilla_keeps_them() {
        needs_names!();
        let rolled = deal(0, true, Some(7));
        let normal = |i: &str| CONTENTS.iter().any(|c| c.get() == i);
        assert!(
            rolled
                .iter()
                .filter(|i| is_bottle(i))
                .all(|i| normal(i) || *i == item::BOTTLE_LETTER.get())
        );
        let rolls: std::collections::BTreeSet<_> = (0..40)
            .flat_map(|s| deal(0, false, Some(s)))
            .filter(|i| is_bottle(i))
            .collect();
        assert!(rolls.len() > 5, "contents vary by seed");
        assert_eq!(&deal(0, true, None)[..5], &pool()[..5]);
    }

    #[test]
    fn five_starting_bottles_need_king_zora_moved() {
        assert!(check_start(4, false).is_ok());
        assert!(check_start(5, true).is_ok());
        let err = check_start(5, false).unwrap_err().to_string();
        assert!(err.contains("King Zora Moved"), "{err}");
    }

    #[test]
    fn placed_bottles_take_room_and_too_many_are_refused() {
        needs_names!();
        let placed = [item::BOTTLE.get(), item::BOTTLE.get()];
        let mut items = pool();
        let d = Deal {
            started: 2,
            letter: true,
            roll: None,
        };
        d.apply(&mut items, cow, placed.into_iter()).unwrap();
        assert_eq!(count(&items, is_bottle), 1);
        let d = Deal {
            started: 4,
            letter: true,
            roll: None,
        };
        let err = d.apply(&mut pool(), cow, placed.into_iter()).unwrap_err();
        assert!(err.to_string().contains("room for 1"), "{err}");
    }
}
