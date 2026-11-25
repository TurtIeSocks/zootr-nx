//! Every palette a seed can recolour, and what each one paints. See
//! `superpowers/specs/2026-11-24-colors-design.md`, The catalog.

use serde::Serialize;

use super::files::*;
use super::paint::{Colour, Hues};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Link,
    Gear,
}

/// One thing a player recolours: its colour in the vanilla game for each
/// slot, and the writes that paint it.
pub struct Palette {
    pub id: &'static str,
    pub label: &'static str,
    pub group: Group,
    pub vanilla: &'static [Colour],
    pub writes: &'static [Write],
}

/// One change a palette makes. Each holds what the dump has there, so a
/// newer dump fails the build instead of being painted wrong.
pub enum Write {
    /// The texels in a band of hues moved to the colour, keeping their
    /// grain. `vanilla` is the band's average colour.
    Band {
        texture: &'static Tex,
        hues: Hues,
        vanilla: Colour,
    },
    /// Every texel the colour, by its brightness. `vanilla` is the
    /// texture's average colour.
    Tint {
        texture: &'static Tex,
        vanilla: Colour,
    },
}

impl Write {
    pub fn texture(&self) -> &'static Tex {
        match self {
            Write::Band { texture, .. } | Write::Tint { texture, .. } => texture,
        }
    }

    pub fn vanilla(&self) -> Colour {
        match self {
            Write::Band { vanilla, .. } | Write::Tint { vanilla, .. } => *vanilla,
        }
    }
}

const fn band(texture: &'static Tex, hues: Hues, vanilla: Colour) -> Write {
    Write::Band {
        texture,
        hues,
        vanilla,
    }
}

/// Link's green, his vest's yellower stripes among it. His vest's brown
/// patch keeps its colour.
pub const GREEN: Hues = (0.15, 0.5);
/// The Goron Tunic's red, either side of 0. Its tan trim stays.
const RED: Hues = (0.9, 0.06);
const BLUE: Hues = (0.5, 0.75);
/// Leather and wood, short of red and of green.
const BROWN: Hues = (0.02, 0.14);
/// A part wholly wood or clay, its reds and yellows with it.
const WOOD: Hues = (0.95, 0.2);

