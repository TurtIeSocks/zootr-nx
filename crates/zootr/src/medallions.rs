//! Song medallions: each song placed in the world shows a copy of the Forest
//! Medallion named for it, where the game shows a rupee. The copies are
//! built from the player's dump when a seed is written. Nothing from the
//! game is committed.

pub(crate) mod paint;

use anyhow::{Context, Result};
use ounce_formats::{
    ActorPack, Byml, BymlDoc, ModBuilder, Romfs, Texture, Vertex, edit_vertices, redraw, rename,
    textures_named, top_level, vertices,
};

use crate::known::{Name, field, item};
use crate::sprites;
use crate::standins::{self, ANIMATION, COMPONENTS, MODEL, Tables};
pub(crate) use paint::Colour;

/// The medallion every song's is copied from. Its name and a song's,
/// a song medal's, are as long, so no offset in a copy moves.
pub(crate) const HOST: &Name = item::MEDAL_FOREST;
/// The host's own textures, each its own file.
const KINDS: [Kind; 6] = [
    Kind::Alb,
    Kind::Emc,
    Kind::Nrm,
    Kind::Ao,
    Kind::Mtl,
    Kind::Rgh,
];
/// The host's body. Its front face is a shallow dish mapped flat onto
/// the textures' face, with the Forest emblem modelled over it: grooves
/// and ridges between `FACE_FLOOR` and its rim, `FACE_EDGE` out, drawn
/// from elsewhere on the textures.
const BODY: &Name = field::MEDAL_BASE_MT_ITEM_MEDAL_FOREST;
const FACE_FLOOR: f32 = 0.0095;
const FACE_EDGE: f32 = 0.106;
/// The dish's height at its rim, which the emblem is laid flat at.
const FACE_Z: f32 = 0.013;
/// The host's files and the songbook's icons, which the dump's cache
/// extracts besides its folders.
// Text: the extract runs before a dump is open, so no name can be read.
pub const FILES: [&str; 8] = [
    "Model/Item_Medal_Forest.Item_Medal_Forest.bfres.zs",
    "Tex/Mt_Item_Medal_Forest_Alb.bntx.zs",
    "Tex/Mt_Item_Medal_Forest_Emc.bntx.zs",
    "Tex/Mt_Item_Medal_Forest_Nrm.bntx.zs",
    "Tex/Mt_Item_Medal_Forest_AO.bntx.zs",
    "Tex/Mt_Item_Medal_Forest_Mtl.bntx.zs",
    "Tex/Mt_Item_Medal_Forest_Rgh.bntx.zs",
    sprites::LAYOUT,
];
/// Where a song's spin in the get window comes from: the Agony Stone's
/// animation sequence, which turns its model's root bone there, and the
/// animations the menu's copy of the host plays on the host's own bones.
const SPINNER: &Name = item::AGONY_STONE;
const SPINS: [&Name; 2] = [field::AS_REF, field::AS_INFO_REF];
const MENU_HOST: &Name = item::MEDAL_FOREST_IMPORTANT_MENU;
/// The songbook icons' names, before each song's number.
const ICON: &str = "OcarinaMelodyIcon_";
/// Each song's colour: a warp song its medallion's.
const COLOURS: [(&Name, Colour); 12] = [
    (item::MELODY_ZELDA_LULLABY, [170, 120, 255]),
    (item::MELODY_EPONA_SONG, [255, 150, 60]),
    (item::MELODY_SARIA_SONG, [90, 230, 90]),
    (item::MELODY_SUN_SONG, [255, 220, 60]),
    (item::MELODY_TIME_SONG, [90, 200, 255]),
    (item::MELODY_STORM_SONG, [150, 170, 210]),
    (item::MELODY_FOREST_MINUET, [60, 220, 60]),
    (item::MELODY_FIRE_BOLERO, [255, 70, 40]),
    (item::MELODY_WATER_SERENADE, [60, 120, 255]),
    (item::MELODY_SPIRIT_REQUIEM, [255, 140, 30]),
    (item::MELODY_SHADOW_NOCTURNE, [170, 60, 230]),
    (item::MELODY_LIGHT_PRELUDE, [255, 240, 120]),
];

