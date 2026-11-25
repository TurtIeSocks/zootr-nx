//! The files Colors paints, as text.

// Text: the extract runs before a dump is open, so no name can be read.

use crate::ageless::files::{CLOTH, VEST};

/// An albedo a palette paints, and its metal map when the paint needs the
/// shine taken off to show.
pub struct Tex {
    pub path: &'static str,
    pub metal: Option<&'static str>,
    /// Set when another character or item uses the texture too: the paint
    /// goes on a copy, named with this letter, in these models only.
    pub copy: Option<Copy>,
}

pub struct Copy {
    pub mark: u8,
    pub models: &'static [&'static str],
    /// Other names the game's texture table (UnifyTex) points at this
    /// texture, each with the models that name it: each gets a copy of
    /// its own, marked the same way.
    pub aliases: &'static [(&'static str, &'static [&'static str])],
}

const fn tex(path: &'static str) -> Tex {
    Tex {
        path,
        metal: None,
        copy: None,
    }
}

const fn metal(path: &'static str, metal: &'static str) -> Tex {
    Tex {
        path,
        metal: Some(metal),
        copy: None,
    }
}

// Link's bodies: grown, young, and young in the opening.
pub const ADULT: &str = "Model/Player_Adult.Player_Adult.bfres.zs";
pub const CHILD: &str = "Model/Player_Child.Player_Child.bfres.zs";
pub const CHILD_OP: &str = "Model/Player_Child_Op.Player_Child_Op.bfres.zs";

// Link's own models, the ones a shared texture's copy goes in.
const LINK: [&str; 6] = [
    ADULT,
    "Model/Player_Adult_Dark.Player_Adult_Dark.bfres.zs",
    CHILD,
    CHILD_OP,
    "Model/Obj_Armor_Adult_Normal.Obj_Armor_Adult_Normal.bfres.zs",
    "Model/Obj_Armor_Child_Normal.Obj_Armor_Child_Normal.bfres.zs",
];

// Kokiri Tunic. His cap's and sleeves' cloth is Mido's too, so it's
// painted as a copy.
pub const CLOTH_TILE: Tex = Tex {
    path: CLOTH[0],
    metal: None,
    copy: Some(Copy {
        mark: b'K',
        models: &LINK,
        aliases: &[],
    }),
};
pub const ADULT_VEST: Tex = tex("Tex/Mt_Vest_Player_Adult_Alb.bntx.zs");
pub const ADULT_CAP: Tex = tex("Tex/Mt_Angin_Acs_Player_Adult_Alb.bntx.zs");
pub const CHILD_VEST: Tex = tex(VEST);
pub const CHILD_PANTS: Tex = tex(CLOTH[1]);
pub const CHILD_PARTS: Tex = tex(CLOTH[2]);
pub const CHILD_LEATHER: Tex = tex(CLOTH[3]);

// Goron Tunic.
pub const GORON_VEST: Tex = tex("Tex/Mt_Vest_Armor_Adult_Goron_Alb.bntx.zs");
pub const GORON_CLOTH: Tex = tex("Tex/Mt_Angin_Armor_Adult_Goron_Alb.bntx.zs");
pub const GORON_INNER: Tex = tex("Tex/Mt_Inner_Armor_Adult_Goron_Alb.bntx.zs");
pub const GORON_LEATHER: Tex = tex("Tex/Mt_Leater_Upper_Armor_Adult_Goron_Alb.bntx.zs");

// Zora Tunic.
pub const ZORA_VEST: Tex = tex("Tex/Mt_Vest_Armor_Adult_Zora_Alb.bntx.zs");
pub const ZORA_CLOTH: Tex = tex("Tex/Mt_Angin_Armor_Adult_Zora_Alb.bntx.zs");
pub const ZORA_CAP: Tex = tex("Tex/Mt_Angin_Acs_Armor_Adult_Zora_Alb.bntx.zs");
pub const ZORA_PARTS: Tex = tex("Tex/Mt_Parts_Armor_Adult_Zora_Alb.bntx.zs");
pub const ZORA_FEATHER: Tex = tex("Tex/Mt_Feather_Armor_Adult_Zora_Alb.bntx.zs");
pub const ZORA_LEATHER: Tex = tex("Tex/Mt_Leater_Upper_Armor_Adult_Zora_Alb.bntx.zs");

// Leather.
pub const ADULT_LEATHER: Tex = tex("Tex/Mt_Leater_Upper_Player_Adult_Alb.bntx.zs");
pub const ADULT_GLOVE: Tex = tex("Tex/Mt_Glove_Player_Adult_Alb.bntx.zs");
pub const ADULT_BOOTS: Tex = tex("Tex/Mt_Boots_Player_Adult_Alb.bntx.zs");

// Gear.
pub const KOKIRI_SWORD: Tex = tex("Tex/Mt_Weapon_Sword_Kokiri_Alb.bntx.zs");
pub const BIGGORON_SWORD: Tex = tex("Tex/Mt_Weapon_Sword_Biggoron_Alb.00.bntx.zs");
pub const DEKU_SHIELD: Tex = tex("Tex/Mt_Body_Weapon_Shield_Deku_Alb.bntx.zs");
pub const HYLIAN_SHIELD: Tex = tex("Tex/Mt_Weapon_Shield_Hylian_Alb.bntx.zs");
pub const MIRROR_SHIELD: Tex = tex("Tex/Mt_Weapon_Shield_Mirror_Alb.bntx.zs");
pub const IRON_BOOTS: Tex = tex("Tex/Mt_Boots_Armor_Adult_HeavyBoot_Alb.bntx.zs");
pub const HAMMER: Tex = metal(
    "Tex/Mt_Item_Hammer_Alb.bntx.zs",
    "Tex/Mt_Item_Hammer_Mtl.bntx.zs",
);
pub const BOW: Tex = tex("Tex/Mt_Weapon_Range_Bow_Alb.bntx.zs");
pub const SLINGSHOT: Tex = tex("Tex/Mt_Weapon_Range_SlingShot_Alb.bntx.zs");
pub const HOOKSHOT: Tex = tex("Tex/Mt_Body_Item_Hookshot_Alb.bntx.zs");
pub const FAIRY_OCARINA: [Tex; 2] = [
    tex("Tex/Mt_Item_Ocarina_Fairy_Alb.00.bntx.zs"),
    tex("Tex/Mt_Item_Ocarina_Fairy_Alb.01.bntx.zs"),
];
pub const OCARINA_OF_TIME: Tex = tex("Tex/Mt_Item_Ocarina_Time_Alb.bntx.zs");

