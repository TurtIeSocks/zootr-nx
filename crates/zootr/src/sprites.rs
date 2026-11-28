//! The item tracker's icons, decoded from the user's own dump to PNG and
//! kept in the cache. Nothing here is committed or shipped.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use ounce_formats::{Romfs, Texture, textures, textures_named};

use crate::game;
use crate::inventory;

mod ring;
use crate::known::{Name, actor, field, item};

/// Pouch icons, one BNTX each, named by actor.
pub(crate) const ICONS: &str = "UI/Tex/Icon/";
/// Holds what `ICONS` lacks: the songbook and file select art.
pub(crate) const LAYOUT: &str = "UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.bntx.zs";
/// Layout textures and the grid icon each one is, matched by eye.
const RENAMES: [(&Name, &Name); 23] = [
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
    (field::HEART_W_00_ASTC4X4, actor::OBJ_HEART_CONTAINER),
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

/// How much of its square an icon's drawing fills. The dump's icons pad
/// theirs unevenly (the Heart Container fills 48%, most icons about 80%),
/// so each is cropped to what it draws and padded back to this.
const FILL: f32 = 0.82;

/// The alpha past which a pixel counts as drawn when framing.
const EDGE: u8 = 32;

/// Heads the manifest, so a folder framed another way builds again.
const FRAMING: &str = "framed 0.82 past 32";

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
    std::iter::once(FRAMING)
        .chain(names)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `dir` holds icons built for `manifest`.
fn built_for(dir: &Path, manifest: &str) -> bool {
    std::fs::read_to_string(dir.join(MANIFEST)).is_ok_and(|m| m == manifest)
}

fn build(romfs: &Romfs, game: &Path, part: &Path) -> Result<()> {
    std::fs::create_dir_all(part)?;
    let wanted: HashSet<&str> = inventory::icons().collect();
    let mut small_key = None;
    for (path, data) in game::search(game, ICONS)? {
        // The search matches anywhere in a path, so not every hit is an icon.
        let Some(name) = path.strip_prefix(ICONS) else {
            continue;
        };
        let name = name.trim_end_matches(".bntx.zs");
        if wanted.contains(name) {
            let data = romfs.dicts().decompress(&path, &data)?;
            let t = decode(&data).into_iter().next();
            if name == crate::keys::SMALL.get() {
                small_key = t.clone();
            }
            write(part, name, t.as_ref())?;
        }
    }
    // The key ring is a model alone: its icon is painted from the key's.
    if wanted.contains(inventory::KEY_RING) {
        let icon = small_key.as_ref().and_then(ring::paint);
        write(part, inventory::KEY_RING, icon.as_ref())?;
    }
    // The Magic Meter is ours: its icon is painted, as the seed paints it.
    if wanted.contains(crate::magic::meter::NAME) {
        let icon = crate::magic::meter::icon(romfs)?;
        write(part, crate::magic::meter::NAME, Some(&icon))?;
    }
    let renamed = |tex: &str| RENAMES.iter().find(|(t, _)| *t == tex).map(|r| r.1);
    for (path, data) in game::search(game, LAYOUT)? {
        let data = romfs.dicts().decompress(&path, &data)?;
        for t in textures_named(&data, |tex| renamed(tex).is_some()).unwrap_or_default() {
            if let Some(name) = renamed(&t.name) {
                write(part, name, Some(&t))?;
            }
        }
    }
    Ok(())
}

fn write(dir: &Path, name: &str, t: Option<&Texture>) -> Result<()> {
    if let Some(t) = t {
        std::fs::write(dir.join(format!("{name}.png")), png(&framed(t))?)?;
    }
    Ok(())
}

/// `t` centred in a square its drawing fills [`FILL`] of, across its longer
/// side. The drawing ends where alpha passes [`EDGE`], so a glow fading out
/// (the Great Fairies') doesn't count as drawn, but stays in the square. A
/// blank `t` stays as it is.
fn framed(t: &Texture) -> Texture {
    let Some((x0, y0, x1, y1)) = drawn_box(t) else {
        return t.clone();
    };
    let (w, h) = (x1 - x0, y1 - y0);
    let side = (w.max(h) as f32 / FILL).ceil() as usize;
    // Where the square's corner falls on `t`, maybe off its edge.
    let (left, top) = (
        x0 as isize - ((side - w) / 2) as isize,
        y0 as isize - ((side - h) / 2) as isize,
    );
    let on = |at: isize, len: usize| (at.max(0) as usize).min(len);
    let (sx0, sx1) = (on(left, t.width), on(left + side as isize, t.width));
    let mut rgba = vec![0; side * side * 4];
    for dy in 0..side {
        let sy = top + dy as isize;
        if sy < 0 || sy as usize >= t.height {
            continue;
        }
        let from = (sy as usize * t.width + sx0) * 4;
        let to = (dy * side + (sx0 as isize - left) as usize) * 4;
        let n = (sx1 - sx0) * 4;
        rgba[to..to + n].copy_from_slice(&t.rgba[from..from + n]);
    }
    Texture {
        name: t.name.clone(),
        width: side,
        height: side,
        rgba,
    }
}

/// The box `t` draws in, its pixels past [`EDGE`], as its left, top, and
/// one past its right and bottom. None for a blank `t`.
fn drawn_box(t: &Texture) -> Option<(usize, usize, usize, usize)> {
    let drawn = |x: usize, y: usize| t.rgba[(y * t.width + x) * 4 + 3] > EDGE;
    let (mut x0, mut y0, mut x1, mut y1) = (t.width, t.height, 0, 0);
    for y in 0..t.height {
        for x in (0..t.width).filter(|&x| drawn(x, y)) {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
    }
    (x0 < x1).then_some((x0, y0, x1, y1))
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
    fn an_icon_is_cropped_and_padded_to_fill_its_square() {
        // A 2x1 drawing in the corner of a 10x10 canvas.
        let mut rgba = vec![0; 10 * 10 * 4];
        rgba[3] = 255;
        rgba[7] = 255;
        // A faint glow under the drawing: kept, but it doesn't widen it.
        rgba[(10 + 1) * 4 + 3] = EDGE;
        let t = Texture {
            name: "FakeIcon".into(),
            width: 10,
            height: 10,
            rgba,
        };
        let f = framed(&t);
        // 2 / 0.82 rounds up to a 3x3 square, the drawing on its middle row
        // and the glow below it. The square's top row is off the canvas.
        assert_eq!((f.width, f.height), (3, 3));
        let alpha: Vec<u8> = f.rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alpha, [0, 0, 0, 255, 255, 0, 0, EDGE, 0]);
        let blank = Texture {
            rgba: vec![0; 16],
            width: 2,
            height: 2,
            ..t
        };
        assert_eq!(framed(&blank).width, 2);
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
        let game = Path::new(game::DEV_DUMP);
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
        build(&romfs, Path::new(game::DEV_DUMP), &out).unwrap();
        let missing: Vec<_> = inventory::icons()
            .filter(|i| !out.join(format!("{i}.png")).exists())
            .collect();
        std::fs::remove_dir_all(&out).unwrap();
        assert!(missing.is_empty(), "no icon for {missing:?}");
    }
}