pub(crate) fn model_path(name: &str) -> String {
    model_file(name, name)
}

/// Model `fmdb`'s file in model project `project`.
pub(crate) fn model_file(project: &str, fmdb: &str) -> String {
    format!("Model/{project}.{fmdb}.bfres.zs")
}

pub(crate) fn texture_path(name: &str, kind: &str) -> String {
    format!("Tex/Mt_{name}_{kind}.bntx.zs")
}

/// A kind of texture the host wears.
#[derive(Clone, Copy)]
enum Kind {
    Alb,
    Emc,
    Nrm,
    Ao,
    Mtl,
    Rgh,
}

impl Kind {
    /// The suffix its file's name ends in.
    fn suffix(self) -> &'static str {
        match self {
            Kind::Alb => "Alb",
            Kind::Emc => "Emc",
            Kind::Nrm => "Nrm",
            Kind::Ao => "AO",
            Kind::Mtl => "Mtl",
            Kind::Rgh => "Rgh",
        }
    }
}

/// The medallion `item` shows, if it's a song: named by its songbook icon.
pub(crate) fn medallion(item: &str) -> Option<String> {
    sprites::songs()
        .find(|(_, song)| *song == item)
        .map(|(icon, _)| format!("{}Song{icon}", HOST.stem()))
}

/// A song's model info path and contents naming its medallion, or `None`
/// for any other pack. Every pack holding the path must carry these bytes,
/// since whichever loads first serves it to all: the song's own and any
/// stand-in copying it.
pub(crate) fn model_info(pack: &ActorPack) -> Result<Option<(String, Vec<u8>)>> {
    let Some(name) = medallion(&pack.name) else {
        return Ok(None);
    };
    let path = standins::path_of(pack, MODEL)?;
    let mut doc = BymlDoc::parse(&standins::raw(pack, &path)?)?;
    let info = doc.root.as_mut_map()?;
    for key in [field::FMDB_NAME, field::MODEL_PROJECT_NAME] {
        info.insert((&**key).into(), Byml::String(name.as_str().into()));
    }
    Ok(Some((path, doc.to_binary())))
}

/// Writes each song's medallion and points the song at it, when `items`
/// holds a song.
pub fn write<'a>(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    mut items: impl Iterator<Item = &'a str>,
) -> Result<()> {
    if !items.any(|i| medallion(i).is_some()) {
        return Ok(());
    }
    let host_model = model_path(HOST);
    let model = flatten_face(&romfs.read(&host_model)?)?;
    let textures = KINDS
        .iter()
        .map(|kind| Ok((*kind, romfs.read(&texture_path(HOST, kind.suffix()))?)))
        .collect::<Result<Vec<_>>>()?;
    let icons = textures_named(&romfs.read(sprites::LAYOUT)?, |n| n.starts_with(ICON))?;
    let (spinner, menu_host) = (
        ActorPack::load(romfs, SPINNER)?,
        ActorPack::load(romfs, MENU_HOST)?,
    );
    let lenders = SPINS
        .map(|key| (key.get(), &spinner))
        .into_iter()
        .chain([(ANIMATION.get(), &menu_host)])
        .collect::<Vec<_>>();
    for (number, song) in sprites::songs() {
        let name = medallion(song).with_context(|| format!("{song} has no medallion"))?;
        builder.add(
            &model_path(&name),
            &rename(&model, HOST, &name)?,
            &host_model,
        )?;
        let icon_name = format!("{ICON}{number}");
        let icon = icons
            .iter()
            .find(|t| t.name == icon_name)
            .with_context(|| format!("no icon {icon_name}"))?;
        let colour = colour(song).with_context(|| format!("no colour for {song}"))?;
        for (kind, data) in &textures {
            let like = texture_path(HOST, kind.suffix());
            let drawn = draw(*kind, &rename(data, HOST, &name)?, icon, colour)?;
            builder.add(&texture_path(&name, kind.suffix()), &drawn, &like)?;
        }
        let pack = ActorPack::load(romfs, song)?;
        rewrite_pack(builder, &pack, &lenders, &spinner)?;
        tables.show(song, HOST, &name)?;
        tables.take(song, MENU_HOST, &[field::ANIMATION_RESOURCES])?;
        tables.add_heaps(song, SPINNER)?;
    }
    Ok(())
}

