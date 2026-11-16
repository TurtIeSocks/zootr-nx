//! The Status panel: Link's meters, gear, age and counters, each a save
//! value with the field it edits in. Most live in `PlayerStatus`, whose
//! members the dump doesn't name, so they're kept here as their hashes.

use ounce_formats::{Save, murmur3};
use serde::Serialize;

use crate::gamedata::GameData;
use crate::inventory::Pouch;
use crate::known::{Name, actor, flag, item};
use crate::names;
use crate::value::{self, Input};

/// Link's health in quarter hearts (`PlayerStatus.Life`).
pub(crate) const LIFE: u32 = crate::dev::HEARTS;
/// Link's heart containers, in quarter hearts.
pub(crate) const MAX_LIFE: u32 = 0xfbe01da1;
/// Link's magic.
const MP: u32 = 0x76b18200;
/// The size of Link's magic meter.
const MP_MAX: u32 = 0x12d63a97;
/// Link's rupees.
const RUPEES: u32 = 0xa77921d7;
/// The sword Link has out, an actor name.
const SWORD: u32 = 0xa95a3521;
/// The shield Link carries.
const SHIELD: u32 = 0xee463235;
/// The tunic Link wears.
const TUNIC: u32 = 0x008d7f0c;
/// The boots Link wears; empty for his own.
const BOOTS: u32 = 0xd107a85d;
/// The mask Link wears.
const MASK: u32 = 0x59d9c7b3;
/// The ocarina Link plays.
const OCARINA: u32 = 0xb72cd3fc;
/// The sword Link has out in the other age.
const OTHER_SWORD: u32 = 0xa1f7ee2b;
/// The shield Link carries in the other age.
const OTHER_SHIELD: u32 = 0x2342b619;
/// Hits the Giant's Knife has left.
const KNIFE_LIFE: u32 = 0xbdf672c2;

/// The most heart containers the game gives, in quarters.
const MAX_HEARTS: i32 = 20 * 4;

/// Where a field sits in the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum StatusGroup {
    Meters,
    Gear,
    Age,
    Counters,
}

/// One value a choice field can take, and what it reads as.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StatusChoice {
    pub value: String,
    pub label: String,
}

/// One field of the panel.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StatusField {
    pub group: StatusGroup,
    pub label: String,
    pub hash: u32,
    pub input: Input,
    /// As the save holds it.
    pub value: String,
    pub choices: Vec<StatusChoice>,
    /// How many of the save's units make one shown: 4 for hearts, kept
    /// in quarters.
    pub per: u32,
    /// The most it takes, in the save's units.
    pub max: Option<i32>,
    /// Why the value looks wrong, without refusing it.
    pub warn: Option<String>,
}

/// What a field reads and how it's edited.
struct Spec {
    group: StatusGroup,
    label: &'static str,
    hash: u32,
    per: u32,
    /// For a choice of gear: each item and its label.
    gear: &'static [(&'static Name, &'static str)],
}

const fn number(group: StatusGroup, label: &'static str, hash: u32, per: u32) -> Spec {
    Spec {
        group,
        label,
        hash,
        per,
        gear: &[],
    }
}

const fn gear(
    label: &'static str,
    hash: u32,
    gear: &'static [(&'static Name, &'static str)],
) -> Spec {
    Spec {
        gear,
        ..number(StatusGroup::Gear, label, hash, 1)
    }
}

const SWORDS: &[(&Name, &str)] = &[
    (actor::WEAPON_SWORD_KOKIRI, "Kokiri Sword"),
    (actor::WEAPON_SWORD_MASTER, "Master Sword"),
    (actor::WEAPON_SWORD_GIANT, "Giant's Knife"),
    (actor::WEAPON_SWORD_BIGGORON, "Biggoron's Sword"),
];
const SHIELDS: &[(&Name, &str)] = &[
    (actor::WEAPON_SHIELD_DEKU, "Deku Shield"),
    (actor::WEAPON_SHIELD_HYLIAN, "Hylian Shield"),
    (actor::WEAPON_SHIELD_MIRROR, "Mirror Shield"),
];

use StatusGroup::*;

#[rustfmt::skip]
const FIELDS: &[Spec] = &[
    number(Meters, "Heart containers", MAX_LIFE, 4),
    number(Meters, "Health", LIFE, 4),
    number(Meters, "Magic meter", MP_MAX, 1),
    number(Meters, "Magic", MP, 1),
    number(Meters, "Rupees", RUPEES, 1),
    gear("Sword", SWORD, SWORDS),
    gear("Shield", SHIELD, SHIELDS),
    gear("Tunic", TUNIC, &[
        (actor::ARMOR_ADULT_NORMAL, "Kokiri Tunic"),
        (actor::ARMOR_ADULT_GORON, "Goron Tunic"),
        (actor::ARMOR_ADULT_ZORA, "Zora Tunic"),
    ]),
    gear("Boots", BOOTS, &[
        (actor::ARMOR_ADULT_HEAVY_BOOT, "Iron Boots"),
        (actor::ARMOR_ADULT_HOVER_BOOT, "Hover Boots"),
    ]),
    gear("Mask", MASK, &[
        (actor::ARMOR_CHILD_MASK_KEATON, "Keaton Mask"),
        (actor::ARMOR_CHILD_MASK_SKULL, "Skull Mask"),
        (actor::ARMOR_CHILD_MASK_SPOOKY, "Spooky Mask"),
        (actor::ARMOR_CHILD_MASK_BUNNY, "Bunny Hood"),
        (actor::ARMOR_CHILD_MASK_TRUTH, "Mask of Truth"),
    ]),
    gear("Ocarina", OCARINA, &[
        (item::OCARINA_FAIRY, "Fairy Ocarina"),
        (item::OCARINA_TIME, "Ocarina of Time"),
    ]),
    gear("Other age's sword", OTHER_SWORD, SWORDS),
    gear("Other age's shield", OTHER_SHIELD, SHIELDS),
    number(Counters, "Giant's Knife hits left", KNIFE_LIFE, 1),
];

