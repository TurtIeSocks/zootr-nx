//! Chest Looks Match Contents: a chest's look says what it holds. A look
//! is a copy of the chest actor, its class part `A` made the look's letter
//! ([`crate::marks::lettered`]), showing a copy of the chest's model whose
//! albedo is painted from the dump each seed: the metal (where the model's
//! metal map says) gold or a key's dungeon colour, or the wood stained.
//! Every seed writes the same copies, and only chests whose contents have
//! a look name one.

mod files;

pub use files::FILES;

use std::borrow::Cow;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, Texture};
use ounce_formats::{materials, redraw, rename, top_level};

use crate::checks::{self, DROP_ACTOR, DYNAMIC, GYAML};
use crate::copies::pointed_at;
use crate::foreign::Owner;
use crate::key_tints::{self, KEY_DUNGEONS};
use crate::keys;
use crate::known::{Name, actor, field, item, texture};
use crate::marks::paint::{Canvas, GOLD};
use crate::marks::{lettered, texture_file};
use crate::medallions::{Colour, model_file};
use crate::rsdb::Tags;
use crate::settings::NewGame;
use crate::standins::{Tables, write_clone};

/// A chest's look, by what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// An item the seed needs: the metal gold.
    Gold,
    /// A key of key dungeon `n`, counting from 1 as
    /// [`key_tints::KEY_DUNGEONS`] does: the metal its colour.
    Key(usize),
    /// A Gold Skulltula token: the wood stained near black.
    Skull,
    /// A Piece of Heart or Heart Container: the wood tinted rose.
    Heart,
}

/// Each key dungeon's letter, in [`KEY_DUNGEONS`] order.
const KEY_LETTERS: [char; KEY_DUNGEONS] = ['B', 'C', 'D', 'E', 'F', 'J', 'K', 'L', 'M'];

/// The skull look's stain and the heart look's tint, lifted by each
/// texel's brightness so the grain stays.
const SKULL_STAIN: Colour = [48, 40, 36];
const HEART_TINT: Colour = [235, 120, 150];

/// What the heart look holds.
const HEARTS: [&Name; 2] = [actor::OBJ_HEART_PIECE, actor::OBJ_HEART_CONTAINER];

/// The part of a chest a look paints.
#[derive(Clone, Copy)]
enum Part {
    Metal,
    Wood,
}

impl Look {
    fn letter(self) -> char {
        match self {
            Look::Gold => 'G',
            Look::Skull => 'S',
            Look::Heart => 'H',
            Look::Key(n) => KEY_LETTERS[n - 1],
        }
    }

    fn all() -> impl Iterator<Item = Look> {
        let keys = (1..=KEY_DUNGEONS).map(Look::Key);
        [Look::Gold, Look::Skull, Look::Heart]
            .into_iter()
            .chain(keys)
    }

    /// What it paints, and in what colour.
    fn paint(self) -> Result<(Part, Colour)> {
        Ok(match self {
            Look::Gold => (Part::Metal, GOLD),
            Look::Key(n) => {
                let colour = key_tints::key_colour(n);
                (
                    Part::Metal,
                    colour.with_context(|| format!("no colour for keys {n}"))?,
                )
            }
            Look::Skull => (Part::Wood, SKULL_STAIN),
            Look::Heart => (Part::Wood, HEART_TINT),
        })
    }
}

/// A chest model a look copies: its mesh, its albedo, and the metal map
/// that says which of the albedo is metal.
struct Model {
    fmdb: &'static Name,
    albedo: &'static Name,
    metal: &'static Name,
}

/// Both chest models' project.
const PROJECT: &Name = actor::OBJ_T_BOX_A;

