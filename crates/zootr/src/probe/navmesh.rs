//! `zootr probe nav`: a map's navmesh, its counts and bounds, or where a
//! point stands on it and the borders around it.

use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{NavMesh, Romfs, Tagfile};

use crate::checks::xyz;
use crate::nav::navmesh_file;

/// Every type a tagfile declares, with its fields.
pub(super) fn print_types(bytes: &[u8]) -> Result<()> {
    let file = Tagfile::parse(bytes)?;
    for (i, t) in file.types.iter().enumerate().skip(1) {
        println!("{i} {} parent {} size {}", t.name, t.parent, file.size(i));
        for f in &t.fields {
            println!("    {} @{} {}", f.name, f.offset, file.types[f.ty].name);
        }
    }
    Ok(())
}

pub(super) fn nav(
    romfs: &Romfs,
    game: &Path,
    map: &str,
    types: bool,
    at: Option<[f32; 3]>,
    within: f32,
) -> Result<()> {
    let bytes = navmesh_file(romfs, game, map)?
        .with_context(|| format!("{map} has no navmesh of its own (Hyrule Field's is in tiles)"))?;
    if types {
        return print_types(&bytes);
    }
    let mesh = NavMesh::parse(&bytes)?;
    println!(
        "{} faces, {} edges ({} on the border), {} vertices, from {} to {}, kept {:.2} from walls",
        mesh.faces.len(),
        mesh.edges.len(),
        mesh.edges.iter().filter(|e| e.across.is_none()).count(),
        mesh.vertices.len(),
        xyz(mesh.min),
        xyz(mesh.max),
        mesh.erosion,
    );
    let Some(p) = at else { return Ok(()) };
    match mesh.footing(p, 3.0) {
        Some(f) => println!(
            "{} stands on face {}, floor {:+.1}",
            xyz(p),
            f.face,
            f.height - p[1]
        ),
        None => println!("{} has no face within 3 of its height", xyz(p)),
    }
    println!("distance  x,z toward  past it  edge");
    for b in mesh.borders(p, within, 3.0, 0.5) {
        let past = b.beyond.map_or("no mesh".into(), |h| format!("{h:+.1}"));
        let [x, z] = b.toward;
        println!(
            "{:>8.1}  {x:+.2},{z:+.2}  {past:>7}  {}",
            b.distance, b.edge
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map;

    /// The runs D start in Kokiri Forest stands on the mesh. Where walk-to
    /// stopped against a tree trunk, Link stood off it, inside the gap the
    /// mesh keeps from walls, with the trunk's border under a metre away.
    #[test]
    fn kokiri_navmesh_matches_where_link_stood() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new(crate::game::DEV_DUMP);
        let mesh = NavMesh::parse(
            &navmesh_file(&romfs, game, map::KOKIRI_FOREST_CHILD)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let start = mesh.footing([-32.0, 121.2, -65.2], 3.0).unwrap();
        assert!((start.height - 121.2).abs() < 0.1, "{start:?}");
        let trunk = [-45.2, 113.0, -42.4];
        assert_eq!(mesh.footing(trunk, 3.0), None);
        let wall = mesh.borders(trunk, 2.0, 3.0, 0.5)[0];
        assert!(wall.distance < 1.0 && wall.beyond.is_none(), "{wall:?}");
    }
}
