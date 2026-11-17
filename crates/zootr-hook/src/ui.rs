//! In-game UI: what Link presses, and slots that hold several things at
//! once. The parts here are pure, so they build and test on the host; the
//! hook feeds them the game's state each frame and applies what they say.
//! See `docs/superpowers/specs/2026-11-16-in-game-ui-design.md`.
//!
//! Text through the game's message system is the next seam to add here.

pub mod cycle;
pub mod input;
pub mod slots;
pub mod store;

pub use cycle::{Cycle, Frame, Out, Sound};
pub use input::{Buttons, Input};
pub use slots::{Age, MultiSlot};
pub use store::{Seen, Store};
