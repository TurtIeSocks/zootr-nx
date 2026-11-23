//! What the hook keeps between frames for the slots Link cycles: each
//! page's object, the controller, and for each slot what the menu showed
//! last frame, its cycle, and Link's choice for each age. All zeros is a fresh start, as the game's
//! memory starts. The choice lives here, so it lasts until the game closes.

use super::{Age, Buttons, Cycle, Frame, Input, MAX_SLOTS, MultiSlot, Page};

/// What the UI thread saw of the menu on its last frame.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Seen {
    /// The slot's page is up with its cursor on the slot.
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

/// A page the hook watches, from its build to its leaving.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PageUp {
    /// The page, from its build. It lives as long as the game.
    pub page: usize,
    /// Between the page's build and its leaving (a page change or the
    /// menu closing).
    pub up: bool,
}

/// One slot's cycle, what the menu showed of it last frame, and Link's
/// choice for each age.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct SlotState {
    pub seen: Seen,
    cycle: Cycle,
    /// Link's choice for a child and an adult, as a table index plus one,
    /// 0 for none.
    choice: [u8; 2],
}

impl SlotState {
    /// Moves the cycle on with this frame's presses, and returns the
    /// presses to keep from the game.
    fn step(&mut self, pressed: Buttons, slot: &MultiSlot) -> Buttons {
        let frame = Frame {
            on_slot: self.seen.on_slot,
            pressed,
            shown: self.showing(slot),
        };
        let age = self.seen.age();
        let out = self.cycle.step(frame, slot, age, self.seen.held());
        if let Some(i) = out.chosen {
            self.choose(i, age);
        }
        out.swallow
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

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Store {
    /// Each [`Page`], by its number.
    pub pages: [PageUp; Page::ALL.len()],
    input: Input,
    /// Each of the table's slots, by its place there.
    pub slots: [SlotState; MAX_SLOTS],
}

impl Store {
    /// Takes the controller's buttons `now`, moves each slot's cycle on,
    /// and returns the buttons to keep from the game.
    pub fn read(&mut self, now: Buttons, slots: &[Option<MultiSlot>; MAX_SLOTS]) -> Buttons {
        self.input.step(now);
        let pressed = self.input.pressed();
        let swallow = (self.slots.iter_mut().zip(slots))
            .filter_map(|(state, slot)| Some(state.step(pressed, slot.as_ref()?)))
            .fold(Buttons::NONE, |all, s| all | s);
        self.input.hide(swallow)
    }

    /// Whether Link is cycling any slot.
    pub fn cycling(&self) -> bool {
        self.slots.iter().any(SlotState::cycling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::slots::tests::FAKE;

    /// The table's slots: only the first.
    const ONE: [Option<MultiSlot>; MAX_SLOTS] = [Some(FAKE), None];

    fn fresh(held: &[usize]) -> Store {
        // SAFETY: the store is valid all zeros, as the game's memory starts.
        let mut store: Store = unsafe { core::mem::zeroed() };
        store.slots[0].seen = Seen {
            on_slot: true,
            child: false,
            held: held.iter().map(|i| 1 << i).sum(),
        };
        store
    }

    #[test]
    fn zeros_show_the_game_s_pick() {
        let store = fresh(&[1, 2]);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
    }

    #[test]
    fn a_cycle_keeps_a_from_the_game_until_its_release_then_confirms() {
        let mut store = fresh(&[1, 2]);
        assert_eq!(store.read(Buttons::A, &ONE), Buttons::A);
        assert_eq!(store.read(Buttons::A, &ONE), Buttons::A, "still held");
        assert_eq!(store.read(Buttons::NONE, &ONE), Buttons::NONE);
        assert!(store.cycling());
        assert_eq!(store.read(Buttons::D_RIGHT, &ONE), Buttons::D_RIGHT);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        store.read(Buttons::NONE, &ONE);
        assert_eq!(store.read(Buttons::A, &ONE), Buttons::NONE, "to the game");
        assert!(!store.cycling());
        assert_eq!(store.slots[0].showing(&FAKE), Some(2), "the choice");
    }

    #[test]
    fn a_cycle_starts_from_the_choice() {
        let mut store = fresh(&[1, 2]);
        store.slots[0].granted(2, &FAKE);
        store.read(Buttons::A, &ONE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        store.read(Buttons::NONE, &ONE);
        store.read(Buttons::D_RIGHT, &ONE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
    }

    #[test]
    fn b_puts_back_what_was_shown() {
        let mut store = fresh(&[1, 2]);
        store.read(Buttons::A, &ONE);
        store.read(Buttons::NONE, &ONE);
        store.read(Buttons::D_LEFT, &ONE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        assert_eq!(store.read(Buttons::B, &ONE), Buttons::B);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
    }

    #[test]
    fn a_choice_traded_away_falls_back_and_a_new_item_is_chosen() {
        let mut store = fresh(&[1, 2]);
        store.slots[0].granted(2, &FAKE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        store.slots[0].seen.held = 1 << 1;
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
        store.slots[0].seen.child = true;
        store.slots[0].seen.held = 1 << 3 | 1 << 4;
        assert_eq!(
            store.slots[0].showing(&FAKE),
            Some(3),
            "the child keeps its own"
        );
        store.slots[0].granted(4, &FAKE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(4));
    }

    #[test]
    fn a_press_cycles_only_the_slot_the_cursor_is_on() {
        let mut store = fresh(&[1, 2]);
        store.slots[1] = store.slots[0];
        store.slots[0].seen.on_slot = false;
        let both = [Some(FAKE), Some(FAKE)];
        assert_eq!(store.read(Buttons::A, &both), Buttons::A);
        store.read(Buttons::NONE, &both);
        assert!(!store.slots[0].cycling());
        assert!(store.slots[1].cycling());
        store.read(Buttons::D_RIGHT, &both);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
        assert_eq!(store.slots[1].showing(&FAKE), Some(2));
    }
}