/// The small chest's model and the big chest's. The big chest's colour
/// animation swaps in a second albedo (`.01`) at frame 1, which only the
/// Treasure Chest Game's chests play, so a look paints the first.
const MODELS: [Model; 2] = [
    Model {
        fmdb: actor::OBJ_T_BOX_A_01,
        albedo: texture::MT_OBJ_T_BOX_A_01_ALB,
        metal: texture::MT_OBJ_T_BOX_A_01_MTL,
    },
    Model {
        fmdb: actor::OBJ_T_BOX_A_02,
        albedo: texture::MT_OBJ_T_BOX_A_02_ALB_00,
        metal: texture::MT_OBJ_T_BOX_A_02_MTL,
    },
];

/// Each chest that takes a look, and its model in [`MODELS`]. The boss key
/// chest has a model of its own, and keeps it.
const HOSTS: [(&Name, usize); 4] = [
    (actor::OBJ_T_BOX_A_01, 0),
    (actor::OBJ_T_BOX_A_01_NO_BAKE, 0),
    (actor::OBJ_T_BOX_A_02, 1),
    (actor::OBJ_T_BOX_A_02_NO_BAKE, 1),
];

/// `name`'s copy for `look`.
fn copy(name: &str, look: Look) -> Result<String> {
    lettered(name, look.letter()).with_context(|| format!("{name} has no class part"))
}

/// The chest a look's copy `name` copies, or `name` itself when it isn't
/// one. Whatever reads a chest's size or kind reads it through this.
pub fn host(name: &str) -> Cow<'_, str> {
    let copied = Look::all().find_map(|look| {
        let copies = |h: &&&Name| lettered(h, look.letter()).as_deref() == Some(name);
        HOSTS.iter().map(|(h, _)| h).find(copies)
    });
    match copied {
        Some(h) => Cow::Owned(h.to_string()),
        None => Cow::Borrowed(name),
    }
}

/// The look of a chest holding `item` in `new_game`'s seed, in map `rel`,
/// with looks on. A trap takes the look of what it passes for, and another
/// player's item is gold when Archipelago classes it progression.
pub fn of(new_game: &NewGame, item: &str, rel: &str) -> Option<Look> {
    if let Some(copy) = new_game.foreign.iter().find(|f| f.actor == item) {
        return match &copy.owner {
            Owner::Trap { .. } => of(new_game, &copy.shows, rel),
            Owner::Player { progression, .. } => progression.then_some(Look::Gold),
        };
    }
    if let Some(dungeon) = key_tints::key_dungeon(item) {
        return Some(Look::Key(dungeon));
    }
    if keys::is_boss(item) || keys::is_small(item) {
        return keys::dungeon_of(rel).map(Look::Key);
    }
    if item == item::TOKEN_GOLD {
        Some(Look::Skull)
    } else if HEARTS.iter().any(|h| *h == item) {
        Some(Look::Heart)
    } else {
        new_game.major.contains(item).then_some(Look::Gold)
    }
}

/// Points each chest in map `rel` that takes a look at its look's copy, by
/// what it drops in `new_game`'s seed. Returns how many changed.
pub fn swap(rel: &str, root: &mut Byml, new_game: &NewGame) -> Result<usize> {
    if !new_game.chest_looks {
        return Ok(0);
    }
    let mut swapped = 0;
    for actor in checks::actors_mut(root)? {
        let Some(gyaml) = checks::str_field(actor, GYAML).map(str::to_owned) else {
            continue;
        };
        if !HOSTS.iter().any(|(h, _)| gyaml == *h) {
            continue;
        }
        let dynamic = actor.as_map()?.get(DYNAMIC);
        let drop = dynamic.and_then(|d| checks::str_field(d, DROP_ACTOR));
        let Some(look) = drop.and_then(|d| of(new_game, d, rel)) else {
            continue;
        };
        let copy = copy(&gyaml, look)?;
        actor.as_mut_map()?.insert(GYAML.into(), copy.into());
        swapped += 1;
    }
    Ok(swapped)
}

