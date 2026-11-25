//! What the hook keeps between frames for the slots Link cycles: each
//! page's object, the controller, and for each slot what the menu showed
//! last frame, its cycle, and Link's choice for each age. All zeros is a fresh start, as the game's
//! memory starts. The choice lives here, so it lasts until the game closes.

use super::songs::{self, OneTap};
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
            self.choose(i, slot);
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

    /// Link picked actor `i`, or just got it: it's the choice for each age
    /// that uses it, so a mask picked as a child is still on as an adult.
    pub fn choose(&mut self, i: usize, slot: &MultiSlot) {
        for age in [Age::Child, Age::Adult] {
            if slot.ages.get(i).is_some_and(|a| a & age as u8 != 0) {
                self.choice[age as usize - 1] = u8::try_from(i + 1).unwrap_or(0);
            }
        }
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
    songs: OneTap,
    /// The songbook, as the ocarina's input last saw it.
    pub book: songs::Seen,
}

/// What the hook does to the controller this read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pads {
    /// Buttons to keep from the game.
    pub hide: Buttons,
    /// Buttons to press for the game.
    pub press: Buttons,
}

impl Store {
    /// Takes the controller's buttons `now`, moves each slot's cycle and
    /// the one-tap songs on (`ms` is the clock, `None` with that setting
    /// off), and says what to keep from the game and press for it.
    pub fn read(
        &mut self,
        now: Buttons,
        slots: &[Option<MultiSlot>; MAX_SLOTS],
        ms: Option<u64>,
    ) -> Pads {
        self.input.step(now);
        let pressed = self.input.pressed();
        let cycles = (self.slots.iter_mut().zip(slots))
            .filter_map(|(state, slot)| Some(state.step(pressed, slot.as_ref()?)))
            .fold(Buttons::NONE, |all, s| all | s);
        let book = ms.map(|ms| songs::Frame {
            book: self.book.at(ms),
            ms,
        });
        let song = self.songs.step(pressed, book);
        Pads {
            hide: self.input.hide(cycles | song.swallow),
            press: song.press,
        }
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

    impl Store {
        /// A read with the one-tap songs off: what's kept from the game.
        fn cycle(&mut self, now: Buttons, slots: &[Option<MultiSlot>; MAX_SLOTS]) -> Buttons {
            self.read(now, slots, None).hide
        }
    }

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
        assert_eq!(store.cycle(Buttons::A, &ONE), Buttons::A);
        assert_eq!(store.cycle(Buttons::A, &ONE), Buttons::A, "still held");
        assert_eq!(store.cycle(Buttons::NONE, &ONE), Buttons::NONE);
        assert!(store.cycling());
        assert_eq!(store.cycle(Buttons::D_RIGHT, &ONE), Buttons::D_RIGHT);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        store.cycle(Buttons::NONE, &ONE);
        assert_eq!(store.cycle(Buttons::A, &ONE), Buttons::NONE, "to the game");
        assert!(!store.cycling());
        assert_eq!(store.slots[0].showing(&FAKE), Some(2), "the choice");
    }

    #[test]
    fn a_cycle_starts_from_the_choice() {
        let mut store = fresh(&[1, 2]);
        store.slots[0].choose(2, &FAKE);
        store.cycle(Buttons::A, &ONE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        store.cycle(Buttons::NONE, &ONE);
        store.cycle(Buttons::D_RIGHT, &ONE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
    }

    #[test]
    fn b_puts_back_what_was_shown() {
        let mut store = fresh(&[1, 2]);
        store.cycle(Buttons::A, &ONE);
        store.cycle(Buttons::NONE, &ONE);
        store.cycle(Buttons::D_LEFT, &ONE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(2));
        assert_eq!(store.cycle(Buttons::B, &ONE), Buttons::B);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
    }

    #[test]
    fn a_choice_traded_away_falls_back_and_a_new_item_is_chosen() {
        let mut store = fresh(&[1, 2]);
        store.slots[0].choose(2, &FAKE);
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
        store.slots[0].choose(4, &FAKE);
        assert_eq!(store.slots[0].showing(&FAKE), Some(4));
    }

    #[test]
    fn a_pick_both_ages_use_holds_for_both() {
        const BOTH: MultiSlot = MultiSlot { ages: &[3, 3] };
        let both = [Some(BOTH), None];
        let mut store = fresh(&[0, 1]);
        for press in [
            Buttons::A,
            Buttons::NONE,
            Buttons::D_RIGHT,
            Buttons::NONE,
            Buttons::A,
        ] {
            store.cycle(press, &both);
        }
        assert_eq!(store.slots[0].showing(&BOTH), Some(1));
        store.slots[0].seen.child = true;
        assert_eq!(store.slots[0].showing(&BOTH), Some(1));
    }

    #[test]
    fn a_press_cycles_only_the_slot_the_cursor_is_on() {
        let mut store = fresh(&[1, 2]);
        store.slots[1] = store.slots[0];
        store.slots[0].seen.on_slot = false;
        let both = [Some(FAKE), Some(FAKE)];
        assert_eq!(store.cycle(Buttons::A, &both), Buttons::A);
        store.cycle(Buttons::NONE, &both);
        assert!(!store.slots[0].cycling());
        assert!(store.slots[1].cycling());
        store.cycle(Buttons::D_RIGHT, &both);
        assert_eq!(store.slots[0].showing(&FAKE), Some(1));
        assert_eq!(store.slots[1].showing(&FAKE), Some(2));
    }

    #[test]
    fn a_tap_keeps_zl_from_the_game_until_it_s_let_go_after_the_song() {
        use songs::{Book, GAP_MS, HOLD_MS, SETTLE_MS, Seen, Song};
        let mut store = fresh(&[]);
        let none = [None, None];
        store.book = Seen {
            book: Book::Open(Song::ZeldasLullaby.notes()),
            ms: 0,
        };
        store.read(Buttons::ZL, &none, Some(0));
        let tap = store.read(Buttons::ZL | Buttons::A, &none, Some(1));
        assert_eq!(tap.hide, Buttons::ZL | Buttons::A);
        // The ocarina's input sees the book shut.
        store.book = Seen {
            book: Book::Shut,
            ms: 2,
        };
        store.read(Buttons::ZL, &none, Some(2));
        let first = store.read(Buttons::ZL | Buttons::B, &none, Some(2 + SETTLE_MS));
        assert_eq!(
            first,
            Pads {
                hide: Buttons::ZL | Buttons::B,
                press: Buttons::Y
            }
        );
        let after = 2 + SETTLE_MS + 6 * (HOLD_MS + GAP_MS);
        let done = store.read(Buttons::ZL, &none, Some(after));
        assert_eq!(done.hide, Buttons::ZL, "held through the song");
        assert_eq!(
            store.read(Buttons::NONE, &none, Some(after + 1)).hide,
            Buttons::NONE
        );
        let fresh_zl = store.read(Buttons::ZL, &none, Some(after + 2));
        assert_eq!(fresh_zl.hide, Buttons::NONE, "a new ZL is the game's");
    }

    #[test]
    fn a_book_last_seen_long_ago_reads_shut() {
        use songs::{Book, FRESH_MS, Seen, Song};
        let mut store = fresh(&[]);
        let none = [None, None];
        // The ocarina was put away with the book open.
        store.book = Seen {
            book: Book::Open(Song::ZeldasLullaby.notes()),
            ms: 0,
        };
        let tap = store.read(Buttons::A, &none, Some(FRESH_MS + 1));
        assert_eq!(tap, Pads::default(), "A is the game's");
    }
}
