//! Keys in their dungeon's colour. Each dungeon with keys gets a copy of
//! the key model, its albedo tinted, and copies of the key actors showing
//! it: the small key's chest content and floor key, and the boss key where
//! the dungeon has one. The dungeon's key chests and floor keys name the
//! copies, and each copy keeps its pouch row, so the game credits a small
//! key. A chest's flow knows the boss key only by its own name, so the hook
//! credits a boss key copy, and a seed without the hook keeps the boss key
//! as it is. With key rings, each dungeon's ring is a small key copy the
//! game doesn't count, showing a ring of its keys ([`rings`]), and the hook
//! fills the counter. With Keysanity the same copy is a single key, and
//! shows one. The Gerudo
//! Fortress's keys, their own actors already, show a model of their own.

mod rings;

use anyhow::{Context, Result, ensure};
use ounce_formats::{
    ActorPack, Byml, BymlDoc, ModBuilder, Romfs, levels, murmur3, pack_file, redraw, rename,
};
use zootr_hook::Credit;

use crate::checks::{self, DROP_ACTOR, GYAML};
use crate::keys::{
    BOSS, DUNGEONS, FORTRESS, FORTRESS_KEY, SMALL, boss_flag, dungeon_of, has_boss, small_flag,
};
use crate::known::{Name, actor, field, flag, flow, item};
use crate::medallions::{self, Colour, model_path, texture_path};
use crate::rsdb::ROW_ID;
use crate::settings::DungeonKeys;
use crate::standins::{
    MODEL, POUCH, POUCH_ACTOR, Tables, actor_path, clone_pack, doc, path_of, pouch_actor, raw,
    renamed, resolved, with_ref, write_clone,
};
use crate::text::Copies;

/// One kind of key's model: the actor it's named like, its textures by
/// suffix, and the prefix of a dungeon's copy, which with the dungeon's
/// two-digit number is as long as the host so no offset in a copy moves.
struct Look {
    host: &'static Name,
    kinds: &'static [&'static str],
    copy: &'static str,
}

/// The small key's, named like the floor key that shows it.
const SMALL_LOOK: Look = Look {
    host: actor::OBJ_DUNGEON_SMALL_KEY,
    kinds: &["Alb", "AO", "Mtl", "Nrm", "Rgh"],
    copy: "Obj_Rando_SmallKey",
};
/// The boss key's. Its gem glows from its own emissive map, left red so a
/// boss key still reads as one.
const BOSS_LOOK: Look = Look {
    host: BOSS,
    kinds: &["Alb", "AO", "Emc", "Inner_Nrm", "Mtl", "Nrm", "Rgh"],
    copy: "Item_Rando_BossKey",
};
/// A key ring's: the small key's textures with a plain metal corner, on a
/// model of the dungeon's keys around a hoop.
const RING_LOOK: Look = Look {
    copy: "Obj_Rando_KeyRing_",
    ..SMALL_LOOK
};
/// The looks with host files of their own: the ring's are the small key's.
#[cfg(test)]
const LOOKS: [&Look; 2] = [&SMALL_LOOK, &BOSS_LOOK];

impl Look {
    /// Dungeon `n`'s copy, by its place in [`DUNGEONS`], counting from 1.
    fn name(&self, n: usize) -> String {
        format!("{}{n:02}", self.copy)
    }

    /// The model and texture files a copy is made from.
    #[cfg(test)]
    fn files(&self) -> impl Iterator<Item = String> {
        let textures = self.kinds.iter().map(|k| texture_path(self.host, k));
        std::iter::once(model_path(self.host)).chain(textures)
    }
}

