//! Skeletal poses and the model shapes they move: what a shelf pose does to
//! an item, and how low the item then reaches.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use ounce_formats::{ActorPack, Base, Romfs};

use super::unpacked;
use crate::known::{actor, field, hash};
use crate::medallions;
use crate::standins::{self, Tables};

/// The animation a shelf pose plays, the first an archive has.
const POSES: [u64; 2] = [field::POSE_SHOP.0, field::POSE_COMMON.0];
const MODELS: &str = "Model/";
const ARCHIVE: &str = ".anim.bfres.zs";
const MODEL: &str = ".bfres.zs";
/// Rupees aren't in the pool as their chest forms, but stand in for it.
const RUPEES: [&crate::known::Name; 3] = [
    actor::T_BOX_RUPEE_BLUE,
    actor::T_BOX_RUPEE_GREEN,
    actor::T_BOX_RUPEE_RED,
];

pub(super) fn poses(romfs: &Romfs, game: &Path, path: &str) -> Result<()> {
    unpacked(romfs, game, path, |path, data, _| {
        println!("{path}");
        for anim in ounce_formats::animation_names(data) {
            let bones = ounce_formats::bases(data, &anim)?.unwrap_or_default();
            println!("  {anim}");
            for (bone, b) in bones.iter().filter(|(_, b)| *b != Default::default()) {
                println!(
                    "    {bone} S{:?} R{:?} T{:?}",
                    b.scale, b.rotate, b.translate
                );
            }
        }
        Ok(())
    })
}

pub(super) fn bounds(romfs: &Romfs, game: &Path, path: &str, pose: Option<&[f32]>) -> Result<()> {
    let pose = pose.map(|p| Base {
        scale: Some([p[3]; 3]),
        rotate: Some([p[0], p[1], p[2], 1.0]),
        translate: Some([0.0, p[4], 0.0]),
    });
    unpacked(romfs, game, path, |path, data, _| {
        println!("{path}");
        for shape in ounce_formats::shape_names(data) {
            let points = ounce_formats::positions(data, &shape)?;
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for p in &points {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            println!("  {shape} {lo:.3?} {hi:.3?}");
            if let Some(pose) = &pose {
                println!("    posed lowest {:.3}", lowest(&points, pose));
            }
        }
        Ok(())
    })
}

/// Prints `shops/floors.rs`: each pool item's lowest point on a shelf, in
/// the pose `standins::shelf_pose` gives it there.
pub(super) fn floors(romfs: &Romfs, game: &Path) -> Result<()> {
    let tables = Tables::load(romfs)?;
    let mut items: Vec<String> = (crate::checks::all(romfs)?.into_iter())
        .map(|c| c.item)
        .collect();
    items.extend(RUPEES.map(|r| r.to_string()));
    items.sort();
    items.dedup();
    // Each item with the model file it shows, its row's model and its
    // shelf pose archive. A song shows the medallion it's painted on.
    let mut wanted = Vec::new();
    for item in items {
        let shown = match medallions::medallion(&item) {
            Some(_) => medallions::HOST.to_string(),
            None => item.clone(),
        };
        let file = standins::model_file(&ActorPack::load(romfs, &shown)?)?;
        let (Some((project, model)), Some(row_model)) =
            (file, standins::model(&tables.actor, &item)?)
        else {
            continue;
        };
        let pose = standins::vanilla_pose(&tables.actor, &item)?;
        wanted.push((item, format!("{project}.{model}{MODEL}"), row_model, pose));
    }
    let files: HashSet<&str> = wanted.iter().map(|(_, f, _, _)| f.as_str()).collect();
    let mut archives: HashSet<String> = wanted.iter().map(|(.., p)| p.clone()).collect();
    archives.extend(wanted.iter().map(|(_, _, m, _)| standins::own_pose(m)));
    archives.insert(standins::stand_pose());

    // One pass over the dump's models.
    let mut meshes: HashMap<String, (Vec<[f32; 3]>, bool)> = HashMap::new();

    let mut roots: HashMap<String, Base> = HashMap::new();
    crate::game::each(game, MODELS, |path, data| {
        let Some(file) = path.strip_prefix(MODELS) else {
            return Ok(());
        };
        if let Some(stem) = file.strip_suffix(ARCHIVE) {
            if archives.contains(stem) {
                let data = romfs.dicts().decompress(&path, &data)?;
                roots.insert(stem.into(), shelf_root(&data)?);
            }
        } else if files.contains(file) {
            let data = romfs.dicts().decompress(&path, &data)?;
            let (points, unread) = meshes.entry(file.into()).or_default();
            for shape in ounce_formats::shape_names(&data) {
                match ounce_formats::positions(&data, &shape) {
                    Ok(p) => points.extend(p),
                    Err(_) => *unread = true,
                }
            }
        }
        Ok(())
    })?;

    let lift = |archive: &str| {
        roots
            .get(archive)
            .and_then(|b| b.translate)
            .map_or(0.0, |t| t[1])
    };
    let mut floors = BTreeMap::new();
    let token = actor::OBJ_TOKEN_GOLD_ANIMATION.to_string();
    for (item, file, model, pose) in &wanted {
        let mesh = match meshes.get(file) {
            Some((points, false)) if !points.is_empty() => Some(points),
            _ => None,
        };
        let stands = *pose == token && mesh.is_some_and(|m| flat(m));
        let pose = match stands {
            true => standins::stand_pose(),
            false => pose.clone(),
        };
        let played = roots.get(&pose).copied().unwrap_or_default();
        let floor = match mesh {
            Some(points) => lowest(points, &played),
            // The row's floor is the model in its own pose for these.
            _ => {
                eprintln!("{item}: vertices unread, from its row");
                standins::table_floor(&tables.actor, item)? - lift(&standins::own_pose(model))
                    + lift(&pose)
            }
        };
        if stands {
            eprintln!("{item}: flat, stands up");
        }
        floors.insert(hash(item), (floor, stands));
    }
    print!("{HEADER}");
    println!("const FLOORS: [(Name, f32, bool); {}] = [", floors.len());
    for (h, (floor, stands)) in floors {
        println!("    (n!({h:#018x}), {floor:.4}, {stands}),");
    }
    println!("];\n{FOOTER}");
    Ok(())
}

const HEADER: &str = "\
//! How far above its origin each pool item's model reaches down on a
//! shelf, in the pose it plays there: its lowest vertex once the pose has
//! scaled, turned and lifted the skeleton root. Written by `zootr probe
//! floors`, from the dump: rerun it when the pool or the poses change. A
//! model whose vertices it can't read takes its `ActorInfo` floor, less
//! its own pose's lift, plus the shelf pose's. A model lying flat in the
//! token's pose stands up in the Deku Shield's instead.

use crate::known::{Name, hash};

/// By the item name's hash, in hash order, with whether the item stands up.
";

const FOOTER: &str = "
/// `item`'s floor on a shelf, if the table has it.
pub fn floor(item: &str) -> Option<f32> {
    row(item).map(|r| r.1)
}

/// Whether `item`'s model lies flat, so stands up on a shelf.
pub fn stands(item: &str) -> bool {
    row(item).is_some_and(|r| r.2)
}

fn row(item: &str) -> Option<&'static (Name, f32, bool)> {
    let at = FLOORS.binary_search_by_key(&hash(item), |(n, ..)| n.0).ok()?;
    Some(&FLOORS[at])
}";