/// Writes every look's model, its painted albedo, and each host's copy for
/// it with the host's rows and tags.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    tags: &mut Tags,
) -> Result<()> {
    for model in &MODELS {
        let like = model_file(PROJECT, model.fmdb);
        let data = romfs.read(&like)?;
        let albedo = romfs.read(&texture_file(model.albedo))?;
        let metal = top_level(&romfs.read(&texture_file(model.metal))?)?;
        for look in Look::all() {
            write_albedo(builder, &albedo, model.albedo, &metal, look)?;
            let file = model_file(&copy(PROJECT, look)?, &copy(model.fmdb, look)?);
            builder.add(&file, &copy_model(&data, model, look)?, &like)?;
        }
    }
    for (host, model) in HOSTS {
        let template = ActorPack::load(romfs, host)?;
        for look in Look::all() {
            let name = copy(host, look)?;
            let project = copy(PROJECT, look)?;
            let fmdb = copy(MODELS[model].fmdb, look)?;
            let [actor, (at, info, like)] = pointed_at(&template, &name, &project, &fmdb)?;
            write_clone(builder, &template, &name, [actor, (at, still(info)?, like)])?;
            tables.copy(&name, host)?;
            tables.point_at(&name, &project, &fmdb)?;
            tags.copy(&name, host)?;
        }
    }
    Ok(())
}

/// ModelInfo `info` without its colour animation, which would swap the
/// host's albedo back in by name.
fn still(mut info: BymlDoc) -> Result<BymlDoc> {
    let root = info.root.as_mut_map()?;
    root.remove(field::MODEL_VARIATION_ANIMS.get());
    Ok(info)
}

/// `data`, albedo `albedo`'s file, painted for `look` where `metal` says,
/// written under its copy's name.
fn write_albedo(
    builder: &mut ModBuilder,
    data: &[u8],
    albedo: &Name,
    metal: &Texture,
    look: Look,
) -> Result<()> {
    let name = copy(albedo, look)?;
    let mut canvas = Canvas::new(top_level(data)?);
    paint(&mut canvas, metal, look)?;
    let painted = redraw(
        &rename(data, albedo, &name)?,
        &canvas.texture.rgba,
        |x, y| !canvas.is_painted(x, y),
    )?;
    builder.add(&texture_file(&name), &painted, &texture_file(albedo))
}

/// `canvas`, a chest's albedo, painted for `look`: its metal or its wood,
/// as `metal`, the chest's metal map, divides them.
fn paint(canvas: &mut Canvas, metal: &Texture, look: Look) -> Result<()> {
    let (part, colour) = look.paint()?;
    let full = metal_peak(metal);
    canvas.wash(colour, |u, v| {
        let share = sample_red(metal, u, v) / full;
        match part {
            Part::Metal => share,
            Part::Wood => 1.0 - share,
        }
    });
    Ok(())
}

/// The metal map's reddest texel: what counts as all metal.
fn metal_peak(metal: &Texture) -> f32 {
    let peak = metal.rgba.chunks(4).map(|px| px[0]).max().unwrap_or(0);
    f32::from(peak.max(1))
}

/// `texture`'s red at (`u`, `v`), fractions of it, bilinear between texels.
fn sample_red(texture: &Texture, u: f32, v: f32) -> f32 {
    let (w, h) = (texture.width, texture.height);
    let (fx, fy) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
    let at = |x: f32, y: f32| {
        let x = (x.max(0.0) as usize).min(w - 1);
        let y = (y.max(0.0) as usize).min(h - 1);
        f32::from(texture.rgba[4 * (y * w + x)])
    };
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1.0, y0) * tx;
    let bottom = at(x0, y0 + 1.0) * (1.0 - tx) + at(x0 + 1.0, y0 + 1.0) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// `data`, `model`'s file, renamed as its copy for `look`: its name
