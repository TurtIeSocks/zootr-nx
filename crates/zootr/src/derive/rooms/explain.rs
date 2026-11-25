//! `zootr logic explain`: what derive sees around a spot or in some rooms.
//! Each navmesh face near the spot with its room, part and arcs out, the
//! numbered placements there, and a top-down picture of the floor by room.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::Result;
use ounce_formats::Romfs;

use super::{Floor, kinds_by_hash, label::path_of, maps};
use crate::entrances::Dungeon;
use crate::nav::graph::{Cross, Graph, Via, dist};
use crate::nav::picture::{Picture, Rgb, cut};
use crate::rules::Book;

/// What to explain.
pub struct Ask {
    /// The spot whose faces and placements to list.
    pub at: Option<[f32; 3]>,
    /// How far round `at` to look, in m.
    pub within: f32,
    /// Where to write the picture.
    pub picture: Option<PathBuf>,
    /// The rooms the picture shows. None: round `at`.
    pub rooms: Vec<u32>,
    /// The height walls are cut at. None: `at`'s, or the shown floor's.
    pub y: Option<f32>,
}

/// How many pixels a metre the picture draws.
const SCALE: f32 = 16.0;
/// How far the picture reaches past what it shows, in m.
const MARGIN: f32 = 4.0;
/// How far above or below the cut height a face is still drawn, in m.
const LAYER: f32 = 6.0;
/// Room colors, by room number.
const PALETTE: [Rgb; 12] = [
    [141, 211, 199],
    [255, 255, 179],
    [190, 186, 218],
    [251, 128, 114],
    [128, 177, 211],
    [253, 180, 98],
    [179, 222, 105],
    [252, 205, 229],
    [188, 128, 189],
    [204, 235, 197],
    [255, 237, 111],
    [150, 200, 255],
];
const ROOMLESS: Rgb = [170, 170, 170];
const OTHER_ROOM: Rgb = [225, 225, 225];
const WALL: Rgb = [0, 0, 0];
const AIRWALL: Rgb = [0, 0, 255];
const SWIM: Rgb = [0, 150, 0];
const CRAWL: Rgb = [230, 120, 0];
const SPOT: Rgb = [230, 0, 0];

/// A face's color under water a child swims: its room's, darkened blue.
fn swum(c: Rgb) -> Rgb {
    [c[0] / 2 + 40, c[1] / 2 + 60, c[2] / 2 + 120]
}

/// What `ask` asks of `dungeon`'s maps, as text, writing its picture.
pub fn explain(
    romfs: &Romfs,
    game: &std::path::Path,
    dungeon: &Dungeon,
    ask: &Ask,
) -> Result<String> {
    let book = Book::load()?;
    let kinds = kinds_by_hash(&book);
    let mut out = String::new();
    let mut drawn = 0;
    for map in maps(romfs, dungeon)? {
        let floor = Floor::of(romfs, game, &map, &kinds)?;
        let graph = floor.level.graph();
        let labels = floor.labels(&graph);
        let room = &labels.room;
        if let Some(at) = ask.at {
            let near: Vec<usize> = (0..graph.centers.len())
                .filter(|&f| dist(graph.centers[f], at) <= ask.within)
                .collect();
            if near.is_empty() {
                continue;
            }
            writeln!(out, "{map}: {} faces within {} m", near.len(), ask.within)?;
            for &f in &near {
                let c = graph.centers[f];
                let water = if graph.deep(c) { ", swum" } else { "" };
                writeln!(
                    out,
                    "face {f} at {:.1}, {:.2}, {:.1}: room {}, part {}{water}",
                    c[0],
                    c[1],
                    c[2],
                    show(room[f]),
                    labels.part[f]
                )?;
                for arc in &graph.arcs[f] {
                    let how = match arc.cross {
                        Cross::Edge(_) => "walk".to_owned(),
                        Cross::Link { via, .. } => via.to_string(),
                    };
                    let (a, b) = path_of(&graph, f, arc);
                    let by: Vec<String> = (floor.on(a, b).iter())
                        .map(|(h, k)| format!("{} {h}", k.name))
                        .collect();
                    let by = if by.is_empty() {
                        String::new()
                    } else {
                        format!(", through {}", by.join(", "))
                    };
                    writeln!(
                        out,
                        "  to face {} (room {}): {how}{by}",
                        arc.to,
                        show(room[arc.to])
                    )?;
                }
            }
            for p in (floor.level.placed.iter()).filter(|p| dist(p.pose.at, at) <= ask.within) {
                let Some(r) = p.hash.and_then(|h| floor.rooms_of.get(&h)) else {
                    continue;
                };
                let [x, y, z] = p.pose.at;
                let hash = p.hash.unwrap_or_default();
                let name = crate::derive::words(&p.name);
                writeln!(
                    out,
                    "placement {name} {hash} at {x:.1}, {y:.1}, {z:.1}: seeds room {r}"
                )?;
            }
        }
        if let Some(path) = &ask.picture
            && let Some(png) = picture(&graph, room, ask)?
        {
            let path = if drawn == 0 {
                path.clone()
            } else {
                path.with_extension(format!("{map}.png"))
            };
            std::fs::write(&path, png)?;
            writeln!(out, "picture of {map}: {}", path.display())?;
            drawn += 1;
        }
    }
    anyhow::ensure!(
        !out.is_empty(),
        "nothing there: no face within {} m, and no rooms drawn",
        ask.within
    );
    Ok(out)
}

fn show(room: Option<u32>) -> String {
    room.map_or("none".to_owned(), |r| r.to_string())
}

