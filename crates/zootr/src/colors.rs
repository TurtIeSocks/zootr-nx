//! Colors: a seed's palettes recoloured, Link's tunics and gear first.
//! Romfs files only: the hook patch never differs by colour. See
//! `superpowers/specs/2026-11-24-colors-design.md`.

mod catalog;
mod files;
mod oklch;
pub mod paint;
mod pick;
pub mod preview;

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{
    ModBuilder, material_param, redraw, rename, set_material_param, top_level, vfx,
};

pub use catalog::{GREEN, Group, Kind, PALETTES, Palette, Write};
use files::{Copy, Tex};
use paint::{Band, Colour};
pub use pick::{
    Colors, Draft, Mode, Pick, Picked, args, draft, drawn, hex, parse_arg, parse_hex, resolve,
};

pub fn palette(id: &str) -> Result<&'static Palette> {
    PALETTES
        .iter()
        .find(|p| p.id == id)
        .with_context(|| format!("no palette {id}: `zootr colors list` names them"))
}

/// The colour of `id`'s first slot in `colors`, when it isn't vanilla.
pub fn first(colors: &Colors, id: &str) -> Result<Option<Colour>> {
    let Some(picked) = colors.get(id) else {
        return Ok(None);
    };
    Ok(Some(picked.slots(palette(id)?)?[0]))
}

/// The files Colors reads, which the dump's cache extracts besides its
/// folders.
pub fn files() -> impl Iterator<Item = &'static str> {
    PALETTES.iter().flat_map(|p| p.writes).flat_map(|w| {
        let params: &[&str] = match &w.kind {
            Kind::Param { models, .. } => models,
            Kind::Vfx { bundle, .. } => std::slice::from_ref(bundle),
            _ => &[],
        };
        let texture = w.texture().into_iter().flat_map(|t| {
            let copies = t
                .copy
                .iter()
                .flat_map(|c| copies(t.path, c))
                .flat_map(|(_, m)| m.iter().copied());
            [t.path].into_iter().chain(t.metal).chain(copies)
        });
        texture.chain(params.iter().copied())
    })
}

/// The spoiler's section: each palette a seed changed and its colours.
/// Nothing for a seed in vanilla colours.
pub fn spoiler(colors: &Colors) -> String {
    let width = colors.keys().map(|id| id.len()).max().unwrap_or(0);
    let lines = colors.iter().map(|(id, p)| {
        let split = p.split.iter().map(|h| format!(" {h}")).collect::<String>();
        format!("  {id:<width$}  {}{split}\n", p.hex)
    });
    match colors.is_empty() {
        true => String::new(),
        false => format!("\nColors\n{}", lines.collect::<String>()),
    }
}

/// One write's goal: the colour its part ends up, as the palette's slot
/// moved from vanilla.
#[derive(Clone)]
struct Job<'a> {
    palette: &'a str,
    write: &'a Write,
    goal: Colour,
}

/// The writes `picked` makes in `palette`, each with its goal.
fn jobs<'a>(palette: &'a Palette, picked: &Picked) -> Result<Vec<Job<'a>>> {
    let slots = picked.slots(palette)?;
    let job = |write| Job {
        palette: palette.id,
        write,
        goal: oklch::follow(
            slots[write.slot],
            palette.vanilla[write.slot],
            write.vanilla(),
        ),
    };
    Ok(palette.writes.iter().map(job).collect())
}

/// Paint every palette `colors` changes. Last of a seed's writes, so a
/// texture another feature copies (the child's tunics) is copied green.
pub fn write(builder: &mut ModBuilder, colors: &Colors) -> Result<()> {
    let colors = with_followers(colors)?;
    let mut by_texture: BTreeMap<&str, (&Tex, Vec<Job>)> = BTreeMap::new();
    let mut by_model: BTreeMap<&str, Vec<Job>> = BTreeMap::new();
    let mut by_bundle: BTreeMap<&str, Vec<Job>> = BTreeMap::new();
    for (id, picked) in &colors {
        for job in jobs(palette(id)?, picked)? {
            match (job.write.texture(), &job.write.kind) {
                (Some(texture), _) => {
                    let entry = by_texture.entry(texture.path);
                    entry.or_insert((texture, Vec::new())).1.push(job);
                }
                (None, Kind::Param { models, .. }) => {
                    for model in *models {
                        by_model.entry(model).or_default().push(job.clone());
                    }
                }
                (None, Kind::Vfx { bundle, .. }) => by_bundle.entry(bundle).or_default().push(job),
                (None, _) => unreachable!("a band or tint has a texture"),
            }
        }
    }
    for (texture, jobs) in by_texture.values() {
        paint_texture(builder, texture, jobs).with_context(|| texture.path.to_owned())?;
    }
    // After the textures: a copy renames its texture in these models too.
    for (model, jobs) in &by_model {
        let data = paint_model(&builder.read(model)?, jobs).with_context(|| model.to_string())?;
        builder.write(model, &data)?;
    }
    for (bundle, jobs) in &by_bundle {
        let data =
            paint_bundle(&builder.read(bundle)?, jobs).with_context(|| bundle.to_string())?;
        builder.write(bundle, &data)?;
    }
    Ok(())
}

