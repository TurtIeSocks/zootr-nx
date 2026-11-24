//! The Magic Meter: an item of our own that turns the meter on and does
//! nothing else, for seeds where the magic items don't. It's Doubled
//! Magic's pack and rows with the single meter, painted meter green.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, ModBuilder, Romfs, murmur3, redraw, top_level};

use crate::gamedata;
use crate::key_tints::paint::{Host, Look};
use crate::key_tints::{self, copied};
use crate::known::{Name, field, hash, item};
use crate::medallions::paint::{self, Band, Hues};
use crate::standins::{PLAIN_DEAL, Tables, clone_pack, write_clone};

pub use super::look::FILES;
use super::look::ICON;
use super::{SIZE, text};

pub const NAME: &str = "Item_Rando_MagicMeter";
pub const METER: &Name = &Name(hash(NAME));

/// The item it's copied from, whose model and icon it wears.
const LIKE: &Name = item::FAIRY_BLESS_MP_DOUBLE;
/// The single meter's size, in the row's units (tenths of the saved max).
const SINGLE: i32 = 24;

/// The copy's model: Doubled Magic's, named as long.
static LOOK: Look = Look {
    host: LIKE,
    material: None,
    kinds: &["Alb", "AO", "Mtl", "Nrm", "Rgh", "Thc", "Trs"],
    others: &[],
    copy: "Item_Rando_MagicMeter_",
    matte: true,
};

/// The gold and bark of the sigil and wreath, as hues. The leaves are
/// green already and keep theirs.
const GOLD: Hues = (10.0 / 360.0, 65.0 / 360.0);
/// The magic meter's green, as hue, saturation and value. Saturated: the
/// sigil is metal, so its colour reaches the eye as reflected light, and
/// the gold's own pale saturation read as gold under a chest's beam (live).
const GREEN: (f32, f32, f32) = (125.0 / 360.0, 0.85, 0.7);

/// `rgba`'s gold moved to the meter's green, its grain kept.
pub fn paint(rgba: &mut [u8]) {
    paint::shift(
        rgba,
        &[Band {
            hues: GOLD,
            to: GREEN,
        }],
    );
}

/// The copy's icon, decoded: Doubled Magic's painted green.
pub fn icon(romfs: &Romfs) -> Result<ounce_formats::Texture> {
    painted(&romfs.read(ICON)?)
}

/// Doubled Magic's icon file `data`, decoded and painted green.
fn painted(data: &[u8]) -> Result<ounce_formats::Texture> {
    let mut t = top_level(data)?;
    paint(&mut t.rgba);
    Ok(t)
}

fn icon_path() -> String {
    ICON.replace(LIKE.get(), NAME)
}

fn model() -> String {
    LOOK.name(1)
}

/// The label naming the Magic Meter: its pack lives only in the seed, and
/// it answers to its own name.
pub fn name_label(item: &str) -> Option<String> {
    (item == NAME).then(|| key_tints::label(NAME))
}

/// The Magic Meter's pack, if `item` is it, built as [`write`] writes it.
pub fn copy_pack(romfs: &Romfs, item: &str) -> Result<Option<ActorPack>> {
    if item != NAME {
        return Ok(None);
    }
    let template = ActorPack::load(romfs, LIKE)?;
    clone_pack(&template, NAME, copied(&template, NAME, &model())?).map(Some)
}

/// Its `IsGet` and pouch count, as Doubled Magic's.
pub fn game_data(root: &mut Byml) -> Result<()> {
    gamedata::copy_item(root, LIKE.get(), NAME)
}

/// The dump's value a seed's GameData copies `hash` from, when `hash` is
/// the meter's `IsGet`. Only a seed's GameData has the meter, so a new
/// game built from the dump's takes the default of the one it copies.
pub fn copied_from(hash: u32) -> Option<u32> {
    let is_get = |item: &str| murmur3(format!("{}.{item}", gamedata::IS_GET).as_bytes());
    (hash == is_get(NAME)).then(|| is_get(LIKE.get()))
}

/// Its model, icon, pack and rows. Its pouch row is Doubled Magic's with
/// the single meter and a plain deal: the deal is what makes a gift a
/// gift (Improved Spin's wave rides on its own).
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    let model = Host::read(romfs, &LOOK)?.paint_with(builder, 1, &paint)?;
    let template = ActorPack::load(romfs, LIKE)?;
    write_clone(builder, &template, NAME, copied(&template, NAME, &model)?)?;
    tables.copy_item(NAME, LIKE)?;
    tables.name_itself(NAME)?;
    tables.show(NAME, LIKE, &model)?;
    let rows = tables.edit_pouch(|row| {
        if text(row, crate::rsdb::ROW_ID) != Some(NAME) {
            return false;
        }
        row.insert(SIZE.into(), Byml::I32(SINGLE));
        row.insert(field::POUCH_SPECIAL_DEAL.get().into(), PLAIN_DEAL.into());
        true
    })?;
    ensure!(rows == 1, "{rows} Magic Meter rows");
    let data = romfs.read(ICON)?;
    let t = painted(&data)?;
    builder
        .add(&icon_path(), &redraw(&data, &t.rgba, |_, _| false)?, ICON)
        .context("the Magic Meter's icon")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_tints::MADE;

    #[test]
    fn the_extracted_files_are_the_look() {
        needs_names!();
        let mut files: Vec<_> = LOOK.files().collect();
        files.push(ICON.into());
        assert_eq!(files, FILES);
    }

    #[test]
    fn it_is_ours_and_its_model_as_long_as_doubled_magics() {
        needs_names!();
        assert!(NAME.starts_with(MADE));
        assert_eq!(model().len(), LIKE.get().len());
    }

    /// The copy is matte, so the green shows instead of the room it
    /// would reflect, and the host keeps its metal.
    #[test]
    fn the_copy_has_no_metal() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-meter-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&romfs, &mut builder, &mut tables).unwrap();
        builder.finish().unwrap();
        let metal = |data: &[u8]| {
            let t = top_level(data).unwrap();
            t.rgba.chunks(4).map(|px| px[0]).max().unwrap()
        };
        let host = FILES[3];
        let copy = host.replace(LIKE.get(), &model());
        let written = std::fs::read(dir.join("romfs").join(&copy)).unwrap();
        let written = romfs.dicts().decompress(&copy, &written).unwrap();
        assert_eq!(metal(&written), 0);
        assert!(metal(&romfs.read(host).unwrap()) > 0);
        std::fs::remove_dir_all(dir).ok();
    }

    /// The sigil's and wreath's gold turns green, in the model and icon.
    #[test]
    fn the_gold_turns_green() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // The albedo, after the model.
        let mut rgba = top_level(&romfs.read(FILES[1]).unwrap()).unwrap().rgba;
        assert!(paint::mean(&rgba, GOLD).is_some());
        paint(&mut rgba);
        assert!(paint::mean(&rgba, GOLD).is_none(), "gold left");
        let green = (GREEN.0 - 0.05, GREEN.0 + 0.05);
        let (_, s, _) = paint::mean(&rgba, green).unwrap();
        assert!(s > 0.7, "a pale green, {s}");
        let icon = icon(&romfs).unwrap();
        assert!(
            paint::mean(&icon.rgba, GOLD).is_none(),
            "the icon kept gold"
        );
    }
}