/// (`PROJECT.FMDB`), its mesh and its albedo. Renaming the mesh renames
/// every texture named after it, so each one but the albedo gets its
/// host's name back.
fn copy_model(data: &[u8], model: &Model, look: Look) -> Result<Vec<u8>> {
    let whole = |project: &str, fmdb: &str| format!("{project}.{fmdb}");
    let fmdb = copy(model.fmdb, look)?;
    let copied = rename(
        data,
        &whole(PROJECT, model.fmdb),
        &whole(&copy(PROJECT, look)?, &fmdb),
    )?;
    let copied = rename(&copied, model.fmdb, &fmdb)?;
    let textures = materials(data)?.into_iter().flat_map(|m| m.textures);
    let kept = textures.filter(|t| t != model.albedo.get());
    kept.fold(Ok(copied), |copied, t| {
        rename(&copied?, &copy(&t, look)?, &t)
    })
}

/// Each painted albedo written to `out` as PNG under its copy's name, for a
/// look before a seed: `probe chests`.
pub fn preview(romfs: &Romfs, out: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    for model in &MODELS {
        let metal = top_level(&romfs.read(&texture_file(model.metal))?)?;
        let host = top_level(&romfs.read(&texture_file(model.albedo))?)?;
        for look in Look::all() {
            let mut canvas = Canvas::new(host.clone());
            paint(&mut canvas, &metal, look)?;
            let path = out.join(format!("{}.png", copy(model.albedo, look)?));
            std::fs::write(&path, crate::sprites::png(&canvas.texture)?)?;
            println!("{}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foreign::Foreign;
    use crate::known::flag;

    const FOREST: &str = "Banc/ForestTemple/FakeRoom.bcett.byml.zs";

    fn seed() -> NewGame {
        let trap = |shows: &Name| Foreign {
            actor: format!("FakeTrap_{}", shows.get()),
            shows: shows.to_string(),
            model: None,
            owner: Owner::Trap {
                label: "Fake".into(),
            },
        };
        let player = |progression| Foreign {
            actor: format!("FakeCopy_{progression}"),
            shows: String::new(),
            model: None,
            owner: Owner::Player {
                item: "Fake Item".into(),
                player: "FakePlayer".into(),
                progression,
            },
        };
        NewGame {
            chest_looks: true,
            major: [item::HOOKSHOT.to_string()].into(),
            foreign: vec![
                trap(item::HOOKSHOT),
                trap(item::TOKEN_GOLD),
                player(true),
                player(false),
            ],
            ..NewGame::default()
        }
    }

    #[test]
    fn a_copy_names_its_host_back() {
        for (h, _) in HOSTS {
            for look in Look::all() {
                let copy = copy(h, look).unwrap();
                assert_eq!(copy.len(), h.len(), "{copy}");
                assert_eq!(host(&copy), h.get(), "{copy}");
            }
            assert_eq!(host(h), h.get(), "a host is its own");
        }
        assert_eq!(host(actor::OBJ_T_BOX_A_03), actor::OBJ_T_BOX_A_03.get());
    }

    #[test]
    fn a_copy_reads_its_host_s_size() {
        let big = copy(actor::OBJ_T_BOX_A_02_NO_BAKE, Look::Gold).unwrap();
        assert!(checks::is_big(&big));
        let small = copy(actor::OBJ_T_BOX_A_01, Look::Skull).unwrap();
        assert_eq!(checks::size_of(&small), Some(checks::Size::Small));
    }

    #[test]
    fn every_look_has_a_letter_of_its_own() {
        let letters: Vec<char> = Look::all().map(Look::letter).collect();
        let mut unique = letters.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), letters.len(), "{letters:?}");
        assert!(
            !letters.contains(&'A') && !letters.contains(&'Z'),
            "the host's and the marks'"
        );
    }

    #[test]
    fn each_class_of_item_takes_its_look() {
        let seed = seed();
        let look = |item: &str| of(&seed, item, FOREST);
        assert_eq!(look(item::HOOKSHOT), Some(Look::Gold));
        assert_eq!(look(item::TOKEN_GOLD), Some(Look::Skull));
        assert_eq!(look(actor::OBJ_HEART_PIECE), Some(Look::Heart));
        assert_eq!(look(actor::T_BOX_RUPEE_RED), None, "junk stays vanilla");
        let forest = keys::dungeon_in(flag::FOREST_TEMPLE).unwrap();
        assert_eq!(
            look(keys::SMALL),
            Some(Look::Key(forest)),
            "its map's dungeon"
        );
        assert_eq!(
            look(&key_tints::ring(2)),
            Some(Look::Key(2)),
            "its own dungeon"
        );
        let tinted = key_tints::chest_key(3);
        assert_eq!(look(&tinted), Some(Look::Key(3)), "a vanilla key, tinted");
        let fortress = format!("{}01", keys::FORTRESS_KEY);
        assert_eq!(look(&fortress), Some(Look::Key(KEY_DUNGEONS)));
    }

    #[test]
    fn a_trap_or_another_player_s_item_looks_like_what_it_passes_for() {
        let seed = seed();
        let look = |item: &str| of(&seed, item, FOREST);
        let hook = format!("FakeTrap_{}", item::HOOKSHOT.get());
        assert_eq!(look(&hook), Some(Look::Gold));
        let token = format!("FakeTrap_{}", item::TOKEN_GOLD.get());
        assert_eq!(look(&token), Some(Look::Skull));
        assert_eq!(look("FakeCopy_true"), Some(Look::Gold));
        assert_eq!(look("FakeCopy_false"), None);
    }

    #[test]
    fn a_look_paints_its_part_and_leaves_the_other() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for model in &MODELS {
            let metal = top_level(&romfs.read(&texture_file(model.metal)).unwrap()).unwrap();
            let host = top_level(&romfs.read(&texture_file(model.albedo)).unwrap()).unwrap();
            let (w, h) = (host.width, host.height);
            // The metal's at the top of both sheets, the wood at the bottom.
            let (top, bottom) = ((w / 2, h / 20), (w / 2, h - h / 20));
            for (look, painted, left) in [(Look::Gold, top, bottom), (Look::Heart, bottom, top)] {
                let mut canvas = Canvas::new(host.clone());
                paint(&mut canvas, &metal, look).unwrap();
                assert!(canvas.is_painted(painted.0, painted.1), "{look:?}");
                assert!(!canvas.is_painted(left.0, left.1), "{look:?}");
            }
        }
    }

    #[test]
    fn a_chest_takes_its_look_only_with_looks_on() {
        let chest = |gyaml: &Name| {
            let dynamic = Byml::from_iter([(DROP_ACTOR.get(), item::HOOKSHOT.to_string().into())]);
            Byml::from_iter([(GYAML, gyaml.to_string().into()), (DYNAMIC, dynamic)])
        };
        let placed = || {
            let actors = [actor::OBJ_T_BOX_A_02_NO_BAKE, actor::OBJ_T_BOX_A_03].map(chest);
            Byml::from_iter([("Actors", Byml::Array(actors.into()))])
        };
        let gyamls = |root: &Byml| -> Vec<String> {
            let actors = root.as_map().unwrap()["Actors"].as_array().unwrap();
            actors
                .iter()
                .map(|a| checks::str_field(a, GYAML).unwrap().to_owned())
                .collect()
        };
        let mut root = placed();
        let off = NewGame {
            chest_looks: false,
            ..seed()
        };
        assert_eq!(swap(FOREST, &mut root, &off).unwrap(), 0);
        assert_eq!(swap(FOREST, &mut root, &seed()).unwrap(), 1);
        let gold = copy(actor::OBJ_T_BOX_A_02_NO_BAKE, Look::Gold).unwrap();
        assert_eq!(gyamls(&root), [gold, actor::OBJ_T_BOX_A_03.to_string()]);
    }
}
