//! The flag map's edges from placements' own params: a placement whose
//! `Dynamic` params name an `EventSetting` row (a scrub's
//! `DamageEventName`) plays that event, from its own code.

use anyhow::Result;
use ounce_formats::{BymlDoc, Romfs, par_map};

use super::{Graph, Kind, Link};
use crate::checks::{GYAML, actors, banc_files, hash_of, str_field};
use crate::events::Flows;

/// The end of every param key that names an event.
const EVENT_NAME: &str = "EventName";

/// One placement naming an event: its actor, hash, area, param and event.
type Names = (String, u64, String, String, String);

/// Adds each placement that names an event row as playing it.
pub(super) fn add(g: &mut Graph, romfs: &Romfs, flows: &Flows) -> Result<()> {
    let files = banc_files(romfs)?;
    let found = par_map(&files, |rel| names(romfs, rel, flows));
    for names in found {
        for (actor, hash, area, param, event) in names? {
            let place = g.node(Kind::Placement, &format!("{actor} {hash}"), || {
                Some(format!("in {area}"))
            });
            let row = g.node(Kind::Event, &event, || None);
            g.edge(place, row, Link::Plays, param, &area, None);
        }
    }
    Ok(())
}

/// The placements in `rel` that name an event row, skipping files that
/// never say `EventName` unparsed.
fn names(romfs: &Romfs, rel: &str, flows: &Flows) -> Result<Vec<Names>> {
    let data = romfs.read(rel)?;
    if memchr::memmem::find(&data, EVENT_NAME.as_bytes()).is_none() {
        return Ok(Vec::new());
    }
    let doc = BymlDoc::parse(&data)?;
    let area = crate::checks::area_of(rel);
    let mut out = Vec::new();
    for a in actors(&doc.root)? {
        let (Some(actor), Some(hash), Ok(map)) = (str_field(a, GYAML), hash_of(a), a.as_map())
        else {
            continue;
        };
        let Some(Ok(dynamic)) = map.get("Dynamic").map(|d| d.as_map()) else {
            continue;
        };
        for (key, value) in dynamic.iter() {
            let Ok(event) = value.as_string() else {
                continue;
            };
            if key.ends_with(EVENT_NAME) && flows.rows().contains_key(event.as_str()) {
                let (actor, area) = (actor.to_owned(), area.to_owned());
                out.push((actor, hash, area, key.to_string(), event.to_string()));
            }
        }
    }
    Ok(out)
}
