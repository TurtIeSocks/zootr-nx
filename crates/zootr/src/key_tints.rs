//! Small keys in their dungeon's colour. Each dungeon with keys gets a
//! copy of the key model, its albedo tinted, and copies of both key actors
//! showing it. The dungeon's key chests and floor keys name the copies,
//! and each copy keeps its pouch row, so the game credits it as a key.
//! The Gerudo Fortress's keys, their own actors already, show a model of
//! their own.

use anyhow::{Context, Result};
use ounce_formats::{
    ActorPack, Byml, BymlDoc, ModBuilder, Romfs, levels, pack_file, redraw, rename,
};

use crate::checks::{self, DROP_ACTOR, GYAML};
use crate::keys::{DUNGEONS, FORTRESS, FORTRESS_KEY, SMALL};
use crate::known::{Name, actor, field, flag, flow, item};
use crate::medallions::{self, Colour, model_path, texture_path};
use crate::standins::{
    MODEL, Tables, actor_path, doc, path_of, raw, renamed, with_ref, write_clone,
};

/// The key's model, named like the floor key that shows it.
const HOST: &Name = actor::OBJ_DUNGEON_SMALL_KEY;
/// The model's own textures, by suffix. Only the albedo is tinted.
const KINDS: [&str; 5] = ["Alb", "AO", "Mtl", "Nrm", "Rgh"];
/// The files the copies are made from, extracted before a dump is open.
pub const FILES: [&str; 6] = [
    "Model/Obj_Dungeon_SmallKey.Obj_Dungeon_SmallKey.bfres.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Alb.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_AO.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Mtl.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Nrm.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Rgh.bntx.zs",
];

/// A dungeon's colour: a temple's its medallion song's, the Training
/// Ground the Temple of Time's.
const SONGS: [(&Name, &Name); 6] = [
    (flag::FOREST_TEMPLE, item::MELODY_FOREST_MINUET),
    (flag::FIRE_TEMPLE, item::MELODY_FIRE_BOLERO),
    (flag::WATER_TEMPLE, item::MELODY_WATER_SERENADE),
    (flag::SHADOW_TEMPLE, item::MELODY_SHADOW_NOCTURNE),
    (flag::SPIRIT_TEMPLE, item::MELODY_SPIRIT_REQUIEM),
    (flag::GERUDO_TRAINING_GROUND, item::MELODY_LIGHT_PRELUDE),
];
/// The rest. Ganon's black is lifted so the key still reads as metal.
const OWN: [(&Name, Colour); 2] = [
    (flow::BOTTOM_OF_THE_WELL, [110, 35, 150]),
    (flag::INSIDE_GANON_CASTLE, [45, 45, 50]),
];

/// The Gerudo Fortress's keys are their own actors already, one for each
/// thief. They show the model after the dungeons'.
const FORTRESS_MODEL: usize = DUNGEONS.len() + 1;
const FORTRESS_COLOUR: Colour = [90, 55, 30];

fn colour(dungeon: &Name) -> Option<Colour> {
    let song = SONGS.iter().find(|(d, _)| *d == dungeon);
    song.and_then(|(_, s)| medallions::colour(s))
        .or_else(|| OWN.iter().find(|(d, _)| *d == dungeon).map(|(_, c)| *c))
}

/// A dungeon's copies, by its place in [`DUNGEONS`], counting from 1: the
/// model (as long as the host's, so no offset in a copy moves), the chest
/// content and the floor key.
fn model(n: usize) -> String {
    format!("Obj_Rando_SmallKey{n:02}")
}

fn chest_key(n: usize) -> String {
    format!("Item_Rando_SmallKey{n:02}")
}

/// The dungeon whose maps `rel`, a Banc file, is under, by its number.
fn dungeon_of(rel: &str) -> Option<usize> {
    let folder = rel.strip_prefix("Banc/")?.split('/').next()?;
    DUNGEONS
        .iter()
        .position(|(d, ..)| *d == folder)
        .map(|i| i + 1)
}

