//! Items for both ages. The pause menu's table greys out the other age's
//! items by two flags per row, and opening both is each item's first gate.
//! [`toggles`] lists each item with its rows and the rest it needs: the
//! other Link's clips ([`crate::ageless_items`]), or a child's boots and
//! tunics ([`gear`]). See the spec,
//! `superpowers/specs/2026-11-24-both-ages-toggles-design.md`.

use anyhow::Result;
use ounce_formats::{Byml, BymlMap};

use crate::known::{Name, field};

pub mod files;
pub mod gear;
pub mod toggles;
pub mod tunics;

/// The flags, each age's.
const AGES: [&Name; 2] = [field::IS_USABLE_CHILD, field::IS_USABLE_ADULT];

/// Whether `actor` is usable at each age, as `AGES` lists them. A missing
/// flag is false.
fn ages(actor: &BymlMap) -> [bool; 2] {
    AGES.map(|flag| actor.get(flag.get()).and_then(|f| f.as_bool().ok()) == Some(true))
}

/// The actor's name, as the table gives it.
pub(crate) fn name(actor: &BymlMap) -> Option<&str> {
    actor
        .get(field::ACTOR_SHORT.get())?
        .as_string()
        .ok()
        .map(|s| s.as_str())
}

/// Each actor row in `root`, the pause menu's table, on every page.
pub(crate) fn rows(root: &mut Byml) -> Result<Vec<&mut BymlMap>> {
    let pages = (root.as_mut_map()?.values_mut()).filter_map(|slots| match slots {
        Byml::Array(slots) => Some(slots),
        _ => None,
    });
    let actors = pages.flatten().filter_map(|slot| {
        match slot.as_mut_map().ok()?.get_mut(field::ACTOR.get()) {
            Some(Byml::Array(actors)) => Some(actors),
            _ => None,
        }
    });
    Ok(actors
        .flatten()
        .filter_map(|a| a.as_mut_map().ok())
        .collect())
}

/// Makes `actor` usable at both ages. Returns whether it wasn't already.
pub(crate) fn open(actor: &mut BymlMap) -> bool {
    let changed = ages(actor) != [true, true];
    for flag in AGES {
        actor.insert(flag.get().into(), Byml::Bool(true));
    }
    changed
}