/// The files the copies are made from, extracted before a dump is open,
/// so spelled out: a [`Name`] needs the dump's names to read.
pub const FILES: [&str; 14] = [
    "Model/Obj_Dungeon_SmallKey.Obj_Dungeon_SmallKey.bfres.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Alb.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_AO.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Mtl.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Nrm.bntx.zs",
    "Tex/Mt_Obj_Dungeon_SmallKey_Rgh.bntx.zs",
    "Model/Item_Dungeon_BossKey.Item_Dungeon_BossKey.bfres.zs",
    "Tex/Mt_Item_Dungeon_BossKey_Alb.bntx.zs",
    "Tex/Mt_Item_Dungeon_BossKey_AO.bntx.zs",
    "Tex/Mt_Item_Dungeon_BossKey_Emc.bntx.zs",
    "Tex/Mt_Item_Dungeon_BossKey_Inner_Nrm.bntx.zs",
    "Tex/Mt_Item_Dungeon_BossKey_Mtl.bntx.zs",
    "Tex/Mt_Item_Dungeon_BossKey_Nrm.bntx.zs",
    "Tex/Mt_Item_Dungeon_BossKey_Rgh.bntx.zs",
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
/// thief. They show the small key model after the dungeons'.
const FORTRESS_MODEL: usize = DUNGEONS.len() + 1;
const FORTRESS_COLOUR: Colour = [90, 55, 30];

fn colour(dungeon: &Name) -> Option<Colour> {
    let song = SONGS.iter().find(|(d, _)| *d == dungeon);
    song.and_then(|(_, s)| medallions::colour(s))
        .or_else(|| OWN.iter().find(|(d, _)| *d == dungeon).map(|(_, c)| *c))
}

/// Dungeon `n`'s small key as a chest's content. On the floor it's the
/// model's own name.
fn chest_key(n: usize) -> String {
    format!("Item_Rando_SmallKey{n:02}")
}

/// Dungeon `n`'s key ring: its small key as a chest's content, but one
/// the game neither stores nor counts. The hook sets the counter.
pub fn ring(n: usize) -> String {
    format!("Item_Rando_KeyRing{n:02}")
}

/// The model dungeon `n`'s ring copy shows as `keys` deals it: the ring
/// of its keys, or with Keysanity its single tinted key.
fn ring_model(n: usize, keys: DungeonKeys) -> String {
    match keys {
        DungeonKeys::Rings => RING_LOOK.name(n),
        _ => SMALL_LOOK.name(n),
    }
}

/// Dungeon `n`'s boss key copy.
pub fn boss(n: usize) -> String {
    BOSS_LOOK.name(n)
}

/// The pack a ring or boss key copy is built as, the same as [`write`]
/// writes it with `keys`, for a holder that stands in for it. `None` for any other
/// item.
pub fn copy_pack(romfs: &Romfs, item: &str, keys: DungeonKeys) -> Result<Option<ActorPack>> {
    for n in 1..=DUNGEONS.len() {
        let (template, model) = if item == ring(n) {
            (SMALL, ring_model(n, keys))
        } else if has_boss(n) && item == boss(n) {
            (BOSS, boss(n))
        } else {
            continue;
        };
        let template = ActorPack::load(romfs, template)?;
        let files = copied(&template, item, &model)?;
        return clone_pack(&template, item, files).map(Some);
    }
    Ok(None)
}

/// The text each copy's get shows, copied from its template's: a copy
/// answers to its own name, so the game looks its lines up under it.
pub fn captions(rings: bool, bosses: bool) -> Copies<'static> {
    let new = made(rings, bosses)
        .into_iter()
        .flat_map(|(copy, template)| {
            TEXT_PARTS.map(|part| {
                (
                    format!("{copy}{part}"),
                    vec![format!("{}{part}", template.get())],
                )
            })
        })
        .collect();
    Copies {
        file: POUCH_TEXT,
        new,
    }
}

/// Each ring (with `rings`) and boss key copy (with `bosses`) the seed
/// makes, with the game's key it copies.
pub fn made(rings: bool, bosses: bool) -> Vec<(String, &'static Name)> {
    let mut made = Vec::new();
    for n in 1..=DUNGEONS.len() {
        if rings {
            made.push((ring(n), SMALL));
        }
        if bosses && has_boss(n) {
            made.push((boss(n), BOSS));
        }
    }
    made
}

/// The lines a key's get reads, each under the key's name.
const TEXT_PARTS: [&str; 3] = ["_Name", "_GetMessage", "_GetCaption"];
const POUCH_TEXT: &str = "ActorMsg/PouchContent";

/// What a copy sets as it's found: a Bool true, an Int to a value, or one
/// more on an Int.
#[derive(Clone, Debug, PartialEq)]
pub enum Sets {
    Bool(String),
    Int(String, i32),
    Add(String, i32),
}

