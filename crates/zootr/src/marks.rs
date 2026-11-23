//! Breakables marked while they hold a check. A dealt breakable stands as
//! a marked copy of its actor until Link picks up its item, then as its
//! vanilla twin (`picked`). The copy shows a copy of its model whose
//! albedo is painted gold ([`paint`]), made from the dump each seed, and
//! keeps its host's rows, its tags among them, so it lifts and breaks the
//! same. Every seed writes the same copies, and only dealt holders name
//! them, so a kind the shuffle leaves out stays vanilla.

mod files;
mod paint;
mod shrubs;

pub use files::FILES;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, ModBuilder, Romfs, redraw, rename, top_level};

use crate::checks::{self, GYAML};
use crate::key_tints::pointed_at;
use crate::known::{Name, actor, texture};
use crate::medallions::model_file;
use crate::rsdb::Tags;
use crate::standins::{Tables, write_clone};
use paint::{Canvas, Mark, Upright};

/// How a model's copy is painted.
#[derive(Clone, Copy)]
enum Look {
    /// A gold band on the pot's shoulder, and Triforces on its belly.
    Pot,
    /// The big crate's diagonal brace in gold, with Triforces along it.
    Brace,
}

/// A breakable's model, and the albedo its copy repaints.
struct Model {
    project: &'static Name,
    fmdb: &'static Name,
    albedo: &'static Name,
    look: Look,
}

const fn pot(fmdb: &'static Name, albedo: &'static Name) -> Model {
    Model {
        project: actor::OBJ_POT_A,
        fmdb,
        albedo,
        look: Look::Pot,
    }
}

static POT_01: Model = pot(actor::OBJ_POT_A_01, texture::ROCK_POT_A_ALB);
static POT_02: Model = pot(actor::OBJ_POT_A_02, texture::ROCK_POT_A_02_ALB);
static POT_03: Model = pot(actor::OBJ_POT_A_03, texture::ROCK_POT_A_03_ALB);
static POT_04: Model = pot(actor::OBJ_POT_A_04, texture::ROCK_POT_A_04_ALB);
static POT_05: Model = pot(actor::OBJ_POT_A_05, texture::ROCK_POT_A_05_ALB);
static POT_06: Model = pot(actor::OBJ_POT_A_06, texture::ROCK_POT_A_06_ALB);
static POT_07: Model = pot(actor::OBJ_POT_A_07, texture::ROCK_POT_A_07_ALB);
static POT_08: Model = pot(actor::OBJ_POT_A_08, texture::ROCK_POT_A_08_ALB);
static BIG_CRATE: Model = Model {
    project: actor::OBJ_BROKEN_BOX_A,
    fmdb: actor::OBJ_BROKEN_BOX_A_04,
    albedo: texture::OBJ_BROKEN_BOX_A_04_ALB,
    look: Look::Brace,
};

/// Each breakable that's marked, and the model it shows.
const MARKED: [(&Name, &Model); 9] = [
    (actor::OBJ_POT_A_01, &POT_01),
    (actor::OBJ_POT_A_02, &POT_02),
    (actor::OBJ_POT_A_03, &POT_03),
    (actor::OBJ_POT_A_04, &POT_04),
    (actor::OBJ_POT_A_05, &POT_05),
    (actor::OBJ_POT_A_06, &POT_06),
    (actor::OBJ_POT_A_07, &POT_07),
    (actor::OBJ_POT_A_08, &POT_08),
    (actor::OBJ_BROKEN_BOX_A_01, &BIG_CRATE),
];

/// Each shrub that's marked: drawn by the foliage system, so marked by its
/// leaves' colour (`shrubs`).
const SHRUBS: [&Name; 1] = [actor::LAND_KOKIRI_PLANT_A_02_DROP_FIXED_03];

