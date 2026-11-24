//! What counts as the item arriving: the flags, stock and get-item
//! message [`Watch`] reads, and the names an item lands as.

use anyhow::{Context, Result};
use ounce_formats::{BymlDoc, Romfs};

use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::inventory;
use crate::known::{Name, field, flag};
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

/// The value a change ends on: its last ` -> ` part, unquoted.
fn target(change: &str) -> &str {
    let to = change.rsplit(" -> ").next().unwrap_or(change);
    to.trim_matches('"')
}

/// The item `changes` say a get-item window named.
pub(super) fn named(changes: &[Change]) -> Option<String> {
    let c = changes.iter().find(|c| c.name == MESSAGE)?;
    let to = target(&c.change);
    (!to.is_empty()).then(|| to.to_owned())
}

/// Whether `c` is the harness's own `item`, given to wield: its get flag,
/// or the get window naming it.
pub(super) fn wielded(c: &Change, item: &Name) -> bool {
    let item = item.get();
    c.name == format!("IsGet.{item}") || (c.name == MESSAGE && target(&c.change) == item)
}

/// A rupee lying on the floor, which Link can pick up beside any spot. A
/// chest's rupee names its chest drop in the get window instead.
pub(super) const FLOOR_RUPEE: &str = "Obj_Rupee_";

/// A get other than the spot's own, as [`got_other`] finds it.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Other {
    /// An `IsGet` flag turned on: the item it's for.
    Flag(String),
    /// The item a get window named.
    Window(String),
}

impl Other {
    fn item(&self) -> &str {
        match self {
            Other::Flag(item) | Other::Window(item) => item,
        }
    }
}

/// As the report names it: the flag, or the window's value and its item.
impl std::fmt::Display for Other {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Other::Flag(item) => write!(f, "IsGet.{item}"),
            Other::Window(item) => write!(f, "{MESSAGE} {item}"),
        }
    }
}

/// The first get outside `wanted` that `changes` show: an `IsGet` flag
/// turned on, else the item a get window named. A flag turned off is a load
/// or a reset, and a floor rupee a pickup beside the spot, never its get.
/// A `windowed` holder (any but a broken one) gives through a get window,
/// so a flag with no window beside it is a pickup Link walked over: a
/// recovery heart by a chest in Jabu-Jabu's Belly that never opened
/// (other machine, 2026-11-17).
pub(super) fn got_other(changes: &[Change], wanted: &[String], windowed: bool) -> Option<Other> {
    let window = named(changes);
    let flag = changes.iter().find_map(|c| {
        let item = c.name.strip_prefix("IsGet.")?;
        let got = !item.starts_with(FLOOR_RUPEE)
            && !wanted.contains(&c.name)
            && c.change.ends_with("-> true");
        got.then(|| Other::Flag(item.to_owned()))
    });
    let flag = flag.filter(|_| !windowed || window.is_some());
    let named = window.filter(|n| !n.starts_with(FLOOR_RUPEE));
    flag.or_else(|| named.map(Other::Window))
}

