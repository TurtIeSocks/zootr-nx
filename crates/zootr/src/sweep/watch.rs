//! What counts as the item arriving: the flags, stock and get-item
//! message [`Watch`] reads, and the names an item lands as.

use anyhow::{Context, Result};
use ounce_formats::{BymlDoc, Romfs};

use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::inventory;
use crate::known::{Name, actor, field, flag};
use crate::live;
use crate::progressive;
use crate::savefile::Change;
use crate::standins;

/// The item's IsGet flags and stock, and the get-item message, to tell
/// when it lands.
pub(super) struct Watch {
    /// The names `item` lands as, [`owned_as`].
    pub(super) items: Vec<String>,
    /// The `IsGet` flags, taken away before the spot so they can change.
    pub(super) flags: Vec<u32>,
    pub(super) stock: Option<u32>,
    /// [`MESSAGE`], emptied before the spot.
    pub(super) message: u32,
}

/// The item a get-item window last named. A chest's rupee leaves its
/// `IsGet` alone once Link has had one, so this is the sure sign.
pub(super) const MESSAGE: &Name = flag::UI_MESSAGE_TARGET_ACTOR_NAME;

/// Whether `named` is the game's own stand-in for `item`: arrows come as a
/// rupee until Link has a bow, and a new file has none.
pub(super) fn swapped(item: &str, named: &str) -> bool {
    item.contains("ArrowBundle") && named.contains("Rupee")
}

/// The item `changes` say a get-item window named.
pub(super) fn named(changes: &[Change]) -> Option<String> {
    let c = changes.iter().find(|c| c.name == MESSAGE)?;
    let to = c.change.rsplit(" -> ").next()?.trim_matches('"');
    (!to.is_empty()).then(|| to.to_owned())
}

/// Whether `c` is the harness's own Blue Fire, given to melt red ice: its
/// get flag, or the get window naming it.
pub(super) fn poured(c: &Change) -> bool {
    let fire = actor::ITEM_BOTTLE_BLUE_FIRE.get();
    c.name == format!("IsGet.{fire}")
        || c.name == MESSAGE
            && c.change.rsplit(" -> ").next().map(|t| t.trim_matches('"')) == Some(fire)
}

/// A rupee lying on the floor, which Link can pick up beside any spot. A
/// chest's rupee names its chest drop in the get window instead.
pub(super) const FLOOR_RUPEE: &str = "Obj_Rupee_";

/// The first get outside `wanted` that `changes` show: an `IsGet` flag
/// turned on, else the item a get window named. A flag turned off is a load
/// or a reset, and a floor rupee a pickup beside the spot, never its get.
pub(super) fn got_other(changes: &[Change], wanted: &[String]) -> Option<String> {
    let flag = changes.iter().find(|c| {
        c.name
            .strip_prefix("IsGet.")
            .is_some_and(|item| !item.starts_with(FLOOR_RUPEE))
            && !wanted.contains(&c.name)
            && c.change.ends_with("-> true")
    });
    let named = named(changes).filter(|n| !n.starts_with(FLOOR_RUPEE));
    (flag.map(|c| c.name.clone())).or(named.map(|n| format!("{MESSAGE} {n}")))
}

/// Whether `other`, what [`got_other`] found, is the get of an item that
/// lands as one of `names`.
pub(super) fn lands_as(other: &str, names: &[String]) -> bool {
    let message = format!("{MESSAGE} ");
    let item = (other.strip_prefix("IsGet.")).or_else(|| other.strip_prefix(&message));
    item.is_some_and(|item| names.iter().any(|n| n == item))
}

