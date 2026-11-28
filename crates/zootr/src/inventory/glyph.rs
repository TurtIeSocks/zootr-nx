//! The stand-in each icon draws when game art is off, in our own words.
//! The app maps a [`Glyph`] to an icon of its own, so it holds no game names.

use serde::Serialize;

use crate::known::{Name, actor, item};

/// What a cell draws when the game's art is off. Tiers share one, as the
/// swords and the three wallets do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum Glyph {
    Sticks,
    Nuts,
    Bombs,
    Bombchus,
    Bow,
    Slingshot,
    Ocarina,
    Hookshot,
    Boomerang,
    Beans,
    ScrubSeeds,
    Lens,
    Hammer,
    FireArrow,
    FireMedallion,
    IceArrow,
    LightArrow,
    LightMedallion,
    DinsFire,
    FaroresWind,
    NayrusLove,
    Bottle,
    Egg,
    Cucco,
    Letter,
    Mask,
    TruthMask,
    Mushroom,
    Saw,
    Sword,
    Prescription,
    Frog,
    EyeDrops,
    ClaimCheck,
    Shield,
    Tunic,
    IronBoots,
    HoverBoots,
    Gauntlets,
    Scale,
    Wallet,
    BombBag,
    Quiver,
    BulletBag,
    HeartPiece,
    Heart,
    Rupee,
    Magic,
    Spin,
    Defense,
    GerudoCard,
    AgonyStone,
    Skulltula,
    Song,
    Stone,
    ForestMedallion,
    WaterMedallion,
    SpiritMedallion,
    ShadowMedallion,
    SmallKey,
    BossKey,
    Map,
    Compass,
    Unknown,
}

/// Each icon a cell can carry, with its glyph.
const GLYPHS: &[(&Name, Glyph)] = &[
    (item::DEKU_STICK, Glyph::Sticks),
    (item::STICK_BAG_SMALL, Glyph::Sticks),
    (item::STICK_BAG_MEDIUM, Glyph::Sticks),
    (item::STICK_BAG_LARGE, Glyph::Sticks),
    (item::DEKU_NUT, Glyph::Nuts),
    (item::NUT_BAG_SMALL, Glyph::Nuts),
    (item::NUT_BAG_MEDIUM, Glyph::Nuts),
    (item::NUT_BAG_LARGE, Glyph::Nuts),
    (item::BOMB, Glyph::Bombs),
    (item::BOMBCHU, Glyph::Bombchus),
    (crate::bombchu_bags::SMALL, Glyph::Bombchus),
    (crate::bombchu_bags::MEDIUM, Glyph::Bombchus),
    (crate::bombchu_bags::LARGE, Glyph::Bombchus),
    (actor::WEAPON_RANGE_BOW, Glyph::Bow),
    (actor::WEAPON_RANGE_SLING_SHOT, Glyph::Slingshot),
    (item::OCARINA_FAIRY, Glyph::Ocarina),
    (item::OCARINA_TIME, Glyph::Ocarina),
    (item::HOOKSHOT, Glyph::Hookshot),
    (item::LONGSHOT, Glyph::Hookshot),
    (item::BOOMERANG, Glyph::Boomerang),
    (item::MAGIC_BEAN, Glyph::Beans),
    (item::SCRUB_BUSINESS_SEED, Glyph::ScrubSeeds),
    (actor::ARMOR_GLASS_TRUTH, Glyph::Lens),
    (item::HAMMER, Glyph::Hammer),
    (item::ARROW_FIRE, Glyph::FireArrow),
    (item::MEDAL_FIRE, Glyph::FireMedallion),
    (item::ARROW_ICE, Glyph::IceArrow),
    (item::ARROW_LIGHT, Glyph::LightArrow),
    (item::MEDAL_LIGHT, Glyph::LightMedallion),
    (item::DIN_FIRE, Glyph::DinsFire),
    (item::FARORE_WIND, Glyph::FaroresWind),
    (item::NAYRU_LOVE, Glyph::NayrusLove),
    (item::BOTTLE, Glyph::Bottle),
    (item::WEIRD_EGG, Glyph::Egg),
    (item::POCKET_EGG, Glyph::Egg),
    (item::ALARM_CUCCO, Glyph::Cucco),
    (item::POCKET_CUCCO, Glyph::Cucco),
    (item::COJIRO_CUCCO, Glyph::Cucco),
    (item::ZELDA_LETTER, Glyph::Letter),
    (actor::ARMOR_CHILD_MASK_KEATON, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_SKULL, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_SPOOKY, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_BUNNY, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_GORON, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_ZORA, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_GERUDO, Glyph::Mask),
    (actor::ARMOR_CHILD_MASK_TRUTH, Glyph::TruthMask),
    (item::ODD_MUSHROOM, Glyph::Mushroom),
    (item::ODD_POWDER, Glyph::Mushroom),
    (item::POACHER_SAW, Glyph::Saw),
    (item::BROKEN_SWORD, Glyph::Sword),
    (actor::WEAPON_SWORD_KOKIRI, Glyph::Sword),
    (actor::WEAPON_SWORD_MASTER, Glyph::Sword),
    (actor::WEAPON_SWORD_GIANT, Glyph::Sword),
    (actor::WEAPON_SWORD_BIGGORON, Glyph::Sword),
    (item::PRESCRIPTION, Glyph::Prescription),
    (item::EYEBALL_FROG, Glyph::Frog),
    (item::EYE_DROP, Glyph::EyeDrops),
    (item::CLAIM_CHECK, Glyph::ClaimCheck),
    (actor::WEAPON_SHIELD_DEKU, Glyph::Shield),
    (actor::WEAPON_SHIELD_HYLIAN, Glyph::Shield),
    (actor::WEAPON_SHIELD_MIRROR, Glyph::Shield),
    (actor::ARMOR_ADULT_NORMAL, Glyph::Tunic),
    (actor::ARMOR_ADULT_GORON, Glyph::Tunic),
    (actor::ARMOR_ADULT_ZORA, Glyph::Tunic),
    (actor::ARMOR_ADULT_HEAVY_BOOT, Glyph::IronBoots),
    (actor::ARMOR_ADULT_HOVER_BOOT, Glyph::HoverBoots),
    (actor::ARMOR_CHILD_GAUNTLET_GORON, Glyph::Gauntlets),
    (actor::ARMOR_ADULT_GAUNTLET_SILVER, Glyph::Gauntlets),
    (actor::ARMOR_ADULT_GAUNTLET_GOLD, Glyph::Gauntlets),
    (item::SCALE_SILVER, Glyph::Scale),
    (item::SCALE_GOLD, Glyph::Scale),
    (item::WALLET_SMALL, Glyph::Wallet),
    (item::WALLET_MEDIUM, Glyph::Wallet),
    (item::WALLET_LARGE, Glyph::Wallet),
    (item::BOMB_BAG_SMALL, Glyph::BombBag),
    (item::BOMB_BAG_MEDIUM, Glyph::BombBag),
    (item::BOMB_BAG_LARGE, Glyph::BombBag),
    (item::QUIVER_SMALL, Glyph::Quiver),
    (item::QUIVER_MEDIUM, Glyph::Quiver),
    (item::QUIVER_LARGE, Glyph::Quiver),
    (item::BULLET_BAG_SMALL, Glyph::BulletBag),
    (item::BULLET_BAG_MEDIUM, Glyph::BulletBag),
    (item::BULLET_BAG_LARGE, Glyph::BulletBag),
    (actor::OBJ_HEART_PIECE, Glyph::HeartPiece),
    (crate::magic::meter::METER, Glyph::Magic),
    (item::FAIRY_BLESS_MP_DOUBLE, Glyph::Magic),
    (item::FAIRY_BLESS_MP, Glyph::Spin),
    (item::FAIRY_BLESS_DEFENSE_DOUBLE, Glyph::Defense),
    (item::TOKEN_GERUDO, Glyph::GerudoCard),
    (item::AGONY_STONE, Glyph::AgonyStone),
    (item::TOKEN_GOLD, Glyph::Skulltula),
    (item::MELODY_ZELDA_LULLABY, Glyph::Song),
    (item::MELODY_EPONA_SONG, Glyph::Song),
    (item::MELODY_SARIA_SONG, Glyph::Song),
    (item::MELODY_SUN_SONG, Glyph::Song),
    (item::MELODY_TIME_SONG, Glyph::Song),
    (item::MELODY_STORM_SONG, Glyph::Song),
    (item::MELODY_FOREST_MINUET, Glyph::Song),
    (item::MELODY_FIRE_BOLERO, Glyph::Song),
    (item::MELODY_WATER_SERENADE, Glyph::Song),
    (item::MELODY_SPIRIT_REQUIEM, Glyph::Song),
    (item::MELODY_SHADOW_NOCTURNE, Glyph::Song),
    (item::MELODY_LIGHT_PRELUDE, Glyph::Song),
    (item::STONE_FOREST, Glyph::Stone),
    (item::STONE_FIRE, Glyph::Stone),
    (item::STONE_WATER, Glyph::Stone),
    (item::MEDAL_FOREST, Glyph::ForestMedallion),
    (item::MEDAL_WATER, Glyph::WaterMedallion),
    (item::MEDAL_SPIRIT, Glyph::SpiritMedallion),
    (item::MEDAL_SHADOW, Glyph::ShadowMedallion),
    (item::DUNGEON_SMALL_KEY, Glyph::SmallKey),
    (item::DUNGEON_BOSS_KEY, Glyph::BossKey),
    (item::DUNGEON_MAP, Glyph::Map),
    (item::DUNGEON_COMPASS, Glyph::Compass),
];

