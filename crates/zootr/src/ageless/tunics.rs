//! Tunics for a child. The adult's Zora and Goron armor is a whole adult
//! upper body, so a child wears copies of it instead, each named in the
//! hook's [`zootr_hook::CHILD_ARMOR`]: his own cloth and vest in the
//! tunic's colours, the rest of his model hidden, with the adult armor's
//! effects. His Kokiri Tunic's copy is the adult's empty armor, which
//! takes a worn tunic off. The hook puts them on him (`patch/gear.rs`).
//! See `superpowers/specs/2026-11-23-child-gear-design.md`.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, BymlDoc, ModBuilder, Romfs};
use zootr_hook::{CHILD_ARMOR, CHILD_GEAR, child_armor};

use super::files::{CLOTH, MODEL, VEST};
use crate::key_tints::pointed_at;
use crate::known::{Name, actor, field};
use crate::medallions::paint::{self, Band, Colour, Hues};
use crate::standins::{Tables, actor_path, raw, write_clone};

/// A tunic's colours on a child: his cloth's, and his vest's a shade
/// apart (the maintainer, 2026-11-24). A cosmetics randomizer writes
/// other values here.
pub struct Colours {
    pub cloth: Colour,
    pub vest: Colour,
}

/// Each tunic a child wears in his own cloth: the adult's armor, the
/// letter that marks its texture copies, and its colours.
pub const TUNICS: [(&Name, u8, Colours); 2] = [
    (
        actor::ARMOR_ADULT_ZORA,
        b'Z',
        Colours {
            cloth: [36, 84, 168],
            vest: [28, 62, 132],
        },
    ),
    (
        actor::ARMOR_ADULT_GORON,
        b'G',
        Colours {
            cloth: [168, 40, 32],
            vest: [128, 32, 26],
        },
    ),
];

/// The groups of his materials a tunic's armor hides on him: his cloth and
/// his vest, which the armor draws in its colours instead.
pub const GROUPS: [(&Name, &[&Name]); 2] = [
    (
        field::G_ANGIN,
        &[
            field::MT_CLOTH,
            field::MT_CLOTH_PARTS,
            field::MT_CLOTH_PARTS_ALPHA_MASK,
        ],
    ),
    (field::G_VEST, &[field::MT_VEST]),
];

/// The hues of his green, his vest's yellower stripes among them. His
/// vest's brown patch keeps its colour.
const GREEN: Hues = (0.15, 0.5);

/// Write each child armor, its model and textures, and its rows in
/// `tables`.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    let child = builder.read(MODEL)?;
    for (adult, mark, colours) in &TUNICS {
        let name = child_name(adult)?;
        let model = format!("Model/{name}.{}.bfres.zs", actor::PLAYER_CHILD);
        builder.add(&model, &cloth_only(&child, *mark)?, MODEL)?;
        let cloth = CLOTH.iter().map(|path| (path, colours.cloth));
        for (path, colour) in cloth.chain([(&VEST, colours.vest)]) {
            let (to, data) = painted(&builder.read(path)?, path, *mark, colour)?;
            builder.add(&to, &data, path)?;
        }
        let template = ActorPack::load(romfs, adult)?;
        let files = pointed_at(&template, &name, &name, actor::PLAYER_CHILD)?;
        write_clone(builder, &template, &name, files)?;
        rows(tables, &name, adult)?;
        tables.point_at(&name, &name, actor::PLAYER_CHILD)?;
    }
    let name = child_name(actor::OBJ_ARMOR_CHILD_NORMAL)?;
    let template = ActorPack::load(romfs, actor::ARMOR_ADULT_NORMAL)?;
    let like = actor_path(&template.name);
    let doc = BymlDoc::parse(&raw(&template, &like)?)?;
    write_clone(builder, &template, &name, [(actor_path(&name), doc, like)])?;
    rows(tables, &name, actor::ARMOR_ADULT_NORMAL)
}

/// Rows for `name`, an armor like `like` that answers to its own name.
fn rows(tables: &mut Tables, name: &str, like: &Name) -> Result<()> {
    tables.copy(name, like)?;
    tables.name_itself(name)
}

/// The armor the hook puts on a child whose tunic's slot holds `worn`.
fn child_name(worn: &Name) -> Result<String> {
    let armor = child_armor(worn.as_bytes(), CHILD_GEAR)
        .map(|at| CHILD_ARMOR[at].name())
        .with_context(|| format!("the hook has no child armor for {worn}"))?;
    Ok(armor.to_str()?.to_owned())
}

/// The texture at `path`, as its name.
fn texture(path: &str) -> &str {
    let name = path.strip_prefix("Tex/").unwrap_or(path);
    name.strip_suffix(".bntx.zs").unwrap_or(name)
}

/// `name` with the letter before its `_Alb` made `mark`: a copy's name,
/// as long as the texture's, as a rename needs.
fn marked(name: &str, mark: u8) -> Result<String> {
    let at = name.len().checked_sub(5).context("too short")?;
    ensure!(name[at + 1..] == *"_Alb", "{name} isn't an albedo");
    Ok(format!(
        "{}{}{}",
        &name[..at],
        mark as char,
        &name[at + 1..]
    ))
}

