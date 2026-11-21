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

/// Where the Gerudo thieves' keys are: [`Keys`], or shuffled one by one.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum FortressKeys {
    /// Carried by the thieves, as in the game
    #[default]
    Vanilla,
    /// Held from the start, so the keys the thieves carry are checks
    Start,
    /// Each key on its own, at any check, and the thieves' keys are checks
    Singles,
}

impl FortressKeys {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Vanilla => Keys::Vanilla.label(),
            Self::Start => Keys::Start.label(),
            Self::Singles => DungeonKeys::Singles.label(),
        }
    }

    /// Whether the keys the thieves carry are checks.
    pub fn moved(self) -> bool {
        self != Self::Vanilla
    }

    /// Whether the keys come as copies the hook credits to the Fortress.
    pub fn copies(self) -> bool {
        self == Self::Singles
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

/// When Jabu-Jabu waits in the adult fountain.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum AdultJabu {
    /// Never: the adult fountain has no Jabu
    #[default]
    Off,
    /// From the start
    Always,
    /// Once the Water sage is done, after Morpha
    AfterMorpha,
}

impl AdultJabu {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Always => "Always",
            Self::AfterMorpha => "After Morpha",
        }
    }
}

/// Which dungeon doors lead to another dungeon.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum DungeonEntrances {
    /// Every door leads where it always does
    #[default]
    Off,
    /// The eleven dungeons swap doors
    On,
    /// The eleven dungeons and Ganon's Castle swap doors
    OnGanon,
}

impl DungeonEntrances {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
            Self::OnGanon => "On, with Ganon's Castle",
        }
    }

    /// Any door can lead adult Link to the Deku Tree, so its debris goes.
    pub(super) fn forces(self) -> Forces {
        match self {
            Self::Off => NOTHING,
            Self::On | Self::OnGanon => (&[crate::presets::ADULT_DEKU_TREE], &[]),
        }
    }
}

/// What a choice's pick turns on: toggles, then start inventory words.
pub(super) type Forces = (&'static [&'static str], &'static [&'static str]);
const NOTHING: Forces = (&[], &[]);

/// How old Link is on a new file.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum StartingAge {
    /// A child in Link's house, as the game starts
    #[default]
    Child,
    /// An adult in the Temple of Time, holding the Master Sword
    Adult,
    /// Child or adult, rolled from the seed
    Random,
}

impl StartingAge {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Child => "Child",
            Self::Adult => "Adult",
            Self::Random => "Random",
        }
    }

    /// Adult goes back to child at the pedestal, which only the open door
    /// swaps at once, and a child sent back mustn't be shut in the forest.
    /// His sword is the pedestal's gift, given in the opening.
    pub(super) fn forces(self) -> Forces {
        match self {
            Self::Adult => (&[super::OPEN_DOOR, crate::presets::OPEN_FOREST], &[]),
            Self::Child | Self::Random => NOTHING,
        }
    }

    /// `self` with random rolled from `seed`.
    pub fn resolve(self, seed: u64) -> Self {
        match self {
            Self::Random => match crate::rng::Rng::new(seed.rotate_left(17)).below(2) {
                0 => Self::Child,
                _ => Self::Adult,
            },
            age => age,
        }
    }
}

/// What caps the Bombchus Link can hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum Bombchus {
    /// The bomb bag's cap, as in the game
    BombBag,
    /// Their own cap, the Biggest Bomb Bag's
    #[default]
    Free,
    /// Three Bombchu Bags in the pool, each a size up
    Bag,
}

impl Bombchus {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::BombBag => "The bomb bag's",
            Self::Free => "Their own, 40",
            Self::Bag => "Bombchu Bags",
        }
    }
}