/// Every field `save` holds, read from it, with `pouch` saying what the
/// wallet allows.
pub fn status(gd: &GameData, pouch: &Pouch, save: &Save) -> Vec<StatusField> {
    let read = |hash: u32| -> Option<(String, String)> {
        let e = save.get(hash)?;
        Some((
            e.kind.to_owned(),
            value::decode(gd, e.kind, save.element(e, 0)?),
        ))
    };
    let int = |hash| read(hash).and_then(|(_, v)| v.parse::<i32>().ok());
    let age = murmur3(flag::SEQUENCE_IS_CHILDHOOD.as_bytes());
    let mut out: Vec<StatusField> = FIELDS
        .iter()
        .filter_map(|spec| {
            let (kind, value) = read(spec.hash)?;
            let max = match spec.hash {
                MAX_LIFE => Some(MAX_HEARTS),
                LIFE => int(MAX_LIFE),
                MP => int(MP_MAX),
                RUPEES => pouch.cap_of_bag(item::WALLET_SMALL.get(), save),
                _ => None,
            };
            let over = max
                .zip(value.parse::<i32>().ok())
                .is_some_and(|(m, v)| v > m);
            let warn = (over && spec.hash != MAX_LIFE).then(|| match spec.hash {
                RUPEES => "More than the wallet holds".to_owned(),
                _ => "More than the meter holds".to_owned(),
            });
            let input = match spec.gear.is_empty() {
                true => value::input(&kind),
                false => Input::Choice,
            };
            Some(StatusField {
                group: spec.group,
                label: spec.label.into(),
                hash: spec.hash,
                input,
                choices: choices(spec.gear, &value),
                value,
                per: spec.per,
                max,
                warn,
            })
        })
        .collect();
    if let Some((_, value)) = read(age) {
        let choice = |value: &str, label: &str| StatusChoice {
            value: value.into(),
            label: label.into(),
        };
        out.push(StatusField {
            group: Age,
            label: "Age".into(),
            hash: age,
            input: Input::Choice,
            value,
            choices: vec![choice("true", "Child"), choice("false", "Adult")],
            per: 1,
            max: None,
            warn: None,
        });
    }
    out
}

/// None, then each piece of gear, then `now` if it's something else, so
/// the field always shows what the save holds.
fn choices(gear: &[(&Name, &str)], now: &str) -> Vec<StatusChoice> {
    if gear.is_empty() {
        return Vec::new();
    }
    let none = StatusChoice {
        value: String::new(),
        label: "None".into(),
    };
    let mut out: Vec<StatusChoice> = std::iter::once(none)
        .chain(gear.iter().map(|(a, label)| StatusChoice {
            value: a.to_string(),
            label: (*label).into(),
        }))
        .collect();
    if !out.iter().any(|c| c.value == now) {
        out.push(StatusChoice {
            value: now.into(),
            label: names::pretty(now),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A save with Ints `(hash, value)` and String64 `(hash, text)`.
    fn save(ints: &[(u32, u32)], strings: &[(u32, &str)]) -> Save {
        let mut pairs = vec![(0, 2)];
        pairs.extend(ints);
        pairs.push((0, 16));
        pairs.extend(strings.iter().map(|&(h, _)| (h, 0)));
        let mut b = Save::fixture(&pairs);
        let first = b.len() - strings.len() * 8;
        for (i, (_, text)) in strings.iter().enumerate() {
            let at = b.len() as u32;
            b[first + i * 8 + 4..first + i * 8 + 8].copy_from_slice(&at.to_le_bytes());
            let mut t = text.as_bytes().to_vec();
            t.resize(64, 0);
            b.extend(t);
        }
        Save::parse(b).unwrap()
    }

    fn field<'a>(fields: &'a [StatusField], label: &str) -> &'a StatusField {
        fields.iter().find(|f| f.label == label).unwrap()
    }

    #[test]
    fn meters_read_in_their_units_and_warn_past_their_limit() {
        let gd = GameData::default();
        let save = save(&[(MAX_LIFE, 12), (LIFE, 16), (MP_MAX, 48), (MP, 10)], &[]);
        let fields = status(&gd, &Pouch::default(), &save);
        let hearts = field(&fields, "Heart containers");
        assert_eq!(
            (hearts.value.as_str(), hearts.per, hearts.max),
            ("12", 4, Some(80))
        );
        let health = field(&fields, "Health");
        assert_eq!(health.max, Some(12));
        assert!(health.warn.is_some());
        assert_eq!(field(&fields, "Magic").warn, None);
        assert!(fields.iter().all(|f| f.label != "Rupees"));
    }

    #[test]
    fn gear_offers_none_each_piece_and_whatever_the_save_holds() {
        needs_names!();
        let gd = GameData::default();
        let save = save(
            &[],
            &[
                (SWORD, actor::WEAPON_SWORD_MASTER.get()),
                (BOOTS, ""),
                (SHIELD, "Fake_Shield_Odd"),
            ],
        );
        let fields = status(&gd, &Pouch::default(), &save);
        let sword = field(&fields, "Sword");
        assert_eq!(sword.input, Input::Choice);
        assert_eq!(sword.value, actor::WEAPON_SWORD_MASTER.get());
        assert_eq!(sword.choices.len(), 5);
        assert_eq!(field(&fields, "Boots").choices[0].label, "None");
        let shield = field(&fields, "Shield");
        assert_eq!(shield.choices.last().unwrap().value, "Fake_Shield_Odd");
    }
}
