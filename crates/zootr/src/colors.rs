//! Colors: a seed's palettes recoloured, Link's tunics and gear first.
//! Romfs files only: the hook patch never differs by colour. See
//! `superpowers/specs/2026-11-24-colors-design.md`.

mod catalog;
mod files;
mod oklch;
pub mod paint;
mod pick;

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{ModBuilder, redraw, rename, top_level};

pub use catalog::{GREEN, Group, PALETTES, Palette, Write};
use files::Tex;
use paint::{Band, Colour};
pub use pick::{
    Colors, Draft, Mode, Pick, Picked, args, draft, hex, parse_arg, parse_hex, resolve,
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
    let textures = PALETTES.iter().flat_map(|p| p.writes).map(Write::texture);
    textures.flat_map(|t| {
        let models = t.copy.iter().flat_map(|c| c.models.iter().copied());
        [t.path].into_iter().chain(t.metal).chain(models)
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
struct Job<'a> {
    palette: &'a str,
    write: &'a Write,
    goal: Colour,
}

/// Paint every palette `colors` changes. Last of a seed's writes, so a
/// texture another feature copies (the child's tunics) is copied green.
pub fn write(builder: &mut ModBuilder, colors: &Colors) -> Result<()> {
    let mut by_texture: BTreeMap<&str, (&Tex, Vec<Job>)> = BTreeMap::new();
    for (id, picked) in colors {
        let palette = palette(id)?;
        let slots = picked.slots(palette)?;
        for write in palette.writes {
            let goal = oklch::follow(slots[0], palette.vanilla[0], write.vanilla());
            let texture = write.texture();
            let (_, jobs) = by_texture
                .entry(texture.path)
                .or_insert((texture, Vec::new()));
            jobs.push(Job {
                palette: palette.id,
                write,
                goal,
            });
        }
    }
    for (texture, jobs) in by_texture.values() {
        paint_texture(builder, texture, jobs).with_context(|| texture.path.to_owned())?;
    }
    Ok(())
}

/// How far a part's average may sit from the dump's before it's taken for
/// a different texture: a hue a few degrees, a channel a few steps.
const HUE_SLACK: f32 = 0.02;
const SV_SLACK: f32 = 0.04;
const RGB_SLACK: u8 = 6;

fn paint_texture(builder: &mut ModBuilder, texture: &Tex, jobs: &[Job]) -> Result<()> {
    let data = builder.read(texture.path)?;
    let mut rgba = top_level(&data)?.rgba;
    let measured: Vec<Colour> = jobs
        .iter()
        .map(|j| measure(&rgba, j.write))
        .collect::<Result<_>>()?;
    for (job, &got) in jobs.iter().zip(&measured) {
        let want = job.write.vanilla();
        ensure!(
            matches(job.write, got, want),
            "{}: the dump's colour here is {} where {} was recorded",
            job.palette,
            hex(got),
            hex(want)
        );
    }
    let bands: Vec<Band> = jobs
        .iter()
        .filter_map(|j| match j.write {
            Write::Band { hues, .. } => Some(Band {
                hues: *hues,
                to: paint::hsv(j.goal),
            }),
            Write::Tint { .. } => None,
        })
        .collect();
    let tints: Vec<_> = jobs
        .iter()
        .filter(|j| matches!(j.write, Write::Tint { .. }))
        .collect();
    ensure!(
        tints.is_empty() || (bands.is_empty() && tints.len() == 1),
        "a tint shares its texture"
    );
    paint::shift(&mut rgba, &bands);
    if let [tint] = tints[..] {
        let colour = brightened(tint.goal, mean_luma(&rgba));
        paint::tint(&mut rgba, colour);
    }
    let drawn = redraw(&data, &rgba, |_, _| false)?;
    match &texture.copy {
        None => builder.write(texture.path, &drawn)?,
        Some(copy) => {
            let (name, to) = (name(texture.path), marked(name(texture.path), copy.mark)?);
            let path = texture.path.replace(name, &to);
            builder.add(&path, &rename(&drawn, name, &to)?, texture.path)?;
            for model in copy.models {
                let data = builder.read(model)?;
                builder.write(model, &rename(&data, name, &to)?)?;
            }
        }
    }
    if let Some(metal) = texture.metal {
        let data = builder.read(metal)?;
        let mut rgba = top_level(&data)?.rgba;
        paint::matte(&mut rgba);
        builder.write(metal, &redraw(&data, &rgba, |_, _| false)?)?;
    }
    Ok(())
}

/// What a write's part averages in `rgba`: its band's texels, or every
/// texel for a tint.
fn measure(rgba: &[u8], write: &Write) -> Result<Colour> {
    match write {
        Write::Band { hues, .. } => {
            let (h, s, v) = paint::mean(rgba, *hues).context("no texel in the band")?;
            Ok(paint::rgb(h, s, v))
        }
        Write::Tint { .. } => Ok(mean(rgba)),
    }
}

fn matches(write: &Write, got: Colour, want: Colour) -> bool {
    match write {
        Write::Band { .. } => {
            let ((gh, gs, gv), (wh, ws, wv)) = (paint::hsv(got), paint::hsv(want));
            let dh = (gh - wh).rem_euclid(1.0);
            dh.min(1.0 - dh) <= HUE_SLACK
                && (gs - ws).abs() <= SV_SLACK
                && (gv - wv).abs() <= SV_SLACK
        }
        Write::Tint { .. } => got
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

/// The texture at `path`, as its name.
pub(crate) fn name(path: &str) -> &str {
    let name = path.strip_prefix("Tex/").unwrap_or(path);
    name.strip_suffix(".bntx.zs").unwrap_or(name)
}

/// `name` with the letter before its `_Alb` made `mark`: a copy's name,
/// as long as the texture's, as a rename needs.
pub(crate) fn marked(name: &str, mark: u8) -> Result<String> {
    let at = name.len().checked_sub(5).context("too short")?;
    ensure!(name[at + 1..] == *"_Alb", "{name} isn't an albedo");
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
    }

    #[test]
    fn a_copy_is_named_as_long_as_its_texture() {
        assert_eq!(marked("Fake_Tile_A_Alb", b'K').unwrap(), "Fake_Tile_K_Alb");
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
                let texture = w.texture();
                let path = match &texture.copy {
                    None => texture.path.to_owned(),
                    Some(copy) => {
                        let (name, to) = (
                            name(texture.path),
                            marked(name(texture.path), copy.mark).unwrap(),
                        );
                        for model in copy.models {
                            let data = crate::game::read_built(&romfs, &dir, model);
                            assert!(has(&data, &to) && !has(&data, name), "{model} names {to}");
                        }
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
                if let (0, Write::Band { .. }) = (i, w) {
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
                let path = write.texture().path;
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
