//! Breakables marked while they hold a check. A dealt breakable stands as
//! a marked copy of its actor until Link picks up its item, then as its
//! vanilla twin (`picked`). The copy shows a copy of its model whose
//! albedo is painted gold ([`paint`]), made from the dump each seed, and
//! keeps its host's rows, its tags among them, so it lifts and breaks the
//! same. Every seed writes the same copies, and only dealt holders name
//! them, so a kind the shuffle leaves out stays vanilla.

mod files;
pub(crate) mod paint;
mod shrubs;

pub use files::FILES;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, redraw, rename, top_level};

use crate::checks::{self, GYAML};
use crate::copies::pointed_at;
use crate::known::{Name, actor, field, texture};
use crate::medallions::model_file;
use crate::rsdb::Tags;
use crate::standins::{Tables, write_clone};
use paint::{Canvas, Mark, Upright};

/// How a dealt breakable looks while it holds a check.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look {
    /// Its marked copy.
    Marked,
}

/// The look of a breakable dealt `item`. Always [`Look::Marked`] for now:
/// item classes (major, key, token, junk) each add a look here and a copy
/// for it, and nothing else moves.
pub fn look(_item: &str) -> Look {
    Look::Marked
}

/// How a model's copy is painted.
#[derive(Clone, Copy)]
enum Paint {
    /// A gold band on the pot's shoulder, and Triforces on its belly.
    Pot,
    /// The big crate's diagonal brace in gold, with Triforces along it.
    Brace,
    /// The small crate: a Triforce on each face's island.
    Faces,
    /// Rocks, hives and grass: the whole albedo in brightened gold.
    Tint,
}

/// A breakable's model, and the albedos its copy repaints.
struct Model {
    project: &'static Name,
    fmdb: &'static Name,
    albedos: &'static [&'static Name],
    paint: Paint,
    /// Whether its project's color animation (`Model/PROJECT.anim`) swaps
    /// its albedo for another pot's by name, so the copy needs one naming
    /// the marked albedos.
    colored: bool,
}

macro_rules! pot {
    ($fmdb:ident, $albedo:ident) => {
        Model {
            project: actor::OBJ_POT_A,
            fmdb: actor::$fmdb,
            albedos: &[texture::$albedo],
            paint: Paint::Pot,
            colored: false,
        }
    };
}

static POT_01: Model = pot!(OBJ_POT_A_01, ROCK_POT_A_ALB);
static POT_02: Model = pot!(OBJ_POT_A_02, ROCK_POT_A_02_ALB);
static POT_03: Model = pot!(OBJ_POT_A_03, ROCK_POT_A_03_ALB);
static POT_04: Model = pot!(OBJ_POT_A_04, ROCK_POT_A_04_ALB);
static POT_05: Model = pot!(OBJ_POT_A_05, ROCK_POT_A_05_ALB);
static POT_06: Model = pot!(OBJ_POT_A_06, ROCK_POT_A_06_ALB);
static POT_07: Model = pot!(OBJ_POT_A_07, ROCK_POT_A_07_ALB);
static POT_08: Model = pot!(OBJ_POT_A_08, ROCK_POT_A_08_ALB);
static BIG_CRATE: Model = Model {
    project: actor::OBJ_BROKEN_BOX_A,
    fmdb: actor::OBJ_BROKEN_BOX_A_04,
    albedos: &[texture::OBJ_BROKEN_BOX_A_04_ALB],
    paint: Paint::Brace,
    colored: false,
};
static SMALL_CRATE: Model = Model {
    project: actor::OBJ_BROKEN_BOX_A,
    fmdb: actor::OBJ_BROKEN_BOX_A_03,
    albedos: &[texture::STONE_BROKEN_BOX_A_ALB],
    paint: Paint::Faces,
    colored: false,
};
static ROCK: Model = Model {
    project: actor::OBJ_ROCK_A,
    fmdb: actor::OBJ_LIFT_ROCK_A_01,
    albedos: &[texture::MT_ROCK_LIFT_ROCK_A_01_ALB],
    paint: Paint::Tint,
    colored: false,
};
static HIVE: Model = Model {
    project: actor::OBJ_KOKIRI_BEEHIVE_A,
    fmdb: actor::OBJ_KOKIRI_BEEHIVE_A_01,
    albedos: &[texture::OBJ_HIDING_HOLE_BEEHIVE_A_ALB],
    paint: Paint::Tint,
    colored: false,
};
/// Grass: its blades, and the billboard it's drawn as from afar.
static GRASS: Model = Model {
    project: actor::OBJ_PLANT_A,
    fmdb: actor::OBJ_PLANT_GRASS_A_02,
    albedos: &[
        texture::PLANT_LIFT_GRASS_A_02_ALB,
        texture::OBJ_PLANT_A_BILL_BOARD_ALB,
    ],
    paint: Paint::Tint,
    colored: false,
};

