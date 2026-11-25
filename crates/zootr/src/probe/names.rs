//! `probe names`: a draft names file for one area, to edit into
//! `logic/names/`.

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;

use crate::ap::names::{area, item, locations};
use crate::check_names::prefix;
use crate::derive::rooms::room_name;
use crate::generate::Spot;
use crate::places;
use crate::rules::{Book, rooms::folder};

/// Prints `want`'s checks by room, then west to east, then north to south.
pub(super) fn draft(romfs: &Romfs, want: &str) -> Result<()> {
    let prefix = prefix(want).with_context(|| format!("no prefix for {want}"))?;
    let checks = crate::ap::export::checks(romfs)?;
    let current = locations(romfs, &checks)?;
    let mine: Vec<_> = (checks.iter())
        .filter(|c| area(&c.area).is_ok_and(|a| a == want))
        .collect();
    ensure!(!mine.is_empty(), "no checks in {want}");
    let spots: Vec<Spot> = (mine.iter())
        .map(|c| Spot {
            hash: c.hash,
            area: c.area.clone(),
            ..Default::default()
        })
        .collect();
    let stand = places::stand(romfs, &spots)?;
    let mut rows: Vec<_> = mine.iter().map(|c| (stand.get(&c.hash), *c)).collect();
    let key = |at: Option<&(i32, [f32; 3])>| at.map(|(room, p)| (*room, p[0], p[2]));
    rows.sort_by(|a, b| {
        let (a, b) = (key(a.0), key(b.0));
        // Unplaced checks (gifts) last.
        (a.is_none().cmp(&b.is_none())).then(a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal))
    });
    println!("# {want}: names start with \"{prefix} \". Rename each line, keep its hash.");
    for (at, c) in rows {
        println!();
        if let Some(rule) = Book::name(c.hash) {
            println!("# rule: {rule}");
        }
        for line in Book::comment(c.hash) {
            println!("{line}");
        }
        let held = item(&c.item, false).unwrap_or(&c.item);
        match at {
            Some((room, [x, y, z])) => {
                let named = u32::try_from(*room)
                    .ok()
                    .and_then(|r| room_name(&folder(&c.area), r));
                let named = named.map(|n| format!(" ({n})")).unwrap_or_default();
                println!("# room {room}{named}, at {x:.0} {y:.0} {z:.0}, held {held}");
            }
            None => println!("# not placed ({}), held {held}", c.holder),
        }
        println!("{} = {:?}", c.hash, current[&c.hash]);
    }
    Ok(())
}
