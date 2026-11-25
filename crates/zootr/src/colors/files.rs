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

// Link's own models, the ones a shared texture's copy goes in.
const LINK: [&str; 6] = [
    "Model/Player_Adult.Player_Adult.bfres.zs",
    "Model/Player_Adult_Dark.Player_Adult_Dark.bfres.zs",
    "Model/Player_Child.Player_Child.bfres.zs",
    "Model/Player_Child_Op.Player_Child_Op.bfres.zs",
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
