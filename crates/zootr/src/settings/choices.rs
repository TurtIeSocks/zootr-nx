//! The settings that pick one of several values.

use clap::ValueEnum;

/// How big each chest is.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum ChestSize {
    /// As the game has them
    #[default]
    Vanilla,
    /// Every big chest becomes a small one
    Small,
}

impl ChestSize {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Small => "All small",
        }
    }
}

/// How items are placed.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum Logic {
    /// Every check Link can be shown to reach is reachable, without tricks
    #[default]
    Glitchless,
    /// Items anywhere, so a seed may be unbeatable
    NoLogic,
}

impl Logic {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Glitchless => "Glitchless",
            Self::NoLogic => "No logic",
        }
    }
}

/// Whether Link starts with keys he'd otherwise find.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum Keys {
    /// Found in their chests, as in the game
    #[default]
    Vanilla,
    /// Held from the start, so the chests that held them are checks
    Start,
}

impl Keys {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Start => "Start with them",
        }
    }
}

/// Where a dungeon's keys are: [`Keys`], or shuffled.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum DungeonKeys {
    /// Found in their chests, as in the game
    #[default]
    Vanilla,
    /// Held from the start, so the chests that held them are checks
    Start,
    /// Each dungeon's small keys as one ring, and its boss key, in any chest
    Rings,
    /// Each small key and boss key on its own, in any chest
    Singles,
}

impl DungeonKeys {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Rings => "Key rings",
            Self::Singles => "Keysanity",
            Self::Vanilla => Keys::Vanilla.label(),
            Self::Start => Keys::Start.label(),
        }
    }

    /// Whether the chests that held keys are checks.
    pub fn moved(self) -> bool {
        self != Self::Vanilla
    }

    /// Whether small keys come as copies the hook credits to their own
    /// dungeon, dealt with the boss keys to any chest.
    pub fn copies(self) -> bool {
        matches!(self, Self::Rings | Self::Singles)
    }
}

/// How many slots of each shop sell a check, with shopsanity on.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum ShopSlots {
    #[value(name = "1")]
    One,
    #[value(name = "2")]
    Two,
    #[value(name = "3")]
    Three,
    #[default]
    #[value(name = "4")]
    Four,
    #[value(name = "5")]
    Five,
    #[value(name = "6")]
    Six,
    #[value(name = "7")]
    Seven,
    #[value(name = "8")]
    Eight,
}

impl ShopSlots {
    pub(super) fn label(self) -> &'static str {
        ["1", "2", "3", "4", "5", "6", "7", "8 (every slot)"][self.count() - 1]
    }

    pub fn count(self) -> usize {
        self as usize + 1
    }
}

/// What every Business Scrub asks for its check.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum ScrubPrice {
    #[value(name = "10")]
    Ten,
    #[value(name = "20")]
    Twenty,
    #[default]
    #[value(name = "40")]
    Forty,
    #[value(name = "60")]
    Sixty,
    #[value(name = "90")]
    Ninety,
}

impl ScrubPrice {
    /// Every price fits the child's wallet, so logic never needs a bigger
    /// one for a scrub.
    pub fn rupees(self) -> i32 {
        [10, 20, 40, 60, 90][self as usize]
    }

    pub(super) fn label(self) -> &'static str {
        [
            "10 rupees",
            "20 rupees",
            "40 rupees",
            "60 rupees",
            "90 rupees",
        ][self as usize]
    }
}

/// How much of the seed's junk becomes ice traps.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum IceTraps {
    /// None
    #[default]
    Off,
    /// One in ten
    Some,
    /// Half
    Lots,
    /// All of it
    Mayhem,
}

impl IceTraps {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Some => "Some (1 in 10)",
            Self::Lots => "Lots (half)",
            Self::Mayhem => "Mayhem (all junk)",
        }
    }

    /// How many of `junk` pieces become traps: a tenth and a half rounded
    /// up, so a seed with any junk gets at least one.
    pub fn of(self, junk: usize) -> usize {
        match self {
            Self::Off => 0,
            Self::Some => junk.div_ceil(10),
            Self::Lots => junk.div_ceil(2),
            Self::Mayhem => junk,
        }
    }
}
