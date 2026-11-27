//! Skeletal poses and the model shapes they move: what a shelf pose does to
//! an item, and how low the item then reaches.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use ounce_formats::{ActorPack, Base, Romfs};

use super::unpacked;
use crate::known::{actor, field, hash};
use crate::medallions;
use crate::progressive;
use crate::standins::{self, Tables};

/// The animation a shelf pose plays, the first an archive has.
const POSES: [u64; 2] = [field::POSE_SHOP.0, field::POSE_COMMON.0];
/// The gap above a shelf's plank, from the spacing between a shop's rows
/// (0.6 m at the Kokiri stall, 0.7 at the Goron).
const HEADROOM: f32 = 0.7;
const MODELS: &str = "Model/";
const ARCHIVE: &str = ".anim.bfres.zs";
const MODEL: &str = ".bfres.zs";
/// Rupees aren't in the pool as their chest forms, but stand in for it.
const RUPEES: [&crate::known::Name; 3] = [
    actor::T_BOX_RUPEE_BLUE,
    actor::T_BOX_RUPEE_GREEN,
    actor::T_BOX_RUPEE_RED,
];

/// An actor with the model file it shows, its row's model and the shelf
/// pose archive it plays.
type Wanted = (String, String, String, String);
/// Each model's vertices, and whether a shape refused to read them.
type Meshes = HashMap<String, (Vec<[f32; 3]>, bool)>;
/// Each animation archive's skeleton root in the shelf pose it holds.
type Roots = HashMap<String, Base>;

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
        if path.contains(".bntx") {
            let (top, format) = (
                ounce_formats::top_level(data)?,
                ounce_formats::format(data)?,
            );
            let redraws = ounce_formats::redraws(format);
            println!(
                "  {} {}x{} format {format:#x} redraws {redraws}",
                top.name, top.width, top.height
            );
        }
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
            match ounce_formats::lods(data, &shape) {
                Ok(lods) => {
                    let each = lods
                        .iter()
                        .map(|l| format!("{}v/{}t", l.vertices.len(), l.indices.len() / 3));
                    println!("    lods {}", each.collect::<Vec<_>>().join(" "));
                }
                Err(e) => println!("    lods: {e}"),
            }
            let attributes = ounce_formats::attributes(data, &shape)?;
            let each = attributes.iter().map(|a| {
                let varies = if a.varies { "" } else { " same" };
                format!("{}@{} {:#x}{varies}", a.name, a.offset, a.format)
            });
            println!("    attributes {}", each.collect::<Vec<_>>().join(", "));
            if let Some(pose) = &pose {
                println!("    posed lowest {:.3}", lowest(&points, pose));
            }
        }
        Ok(())
    })
}