/// A pot's albedo runs up the pot from its foot (v 0) to its lip (v 0.45),
/// then down its inside (`probe uvs`), the same rows on every pot, which
/// share one mesh. So the image is the pot upside down.
/// Its band: the shoulder, neck and lip, as fractions of the height.
const POT_BAND: (f32, f32) = (0.31, 0.5);
const BAND_FEATHER: f32 = 0.02;
/// Where a pot's Triforces go: on its belly, half its wrap apart, and how
/// tall each is.
const POT_STAMPS: [(f32, f32); 2] = [(0.25, 0.17), (0.75, 0.17)];
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

/// The kinds of texture a copy keeps its host's of: all but the albedo.
const KEPT: [&str; 6] = ["_Nrm", "_Rgh", "_AO", "_Mtl", "_Emm", "_Gn0"];

/// The marked copy of a name: its class part `A` made `Z`, so the copy
/// is as long as the name, as a model's rename needs.
pub fn marked(name: &str) -> Result<String> {
    let mut parts: Vec<&str> = name.split('_').collect();
    let class = (parts.iter().position(|p| *p == "A"))
        .with_context(|| format!("{name} has no class part to mark"))?;
    parts[class] = "Z";
    Ok(parts.join("_"))
}

/// The marked copy `actor` stands as while it holds a check, if it's a
/// breakable that's marked.
fn marked_actor(actor: &str) -> Option<String> {
    let found = MARKED.iter().any(|(a, _)| *a == actor) || SHRUBS.iter().any(|a| *a == actor);
    found.then(|| marked(actor).ok()).flatten()
}

pub(crate) fn texture_path(name: &str) -> String {
    format!("Tex/{name}.bntx.zs")
}

/// Writes each marked model, its painted albedo, and each marked actor
/// with its host's rows and tags. Without its Tag row a marked pot can't
/// be lifted (spike, 2026-11-22).
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    let triforce = Mark::triforce(romfs)?;
    let mut tags = Tags::load(romfs)?;
    let mut done: Vec<&Name> = Vec::new();
    for (host, model) in MARKED {
        if !done.contains(&model.fmdb) {
            write_model(romfs, builder, model, &triforce)?;
            done.push(model.fmdb);
        }
        let name = marked(host)?;
        let (project, fmdb) = (marked(model.project)?, marked(model.fmdb)?);
        let template = ActorPack::load(romfs, host)?;
        write_clone(
            builder,
            &template,
            &name,
            pointed_at(&template, &name, &project, &fmdb)?,
        )?;
        tables.copy(&name, host)?;
        tables.point_at(&name, &project, &fmdb)?;
        tags.copy(&name, host)?;
    }
    for host in SHRUBS {
        shrubs::write(romfs, builder, tables, &mut tags, host)?;
    }
    tags.write(builder)
}

/// `model`'s marked copy, naming its marked albedo, and the albedo painted.
fn write_model(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    model: &Model,
    triforce: &Mark,
) -> Result<()> {
    let host = model_file(model.project, model.fmdb);
    let project = marked(model.project)?;
    let copy = copy_model(&romfs.read(&host)?, model)?;
    builder.add(&model_file(&project, &marked(model.fmdb)?), &copy, &host)?;
    let albedo = marked(model.albedo)?;
    let like = texture_path(model.albedo);
    let data = romfs.read(&like)?;
    let mut canvas = Canvas::new(top_level(&data)?);
    paint(&mut canvas, model.look, triforce);
    let width = canvas.texture.width;
    let painted = |x: usize, y: usize| canvas.painted[y * width + x];
    let data = redraw(
        &rename(&data, model.albedo, &albedo)?,
        &canvas.texture.rgba,
        |x, y| !painted(x, y),
    )?;
    builder.add(&texture_path(&albedo), &data, &like)
}

