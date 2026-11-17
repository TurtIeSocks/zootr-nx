//! What the hook keeps between frames for the slot Link cycles: the Items
//! page, what the menu showed last frame, the controller, the cycle, and
//! Link's choice for each age. All zeros is a fresh start, as the game's
//! memory starts. The choice lives here, so it lasts until the game closes.

use super::{Age, Buttons, Cycle, Frame, Input, MultiSlot};

/// What the UI thread saw of the menu on its last frame.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Seen {
    /// The Items page is up with its cursor on the slot.
    pub on_slot: bool,
    pub child: bool,
    /// Which of the slot's actors Link holds, one bit each by table index.
    pub held: u32,
}

impl Seen {
    fn age(&self) -> Age {
        match self.child {
            true => Age::Child,
            false => Age::Adult,
        }
    }

    fn held(&self) -> impl Fn(usize) -> bool + '_ {
        |i| i < 32 && self.held >> i & 1 != 0
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Store {
    /// The Items page, from its build. It lives as long as the game.
    pub page: usize,
    /// Between the page's build and its leaving (a page change or the
    /// menu closing).
    pub up: bool,
    pub seen: Seen,
    input: Input,
    cycle: Cycle,
    /// Link's choice for a child and an adult, as a table index plus one,
    /// 0 for none.
    choice: [u8; 2],
}

impl Store {
    /// Takes the controller's buttons `now`, moves the cycle on, and
    /// returns the buttons to keep from the game.
    pub fn read(&mut self, now: Buttons, slot: &MultiSlot) -> Buttons {
        self.input.step(now);
        let frame = Frame {
            on_slot: self.seen.on_slot,
            pressed: self.input.pressed(),
            shown: self.showing(slot),
        };
        let age = self.seen.age();
        let out = self.cycle.step(frame, slot, age, self.seen.held());
        if let Some(i) = out.chosen {
            self.choose(i, age);
        }
        self.input.hide(out.swallow)
    }

    /// Whether Link is cycling the slot.
    pub fn cycling(&self) -> bool {
        matches!(self.cycle, Cycle::On { .. })
    }

    /// What the slot should show now, see [`Cycle::showing`].
    pub fn showing(&self, slot: &MultiSlot) -> Option<usize> {
        let age = self.seen.age();
        self.cycle
            .showing(slot, self.choice(age), age, self.seen.held())
    }

    /// Link just got actor `i`: it's the choice for each age that uses it.
    pub fn granted(&mut self, i: usize, slot: &MultiSlot) {
        for age in [Age::Child, Age::Adult] {
            if slot.ages.get(i).is_some_and(|a| a & age as u8 != 0) {
                self.choose(i, age);
            }
        }
    }

    fn choose(&mut self, i: usize, age: Age) {
        self.choice[age as usize - 1] = u8::try_from(i + 1).unwrap_or(0);
    }

    fn choice(&self, age: Age) -> Option<usize> {
        self.choice[age as usize - 1]
            .checked_sub(1)
            .map(usize::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::slots::tests::FAKE;

    fn fresh(held: &[usize]) -> Store {
        // SAFETY: the store is valid all zeros, as the game's memory starts.
        let mut store: Store = unsafe { core::mem::zeroed() };
        store.seen = Seen {
            on_slot: true,
            child: false,
            held: held.iter().map(|i| 1 << i).sum(),
        };
        store
    }

    #[test]
    fn zeros_show_the_game_s_pick() {
        let store = fresh(&[1, 2]);
        assert_eq!(store.showing(&FAKE), Some(1));
    }

    #[test]
    fn a_cycle_keeps_a_from_the_game_until_its_release_then_confirms() {
        let mut store = fresh(&[1, 2]);
        assert_eq!(store.read(Buttons::A, &FAKE), Buttons::A);
        assert_eq!(store.read(Buttons::A, &FAKE), Buttons::A, "still held");
        assert_eq!(store.read(Buttons::NONE, &FAKE), Buttons::NONE);
        assert!(store.cycling());
        assert_eq!(store.read(Buttons::D_RIGHT, &FAKE), Buttons::D_RIGHT);
        assert_eq!(store.showing(&FAKE), Some(2));
        store.read(Buttons::NONE, &FAKE);
        assert_eq!(store.read(Buttons::A, &FAKE), Buttons::NONE, "to the game");
        assert!(!store.cycling());
        assert_eq!(store.showing(&FAKE), Some(2), "the choice");
    }

    #[test]
    fn a_cycle_starts_from_the_choice() {
        let mut store = fresh(&[1, 2]);
        store.granted(2, &FAKE);
        store.read(Buttons::A, &FAKE);
        assert_eq!(store.showing(&FAKE), Some(2));
        store.read(Buttons::NONE, &FAKE);
        store.read(Buttons::D_RIGHT, &FAKE);
        assert_eq!(store.showing(&FAKE), Some(1));
    }

    #[test]
    fn b_puts_back_what_was_shown() {
        let mut store = fresh(&[1, 2]);
        store.read(Buttons::A, &FAKE);
        store.read(Buttons::NONE, &FAKE);
        store.read(Buttons::D_LEFT, &FAKE);
        assert_eq!(store.showing(&FAKE), Some(2));
        assert_eq!(store.read(Buttons::B, &FAKE), Buttons::B);
        assert_eq!(store.showing(&FAKE), Some(1));
    }

    #[test]
    fn a_choice_traded_away_falls_back_and_a_new_item_is_chosen() {
        let mut store = fresh(&[1, 2]);
        store.granted(2, &FAKE);
        assert_eq!(store.showing(&FAKE), Some(2));
        store.seen.held = 1 << 1;
        assert_eq!(store.showing(&FAKE), Some(1));
        store.seen.child = true;
        store.seen.held = 1 << 3 | 1 << 4;
        assert_eq!(store.showing(&FAKE), Some(3), "the child keeps its own");
        store.granted(4, &FAKE);
        assert_eq!(store.showing(&FAKE), Some(4));
    }
}