/// What each copy sets as it's found: with `bosses` a boss key's flag, with
/// key rings a ring's counter, full, and with Keysanity one more on a small
/// key's counter.
pub fn sets(bosses: bool, keys: DungeonKeys) -> Vec<(String, Sets)> {
    let mut out = Vec::new();
    for (i, (dungeon, small, room)) in DUNGEONS.iter().enumerate() {
        if bosses && room.is_some() {
            out.push((boss(i + 1), Sets::Bool(boss_flag(dungeon))));
        }
        let counter = small_flag(dungeon);
        match keys {
            DungeonKeys::Rings => out.push((ring(i + 1), Sets::Int(counter, *small))),
            DungeonKeys::Singles => out.push((ring(i + 1), Sets::Add(counter, 1))),
            DungeonKeys::Vanilla | DungeonKeys::Start => {}
        }
    }
    out
}

/// [`sets`] as the hook's table names it.
pub fn credits(bosses: bool, keys: DungeonKeys) -> Vec<(String, Credit)> {
    let hash = |name: &str| murmur3(name.as_bytes());
    let credit = |s: Sets| match s {
        Sets::Bool(name) => Credit::Bool(hash(&name)),
        Sets::Int(name, value) => Credit::Int(hash(&name), value),
        Sets::Add(name, n) => Credit::Add(hash(&name), n),
    };
    (sets(bosses, keys).into_iter())
        .map(|(item, s)| (item, credit(s)))
        .collect()
}

/// Writes each dungeon's model and actor copies, the boss key's only with
/// `bosses`, and with `keys` dealing copies the ring: showing a ring with
/// key rings, a single key with Keysanity. Points the Fortress keys at a
/// model of their own.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    bosses: bool,
    keys: DungeonKeys,
) -> Result<()> {
    let small = Host::read(romfs, &SMALL_LOOK)?;
    let boss = Host::read(romfs, &BOSS_LOOK)?;
    let floor = ActorPack::load(romfs, SMALL_LOOK.host)?;
    let chest = ActorPack::load(romfs, SMALL)?;
    let boss_chest = ActorPack::load(romfs, BOSS)?;
    for (i, (dungeon, count, _)) in DUNGEONS.iter().enumerate() {
        let n = i + 1;
        let tint = colour(dungeon).with_context(|| format!("no colour for {dungeon}"))?;
        let name = small.paint(builder, n, tint)?;
        copy(builder, tables, &chest, &chest_key(n), &name, false)?;
        copy(builder, tables, &floor, &name, &name, false)?;
        if keys == DungeonKeys::Rings {
            let model = rings::model(&small.model, *count as usize)?;
            small.write_as(builder, &ring_model(n, keys), &model, tint, true)?;
        }
        if keys.copies() {
            copy(
                builder,
                tables,
                &chest,
                &ring(n),
                &ring_model(n, keys),
                true,
            )?;
        }
        if bosses && has_boss(n) {
            let name = boss.paint(builder, n, tint)?;
            copy(builder, tables, &boss_chest, &name, &name, true)?;
        }
    }
    uncounted(tables, bosses, keys.copies())?;
    let name = small.paint(builder, FORTRESS_MODEL, FORTRESS_COLOUR)?;
    for k in 1..=FORTRESS.1 {
        let pack = ActorPack::load(romfs, &format!("{FORTRESS_KEY}{k:02}"))?;
        repoint(builder, &pack, &name)?;
        tables.show(&pack.name, SMALL_LOOK.host, &name)?;
    }
    Ok(())
}

/// Every credited copy's pouch row is a plain item. The template's deal
/// would count a small key for the dungeon Link stands in (`SmallKey`), or
/// keep a pickup from handing over a boss key (`NotPushToPouch`).
fn uncounted(tables: &mut Tables, bosses: bool, rings: bool) -> Result<()> {
    let copies: Vec<String> = made(rings, bosses).into_iter().map(|(c, _)| c).collect();
    let changed = tables.edit_pouch(|row| {
        let id = row.get(ROW_ID).and_then(|v| v.as_string().ok());
        let hit = id.is_some_and(|id| copies.iter().any(|c| c == id));
        if hit {
            row.insert(field::POUCH_SPECIAL_DEAL.get().into(), PLAIN_DEAL.into());
        }
        hit
    })?;
    ensure!(
        changed == copies.len(),
        "{changed} copy rows of {}",
        copies.len()
    );
    Ok(())
}

