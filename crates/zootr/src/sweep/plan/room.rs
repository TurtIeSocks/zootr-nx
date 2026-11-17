//! What a room needs set before its spot can be reached: water drained or
//! lowered, a note read beside a chest.

use std::sync::LazyLock;

use crate::known::{Name, actor, field, flag, flow};

/// A map with water that play drains before its low spots can be
/// reached, and the event that drains it: the Well's first room stays
/// underwater until Zelda's Lullaby is played at its Triforce point. Its
/// flags set alone leave the water up, the event lowers it.
const DRAINS: [(&Name, &Name); 1] = [(
    flow::BOTTOM_OF_THE_WELL,
    field::BOTTOM_OF_THE_WELL_LEVEL_ROOM01_WATER_LEVEL_DOWN,
)];

/// The event that drains `map`'s water, if play drains any.
pub(super) fn drain(map: &str) -> Option<String> {
    let mut of = DRAINS.iter().filter(|(m, _)| *m == map);
    of.next().map(|(_, event)| event.to_string())
}

/// Maps whose water level play changes, and the level that has their low
/// rooms dry. A new file starts the Water Temple at 2, its lower rooms
/// flooded and their chests under the surface. At 0, as Zelda's Lullaby at
/// the lowest tablet leaves it, they're dry (Mac, 2026-11-14). The map
/// reads the level as it loads, so it's held like a flag. The Forest
/// Temple's well stays full, its chest under the surface, until a switch
/// sets its Bool (the sweep swam above the chest on every side,
/// 2026-11-15). Morpha's pool stays over the boss heart until her defeat
/// sets its Bool: the quest step alone left the heart 5 m under water
/// (0 of 36 sweeps, Mac 2026-11-16).
static WATER: LazyLock<[(&Name, String); 3]> = LazyLock::new(|| {
    [
        (
            flag::WATER_TEMPLE,
            format!("{}=0", field::WATER_TEMPLE_ROOM00_WATER_LEVEL),
        ),
        (flag::WATER_TEMPLE, flag::MORPHA_DELETE_WATER.to_string()),
        (
            flag::FOREST_TEMPLE,
            field::FOREST_TEMPLE_ROOM07_CHANGE_WATER_LEVEL_ACTOR.to_string(),
        ),
    ]
});

/// The water level `map` is planned at, if play changes it.
pub(super) fn water(map: &str) -> Vec<String> {
    let of = WATER.iter().filter(|(m, _)| *m == map);
    of.map(|(_, level)| level.to_string()).collect()
}

/// Chests whose own flow gives the item only once a note beside them is
/// read, by holder, with the flag the note sets. Unread, the Hyrule Field
/// note chest (the field's eleventh note chest) opens empty and Navi talks.
/// It gives only to a child, and its child map is planned first.
const NOTED: [(&Name, &Name); 1] = [(
    actor::OBJ_T_BOX_A_01_MEMO11,
    flag::HAVE_READ_HRL_FIELD_NOTE11,
)];

/// The flags a chest's note must have set before it gives its item.
pub(super) fn noted(holder: &str) -> Vec<String> {
    let of = NOTED.iter().filter(|(h, _)| *h == holder);
    of.map(|(_, flag)| flag.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::tests::seed;
    use super::super::{Planner, Trigger};
    use super::*;
    use crate::known::map;

    #[test]
    fn a_chest_in_the_well_is_planned_with_its_water_drained() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        // The Well's Lower Blue Rupee 1, underwater until the lullaby.
        let lower = planned.iter().find(|p| p.spot.hash == 12988877782812797760);
        let Some(Trigger::Open { first, .. }) = lower.map(|p| &p.trigger) else {
            panic!("{lower:?}");
        };
        let drain = field::BOTTOM_OF_THE_WELL_LEVEL_ROOM01_WATER_LEVEL_DOWN;
        assert_eq!(first.as_deref(), Some(&**drain));
    }

    #[test]
    fn the_field_note_chest_is_opened_by_a_child_with_its_note_read() {
        let Some((_, _, planned)) = seed() else {
            return;
        };
        let chest = planned.iter().find(|p| p.spot.hash == 2967378296224661060);
        let Some(Trigger::Open { ready, map, .. }) = chest.map(|p| &p.trigger) else {
            panic!("{chest:?}");
        };
        assert_eq!(ready, &[flag::HAVE_READ_HRL_FIELD_NOTE11]);
        assert_eq!(map, map::HYRULE_FIELD_CHILD);
    }

    #[test]
    fn a_water_temple_chest_is_planned_at_low_water_with_its_gate() {
        needs_names!();
        assert_eq!(
            water(flag::WATER_TEMPLE),
            [
                format!("{}=0", field::WATER_TEMPLE_ROOM00_WATER_LEVEL),
                flag::MORPHA_DELETE_WATER.to_string()
            ]
        );
        assert!(water(flow::BOTTOM_OF_THE_WELL).is_empty());
        assert_eq!(
            water(flag::FOREST_TEMPLE),
            [field::FOREST_TEMPLE_ROOM07_CHANGE_WATER_LEVEL_ACTOR.to_string()]
        );
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut planner = Planner::new(&romfs, BTreeSet::new());
        let (ready, ..) = planner.appear(18099019250896853049).unwrap();
        assert_eq!(ready, [flag::WATER_TEMPLE_ROOM18_TRESURE]);
    }
}