/// The skeleton root's values in the shelf pose an archive holds, or none
/// where it leaves the root alone.
fn shelf_root(data: &[u8]) -> Result<Base> {
    let names = ounce_formats::animation_names(data);
    let Some(anim) = POSES
        .iter()
        .find_map(|p| names.iter().find(|n| hash(n) == *p))
    else {
        return Ok(Base::default());
    };
    let bones = ounce_formats::bases(data, anim)?.unwrap_or_default();
    let root = bones
        .into_iter()
        .find(|(n, _)| hash(n) == field::SKL_ROOT.0);
    Ok(root.map(|(_, b)| b).unwrap_or_default())
}

/// Whether `points` lie flat: thinner upright than half their narrower
/// side across.
fn flat(points: &[[f32; 3]]) -> bool {
    let extent = |i: usize| {
        let (lo, hi) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p[i]), hi.max(p[i]))
        });
        hi - lo
    };
    extent(1) < 0.5 * extent(0).min(extent(2))
}

/// The lowest of `points` once `pose` scales, turns (Euler X, Y, Z) and
/// lifts them.
fn lowest(points: &[[f32; 3]], pose: &Base) -> f32 {
    let scale = pose.scale.unwrap_or([1.0; 3]);
    let [x, y, z, _] = pose.rotate.unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    // Row Y of Rz * Ry * Rx.
    let row = [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz];
    let low = (points.iter())
        .map(|p| (0..3).map(|i| row[i] * p[i] * scale[i]).sum::<f32>())
        .fold(f32::MAX, f32::min);
    low + pose.translate.map_or(0.0, |t| t[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plate_lies_flat_and_a_rod_or_a_post_does_not() {
        let plate = [[-1.0, 0.0, -1.0], [1.0, 0.2, 1.0]];
        let rod = [[-1.0, 0.0, -0.1], [1.0, 0.1, 0.1]];
        let post = [[-0.1, 0.0, -0.1], [0.1, 1.0, 0.1]];
        assert!(flat(&plate));
        assert!(!flat(&rod));
        assert!(!flat(&post));
    }

    #[test]
    fn a_pose_scales_turns_and_lifts_the_lowest_point() {
        let points = [[0.0, -0.1, 0.0], [0.0, 0.2, -0.5]];
        assert_eq!(lowest(&points, &Base::default()), -0.1);
        let lifted = Base {
            scale: Some([0.5; 3]),
            translate: Some([0.0, 0.3, 0.0]),
            ..Base::default()
        };
        assert!((lowest(&points, &lifted) - 0.25).abs() < 1e-6);
        // A quarter turn about X stands the far point (z -0.5) up: y = -z.
        let turned = Base {
            rotate: Some([std::f32::consts::FRAC_PI_2, 0.0, 0.0, 1.0]),
            ..Base::default()
        };
        assert!(lowest(&points, &turned).abs() < 1e-6);
    }
}