/// The palette `id` moves with while it isn't picked, if any.
pub fn leader(id: &str) -> Option<&'static Palette> {
    let (_, leader) = catalog::FOLLOWS.iter().find(|(f, _)| *f == id)?;
    palette(leader).ok()
}

/// `colors` with each palette that follows another, and isn't picked
/// itself, moved as its leader moved: the brows with the hair.
fn with_followers(colors: &Colors) -> Result<Colors> {
    let mut out = colors.clone();
    for &(follower, leader) in catalog::FOLLOWS {
        let Some(picked) = colors
            .get(leader)
            .filter(|_| !colors.contains_key(follower))
        else {
            continue;
        };
        let (leader, follower_palette) = (palette(leader)?, palette(follower)?);
        let to = picked.slots(leader)?[0];
        let first = oklch::follow(to, leader.vanilla[0], follower_palette.vanilla[0]);
        out.insert(follower.to_owned(), Picked::of(first));
    }
    Ok(out)
}

/// How far a param may sit from the colour recorded, per channel.
const PARAM_SLACK: f32 = 0.002;

/// `model` with each of its param jobs written, each checked first.
fn paint_model(model: &[u8], jobs: &[Job]) -> Result<Vec<u8>> {
    let mut data = model.to_vec();
    for job in jobs {
        let Kind::Param { material, hash, .. } = job.write.kind else {
            unreachable!("a model's jobs are params");
        };
        let old = check_param(&data, job.write).with_context(|| job.palette.to_owned())?;
        data = set_material_param(&data, material, hash, param_value(job.goal, old))?;
    }
    Ok(data)
}

/// Fails unless `write`'s param in `model` holds the colour recorded, and
/// returns what it holds.
fn check_param(model: &[u8], write: &Write) -> Result<[f32; 4]> {
    let Kind::Param {
        material,
        hash,
        vanilla,
        ..
    } = write.kind
    else {
        anyhow::bail!("not a param");
    };
    let old =
        material_param(model, material, hash).with_context(|| format!("{material} {hash:016x}"))?;
    ensure!(
        old.iter()
            .zip(vanilla)
            .all(|(o, v)| (o - v).abs() <= PARAM_SLACK),
        "{material} {hash:016x} holds {:?} where {vanilla:?} was recorded",
        &old[..3]
    );
    Ok(old)
}

/// `bundle` with each of its effect jobs written, each checked first.
fn paint_bundle(bundle: &[u8], jobs: &[Job]) -> Result<Vec<u8>> {
    let mut data = bundle.to_vec();
    for job in jobs {
        for c in check_vfx(bundle, job.write).with_context(|| job.palette.to_owned())? {
            let [r, g, b, _] = param_value(job.goal, c.value);
            vfx::set_constant(&mut data, c.at, [r, g, b]);
        }
    }
    Ok(data)
}

/// The constants `write` changes in `bundle`: every one with its hash
/// holding its colour. Fails when there's none.
fn check_vfx(bundle: &[u8], write: &Write) -> Result<Vec<vfx::Constant>> {
    let Kind::Vfx { hash, vanilla, .. } = write.kind else {
        anyhow::bail!("not an effect constant");
    };
    let found: Vec<_> = vfx::constants(bundle)?
        .into_iter()
        .filter(|c| c.hash == hash)
        .collect();
    let held: Vec<_> = found
        .iter()
        .filter(|c| {
            c.value
                .iter()
                .zip(vanilla)
                .all(|(o, v)| (o - v).abs() <= PARAM_SLACK)
        })
        .cloned()
        .collect();
    ensure!(
        !held.is_empty(),
        "no effect constant {hash:016x} holds {vanilla:?}: {:?}",
        found.iter().map(|c| &c.value[..3]).collect::<Vec<_>>()
    );
    Ok(held)
}

