//! A menu slot that holds several things, like the trade slot.

/// The most slots the hook cycles: the trade slot and the mask slot.
pub const MAX_SLOTS: usize = 2;

/// The pause menu pages a slot can be on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Page {
    Items,
    Equipment,
}

impl Page {
    pub const ALL: [Page; 2] = [Page::Items, Page::Equipment];

    pub fn from_u8(b: u8) -> Option<Self> {
        Self::ALL.get(usize::from(b)).copied()
    }
}

/// Link's age, as the bits a slot's `ages` holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Age {
    Child = 1,
    Adult = 2,
}

/// One shared slot's actors in the table's order. The game shows the first
/// one Link holds and his age can use, and its tables list a line latest
/// first (Claim Check before Pocket Egg), so forward along the line is
/// toward the start.
#[derive(Clone, Copy, Debug)]
pub struct MultiSlot<'a> {
    /// Each actor's [`Age`] bits, in table order.
    pub ages: &'a [u8],
}

impl MultiSlot<'_> {
    /// Whether Link holds actor `i` and his age can use it.
    pub fn usable(&self, i: usize, age: Age, held: &impl Fn(usize) -> bool) -> bool {
        self.ages.get(i).is_some_and(|a| a & age as u8 != 0) && held(i)
    }

    /// What the game shows: the first usable actor Link holds.
    pub fn shown(&self, age: Age, held: impl Fn(usize) -> bool) -> Option<usize> {
        (0..self.ages.len()).find(|&i| self.usable(i, age, &held))
    }

    /// How many usable actors Link holds.
    pub fn count(&self, age: Age, held: impl Fn(usize) -> bool) -> usize {
        (0..self.ages.len())
            .filter(|&i| self.usable(i, age, &held))
            .count()
    }

    /// The next usable actor Link holds after `from` along the line,
    /// forward or back, wrapping. `from` itself if it's the only one.
    pub fn step(
        &self,
        age: Age,
        held: impl Fn(usize) -> bool,
        from: usize,
        forward: bool,
    ) -> Option<usize> {
        let n = self.ages.len();
        // `checked_rem` keeps a panic (and its relocated location) out of
        // the hook: `n` is never 0 inside the range.
        (1..=n)
            .filter_map(|k| {
                match forward {
                    true => from + n - k,
                    false => from + k,
                }
                .checked_rem(n)
            })
            .find(|&i| self.usable(i, age, &held))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const C: u8 = Age::Child as u8;
    const A: u8 = Age::Adult as u8;
    /// A made-up slot: three adult items, then two child ones, latest
    /// first as the game's tables list them.
    pub(crate) const FAKE: MultiSlot = MultiSlot {
        ages: &[A, A, A, C, C],
    };

    pub(crate) fn holding(these: &[usize]) -> impl Fn(usize) -> bool + '_ {
        move |i| these.contains(&i)
    }

    #[test]
    fn the_game_shows_the_first_held_for_the_age() {
        assert_eq!(FAKE.shown(Age::Adult, holding(&[1, 2, 4])), Some(1));
        assert_eq!(FAKE.shown(Age::Child, holding(&[1, 2, 4])), Some(4));
        assert_eq!(FAKE.shown(Age::Child, holding(&[1])), None);
        assert_eq!(FAKE.count(Age::Adult, holding(&[0, 2, 3])), 2);
    }

    #[test]
    fn forward_walks_toward_the_start_and_wraps() {
        let held = || holding(&[0, 2]);
        assert_eq!(FAKE.step(Age::Adult, held(), 2, true), Some(0));
        assert_eq!(FAKE.step(Age::Adult, held(), 0, true), Some(2));
        assert_eq!(FAKE.step(Age::Adult, held(), 0, false), Some(2));
        assert_eq!(FAKE.step(Age::Adult, held(), 2, false), Some(0));
    }

    #[test]
    fn a_step_skips_the_other_age_and_stays_on_a_lone_item() {
        assert_eq!(FAKE.step(Age::Adult, holding(&[1, 3]), 1, true), Some(1));
        assert_eq!(
            FAKE.step(Age::Child, holding(&[1, 3, 4]), 3, false),
            Some(4)
        );
    }
}