/// The child's model with only his cloth and vest drawn ([`GROUPS`]'
/// materials), and his textures for them the copies marked `mark`.
fn cloth_only(child: &[u8], mark: u8) -> Result<Vec<u8>> {
    let kept: Vec<&str> = (GROUPS.iter())
        .flat_map(|(_, materials)| materials.iter().map(|m| m.get()))
        .collect();
    let mut model = child.to_vec();
    for shape in ounce_formats::shape_names(child) {
        // A shape is named for its mesh, then its material.
        let material = shape.rsplit_once("__").map(|(_, m)| m);
        if !material.is_some_and(|m| kept.contains(&m)) {
            model = ounce_formats::hide_shape(&model, &shape)?;
        }
    }
    for path in CLOTH.iter().chain([&VEST]) {
        let name = texture(path);
        model = ounce_formats::rename(&model, name, &marked(name, mark)?)?;
    }
    Ok(model)
}

/// The texture `data`, at `path`, with its green moved to `colour`, as its
/// copy marked `mark`: the copy's path and its file.
fn painted(data: &[u8], path: &str, mark: u8, colour: Colour) -> Result<(String, Vec<u8>)> {
    let name = texture(path);
    let copy = marked(name, mark)?;
    let data = ounce_formats::rename(data, name, &copy)?;
    let mut top = ounce_formats::top_level(&data)?;
    let to = paint::hsv(colour);
    paint::shift(&mut top.rgba, &[Band { hues: GREEN, to }]);
    let drawn = ounce_formats::redraw(&data, &top.rgba, |_, _| false)?;
    Ok((format!("Tex/{copy}.bntx.zs"), drawn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zootr_hook::fnv;

    #[test]
    fn the_hook_dresses_a_child_in_each_tunics_copy() {
        if crate::game::no_names() {
            return;
        }
        let worn = [TUNICS[0].0, TUNICS[1].0, actor::OBJ_ARMOR_CHILD_NORMAL];
        for (at, worn) in worn.iter().enumerate() {
            assert_eq!(fnv(worn.as_bytes()), CHILD_ARMOR[at].worn, "{worn}");
            assert_eq!(child_armor(worn.as_bytes(), CHILD_GEAR), Some(at));
            assert_eq!(
                child_armor(worn.as_bytes(), 0),
                None,
                "{worn} without the flag"
            );
        }
        let adult = actor::ARMOR_ADULT_NORMAL.as_bytes();
        assert_eq!(child_armor(adult, CHILD_GEAR), None);
    }

    #[test]
    fn a_copy_keeps_its_textures_name_length() {
        let copy = marked("Fake_Tile_A_Alb", b'Z').unwrap();
        assert_eq!(copy, "Fake_Tile_Z_Alb");
        assert!(marked("Fake_Tile_A_Nrm", b'Z').is_err());
    }

    #[test]
    fn each_tunic_paints_his_cloth_and_vest_in_its_table_colours() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let hue = |rgba: &[u8]| paint::mean(rgba, (0.0, 1.0)).unwrap().0;
        for (_, mark, colours) in &TUNICS {
            for (path, colour) in [(CLOTH[0], colours.cloth), (VEST, colours.vest)] {
                let data = romfs.read(path).unwrap();
                let (to, drawn) = painted(&data, path, *mark, colour).unwrap();
                assert!(to.contains(&marked(texture(path), *mark).unwrap()), "{to}");
                let top = ounce_formats::top_level(&drawn).unwrap();
                let (want, _, _) = paint::hsv(colour);
                let got = paint::mean(&top.rgba, (want - 0.05, want + 0.05));
                assert!(got.is_some(), "{path} carries {colour:?}");
                // Another colour in the table paints another texture.
                let other = [colour[2], colour[1], colour[0]];
                let (_, again) = painted(&data, path, *mark, other).unwrap();
                let again = ounce_formats::top_level(&again).unwrap();
                assert!((hue(&again.rgba) - hue(&top.rgba)).abs() > 0.05, "{path}");
            }
        }
    }

    #[test]
    fn his_tunic_model_draws_only_his_cloth_and_vest_from_the_copies() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let child = romfs.read(MODEL).unwrap();
        let model = cloth_only(&child, b'Z').unwrap();
        assert_eq!(model.len(), child.len());
        let has = |data: &[u8], name: &str| data.windows(name.len()).any(|w| w == name.as_bytes());
        for path in CLOTH.iter().chain([&VEST]) {
            let name = texture(path);
            assert!(!has(&model, name), "{name} is renamed");
            assert!(has(&model, &marked(name, b'Z').unwrap()));
        }
        assert_eq!(
            ounce_formats::shape_names(&model),
            ounce_formats::shape_names(&child)
        );
    }

    #[test]
    fn a_seed_writes_every_child_armor() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-tunics-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&romfs, &mut builder, &mut tables).unwrap();
        tables.write(&mut builder).unwrap();
        builder.finish().unwrap();
        for armor in &CHILD_ARMOR {
            let name = armor.name().to_str().unwrap();
            let pack = dir.join("romfs").join(ounce_formats::pack_file(name));
            assert!(pack.exists(), "{name}");
        }
        for (_, mark, _) in &TUNICS {
            for path in CLOTH.iter().chain([&VEST]) {
                let copy = marked(texture(path), *mark).unwrap();
                assert!(
                    dir.join("romfs/Tex")
                        .join(format!("{copy}.bntx.zs"))
                        .exists()
                );
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
