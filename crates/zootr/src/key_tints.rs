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

pub(crate) mod paint;
mod rings;

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, murmur3, pack_file};
use zootr_hook::Credit;

use crate::checks::{self, DROP_ACTOR, GYAML};
use crate::keys::{
    BOSS, DUNGEONS, FORTRESS, FORTRESS_KEY, SMALL, boss_flag, dungeon_of, has_boss, small_flag,
};
use crate::known::{Name, actor, field, flag, flow, item};
use crate::medallions::{self, Colour};
use crate::settings::DungeonKeys;
use crate::standins::{
    MODEL, POUCH, POUCH_ACTOR, Tables, actor_path, clone_pack, doc, path_of, pouch_actor, raw,
    renamed, resolved, with_ref, write_clone,
};
use crate::text::{Copies, CopyLine, Named, Says};
use paint::{Host, Look};

/// The small key's, named like the floor key that shows it.
const SMALL_LOOK: Look = Look {
    host: actor::OBJ_DUNGEON_SMALL_KEY,
    material: None,
    kinds: &["Alb", "AO", "Mtl", "Nrm", "Rgh"],
    copy: "Obj_Rando_SmallKey",
};
/// The boss key's. Its gem glows from its own emissive map, left red so a
/// boss key still reads as one.
const BOSS_LOOK: Look = Look {
    host: BOSS,
    material: None,
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

/// How every item actor the seed makes begins, the dump's prefix and ours.
pub(crate) const MADE: &str = "Item_Rando_";

/// Dungeon `n`'s small key as a chest's content. On the floor it's the
/// model's own name.
fn chest_key(n: usize) -> String {
    format!("{MADE}SmallKey{n:02}")
}

/// Dungeon `n`'s key ring: its small key as a chest's content, but one
/// the game neither stores nor counts. The hook sets the counter.
pub fn ring(n: usize) -> String {
    format!("{MADE}KeyRing{n:02}")
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
    let Some((template, model)) = copy_of(item, keys) else {
        return Ok(None);
    };
    let template = ActorPack::load(romfs, template)?;
    let files = copied(&template, item, &model)?;
    clone_pack(&template, item, files).map(Some)
}

/// The label naming `item`, if it's a ring or boss key copy. Its pack
/// lives only in the seed, and it answers to its own name, whose lines
/// [`captions`] writes.
pub fn name_label(item: &str) -> Option<String> {
    copy_of(item, DungeonKeys::default()).map(|_| format!("{item}{}", TEXT_PARTS[0]))
}

/// The key `item` copies and the model it shows as `keys` deals it.
fn copy_of(item: &str, keys: DungeonKeys) -> Option<(&'static Name, String)> {
    (1..=DUNGEONS.len()).find_map(|n| {
        if item == ring(n) {
            Some((SMALL, ring_model(n, keys)))
        } else if has_boss(n) && item == boss(n) {
            Some((BOSS, boss(n)))
        } else {
            None
        }
    })
}

/// The text each copy's get shows, copied from its template's and naming
/// its dungeon: a copy answers to its own name, so the game looks its
/// lines up under it.
pub fn captions(keys: DungeonKeys, bosses: bool) -> Copies<'static> {
    let new = made(keys.copies(), bosses)
        .into_iter()
        .flat_map(|(copy, template, n)| {
            let lines = match template == BOSS {
                true => &BOSS_TEXT,
                false if keys == DungeonKeys::Singles => &SINGLE_TEXT,
                false => &RING_TEXT,
            };
            TEXT_PARTS
                .iter()
                .zip(lines)
                .map(move |(part, &(english, bracketed))| {
                    let says = english.map_or(Says::Source, |english| {
                        Says::Named(Named {
                            place: place(n),
                            english,
                            bracketed,
                        })
                    });
                    CopyLine {
                        label: format!("{copy}{part}"),
                        sources: vec![format!("{}{part}", template.get())],
                        says,
                    }
                })
        })
        .collect();
    Copies {
        file: POUCH_TEXT,
        new,
    }
}

/// Dungeon `n`'s label in `LocationMsg/Location`. Ganon's Castle reads
/// "Inside Ganondorf's Castle" under its own, so it takes the castle's.
fn place(n: usize) -> &'static str {
    match DUNGEONS[n - 1].0 {
        d if d == flag::INSIDE_GANON_CASTLE => flag::GANON_CASTLE.get(),
        d => d.get(),
    }
}