/// Writes each dungeon's model and actor copies, and points the Fortress
/// keys at a model of their own.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    let host = Host::read(romfs)?;
    let floor = ActorPack::load(romfs, HOST)?;
    let chest = ActorPack::load(romfs, SMALL)?;
    for (i, (dungeon, ..)) in DUNGEONS.iter().enumerate() {
        let n = i + 1;
        let tint = colour(dungeon).with_context(|| format!("no colour for {dungeon}"))?;
        let name = host.paint(builder, n, tint)?;
        for (template, copy) in [(&chest, chest_key(n)), (&floor, name.clone())] {
            let files = pointed(template, &copy, &name)?;
            write_clone(builder, template, &copy, files)?;
            tables.copy_item(&copy, &template.name)?;
            tables.show(&copy, &template.name, &name)?;
        }
    }
    let name = host.paint(builder, FORTRESS_MODEL, FORTRESS_COLOUR)?;
    for k in 1..=FORTRESS.1 {
        let pack = ActorPack::load(romfs, &format!("{FORTRESS_KEY}{k:02}"))?;
        repoint(builder, &pack, &name)?;
        tables.show(&pack.name, HOST, &name)?;
    }
    Ok(())
}

/// The host model and its textures, read once.
struct Host {
    model: Vec<u8>,
    textures: Vec<(&'static str, Vec<u8>)>,
}

impl Host {
    fn read(romfs: &Romfs) -> Result<Self> {
        let textures = KINDS
            .iter()
            .map(|kind| Ok((*kind, romfs.read(&texture_path(HOST, kind))?)))
            .collect::<Result<_>>()?;
        Ok(Self {
            model: romfs.read(&model_path(HOST))?,
            textures,
        })
    }

