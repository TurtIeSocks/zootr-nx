//! Buttons, and which of them changed since the last frame.

use core::ops::{BitAnd, BitOr, Not};

/// A set of buttons, in `nn::hid`'s Npad bit order. The hook converts the
/// game's own layout to this as it reads it, if the two differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u32);

impl Buttons {
    pub const NONE: Self = Self(0);
    pub const A: Self = Self(1 << 0);
    pub const B: Self = Self(1 << 1);
    pub const X: Self = Self(1 << 2);
    pub const Y: Self = Self(1 << 3);
    pub const L: Self = Self(1 << 6);
    pub const R: Self = Self(1 << 7);
    pub const ZL: Self = Self(1 << 8);
    pub const ZR: Self = Self(1 << 9);
    pub const PLUS: Self = Self(1 << 10);
    pub const MINUS: Self = Self(1 << 11);
    pub const D_LEFT: Self = Self(1 << 12);
    pub const D_UP: Self = Self(1 << 13);
    pub const D_RIGHT: Self = Self(1 << 14);
    pub const D_DOWN: Self = Self(1 << 15);
    pub const STICK_LEFT: Self = Self(1 << 16);
    pub const STICK_UP: Self = Self(1 << 17);
    pub const STICK_RIGHT: Self = Self(1 << 18);
    pub const STICK_DOWN: Self = Self(1 << 19);

    /// Left on the D-pad or the left stick.
    pub const LEFT: Self = Self(Self::D_LEFT.0 | Self::STICK_LEFT.0);
    /// Right on the D-pad or the left stick.
    pub const RIGHT: Self = Self(Self::D_RIGHT.0 | Self::STICK_RIGHT.0);

    /// Whether any of `these` is in the set.
    pub const fn any(self, these: Self) -> bool {
        self.0 & these.0 != 0
    }

    /// Whether all of `these` are in the set.
    pub const fn all(self, these: Self) -> bool {
        self.0 & these.0 == these.0
    }
}

impl BitOr for Buttons {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitAnd for Buttons {
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Not for Buttons {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

/// This frame's buttons beside last frame's, and the ones kept from the
/// game. All zeros is no buttons, as the game's memory starts.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub held: Buttons,
    before: Buttons,
    hidden: Buttons,
}

impl Input {
    /// Moves on to a frame holding `now`.
    pub fn step(&mut self, now: Buttons) {
        self.before = self.held;
        self.held = now;
    }

    /// Buttons down this frame and up the last.
    pub fn pressed(&self) -> Buttons {
        self.held & !self.before
    }

    /// Buttons up this frame and down the last.
    pub fn released(&self) -> Buttons {
        self.before & !self.held
    }

    /// Keeps `swallow` from the game, and every button already kept, until
    /// it's released: a held button let through later would reach the game
    /// as a fresh press. Returns the buttons to clear this frame.
    pub fn hide(&mut self, swallow: Buttons) -> Buttons {
        self.hidden = (self.hidden | swallow) & self.held;
        self.hidden
    }

    /// Whether all of `hold` are down and one of `press` was just pressed:
    /// a combo like holding L and pressing D-pad up.
    pub fn combo(&self, hold: Buttons, press: Buttons) -> bool {
        self.held.all(hold) && self.pressed().any(press)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(frames: &[Buttons]) -> Input {
        let mut input = Input::default();
        frames.iter().for_each(|&b| input.step(b));
        input
    }

    #[test]
    fn a_press_is_seen_for_one_frame_only() {
        assert_eq!(after(&[Buttons::A]).pressed(), Buttons::A);
        assert_eq!(after(&[Buttons::A, Buttons::A]).pressed(), Buttons::NONE);
        assert_eq!(after(&[Buttons::A, Buttons::NONE]).released(), Buttons::A);
    }

    #[test]
    fn left_and_right_take_the_stick_or_the_d_pad() {
        assert!(Buttons::STICK_LEFT.any(Buttons::LEFT));
        assert!(Buttons::D_RIGHT.any(Buttons::RIGHT));
        assert!(!Buttons::D_UP.any(Buttons::LEFT | Buttons::RIGHT));
    }

    #[test]
    fn a_hidden_button_stays_hidden_until_released() {
        let mut input = Input::default();
        input.step(Buttons::A);
        assert_eq!(input.hide(Buttons::A), Buttons::A);
        input.step(Buttons::A | Buttons::B);
        assert_eq!(input.hide(Buttons::NONE), Buttons::A);
        input.step(Buttons::B);
        assert_eq!(input.hide(Buttons::NONE), Buttons::NONE);
        input.step(Buttons::A);
        assert_eq!(input.hide(Buttons::NONE), Buttons::NONE);
    }

    #[test]
    fn a_combo_wants_the_hold_down_and_a_fresh_press() {
        let held = Buttons::L | Buttons::D_UP;
        assert!(after(&[Buttons::L, held]).combo(Buttons::L, Buttons::D_UP));
        assert!(!after(&[held, held]).combo(Buttons::L, Buttons::D_UP));
        assert!(!after(&[Buttons::D_UP]).combo(Buttons::L, Buttons::D_UP));
    }
}