/// Whether `other`, what [`got_other`] found, is the get of an item that
/// lands as one of `names`.
pub(super) fn lands_as(other: &Other, names: &[String]) -> bool {
    names.iter().any(|n| n == other.item())
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

/// [`twins`], the gear its pouch row says `item` is owned as (a pickup's
/// actor and its gear's flag can differ), and the `Item_` row of an `Obj_`
/// pickup that counts the same thing: a Deku Stick in a pot lands as the
/// stick's `Item_` row.
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
    let counts = |row: &ounce_formats::BymlMap| -> Option<String> {
        row.get(field::MAX_NUM_GAME_DATA.get())?
            .as_string()
            .ok()
            .map(|s| s.to_string())
    };
    let count = counts(&row);
    let item_row = item.strip_prefix("Obj_").map(|rest| format!("Item_{rest}"));
    if let Some(other) = item_row.filter(|o| count.is_some() && !out.contains(o))
        && standins::row(&doc, &other).is_ok_and(|o| counts(&o) == count)
    {
        out.push(other);
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
        self.hashes().map(|h| game.get(gd, h)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flag, item};

    fn change(name: &str, change: &str) -> Change {
        Change {
            hash: 0,
            name: name.into(),
            label: None,
            kind: String::new(),
            change: change.into(),
        }
    }

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
    fn a_pickup_is_watched_as_the_item_row_counting_the_same_thing() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let obj = |i: &Name| format!("Obj_{}", i.get().strip_prefix("Item_").unwrap());
        for i in [item::DEKU_STICK, item::DEKU_NUT, item::BOMB] {
            let names = owned_as(&romfs, &obj(i)).unwrap();
            assert!(names.iter().any(|n| n == i.get()), "{names:?}");
        }
    }

    #[test]
    fn the_harness_blue_fire_is_told_apart() {
        needs_names!();
        let fire = actor::ITEM_BOTTLE_BLUE_FIRE;
        assert!(wielded(
            &change(&format!("IsGet.{fire}"), "false -> true"),
            fire
        ));
        assert!(wielded(
            &change(MESSAGE, &format!(r#""" -> "{fire}""#)),
            fire
        ));
        let chest = format!(r#""{fire}" -> "{}""#, actor::T_BOX_RUPEE_BLUE);
        assert!(!wielded(&change(MESSAGE, &chest), fire));
        assert!(!wielded(
            &change(&format!("IsGet.{}", actor::OBJ_RUPEE_RED), "false -> true"),
            fire
        ));
    }

    #[test]
    fn a_get_message_names_its_item() {
        needs_names!();
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
        let wanted = [format!("IsGet.{}", item::HOVER_BOOT)];
        let lost = [change(
            &format!("IsGet.{}", actor::OBJ_HEART_CONTAINER),
            "true -> false",
        )];
        assert_eq!(got_other(&lost, &wanted, false), None);
        let got = [change("IsGet.Item_A", "false -> true")];
        let item_a = || Other::Flag("Item_A".into());
        assert_eq!(got_other(&got, &wanted, false), Some(item_a()));
        assert_eq!(item_a().to_string(), "IsGet.Item_A", "as reported");
        let ours = [change(
            &format!("IsGet.{}", item::HOVER_BOOT),
            "false -> true",
        )];
        assert_eq!(got_other(&ours, &wanted, false), None);
        assert_eq!(got_other(&got, &wanted, true), None, "no window, a pickup");
    }

    #[test]
    fn beside_a_whole_holder_a_flag_counts_only_with_a_window() {
        needs_names!();
        let wanted = [format!("IsGet.{}", item::HOVER_BOOT)];
        let window = [
            change("IsGet.Item_A", "false -> true"),
            change(MESSAGE, r#""" -> "Item_A""#),
        ];
        assert_eq!(
            got_other(&window, &wanted, true),
            Some(Other::Flag("Item_A".into()))
        );
    }

    #[test]
    fn another_get_is_told_to_be_an_items_by_its_names() {
        needs_names!();
        let names = twins(actor::T_BOX_RUPEE_RED);
        let flag = Other::Flag(actor::OBJ_RUPEE_RED.to_string());
        assert!(lands_as(&flag, &names));
        let window = Other::Window(actor::T_BOX_RUPEE_RED.to_string());
        assert!(lands_as(&window, &names));
        assert!(!lands_as(&Other::Flag("Item_A".into()), &names));
    }

    #[test]
    fn a_rupee_off_the_floor_is_not_another_get() {
        needs_names!();
        let wanted = [format!("IsGet.{}", item::NUT_BAG_MEDIUM)];
        let floor = [
            change(
                &format!("IsGet.{}", actor::OBJ_RUPEE_GREEN),
                "false -> true",
            ),
            change(MESSAGE, &format!(r#""" -> "{}""#, actor::OBJ_RUPEE_GREEN)),
        ];
        assert_eq!(got_other(&floor, &wanted, false), None);
        let chest = [
            change(
                &format!("IsGet.{}", actor::OBJ_RUPEE_GREEN),
                "false -> true",
            ),
            change(MESSAGE, &format!(r#""" -> "{}""#, actor::T_BOX_RUPEE_GREEN)),
        ];
        let got = got_other(&chest, &wanted, false);
        assert_eq!(
            got,
            Some(Other::Window(actor::T_BOX_RUPEE_GREEN.to_string()))
        );
        assert_eq!(
            got.map(|o| o.to_string()).as_deref(),
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
