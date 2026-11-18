//! A map's waters, as a route swims them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ounce_formats::Romfs;

use crate::nav::Level;
use crate::nav::graph::{Graph, STEP_UP};
use crate::nav::water::{NoSwim, bodies};

/// Prints each body of water in `map`: its planes with their surface,
/// depth and presence, then each two stretches of its shore and the swim
/// between them, or that the way across is over land or off the water.
/// Ends with how many crossings swim and how many islands of mesh there
/// are without swims and with them. Waters show as with no flag set. With
/// `svg`, also draws it to that file, round a point when one is given.
pub fn water(
    romfs: &Romfs,
    game: &Path,
    map: &str,
    svg: Option<(PathBuf, Option<[f32; 3]>, f32)>,
) -> Result<()> {
    let level = Level::of(romfs, game, map)?;
    let graph = level.graph();
    let xyz = |p: [f32; 3]| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]);
    let crossings = graph.crossings(&level.waters);
    let shores = &graph.shores;
    for (n, body) in bodies(&level.waters).iter().enumerate() {
        let on = shores.iter().filter(|s| body.contains(&s.water)).count();
        println!("body {n}: {} planes, {on} shore points", body.len());
        for &i in body {
            let w = &level.waters[i];
            let hash = w.hash.map_or("-".into(), |h| h.to_string());
            let presence = w
                .presence
                .as_ref()
                .map_or(String::new(), |p| format!(" if {p}"));
            println!(
                "  {} {hash} surface {:.1} depth {:.1}{presence}",
                w.name, w.surface, w.depth
            );
        }
        for c in crossings.iter().filter(|c| c.body == n) {
            let how = match c.not {
                None => "swims".to_string(),
                Some(NoSwim::Land(p)) => format!("no swim, land at {}", xyz(p)),
                Some(NoSwim::Dry(p)) => format!("no swim, no water at {}", xyz(p)),
            };
            println!(
                "  {how}: face {} at {} to face {} at {} ({:.1} m)",
                c.from.face,
                xyz(c.from.at),
                c.to.face,
                xyz(c.to.at),
                crate::checks::flat(c.from.at, c.to.at)
            );
        }
    }
    let swum = crossings.iter().filter(|c| c.not.is_none()).count();
    println!("{swum} of {} crossings swim", crossings.len());
    let dry = Graph::build(&level.mesh, &level.placed, STEP_UP).with_climbs(&level.climbs);
    println!(
        "islands of 20 faces or more: {} without swims, {} with",
        islands(&dry),
        islands(&graph)
    );
    if let Some((file, at, within)) = svg {
        std::fs::write(&file, draw(&graph, &level.waters, &crossings, at, within))?;
        println!("drawn to {}", file.display());
    }
    Ok(())
}

/// How far above or below `--at` a face is drawn, in m.
const SHOWN_TALL: f32 = 8.0;
/// How far above or below `--at` a water, its shores and swims are drawn,
/// in m.
const SHOWN_WATER: f32 = 4.0;

