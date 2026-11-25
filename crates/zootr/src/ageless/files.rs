//! The files gear for both ages reads, as text.

// Text: the extract runs before a dump is open, so no name can be read.

/// The child's model: the materials the boots' and tunics' groups name,
/// and the cloth his tunics copy.
pub const MODEL: &str = "Model/Player_Child.Player_Child.bfres.zs";
/// The textures of his cloth, which his tunics copy in their color: the
/// leather's holds his headband's green.
pub const CLOTH: [&str; 4] = [
    "Tex/Angin_Tile_A_Alb.bntx.zs",
    "Tex/Angin_Tile_A_Dark_Alb.bntx.zs",
    "Tex/Mt_Cloth_Parts_Player_Child_Alb.bntx.zs",
    "Tex/Mt_Leather_Player_Child_Alb.bntx.zs",
];
/// The texture of his vest, which his tunics copy in their vest's color.
pub const VEST: &str = "Tex/Mt_Vest_Player_Child_Alb.bntx.zs";

/// The files read beside the packs, which the dump's cache extracts
/// besides its folders.
pub const FILES: [&str; 6] = [MODEL, CLOTH[0], CLOTH[1], CLOTH[2], CLOTH[3], VEST];
