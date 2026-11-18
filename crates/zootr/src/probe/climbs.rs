//! A map's ladders and climb walls, as a route climbs them.

use std::path::Path;

use anyhow::Result;
use ounce_formats::Romfs;

use crate::nav::Level;

/// Prints each climb in `map`: its actor and placement, how high it rises
/// from its foot, and the floor at each end a route links, or why it has
/// no link. Ends with how many are linked.
pub fn climbs(romfs: &Romfs, game: &Path, map: &str) -> Result<()> {
    let level = Level::of(romfs, game, map)?;
    let graph = level.graph();
    let xyz = |p: [f32; 3]| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]);
    let mut linked = 0;
    for c in &level.climbs {
        let hash = c.hash.map_or("-".into(), |h| h.to_string());
        let rise = c.top[1] - c.foot[1];
        let ends = match graph.climb_ends(c) {
            Ok((foot, top)) => {
                linked += 1;
                format!(
                    "face {} at {} to face {} at {}",
                    foot.face,
                    xyz(foot.at),
                    top.face,
                    xyz(top.at)
                )
            }
            Err(why) => why.into(),
        };
        println!(
            "{} {hash} rises {rise:.1} from {}: {ends}",
            c.name,
            xyz(c.foot)
        );
    }
    println!("{linked} of {} linked", level.climbs.len());
    Ok(())
}