/// Each ring (with `rings`) and boss key copy (with `bosses`) the seed
/// makes, with the game's key it copies and its dungeon's number in
/// [`DUNGEONS`].
pub fn made(rings: bool, bosses: bool) -> Vec<(String, &'static Name, usize)> {
    let mut made = Vec::new();
    for n in 1..=DUNGEONS.len() {
        if rings {
            made.push((ring(n), SMALL, n));
        }
        if bosses && has_boss(n) {
            made.push((boss(n), BOSS, n));
        }
    }
    made
}

/// The lines a key's get reads, each under the key's name.
pub(crate) const TEXT_PARTS: [&str; 3] = ["_Name", "_GetMessage", "_GetCaption"];

/// Each of [`TEXT_PARTS`]' English line for a copy, `{}` its dungeon's
/// name, and whether every other language adds the name in brackets.
/// `None` keeps the game's line in every language.
type CopyText = [(Option<&'static str>, bool); 3];
const RING_TEXT: CopyText = [
    (Some("{} Key Ring"), true),
    (Some("You found {the} {} key ring!"), true),
    (Some("It holds every {} small key."), false),
];
const SINGLE_TEXT: CopyText = [
    (Some("{} Small Key"), true),
    (Some("You found {a} {} small key!"), true),
    (Some("It unlocks one {} door."), false),
];
const BOSS_TEXT: CopyText = [
    (Some("{} Boss Key"), true),
    (Some("You found {the} {} boss key!"), true),
    (None, false),
];
pub(crate) const POUCH_TEXT: &str = "ActorMsg/PouchContent";

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
    let copies: Vec<String> = made(rings, bosses).into_iter().map(|(c, ..)| c).collect();
    let changed = tables.plain_deal(&copies)?;
    ensure!(
        changed == copies.len(),
        "{changed} copy rows of {}",
        copies.len()
    );
    Ok(())
}

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

/// `name`'s ActorParam and ModelInfo as `template`'s, the ModelInfo a file
/// of its own naming `model`, since packs sharing a file share whichever
/// loaded first. Each is `(path, doc, the template file it copies)`.
pub(crate) fn pointed(
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
    fn a_copy_s_text_names_its_dungeon_in_english_and_in_brackets_elsewhere() {
        use ounce_formats::{Msbt, Part};
        let Some(romfs) = dev_dump() else { return };
        let dungeon = |d: &Name| DUNGEONS.iter().position(|e| e.0 == d).unwrap() + 1;
        let (forest, ganon) = (
            dungeon(flag::FOREST_TEMPLE),
            dungeon(flag::INSIDE_GANON_CASTLE),
        );
        let text = |s: &str| vec![Part::Text(s.into())];
        let (rings, singles) = (
            captions(DungeonKeys::Rings, true),
            captions(DungeonKeys::Singles, false),
        );
        let rels = crate::text::archives(&romfs).unwrap();
        assert_eq!(rels.len(), 16);
        for rel in rels {
            let data = romfs.read(&rel).unwrap();
            let read = |d: &[u8], file: &str| {
                let sarc = roead::sarc::Sarc::new(d).unwrap();
                Msbt::parse(sarc.get_data(file).unwrap()).unwrap()
            };
            let pouch = format!("{POUCH_TEXT}.msbt");
            let (old, places) = (read(&data, &pouch), read(&data, crate::names::AREAS));
            let line = |m: &Msbt, label: String| m.get(&label).unwrap().unwrap();
            let ring_text = read(&crate::text::copied(&data, &rel, &rings).unwrap(), &pouch);
            let single_text = read(&crate::text::copied(&data, &rel, &singles).unwrap(), &pouch);
            let (small, boss_key) = (SMALL.get(), BOSS.get());
            // Every language keeps the game's own captions but a ring's
            // and a single key's in English.
            assert_eq!(
                line(&ring_text, format!("{}_GetCaption", boss(forest))),
                line(&old, format!("{boss_key}_GetCaption")),
                "{rel}"
            );
            if crate::text::is_english(&rel) {
                let cases = [
                    (
                        &ring_text,
                        ring(forest),
                        "_GetMessage",
                        "You found the Forest Temple key ring!",
                    ),
                    (&ring_text, ring(forest), "_Name", "Forest Temple Key Ring"),
                    (
                        &ring_text,
                        ring(forest),
                        "_GetCaption",
                        "It holds every Forest Temple small key.",
                    ),
                    (
                        &ring_text,
                        ring(ganon),
                        "_Name",
                        "Ganondorf's Castle Key Ring",
                    ),
                    (
                        &ring_text,
                        ring(ganon),
                        "_GetMessage",
                        "You found Ganondorf's Castle key ring!",
                    ),
                    (
                        &ring_text,
                        boss(ganon),
                        "_GetMessage",
                        "You found Ganondorf's Castle boss key!",
                    ),
                    (
                        &single_text,
                        ring(ganon),
                        "_GetMessage",
                        "You found Ganondorf's Castle small key!",
                    ),
                    (
                        &ring_text,
                        boss(forest),
                        "_GetMessage",
                        "You found the Forest Temple boss key!",
                    ),
                    (
                        &ring_text,
                        boss(ganon),
                        "_Name",
                        "Ganondorf's Castle Boss Key",
                    ),
                    (
                        &single_text,
                        ring(forest),
                        "_GetMessage",
                        "You found a Forest Temple small key!",
                    ),
                    (
                        &single_text,
                        ring(forest),
                        "_GetCaption",
                        "It unlocks one Forest Temple door.",
                    ),
                ];
                for (m, copy, part, want) in cases {
                    assert_eq!(
                        line(m, format!("{copy}{part}")),
                        text(want),
                        "{rel} {copy}{part}"
                    );
                }
                continue;
            }
            let wide = rel.contains("zh.") || rel.contains("ja.");
            let (open, close) = if wide { ("（", "）") } else { (" (", ")") };
            let bracketed = |source: String, place: &Name| {
                let mut want = line(&old, source);
                want.push(Part::Text(open.into()));
                want.extend(line(&places, place.get().into()));
                want.push(Part::Text(close.into()));
                // Adjacent text runs read back as one.
                want.into_iter()
                    .fold(Vec::new(), |mut out: Vec<Part>, part| {
                        match (out.last_mut(), part) {
                            (Some(Part::Text(last)), Part::Text(t)) => last.push_str(&t),
                            (_, part) => out.push(part),
                        }
                        out
                    })
            };
            let cases = [
                (
                    &ring_text,
                    ring(forest),
                    "_GetMessage",
                    small,
                    flag::FOREST_TEMPLE,
                ),
                (
                    &ring_text,
                    ring(forest),
                    "_Name",
                    small,
                    flag::FOREST_TEMPLE,
                ),
                (&ring_text, ring(ganon), "_Name", small, flag::GANON_CASTLE),
                (
                    &ring_text,
                    boss(forest),
                    "_GetMessage",
                    boss_key,
                    flag::FOREST_TEMPLE,
                ),
                (
                    &single_text,
                    ring(forest),
                    "_GetMessage",
                    small,
                    flag::FOREST_TEMPLE,
                ),
            ];
            for (m, copy, part, template, place) in cases {
                let want = bracketed(format!("{template}{part}"), place);
                assert_eq!(line(m, format!("{copy}{part}")), want, "{rel} {copy}{part}");
            }
            assert_eq!(
                line(&ring_text, format!("{}_GetCaption", ring(forest))),
                line(&old, format!("{small}_GetCaption")),
                "{rel}"
            );
        }
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
            let lods = ounce_formats::lods(&read(&medallions::model_path(model)), &shape).unwrap();
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
                // The hoop, with any Triforces, weighs less than two keys.
                assert!(key_vertices(&shown) < one * (rings + 2), "{shown}");
                for kind in SMALL_LOOK.kinds {
                    assert!(
                        !read(&medallions::texture_path(&shown, kind)).is_empty(),
                        "{shown} {kind}"
                    );
                }
            }
            copies.push((ring(n), SMALL, shown));
            let credited = made(true, true);
            for (copy, template, _) in &copies {
                let own = credited.iter().any(|(c, ..)| c == copy);
                let pouch = crate::standins::row(&tables.pouch, copy).unwrap();
                let deal = pouch.get(field::POUCH_SPECIAL_DEAL.get()).unwrap();
                let actor = crate::standins::row(&tables.actor, copy).unwrap();
                let called = actor.get(flag::ACTOR_NAME.get()).unwrap();
                let called_is = if own { copy.as_str() } else { template.get() };
                assert_eq!(called.as_string().unwrap(), called_is, "{copy}");
                if own {
                    assert_eq!(
                        deal.as_string().unwrap(),
                        crate::standins::PLAIN_DEAL,
                        "{copy}"
                    );
                }
            }
            for (copy, template, model) in copies {
                assert!(!read(&medallions::model_path(&model)).is_empty(), "{model}");
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
