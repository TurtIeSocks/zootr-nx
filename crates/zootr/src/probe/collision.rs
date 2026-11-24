//! A map's collision meshes as the game's collision filter sorts them.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::Result;
use ounce_formats::Romfs;

use super::tally;
use crate::checks::xyz;
use crate::nav::filter::Filter;
use crate::nav::mesh::{self, MeshUse};
use crate::warp;

/// How far below a placement's feet its floor may be and still count as
/// under it, and how far above (a foot sunk into the floor).
const BELOW: f32 = 0.3;
const ABOVE: f32 = 0.1;
/// How far down the floor under a placement is looked for.
const LOOK: f32 = 3.0;
/// How far out from a placement's origin its footprint is sampled: a
/// chest stands over a seam between two pieces as often as not.
const FOOT: f32 = 0.5;

/// Prints each piece placed in `map` with a collision mesh once: how many
/// times it's placed, each mesh, whether it's an invisible wall (`air`)
/// or not (`solid`), what it does to Link (`foot`, `climb`, `unridable`,
/// or `none`), and each of its materials with its tags. With `under`,
/// the floor under each other placement instead: its height below the
/// placement's feet, and how many stand within [`BELOW`] of theirs.
pub fn collision(
    romfs: &Romfs,
    game: &Path,
    map: &str,
    under: bool,
    at: Option<[f32; 3]>,
) -> Result<()> {
    let placed = warp::placements_in(romfs, map)?;
    let filter = Filter::get(romfs)?;
    let mut uses: BTreeMap<&str, (usize, Vec<MeshUse>)> = BTreeMap::new();
    for p in &placed {
        tally(&mut uses, &p.name, || {
            mesh::mesh_names(romfs, filter, &p.name)
        })?;
    }
    if under {
        return floors_under(romfs, game, &placed, &uses);
    }
    let wanted: HashSet<&str> = (uses.values().flat_map(|(_, u)| u))
        .map(|u| u.name.as_str())
        .collect();
    let meshes = mesh::meshes(romfs, game, &wanted)?;
    if let Some(at) = at {
        let walls = mesh::walls(romfs, game, &placed)?;
        return triangles_at(filter, &placed, &uses, &meshes, &walls, at);
    }
    for (actor, (count, uses)) in uses.iter().filter(|(_, (_, u))| !u.is_empty()) {
        for u in uses {
            let Some(m) = meshes.get(&u.name) else {
                println!("{count:>4} {actor} {} missing", u.name);
                continue;
            };
            let t = mesh::touch(filter, u.body, m);
            let does = match (t.on_foot, t.climbing, t.ridable) {
                (false, false, _) => "none",
                (false, true, _) => "climb",
                (true, _, false) => "foot unridable",
                (true, _, true) => "foot",
            };
            let kind = if u.air { "air" } else { "solid" };
            let materials: Vec<String> = (m.materials.iter())
                .map(|m| {
                    let name = |list: &[String], i: u32| {
                        list.get(i as usize)
                            .cloned()
                            .unwrap_or_else(|| i.to_string())
                    };
                    let tags = filter.tag_names(m.tags).join("+");
                    format!(
                        "{}/{}{}{}",
                        name(&filter.materials, m.material),
                        name(&filter.sub_materials, m.sub),
                        if tags.is_empty() { "" } else { " " },
                        tags,
                    )
                })
                .collect();
            println!(
                "{count:>4} {actor} {} {kind} {does} {} tris [{}]",
                u.name,
                m.triangles.len(),
                materials.join(", ")
            );
        }
    }
    Ok(())
}

