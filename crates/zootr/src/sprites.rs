//! The item tracker's icons, decoded from the user's own dump to PNG and
//! kept in the cache. Nothing here is committed or shipped.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use ounce_formats::{Romfs, Texture, textures, textures_named};

use crate::game;
use crate::inventory;
use crate::known::{Name, actor, field, item};

/// Pouch icons, one BNTX each, named by actor.
const ICONS: &str = "UI/Tex/Icon/";
/// Holds what `ICONS` lacks: the songbook and file select art.
pub(crate) const LAYOUT: &str = "UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.bntx.zs";
/// Layout textures and the grid icon each one is, matched by eye.
const RENAMES: [(&Name, &Name); 22] = [
    (field::OCARINA_MELODY_ICON_00, item::MELODY_ZELDA_LULLABY),
    (field::OCARINA_MELODY_ICON_01, item::MELODY_EPONA_SONG),
    (field::OCARINA_MELODY_ICON_02, item::MELODY_SARIA_SONG),
    (field::OCARINA_MELODY_ICON_03, item::MELODY_SUN_SONG),
    (field::OCARINA_MELODY_ICON_04, item::MELODY_TIME_SONG),
    (field::OCARINA_MELODY_ICON_05, item::MELODY_STORM_SONG),
    (field::OCARINA_MELODY_ICON_07, item::MELODY_FOREST_MINUET),
    (field::OCARINA_MELODY_ICON_08, item::MELODY_FIRE_BOLERO),
    (field::OCARINA_MELODY_ICON_09, item::MELODY_WATER_SERENADE),
    (field::OCARINA_MELODY_ICON_10, item::MELODY_SPIRIT_REQUIEM),
    (field::OCARINA_MELODY_ICON_11, item::MELODY_SHADOW_NOCTURNE),
    (field::OCARINA_MELODY_ICON_12, item::MELODY_LIGHT_PRELUDE),
    (field::DATA_FILE_IMPORTANT_ICON_00, item::STONE_FOREST),
    (field::DATA_FILE_IMPORTANT_ICON_01, item::STONE_FIRE),
    (field::DATA_FILE_IMPORTANT_ICON_02, item::STONE_WATER),
    (field::DATA_FILE_IMPORTANT_ICON_03, item::MEDAL_LIGHT),
    (field::DATA_FILE_IMPORTANT_ICON_04, item::MEDAL_FOREST),
    (field::DATA_FILE_IMPORTANT_ICON_05, item::MEDAL_FIRE),
    (field::DATA_FILE_IMPORTANT_ICON_06, item::MEDAL_WATER),
    (field::DATA_FILE_IMPORTANT_ICON_07, item::MEDAL_SPIRIT),
    (field::DATA_FILE_IMPORTANT_ICON_08, item::MEDAL_SHADOW),
    (field::COLLECT_HEART_PIECE_00, actor::OBJ_HEART_PIECE),
];

/// Each song's songbook icon number, `NN` of `OcarinaMelodyIcon_NN`, and
/// its item.
pub(crate) fn songs() -> impl Iterator<Item = (&'static str, &'static str)> {
    RENAMES
        .iter()
        .filter_map(|(tex, item)| Some((tex.get().strip_prefix("OcarinaMelodyIcon_")?, item.get())))
}

static BUILDING: Mutex<()> = Mutex::new(());

/// Kept in the folder: the icon names it was built for, one per line.
const MANIFEST: &str = "icons.txt";

/// The folder of `<icon>.png` for the dump at `game`, built on first use and
/// again whenever the grid wants other icons than it was built for. An icon
/// that doesn't decode is left out, and the grid draws its own.
pub fn folder(romfs: &Romfs, game: &Path) -> Result<PathBuf> {
    let out = game::cached(game, "icons")?;
    let _lock = BUILDING.lock().unwrap_or_else(|e| e.into_inner());
    let manifest = manifest();
    if built_for(&out, &manifest) {
        return Ok(out);
    }
    let part = out.with_extension("part");
    let _ = std::fs::remove_dir_all(&part);
    build(romfs, game, &part)?;
    std::fs::write(part.join(MANIFEST), &manifest)?;
    let _ = std::fs::remove_dir_all(&out);
    std::fs::rename(&part, &out)?;
    Ok(out)
}

