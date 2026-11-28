//! `probe heights`, which writes `heights/table.rs`, and `probe shelves`,
//! which shows what the table makes of every shop slot.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Base, Romfs};

use crate::heights::{self, Frame, Kind, Role, Shown};
use crate::known::{actor, field, hash};
use crate::medallions;
use crate::progressive;
use crate::standins::{self, Tables};

const MODELS: &str = "Model/";
const ARCHIVE: &str = ".anim.bfres.zs";
const MODEL: &str = ".bfres.zs";
/// The bone every pose moves, and the one under it.
const ROOT: &str = "Root";
/// Rupees aren't in the pool as their chest forms, but stand in for it.
const RUPEES: [&crate::known::Name; 3] = [
    actor::T_BOX_RUPEE_BLUE,
    actor::T_BOX_RUPEE_GREEN,
    actor::T_BOX_RUPEE_RED,
];
/// Frames closer than this, in every term, are one.
const SAME: f32 = 1e-4;

/// Every holder the build can show, with the actor whose `ActorInfo` box
/// stands in when its model has no readable vertices.
fn universe(romfs: &Romfs, tables: &Tables) -> Result<BTreeMap<Shown, String>> {
    let checks = crate::checks::all(romfs)?;
    let slots = crate::shops::slots(romfs)?;
    let mut items: Vec<String> = checks.iter().map(|c| c.item.clone()).collect();
    items.extend(RUPEES.map(|r| r.to_string()));
    let levels = progressive::LINES.iter().flat_map(|l| l.iter());
    items.extend(levels.map(|(level, _)| level.to_string()));
    items.sort();
    items.dedup();
    let templates: Vec<ActorPack> = crate::pickups::TEMPLATES
        .iter()
        .map(|t| ActorPack::load(romfs, t))
        .collect::<Result<_>>()?;
    let goods: Vec<ActorPack> = (slots.iter().map(|s| s.goods.as_str()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|g| ActorPack::load(romfs, g))
        .collect::<Result<_>>()?;
    let mut out = BTreeMap::new();
    let mut add = |shown: Shown, row: &str| {
        out.entry(shown).or_insert_with(|| row.to_string());
    };
    // Each actor as it stands placed: the templates, the goods, the
    // holders a placed item anchors to and each item's own pickup.
    let mut placed: BTreeSet<String> = checks.iter().map(|c| c.holder.clone()).collect();
    placed.extend(
        (items.iter())
            .flat_map(|i| [false, true].map(|d| crate::pickups::pickup_for(i, d)))
            .flatten(),
    );
    for name in placed {
        let Ok(pack) = ActorPack::load(romfs, &name) else {
            continue;
        };
        if let Ok(shown) = heights::shown(&pack, &pack, Role::Rest, None) {
            add(shown, &name);
        }
    }
    for pack in templates.iter() {
        add(heights::shown(pack, pack, Role::Rest, None)?, &pack.name);
    }
    for pack in &goods {
        add(heights::shown(pack, pack, Role::Shelf, None)?, &pack.name);
    }
    // Each item as a stand-in: on each template, and on a shelf in each
    // pose the shelf may give it.
    for item in &items {
        let (pack, row) = match (medallions::medallion(item), progressive::worn(item)) {
            (Some(_), _) => (ActorPack::load(romfs, item)?, item.clone()),
            (None, Some(worn)) => (ActorPack::load(romfs, worn)?, worn.to_string()),
            (None, None) => (ActorPack::load(romfs, item)?, item.clone()),
        };
        for template in &templates {
            add(heights::shown(template, &pack, Role::Rest, None)?, &row);
        }
        let vanilla = standins::vanilla_pose(&tables.actor, &row)?;
        for pose in [
            vanilla,
            standins::stand_pose(),
            standins::TOKEN_POSE.to_string(),
        ] {
            add(
                heights::shown(&goods[0], &pack, Role::Shelf, Some(&pose))?,
                &row,
            );
        }
    }
    Ok(out)
}

/// Prints `heights/table.rs`: each shown model's hull and each pose's
/// frames. What it couldn't read goes to stderr.
pub(super) fn heights(romfs: &Romfs, game: &Path) -> Result<()> {
    let tables = Tables::load(romfs)?;
    let shown = universe(romfs, &tables)?;
    // Each model to read, by the file it reads: a song's medallion is the
    // host's mesh.
    let host = {
        let pack = ActorPack::load(romfs, medallions::HOST)?;
        let (project, model) = standins::model_file(&pack)?.context("the host shows no model")?;
        format!("{project}.{model}")
    };
    let mut files: HashMap<String, Vec<(&str, &str)>> = HashMap::new();
    for (s, row) in &shown {
        let file = match s.model.starts_with(medallions::HOST.stem()) && s.model.contains("Song") {
            true => host.clone(),
            false => s.model.clone(),
        };
        files
            .entry(format!("{file}{MODEL}"))
            .or_default()
            .push((&s.model, row));
    }
    let archives: BTreeSet<&str> = shown
        .keys()
        .flat_map(|s| s.archives.iter().map(String::as_str))
        .collect();

    let dirs = heights::directions();
    let mut models: BTreeMap<u64, (Kind, Vec<[i16; 3]>)> = BTreeMap::new();
    let mut clips: HashMap<String, Vec<u8>> = HashMap::new();
    crate::game::each(game, MODELS, |path, data| {
        let Some(file) = path.strip_prefix(MODELS) else {
            return Ok(());
        };
        if let Some(stem) = file.strip_suffix(ARCHIVE) {
            if archives.contains(stem) {
                clips.insert(stem.into(), romfs.dicts().decompress(&path, &data)?);
            }
        } else if let Some(users) = files.get(file) {
            let data = romfs.dicts().decompress(&path, &data)?;
            let (kind, points) = mesh(&data)?;
            let hull = heights::extremes(&points, &dirs);
            if hull.is_empty() {
                return Ok(());
            }
            let mm: Vec<[i16; 3]> = hull
                .iter()
                .map(|p| p.map(|c| (c * 1000.0).round() as i16))
                .collect();
            for (model, _) in users {
                models.insert(hash(model), (kind, mm.clone()));
            }
        }
        Ok(())
    })?;
    // What no mesh answered: the box, else the origin.
    for (s, row) in &shown {
        if models.contains_key(&s.model_key()) {
            continue;
        }
        let (kind, points) = match corners(&tables, row) {
            Some(points) => (Kind::Box, points),
            None => (Kind::Missing, vec![[0.0; 3]]),
        };
        eprintln!("{}: {kind:?}, from {row}", s.model);
        let mm = points
            .iter()
            .map(|p| p.map(|c| (c * 1000.0).round() as i16))
            .collect();
        models.insert(s.model_key(), (kind, mm));
    }
    for s in shown.keys() {
        if let Some((Kind::Skinned, _)) = models.get(&s.model_key()) {
            eprintln!("{}: skinned, posed by its root alone", s.model);
        }
    }
    let mut poses: BTreeMap<u64, Vec<Frame>> = BTreeMap::new();
    for s in shown.keys() {
        if poses.contains_key(&s.pose_key()) {
            continue;
        }
        poses.insert(s.pose_key(), frames(&clips, &s.archives, s.role)?);
    }

    print!("{HEADER}");
    println!("#[rustfmt::skip]");
    println!(
        "pub(super) const MODELS: [(Name, u8, &[[i16; 3]]); {}] = [",
        models.len()
    );
    for (key, (kind, points)) in &models {
        let each: Vec<String> = points
            .iter()
            .map(|[x, y, z]| format!("[{x},{y},{z}]"))
            .collect();
        println!(
            "    (n!({key:#018x}), {}, &[{}]),",
            *kind as u8,
            each.join(",")
        );
    }
    println!("];");
    println!();
    println!("{POSES_DOC}");
    println!("#[rustfmt::skip]");
    println!(
        "pub(super) const POSES: [(Name, &[Frame]); {}] = [",
        poses.len()
    );
    for (key, frames) in &poses {
        let each: Vec<String> = (frames.iter())
            .map(|f| format!("[{}]", f.map(|v| format!("{v:.4}")).join(",")))
            .collect();
        println!("    (n!({key:#018x}), &[{}]),", each.join(","));
    }
    println!("];");
    Ok(())
}

const HEADER: &str = "\
//! Written by `zootr probe heights`, from the dump: rerun it when the pool,
//! the models or the poses change, never edit it by hand. See [`super`].
#![allow(clippy::approx_constant)]

use super::{Frame, Name};
use crate::n;

/// Each model's hull, by its file's hash, in hash order: what its row was
/// made from ([`super::Kind`]) and its points in millimetres.
";

const POSES_DOC: &str = "\
/// Each pose's root transform at every frame of its clip, by its key's
/// hash ([`super::pose_key`]), in hash order.";

/// A model's vertices in its own space, and whether a shape is skinned to
/// more than one bone (posed then by the root alone).
fn mesh(data: &[u8]) -> Result<(Kind, Vec<[f32; 3]>)> {
    let skeleton = ounce_formats::skeleton(data)?;
    let mut kind = Kind::Mesh;
    let mut points = Vec::new();
    for shape in ounce_formats::shapes_of(data)? {
        match ounce_formats::model_vertices(data, &skeleton, &shape.name) {
            Ok(placed) => points.extend(placed.into_iter().map(|(p, _)| p)),
            Err(e) => eprintln!("  {}: {e}", shape.name),
        }
        if shape.skin > 1 {
            kind = Kind::Skinned;
        }
    }
    Ok((kind, points))
}

/// An actor's `ActorInfo` model box's corners.
fn corners(tables: &Tables, actor: &str) -> Option<Vec<[f32; 3]>> {
    let row = standins::row(&tables.actor, actor).ok()?;
    let corner = |key: &crate::known::Name| -> Option<[f32; 3]> {
        let map = row.get(key.get())?.as_map().ok()?;
        Some(["X", "Y", "Z"].map(|a| map.get(a).and_then(standins::float).unwrap_or(0.0)))
    };
    let (lo, hi) = (
        corner(field::MODEL_AABB_MIN)?,
        corner(field::MODEL_AABB_MAX)?,
    );
    Some(
        (0..8)
            .map(|i| [0, 1, 2].map(|a| if i >> a & 1 == 0 { lo[a] } else { hi[a] }))
            .collect(),
    )
}

/// The root's transform at every frame of the clip `archives` play in
/// `role`: the first of its clips any archive has, the root's frames each
/// carrying the bone under it. No clip, and the root stays put.
fn frames(clips: &HashMap<String, Vec<u8>>, archives: &[String], role: Role) -> Result<Vec<Frame>> {
    let found = role.clips().iter().find_map(|clip| {
        archives.iter().find_map(|a| {
            let data = clips.get(a)?;
            let name = ounce_formats::animation_names(data)
                .into_iter()
                .find(|n| hash(n) == clip.0)?;
            Some((data, name))
        })
    });
    let Some((data, clip)) = found else {
        return Ok(vec![heights::frame([1.0; 3], [0.0; 3], [0.0; 3])]);
    };
    let bone = |name: &str| -> Result<Vec<Frame>> {
        let each = ounce_formats::fska::frames(data, &clip, name)?
            .unwrap_or_else(|| vec![Base::default()]);
        Ok(dedup(each.iter().map(to_frame).collect()))
    };
    let below = ounce_formats::bases(data, &clip)?
        .unwrap_or_default()
        .into_iter()
        .find(|(n, _)| hash(n) == field::SKL_ROOT.0)
        .map(|(n, _)| n);
    let (roots, under) = (
        bone(ROOT)?,
        match below {
            Some(n) => bone(&n)?,
            None => vec![to_frame(&Base::default())],
        },
    );
    Ok(dedup(
        (roots.iter())
            .flat_map(|r| under.iter().map(move |u| heights::chain(r, u)))
            .collect(),
    ))
}

fn to_frame(b: &Base) -> Frame {
    let r = b.rotate.unwrap_or_default();
    heights::frame(
        b.scale.unwrap_or([1.0; 3]),
        [r[0], r[1], r[2]],
        b.translate.unwrap_or_default(),
    )
}

fn dedup(frames: Vec<Frame>) -> Vec<Frame> {
    let mut out: Vec<Frame> = Vec::new();
    for f in frames {
        if !out
            .iter()
            .any(|o| o.iter().zip(&f).all(|(a, b)| (a - b).abs() < SAME))
        {
            out.push(f);
        }
    }
    out
}

/// Each shop slot as a seed stocks it: its goods' bottom and the gap up
/// to the shelf above, and with `try` what an item would do there, from
/// the table.
pub(super) fn shelves(romfs: &Romfs, tries: &[String], only: Option<&str>) -> Result<()> {
    let tables = Tables::load(romfs)?;
    let slots = crate::shops::slots(romfs)?;
    let gaps = crate::shops::gaps(romfs, &tables, &slots)?;
    for slot in slots
        .iter()
        .filter(|s| only.is_none_or(|name| s.shop == name))
    {
        let goods = ActorPack::load(romfs, &slot.goods)?;
        let turn = heights::euler(slot.rotate.unwrap_or_default());
        let hull =
            heights::shown(&goods, &goods, Role::Shelf, None)?.hull(&tables.actor, &slot.goods)?;
        let gap = gaps.get(&slot.hash()).copied();
        println!(
            "{} {}: {}  y {:.4}  rot {:.3?}  bottom {:+.4} ({:?})  gap {}",
            slot.shop,
            slot.number,
            slot.goods,
            slot.pos[1],
            slot.rotate.unwrap_or_default(),
            hull.lo(&turn),
            hull.kind,
            gap.map_or("none".into(), |g| format!("{g:.4}")),
        );
        for item in tries {
            let pack = ActorPack::load(romfs, item)?;
            let fit = standins::fit(&tables, &goods, &pack, slot.rotate.unwrap_or_default(), gap)?;
            println!(
                "    {item}: pose {}  rise {:+.4}  turn {}",
                fit.pose,
                fit.rise,
                fit.rotate.map_or("none".into(), |r| format!("{r:.4?}")),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    /// The table is fresh: every holder a seed can show finds its model,
    /// from its mesh or at least its box, and its pose. Nothing falls back
    /// to the origin.
    #[test]
    fn every_holder_a_seed_can_show_finds_its_rows() {
        let Some(romfs) = dev_dump() else { return };
        let tables = Tables::load(&romfs).unwrap();
        let lacking: Vec<String> = (universe(&romfs, &tables).unwrap().into_iter())
            .filter(|(s, _)| !matches!(heights::rows(s), (Some(k), true) if k != Kind::Missing))
            .map(|(s, row)| format!("{row}: {s:?} {:?}", heights::rows(&s)))
            .collect();
        assert!(
            lacking.is_empty(),
            "rerun `zootr probe heights`: {lacking:#?}"
        );
    }
}