/// Prints `shops/floors/table.rs`: each pool item's lowest point on a
/// shelf, in the pose `standins::shelf_pose` gives it there, and every
/// level of each line, which a shelf's steps show.
pub(super) fn floors(romfs: &Romfs, game: &Path) -> Result<()> {
    let tables = Tables::load(romfs)?;
    let mut items: Vec<String> = (crate::checks::all(romfs)?.into_iter())
        .map(|c| c.item)
        .collect();
    items.extend(RUPEES.map(|r| r.to_string()));
    let levels = crate::progressive::LINES.iter().flat_map(|l| l.iter());
    items.extend(levels.map(|(level, _)| level.to_string()));
    items.sort();
    items.dedup();
    let wanted = wanted(romfs, &tables, items)?;
    let (meshes, roots) = models(romfs, game, &wanted)?;

    let lift = |archive: &str| {
        roots
            .get(archive)
            .and_then(|b| b.translate)
            .map_or(0.0, |t| t[1])
    };
    let mut floors = BTreeMap::new();
    let token = actor::OBJ_TOKEN_GOLD_ANIMATION.to_string();
    let leaning = roots
        .get(&standins::stand_pose())
        .copied()
        .unwrap_or_default();
    for (item, file, model, pose) in &wanted {
        let mesh = match meshes.get(file) {
            Some((points, false)) if !points.is_empty() => Some(points),
            _ => None,
        };
        // A flat model leans back only while the lean fits the gap to the
        // shelf above it. A 1.3 m sword doesn't, so it lies flat like the
        // game leaves it.
        let stands =
            *pose == token && mesh.is_some_and(|m| flat(m) && height(m, &leaning) <= HEADROOM);
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
    println!(
        "pub(super) const FLOORS: [(Name, f32, bool); {}] = [",
        floors.len()
    );
    for (h, (floor, stands)) in floors {
        println!("    (n!({h:#018x}), {floor:.4}, {stands}),");
    }
    println!("];");
    Ok(())
}

/// Each shop slot's shelf, read from the dump: where its goods' placement
/// sits, how low the goods' own model reaches there, the plank that follows,
/// and the gap up to the plank above. With `try`, what an item would do at
/// that slot: the steepest turn that fits the gap, and the rise that sets it
/// on the plank.
pub(super) fn shelves(
    romfs: &Romfs,
    game: &Path,
    tries: &[String],
    only: Option<&str>,
) -> Result<()> {
    let tables = Tables::load(romfs)?;
    let all = crate::shops::slots(romfs)?;
    let slots: Vec<&crate::shops::Slot> = all
        .iter()
        .filter(|s| only.is_none_or(|name| s.shop == name))
        .collect();
    let names: Vec<String> = (slots.iter())
        .map(|s| s.goods.clone())
        .chain(tries.iter().cloned())
        .collect();
    let wanted = wanted(romfs, &tables, names)?;
    let (meshes, roots) = models(romfs, game, &wanted)?;
    // An actor's shelf pose and its model's vertices, by the name a slot
    // sells or a keeper shelves.
    let posed = |name: &str| -> Option<(&Vec<[f32; 3]>, Base)> {
        let (_, file, _, archive) = wanted.iter().find(|(item, ..)| item == name)?;
        let (points, unread) = meshes.get(file)?;
        if points.is_empty() || *unread {
            return None;
        }
        Some((points, roots.get(archive).copied().unwrap_or_default()))
    };
    for slot in &slots {
        let place = slot.rotate.unwrap_or([0.0; 3]);
        let Some((goods_mesh, goods_pose)) = posed(&slot.goods) else {
            println!(
                "{} {}: {} has no readable model",
                slot.shop, slot.number, slot.goods
            );
            continue;
        };
        // Where the goods' own model reaches, and the shelf line its bottom
        // lands on. The turn is the placement's own, which for most goods is
        // a yaw and leaves the height alone.
        let turned = span_in(goods_mesh, &goods_pose, place);
        let flat = span_in(goods_mesh, &goods_pose, [0.0; 3]);
        let shelf = slot.pos[1] + turned.0;
        print!(
            "{} {}: {}  y {:.4}  rot {:.3?}  bottom {:+.4} (unturned {:+.4})  shelf {:.4}",
            slot.shop, slot.number, slot.goods, slot.pos[1], place, turned.0, flat.0, shelf,
        );
        if let Some(old) = crate::shops::planks::plank(slot.shop, slot.number) {
            print!(
                "  plank measured {old:+.4}  goods {float:+.4} off it",
                float = turned.0 - old
            );
        }
        // The slot standing on the same spot of the shelf, one row up: the
        // room between this goods' bottom and that one's.
        let above = (slots.iter())
            .filter(|o| o.shop == slot.shop && o.pos[1] > slot.pos[1] + 0.2)
            .map(|o| (crate::checks::flat(o.pos, slot.pos), o))
            .filter(|(near, _)| *near < 0.3)
            .min_by(|(a, x), (b, y)| a.total_cmp(b).then(x.pos[1].total_cmp(&y.pos[1])));
        let gap = match above {
            Some((near, o)) => match posed(&o.goods) {
                Some((mesh, pose)) => {
                    let up = o.pos[1] + span_in(mesh, &pose, o.rotate.unwrap_or([0.0; 3])).0;
                    print!(
                        "  above {} at {:.2} m  gap {:.4}",
                        o.number,
                        near,
                        up - shelf
                    );
                    Some(up - shelf)
                }
                None => None,
            },
            None => None,
        };
        println!();
        for item in tries {
            let Some((mesh, pose)) = posed(item) else {
                continue;
            };
            let Some(gap) = gap else {
                println!("    {item}: no shelf above");
                continue;
            };
            let best = (0..3)
                .filter(|axis| *axis != 1)
                .flat_map(|axis| (0..=180u32).map(move |deg| (axis, deg)))
                // The pose that fills the gap most, and among those the
                // least turn. A yaw about Y is skipped: it changes nothing.
                .filter_map(|(axis, deg)| {
                    let mut turn = [0.0f32; 3];
                    turn[axis] = (deg as f32 - 90.0) * std::f32::consts::FRAC_PI_2 / 90.0;
                    let (lo, hi) = span_in(mesh, &pose, turn);
                    (hi - lo <= gap).then_some((hi - lo, deg.abs_diff(90), axis, lo, turn))
                })
                .max_by(|(tall, steep, ..), (other, deeper, ..)| {
                    tall.total_cmp(other).then(deeper.cmp(steep))
                });
            match best {
                Some((height, steep, axis, lo, turn)) => {
                    let (rise, at, way, rad) = (
                        turned.0 - lo,
                        slot.hash(),
                        ['X', 'Y', 'Z'][axis],
                        turn[axis],
                    );
                    // What the shelf code raises this item today, off the
                    // rupee's plank. A seed testing this pose adds the rest on
                    // top, since the shelf code's own field already ran.
                    let bottom = crate::shops::floor(item)
                        .or_else(|| standins::table_floor(&tables.actor, item).ok());
                    let today = crate::shops::planks::plank(slot.shop, slot.number)
                        .zip(bottom)
                        .map_or(0.0, |(plank, low)| plank - low);
                    let (extra, lean) = (
                        rise - today,
                        if crate::shops::stands(item) {
                            " leans today"
                        } else {
                            ""
                        },
                    );
                    println!(
                        "    {item}: {steep}° about {way}  height {height:.4}  floor {lo:+.4}  rise {rise:+.4}  today {today:+.4}{lean}"
                    );
                    println!(
                        "      --field {at:#018x}.Rotate.{axis}={rad:.4} --field {at:#018x}.Translate.1+={extra:.4}"
                    );
                }
                None => println!("    {item}: no turn fits {gap:.4} m, it lies as it is"),
            }
        }
    }
    Ok(())
}

/// Each item with the model file it shows, its row's model and its shelf
/// pose archive. A song shows the medallion it's painted on, and a level of
/// our own wears the actor it's copied from, which is the only one the dump
/// has a pack and a row for. The name stays ours, since that's what a holder
/// sells.
fn wanted(
    romfs: &Romfs,
    tables: &Tables,
    items: impl IntoIterator<Item = String>,
) -> Result<Vec<Wanted>> {
    let mut out = Vec::new();
    for item in items {
        let (pack, row) = match (medallions::medallion(&item), progressive::worn(&item)) {
            (Some(_), _) => (medallions::HOST.to_string(), item.clone()),
            (None, Some(worn)) => (worn.get().to_string(), worn.get().to_string()),
            (None, None) => (item.clone(), item.clone()),
        };
        let file = standins::model_file(&ActorPack::load(romfs, &pack)?)?;
        let (Some((project, model)), Some(row_model)) =
            (file, standins::model(&tables.actor, &row)?)
        else {
            continue;
        };
        let pose = standins::vanilla_pose(&tables.actor, &row)?;
        out.push((item, format!("{project}.{model}{MODEL}"), row_model, pose));
    }
    Ok(out)
}

/// One pass over the dump's models: each wanted model's vertices, and each
/// wanted archive's skeleton root in the shelf pose it holds.
fn models(romfs: &Romfs, game: &Path, wanted: &[Wanted]) -> Result<(Meshes, Roots)> {
    let files: HashSet<&str> = wanted.iter().map(|(_, f, _, _)| f.as_str()).collect();
    let mut archives: HashSet<String> = wanted.iter().map(|(.., p)| p.clone()).collect();
    archives.extend(wanted.iter().map(|(_, _, m, _)| standins::own_pose(m)));
    archives.insert(standins::stand_pose());
    let mut meshes: Meshes = HashMap::new();
    let mut roots: Roots = HashMap::new();
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
    Ok((meshes, roots))
}

const HEADER: &str = "\
//! Written by `zootr probe floors`, from the dump: rerun it when the pool
//! or the poses change, never edit it by hand. See [`super`].

use crate::known::Name;

/// By the item name's hash, in hash order, with whether the item stands up.
";

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
    span(points, pose).0
}