/// The param that shows `goal` at the brightness `old` had: its colour
/// from `goal`, its brightest channel from `old`. Black stays black.
fn param_value(goal: Colour, old: [f32; 4]) -> [f32; 4] {
    let lin = oklch::to_linear(goal);
    let top = lin.iter().copied().fold(0.0, f32::max);
    let keep = old[..3].iter().copied().fold(0.0, f32::max);
    let scale = if top > 0.0 { keep / top } else { 0.0 };
    [lin[0] * scale, lin[1] * scale, lin[2] * scale, old[3]]
}

/// How far a part's average may sit from the dump's before it's taken for
/// a different texture: a hue a few degrees, a channel a few steps.
const HUE_SLACK: f32 = 0.02;
const SV_SLACK: f32 = 0.04;
const RGB_SLACK: u8 = 6;

fn paint_texture(builder: &mut ModBuilder, texture: &Tex, jobs: &[Job]) -> Result<()> {
    let data = builder.read(texture.path)?;
    let mut rgba = top_level(&data)?.rgba;
    paint_jobs(&mut rgba, jobs)?;
    let drawn = redraw(&data, &rgba, |_, _| false)?;
    let copy = texture.copy.as_ref();
    let names = copy.map(|c| (c.mark, copies(texture.path, c).collect()));
    write_painted(builder, texture.path, &drawn, names)?;
    if let Some(metal) = texture.metal {
        let data = builder.read(metal)?;
        let mut rgba = top_level(&data)?.rgba;
        paint::matte(&mut rgba);
        let drawn = redraw(&data, &rgba, |_, _| false)?;
        // A copied albedo's metal map is shared too, so it's copied with it.
        let names = copy.map(|c| (c.mark, vec![(name(metal), c.models)]));
        write_painted(builder, metal, &drawn, names)?;
    }
    Ok(())
}

/// A name a texture goes by, with the models that name it.
type Named<'a> = (&'a str, &'static [&'static str]);

/// A painted texture, written in place, or as a copy: each name its
/// models know it by (with its mark and the models) gets the copy under
/// its own marked name, renamed in those models. The file inside keeps
/// the copy's name: the texture table already serves one file under
/// another name.
fn write_painted(
    builder: &mut ModBuilder,
    path: &str,
    drawn: &[u8],
    copy: Option<(u8, Vec<Named>)>,
) -> Result<()> {
    let Some((mark, names)) = copy else {
        return builder.write(path, drawn);
    };
    let (name, to) = (name(path), marked(name(path), mark)?);
    let drawn = rename(drawn, name, &to)?;
    for (alias, models) in names {
        let alias_to = marked(alias, mark)?;
        builder.add(&path.replace(name, &alias_to), &drawn, path)?;
        for model in models {
            let data = builder.read(model)?;
            builder.write(model, &rename(&data, alias, &alias_to)?)?;
        }
    }
    Ok(())
}

/// Each of one texture's jobs checked against what `rgba` holds, then
/// painted into it. The seed and the app's preview both paint here.
fn paint_jobs(rgba: &mut [u8], jobs: &[Job]) -> Result<()> {
    for job in jobs {
        check(rgba, job.write).with_context(|| job.palette.to_owned())?;
    }
    let bands: Vec<Band> = jobs
        .iter()
        .filter_map(|j| match j.write.kind {
            Kind::Band { hues, .. } => Some(Band {
                hues,
                to: paint::hsv(j.goal),
            }),
            _ => None,
        })
        .collect();
    let tints: Vec<_> = jobs
        .iter()
        .filter(|j| matches!(j.write.kind, Kind::Tint { .. }))
        .collect();
    ensure!(
        tints.is_empty() || (bands.is_empty() && tints.len() == 1),
        "a tint shares its texture"
    );
    paint::shift(rgba, &bands);
    if let [tint] = tints[..] {
        let colour = brightened(tint.goal, mean_luma(rgba));
        paint::tint(rgba, colour);
    }
    Ok(())
}

