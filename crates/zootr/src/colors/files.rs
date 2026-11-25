//! The files Colors paints, as text.

// Text: the extract runs before a dump is open, so no name can be read.

use crate::ageless::files::{CLOTH, VEST};

/// An albedo a palette paints, and its metal map when the paint needs the
/// shine taken off to show.
pub struct Tex {
    pub path: &'static str,
    /// Its name in a file of many (the UI's atlas), or none for a file
    /// of one.
    pub name: Option<&'static str>,
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

/// Which texture a `Tex` is: its file, and its name in a file of many.
pub type TexKey = (&'static str, Option<&'static str>);

impl Tex {
    pub fn key(&self) -> TexKey {
        (self.path, self.name)
    }

    /// The name a model's material gives it, for a file of one:
    /// `Tex/<name>.bntx.zs`.
    pub fn stem(&self) -> Option<&'static str> {
        match self.name {
            None => stem(self.path),
            Some(_) => None,
        }
    }

    /// Its first level, from its file's `data`.
    pub fn texels(&self, data: &[u8]) -> anyhow::Result<ounce_formats::Texture> {
        match self.name {
            None => ounce_formats::top_level(data),
            Some(name) => ounce_formats::texture_named(data, name),
        }
    }

    /// Its file's `data` with its texels redrawn from `rgba`.
    pub fn redrawn(&self, data: &[u8], rgba: &[u8]) -> anyhow::Result<Vec<u8>> {
        match self.name {
            None => ounce_formats::redraw(data, rgba, |_, _| false),
            Some(name) => ounce_formats::redraw_named(data, name, rgba, |_, _| false),
        }
    }
}

/// The texture name a `Tex/<name>.bntx.zs` file holds. A name may end
/// in a variant's number (`_Alb.00`), as a material names it.
pub fn stem(path: &str) -> Option<&str> {
    path.strip_prefix("Tex/")?.strip_suffix(".bntx.zs")
}

/// A texture in the UI's atlas, by name.
const fn atlas(name: &'static str) -> Tex {
    Tex {
        path: UI_ATLAS,
        name: Some(name),
        metal: None,
        copy: None,
    }
}

const fn tex(path: &'static str) -> Tex {
    Tex {
        path,
        name: None,
        metal: None,
        copy: None,
    }
}