/// The deal of an item the game stores and counts no other way.
const PLAIN_DEAL: &str = "None";

/// `template` copied as `name`, showing `model`, with its rows. An `own`
/// copy answers to its own name (a ring or boss key copy the hook
/// credits), any other to the template's, as the game's own key.
fn copy(
    builder: &mut ModBuilder,
    tables: &mut Tables,
    template: &ActorPack,
    name: &str,
    model: &str,
    own: bool,
) -> Result<()> {
    let files = match own {
        true => copied(template, name, model)?,
        false => pointed(template, name, model)?.into(),
    };
    write_clone(builder, template, name, files)?;
    tables.copy_item(name, &template.name)?;
    if own {
        tables.name_itself(name)?;
    }
    tables.show(name, &template.name, model)
}

/// A key's host model and its textures, read once.
struct Host {
    look: &'static Look,
    model: Vec<u8>,
    textures: Vec<(&'static str, Vec<u8>)>,
}

impl Host {
    fn read(romfs: &Romfs, look: &'static Look) -> Result<Self> {
        let textures = look
            .kinds
            .iter()
            .map(|kind| Ok((*kind, romfs.read(&texture_path(look.host, kind))?)))
            .collect::<Result<_>>()?;
        Ok(Self {
            look,
            model: romfs.read(&model_path(look.host))?,
            textures,
        })
    }

    /// Model `n`, its albedo tinted `tint`. Returns its name.
    fn paint(&self, builder: &mut ModBuilder, n: usize, tint: Colour) -> Result<String> {
        let name = self.look.name(n);
        self.write_as(builder, &name, &self.model, tint, false)?;
        Ok(name)
    }

