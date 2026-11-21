//! What game code asks the world, by node class: a class whose answer
//! comes from the clock, Link's age, chance or the controller, outside any
//! data. Each row is checked in the game (notes in the private research,
//! never here); a class not listed may still ask, and `flagmap stats`
//! lists the most used ones no row covers.

use serde::Serialize;

use crate::known::{Name, field};

/// Something game code reads that no data holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    Clock,
    Age,
    Random,
    Input,
}

impl Game {
    pub const ALL: [Game; 4] = [Game::Clock, Game::Age, Game::Random, Game::Input];

    pub fn word(self) -> &'static str {
        match self {
            Game::Clock => "clock",
            Game::Age => "age",
            Game::Random => "random",
            Game::Input => "input",
        }
    }
}

/// Node classes and what each asks.
pub const ASKS: &[(&Name, Game)] = &[
    (field::HOLD_WM_GET_TIME_DIV, Game::Clock),
    (field::QUERY_TIME_DIVISION, Game::Clock),
    (field::EXECUTE_SKULLTULA_GOLD_EMIT_TIME, Game::Clock),
    (field::HOLD_IS_CHILDHOOD_OR_ADULTHOOD, Game::Age),
];