/// The host model with the Forest emblem laid flat on its face and drawn
/// from the face's own texture, where the icon is painted, lit as the
/// dish's middle is.
fn flatten_face(model: &[u8]) -> Result<Vec<u8>> {
    let middle = vertices(model, BODY)?
        .into_iter()
        .find(|v| radius(v) < 1e-3 && on_face_map(v))
        .context("the face's middle")?;
    edit_vertices(model, BODY, |v| {
        if in_face(v) && !on_face_map(v) {
            v.position[2] = FACE_Z;
            v.uv = face_uv(v);
            v.normal = middle.normal;
            v.tangent = middle.tangent;
        }
    })
}

fn radius(v: &Vertex) -> f32 {
    v.position[0].hypot(v.position[1])
}

fn in_face(v: &Vertex) -> bool {
    v.position[2] > FACE_FLOOR && radius(v) < FACE_EDGE
}

/// Where the face's flat mapping puts `v` on the textures.
fn face_uv(v: &Vertex) -> [f32; 2] {
    let [x, y, _] = v.position;
    [
        paint::FACE.0 + paint::FACE_SCALE * x,
        paint::FACE.1 - paint::FACE_SCALE * y,
    ]
}

fn on_face_map(v: &Vertex) -> bool {
    let uv = face_uv(v);
    (uv[0] - v.uv[0]).abs() < 2e-3 && (uv[1] - v.uv[1]).abs() < 2e-3
}

/// A song's pack showing its medallion, and taking each component of
/// `lenders` from the pack lending it, the file it names too. Its actor
/// file is sized like `spinner`'s.
fn rewrite_pack(
    builder: &mut ModBuilder,
    pack: &ActorPack,
    lenders: &[(&str, &ActorPack)],
    spinner: &ActorPack,
) -> Result<()> {
    let mut writer = pack.writer();
    let info = model_info(pack)?;
    let (info_path, info) = info.with_context(|| format!("{} isn't a song", pack.name))?;
    let old_info = standins::raw(pack, &info_path)?.len();
    builder.resize(&info_path, old_info, info.len())?;
    writer.add_file(info_path, info);
    let actor_path = standins::actor_path(&pack.name);
    let actor_raw = standins::raw(pack, &actor_path)?;
    let mut actor = BymlDoc::parse(&actor_raw)?;
    let mut refs = standins::components(actor.root.as_map()?)?;
    for (key, lender) in lenders {
        standins::lend(&mut writer, &mut refs, key, lender)?;
    }
    actor
        .root
        .as_mut_map()?
        .insert(COMPONENTS.into(), Byml::Map(refs));
    let new_actor = actor.to_binary();
    let old_actor = actor_raw.len();
    let like = standins::actor_path(&spinner.name);
    let like_size = standins::raw(spinner, &like)?.len();
    builder.resize_like(&actor_path, old_actor, new_actor.len(), &like, like_size)?;
    writer.add_file(actor_path, new_actor);
    builder.write(&ounce_formats::pack_file(&pack.name), &writer.to_binary())
}

pub(crate) fn colour(song: &str) -> Option<Colour> {
    COLOURS.iter().find(|(s, _)| *s == song).map(|(_, c)| *c)
}