/// The items whose `IsGet` tells `item` landed: itself, a chest item's
/// field twin, which its get sets instead (a chest rupee sets
/// the floor rupee's flag), and every level of its upgrade line, as the
/// hook gives the next level Link lacks (a small bag lands as
/// the medium one once he has a bag).
pub(super) fn twins(item: &str) -> Vec<String> {
    let twin = item.strip_prefix("TBox_").map(|rest| format!("Obj_{rest}"));
    let names = std::iter::once(item.to_owned()).chain(twin);
    let names = names.chain(progressive::levels(item).into_iter().map(str::to_owned));
    let mut out = Vec::new();
    for name in names {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// The pouch row fields naming the gear a pickup is owned as.
pub(super) const GEAR: [&Name; 2] = [field::ADULT_EQUIPMENT, field::CHILD_EQUIPMENT];

/// [`twins`], and the gear its pouch row says `item` is owned as:
/// a pickup's actor and its gear's flag can differ.
pub(super) fn owned_as(romfs: &Romfs, item: &str) -> Result<Vec<String>> {
    let mut out = twins(item);
    let table = standins::POUCH_ACTOR_INFO;
    let doc = BymlDoc::parse(&romfs.read(table)?).with_context(|| table.to_owned())?;
    let Ok(row) = standins::row(&doc, item) else {
        return Ok(out);
    };
    for path in GEAR
        .iter()
        .filter_map(|k| row.get(k.get())?.as_string().ok())
    {
        let file = path.rsplit('/').next().unwrap_or(path.as_str());
        let actor = file.split('.').next().unwrap_or(file).to_owned();
        if !out.contains(&actor) {
            out.push(actor);
        }
    }
    Ok(out)
}

impl Watch {
    pub(super) fn new(gd: &GameData, romfs: &Romfs, item: &str) -> Result<Self> {
        let items = owned_as(romfs, item)?;
        let flags = (items.iter())
            .filter(|t| inventory::has_flag(gd, t))
            .map(|t| live::hash(&format!("IsGet.{t}")))
            .collect::<Result<_>>()?;
        let stock = inventory::stock(gd, item)?;
        let message = live::hash(MESSAGE)?;
        Ok(Self {
            items,
            flags,
            stock,
            message,
        })
    }

    pub(super) fn hashes(&self) -> impl Iterator<Item = u32> + '_ {
        let message = std::iter::once(self.message);
        self.flags.iter().copied().chain(self.stock).chain(message)
    }

    pub(super) fn names(&self, gd: &GameData) -> Vec<String> {
        self.hashes().map(|h| gd.name(h)).collect()
    }

    pub(super) fn read(&self, dev: &Dev, gd: &GameData) -> Result<Vec<String>> {
        let game = dev.game();
        (self.hashes())
            .map(|h| live::read(game, gd, dev.global(), h))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, item};

    #[test]
    fn a_chest_item_is_watched_with_its_field_twin() {
        needs_names!();
        assert_eq!(
            twins(actor::T_BOX_RUPEE_RED),
            [actor::T_BOX_RUPEE_RED, actor::OBJ_RUPEE_RED]
        );
        assert_eq!(twins("Item_A"), ["Item_A"]);
    }

    #[test]
    fn an_upgrade_is_watched_at_every_level_of_its_line() {
        needs_names!();
        let names = twins(item::RANGE_BOW);
        assert_eq!(names[0], item::RANGE_BOW);
        for n in [
            actor::WEAPON_RANGE_BOW,
            item::QUIVER_MEDIUM,
            item::QUIVER_LARGE,
        ] {
            assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
        }
        assert_eq!(
            twins(item::SCALE_SILVER),
            [item::SCALE_SILVER, item::SCALE_GOLD]
        );
    }

    #[test]
    fn a_gear_pickup_is_watched_as_the_gear_it_is_owned_as() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let names = owned_as(&romfs, actor::OBJ_ARMOR_GORON).unwrap();
        assert_eq!(names, [actor::OBJ_ARMOR_GORON, actor::ARMOR_ADULT_GORON]);
        assert_eq!(owned_as(&romfs, "Item_A").unwrap(), ["Item_A"]);
    }

    #[test]
    fn the_harness_blue_fire_is_told_apart() {
        needs_names!();
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let fire = actor::ITEM_BOTTLE_BLUE_FIRE;
        assert!(poured(&change(&format!("IsGet.{fire}"), "false -> true")));
        assert!(poured(&change(MESSAGE, &format!(r#""" -> "{fire}""#))));
        let chest = format!(r#""{fire}" -> "{}""#, actor::T_BOX_RUPEE_BLUE);
        assert!(!poured(&change(MESSAGE, &chest)));
        assert!(!poured(&change(
            &format!("IsGet.{}", actor::OBJ_RUPEE_RED),
            "false -> true"
        )));
    }

    #[test]
    fn a_get_message_names_its_item() {
        needs_names!();
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let got = [
            change(&format!("IsGet.{}", actor::OBJ_RUPEE_RED), "false -> true"),
            change(
                MESSAGE,
                &format!(
                    r#""{}" -> "{}""#,
                    actor::OBJ_HEART_CONTAINER,
                    actor::T_BOX_RUPEE_BLUE
                ),
            ),
        ];
        assert_eq!(named(&got).as_deref(), Some(actor::T_BOX_RUPEE_BLUE.get()));
        let cleared = [change(
            MESSAGE,
            &format!(r#""{}" -> """#, actor::T_BOX_RUPEE_BLUE),
        )];
        assert_eq!(named(&cleared), None);
        assert_eq!(named(&got[..1]), None);
    }

    #[test]
    fn only_a_flag_turned_on_is_another_get() {
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let wanted = [format!("IsGet.{}", item::HOVER_BOOT)];
        let lost = [change(
            &format!("IsGet.{}", actor::OBJ_HEART_CONTAINER),
            "true -> false",
        )];
        assert_eq!(got_other(&lost, &wanted), None);
        let got = [change("IsGet.Item_A", "false -> true")];
        assert_eq!(got_other(&got, &wanted).as_deref(), Some("IsGet.Item_A"));
        let ours = [change(
            &format!("IsGet.{}", item::HOVER_BOOT),
            "false -> true",
        )];
        assert_eq!(got_other(&ours, &wanted), None);
    }

    #[test]
    fn another_get_is_told_to_be_an_items_by_its_names() {
        needs_names!();
        let names = twins(actor::T_BOX_RUPEE_RED);
        let flag = format!("IsGet.{}", actor::OBJ_RUPEE_RED);
        assert!(lands_as(&flag, &names));
        let message = format!("{MESSAGE} {}", actor::T_BOX_RUPEE_RED);
        assert!(lands_as(&message, &names));
        assert!(!lands_as("IsGet.Item_A", &names));
        assert!(!lands_as(actor::OBJ_RUPEE_RED, &names), "neither form");
    }

    #[test]
    fn a_rupee_off_the_floor_is_not_another_get() {
        needs_names!();
        let change = |name: &str, change: &str| Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        };
        let wanted = [format!("IsGet.{}", item::NUT_BAG_MEDIUM)];
        let floor = [
            change(
                &format!("IsGet.{}", actor::OBJ_RUPEE_GREEN),
                "false -> true",
            ),
            change(MESSAGE, &format!(r#""" -> "{}""#, actor::OBJ_RUPEE_GREEN)),
        ];
        assert_eq!(got_other(&floor, &wanted), None);
        let chest = [
            change(
                &format!("IsGet.{}", actor::OBJ_RUPEE_GREEN),
                "false -> true",
            ),
            change(MESSAGE, &format!(r#""" -> "{}""#, actor::T_BOX_RUPEE_GREEN)),
        ];
        let got = got_other(&chest, &wanted);
        assert_eq!(
            got.as_deref(),
            Some(
                format!(
                    "{} {}",
                    flag::UI_MESSAGE_TARGET_ACTOR_NAME,
                    actor::T_BOX_RUPEE_GREEN
                )
                .as_str()
            )
        );
    }

    #[test]
    fn arrows_without_a_bow_come_as_a_rupee() {
        needs_names!();
        assert!(swapped(
            actor::T_BOX_ARROW_BUNDLE_MEDIUM,
            actor::T_BOX_RUPEE_RED
        ));
        assert!(!swapped(actor::T_BOX_ARROW_BUNDLE_MEDIUM, "Item_A"));
        assert!(!swapped(actor::T_BOX_RUPEE_BLUE, actor::T_BOX_RUPEE_RED));
    }
}