/// The flying pots' one model, the plain pot's mesh, colored as each
/// temple's pot by its animation.
static FLYING_POT: Model = Model {
    project: actor::ENEMY_FLYING_POT,
    fmdb: actor::ENEMY_FLYING_POT,
    albedos: &[texture::ROCK_POT_A_ALB],
    paint: Paint::Pot,
    colored: true,
};

/// Each breakable that's marked, and the model it shows. A respawning or
/// Dream pot shows its plain pot's model, and the rocks and grass share
/// one each.
const MARKED: [(&Name, &Model); 24] = [
    (actor::OBJ_POT_A_01, &POT_01),
    (actor::OBJ_POT_A_02, &POT_02),
    (actor::OBJ_POT_A_03, &POT_03),
    (actor::OBJ_POT_A_04, &POT_04),
    (actor::OBJ_POT_A_05, &POT_05),
    (actor::OBJ_POT_A_06, &POT_06),
    (actor::OBJ_POT_A_07, &POT_07),
    (actor::OBJ_POT_A_08, &POT_08),
    (actor::OBJ_POT_A_02_RESPAWN, &POT_02),
    (actor::OBJ_POT_A_05_RESPAWN, &POT_05),
    (actor::OBJ_POT_A_07_RESPAWN, &POT_07),
    (actor::OBJ_POT_A_08_RESPAWN, &POT_08),
    (actor::OBJ_POT_A_06_DREAM, &POT_06),
    (actor::ENEMY_FLYING_POT_ICE_CAVERN, &FLYING_POT),
    (actor::ENEMY_FLYING_POT_SHADOW_TEMPLE, &FLYING_POT),
    (actor::ENEMY_FLYING_POT_SPIRIT_TEMPLE, &FLYING_POT),
    (actor::OBJ_BROKEN_BOX_A_01, &BIG_CRATE),
    (actor::OBJ_BROKEN_BOX_A_03, &SMALL_CRATE),
    (actor::OBJ_LIFT_ROCK_A_01, &ROCK),
    (actor::OBJ_LIFT_ROCK_A_03, &ROCK),
    (actor::OBJ_KOKIRI_BEEHIVE_A_01, &HIVE),
    (actor::OBJ_PLANT_GRASS_A_01, &GRASS),
    (actor::OBJ_PLANT_GRASS_A_01_RESPAWN, &GRASS),
    (actor::OBJ_PLANT_GRASS_A_03, &GRASS),
];

/// Each shrub that's marked: drawn by the foliage system, so marked by its
/// leaves' color (`shrubs`).
const SHRUBS: [&Name; 1] = [actor::LAND_KOKIRI_PLANT_A_02_DROP_FIXED_03];

// A pot's albedo runs up the pot from its foot (v 0) to its lip (v 0.45),
// then down its inside (`probe uvs`), the same rows on every pot, which
// share one mesh. So the image is the pot upside down.

/// A pot's band: the shoulder, neck and lip, as fractions of the height.
const POT_BAND: (f32, f32) = (0.31, 0.5);
/// How far a pot's band fades in and out at its edges, as a fraction of
/// the height. A brace's patch fades over half that.
const BAND_FEATHER: f32 = 0.02;
/// Where a pot's Triforces go, each a (u, v) centre: on its belly, a
/// third of its wrap apart so one always faces the camera.
const POT_STAMPS: [(f32, f32); 3] = [(1.0 / 6.0, 0.17), (0.5, 0.17), (5.0 / 6.0, 0.17)];
/// How tall each of a pot's Triforces is, as a fraction of the height.
const POT_STAMP_HEIGHT: f32 = 0.17;
/// A pot's metres per v over metres per u (`probe uvs`).
const POT_STRETCH: f32 = 1.734 / 1.567;

/// The big crate's braces: the plank on its albedo each brace shows (two
/// faces have one, found with a numbered grid live), from its top left to
/// its bottom right in texels of the 512 square. Its bevels sit on its
/// edges.
const BRACES: [((f32, f32), (f32, f32)); 2] = [
    ((310.0, 246.0), (348.0, 370.0)),
    ((429.0, 264.0), (469.0, 406.0)),
];
const BRACE_SIDE: f32 = 512.0;
/// Where along each brace its Triforces go, as fractions of its length,
/// and how tall each is, as a fraction of the texture, to fit inside its
/// bevels.
const BRACE_STAMPS: [f32; 3] = [0.2, 0.5, 0.8];
const BRACE_STAMP_HEIGHT: f32 = 0.045;

