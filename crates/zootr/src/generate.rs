//! Builds a seed's mod and reports what it placed.
//!
//! The options and report types live in `extras` and `report`, the dealing
//! of a seed in `seed`, the mod's files in `write`, and putting it on the
//! targets in `install`.

mod extras;
mod install;
pub mod plando;
mod report;
mod seed;
mod write;

#[cfg(test)]
mod tests;

pub use extras::Extras;
pub use install::{install, running, uninstall};
pub use report::{Held, Report, RoundEvent, Spot, Start, Written};
pub use seed::{generate, shuffle, time_seed};
pub use write::write_mod;