/// Fails unless `write`'s part in `rgba` is the colour the catalog
/// recorded: a newer dump is refused, not painted wrong.
fn check(rgba: &[u8], write: &Write) -> Result<()> {
    let (got, want) = (measure(rgba, write)?, write.vanilla());
    ensure!(
        matches(write, got, want),
        "the dump's colour here is {} where {} was recorded",
        hex(got),
        hex(want)
    );
    Ok(())
}

/// What a write's part averages in `rgba`: its band's texels, or every
/// texel for a tint.
fn measure(rgba: &[u8], write: &Write) -> Result<Colour> {
    match write.kind {
        Kind::Band { hues, .. } => {
            let (h, s, v) = paint::mean(rgba, hues).context("no texel in the band")?;
            Ok(paint::rgb(h, s, v))
        }
        Kind::Tint { .. } => Ok(mean(rgba)),
        Kind::Param { .. } | Kind::Vfx { .. } => anyhow::bail!("only a texture has texels"),
    }
}

fn matches(write: &Write, got: Colour, want: Colour) -> bool {
    match write.kind {
        Kind::Band { .. } => {
            let ((gh, gs, gv), (wh, ws, wv)) = (paint::hsv(got), paint::hsv(want));
            let dh = (gh - wh).rem_euclid(1.0);
            dh.min(1.0 - dh) <= HUE_SLACK
                && (gs - ws).abs() <= SV_SLACK
                && (gv - wv).abs() <= SV_SLACK
        }
        Kind::Tint { .. } | Kind::Param { .. } | Kind::Vfx { .. } => got
            .iter()
            .zip(want)
            .all(|(g, w)| g.abs_diff(w) <= RGB_SLACK),
    }
}

/// The average colour of `rgba`.
fn mean(rgba: &[u8]) -> Colour {
    let n = (rgba.len() / 4).max(1) as u64;
    std::array::from_fn(|c| {
        let sum: u64 = rgba.chunks(4).map(|px| px[c] as u64).sum();
        (sum / n) as u8
    })
}

fn mean_luma(rgba: &[u8]) -> u32 {
    let n = (rgba.len() / 4).max(1) as u64;
    (rgba.chunks(4).map(|px| paint::luma(px) as u64).sum::<u64>() / n) as u32
}

/// The tint that brings a texture whose texels average `luma` to `goal`
/// on average: a tint scales by brightness, so a dark texture needs a
/// brighter one.
fn brightened(goal: Colour, luma: u32) -> Colour {
    goal.map(|c| (c as u32 * 255 / luma.max(1)).min(255) as u8)
}