/// The small crate's islands, one a face (`probe uvs`): the four sides',
/// then the top's centres. Its image runs down each side (v 0 at its top
/// edge), and a step along u or v goes as far on the crate.
const FACES: [(f32, f32); 5] = [
    (0.173, 0.195),
    (0.522, 0.193),
    (0.176, 0.56),
    (0.516, 0.563),
    (0.815, 0.753),
];
const FACE_STAMP_HEIGHT: f32 = 0.16;

/// The kinds of texture a copy keeps its host's of: all but the albedo.
const KEPT: [&str; 6] = ["_Nrm", "_Rgh", "_AO", "_Mtl", "_Emm", "_Gn0"];

/// Words a name without a class part is marked by, each swapped for one
/// as long.
const WORDS: [(&str, &str); 1] = [("FlyingPot", "MarkedPot")];

/// The marked copy of a name: its class part `A` made `Z`, or else a word
/// of [`WORDS`] swapped, so the copy is as long as the name, as a model's
/// rename needs.
pub fn marked(name: &str) -> Result<String> {
    if let Some(copy) = lettered(name, MARK) {
        return Ok(copy);
    }
    let parts: Vec<&str> = name.split('_').collect();
    let (from, to) = (WORDS.iter())
        .find(|(from, _)| parts.contains(from))
        .with_context(|| format!("{name} has no class part or word to mark"))?;
    Ok(name.replacen(from, to, 1))
}

/// The class letter a marked copy takes.
const MARK: char = 'Z';

/// `name` with its class part `A` made `letter`, as long as `name`, as a
/// model's rename needs. None without a class part.
pub(crate) fn lettered(name: &str, letter: char) -> Option<String> {
    let mut parts: Vec<String> = name.split('_').map(str::to_owned).collect();
    let class = parts.iter().position(|p| p == "A")?;
    parts[class] = letter.to_string();
    Some(parts.join("_"))
}

/// The marked copy `actor` stands as while it holds a check, if it's a
/// breakable that's marked.
fn marked_actor(actor: &str) -> Option<String> {
    let found = MARKED.iter().any(|(a, _)| *a == actor) || SHRUBS.iter().any(|a| *a == actor);
    found.then(|| marked(actor).ok()).flatten()
}

/// Texture `name`'s file.
pub(crate) fn texture_file(name: &str) -> String {
    format!("Tex/{name}.bntx.zs")
}

/// Writes each marked model, its painted albedo, and each marked actor
/// with its host's rows and tags. Without its Tag row a marked pot can't
/// be lifted (spike, 2026-11-22).
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    tags: &mut Tags,
) -> Result<()> {
    let triforce = Mark::triforce(romfs)?;
    for (albedo, style) in albedos() {
        write_albedo(romfs, builder, albedo, style, &triforce)?;
    }
    let mut done: Vec<&Name> = Vec::new();
    for (host, model) in MARKED {
        if !done.contains(&model.fmdb) {
            write_model(romfs, builder, model)?;
            done.push(model.fmdb);
        }
        let name = marked(host)?;
        let (project, fmdb) = (marked(model.project)?, marked(model.fmdb)?);
        let template = ActorPack::load(romfs, host)?;
        let [actor, (at, info, like)] = pointed_at(&template, &name, &project, &fmdb)?;
        let info = if model.colored {
            colored_by(info, model.project, &project)?
        } else {
            info
        };
        write_clone(builder, &template, &name, [actor, (at, info, like)])?;
        tables.copy(&name, host)?;
        tables.point_at(&name, &project, &fmdb)?;
        tags.copy(&name, host)?;
    }
    for host in SHRUBS {
        shrubs::write(romfs, builder, tables, tags, host)?;
    }
    Ok(())
}

/// `model`'s marked copy, naming its marked albedos, and its color
/// animation's when it has one.
fn write_model(romfs: &Romfs, builder: &mut ModBuilder, model: &Model) -> Result<()> {
    let host = model_file(model.project, model.fmdb);
    let copy = copy_model(&romfs.read(&host)?, model)?;
    let project = marked(model.project)?;
    builder.add(&model_file(&project, &marked(model.fmdb)?), &copy, &host)?;
    if model.colored {
        let like = anim_file(model.project);
        let copy = copy_anim(&romfs.read(&like)?, model.project)?;
        builder.add(&anim_file(&project), &copy, &like)?;
    }
    Ok(())
}