    /// Model `n`, its albedo tinted `tint`. Returns its name.
    fn paint(&self, builder: &mut ModBuilder, n: usize, tint: Colour) -> Result<String> {
        let name = model(n);
        let like = model_path(HOST);
        builder.add(
            &model_path(&name),
            &rename(&self.model, HOST, &name)?,
            &like,
        )?;
        for (kind, data) in &self.textures {
            let mut data = rename(data, HOST, &name)?;
            if *kind == "Alb" {
                data = tinted(&data, tint)?;
            }
            builder.add(&texture_path(&name, kind), &data, &texture_path(HOST, kind))?;
        }
        Ok(name)
    }
}

/// A texture's first level multiplied by `colour`. The albedo is a near
/// neutral grey, so the key takes the colour and the metal maps keep the
/// shine.
fn tinted(data: &[u8], colour: Colour) -> Result<Vec<u8>> {
    let mut t = levels(data)?.swap_remove(0);
    multiply(&mut t.rgba, colour);
    redraw(data, &t.rgba, |_, _| false)
}

fn multiply(rgba: &mut [u8], colour: Colour) {
    for px in rgba.chunks_mut(4) {
        for (c, tint) in px.iter_mut().zip(colour) {
            *c = ((*c as u16 * tint as u16 + 127) / 255) as u8;
        }
    }
}

/// `name`'s ActorParam and ModelInfo as `template`'s, the ModelInfo a file
/// of its own naming `model`, since packs sharing a file share whichever
/// loaded first. Each is `(path, doc, the template file it copies)`.
fn pointed(
    template: &ActorPack,
    name: &str,
    model: &str,
) -> Result<[(String, BymlDoc, String); 2]> {
    let actor_like = actor_path(&template.name);
    let info_like = path_of(template, MODEL)?;
    let info_path = renamed(&info_like, name);
    let actor_doc = BymlDoc::parse(&raw(template, &actor_like)?)?;
    let actor = with_ref(actor_doc.root.as_map()?, MODEL, &info_path)?;
    let info_doc = BymlDoc::parse(&raw(template, &info_like)?)?;
    let mut info = info_doc.root.as_map()?.clone();
    for key in [field::FMDB_NAME, field::MODEL_PROJECT_NAME] {
        info.insert(key.get().into(), Byml::String(model.into()));
    }
    Ok([
        (actor_path(name), doc(actor, &actor_doc), actor_like),
        (info_path, doc(info, &info_doc), info_like),
    ])
}

/// `pack` rewritten in place to show `model`.
fn repoint(builder: &mut ModBuilder, pack: &ActorPack, model: &str) -> Result<()> {
    let mut writer = pack.writer();
    for (path, data, like) in pointed(pack, &pack.name, model)? {
        let data = data.to_binary();
        let like_size = raw(pack, &like)?.len();
        match path == like {
            true => builder.resize(&path, like_size, data.len())?,
            false => builder.add_entry(&path, &like, like_size, data.len())?,
        }
        writer.add_file(path, data);
    }
    builder.write(&pack_file(&pack.name), &writer.to_binary())
}

/// Swaps the small keys placed in `rel`'s map, a dungeon's, for that
/// dungeon's copies: a chest's content and a floor key. Returns how many.
pub fn swap(rel: &str, root: &mut Byml) -> Result<usize> {
    let Some(n) = dungeon_of(rel) else {
        return Ok(0);
    };
    let mut swapped = 0;
    for actor in checks::actors_mut(root)? {
        if checks::str_field(actor, GYAML) == Some(HOST.as_ref()) {
            actor.as_mut_map()?.insert(GYAML.into(), model(n).into());
            swapped += 1;
        }
        let Some(dynamic) = actor.as_mut_map()?.get_mut("Dynamic") else {
            continue;
        };
        if checks::str_field(dynamic, DROP_ACTOR) == Some(SMALL.as_ref()) {
            dynamic
                .as_mut_map()?
                .insert((&**DROP_ACTOR).into(), chest_key(n).into());
            swapped += 1;
        }
    }
    Ok(swapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    #[test]
    fn a_grey_texel_takes_the_colour_scaled_by_its_grey() {
        let mut rgba = [255, 255, 255, 255, 128, 128, 128, 200];
        multiply(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [60, 220, 60, 255, 30, 110, 30, 200]);
    }

    #[test]
    fn every_dungeon_with_keys_has_a_colour() {
        for (dungeon, ..) in DUNGEONS {
            assert!(colour(dungeon).is_some(), "{dungeon}");
        }
    }

    #[test]
    fn copies_are_named_as_long_as_the_host() {
        let host = FILES[0].split(['/', '.']).nth(1).unwrap();
        assert_eq!(model(1).len(), host.len());
    }

    #[test]
    fn each_dungeon_s_keys_name_its_copies() {
        let Some(romfs) = dev_dump() else { return };
        let mut per_dungeon = vec![0; DUNGEONS.len() + 1];
        for rel in checks::banc_files(&romfs).unwrap() {
            let mut doc = checks::load_banc(&romfs, &rel).unwrap();
            let swapped = swap(&rel, &mut doc.root).unwrap();
            let Some(n) = dungeon_of(&rel) else {
                assert_eq!(swapped, 0, "{rel}");
                continue;
            };
            per_dungeon[n] += swapped;
            for actor in checks::actors(&doc.root).unwrap() {
                assert_ne!(
                    checks::str_field(actor, GYAML),
                    Some(HOST.as_ref()),
                    "{rel}"
                );
            }
        }
        for (i, (dungeon, ..)) in DUNGEONS.iter().enumerate() {
            assert!(per_dungeon[i + 1] > 0, "no keys swapped in {dungeon}");
        }
    }

    #[test]
    fn each_copy_shows_its_own_model_from_its_own_file() {
        let Some(romfs) = dev_dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-key-tints-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&romfs, &mut builder, &mut tables).unwrap();
        builder.finish().unwrap();
        let read = |rel: &str| {
            let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
            romfs.dicts().decompress(rel, &raw).unwrap()
        };
        let info = path_of(&ActorPack::load(&romfs, SMALL).unwrap(), MODEL).unwrap();
        let has = |bytes: &[u8], s: &str| bytes.windows(s.len()).any(|w| w == s.as_bytes());
        for n in 1..=DUNGEONS.len() {
            assert!(!read(&model_path(&model(n))).is_empty());
            for copy in [chest_key(n), model(n)] {
                let row = crate::standins::row(&tables.actor, &copy).unwrap();
                let fmdb = row
                    .get(field::FMDB_NAME.get())
                    .and_then(|v| v.as_string().ok());
                assert_eq!(fmdb.map(|s| s.to_string()), Some(model(n)), "{copy}");
                crate::standins::row(&tables.pouch, &copy).unwrap();
                let pack = read(&format!("Pack/Actor/{copy}.pack.zs"));
                assert!(
                    has(&pack, &renamed(&info, &copy)),
                    "{copy} has no ModelInfo of its own"
                );
            }
        }
        let floor_info = path_of(&ActorPack::load(&romfs, HOST).unwrap(), MODEL).unwrap();
        for k in 1..=FORTRESS.1 {
            let key = format!("{FORTRESS_KEY}{k:02}");
            let row = crate::standins::row(&tables.actor, &key).unwrap();
            let fmdb = row
                .get(field::FMDB_NAME.get())
                .and_then(|v| v.as_string().ok());
            assert_eq!(fmdb.map(|s| s.to_string()), Some(model(FORTRESS_MODEL)));
            let pack = read(&format!("Pack/Actor/{key}.pack.zs"));
            assert!(has(&pack, &renamed(&floor_info, &key)), "{key}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
