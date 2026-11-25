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
    Effects,
}

/// One thing a player recolours: its colour in the vanilla game for each
/// slot, and the writes that paint it.
pub struct Palette {
    pub id: &'static str,
    pub label: &'static str,
    pub group: Group,
    pub vanilla: &'static [Colour],
    /// Each slot's name, when there's more than one: what Split shows.
    pub names: &'static [&'static str],
    pub writes: &'static [Write],
}

impl Palette {
    /// `other` moved as `from` moved to `to`, the way this palette's parts
    /// follow its slots. An effect's parts follow by hue alone: their
    /// brightness is their own, and a lighter pick would wash them white.
    pub fn follow(&self, to: Colour, from: Colour, other: Colour) -> Colour {
        match self.group {
            Group::Effects => super::oklch::follow_hue(to, from, other),
            Group::Link | Group::Gear => super::oklch::follow(to, from, other),
        }
    }
}

/// One change a palette makes, following one of its slots.
pub struct Write {
    pub slot: usize,
    pub kind: Kind,
}

/// What a write changes. Each holds what the dump has there, so a newer
/// dump fails the build instead of being painted wrong.
pub enum Kind {
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
    /// A four-float colour in every material named `material` in each of
    /// `models`, in linear light: its brightest channel kept, so a gem
    /// keeps its depth. `vanilla` is its colour, linear.
    Param {
        models: &'static [&'static str],
        material: &'static str,
        hash: u64,
        vanilla: [f32; 3],
    },
    /// An effect's light. The game reads an effect's constants at run
    /// time only for its light: a particle's colour is compiled into its
    /// shader, and these constants change nothing there. Every coloured
    /// constant in `bundle`'s esets whose names start with one of `esets`: each turned as the slot turned from vanilla, by
    /// its own hue, its brightest channel kept as a param's is. Greys stay
    /// grey. `sum` is the crc32 of those esets' constants in the dump
    /// (`probe vfx BUNDLE --sum PREFIX`), so a newer dump fails the build.
    Vfx {
        bundle: &'static str,
        esets: &'static [&'static str],
        sum: u32,
    },
}

impl Write {
    /// The albedo it paints, or none for a param.
    pub fn texture(&self) -> Option<&'static Tex> {
        match self.kind {
            Kind::Band { texture, .. } | Kind::Tint { texture, .. } => Some(texture),
            Kind::Param { .. } | Kind::Vfx { .. } => None,
        }
    }

    /// Its part's colour in the vanilla game, as sRGB. None for an
    /// effect: each of its constants has its own.
    pub fn vanilla(&self) -> Option<Colour> {
        match self.kind {
            Kind::Band { vanilla, .. } | Kind::Tint { vanilla, .. } => Some(vanilla),
            Kind::Param { vanilla, .. } => Some(super::oklch::from_linear(vanilla)),
            Kind::Vfx { .. } => None,
        }
    }

    /// The same write, following slot `slot`.
    const fn at(self, slot: usize) -> Write {
        Write { slot, ..self }
    }
}

// The four colour params Link's and the gear's materials share, by name
// hash. What each means is the shader's: base, shade, rim or shine, tip.
const C0: u64 = 0x68e1_31e7_2bcd_91ad;
const C1: u64 = 0xe647_277d_46e8_c1d8;
const C2: u64 = 0x0187_464e_c654_1f6e;
const C3: u64 = 0x9613_83b7_9667_f911;

const fn tint(texture: &'static Tex, vanilla: Colour) -> Write {
    Write {
        slot: 0,
        kind: Kind::Tint { texture, vanilla },
    }
}

const fn param(
    models: &'static [&'static str],
    material: &'static str,
    hash: u64,
    vanilla: [f32; 3],
) -> Write {
    Write {
        slot: 0,
        kind: Kind::Param {
            models,
            material,
            hash,
            vanilla,
        },
    }
}