/// How tall `points` stand in `pose`: the top vertex less the bottom.
fn height(points: &[[f32; 3]], pose: &Base) -> f32 {
    let (lo, hi) = span(points, pose);
    hi - lo
}

/// The lowest and highest of `points` once `pose` scales, turns (Euler X,
/// Y, Z) and lifts them.
fn span(points: &[[f32; 3]], pose: &Base) -> (f32, f32) {
    span_in(points, pose, [0.0; 3])
}

/// The lowest and highest of `points` once `pose` has them, and a placement
/// then turns the whole thing about its own origin by `place` (Euler X, Y,
/// Z). The game puts a goods actor's rotation outside its animation pose,
/// so the pose runs first and the turn lifts whatever the pose left low.
fn span_in(points: &[[f32; 3]], pose: &Base, place: [f32; 3]) -> (f32, f32) {
    let scale = pose.scale.unwrap_or([1.0; 3]);
    let [px, py, pz, _] = pose.rotate.unwrap_or([0.0; 4]);
    let turn = euler(place);
    let posed = euler([px, py, pz]);
    // Row Y of the placement's turn after the pose's, and where the pose's
    // lift lands once the turn has moved it.
    let row = [0, 1, 2].map(|j| (0..3).map(|i| turn[1][i] * posed[i][j]).sum::<f32>());
    let lift = pose
        .translate
        .map_or(0.0, |t| (0..3).map(|i| turn[1][i] * t[i]).sum::<f32>());
    points.iter().fold((f32::MAX, f32::MIN), |(low, high), p| {
        let y = (0..3).map(|i| row[i] * p[i] * scale[i]).sum::<f32>() + lift;
        (low.min(y), high.max(y))
    })
}

