//! The files the Magic Meter's look is made from, spelled out: they're
//! extracted before a dump's names can be read. The guard lets this file
//! name them.

/// Doubled Magic's icon, which the copy's is painted from.
pub(super) const ICON: &str = "UI/Tex/Icon/Item_FairyBless_MPDouble.bntx.zs";

/// The files the look is made from, spelled out like
/// [`crate::key_tints::FILES`]: extracted before a dump's names are read.
pub const FILES: [&str; 9] = [
    "Model/Item_FairyBless_MPDouble.Item_FairyBless_MPDouble.bfres.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_Alb.bntx.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_AO.bntx.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_Mtl.bntx.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_Nrm.bntx.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_Rgh.bntx.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_Thc.bntx.zs",
    "Tex/Mt_Item_FairyBless_MPDouble_Trs.bntx.zs",
    ICON,
];
