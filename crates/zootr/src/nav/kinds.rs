//! The words that name a placement (tree, chest, door) and how `use` acts
//! on each. Data only, nothing here presses a button.

use crate::around::seen;
use crate::play::PRESS;

/// How `use` gets a thing to answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// Stand `stand` m in front of it, face it and press A.
    Face { stand: f32 },
    /// Stand `stand` m in front of it, face it and hold each of `by` in
    /// turn (the sword, a roll), then step onto its spot: it breaks where
    /// it stands and drops what it held there.
    Break { stand: f32, by: Blow },
    /// Stand `stand` m before it and walk through. A doesn't open it.
    Through { stand: f32 },
    /// Walk over it.
    Over,
}

/// The buttons that break a thing, each held for its milliseconds in
/// turn, the camera behind Link so `LUp` is straight ahead.
pub type Blow = &'static [(&'static str, u64)];

/// The sword: Y, as B in the remake does nothing with it.
pub const SWORD: Blow = &[("Y", PRESS)];

/// A running roll forward, the only thing that breaks a box ("breaks when
/// it takes the player's forward roll", its damage parameters say). A
/// standing roll only bonks off it, and A beside it climbs it. A step of
/// run-up is enough: from 3 m with a longer run, the roll ended short of
/// the box (Market guard house, Mac, 2026-11-15).
pub const ROLL: Blow = &[("LUp", 100), ("LUp+A", 400)];

/// A to lift a rock over Link's head, held until the lift is over (an A
/// sooner is lost), then A to throw it: it breaks where it lands, a step
/// or two ahead.
pub const THROW: Blow = &[("A", 1500), ("A", PRESS)];

/// A kind of placement: the name segment that marks it, the word that
/// shows for it and how `use` acts, when it can be used.
#[derive(Debug, PartialEq)]
pub struct Kind {
    pub segment: &'static str,
    pub word: &'static str,
    pub act: Option<Act>,
}

const fn kind(segment: &'static str, word: &'static str, act: Option<Act>) -> Kind {
    Kind { segment, word, act }
}

/// In the order they're tried: a scrub's actor is a scrub before it
/// is an enemy.
const KINDS: [Kind; 16] = [
    kind("Door", "door", Some(Act::Through { stand: 1.0 })),
    kind("TBox", "chest", Some(Act::Face { stand: 0.8 })),
    kind(
        "Pot",
        "pot",
        Some(Act::Break {
            stand: 0.7,
            by: SWORD,
        }),
    ),
    kind(
        "Box",
        "box",
        Some(Act::Break {
            stand: 1.4,
            by: ROLL,
        }),
    ),
    kind(
        "Rock",
        "rock",
        Some(Act::Break {
            stand: 0.8,
            by: THROW,
        }),
    ),
    // A heart piece or container lying out: A takes it.
    kind("Piece", "item", Some(Act::Face { stand: 0.5 })),
    kind("Scrub", "scrub", Some(Act::Face { stand: 1.2 })),
    kind("Sign", "sign", Some(Act::Face { stand: 1.2 })),
    kind("Npc", "person", Some(Act::Face { stand: 1.2 })),
    kind("Enemy", "enemy", None),
    kind("Item", "item", Some(Act::Over)),
    kind("Container", "item", Some(Act::Face { stand: 0.5 })),
    kind("Tree", "tree", None),
    kind("Cliff", "cliff", None),
    kind("Interior", "house", None),
    kind("House", "house", None),
];

/// What kind of thing a placement named `name` is, by the first of its
/// `_` separated parts that ends the way a kind does, so `DekuTree` is a
/// tree.
pub fn kind_of(name: &str) -> Option<&'static Kind> {
    KINDS
        .iter()
        .find(|k| name.split('_').any(|part| part.ends_with(k.segment)))
}

/// The word for a placement's name.
pub fn word(name: &str) -> Option<&'static str> {
    kind_of(name).map(|k| k.word)
}

/// What to call a placement that Link can see or touch, or that is part of
/// the scenery (`Land_`): its word, else its own name when he can see it.
/// `None` for areas, anchors and locators, which have no business naming a
/// wall.
pub fn label(name: &str) -> Option<String> {
    let seen = seen(name);
    match word(name) {
        Some(w) if seen || name.starts_with("Land_") => Some(w.to_owned()),
        None if seen => Some(name.to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    #[test]
    fn placements_are_named_by_their_kind() {
        needs_names!();
        let cases: [(&str, &str); 10] = [
            (actor::LAND_KOKIRI_CLIFF_E_05, "cliff"),
            ("Tree_Kokiri_A", "tree"),
            (actor::LAND_KOKIRI_DEKU_TREE_A_01, "tree"),
            (actor::OBJ_T_BOX_A_01, "chest"),
            (actor::OBJ_KOKIRI_DOOR_A_02, "door"),
            (actor::NPC_MIDO, "person"),
            (actor::ENEMY_DEKUBABA_WITHERED, "enemy"),
            (actor::ENEMY_SCRUB_BUSINESS, "scrub"),
            (actor::LAND_KOKIRI_INTERIOR_A_01, "house"),
            (actor::LAND_KOKIRI_HOUSE_OUTSIDE_D_00, "house"),
        ];
        for (name, word) in cases {
            assert_eq!(super::word(name), Some(word), "{name}");
        }
        assert_eq!(super::word("Obj_M"), None);
    }

    #[test]
    fn an_unknown_name_shows_raw_when_he_can_see_it() {
        needs_names!();
        assert_eq!(label("Obj_M").as_deref(), Some("Obj_M"));
        assert_eq!(label(actor::OBJ_T_BOX_A_01).as_deref(), Some("chest"));
        assert_eq!(
            label(actor::LAND_KOKIRI_CLIFF_E_05).as_deref(),
            Some("cliff")
        );
        assert_eq!(label(actor::LAND_KOKIRI_FIELD_PROP_A_01), None);
        // An area around a chest is no chest.
        assert_eq!(label(actor::DROP_KEEP_AWAY_AREA_T_BOX_A_01), None);
    }

    #[test]
    fn use_acts_by_kind() {
        needs_names!();
        let act = |name| kind_of(name).and_then(|k| k.act);
        assert_eq!(act(actor::OBJ_T_BOX_A_01), Some(Act::Face { stand: 0.8 }));
        assert_eq!(act(actor::NPC_MIDO), Some(Act::Face { stand: 1.2 }));
        let sword = Act::Break {
            stand: 0.7,
            by: SWORD,
        };
        assert_eq!(act(actor::OBJ_POT_A_01), Some(sword));
        let roll = Act::Break {
            stand: 1.4,
            by: ROLL,
        };
        assert_eq!(act(actor::OBJ_BROKEN_BOX_A_01), Some(roll));
        let throw = Act::Break {
            stand: 0.8,
            by: THROW,
        };
        assert_eq!(act(actor::OBJ_LIFT_ROCK_A_01), Some(throw));
        assert_eq!(
            act(actor::ENEMY_SCRUB_DEKU_LEAF),
            Some(Act::Face { stand: 1.2 })
        );
        assert_eq!(
            act(actor::OBJ_KOKIRI_DOOR_A_02),
            Some(Act::Through { stand: 1.0 })
        );
        assert_eq!(act("Item_G"), Some(Act::Over));
        assert_eq!(act("Tree_Kokiri_A"), None);
    }
}