pub const PALETTES: &[Palette] = &[
    Palette {
        id: "kokiri-tunic",
        label: "Kokiri Tunic",
        group: Group::Link,
        vanilla: &[[64, 82, 43]],
        writes: &[
            band(&CLOTH_TILE, GREEN, [64, 82, 43]),
            band(&ADULT_VEST, GREEN, [57, 68, 47]),
            band(&ADULT_CAP, GREEN, [74, 81, 43]),
            band(&CHILD_VEST, GREEN, [58, 69, 48]),
            band(&CHILD_PANTS, GREEN, [51, 66, 34]),
            band(&CHILD_PARTS, GREEN, [74, 87, 44]),
            band(&CHILD_LEATHER, GREEN, [52, 52, 26]),
        ],
    },
    Palette {
        id: "goron-tunic",
        label: "Goron Tunic",
        group: Group::Link,
        vanilla: &[[91, 25, 26]],
        writes: &[
            band(&GORON_CLOTH, RED, [91, 25, 26]),
            band(&GORON_VEST, RED, [53, 30, 32]),
            band(&GORON_INNER, RED, [65, 47, 53]),
        ],
    },
    Palette {
        id: "zora-tunic",
        label: "Zora Tunic",
        group: Group::Link,
        vanilla: &[[62, 88, 143]],
        writes: &[
            band(&ZORA_CLOTH, BLUE, [62, 88, 143]),
            band(&ZORA_VEST, BLUE, [46, 56, 84]),
            band(&ZORA_CAP, BLUE, [48, 60, 94]),
            band(&ZORA_PARTS, BLUE, [90, 124, 151]),
            band(&ZORA_FEATHER, BLUE, [99, 128, 149]),
        ],
    },
    Palette {
        id: "leather",
        label: "Belt, straps, gloves, boots",
        group: Group::Link,
        vanilla: &[[111, 86, 65]],
        writes: &[
            band(&ADULT_LEATHER, BROWN, [111, 86, 65]),
            band(&ADULT_GLOVE, BROWN, [86, 66, 51]),
            band(&ADULT_BOOTS, BROWN, [99, 76, 56]),
            band(&CHILD_LEATHER, BROWN, [81, 61, 42]),
            band(&GORON_LEATHER, BROWN, [115, 86, 62]),
            band(&ZORA_LEATHER, BROWN, [117, 94, 77]),
        ],
    },
    Palette {
        id: "kokiri-sword",
        label: "Kokiri Sword hilt",
        group: Group::Gear,
        vanilla: &[[87, 67, 48]],
        writes: &[
            band(&KOKIRI_SWORD, BROWN, [87, 67, 48]),
            band(&KOKIRI_SWORD, (0.92, 0.02), [128, 57, 55]),
        ],
    },
    Palette {
        id: "biggoron-sword",
        label: "Giant's Knife and Biggoron's Sword hilt",
        group: Group::Gear,
        vanilla: &[[53, 80, 148]],
        writes: &[band(&BIGGORON_SWORD, BLUE, [53, 80, 148])],
    },
    Palette {
        id: "deku-shield",
        label: "Deku Shield",
        group: Group::Gear,
        vanilla: &[[110, 86, 59]],
        writes: &[band(&DEKU_SHIELD, WOOD, [110, 86, 59])],
    },
    Palette {
        id: "hylian-shield",
        label: "Hylian Shield face",
        group: Group::Gear,
        vanilla: &[[41, 45, 90]],
        writes: &[band(&HYLIAN_SHIELD, BLUE, [41, 45, 90])],
    },
    Palette {
        id: "mirror-shield",
        label: "Mirror Shield frame",
        group: Group::Gear,
        vanilla: &[[109, 43, 31]],
        writes: &[band(&MIRROR_SHIELD, (0.92, 0.04), [109, 43, 31])],
    },
    Palette {
        id: "iron-boots",
        label: "Iron Boots",
        group: Group::Gear,
        vanilla: &[[115, 104, 97]],
        writes: &[Write::Tint {
            texture: &IRON_BOOTS,
            vanilla: [115, 104, 97],
        }],
    },
    Palette {
        id: "hammer",
        label: "Megaton Hammer",
        group: Group::Gear,
        vanilla: &[[130, 120, 107]],
        writes: &[Write::Tint {
            texture: &HAMMER,
            vanilla: [130, 120, 107],
        }],
    },
    Palette {
        id: "bow",
        label: "Fairy Bow",
        group: Group::Gear,
        vanilla: &[[140, 38, 21]],
        writes: &[
            band(&BOW, (0.93, 0.035), [140, 38, 21]),
            band(&BOW, (0.6, 0.72), [47, 49, 92]),
        ],
    },
    Palette {
        id: "slingshot",
        label: "Slingshot",
        group: Group::Gear,
        vanilla: &[[152, 108, 71]],
        writes: &[band(&SLINGSHOT, WOOD, [152, 108, 71])],
    },
    Palette {
        id: "hookshot",
        label: "Hookshot and Longshot",
        group: Group::Gear,
        vanilla: &[[110, 57, 128]],
        writes: &[
            band(&HOOKSHOT, (0.72, 0.84), [110, 57, 128]),
            // Its magenta trim apart, so it follows the purple by eye, not
            // by HSV hue (which lands a gold pick's trim on lime).
            band(&HOOKSHOT, (0.84, 0.95), [110, 46, 91]),
            band(&HOOKSHOT, (0.5, 0.66), [75, 95, 130]),
        ],
    },
    Palette {
        id: "fairy-ocarina",
        label: "Fairy Ocarina",
        group: Group::Gear,
        vanilla: &[[139, 111, 69]],
        writes: &[
            band(&FAIRY_OCARINA[0], (0.03, 0.2), [139, 111, 69]),
            band(&FAIRY_OCARINA[1], (0.03, 0.2), [139, 111, 69]),
        ],
    },
    Palette {
        id: "ocarina-of-time",
        label: "Ocarina of Time",
        group: Group::Gear,
        vanilla: &[[33, 66, 95]],
        writes: &[band(&OCARINA_OF_TIME, (0.5, 0.7), [33, 66, 95])],
    },
];