/// A project's color animation.
fn anim_file(project: &str) -> String {
    format!("Model/{project}.anim.bfres.zs")
}

/// `data`, `project`'s color animation, renamed as its marked copy's: its
/// project marked, and every pot albedo it swaps in.
fn copy_anim(data: &[u8], project: &str) -> Result<Vec<u8>> {
    let copy = rename(data, project, &marked(project)?)?;
    let mut pots = albedos()
        .into_iter()
        .filter(|(_, style)| matches!(style, Paint::Pot));
    pots.try_fold(copy, |copy, (albedo, _)| {
        rename(&copy, albedo, &marked(albedo)?)
    })
}

/// ModelInfo `info` with each color animation it plays named for
/// `project`'s marked copy, `marked`, not `project`.
fn colored_by(mut info: BymlDoc, project: &str, marked: &str) -> Result<BymlDoc> {
    let anims = info
        .root
        .as_mut_map()?
        .get_mut(field::MODEL_VARIATION_ANIMS.get());
    let anims = anims.context("a colored model's ModelInfo plays no animation")?;
    for anim in anims.as_mut_array()? {
        let anim = anim.as_mut_map()?;
        let fmab = (anim.get(field::FMAB.get()))
            .and_then(|f| f.as_string().ok())
            .context("an animation with no Fmab")?
            .replace(project, marked);
        anim.insert(field::FMAB.get().into(), Byml::String(fmab.into()));
    }
    Ok(info)
}

/// `albedo`'s marked copy, painted in `style`.
fn write_albedo(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    albedo: &Name,
    style: Paint,
    triforce: &Mark,
) -> Result<()> {
    let name = marked(albedo)?;
    let like = texture_file(albedo);
    let data = romfs.read(&like)?;
    let mut canvas = Canvas::new(top_level(&data)?);
    paint(&mut canvas, style, triforce);
    let data = redraw(
        &rename(&data, albedo, &name)?,
        &canvas.texture.rgba,
        |x, y| !canvas.is_painted(x, y),
    )?;
    builder.add(&texture_file(&name), &data, &like)
}

/// `data`, `model`'s file, renamed as its marked copy: its name
/// (`PROJECT.FMDB`, the only place the project shows), its model and its
/// albedos marked. A texture named after the model is renamed with it,
/// so every one the copy doesn't repaint gets its host's name back.
fn copy_model(data: &[u8], model: &Model) -> Result<Vec<u8>> {
    let whole = |project: &str, fmdb: &str| format!("{project}.{fmdb}");
    let copy = rename(
        data,
        &whole(model.project, model.fmdb),
        &whole(&marked(model.project)?, &marked(model.fmdb)?),
    )?;
    let copy = rename(&copy, model.fmdb, &marked(model.fmdb)?)?;
    let copy = model
        .albedos
        .iter()
        .try_fold(copy, |copy, albedo| rename(&copy, albedo, &marked(albedo)?))?;
    KEPT.iter().try_fold(copy, |copy, kind| {
        let kept = format!("{}{kind}", model.fmdb.get());
        rename(&copy, &marked(&kept)?, &kept)
    })
}

/// Each marked albedo painted, written to `out` as PNG under its marked
/// name beside its host's, for a look before a seed: `probe marks`.
pub fn preview(romfs: &Romfs, out: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let triforce = Mark::triforce(romfs)?;
    for (albedo, style) in albedos() {
        let host = top_level(&romfs.read(&texture_file(albedo))?)?;
        std::fs::write(
            out.join(format!("{}.png", albedo.get())),
            crate::sprites::png(&host)?,
        )?;
        let mut canvas = Canvas::new(host);
        paint(&mut canvas, style, &triforce);
        let path = out.join(format!("{}.png", marked(albedo)?));
        std::fs::write(&path, crate::sprites::png(&canvas.texture)?)?;
        println!("{}", path.display());
    }
    Ok(())
}

/// Each albedo a marked model repaints, once, with its paint.
fn albedos() -> Vec<(&'static Name, Paint)> {
    let mut all: Vec<(&Name, Paint)> = Vec::new();
    for (_, model) in MARKED {
        for albedo in model.albedos {
            if !all.iter().any(|(a, _)| a == albedo) {
                all.push((albedo, model.paint));
            }
        }
    }
    all
}