    /// `model`, a copy of the host's, written as `name` with the host's
    /// textures, the albedo tinted `tint`, and with `ring` each with the
    /// ring's plain metal corner.
    fn write_as(
        &self,
        builder: &mut ModBuilder,
        name: &str,
        model: &[u8],
        tint: Colour,
        ring: bool,
    ) -> Result<()> {
        let host = self.look.host;
        builder.add(
            &model_path(name),
            &rename(model, host, name)?,
            &model_path(host),
        )?;
        for (kind, data) in &self.textures {
            let data = texture(&rename(data, host, name)?, kind, tint, ring)?;
            builder.add(&texture_path(name, kind), &data, &texture_path(host, kind))?;
        }
        Ok(())
    }
}

/// A texture of `kind`: an albedo's first level in `colour`, each texel
/// scaled by its brightness, so the metal maps keep the shine and a
/// coloured albedo (the boss key's gold) takes the colour as cleanly as a
/// grey one. With `ring`, any kind with the ring's plain metal corner.
fn texture(data: &[u8], kind: &str, colour: Colour, ring: bool) -> Result<Vec<u8>> {
    let albedo = kind == "Alb";
    if !albedo && !ring {
        return Ok(data.to_vec());
    }
    let mut t = levels(data)?.swap_remove(0);
    if albedo {
        tint(&mut t.rgba, colour);
    }
    if ring {
        rings::paint(&mut t, kind)?;
    }
    let (w, h) = (t.width, t.height);
    redraw(data, &t.rgba, |x, y| !albedo && !rings::painted(w, h, x, y))
}

fn tint(rgba: &mut [u8], colour: Colour) {
    for px in rgba.chunks_mut(4) {
        // Rec. 709 luma, weights out of 256.
        let luma = (px[0] as u32 * 54 + px[1] as u32 * 183 + px[2] as u32 * 19 + 128) / 256;
        for (c, tint) in px.iter_mut().zip(colour) {
            *c = ((luma * tint as u32 + 127) / 255) as u8;
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

/// A copy's files: `template`'s actor showing `model`, as [`pointed`], and
/// its own pouch content naming the copy. Left naming the template, the
/// pouch would hand over the game's key from any holder that resolves it.
fn copied(template: &ActorPack, name: &str, model: &str) -> Result<Vec<(String, BymlDoc, String)>> {
    let [(actor_at, actor, actor_like), info] = pointed(template, name, model)?;
    let pouch_like = path_of(template, POUCH)?;
    let pouch_at = renamed(&pouch_like, name);
    let pouch_doc = BymlDoc::parse(&raw(template, &pouch_like)?)?;
    let mut pouch = resolved(template, &pouch_like)?;
    pouch.insert(POUCH_ACTOR.get().into(), pouch_actor(name));
    let actor = doc(with_ref(actor.root.as_map()?, POUCH, &pouch_at)?, &actor);
    Ok(vec![
        (actor_at, actor, actor_like),
        info,
        (pouch_at, doc(pouch, &pouch_doc), pouch_like),
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

/// Swaps the keys placed in `rel`'s map, a dungeon's, for that dungeon's
/// copies: a chest's content and a floor key, and with `bosses` its boss
/// key. Returns how many.
pub fn swap(rel: &str, root: &mut Byml, bosses: bool) -> Result<usize> {
    let Some(n) = dungeon_of(rel) else {
        return Ok(0);
    };
    let floor = SMALL_LOOK.name(n);
    let mut contents = vec![(SMALL, chest_key(n))];
    if bosses && has_boss(n) {
        contents.push((BOSS, BOSS_LOOK.name(n)));
    }
    let mut swapped = 0;
    for actor in checks::actors_mut(root)? {
        if checks::str_field(actor, GYAML) == Some(SMALL_LOOK.host.as_ref()) {
            actor
                .as_mut_map()?
                .insert(GYAML.into(), floor.as_str().into());
            swapped += 1;
        }
        let Some(dynamic) = actor.as_mut_map()?.get_mut("Dynamic") else {
            continue;
        };
        let held = checks::str_field(dynamic, DROP_ACTOR);
        if let Some((_, copy)) = contents
            .iter()
            .find(|(from, _)| held == Some(from.as_ref()))
        {
            dynamic
                .as_mut_map()?
                .insert((&**DROP_ACTOR).into(), copy.as_str().into());
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
        tint(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [60, 220, 60, 255, 30, 110, 30, 200]);
    }

    #[test]
    fn a_gold_texel_takes_the_colour_by_its_brightness_not_its_hue() {
        let mut rgba = [255, 200, 100, 255];
        tint(&mut rgba, [60, 220, 60]);
        assert_eq!(rgba, [48, 176, 48, 255]);
    }

    #[test]
    fn each_copy_credits_its_dungeon_s_flag_or_counter() {
        if crate::game::no_names() {
            return;
        }
        let bosses = DUNGEONS.iter().filter(|d| d.2.is_some()).count();
        assert_eq!(credits(true, DungeonKeys::Start).len(), bosses);
        assert_eq!(credits(false, DungeonKeys::Rings).len(), DUNGEONS.len());
        assert!(credits(false, DungeonKeys::Vanilla).is_empty());
        let forest = DUNGEONS
            .iter()
            .position(|d| d.0 == flag::FOREST_TEMPLE)
            .unwrap();
        let credits = credits(true, DungeonKeys::Rings);
        let of = |name: String| credits.iter().find(|(n, _)| *n == name).map(|c| c.1);
        // The Forest Temple's, as `probe data` names them.
        assert_eq!(of(boss(forest + 1)), Some(Credit::Bool(0x4273_33a0)));
        assert_eq!(of(ring(forest + 1)), Some(Credit::Int(0x9023_7092, 5)));
        let singles = super::credits(true, DungeonKeys::Singles);
        let one = singles
            .iter()
            .find(|(n, _)| *n == ring(forest + 1))
            .map(|c| c.1);
        assert_eq!(one, Some(Credit::Add(0x9023_7092, 1)));
    }

    #[test]
    fn every_dungeon_with_keys_has_a_colour() {
        // A song's colour comes from its name.
        if crate::game::no_names() {
            return;
        }
        for (dungeon, ..) in DUNGEONS {
            assert!(colour(dungeon).is_some(), "{dungeon}");
        }
    }

    #[test]
    fn the_spelled_out_files_are_each_look_s() {
        if crate::game::no_names() {
            return;
        }
        let looks: Vec<String> = LOOKS.into_iter().flat_map(Look::files).collect();
        assert_eq!(looks, FILES);
    }

    #[test]
    fn copies_are_named_as_long_as_their_host() {
        if crate::game::no_names() {
            return;
        }
        for look in [&SMALL_LOOK, &BOSS_LOOK, &RING_LOOK] {
            assert_eq!(look.name(DUNGEONS.len() + 1).len(), look.host.len());
        }
    }

    #[test]
    fn each_dungeon_s_keys_name_its_copies() {
        let Some(romfs) = dev_dump() else { return };
        let mut per_dungeon = vec![0; DUNGEONS.len() + 1];
        let mut bosses = vec![0; DUNGEONS.len() + 1];
        for rel in checks::banc_files(&romfs).unwrap() {
            let mut doc = checks::load_banc(&romfs, &rel).unwrap();
            let mut plain = doc.root.clone();
            swap(&rel, &mut plain, false).unwrap();
            for actor in checks::actors(&plain).unwrap() {
                let held = actor.as_map().ok().and_then(|m| m.get("Dynamic"));
                let held = held
                    .and_then(|d| checks::str_field(d, DROP_ACTOR))
                    .unwrap_or("");
                assert!(!held.starts_with(BOSS_LOOK.copy), "{rel} without the hook");
            }
            let swapped = swap(&rel, &mut doc.root, true).unwrap();
            let Some(n) = dungeon_of(&rel) else {
                assert_eq!(swapped, 0, "{rel}");
                continue;
            };
            per_dungeon[n] += swapped;
            for actor in checks::actors(&doc.root).unwrap() {
                let gyaml = checks::str_field(actor, GYAML);
                assert_ne!(gyaml, Some(SMALL_LOOK.host.as_ref()), "{rel}");
                let held = actor.as_map().ok().and_then(|m| m.get("Dynamic"));
                let held = held.and_then(|d| checks::str_field(d, DROP_ACTOR));
                for key in [SMALL, BOSS] {
                    assert_ne!(held, Some(key.as_ref()), "{rel}");
                }
                bosses[n] += usize::from(held == Some(BOSS_LOOK.name(n).as_str()));
            }
        }
        for (i, (dungeon, _, boss)) in DUNGEONS.iter().enumerate() {
            assert!(per_dungeon[i + 1] > 0, "no keys swapped in {dungeon}");
            let boss = usize::from(boss.is_some());
            assert_eq!(bosses[i + 1], boss, "{dungeon}'s boss key");
        }
    }

    #[test]
    fn a_copy_s_pack_built_in_memory_is_the_one_the_seed_writes() {
        let Some(romfs) = dev_dump() else { return };
        for keys in [DungeonKeys::Rings, DungeonKeys::Singles] {
            let dir = std::env::temp_dir()
                .join(format!("zootr-copy-pack-{}-{keys:?}", std::process::id()));
            let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
            let mut tables = Tables::load(&romfs).unwrap();
            write(&romfs, &mut builder, &mut tables, true, keys).unwrap();
            builder.finish().unwrap();
            for copy in [ring(2), boss(2)] {
                let rel = pack_file(&copy);
                let raw = std::fs::read(dir.join("romfs").join(&rel)).unwrap();
                let bytes = romfs.dicts().decompress(&rel, &raw).unwrap();
                let written = ActorPack::from_bytes(&copy, bytes.to_vec()).unwrap();
                let built = copy_pack(&romfs, &copy, keys).unwrap().unwrap();
                assert_eq!(built.files(), written.files(), "{copy} {keys:?}");
                for f in built.files() {
                    assert_eq!(built.raw(&f), written.raw(&f), "{copy} {keys:?}: {f}");
                }
            }
            let hookshot = copy_pack(&romfs, item::HOOKSHOT, keys).unwrap();
            assert!(hookshot.is_none());
        }
    }

    #[test]
    fn each_copy_shows_its_own_model_from_its_own_file_and_a_ring_its_keys() {
        let Some(romfs) = dev_dump() else { return };
        shows_own_models(&romfs, DungeonKeys::Rings);
        shows_own_models(&romfs, DungeonKeys::Singles);
    }

    fn shows_own_models(romfs: &Romfs, keys: DungeonKeys) {
        let dir =
            std::env::temp_dir().join(format!("zootr-key-tints-{}-{keys:?}", std::process::id()));
        let mut builder = ModBuilder::new(romfs, &dir).unwrap();
        let mut tables = Tables::load(romfs).unwrap();
        write(romfs, &mut builder, &mut tables, true, keys).unwrap();
        builder.finish().unwrap();
        let read = |rel: &str| {
            let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
            romfs.dicts().decompress(rel, &raw).unwrap()
        };
        let info = |name: &str| path_of(&ActorPack::load(romfs, name).unwrap(), MODEL).unwrap();
        let has = |bytes: &[u8], s: &str| bytes.windows(s.len()).any(|w| w == s.as_bytes());
        let fmdb = |name: &str| {
            let row = crate::standins::row(&tables.actor, name).unwrap();
            let fmdb = row
                .get(field::FMDB_NAME.get())
                .and_then(|v| v.as_string().ok());
            fmdb.map(|s| s.to_string())
        };
        let key_vertices = |model: &str| {
            let shape = rings::SHAPE.replace(SMALL_LOOK.host.as_ref(), model);
            let lods = ounce_formats::lods(&read(&model_path(model)), &shape).unwrap();
            lods[0].vertices.len()
        };
        let one = key_vertices(&SMALL_LOOK.name(1));
        for (i, (_, count, _)) in DUNGEONS.iter().enumerate() {
            let n = i + 1;
            let small = SMALL_LOOK.name(n);
            let mut copies = vec![
                (chest_key(n), SMALL, small.clone()),
                (small.clone(), SMALL, small),
            ];
            if has_boss(n) {
                let boss = BOSS_LOOK.name(n);
                copies.push((boss.clone(), BOSS, boss));
            }
            let shown = match keys {
                DungeonKeys::Rings => RING_LOOK.name(n),
                _ => SMALL_LOOK.name(n),
            };
            let rings = *count as usize * usize::from(keys == DungeonKeys::Rings);
            assert!(key_vertices(&shown) >= one * rings.max(1), "{shown}");
            if rings > 0 {
                // The hoop and its Triforces weigh less than two keys.
                assert!(key_vertices(&shown) < one * (rings + 2), "{shown}");
                for kind in SMALL_LOOK.kinds {
                    assert!(
                        !read(&texture_path(&shown, kind)).is_empty(),
                        "{shown} {kind}"
                    );
                }
            }
            copies.push((ring(n), SMALL, shown));
            let credited = made(true, true);
            for (copy, template, _) in &copies {
                let own = credited.iter().any(|(c, _)| c == copy);
                let pouch = crate::standins::row(&tables.pouch, copy).unwrap();
                let deal = pouch.get(field::POUCH_SPECIAL_DEAL.get()).unwrap();
                let actor = crate::standins::row(&tables.actor, copy).unwrap();
                let called = actor.get(flag::ACTOR_NAME.get()).unwrap();
                let called_is = if own { copy.as_str() } else { template.get() };
                assert_eq!(called.as_string().unwrap(), called_is, "{copy}");
                if own {
                    assert_eq!(deal.as_string().unwrap(), PLAIN_DEAL, "{copy}");
                }
            }
            for (copy, template, model) in copies {
                assert!(!read(&model_path(&model)).is_empty(), "{model}");
                assert_eq!(fmdb(&copy), Some(model), "{copy}");
                crate::standins::row(&tables.pouch, &copy).unwrap();
                let pack = read(&format!("Pack/Actor/{copy}.pack.zs"));
                let own = renamed(&info(template), &copy);
                assert!(has(&pack, &own), "{copy} has no ModelInfo of its own");
            }
        }
        let floor_info = info(SMALL_LOOK.host);
        for k in 1..=FORTRESS.1 {
            let key = format!("{FORTRESS_KEY}{k:02}");
            assert_eq!(fmdb(&key), Some(SMALL_LOOK.name(FORTRESS_MODEL)));
            let pack = read(&format!("Pack/Actor/{key}.pack.zs"));
            assert!(has(&pack, &renamed(&floor_info, &key)), "{key}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
