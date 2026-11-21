//! How a Bombchu Bag looks: its bomb bag's model painted the Bombchu's
//! blue. The guard lets this file spell the files out, since they're
//! extracted before a dump's names can be read.

use anyhow::Result;
use ounce_formats::{ModBuilder, Romfs};

use super::BOMB_BAGS;
use crate::key_tints::paint::{Host, Look};
use crate::known::Name;
use crate::medallions::Colour;

/// The files [`LOOKS`] are made from, spelled out like
/// [`crate::key_tints::FILES`]: extracted before a dump's names are read.
pub const FILES: [&str; 18] = [
    "Model/Item_BombBag_Small.Item_BombBag_Small.bfres.zs",
    "Tex/Mt_Item_BombBag_Small_Alb.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_AO.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_Mtl.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_Nrm.bntx.zs",
    "Tex/Mt_Item_BombBag_Small_Rgh.bntx.zs",
    "Model/Item_BombBag_Medium.Item_BombBag_Medium.bfres.zs",
    "Tex/Mt_Item_BombBag_Medium_Alb.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_AO.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_Mtl.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_Nrm.bntx.zs",
    "Tex/Mt_Item_BombBag_Medium_Rgh.bntx.zs",
    "Model/Item_BombBag_Large.Item_BombBag_Large.bfres.zs",
    "Tex/Mt_Item_BombBag_Large_Alb.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_AO.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_Mtl.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_Nrm.bntx.zs",
    "Tex/Mt_Item_BombBag_Large_Rgh.bntx.zs",
];

/// Each bag's look: its bomb bag's model and textures, the albedo in
/// [`BLUE`]. A copy's name is as long as its host's (`paint::Look`), so the
/// Medium's prefix is a letter longer.
static LOOKS: [Look; 3] = [
    look(BOMB_BAGS[0], "Item_RandoChuBag"),
    look(BOMB_BAGS[1], "Item_Rando_ChuBag"),
    look(BOMB_BAGS[2], "Item_RandoChuBag"),
];

/// The Bombchu's own blue, so a bag reads as theirs and never as a bomb
/// bag.
const BLUE: Colour = [70, 70, 220];

const fn look(host: &'static Name, copy: &'static str) -> Look {
    Look {
        host,
        material: None,
        kinds: &["Alb", "AO", "Mtl", "Nrm", "Rgh"],
        others: &[],
        copy,
    }
}

/// Bag `at`'s model, counting from 0.
pub(super) fn model(at: usize) -> String {
    LOOKS[at].name(at + 1)
}

/// Writes bag `at`'s painted model. Returns its name.
pub(super) fn paint(romfs: &Romfs, builder: &mut ModBuilder, at: usize) -> Result<String> {
    Host::read(romfs, &LOOKS[at])?.paint(builder, at + 1, BLUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extracted_files_are_the_looks() {
        needs_names!();
        let files: Vec<_> = LOOKS.iter().flat_map(Look::files).collect();
        assert_eq!(files, FILES);
    }

    /// A painted copy keeps every offset of its host's model.
    #[test]
    fn each_model_is_named_as_long_as_its_bomb_bags() {
        needs_names!();
        for (at, look) in LOOKS.iter().enumerate() {
            assert_eq!(model(at).len(), look.host.get().len(), "{}", model(at));
        }
    }
}