/// Each name a copied texture goes by, its own first, with the models
/// that name it.
pub(crate) fn copies<'a>(
    path: &'a str,
    copy: &'a Copy,
) -> impl Iterator<Item = (&'a str, &'static [&'static str])> {
    [(name(path), copy.models)]
        .into_iter()
        .chain(copy.aliases.iter().copied())
}

/// The texture at `path`, as its name.
pub(crate) fn name(path: &str) -> &str {
    let name = path.strip_prefix("Tex/").unwrap_or(path);
    name.strip_suffix(".bntx.zs").unwrap_or(name)
}

/// `name` with the letter before its `_Alb` (or a gradient's `_Gn0`, a
/// metal map's `_Mtl`) made `mark`: a copy's name, as long as the
/// texture's, as a rename needs.
pub(crate) fn marked(name: &str, mark: u8) -> Result<String> {
    let at = name.len().checked_sub(5).context("too short")?;
    ensure!(
        matches!(&name[at + 1..], "_Alb" | "_Gn0" | "_Mtl"),
        "{name} isn't an albedo, a gradient or a metal map"
    );
    Ok(format!(
        "{}{}{}",
        &name[..at],
        mark as char,
        &name[at + 1..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_has_a_unique_id_and_its_slots() {
        let mut ids: Vec<_> = PALETTES.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PALETTES.len());
        assert!(
            PALETTES
                .iter()
                .all(|p| !p.vanilla.is_empty() && !p.writes.is_empty())
        );
        for p in PALETTES {
            let slots = p.vanilla.len();
            let named = if slots == 1 { 0 } else { slots };
            assert_eq!(p.names.len(), named, "{} names each of its slots", p.id);
            for slot in 0..slots {
                assert!(
                    p.writes.iter().any(|w| w.slot == slot),
                    "{} slot {slot}",
                    p.id
                );
            }
            assert!(p.writes.iter().all(|w| w.slot < slots), "{}", p.id);
        }
        for &(follower, leader) in catalog::FOLLOWS {
            assert!(palette(follower).is_ok() && palette(leader).is_ok());
        }
    }

    #[test]
    fn a_param_keeps_its_brightest_channel() {
        let old = [0.5, 0.169, 0.0174, 1.0];
        let blue = param_value([0, 0, 255], old);
        assert_eq!(blue, [0.0, 0.0, 0.5, 1.0]);
        let grey = param_value([128, 128, 128], old);
        assert!(grey[..3].iter().all(|&c| (c - 0.5).abs() < 1e-6));
        assert_eq!(param_value([0, 0, 0], old), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn the_brows_follow_the_hair_unless_picked() {
        let hair = Picked::of([40, 60, 200]);
        let colors: Colors = [("hair".to_owned(), hair.clone())].into();
        let brows = with_followers(&colors).unwrap()["brows"].slots(palette("brows").unwrap());
        // The brows turn by the angle the hair turned.
        let hue = |c| oklch::lch(c).h;
        let turn = |to, from| (hue(to) - hue(from)).rem_euclid(1.0);
        let hair_turn = turn([40, 60, 200], palette("hair").unwrap().vanilla[0]);
        let brows_turn = turn(brows.unwrap()[0], palette("brows").unwrap().vanilla[0]);
        assert!(
            (brows_turn - hair_turn).abs() < 0.02,
            "{brows_turn} against {hair_turn}"
        );
        let own = Picked::of([200, 20, 20]);
        let both: Colors = [("hair".to_owned(), hair), ("brows".to_owned(), own.clone())].into();
        assert_eq!(with_followers(&both).unwrap()["brows"], own);
        assert!(
            !with_followers(&Colors::new())
                .unwrap()
                .contains_key("brows")
        );
    }

    #[test]
    fn a_copy_is_named_as_long_as_its_texture() {
        assert_eq!(marked("Fake_Tile_A_Alb", b'K').unwrap(), "Fake_Tile_K_Alb");
        assert_eq!(marked("Fake_Hair_A_Gn0", b'L').unwrap(), "Fake_Hair_L_Gn0");
        assert_eq!(marked("Fake_Tile_A_Mtl", b'S').unwrap(), "Fake_Tile_S_Mtl");
        assert!(marked("Fake_Tile_A_Nrm", b'K').is_err());
        assert_eq!(name("Tex/Fake_Tile_A_Alb.bntx.zs"), "Fake_Tile_A_Alb");
    }

    #[test]
    fn a_dark_texture_takes_a_brighter_tint() {
        assert_eq!(brightened([100, 50, 20], 128), [199, 99, 39]);
        assert_eq!(brightened([200, 50, 20], 64), [255, 199, 79]);
    }

    /// Every palette paints: each texture rewritten in its colour, a
    /// shared one as a copy Link's models name instead.
    #[test]
    fn every_palette_paints_its_textures() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-colors-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let pink = [230, 80, 170];
        let colors: Colors = PALETTES
            .iter()
            .map(|p| (p.id.to_owned(), Picked::of(pink)))
            .collect();
        write(&mut builder, &colors).unwrap();
        builder.finish().unwrap();
        let has = |data: &[u8], name: &str| data.windows(name.len()).any(|w| w == name.as_bytes());
        for palette in PALETTES {
            for (i, w) in palette.writes.iter().enumerate() {
                if let Kind::Param {
                    models,
                    material,
                    hash,
                    vanilla,
                } = w.kind
                {
                    for model in models {
                        let data = crate::game::read_built(&romfs, &dir, model);
                        let got = material_param(&data, material, hash).unwrap();
                        // White (the hair's shine) stays white under a
                        // pick brighter than the slot's vanilla.
                        if vanilla != [1.0; 3] {
                            assert_ne!(got[..3], vanilla, "{} {model} {material}", palette.id);
                        }
                        if i == 0 {
                            let got = oklch::from_linear([got[0], got[1], got[2]]);
                            let near = (paint::hsv(got).0 - paint::hsv(pink).0).abs();
                            assert!(near < 0.03, "{} {material} is {got:?}", palette.id);
                        }
                    }
                    continue;
                }
                if let Kind::Vfx { bundle, .. } = w.kind {
                    let data = crate::game::read_built(&romfs, &dir, bundle);
                    assert!(
                        check_vfx(&data, w).is_err(),
                        "{} {bundle} is painted",
                        palette.id
                    );
                    if i == 0 {
                        let vanilla = romfs.read(bundle).unwrap();
                        let at = check_vfx(&vanilla, w).unwrap()[0].at;
                        let c = &vfx::constants(&data).unwrap();
                        let got = c.iter().find(|c| c.at == at).unwrap().value;
                        let top = got[..3].iter().copied().fold(0.0, f32::max);
                        let got = oklch::from_linear([got[0] / top, got[1] / top, got[2] / top]);
                        let near = (paint::hsv(got).0 - paint::hsv(pink).0).abs();
                        assert!(near < 0.03, "{} {bundle} is {got:?}", palette.id);
                    }
                    continue;
                }
                let texture = w.texture().unwrap();
                let path = match &texture.copy {
                    None => texture.path.to_owned(),
                    Some(copy) => {
                        let name = name(texture.path);
                        for (alias, models) in copies(texture.path, copy) {
                            let to = marked(alias, copy.mark).unwrap();
                            let path = texture.path.replace(name, &to);
                            assert!(dir.join("romfs").join(&path).exists(), "{path} is written");
                            for model in models {
                                let data = crate::game::read_built(&romfs, &dir, model);
                                assert!(
                                    has(&data, &to) && !has(&data, alias),
                                    "{model} names {to}"
                                );
                            }
                        }
                        if let Some(metal) = texture.metal {
                            let to = marked(super::name(metal), copy.mark).unwrap();
                            assert!(
                                dir.join("romfs")
                                    .join(metal.replace(super::name(metal), &to))
                                    .exists()
                            );
                            assert!(
                                !dir.join("romfs").join(metal).exists(),
                                "{metal} is left alone"
                            );
                            for model in copy.models {
                                let data = crate::game::read_built(&romfs, &dir, model);
                                assert!(has(&data, &to), "{model} names {to}");
                            }
                        }
                        let to = marked(name, copy.mark).unwrap();
                        assert!(
                            !dir.join("romfs").join(texture.path).exists(),
                            "{name} is left alone"
                        );
                        texture.path.replace(name, &to)
                    }
                };
                let rgba = top_level(&crate::game::read_built(&romfs, &dir, &path))
                    .unwrap()
                    .rgba;
                let vanilla = top_level(&romfs.read(texture.path).unwrap()).unwrap().rgba;
                assert_ne!(rgba, vanilla, "{} {path} is painted", palette.id);
                if let (0, Kind::Band { .. }) = (i, &w.kind) {
                    // The palette's first part takes the colour itself.
                    let pink_hue = paint::hsv(pink).0;
                    let near = (pink_hue - 0.02, pink_hue + 0.02);
                    assert!(paint::mean(&rgba, near).is_some(), "{} {path}", palette.id);
                }
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }

    /// Every write finds its part in the dump as recorded, and every
    /// palette paints without error.
    #[test]
    fn every_write_finds_its_vanilla_colour() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut wrong = Vec::new();
        for palette in PALETTES {
            for write in palette.writes {
                if let Kind::Param { models, .. } = write.kind {
                    for model in models {
                        let checked = check_param(&romfs.read(model).unwrap(), write);
                        if let Err(e) = checked {
                            wrong.push(format!("{} {model}: {e:#}", palette.id));
                        }
                    }
                    continue;
                }
                if let Kind::Vfx { bundle, .. } = write.kind {
                    if let Err(e) = check_vfx(&romfs.read(bundle).unwrap(), write) {
                        wrong.push(format!("{} {bundle}: {e:#}", palette.id));
                    }
                    continue;
                }
                let path = write.texture().unwrap().path;
                let rgba = top_level(&romfs.read(path).unwrap()).unwrap().rgba;
                match measure(&rgba, write) {
                    Ok(got) if matches(write, got, write.vanilla()) => {}
                    Ok(got) => wrong.push(format!("{} {path}: {}", palette.id, hex(got))),
                    Err(e) => wrong.push(format!("{} {path}: {e}", palette.id)),
                }
            }
        }
        assert!(wrong.is_empty(), "measured:\n{}", wrong.join("\n"));
    }
}
