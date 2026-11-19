//! Randomizer generator for Ocarina of Time on Switch 2.

/// Returns from a test early without the dev dump, which is where tests
/// read the game's names. See `game::no_names`.
#[cfg(test)]
macro_rules! needs_names {
    () => {
        if $crate::game::no_names() {
            return;
        }
    };
}

mod ageless;
mod ammo;
pub mod ap;
pub mod around;
mod arrivals;
mod bombchus;
mod bombs;
mod bridge;
pub mod build;
mod checks;
mod chest_levels;
pub mod config;
mod cows;
mod cut;
pub mod cutscenes;
mod dampe;
pub mod dev;
mod dig;
mod door;
mod enemies;
mod events;
mod fill;
mod flags;
mod flows;
mod foreign;
mod fortress;
pub mod game;
pub mod gamedata;
pub mod gates;
mod gear;
pub mod generate;
mod gerudo;
mod gifts;
mod grant;
mod graphs;
mod held;
pub mod hints;
pub mod history;
mod ice_traps;
mod icons;
pub mod inventory;
pub mod items;
pub mod kept;
mod key_rooms;
mod key_tints;
mod keys;
#[macro_use]
pub mod known;
pub mod labels;
mod ledger;
pub mod live;
mod logic;
pub mod look;
mod map_copies;
mod medallions;
mod names;
mod narrowing;
pub mod nav;
mod navi;
pub mod notice;
mod patch;
mod picked;
mod pickups;
pub mod places;
pub mod platform;
pub mod play;
pub mod presets;
mod probe;
pub mod progress;
mod progressive;
pub mod recorder;
mod rng;
mod rsdb;
mod rules;
pub mod save_editor;
pub mod savefile;
mod scrubs;
pub mod settings;
pub mod share;
mod shops;
mod shows;
mod shuffle;
pub mod spawn;
pub mod sprites;
mod standins;
pub mod starting;
pub mod steps;
pub mod sweep;
mod text;
mod tools;
pub mod tracker;
mod trade_chain;
mod trade_slot;
pub mod turn;
mod twins;
pub mod value;
pub mod vision;
pub mod warp;
mod world;

pub mod cli;
