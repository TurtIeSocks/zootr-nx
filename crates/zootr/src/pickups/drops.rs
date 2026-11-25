//! What a holder drops for the item it holds: the pickup itself where one
//! shows it, a stand-in built on a template otherwise, and what a holder
//! with no fixed drop brings from its drop table.

use super::DROP;
use crate::known::{Name, actor, field as fields, map};
use crate::warp;

/// A pickup that gives a major item with its get, the stand-ins' template.
pub(super) const TEMPLATE: &Name = actor::OBJ_ARROW_FIRE;
/// The template of a stand-in a holder drops: a pickup that gives with the
/// same get, but falls, bounces and lands as drops do. One built on
/// [`TEMPLATE`], which lies placed and never falls, drops through the floor
/// (Mac, 2026-11-14).
pub(super) const DROPPED: &Name = actor::OBJ_SHIELD_DEKU;
/// Pickups the game only places, never drops. They have no contact with
/// the ground, so a holder drops a stand-in for them.
pub(super) const PLACED_ONLY: [&Name; 2] = [actor::OBJ_HEART_PIECE, actor::OBJ_HEART_CONTAINER];
/// World pickups already, which a holder gives as they are.
const PICKUP: &str = "Obj_";
/// A chest's rupee, which a holder drops as the floor rupee of its color:
/// a stand-in rupee pays 1 (Mac, 2026-11-14).
const CHEST_RUPEE: &str = "TBox_Rupee_";
const FLOOR_RUPEE: &str = "Obj_Rupee_";

/// The pickup a holder places for `item` as it is, when there is one that
/// shows where it's put: one that lands, when the holder `dropped` it.
pub(super) fn pickup_for(item: &str, dropped: bool) -> Option<String> {
    let pickup = match item.strip_prefix(CHEST_RUPEE) {
        Some(color) => format!("{FLOOR_RUPEE}{color}"),
        None => item.starts_with(PICKUP).then(|| item.to_owned())?,
    };
    (!dropped || !PLACED_ONLY.iter().any(|p| **p == *pickup)).then_some(pickup)
}

/// What a holder placed without a fixed drop brings to the pool: what its
/// drop table always gives, Deku seeds in a child's file or arrows in an
/// adult's for a range weapon table, and a Recovery Heart for the rest (a
/// table picking what Link is lowest on, or none), as Ship does.
pub(super) fn own_drop(table: Option<&str>, file: &str) -> &'static Name {
    const ALWAYS: [(&Name, &Name); 6] = [
        (fields::DROP_TABLE_OBJ_HEART, actor::OBJ_HEART),
        (
            fields::DROP_TABLE_OBJ_MAGIC_JAR_SMALL,
            actor::OBJ_MAGIC_JAR_SMALL,
        ),
        (fields::DROP_TABLE_OBJ_BOMB, actor::OBJ_BOMB),
        (fields::DROP_TABLE_OBJ_DEKU_STICK, actor::OBJ_DEKU_STICK),
        (fields::DROP_TABLE_OBJ_FAIRY, actor::OBJ_FAIRY),
        (fields::DROP_RUPEE_RED2, actor::OBJ_RUPEE_RED),
    ];
    const RANGE: [&Name; 2] = [
        fields::DROP_TABLE_RANGE_WEAPON_01,
        fields::DROP_TABLE_RANGE_WEAPON_02,
    ];
    let Some(table) = table else {
        return actor::OBJ_HEART;
    };
    if let Some((_, item)) = ALWAYS.iter().find(|(t, _)| **t == *table) {
        return item;
    }
    match RANGE.iter().any(|t| **t == *table) {
        true if child_file(file) => actor::OBJ_DEKU_SEED,
        true => actor::OBJ_ARROW_BUNDLE_SMALL,
        false => actor::OBJ_HEART,
    }
}

/// The dungeons only a child plays, whose files name no age.
const CHILD_DUNGEONS: [&Name; 4] = [
    map::INSIDE_THE_DEKU_TREE,
    map::DODONGO_CAVERN,
    map::INSIDE_JABU_JABU_BELLY,
    map::BOTTOM_OF_THE_WELL,
];

/// Whether Link is a child in Banc file `file`.
fn child_file(file: &str) -> bool {
    let map = warp::map_of(file);
    warp::era(map).unwrap_or_else(|| CHILD_DUNGEONS.iter().any(|d| **d == *map))
}

/// The pickup a stand-in for what `field` names is built on.
pub(super) fn template_for(field: &str) -> &'static str {
    match field {
        DROP => DROPPED,
        _ => TEMPLATE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::GYAML;
    use crate::known::{field, item};

    #[test]
    fn a_chest_rupee_drops_as_the_floor_rupee_and_a_major_item_as_a_stand_in() {
        needs_names!();
        assert_eq!(
            pickup_for(actor::T_BOX_RUPEE_RED, true).as_deref(),
            Some(actor::OBJ_RUPEE_RED.get())
        );
        assert_eq!(
            pickup_for(actor::OBJ_HEART_PIECE, false).as_deref(),
            Some(actor::OBJ_HEART_PIECE.get())
        );
        assert_eq!(pickup_for(item::SHIELD_MIRROR, false), None);
    }

    #[test]
    fn a_heart_piece_a_holder_drops_is_a_stand_in_that_lands() {
        needs_names!();
        assert_eq!(pickup_for(actor::OBJ_HEART_PIECE, true), None);
        assert_eq!(pickup_for(actor::OBJ_HEART_CONTAINER, true), None);
        assert_eq!(template_for(DROP), DROPPED);
        assert_eq!(template_for(GYAML), TEMPLATE);
    }

    #[test]
    fn a_holder_without_a_fixed_drop_brings_what_its_table_gives() {
        needs_names!();
        let file = |map: &Name| format!("Banc/X/{}_Dynamic.bcett.byml.zs", map.get());
        let (child, adult) = (file(map::GORON_CITY_CHILD), file(map::FOREST_TEMPLE));
        let stick = Some(field::DROP_TABLE_OBJ_DEKU_STICK.get());
        assert_eq!(own_drop(stick, &adult), actor::OBJ_DEKU_STICK);
        let range = Some(field::DROP_TABLE_RANGE_WEAPON_02.get());
        assert_eq!(own_drop(range, &child), actor::OBJ_DEKU_SEED);
        assert_eq!(own_drop(range, &adult), actor::OBJ_ARROW_BUNDLE_SMALL);
        // A child's dungeon names no age.
        assert_eq!(
            own_drop(range, &file(map::DODONGO_CAVERN)),
            actor::OBJ_DEKU_SEED
        );
        assert_eq!(own_drop(Some("Drop_FakeSupply"), &child), actor::OBJ_HEART);
        assert_eq!(own_drop(None, &child), actor::OBJ_HEART);
    }
}