// Hair: its colour is its albedo, and its gradient (a LUT) colours the
// highlight (seen live). The albedo is Dark Link's too and the child's
// through the texture table, so Link's is a copy, and the child's another.
pub const HAIR: Tex = Tex {
    path: "Tex/Mt_Hairs_Player_Adult_Alb.bntx.zs",
    metal: None,
    copy: Some(Copy {
        mark: b'L',
        models: &[ADULT],
        aliases: &[("Player_Child_Hairs_Alb", &[CHILD, CHILD_OP])],
    }),
};
// The grown one's gradient is every character's, so it's a copy too.
pub const ADULT_HAIR: Tex = Tex {
    path: "Tex/CmnTex_Chara_Hair_LUT_A_Gn0.bntx.zs",
    metal: None,
    copy: Some(Copy {
        mark: b'L',
        models: &[ADULT],
        aliases: &[],
    }),
};
pub const CHILD_HAIR: Tex = tex("Tex/Player_Child_Hairs_Gn0.bntx.zs");

// Gear whose look is partly material params: their models.
pub const MASTER: &str = "Model/Weapon_Sword_Master.Weapon_Sword_Master.bfres.zs";
pub const MASTER_PEDESTAL: &str = "Model/Weapon_Sword_Master.Land_HrlCity_MasterSword.bfres.zs";
pub const SILVER: &str = "Model/Armor_Adult_Gauntlet_Silver.Armor_Adult_Gauntlet_Silver.bfres.zs";
pub const GOLD: &str = "Model/Armor_Adult_Gauntlet_Gold.Armor_Adult_Gauntlet_Gold.bfres.zs";
pub const BRACELET: &str = "Model/Item_Gauntlet_Goron.Item_Gauntlet_Goron.bfres.zs";
pub const HOVER: &str = "Model/Armor_Adult_HoverBoot.Armor_Adult_HoverBoot.bfres.zs";
pub const BOOMERANG_MODEL: &str = "Model/Item_Boomerang.Item_Boomerang.bfres.zs";

pub const MASTER_SWORD: Tex = tex("Tex/Mt_Weapon_Sword_Master_Alb.bntx.zs");
// The Golden Gauntlets wear this silver too, so the Silver Gauntlets'
// paint goes on a copy, its metal map with it: taking the shine off the
// shared one left the Golden Gauntlets' plates flat white (seen live).
pub const SILVER_GAUNTLETS: Tex = Tex {
    path: "Tex/Mt_Silver_Armor_Adult_Gauntlet_Silver_Alb.bntx.zs",
    metal: Some("Tex/Mt_Silver_Armor_Adult_Gauntlet_Silver_Mtl.bntx.zs"),
    copy: Some(Copy {
        mark: b'S',
        models: &[SILVER],
        aliases: &[],
    }),
};
pub const GOLD_GAUNTLETS: Tex = metal(
    "Tex/Mt_Gold_Armor_Adult_Gauntlet_Gold_Alb.bntx.zs",
    "Tex/Mt_Gold_Armor_Adult_Gauntlet_Gold_Mtl.bntx.zs",
);
pub const GORON_BRACELET: Tex = metal(
    "Tex/Mt_Item_Gauntlet_Goron_Alb.bntx.zs",
    "Tex/Mt_Item_Gauntlet_Goron_Mtl.bntx.zs",
);
pub const HOVER_BOOTS: Tex = tex("Tex/Mt_Boots_Armor_Adult_HoverBoot_Alb.bntx.zs");
pub const BOOMERANG: Tex = tex("Tex/Mt_Item_Boomerang_Alb.bntx.zs");

// Materials whose params a palette sets, by name.
pub const EYEBROW: &str = "Mt_Eyebrow";
pub const EYELASHES: &str = "Mt_Eyelashes";
pub const EYELASHES_1: &str = "Mt_Eyelashes_01";
pub const EYELASHES_2: &str = "Mt_Eyelashes_02";
pub const EYELASHES_3: &str = "Mt_Eyelashes_03";
pub const SWORD_SHEEN: &str = "Mt_Weapon_Sword_Master";
pub const JEWELRY: &str = "Mt_Jewelry";
pub const BRACELET_GLASS: &str = "Mt_Item_Gauntlet_Goron_Glass";
pub const BOOTS_GLASS: &str = "Mt_Boots_TransParent";
pub const BOOMERANG_GEM: &str = "Mt_Item_Boomerang_Gem";
pub const BOOMERANG_TILE_1: &str = "Mt_Item_Boomerang_Tile_01";
pub const BOOMERANG_TILE_2: &str = "Mt_Item_Boomerang_Tile_02";
pub const BOOMERANG_TILE_3: &str = "Mt_Item_Boomerang_Tile_03";