/// A host texture of `kind` as a song's medallion wears it: the albedo and
/// glow in its colour, the icon on the face, raised in the normal map and
/// unshaded in the occlusion map. The rest are the host's.
fn draw(kind: Kind, data: &[u8], icon: &Texture, colour: Colour) -> Result<Vec<u8>> {
    match kind {
        Kind::Alb | Kind::Emc => {
            let mut t = top_level(data)?;
            paint::recolour(&mut t, colour);
            if matches!(kind, Kind::Alb) {
                paint::paint_icon(&mut t, icon, colour);
            }
            redraw(data, &t.rgba, |_, _| false)
        }
        Kind::Nrm | Kind::Ao => {
            let mut t = top_level(data)?;
            match kind {
                Kind::Nrm => paint::emboss(&mut t, icon),
                _ => paint::fill(&mut t, 255),
            }
            let (w, h) = (t.width, t.height);
            redraw(data, &t.rgba, |x, y| !paint::on_face(w, h, x, y))
        }
        Kind::Mtl | Kind::Rgh => Ok(data.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{dev_dump, packs_holding};
    use crate::known::{field, item};
    use ounce_formats::levels;

    #[test]
    fn the_host_files_are_where_its_name_puts_them() {
        needs_names!();
        let mut named = vec![model_path(HOST)];
        named.extend(KINDS.iter().map(|k| texture_path(HOST, k.suffix())));
        assert_eq!(named, FILES[..7]);
    }

    #[test]
    fn every_song_has_a_medallion_named_as_long_as_the_host() {
        needs_names!();
        let songs: Vec<_> = sprites::songs().collect();
        assert_eq!(songs.len(), 12);
        for (_, song) in songs {
            assert_eq!(medallion(song).unwrap().len(), HOST.len(), "{song}");
        }
        assert_eq!(medallion("Item_A"), None);
    }

    /// The mean difference per channel between two RGBA8 images.
    fn mean_diff(a: &[u8], b: &[u8]) -> f64 {
        let total: u64 = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u64).sum();
        total as f64 / a.len() as f64
    }

    #[test]
    fn each_host_mip_level_is_the_one_above_it_halved() {
        let Some(romfs) = dev_dump() else { return };
        for file in &FILES[1..7] {
            let levels = levels(&romfs.read(file).unwrap()).unwrap();
            for pair in levels.windows(2).take(4) {
                let (big, small) = (&pair[0], &pair[1]);
                let shrunk = ounce_formats::redraw_halve(&big.rgba, big.width, big.height);
                let d = mean_diff(&shrunk, &small.rgba);
                assert!(d < 12.0, "{file} at {}: {d}", small.width);
            }
        }
    }

    #[test]
    fn a_redrawn_host_texture_decodes_close_to_its_own() {
        let Some(romfs) = dev_dump() else { return };
        for file in &FILES[1..7] {
            let data = romfs.read(file).unwrap();
            let own = &levels(&data).unwrap()[0];
            assert!(redraw(&data, &own.rgba, |_, _| true).unwrap() == data);
            let drawn = levels(&redraw(&data, &own.rgba, |_, _| false).unwrap()).unwrap();
            let mut meant = own.rgba.clone();
            // Deeper levels pack more detail into each two-colour block.
            for (i, level) in drawn.iter().enumerate().take(3) {
                if i > 0 {
                    let above = &drawn[i - 1];
                    meant = ounce_formats::redraw_halve(&meant, above.width, above.height);
                }
                let d = mean_diff(&level.rgba, &meant);
                assert!(d < 6.0, "{file} level {i}: {d}");
            }
        }
    }

    /// A redraw is kept by its inputs: the same ones give the kept file,
    /// other texels or another keep never do.
    #[test]
    fn a_kept_redraw_answers_only_its_own_inputs() {
        let Some(romfs) = dev_dump() else { return };
        let data = romfs.read(FILES[1]).unwrap();
        let own = &levels(&data).unwrap()[0];
        let drawn = redraw(&data, &own.rgba, |_, _| false).unwrap();
        assert!(redraw(&data, &own.rgba, |_, _| false).unwrap() == drawn);
        let mut dark = own.rgba.clone();
        dark.iter_mut().for_each(|c| *c /= 2);
        assert!(redraw(&data, &dark, |_, _| false).unwrap() != drawn);
        assert!(redraw(&data, &own.rgba, |_, _| true).unwrap() == data);
    }

    #[test]
    fn rebuilt_dictionaries_match_the_hosts_own() {
        let Some(romfs) = dev_dump() else { return };
        for file in &FILES[..7] {
            let data = romfs.read(file).unwrap();
            assert!(rename(&data, HOST, HOST).unwrap() == data, "{file}");
        }
    }

    #[test]
    fn the_flattened_face_is_drawn_from_the_painted_disc_alone() {
        let Some(romfs) = dev_dump() else { return };
        let data = romfs.read(&model_path(HOST)).unwrap();
        let before = vertices(&data, BODY).unwrap();
        // The dish: its middle and a ring at the rim, already on the map.
        let dish = before
            .iter()
            .filter(|v| in_face(v) && on_face_map(v))
            .count();
        assert!(dish >= 11, "{dish}");
        let after = vertices(&flatten_face(&data).unwrap(), BODY).unwrap();
        let moved = before.iter().zip(&after).filter(|(b, a)| b != a).count();
        assert!(moved > 900, "{moved}");
        for (b, a) in before.iter().zip(&after) {
            if in_face(b) {
                assert!(on_face_map(a) && a.position[2] <= FACE_Z + 1e-3, "{b:?}");
            } else {
                assert_eq!(b, a);
            }
        }
    }

    #[test]
    fn the_lenders_have_the_spins_parts() {
        let Some(romfs) = dev_dump() else { return };
        let spinner = ActorPack::load(&romfs, SPINNER).unwrap();
        let param = spinner.actor_param().unwrap().unwrap();
        for key in SPINS.iter() {
            assert!(standins::component(&param, key).is_some(), "{key}");
        }
        let menu = ActorPack::load(&romfs, MENU_HOST).unwrap();
        let param = menu.actor_param().unwrap().unwrap();
        let animation = standins::component(&param, ANIMATION).unwrap();
        let (_, data) = menu.find(&animation).unwrap();
        let text = String::from_utf8_lossy(data);
        assert!(text.contains(&**item::COMMON_ANIMATION));
        // The menu's copy shows the host's own model.
        let model = standins::component(&param, MODEL).unwrap();
        assert!(model.contains(&**HOST), "{model}");
    }

    #[test]
    fn every_song_has_a_colour() {
        for (_, song) in sprites::songs() {
            assert!(colour(song).is_some(), "{song}");
        }
    }

    #[test]
    fn every_song_has_a_songbook_icon() {
        let Some(romfs) = dev_dump() else { return };
        let data = romfs.read(sprites::LAYOUT).unwrap();
        let icons = textures_named(&data, |n| n.starts_with(ICON)).unwrap();
        for (number, song) in sprites::songs() {
            let name = format!("{ICON}{number}");
            assert!(icons.iter().any(|t| t.name == name), "{song}");
        }
    }

    #[test]
    fn only_its_own_pack_holds_a_songs_model_info() {
        let Some(romfs) = dev_dump() else { return };
        for (_, song) in sprites::songs() {
            let pack = ActorPack::load(&romfs, song).unwrap();
            let (path, info) = model_info(&pack).unwrap().unwrap();
            assert_eq!(packs_holding(&romfs, &path), [song]);
            let doc = BymlDoc::parse(&info).unwrap();
            let fmdb = doc.root.as_map().unwrap().get(&**field::FMDB_NAME).cloned();
            assert_eq!(fmdb, Some(Byml::String(medallion(song).unwrap().into())));
        }
    }
}
