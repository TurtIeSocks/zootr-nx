//! What game code asks the world, by node class: a class whose answer
//! comes from the clock, Link's age, chance or the controller, outside any
//! data, or from what Link wears. Each row is checked in the game (notes in the private research,
//! never here); a class not listed may still ask, and `flagmap stats`
//! lists the most used ones no row covers.

use serde::Serialize;

use crate::known::{Name, actor, field};

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
    (field::QUERY_IS_CHILDHOOD, Game::Age),
];

/// Node classes and the equipment each asks about or changes: whether Link
/// wears it, or putting it on.
pub const ABOUT: &[(&Name, &Name)] = &[
    (
        field::HOLD_PLAYER_CHECK_BOOTS,
        actor::ARMOR_ADULT_HEAVY_BOOT,
    ),
    (
        field::HOLD_PLAYER_CHECK_BOOTS,
        actor::ARMOR_ADULT_HOVER_BOOT,
    ),
    (
        field::QUERY_IS_CONTACT_PLAYER_EQUIP_BOOTS,
        actor::ARMOR_ADULT_HEAVY_BOOT,
    ),
    (
        field::QUERY_IS_CONTACT_PLAYER_EQUIP_BOOTS,
        actor::ARMOR_ADULT_HOVER_BOOT,
    ),
    (
        field::EXECUTE_PLAYER_BOOTS_CHANGE,
        actor::ARMOR_ADULT_HEAVY_BOOT,
    ),
    (
        field::EXECUTE_PLAYER_BOOTS_CHANGE,
        actor::ARMOR_ADULT_HOVER_BOOT,
    ),
    (
        field::SELECTOR_PLAYER_SCHEDULER_BOOTS_CHANGE,
        actor::ARMOR_ADULT_HEAVY_BOOT,
    ),
    (
        field::SELECTOR_PLAYER_SCHEDULER_BOOTS_CHANGE,
        actor::ARMOR_ADULT_HOVER_BOOT,
    ),
];