/// Prints the floor under each chest, pot and start spot: the highest
/// ridable solid floor from [`ABOVE`] over its feet to [`LOOK`] under
/// them, anywhere within [`FOOT`] of its origin.
fn floors_under(
    romfs: &Romfs,
    game: &Path,
    placed: &[warp::Placement],
    uses: &BTreeMap<&str, (usize, Vec<MeshUse>)>,
) -> Result<()> {
    let walls = mesh::walls(romfs, game, placed)?;
    let stands = |p: &&warp::Placement| {
        let held = p.name.starts_with("Obj_") && crate::nav::kinds::is_chest_or_pot(&p.name);
        uses[p.name.as_str()].1.is_empty()
            && (held || p.name == crate::known::actor::START_POS.get())
    };
    let ring = [(0.0, 0.0)].into_iter().chain((0..8).flat_map(|i| {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        [0.5, 1.0].map(|r| (FOOT * r * a.cos(), FOOT * r * a.sin()))
    }));
    let ring: Vec<(f32, f32)> = ring.collect();
    let (mut near, mut all) = (0, 0);
    for p in placed.iter().filter(stands) {
        let [x, y, z] = p.pose.at;
        let floor = (walls.solid.iter().filter(|w| w.ridable))
            .flat_map(|w| ring.iter().flat_map(|(dx, dz)| w.floors_at(x + dx, z + dz)))
            .filter(|&h| h <= y + ABOVE && h >= y - LOOK)
            .max_by(f32::total_cmp);
        all += 1;
        let gap = floor.map(|h| y - h);
        near += usize::from(gap.is_some_and(|g| g <= BELOW));
        let gap = gap.map_or("none".to_owned(), |g| format!("{g:.2}"));
        let hash = p.hash.map(|h| h.to_string()).unwrap_or_default();
        println!("{gap:>6} {} {hash} {}", p.name, xyz(p.pose.at));
    }
    println!("{near} of {all} stand within {BELOW} m of a floor");
    Ok(())
}

/// Prints every collision triangle straight over or under `at` within
/// [`LOOK`], highest first: its height, its piece and mesh, its material
/// and what that does to Link, and whether it faces up gently enough to
/// stand on (`floor`) or not (`steep`).
fn triangles_at(
    filter: &Filter,
    placed: &[warp::Placement],
    uses: &BTreeMap<&str, (usize, Vec<MeshUse>)>,
    meshes: &std::collections::HashMap<String, ounce_formats::ShapeMesh>,
    walls: &mesh::Walls,
    [x, y, z]: [f32; 3],
) -> Result<()> {
    let mut found = vec![];
    for p in placed {
        for u in &uses[p.name.as_str()].1 {
            let Some(m) = meshes.get(&u.name) else {
                continue;
            };
            let touches = mesh::touches(filter, u.body, m);
            let posed = mesh::Wall::posed(&u.name, m, &p.pose);
            for (i, t) in posed.triangles.iter().enumerate() {
                let Some(h) = mesh::height_in(t, x, z).filter(|h| (h - y).abs() <= LOOK) else {
                    continue;
                };
                let k = usize::from(m.triangle_materials.get(i).copied().unwrap_or(0));
                let touch = touches.get(k).copied().unwrap_or_default();
                let material = m.materials.get(k).map_or(String::new(), |mat| {
                    let name = filter
                        .materials
                        .get(mat.material as usize)
                        .cloned()
                        .unwrap_or_default();
                    format!("{name} {}", filter.tag_names(mat.tags).join("+"))
                });
                let up = match (mesh::floor_normal(t), mesh::facing_up(t)) {
                    (true, true) => "floor",
                    (true, false) => "ceiling",
                    (false, _) => "steep",
                };
                let does = match (touch.on_foot, touch.ridable) {
                    (false, _) => "none",
                    (true, false) => "unridable",
                    (true, true) => "foot",
                };
                let kind = if u.air { "air" } else { "solid" };
                found.push((
                    h,
                    format!("{} {} {kind} {up} {does} [{material}]", p.name, u.name),
                ));
            }
        }
    }
    let round = crate::nav::floors::Round::of(walls);
    for (h, _) in found.iter().filter(|(h, _)| (h - y).abs() <= 0.5) {
        let rays = round.rays([x, *h, z]);
        let shown: Vec<String> = (rays.iter())
            .map(|r| {
                r.map_or("-".to_owned(), |(d, back)| {
                    format!("{d:.1}{}", if back { "b" } else { "f" })
                })
            })
            .collect();
        println!(
            "rays from {h:.2} (east first, b = a wall's back): {}",
            shown.join(" ")
        );
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (h, line) in found {
        println!("{h:>8.2} {line}");
    }
    Ok(())
}
