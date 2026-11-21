//! The flag map's data tables: each row of the pouch's actor table, and
//! each actor on a page of the pause menu's pouch table, is a state its
//! actor has. A field naming another actor's param links the row to that
//! actor. A row with a field for one age and not the same field for the
//! other says so in its detail: the Iron Boots' `AdultEquipment` with no
//! `ChildEquipment`, their menu entry's `IsUsableAdult` with no
//! `IsUsableChild`.

use anyhow::Result;
use ounce_formats::{Byml, BymlDoc, BymlMap, Romfs};

use super::{Graph, Kind, Link};
use crate::game::POUCH_TABLE;
use crate::items::show;
use crate::known::field;
use crate::rsdb::ROW_ID;
use crate::standins::{POUCH_ACTOR_INFO, row_id};

/// The words in a field's name that say which age it's for.
const AGES: [(&str, &str); 2] = [("Child", "Adult"), ("Adult", "Child")];

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

/// One row: a state `actor` has, and the actors its fields name.
fn row(g: &mut Graph, (table, file): (&str, &str), actor: &str, map: &BymlMap) {
    if actor.is_empty() {
        return;
    }
    let mut fields = map.clone();
    fields.remove(ROW_ID);
    let state = g.node(Kind::State, &format!("{table}.{actor}"), || {
        let only = one_age(map).into_iter().map(|o| format!("; {o}"));
        let only: String = only.collect();
        Some(format!(
            "row of {table}: {}{only}",
            show(&Byml::Map(fields))
        ))
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
}

/// Each field the row has for one age only, as "adult only:
/// AdultEquipment, no ChildEquipment".
fn one_age(map: &BymlMap) -> Vec<String> {
    let mut only: Vec<_> = (map.keys())
        .flat_map(|key| AGES.map(|(this, other)| (key, this, other)))
        .filter(|(key, this, _)| key.contains(this))
        .map(|(key, this, other)| (key, this, key.replace(this, other)))
        .filter(|(_, _, twin)| !map.contains_key(twin.as_str()))
        .map(|(key, this, twin)| format!("{} only: {key}, no {twin}", this.to_lowercase()))
        .collect();
    only.sort();
    only
}

#[cfg(test)]
mod tests {
    use super::super::Graph;
    use crate::known::{Name, actor, field};

    /// From the Iron Boots: their pouch row, the worn actor, its menu
    /// entry, and the adult-only fields that keep a child out of them.
    #[test]
    fn the_iron_boots_reach_their_rows_and_age_limits() {
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
        let says = |state: usize, adult: &Name, child: &Name| {
            let detail = g.nodes[state].detail.as_deref().unwrap_or_default();
            detail.contains(&format!("adult only: {adult}, no {child}"))
        };
        let (adult, child) = (field::ADULT_EQUIPMENT, field::CHILD_EQUIPMENT);
        assert!(says(info, adult, child), "pouch row isn't adult only");
        let (adult, child) = (field::IS_USABLE_ADULT, field::IS_USABLE_CHILD);
        assert!(says(menu, adult, child), "menu entry isn't adult only");
    }

    #[test]
    fn a_field_for_one_age_says_so() {
        let mut map = ounce_formats::BymlMap::default();
        for key in ["FakeChildGear", "FakeAdultGear", "IsFakeAdult", "FakePrice"] {
            map.insert(key.into(), ounce_formats::Byml::Bool(true));
        }
        let want = "adult only: IsFakeAdult, no IsFakeChild";
        assert_eq!(super::one_age(&map), [want]);
    }
}