impl Glyph {
    /// The glyph for the icon `name`, or `Unknown` for one the table lacks.
    pub fn of(name: &Name) -> Glyph {
        GLYPHS
            .iter()
            .find_map(|&(n, glyph)| (n == name).then_some(glyph))
            .unwrap_or(Glyph::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{GRID, KEPT};

    #[test]
    fn every_tier_in_the_grid_has_a_glyph() {
        let drawn = GRID.iter().flat_map(|s| match &s.icon {
            Some(icon) => std::slice::from_ref(icon),
            None => s.tiers,
        });
        for name in drawn {
            assert_ne!(Glyph::of(name), Glyph::Unknown, "{name}");
        }
    }

    #[test]
    fn every_dungeon_icon_has_a_glyph() {
        for (_, item, _) in KEPT.iter() {
            assert_ne!(Glyph::of(item), Glyph::Unknown, "{item}");
        }
    }

    #[test]
    fn a_name_is_in_the_table_once() {
        for (i, (name, _)) in GLYPHS.iter().enumerate() {
            let again = GLYPHS[i + 1..].iter().any(|(n, _)| n == name);
            assert!(!again, "{name}");
        }
    }

    #[test]
    fn tiers_share_a_glyph_and_a_stranger_is_unknown() {
        assert_eq!(Glyph::of(item::HOOKSHOT), Glyph::of(item::LONGSHOT));
        assert_ne!(Glyph::of(item::HOOKSHOT), Glyph::of(item::BOMB));
        assert_eq!(Glyph::of(&Name(0)), Glyph::Unknown);
    }

    #[test]
    fn a_glyph_serializes_as_kebab_case() {
        let json = serde_json::to_string(&Glyph::of(item::ARROW_FIRE)).unwrap();
        assert_eq!(json, "\"fire-arrow\"");
    }
}