/// The map from above as an SVG, x right and z down: each face shaded from
/// dark (low) to light (high), each water's box in blue, shore points in
/// orange, swims in green and crossings that don't swim in red. Only what
/// lies within `within` of `at`, when given.
fn draw(
    graph: &Graph,
    waters: &[crate::nav::water::Water],
    crossings: &[crate::nav::water::Crossing],
    at: Option<[f32; 3]>,
    within: f32,
) -> String {
    let mesh = graph.mesh;
    let near = |p: [f32; 3], tall: f32| {
        at.is_none_or(|a| {
            (p[0] - a[0]).abs() <= within
                && (p[2] - a[2]).abs() <= within
                && (p[1] - a[1]).abs() <= tall
        })
    };
    let shown: Vec<usize> = (0..mesh.faces.len())
        .filter(|&f| near(graph.centers[f], SHOWN_TALL))
        .collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for &f in &shown {
        for e in mesh.ring(f) {
            let v = mesh.vertices[e.a];
            for i in 0..3 {
                (lo[i], hi[i]) = (lo[i].min(v[i]), hi[i].max(v[i]));
            }
        }
    }
    let rise = (hi[1] - lo[1]).max(1.0);
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" style="background:#111">"#,
        lo[0] - 2.0,
        lo[2] - 2.0,
        hi[0] - lo[0] + 4.0,
        hi[2] - lo[2] + 4.0
    );
    for &f in &shown {
        let points: Vec<String> = (mesh.ring(f).iter())
            .map(|e| format!("{:.2},{:.2}", mesh.vertices[e.a][0], mesh.vertices[e.a][2]))
            .collect();
        let shade = 40 + ((graph.centers[f][1] - lo[1]) / rise * 200.0) as u8;
        let _ = writeln!(
            out,
            r##"<polygon points="{}" fill="rgb({shade},{shade},{shade})" stroke="#000" stroke-width="0.03"><title>face {f} y {:.1}</title></polygon>"##,
            points.join(" "),
            graph.centers[f][1]
        );
    }
    for w in waters
        .iter()
        .filter(|w| near([w.corners[0][0], w.surface, w.corners[0][1]], SHOWN_WATER))
    {
        let points: Vec<String> = w
            .corners
            .iter()
            .map(|c| format!("{:.2},{:.2}", c[0], c[1]))
            .collect();
        let _ = writeln!(
            out,
            r##"<polygon points="{}" fill="#2060ff" fill-opacity="0.15" stroke="#4080ff" stroke-width="0.15"><title>{} surface {:.1}</title></polygon>"##,
            points.join(" "),
            w.name,
            w.surface
        );
    }
    for c in crossings.iter().filter(|c| near(c.from.at, SHOWN_WATER)) {
        let (color, width) = if c.not.is_none() {
            ("#30ff60", 0.25)
        } else {
            ("#ff3030", 0.05)
        };
        let _ = writeln!(
            out,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{color}" stroke-width="{width}"/>"#,
            c.from.at[0], c.from.at[2], c.to.at[0], c.to.at[2]
        );
    }
    for s in graph.shores.iter().filter(|s| near(s.at, SHOWN_WATER)) {
        let _ = writeln!(
            out,
            r##"<circle cx="{:.2}" cy="{:.2}" r="0.3" fill="#ff9020"><title>y {:.1}</title></circle>"##,
            s.at[0], s.at[2], s.at[1]
        );
    }
    out.push_str("</svg>\n");
    out
}

/// Prints the route in `map` from `from` to `to`: each waypoint and how
/// it's reached, then its length. Waters show as with no flag set.
pub fn route(romfs: &Romfs, game: &Path, map: &str, from: [f32; 3], to: [f32; 3]) -> Result<()> {
    let level = Level::of(romfs, game, map)?;
    let graph = level.graph();
    let route = match crate::nav::route::plan(&graph, from, to) {
        Ok(route) => route,
        Err(e) => {
            // Each end's face, and how many faces it reaches going forward.
            for (end, p) in [("from", from), ("to", to)] {
                if let Some(s) = graph.locate(p) {
                    let n = reached(&graph, s.face).len();
                    println!("{end} face {} at {:.1}: reaches {n} faces", s.face, s.at[1]);
                }
            }
            return Err(e);
        }
    };
    for w in &route.waypoints {
        println!("{:.1},{:.1},{:.1} {}", w.at[0], w.at[1], w.at[2], w.via);
    }
    println!("{:.1} m", route.length);
    Ok(())
}

/// The faces `face` reaches, going forward along the graph's arcs.
fn reached(graph: &Graph, face: usize) -> std::collections::HashSet<usize> {
    let mut seen = std::collections::HashSet::from([face]);
    let mut todo = vec![face];
    while let Some(f) = todo.pop() {
        for a in &graph.arcs[f] {
            if seen.insert(a.to) {
                todo.push(a.to);
            }
        }
    }
    seen
}

/// How many parts of the graph, of at least 20 faces each, Link gets
/// round within, going any way across every arc.
fn islands(graph: &Graph) -> usize {
    let n = graph.arcs.len();
    let mut of: Vec<usize> = (0..n).collect();
    fn root(of: &mut [usize], mut i: usize) -> usize {
        while of[i] != i {
            of[i] = of[of[i]];
            i = of[i];
        }
        i
    }
    for (from, arcs) in graph.arcs.iter().enumerate() {
        for a in arcs {
            let (r, s) = (root(&mut of, from), root(&mut of, a.to));
            of[r] = s;
        }
    }
    let mut sizes = std::collections::HashMap::new();
    for f in 0..n {
        *sizes.entry(root(&mut of, f)).or_insert(0) += 1;
    }
    sizes.values().filter(|&&s| s >= 20).count()
}