fn paint(canvas: &mut Canvas, style: Paint, triforce: &Mark) {
    match style {
        Paint::Pot => {
            canvas.band(POT_BAND.0, POT_BAND.1, BAND_FEATHER);
            for at in POT_STAMPS {
                canvas.stamp(
                    triforce,
                    at,
                    POT_STAMP_HEIGHT,
                    POT_STRETCH,
                    Upright::Flipped,
                );
            }
        }
        Paint::Brace => {
            for ((left, top), (right, bottom)) in BRACES {
                let [left, top, right, bottom] = [left, top, right, bottom].map(|t| t / BRACE_SIDE);
                canvas.patch((left, top), (right, bottom), BAND_FEATHER / 2.0);
                for along in BRACE_STAMPS {
                    let at = ((left + right) / 2.0, top + (bottom - top) * along);
                    canvas.stamp(triforce, at, BRACE_STAMP_HEIGHT, 1.0, Upright::Flipped);
                }
            }
        }
        Paint::Faces => {
            for at in FACES {
                canvas.stamp(triforce, at, FACE_STAMP_HEIGHT, 1.0, Upright::AsIs);
            }
        }
        Paint::Tint => canvas.tint(),
    }
}

/// Points `actor`, a holder standing until its check is picked, at its
/// copy for `look`, if it's a breakable that's marked. Its twin, cloned
/// before, keeps the vanilla one. Returns whether it changed.
pub fn mark(actor: &mut Byml, look: Look) -> Result<bool> {
    let Look::Marked = look;
    let Some(copy) = checks::str_field(actor, GYAML).and_then(marked_actor) else {
        return Ok(false);
    };
    actor.as_mut_map()?.insert(GYAML.into(), copy.into());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_spelled_out_are_the_ones_the_marks_read() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut read: Vec<String> = Vec::new();
        for (_, model) in MARKED {
            let file = model_file(model.project, model.fmdb);
            if !read.contains(&file) {
                read.push(file);
                if model.colored {
                    read.push(anim_file(model.project));
                }
            }
        }
        read.extend(
            albedos()
                .into_iter()
                .map(|(albedo, _)| texture_file(albedo)),
        );
        read.push(texture_file(texture::TRI_FORCE_MARK_GRAVEYARD_ASSETS_A_ALB));
        read.extend(shrubs::geometry_files(&romfs, &SHRUBS).unwrap());
        assert_eq!(read, FILES);
    }

    #[test]
    fn a_copy_names_its_marked_albedo_and_its_hosts_other_textures() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let has = |data: &[u8], name: &str| data.windows(name.len()).any(|w| w == name.as_bytes());
        for model in [&BIG_CRATE, &POT_01, &SMALL_CRATE, &ROCK, &HIVE, &GRASS] {
            let host = romfs.read(&model_file(model.project, model.fmdb)).unwrap();
            let copy = copy_model(&host, model).unwrap();
            assert_eq!(copy.len(), host.len());
            for albedo in model.albedos {
                assert!(has(&copy, &marked(albedo).unwrap()), "{albedo}");
                assert!(!has(&copy, albedo), "{albedo}");
            }
            let whole = |project: &str, fmdb: &str| format!("{project}.{fmdb}");
            let copied = whole(
                &marked(model.project).unwrap(),
                &marked(model.fmdb).unwrap(),
            );
            assert!(has(&copy, &copied), "{copied}");
            assert!(has(&copy, &marked(model.fmdb).unwrap()));
            let normal = format!("{}_Nrm", model.fmdb.get());
            assert_eq!(has(&copy, &normal), has(&host, &normal), "{normal}");
            assert!(!has(&copy, &marked(&normal).unwrap()));
        }
    }

    #[test]
    fn a_marked_name_swaps_its_class_and_keeps_its_length() {
        assert_eq!(marked("Obj_FakePot_A_01").unwrap(), "Obj_FakePot_Z_01");
        assert_eq!(marked("Rock_FakePot_A_Alb").unwrap(), "Rock_FakePot_Z_Alb");
        assert!(marked("Enemy_FakePot").is_err());
    }

    #[test]
    fn a_name_without_a_class_is_marked_by_its_word() {
        let (word, swapped) = WORDS[0];
        let name = format!("Enemy_{word}_FakeTemple");
        assert_eq!(
            marked(&name).unwrap(),
            format!("Enemy_{swapped}_FakeTemple")
        );
        assert_eq!(swapped.len(), word.len());
    }

    #[test]
    fn a_flying_pot_s_copy_colors_by_its_own_animation_and_the_marked_pots() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let has = |data: &[u8], name: &str| data.windows(name.len()).any(|w| w == name.as_bytes());
        let host = romfs.read(&anim_file(FLYING_POT.project)).unwrap();
        let copy = copy_anim(&host, FLYING_POT.project).unwrap();
        assert!(!has(&copy, FLYING_POT.project));
        for (albedo, _) in albedos() {
            assert!(!has(&copy, albedo), "{albedo}");
            assert_eq!(
                has(&copy, &marked(albedo).unwrap()),
                has(&host, albedo),
                "{albedo}"
            );
        }
        let host = ActorPack::load(&romfs, actor::ENEMY_FLYING_POT_ICE_CAVERN).unwrap();
        let project = marked(FLYING_POT.project).unwrap();
        let [_, (_, info, _)] = pointed_at(&host, "Enemy_FakePot", &project, &project).unwrap();
        let info = colored_by(info, FLYING_POT.project, &project).unwrap();
        let anims = &info.root.as_map().unwrap()[field::MODEL_VARIATION_ANIMS.get()];
        let fmab = anims.as_array().unwrap()[0].as_map().unwrap()[field::FMAB.get()].clone();
        let Byml::String(fmab) = fmab else {
            panic!("no Fmab")
        };
        assert!(
            fmab.contains(&project) && !fmab.contains(FLYING_POT.project.get()),
            "{fmab}"
        );
    }

    #[test]
    fn every_breakable_the_shuffle_deals_has_a_marked_copy() {
        use crate::pickups::{CRATES, FLYING_POTS, GRASS, HIVES, POTS, ROCKS};
        let kinds = [&POTS, &FLYING_POTS, &CRATES, &ROCKS, &GRASS, &HIVES];
        for host in kinds.iter().flat_map(|k| k.actors) {
            assert!(
                MARKED.iter().any(|(a, _)| a == host) || SHRUBS.contains(host),
                "{host}"
            );
        }
    }

    #[test]
    fn a_marked_copy_keeps_its_hosts_kind_for_the_player() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let hosts = MARKED.iter().map(|(a, _)| *a).chain(SHRUBS);
        for host in hosts {
            let copy = marked(host).unwrap();
            assert_eq!(
                crate::nav::kinds::word(&copy),
                crate::nav::kinds::word(host),
                "{copy}"
            );
        }
    }

    #[test]
    fn no_two_marked_models_share_a_path() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let mut paths: Vec<String> = MARKED
            .iter()
            .map(|(_, m)| model_file(&marked(m.project).unwrap(), &marked(m.fmdb).unwrap()))
            .collect();
        let models = MARKED
            .iter()
            .map(|(_, m)| m.fmdb)
            .collect::<std::collections::BTreeSet<_>>();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), models.len());
    }

    #[test]
    fn every_holder_looks_marked_whatever_it_holds() {
        assert_eq!(look("Obj_FakeRupee"), Look::Marked);
        assert_eq!(look(""), Look::Marked);
    }

    #[test]
    fn no_marked_name_is_the_game_s_own() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (host, model) in MARKED {
            let names = [host, model.project, model.fmdb]
                .into_iter()
                .chain(model.albedos.iter().copied());
            for name in names {
                let copy = marked(name).unwrap();
                assert_eq!(copy.len(), name.len());
                let exists = |rel: &str| romfs.root().join(rel).exists();
                assert!(!exists(&format!("Pack/Actor/{copy}.pack.zs")), "{copy}");
                assert!(!exists(&texture_file(&copy)), "{copy}");
                assert!(!exists(&model_file(&copy, &copy)), "{copy}");
            }
            assert!(romfs.read(&model_file(model.project, model.fmdb)).is_ok());
            for albedo in model.albedos {
                assert!(romfs.read(&texture_file(albedo)).is_ok());
            }
        }
    }

    #[test]
    fn a_holder_names_its_marked_copy_and_anything_else_stays() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let pot = actor::OBJ_POT_A_01;
        let mut holder = Byml::from_iter([(GYAML, Byml::String(pot.get().into()))]);
        assert!(mark(&mut holder, Look::Marked).unwrap());
        assert_eq!(
            checks::str_field(&holder, GYAML),
            Some(marked(pot).unwrap().as_str())
        );
        let mut other = Byml::from_iter([(GYAML, Byml::String("Obj_FakeRock_A_01".into()))]);
        assert!(!mark(&mut other, Look::Marked).unwrap());
    }
}
