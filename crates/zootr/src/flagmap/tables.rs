//! The flag map's data tables: each row of the pouch's actor table, and
//! each actor on a page of the pause menu's pouch table, is a state its
//! actor has. A field naming another actor's param links the row to that
//! actor, and a row with one age's field of a pair and not the other's is
//! asked by `age`: the Iron Boots' `AdultEquipment` with no
//! `ChildEquipment`, their menu entry's `IsUsableAdult` with no
//! `IsUsableChild`.

use anyhow::Result;
use ounce_formats::{Byml, BymlDoc, BymlMap, Romfs};

use super::asks::Game;
use super::{Graph, Kind, Link};
use crate::game::POUCH_TABLE;
use crate::items::show;
use crate::known::{Name, field};
use crate::rsdb::ROW_ID;
use crate::standins::{POUCH_ACTOR_INFO, row_id};

/// Each table's (child, adult) field pairs.
const AGES: [(&Name, &Name); 2] = [
    (field::CHILD_EQUIPMENT, field::ADULT_EQUIPMENT),
    (field::IS_USABLE_CHILD, field::IS_USABLE_ADULT),
];

/// How a field names an actor: by its param file.
const PARAM: (&str, &str) = ("Work/Actor/", ".engine__actor__ActorParam.gyml");

/// The tables' names as the map shows them.
const POUCH_INFO_NAME: &str = "PouchActorInfo";
const POUCH_TABLE_NAME: &str = "PouchTable";

/// Adds every pouch row and pouch table entry.
pub(super) fn add(g: &mut Graph, romfs: &Romfs) -> Result<()> {
    let info = BymlDoc::parse(&romfs.read(POUCH_ACTOR_INFO)?)?;
    for r in info.root.as_array()? {
        if let Ok(map) = r.as_map() {
            row(g, (POUCH_INFO_NAME, POUCH_ACTOR_INFO), row_id(r), map);
        }
    }
    let pouch = BymlDoc::parse(&romfs.read(POUCH_TABLE)?)?;
    let pages = (pouch.root.as_map()?.iter())
        .filter_map(|(page, slots)| Some((page, slots.as_array().ok()?)));
    for (page, slots) in pages {
        let actors = (slots.iter())
            .filter_map(|slot| slot.as_map().ok()?.get(field::ACTOR.get())?.as_array().ok());
        for map in actors.flatten().filter_map(|a| a.as_map().ok()) {
            let Some(name) = map
                .get(field::ACTOR_SHORT.get())
                .and_then(|n| n.as_string().ok())
            else {
                continue;
            };
            let mut map = map.clone();
            map.insert(page.as_str().into(), Byml::Bool(true));
            row(g, (POUCH_TABLE_NAME, POUCH_TABLE), name, &map);
        }
    }
    Ok(())
}

/// One row: a state `actor` has, the actors its fields name, and `age`
/// when it opens to one age only.
fn row(g: &mut Graph, (table, file): (&str, &str), actor: &str, map: &BymlMap) {
    if actor.is_empty() {
        return;
    }
    let mut fields = map.clone();
    fields.remove(ROW_ID);
    let state = g.node(Kind::State, &format!("{table}.{actor}"), || {
        Some(format!("row of {table}: {}", show(&Byml::Map(fields))))
    });
    let owner = g.node(Kind::Actor, actor, || None);
    g.edge(owner, state, Link::Has, table.to_owned(), file, None);
    let mut named: Vec<_> = (map.iter())
        .filter_map(|(key, v)| {
            let v = v.as_string().ok()?;
            let name = v.strip_prefix(PARAM.0)?.strip_suffix(PARAM.1)?;
            Some((key.to_string(), name.to_owned()))
        })
        .collect();
    named.sort();
    for (key, name) in named {
        let to = g.node(Kind::Actor, &name, || None);
        g.edge(state, to, Link::Has, key, file, None);
    }
    for (child, adult) in AGES {
        let label = match (map.contains_key(child.get()), map.contains_key(adult.get())) {
            (true, false) => format!("child only: {child}, no {adult}"),
            (false, true) => format!("adult only: {adult}, no {child}"),
            _ => continue,
        };
        let age = g.node(Kind::Game, Game::Age.word(), || None);
        g.edge(age, state, Link::Asks, label, file, None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::Graph;
    use super::Game;
    use crate::known::{actor, field};

    /// From the Iron Boots: their pouch rows, the adult-only fields that
    /// keep a child out of them, and the game code that checks and changes
    /// boots, child Link's own boots change included.
    #[test]
    fn the_iron_boots_reach_their_rows_their_age_limits_and_boot_code() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let g = Graph::build(&romfs).unwrap();
        let find = |id: String| g.find(&id).unwrap_or_else(|e| panic!("{e:#}"));
        let path = |a: usize, b: usize| !g.trace(a, b).paths.is_empty();
        let (boots, armor) = (&**actor::ITEM_HEAVY_BOOT, &**actor::ARMOR_ADULT_HEAVY_BOOT);
        let item = find(format!("actor:{boots}"));
        let worn = find(format!("actor:{armor}"));
        let info = find(format!("state:{}.{boots}", super::POUCH_INFO_NAME));
        let menu = find(format!("state:{}.{armor}", super::POUCH_TABLE_NAME));
        assert!(path(item, info) && path(info, worn) && path(worn, menu));
        let age = find(format!("game:{}", Game::Age.word()));
        let asked = |state: usize, field: &crate::known::Name| {
            g.ins[state].iter().any(|&e| {
                let e = &g.edges[e];
                e.from == age && e.label.starts_with("adult only") && e.label.contains(field.get())
            })
        };
        assert!(
            asked(info, field::ADULT_EQUIPMENT),
            "pouch row isn't adult only"
        );
        assert!(
            asked(menu, field::IS_USABLE_ADULT),
            "menu entry isn't adult only"
        );
        let down = g.walk(item, super::super::Dir::Down, 3, None);
        let limits = down
            .edges
            .iter()
            .filter(|l| l.from.id == format!("game:{}", Game::Age.word()));
        assert_eq!(
            limits.count(),
            2,
            "down from the boots shows both age limits"
        );
        let change = find(format!("class:{}", field::EXECUTE_PLAYER_BOOTS_CHANGE));
        let child = &**actor::PLAYER_CHILD;
        let child = find(format!(
            "graph:{child}/{}.action.bootschange.module",
            actor::PLAYER
        ));
        assert!(path(worn, change) && path(worn, child));
    }
}