/// `data`, `model`'s file, renamed as its marked copy: its project and
/// its albedo marked. A texture named after the model is renamed with it,
/// so every one the copy doesn't repaint gets its host's name back.
fn copy_model(data: &[u8], model: &Model) -> Result<Vec<u8>> {
    let copy = rename(data, model.project, &marked(model.project)?)?;
    let copy = rename(&copy, model.albedo, &marked(model.albedo)?)?;
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
    for (_, model) in MARKED {
        let host = top_level(&romfs.read(&texture_path(model.albedo))?)?;
        std::fs::write(
            out.join(format!("{}.png", model.albedo.get())),
            crate::sprites::png(&host)?,
        )?;
        let mut canvas = Canvas::new(host);
        paint(&mut canvas, model.look, &triforce);
        let name = marked(model.albedo)?;
        let path = out.join(format!("{name}.png"));
        std::fs::write(&path, crate::sprites::png(&canvas.texture)?)?;
        println!("{}", path.display());
    }
    Ok(())
}

fn paint(canvas: &mut Canvas, look: Look, triforce: &Mark) {
    match look {
        Look::Pot => {
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
        Look::Brace => {
            for ((left, top), (right, bottom)) in BRACES {
                let [left, top, right, bottom] = [left, top, right, bottom].map(|t| t / BRACE_SIDE);
                canvas.patch((left, top), (right, bottom), BAND_FEATHER / 2.0);
                for along in BRACE_STAMPS {
                    let at = ((left + right) / 2.0, top + (bottom - top) * along);
                    canvas.stamp(triforce, at, BRACE_STAMP_HEIGHT, 1.0, Upright::Flipped);
                }
            }
        }
    }
}

/// Points `actor`, a holder standing until its check is picked, at its
/// marked copy, if it's a breakable that's marked. Its twin, cloned
/// before, keeps the vanilla one. Returns whether it changed.
pub fn mark(actor: &mut Byml) -> Result<bool> {
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
            }
        }
        for (_, model) in MARKED {
            let file = texture_path(model.albedo);
            if !read.contains(&file) {
                read.push(file);
            }
        }
        read.push(texture_path(texture::TRI_FORCE_MARK_GRAVEYARD_ASSETS_A_ALB));
        read.extend(shrubs::geometry_files(&romfs, &SHRUBS).unwrap());
        assert_eq!(read, FILES);
    }

    #[test]
    fn a_copy_names_its_marked_albedo_and_its_hosts_other_textures() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let has = |data: &[u8], name: &str| data.windows(name.len()).any(|w| w == name.as_bytes());
        for model in [&BIG_CRATE, &POT_01] {
            let host = romfs.read(&model_file(model.project, model.fmdb)).unwrap();
            let copy = copy_model(&host, model).unwrap();
            assert_eq!(copy.len(), host.len());
            assert!(has(&copy, &marked(model.albedo).unwrap()));
            assert!(!has(&copy, model.albedo));
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
    fn no_marked_name_is_the_game_s_own() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (host, model) in MARKED {
            for name in [host, model.project, model.fmdb, model.albedo] {
                let copy = marked(name).unwrap();
                assert_eq!(copy.len(), name.len());
                let exists = |rel: &str| romfs.root().join(rel).exists();
                assert!(!exists(&format!("Pack/Actor/{copy}.pack.zs")), "{copy}");
                assert!(!exists(&texture_path(&copy)), "{copy}");
                assert!(!exists(&model_file(&copy, &copy)), "{copy}");
            }
            assert!(romfs.read(&model_file(model.project, model.fmdb)).is_ok());
            assert!(romfs.read(&texture_path(model.albedo)).is_ok());
        }
    }

    #[test]
    fn a_holder_names_its_marked_copy_and_anything_else_stays() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let pot = actor::OBJ_POT_A_01;
        let mut holder = Byml::from_iter([(GYAML, Byml::String(pot.get().into()))]);
        assert!(mark(&mut holder).unwrap());
        assert_eq!(
            checks::str_field(&holder, GYAML),
            Some(marked(pot).unwrap().as_str())
        );
        let mut other = Byml::from_iter([(GYAML, Byml::String("Obj_FakeRock_A_01".into()))]);
        assert!(!mark(&mut other).unwrap());
    }
}
