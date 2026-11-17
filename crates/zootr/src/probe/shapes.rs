//! The solid shapes of a map's placed actors, as a route sees them.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use ounce_formats::{NavMesh, Romfs};

use crate::nav::clear::crosses;
use crate::nav::graph::{Graph, STEP_UP};
use crate::nav::route::{Route, plan};
use crate::nav::shape::{self, Shape, Solid};
use crate::nav::{Level, kinds};
use crate::warp;

/// How far from a placement's height the floor under it may be for it to
/// stand on the mesh.
const ON_MESH: f32 = 1.0;

/// Prints each actor placed in `map` once: how many times it's placed,
/// whether its pack cuts the navmesh at run time (`cuts`, the ones a route
/// keeps clear of, or `cuts, no layer` when its body collides with
/// nothing) or not (`static`), and each part of its main body's
/// shape with its size in its own space, then how many of its placements
/// stand on the navmesh (`on N`), as against in a hole the mesh leaves for
/// them. With `gyaml`, only actors whose name contains it, and each
/// placement's footprint as placed.
pub fn shapes(romfs: &Romfs, game: &Path, map: &str, gyaml: Option<&str>) -> Result<()> {
    let placed = warp::placements_in(romfs, map)?;
    let mesh = crate::nav::load(romfs, game, map)?;
    let on = |name: &str| {
        let of = placed.iter().filter(|p| p.name == name);
        of.filter(|p| mesh.footing(p.pose.at, ON_MESH).is_some())
            .count()
    };
    let picked = |name: &str| gyaml.is_none_or(|g| name.contains(g));
    let mut by: BTreeMap<&str, (usize, Option<Shape>)> = BTreeMap::new();
    for p in placed.iter().filter(|p| picked(&p.name)) {
        if !by.contains_key(p.name.as_str()) {
            by.insert(&p.name, (0, shape::shape_of(romfs, &p.name)?));
        }
        by.get_mut(p.name.as_str()).unwrap().0 += 1;
    }
    let (mut cut, mut loose, mut fixed, mut empty, mut none) = (0, 0, 0, 0, 0);
    for (name, (count, shape)) in &by {
        let Some(shape) = shape else {
            none += count;
            println!("{count:>4} {name} none");
            continue;
        };
        let how = match (shape.parts.is_empty(), shape.cuts_mesh) {
            (true, _) => {
                empty += count;
                "no parts"
            }
            (false, true) if !shape.collides => {
                loose += count;
                "cuts, no layer"
            }
            (false, true) => {
                cut += count;
                "cuts"
            }
            (false, false) => {
                fixed += count;
                "static"
            }
        };
        let parts = shape.parts.iter().map(|p| {
            let size: [f32; 3] = std::array::from_fn(|i| p.max[i] - p.min[i]);
            format!(
                "{} {:.2}x{:.2}x{:.2} from {:.2},{:.2},{:.2}",
                p.kind, size[0], size[1], size[2], p.min[0], p.min[1], p.min[2]
            )
        });
        println!(
            "{count:>4} {name} {how} on {} {}",
            on(name),
            parts.collect::<Vec<_>>().join("; ")
        );
        if gyaml.is_none() {
            continue;
        }
        for p in placed.iter().filter(|p| p.name == **name) {
            for part in &shape.parts {
                let s = shape::solid(part, p);
                let ring = s.ring.iter().map(|[x, z]| format!("{x:.2},{z:.2}"));
                let (under, of) = under(&mesh, &s, p.pose.at[1]);
                println!(
                    "     {} y {:.2}..{:.2} mesh under {under}/{of} ring {}",
                    p.hash.unwrap_or_default(),
                    s.low,
                    s.high,
                    ring.collect::<Vec<_>>().join(" ")
                );
            }
        }
    }
    println!(
        "{} placements: {cut} cut the mesh, {loose} cut it with no layer, {fixed} static, {empty} with no parts, {none} with no shape",
        cut + loose + fixed + empty + none
    );
    Ok(())
}

/// The spacing of the spots a footprint is sampled at.
const SAMPLE: f32 = 0.25;

/// Of the spots a quarter metre apart inside `solid`'s footprint, how many
/// have the mesh under them near `y`, and how many there are.
fn under(mesh: &NavMesh, solid: &Solid, y: f32) -> (usize, usize) {
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &solid.ring {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let steps = |i: usize| ((hi[i] - lo[i]) / SAMPLE).ceil() as usize;
    let spots = (0..=steps(0)).flat_map(|i| {
        (0..=steps(1)).map(move |j| [lo[0] + i as f32 * SAMPLE, lo[1] + j as f32 * SAMPLE])
    });
    let spots: Vec<[f32; 2]> = spots.filter(|&p| solid.holds(p)).collect();
    let on = spots
        .iter()
        .filter(|p| mesh.footing([p[0], y, p[1]], ON_MESH).is_some());
    (on.count(), spots.len())
}

/// How far from a solid's middle a walk across it starts and ends, in m.
const ACROSS: f32 = 2.5;

/// For each solid of `map` standing on the mesh, a walk across it from
/// west to east and from south to north, with and without the solids:
/// each walk that the mesh alone sends through the solid, and whether the
/// route now goes round it (`round`) or still through (`through`, when
/// no way round stays on the mesh nearby). Then the tally.
pub fn crossings(romfs: &Romfs, game: &Path, map: &str) -> Result<()> {
    let level = Level::of(romfs, game, map)?;
    let (bare, graph) = (
        Graph::build(&level.mesh, &level.placed, STEP_UP),
        level.graph(),
    );
    let hits = |r: &Route, s: &Solid| {
        (r.waypoints.windows(2)).any(|w| crosses(&s.ring, flat(w[0].at), flat(w[1].at)))
    };
    let (mut round, mut through) = (0, 0);
    for s in &level.solids {
        let n = s.ring.len() as f32;
        let [x, z] = (s.ring.iter()).fold([0.0, 0.0], |m, c| [m[0] + c[0] / n, m[1] + c[1] / n]);
        if level.mesh.footing([x, s.low, z], ON_MESH).is_none() {
            continue;
        }
        for [dx, dz] in [[ACROSS, 0.0], [0.0, ACROSS]] {
            let (from, to) = ([x - dx, s.low, z - dz], [x + dx, s.low, z + dz]);
            let Ok(before) = plan(&bare, from, to) else {
                continue;
            };
            if !hits(&before, s) {
                continue;
            }
            let after = plan(&graph, from, to)?;
            let how = match hits(&after, s) {
                true => {
                    through += 1;
                    "through"
                }
                false => {
                    round += 1;
                    "round"
                }
            };
            let what = kinds::word(&s.name).unwrap_or(&s.name);
            println!(
                "{how:>7} {what} {} at {x:.1},{:.1},{z:.1}: {:.1} m, was {:.1}",
                s.hash.unwrap_or_default(),
                s.low,
                after.length,
                before.length
            );
        }
    }
    println!(
        "{} solids: of {} walks through one on the mesh alone, {round} now go round and {through} still through",
        level.solids.len(),
        round + through
    );
    Ok(())
}

fn flat(p: [f32; 3]) -> [f32; 2] {
    [p[0], p[2]]
}