const fn metal(path: &'static str, metal: &'static str) -> Tex {
    Tex {
        path,
        name: None,
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
    name: None,
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
    name: None,
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
    name: None,
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
    name: None,
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

// Layers the game blends over Link's eyes through its shader (a glint, a
// shadow). Drawn plain, they cover the eyes (the child's shadow, with an
// albedo, in black): the Model tab leaves them out.
pub const EYE_LAYERS: [&str; 3] = ["Mt_Eye_Outer", "Mt_Eyeshadow", "Mt_EyeShadow_test"];
pub const BOOMERANG_TILE_1: &str = "Mt_Item_Boomerang_Tile_01";
pub const BOOMERANG_TILE_2: &str = "Mt_Item_Boomerang_Tile_02";
pub const BOOMERANG_TILE_3: &str = "Mt_Item_Boomerang_Tile_03";

/// An effect bundle, by its owner's name.
macro_rules! effect {
    ($owner:literal) => {
        concat!("Effect/", $owner, ".Nin_Ounce_NVN2.vsetb.byml.zs")
    };
}

/// The effects the game loads at boot. Its index (`EffectFileInfo`)
/// lists every bundle it loads by name besides; a bundle it doesn't list
/// (the Purple and Gold Rupees' own) is a stale copy of esets in here.
pub const STATIC_FX: &str = effect!("static");
/// The index of the bundles the game loads.
pub const EFFECT_INDEX: &str = "Effect/EffectFileInfo.Product.110.Nin_Ounce_NVN2.byml.zs";

// Navi: her model, its two body materials, and her cutscene double's
// effects.
pub const NAVI: &str = "Model/Npc_Navi.Npc_Navi.bfres.zs";
pub const NAVI_BODY: &str = "Mt_Body";
pub const NAVI_BODY_MASK: &str = "Mt_Body_AlphaMask";
pub const NAVI_FX: &str = effect!("Dm_Npc_Navi");

/// A UI layout archive, by its name.
macro_rules! layouts {
    ($name:literal) => {
        concat!(
            "UI/Layout2Archive/",
            $name,
            ".Product.100.Nin_Ounce_NVN2.blarc.zs"
        )
    };
}

/// The HUD's and menus' layouts.
pub const COMMON_UI: &str = layouts!("Common");
/// The title screen's layouts.
pub const TITLE_UI: &str = layouts!("Title");
/// The notes' layouts.
pub const NOTES_UI: &str = layouts!("ZeldaNotes");
/// Every UI layout archive.
pub const LAYOUT_ARCHIVES: [&str; 4] = [COMMON_UI, TITLE_UI, NOTES_UI, layouts!("Boot")];

/// A heart on the HUD, full or part full or empty: the low health pulse
/// is a palette entry in each one's layout.
pub const HEARTS: &str = "PaHeartGaugeChild";
/// The magic meter's layout.
pub const MAGIC: &str = "PaMpGauge_00";

/// The UI's texture atlas: every HUD and menu texture.
pub const UI_ATLAS: &str = crate::sprites::LAYOUT;
pub const HEART_FILL: Tex = atlas("HeartW_00_ASTC4x4");
pub const MAGIC_FILL: Tex = atlas("MpGaugeCenter_00");
/// The glow around the magic meter while a spell or the Lens drains it.
pub const MAGIC_GLOW: [Tex; 4] = [
    atlas("MpGaugeGlow_00"),
    atlas("MpGaugeGlow_01"),
    atlas("MpGaugeFlash_00"),
    atlas("MpGaugeFlash_01"),
];
pub const RUPEE_ICON: Tex = atlas("Nt_RupeeIconNew_00");
pub const MENU_HEADER: Tex = atlas("MenuHeader_00");

/// One way the Model tab shows a palette: models drawn together.
pub struct View {
    pub label: &'static str,
    pub models: &'static [&'static str],
    /// The pack of an armor worn over the first model (Link), whose
    /// ArmorParam names the material groups of his that it hides.
    pub worn: Option<&'static str>,
}

const fn view(label: &'static str, models: &'static [&'static str]) -> View {
    View {
        label,
        models,
        worn: None,
    }
}

/// Link (`models`, his first) in an armor from pack `pack`, as the game
/// dresses him.
const fn worn(label: &'static str, models: &'static [&'static str], pack: &'static str) -> View {
    View {
        label,
        models,
        worn: Some(pack),
    }
}

/// Armor `model` from pack `pack` worn by adult Link, then alone.
macro_rules! on_link {
    ($model:expr, $pack:expr) => {
        &[
            worn("On Link", &[ADULT, $model], $pack),
            view("Alone", &[$model]),
        ]
    };
}

const HEAVY: &str = "Model/Armor_Adult_HeavyBoot.Armor_Adult_HeavyBoot.bfres.zs";

/// The pack holding Link's own params: each material group armor hides.
pub const PLAYER_BASE: &str = "Pack/Actor/Player_Base.pack.zs";

const ADULT_VIEW: View = view("Adult", &[ADULT]);
const CHILD_VIEW: View = view("Child", &[CHILD]);

const LINK_VIEWS: &[View] = &[ADULT_VIEW, CHILD_VIEW];

/// Each palette with a model, and the views it shows on.
const VIEWS: &[(&str, &[View])] = &[
    ("kokiri-tunic", LINK_VIEWS),
    ("leather", LINK_VIEWS),
    ("hair", LINK_VIEWS),
    ("brows", LINK_VIEWS),
    (
        "goron-tunic",
        &[worn(
            "Adult",
            &[ADULT, "Model/Armor_Adult_Goron.Armor_Adult_Goron.bfres.zs"],
            "Pack/Actor/Armor_Adult_Goron.pack.zs",
        )],
    ),
    (
        "zora-tunic",
        &[worn(
            "Adult",
            &[ADULT, "Model/Armor_Adult_Zora.Armor_Adult_Zora.bfres.zs"],
            "Pack/Actor/Armor_Adult_Zora.pack.zs",
        )],
    ),
    (
        "kokiri-sword",
        &[view(
            "Sword",
            &["Model/Weapon_Sword_Kokiri.Weapon_Sword_Kokiri.bfres.zs"],
        )],
    ),
    (
        "biggoron-sword",
        &[view(
            "Sword",
            &["Model/Weapon_Sword_Biggoron.Weapon_Sword_Biggoron.bfres.zs"],
        )],
    ),
    ("master-sword", &[view("Sword", &[MASTER])]),
    (
        "deku-shield",
        &[view(
            "Shield",
            &["Model/Weapon_Shield_Deku.Weapon_Shield_Deku.bfres.zs"],
        )],
    ),
    (
        "hylian-shield",
        &[view(
            "Shield",
            &["Model/Weapon_Shield_Hylian.Weapon_Shield_Hylian.bfres.zs"],
        )],
    ),
    (
        "mirror-shield",
        &[view(
            "Shield",
            &["Model/Weapon_Shield_Mirror.Weapon_Shield_Mirror.bfres.zs"],
        )],
    ),
    (
        "iron-boots",
        on_link!(HEAVY, "Pack/Actor/Armor_Adult_HeavyBoot.pack.zs"),
    ),
    (
        "hover-boots",
        on_link!(HOVER, "Pack/Actor/Armor_Adult_HoverBoot.pack.zs"),
    ),
    (
        "hammer",
        &[view("Hammer", &["Model/Item_Hammer.Item_Hammer.bfres.zs"])],
    ),
    (
        "bow",
        &[view(
            "Bow",
            &["Model/Weapon_Range_Bow.Weapon_Range_Bow.bfres.zs"],
        )],
    ),
    (
        "slingshot",
        &[view(
            "Slingshot",
            &["Model/Weapon_Range_SlingShot.Weapon_Range_SlingShot.bfres.zs"],
        )],
    ),
    (
        "hookshot",
        &[view(
            "Hookshot",
            &["Model/Item_Hookshot.Item_Hookshot_Body.bfres.zs"],
        )],
    ),
    (
        "fairy-ocarina",
        &[view(
            "Ocarina",
            &["Model/Item_Ocarina_Fairy.Item_Ocarina_Fairy.bfres.zs"],
        )],
    ),
    (
        "ocarina-of-time",
        &[view(
            "Ocarina",
            &["Model/Item_Ocarina_Time.Item_Ocarina_Time.bfres.zs"],
        )],
    ),
    // Alone: the gauntlets bind in a T pose and Link in an A pose, so worn
    // they float until they are skinned to his bones by name.
    ("silver-gauntlets", &[view("Gauntlets", &[SILVER])]),
    ("gold-gauntlets", &[view("Gauntlets", &[GOLD])]),
    ("goron-bracelet", &[view("Bracelet", &[BRACELET])]),
    ("boomerang", &[view("Boomerang", &[BOOMERANG_MODEL])]),
];

/// The views palette `id` shows on, none for one with no model.
pub fn views(id: &str) -> &'static [View] {
    VIEWS.iter().find(|(p, _)| *p == id).map_or(&[], |(_, v)| v)
}
