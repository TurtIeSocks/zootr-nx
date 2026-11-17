//! Cycle mode on a shared slot, as Ship of Harkinian has it: A on the slot
//! enters, left and right step through what Link holds, A confirms and goes
//! on to the game's own select, B or leaving the slot puts back what was
//! shown before.

use super::{Age, Buttons, MultiSlot};

/// A sound for the hook to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Enter,
    Step,
    Leave,
}

/// What the menu tells the cycle this frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Whether the cursor is on this slot.
    pub on_slot: bool,
    pub pressed: Buttons,
}

/// What the hook does for the cycle this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Out {
    /// Presses the game mustn't see.
    pub swallow: Buttons,
    /// The actor to put first in the slot, by its table index.
    pub show: Option<usize>,
    /// The actor Link confirmed, to keep as his choice.
    pub chosen: Option<usize>,
    pub sound: Option<Sound>,
}

/// Off, or cycling with what the slot showed on entering and shows now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Cycle {
    #[default]
    Off,
    On {
        before: usize,
        now: usize,
    },
}

impl Cycle {
    /// Moves on one frame of the menu.
    pub fn step(
        &mut self,
        frame: Frame,
        slot: &MultiSlot,
        age: Age,
        held: impl Fn(usize) -> bool,
    ) -> Out {
        let pressed = frame.pressed;
        let Self::On { before, now } = *self else {
            let enter = frame.on_slot && pressed.any(Buttons::A) && slot.count(age, &held) > 1;
            let Some(shown) = slot.shown(age, &held).filter(|_| enter) else {
                return Out::default();
            };
            *self = Self::On {
                before: shown,
                now: shown,
            };
            return Out {
                swallow: Buttons::A,
                sound: Some(Sound::Enter),
                ..Out::default()
            };
        };
        if !frame.on_slot || pressed.any(Buttons::B) {
            *self = Self::Off;
            return Out {
                swallow: pressed & Buttons::B,
                show: Some(before),
                sound: Some(Sound::Leave),
                ..Out::default()
            };
        }
        if pressed.any(Buttons::A) {
            *self = Self::Off;
            return Out {
                chosen: Some(now),
                ..Out::default()
            };
        }
        let turn = pressed & (Buttons::LEFT | Buttons::RIGHT);
        if turn == Buttons::NONE {
            return Out::default();
        }
        let next = (slot.step(age, &held, now, turn.any(Buttons::RIGHT))).unwrap_or(now);
        *self = Self::On { before, now: next };
        Out {
            swallow: turn,
            show: Some(next),
            sound: Some(Sound::Step),
            ..Out::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::slots::tests::{FAKE, holding};

    const ADULT: Age = Age::Adult;

    fn on(pressed: Buttons) -> Frame {
        Frame {
            on_slot: true,
            pressed,
        }
    }

    fn run(cycle: &mut Cycle, frame: Frame, held: &[usize]) -> Out {
        cycle.step(frame, &FAKE, ADULT, holding(held))
    }

    #[test]
    fn a_enters_only_with_two_held_and_the_game_never_sees_it() {
        let mut cycle = Cycle::Off;
        assert_eq!(run(&mut cycle, on(Buttons::A), &[1]), Out::default());
        assert_eq!(cycle, Cycle::Off);
        let out = run(&mut cycle, on(Buttons::A), &[1, 2]);
        assert_eq!((out.swallow, out.sound), (Buttons::A, Some(Sound::Enter)));
        assert_eq!(cycle, Cycle::On { before: 1, now: 1 });
    }

    #[test]
    fn a_press_off_the_slot_does_nothing() {
        let mut cycle = Cycle::Off;
        let frame = Frame {
            on_slot: false,
            pressed: Buttons::A,
        };
        assert_eq!(run(&mut cycle, frame, &[1, 2]), Out::default());
    }

    #[test]
    fn right_steps_forward_and_is_swallowed() {
        let mut cycle = Cycle::On { before: 1, now: 1 };
        let out = run(&mut cycle, on(Buttons::D_RIGHT), &[0, 1, 2]);
        assert_eq!(out.show, Some(0));
        assert_eq!(out.swallow, Buttons::D_RIGHT);
        assert_eq!(out.sound, Some(Sound::Step));
        let out = run(&mut cycle, on(Buttons::STICK_LEFT), &[0, 1, 2]);
        assert_eq!(out.show, Some(1));
    }

    #[test]
    fn a_confirms_and_goes_on_to_the_game() {
        let mut cycle = Cycle::On { before: 1, now: 2 };
        let out = run(&mut cycle, on(Buttons::A), &[1, 2]);
        assert_eq!(out.chosen, Some(2));
        assert_eq!(out.swallow, Buttons::NONE);
        assert_eq!(cycle, Cycle::Off);
    }

    #[test]
    fn b_or_leaving_the_slot_puts_back_what_was_shown() {
        let mut cycle = Cycle::On { before: 1, now: 2 };
        let out = run(&mut cycle, on(Buttons::B), &[1, 2]);
        assert_eq!((out.show, out.swallow), (Some(1), Buttons::B));
        assert_eq!(out.sound, Some(Sound::Leave));

        let mut cycle = Cycle::On { before: 1, now: 2 };
        let away = Frame {
            on_slot: false,
            pressed: Buttons::D_UP,
        };
        let out = run(&mut cycle, away, &[1, 2]);
        assert_eq!((out.show, out.swallow), (Some(1), Buttons::NONE));
        assert_eq!(cycle, Cycle::Off);
    }
}