const fn vfx(bundle: &'static str, esets: &'static [&'static str], sum: u32) -> Write {
    Write {
        slot: 0,
        kind: Kind::Vfx { bundle, esets, sum },
    }
}

const fn band(texture: &'static Tex, hues: Hues, vanilla: Colour) -> Write {
    Write {
        slot: 0,
        kind: Kind::Band {
            texture,
            hues,
            vanilla,
        },
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

/// Palettes that move with another while not picked themselves:
/// (follower, leader).
pub const FOLLOWS: &[(&str, &str)] = &[("brows", "hair")];

pub const PALETTES: &[Palette] = &[
    Palette {
        id: "kokiri-tunic",
        label: "Kokiri Tunic",
        group: Group::Link,
        vanilla: &[[64, 82, 43]],
        names: &[],
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
        names: &[],
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
        names: &[],
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
        names: &[],
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
        id: "hair",
        label: "Hair",
        group: Group::Link,
        vanilla: &[[163, 126, 59]],
        names: &[],
        writes: &[
            band(&HAIR, (0.02, 0.2), [163, 126, 59]),
            band(&ADULT_HAIR, (0.02, 0.2), [252, 203, 75]),
            band(&CHILD_HAIR, (0.02, 0.2), [251, 207, 82]),
        ],
    },
    Palette {
        id: "brows",
        label: "Eyebrows and lashes",
        group: Group::Link,
        vanilla: &[
            [151, 108, 65],
            [172, 121, 27],
            [115, 88, 60],
            [181, 139, 94],
        ],
        names: &["Brows", "Shade", "Dark", "Light"],
        writes: &[
            param(
                &[ADULT, CHILD, CHILD_OP],
                EYEBROW,
                C0,
                [0.3093, 0.1511, 0.0521],
            ),
            param(&[ADULT, CHILD, CHILD_OP], EYEBROW, C1, [0.41, 0.19, 0.011]).at(1),
            param(
                &[ADULT, CHILD, CHILD_OP],
                EYEBROW,
                C2,
                [0.1706, 0.0982, 0.0454],
            )
            .at(2),
            param(
                &[ADULT, CHILD, CHILD_OP],
                EYEBROW,
                C3,
                [0.4624, 0.2567, 0.1114],
            )
            .at(3),
            param(&[ADULT], EYELASHES, C0, [0.2234, 0.1308, 0.0455]),
            param(&[ADULT], EYELASHES, C1, [0.2234, 0.1308, 0.0455]).at(1),
            param(&[ADULT], EYELASHES, C2, [0.0283, 0.0201, 0.0125]).at(2),
            param(&[ADULT], EYELASHES, C3, [0.48, 0.3244, 0.1619]).at(3),
            param(&[CHILD], EYELASHES_1, C0, [0.2085, 0.1385, 0.0541]),
            param(&[CHILD], EYELASHES_1, C1, [0.1047, 0.0651, 0.0363]).at(1),
            param(&[CHILD], EYELASHES_1, C2, [0.0331, 0.0253, 0.0189]).at(2),
            param(&[CHILD_OP], EYELASHES_1, C2, [0.0269, 0.023, 0.0344]).at(2),
            param(&[CHILD], EYELASHES_1, C3, [0.214, 0.1552, 0.0713]).at(3),
            param(&[CHILD_OP], EYELASHES_1, C3, [0.0436, 0.0374, 0.055]).at(3),
            param(&[CHILD], EYELASHES_2, C0, [0.2085, 0.1385, 0.0541]),
            param(&[CHILD], EYELASHES_2, C1, [0.1047, 0.0651, 0.0363]).at(1),
            param(&[CHILD], EYELASHES_2, C2, [0.0331, 0.0253, 0.0189]).at(2),
            param(&[CHILD_OP], EYELASHES_2, C2, [0.0269, 0.023, 0.0344]).at(2),
            param(&[CHILD], EYELASHES_2, C3, [0.214, 0.1088, 0.01]).at(3),
            param(&[CHILD_OP], EYELASHES_2, C3, [0.0436, 0.0374, 0.055]).at(3),
            param(&[CHILD], EYELASHES_3, C0, [0.2085, 0.1385, 0.0541]),
            param(&[CHILD], EYELASHES_3, C1, [0.1047, 0.0651, 0.0363]).at(1),
            param(&[CHILD], EYELASHES_3, C2, [0.0331, 0.0253, 0.0189]).at(2),
            param(&[CHILD_OP], EYELASHES_3, C2, [0.0269, 0.023, 0.0344]).at(2),
            param(&[CHILD], EYELASHES_3, C3, [0.214, 0.1552, 0.0713]).at(3),
            param(&[CHILD_OP], EYELASHES_3, C3, [0.0436, 0.0374, 0.055]).at(3),
        ],
    },
    Palette {
        id: "kokiri-sword",
        label: "Kokiri Sword hilt",
        group: Group::Gear,
        vanilla: &[[87, 67, 48]],
        names: &[],
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
        names: &[],
        writes: &[band(&BIGGORON_SWORD, BLUE, [53, 80, 148])],
    },
    Palette {
        id: "deku-shield",
        label: "Deku Shield",
        group: Group::Gear,
        vanilla: &[[110, 86, 59]],
        names: &[],
        writes: &[band(&DEKU_SHIELD, WOOD, [110, 86, 59])],
    },
    Palette {
        id: "hylian-shield",
        label: "Hylian Shield face",
        group: Group::Gear,
        vanilla: &[[41, 45, 90]],
        names: &[],
        writes: &[band(&HYLIAN_SHIELD, BLUE, [41, 45, 90])],
    },
    Palette {
        id: "mirror-shield",
        label: "Mirror Shield frame",
        group: Group::Gear,
        vanilla: &[[109, 43, 31]],
        names: &[],
        writes: &[band(&MIRROR_SHIELD, (0.92, 0.04), [109, 43, 31])],
    },
    Palette {
        id: "iron-boots",
        label: "Iron Boots",
        group: Group::Gear,
        vanilla: &[[115, 104, 97]],
        names: &[],
        writes: &[tint(&IRON_BOOTS, [115, 104, 97])],
    },
    Palette {
        id: "hammer",
        label: "Megaton Hammer",
        group: Group::Gear,
        vanilla: &[[130, 120, 107]],
        names: &[],
        writes: &[tint(&HAMMER, [130, 120, 107])],
    },
    Palette {
        id: "bow",
        label: "Fairy Bow",
        group: Group::Gear,
        vanilla: &[[140, 38, 21]],
        names: &[],
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
        names: &[],
        writes: &[band(&SLINGSHOT, WOOD, [152, 108, 71])],
    },
    Palette {
        id: "hookshot",
        label: "Hookshot and Longshot",
        group: Group::Gear,
        vanilla: &[[110, 57, 128]],
        names: &[],
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
        names: &[],
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
        names: &[],
        writes: &[band(&OCARINA_OF_TIME, (0.5, 0.7), [33, 66, 95])],
    },
    Palette {
        id: "master-sword",
        label: "Master Sword",
        group: Group::Gear,
        vanilla: &[
            [78, 79, 137],
            [41, 153, 170],
            [235, 175, 255],
            [255, 255, 0],
        ],
        names: &["Hilt", "Sheen", "Sheen 2", "Sheen 3"],
        writes: &[
            band(&MASTER_SWORD, (0.55, 0.75), [78, 79, 137]),
            param(
                &[MASTER, MASTER_PEDESTAL],
                SWORD_SHEEN,
                C0,
                [0.0225, 0.3182, 0.4024],
            )
            .at(1),
            param(
                &[MASTER, MASTER_PEDESTAL],
                SWORD_SHEEN,
                C1,
                [0.8317, 0.4283, 1.0],
            )
            .at(2),
            param(&[MASTER, MASTER_PEDESTAL], SWORD_SHEEN, C2, [1.0, 1.0, 0.0]).at(3),
        ],
    },
    Palette {
        id: "silver-gauntlets",
        label: "Silver Gauntlets",
        group: Group::Gear,
        vanilla: &[[148, 141, 134], [179, 89, 89]],
        names: &["Metal", "Gem"],
        writes: &[
            tint(&SILVER_GAUNTLETS, [148, 141, 134]),
            param(&[SILVER], JEWELRY, C0, [0.448, 0.1005, 0.1005]).at(1),
            param(&[SILVER], JEWELRY, C1, [0.0597, 0.0, 0.0245]).at(1),
            param(&[SILVER], JEWELRY, C2, [1.0, 0.0546, 0.0]).at(1),
        ],
    },
    Palette {
        id: "gold-gauntlets",
        label: "Golden Gauntlets",
        group: Group::Gear,
        vanilla: &[[214, 181, 114], [179, 89, 89]],
        names: &["Gold", "Gem"],
        writes: &[
            band(&GOLD_GAUNTLETS, (0.07, 0.18), [214, 181, 114]),
            param(&[GOLD], JEWELRY, C0, [0.448, 0.1005, 0.1005]).at(1),
            param(&[GOLD], JEWELRY, C1, [0.0597, 0.0, 0.0245]).at(1),
            param(&[GOLD], JEWELRY, C2, [1.0, 0.0546, 0.0]).at(1),
        ],
    },
    Palette {
        id: "goron-bracelet",
        label: "Goron's Bracelet",
        group: Group::Gear,
        vanilla: &[[212, 164, 90], [255, 13, 0]],
        names: &["Gold", "Gem"],
        writes: &[
            band(&GORON_BRACELET, (0.07, 0.18), [212, 164, 90]),
            param(&[BRACELET], BRACELET_GLASS, C0, [1.0, 0.0039, 0.0]).at(1),
            param(&[BRACELET], BRACELET_GLASS, C1, [0.1329, 0.0, 0.0331]).at(1),
            param(&[BRACELET], BRACELET_GLASS, C2, [0.6921, 0.0946, 0.1511]).at(1),
        ],
    },
    Palette {
        id: "hover-boots",
        label: "Hover Boots",
        group: Group::Gear,
        vanilla: &[[172, 151, 85], [250, 255, 191]],
        names: &["Gold", "Glow"],
        writes: &[
            band(&HOVER_BOOTS, (0.09, 0.18), [172, 151, 85]),
            param(&[HOVER], BOOTS_GLASS, C0, [0.1329, 0.1233, 0.0637]).at(1),
            param(&[HOVER], BOOTS_GLASS, C1, [0.9533, 1.0, 0.5225]).at(1),
        ],
    },
    Palette {
        id: "boomerang",
        label: "Boomerang",
        group: Group::Gear,
        vanilla: &[[98, 67, 50], [221, 22, 29], [44, 76, 51], [61, 125, 139]],
        names: &["Wood", "Gem", "Tiles", "Inlay"],
        writes: &[
            band(&BOOMERANG, (0.025, 0.08), [98, 67, 50]),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_GEM,
                C0,
                [0.2499, 0.0019, 0.0019],
            )
            .at(1),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_GEM,
                C1,
                [0.7237, 0.0079, 0.0125],
            )
            .at(1),
            param(&[BOOMERANG_MODEL], BOOMERANG_GEM, C2, [1.0, 0.0, 0.0281]).at(1),
            band(&BOOMERANG, (0.3, 0.45), [44, 76, 51]).at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_1,
                C0,
                [0.0056, 0.0331, 0.0045],
            )
            .at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_1,
                C1,
                [0.1511, 0.0489, 0.0958],
            )
            .at(2),
            param(&[BOOMERANG_MODEL], BOOMERANG_TILE_1, C2, [0.0, 0.32, 0.17]).at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_1,
                C3,
                [0.62, 0.3672, 0.2685],
            )
            .at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_2,
                C0,
                [0.0367, 0.0835, 0.0001],
            )
            .at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_2,
                C1,
                [0.25, 0.1706, 0.0055],
            )
            .at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_2,
                C2,
                [0.24, 0.41, 0.0809],
            )
            .at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_2,
                C3,
                [0.68, 0.4995, 0.2094],
            )
            .at(2),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_3,
                C0,
                [0.007, 0.0509, 0.0349],
            )
            .at(3),
            param(
                &[BOOMERANG_MODEL],
                BOOMERANG_TILE_3,
                C1,
                [0.0466, 0.2049, 0.2581],
            )
            .at(3),
            param(&[BOOMERANG_MODEL], BOOMERANG_TILE_3, C2, [0.22, 0.19, 0.2]).at(3),
            param(&[BOOMERANG_MODEL], BOOMERANG_TILE_3, C3, [0.06, 0.54, 0.28]).at(3),
        ],
    },
    Palette {
        id: "navi-idle",
        label: "Navi, idle",
        group: Group::Effects,
        vanilla: &[[222, 215, 203], [225, 249, 255]],
        names: &["Inner", "Outer"],
        writes: &[
            param(&[NAVI], NAVI_BODY, C1, [0.7312, 0.6775, 0.5954]),
            param(&[NAVI], NAVI_BODY_MASK, C1, [0.7312, 0.6775, 0.5954]),
            vfx(STATIC_FX, &["Npc_Navi_Wait_"], 0xf96a_bf84).at(1),
            vfx(NAVI_FX, &["Npc_Navi_Wait_"], 0xf96a_bf84).at(1),
        ],
    },
];