/// The rows of Rz * Ry * Rx for Euler X, Y, Z in radians.
fn euler([x, y, z]: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        [cy * cz, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, cy * sx, cy * cx],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floors_table_opens_with_the_probe_header_and_is_sorted_and_counted() {
        let table = include_str!("../shops/floors/table.rs");
        assert!(table.starts_with(HEADER));
        let declared = table
            .split_once("pub(super) const FLOORS: [(Name, f32, bool); ")
            .and_then(|(_, rest)| rest.split_once(']'))
            .and_then(|(n, _)| n.parse::<usize>().ok())
            .expect("the table declares its length");
        // `row` binary searches by hash, so the rows must stay in order.
        let hashes: Vec<u64> = (table.lines())
            .filter_map(|l| l.trim_start().strip_prefix("(n!(0x"))
            .map(|l| u64::from_str_radix(&l[..16], 16).unwrap())
            .collect();
        assert_eq!(hashes.len(), declared);
        assert!(hashes.windows(2).all(|w| w[0] < w[1]));
    }

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
    fn a_lean_that_reaches_the_shelf_above_lies_flat() {
        let lean = Base {
            rotate: Some([std::f32::consts::FRAC_PI_3, 0.0, 0.0, 1.0]),
            ..Default::default()
        };
        let shield = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.5]];
        let sword = [[0.0, 0.0, 0.0], [0.0, 0.0, 1.4]];
        assert!(height(&shield, &lean) <= HEADROOM);
        assert!(height(&sword, &lean) > HEADROOM);
    }

    #[test]
    fn a_placements_turn_runs_after_the_pose() {
        let quarter = [std::f32::consts::FRAC_PI_2, 0.0, 0.0];
        let rod = [[0.0, 0.0, -0.2], [0.0, 0.0, 1.0]];
        // A goods actor's turn sits outside its animation pose, so a quarter
        // turn about X sends the end reaching along +Z down under it.
        let (low, high) = span_in(&rod, &Base::default(), quarter);
        assert!((low + 1.0).abs() < 1e-6);
        assert!((high - 0.2).abs() < 1e-6);
        // The pose's lift is turned with the model, not left upright.
        let back = Base {
            translate: Some([0.0, 0.0, 0.5]),
            ..Base::default()
        };
        let (low, high) = span_in(&rod, &back, quarter);
        assert!((low + 1.5).abs() < 1e-6);
        assert!((high + 0.3).abs() < 1e-6);
        // Unturned, the turn changes nothing.
        assert_eq!(span(&rod, &back), span_in(&rod, &back, [0.0; 3]));
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