/// Every icon name the grid wants, sorted, one per line.
fn manifest() -> String {
    let mut names: Vec<_> = inventory::icons().collect();
    names.sort_unstable();
    names.dedup();
    names.join("\n")
}

/// Whether `dir` holds icons built for `manifest`.
fn built_for(dir: &Path, manifest: &str) -> bool {
    std::fs::read_to_string(dir.join(MANIFEST)).is_ok_and(|m| m == manifest)
}

fn build(romfs: &Romfs, game: &Path, part: &Path) -> Result<()> {
    std::fs::create_dir_all(part)?;
    let wanted: HashSet<&str> = inventory::icons().collect();
    for (path, data) in game::search(game, ICONS)? {
        let name = path[ICONS.len()..].trim_end_matches(".bntx.zs");
        if wanted.contains(name) {
            let data = romfs.dicts().decompress(&path, &data)?;
            write(part, name, decode(&data).first())?;
        }
    }
    for (path, data) in game::search(game, LAYOUT)? {
        let data = romfs.dicts().decompress(&path, &data)?;
        let renamed = |tex: &str| RENAMES.iter().find(|(t, _)| *t == tex).map(|r| r.1);
        for t in textures_named(&data, |tex| renamed(tex).is_some()).unwrap_or_default() {
            write(part, renamed(&t.name).expect("kept"), Some(&t))?;
        }
    }
    Ok(())
}

fn write(dir: &Path, name: &str, t: Option<&Texture>) -> Result<()> {
    if let Some(t) = t {
        std::fs::write(dir.join(format!("{name}.png")), png(t)?)?;
    }
    Ok(())
}

/// Every texture in a BNTX, or none if it doesn't decode.
fn decode(data: &[u8]) -> Vec<Texture> {
    textures(data).unwrap_or_default()
}

pub fn png(t: &Texture) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, t.width as u32, t.height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&t.rgba)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;

    #[test]
    fn a_folder_counts_as_built_only_for_the_same_icons() {
        let dir = std::env::temp_dir().join(format!("zootr-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Built before the manifest, or for an older grid: rebuild.
        assert!(!built_for(&dir, &manifest()));
        std::fs::write(dir.join(MANIFEST), item::HOOKSHOT.get()).unwrap();
        assert!(!built_for(&dir, &manifest()));
        std::fs::write(dir.join(MANIFEST), manifest()).unwrap();
        assert!(built_for(&dir, &manifest()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_texture_decodes_to_nothing() {
        assert!(decode(b"BNTX but not really").is_empty());
    }

    #[test]
    fn the_hookshot_decodes_at_its_size() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new("../../dump/oot.nup");
        let (path, data) = game::search(game, &format!("UI/Tex/Icon/{}.", item::HOOKSHOT))
            .unwrap()
            .remove(0);
        let t = &decode(&romfs.dicts().decompress(&path, &data).unwrap())[0];
        assert_eq!(
            (t.name.as_str(), t.width, t.height),
            (item::HOOKSHOT.get(), 341, 341)
        );
        assert_eq!(t.rgba.len(), 341 * 341 * 4);
    }

    #[test]
    fn every_grid_icon_is_built() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let out = std::env::temp_dir().join(format!("zootr-icons-{}", std::process::id()));
        build(&romfs, Path::new("../../dump/oot.nup"), &out).unwrap();
        let missing: Vec<_> = inventory::icons()
            .filter(|i| !out.join(format!("{i}.png")).exists())
            .collect();
        std::fs::remove_dir_all(&out).unwrap();
        assert!(missing.is_empty(), "no icon for {missing:?}");
    }
}