/// The picture `ask` wants of `graph`, or None when nothing it shows is
/// on this map.
fn picture(graph: &Graph, room: &[Option<u32>], ask: &Ask) -> Result<Option<Vec<u8>>> {
    let shown: Vec<usize> = (0..graph.centers.len())
        .filter(|&f| match ask.at {
            _ if !ask.rooms.is_empty() => room[f].is_some_and(|r| ask.rooms.contains(&r)),
            Some(at) => dist(graph.centers[f], at) <= ask.within.max(15.0),
            None => false,
        })
        .collect();
    if shown.is_empty() {
        return Ok(None);
    }
    let ys = |f: &usize| graph.centers[*f][1];
    let y = ask.y.or(ask.at.map(|a| a[1])).unwrap_or_else(|| {
        let mut h: Vec<f32> = shown.iter().map(ys).collect();
        h.sort_by(f32::total_cmp);
        h[h.len() / 2]
    });
    let (xs, zs) = (
        shown.iter().map(|&f| graph.centers[f][0]),
        shown.iter().map(|&f| graph.centers[f][2]),
    );
    let lo = [
        xs.clone().fold(f32::MAX, f32::min) - MARGIN,
        zs.clone().fold(f32::MAX, f32::min) - MARGIN,
    ];
    let hi = [
        xs.fold(f32::MIN, f32::max) + MARGIN,
        zs.fold(f32::MIN, f32::max) + MARGIN,
    ];
    let inside = |p: [f32; 3]| (lo[0]..hi[0]).contains(&p[0]) && (lo[1]..hi[1]).contains(&p[2]);
    let mut pic = Picture::new(lo, hi, SCALE, [255, 255, 255]);
    let color = |r: Option<u32>| match r {
        None => ROOMLESS,
        Some(r) if ask.rooms.is_empty() || ask.rooms.contains(&r) => {
            PALETTE[r as usize % PALETTE.len()]
        }
        Some(_) => OTHER_ROOM,
    };
    let mesh = graph.mesh;
    for (f, (&c, &r)) in graph.centers.iter().zip(room).enumerate() {
        if !inside(c) || (c[1] - y).abs() > LAYER {
            continue;
        }
        let ring: Vec<[f32; 2]> = (mesh.ring(f).iter())
            .map(|e| [mesh.vertices[e.a][0], mesh.vertices[e.a][2]])
            .collect();
        let fill = if graph.deep(c) {
            swum(color(r))
        } else {
            color(r)
        };
        pic.fill(&ring, fill);
    }
    for (walls, c, width) in [(graph.solid, WALL, 2), (graph.walls, AIRWALL, 3)] {
        for t in walls.iter().flat_map(|w| &w.triangles) {
            for dy in [0.5, 1.5] {
                if let Some([a, b]) = cut(t, y + dy) {
                    pic.line(a, b, c, width);
                }
            }
        }
    }
    for arc in graph.arcs.iter().flatten() {
        if let Cross::Link { via, from, to } = arc.cross
            && let Some(c) = match via {
                Via::Swim(_) => Some(SWIM),
                Via::Crawl(_) => Some(CRAWL),
                _ => None,
            }
        {
            pic.line([from[0], from[2]], [to[0], to[2]], c, 2);
        }
    }
    // Each room's number at the middle of its dry floor in view.
    let mut rooms: Vec<u32> = room.iter().flatten().copied().collect();
    rooms.sort_unstable();
    rooms.dedup();
    for r in rooms {
        let fs: Vec<[f32; 3]> = (0..graph.centers.len())
            .filter(|&f| room[f] == Some(r))
            .map(|f| graph.centers[f])
            .filter(|&c| inside(c) && (c[1] - y).abs() <= LAYER && !graph.deep(c))
            .collect();
        if fs.is_empty() {
            continue;
        }
        let n = fs.len() as f32;
        let mid = [
            fs.iter().map(|c| c[0]).sum::<f32>() / n,
            fs.iter().map(|c| c[2]).sum::<f32>() / n,
        ];
        pic.number(r, mid, 2.4, WALL);
    }
    if let Some(at) = ask.at {
        let [x, _, z] = at;
        pic.line([x - 1.0, z - 1.0], [x + 1.0, z + 1.0], SPOT, 4);
        pic.line([x - 1.0, z + 1.0], [x + 1.0, z - 1.0], SPOT, 4);
    }
    Ok(Some(pic.png()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The water room's door: the basement scrub room's side is room 5,
    /// the landing past it room 6, and the picture of both draws.
    #[test]
    #[ignore = "derives the Deku Tree from the dump: run with `--ignored` when changing derive"]
    fn the_water_room_door_parts_rooms_5_and_6() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = std::path::Path::new("../../dump/oot.nup");
        let deku = crate::entrances::DUNGEONS
            .iter()
            .find(|d| d.id == "deku-tree")
            .unwrap();
        let dir = std::env::temp_dir().join(format!("explain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("door.png");
        let ask = Ask {
            at: Some([-17.9, 78.8, 30.1]),
            within: 2.0,
            picture: Some(png.clone()),
            rooms: vec![5, 6],
            y: None,
        };
        let text = explain(&romfs, game, deku, &ask).unwrap();
        assert!(
            text.contains("seeds room 5") && text.contains("seeds room 6"),
            "{text}"
        );
        assert!(text.contains("through Switch Shutter"), "{text}");
        assert!(std::fs::metadata(&png).unwrap().len() > 1000);
        std::fs::remove_dir_all(dir).ok();
    }
}
