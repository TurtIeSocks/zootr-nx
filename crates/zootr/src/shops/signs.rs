//! The sign under shelf goods that says how many a sale gives ("x10"). It's
//! a placement of its own, which the goods name by a `Reference` link. A
//! check slot's sign says the count of what the slot sells. A sale of one,
//! or of a count no board says, gets no sign, like the vanilla goods that
//! sell one: the only board left over is the boards' base actor, which has
//! the physics and the sensor set but no model of its own, so the engine
//! draws its Dummy in red. Each step of a stepped slot gets a copy of the
//! sign, gated like the step's goods, as a step may sell another count.

use ounce_formats::Byml;

use crate::known::field;
use crate::standins::{Tables, row};

/// The link from goods to their sign.
const SIGN: &str = "Reference";
/// The signs that say a count, by the count, two digits.
pub(super) const COUNTED: &str = "Shop_Board_Bundle_";
/// The Bool a sign with nothing to say waits on. Nothing sets it.
pub const BLANK: &str = "ZootrNoShopSign";

/// A placement's links to others.
pub(super) fn links(actor: &Byml) -> &[Byml] {
    let links = actor.as_map().ok().and_then(|m| m.get("Links"));
    links.and_then(|l| l.as_array().ok()).unwrap_or_default()
}

/// The placement a link points at.
pub(super) fn dst(link: &Byml) -> Option<u64> {
    link.as_map().ok()?.get("Dst")?.as_int::<u64>().ok()
}

fn is_sign(link: &Byml) -> bool {
    let name = link.as_map().ok().and_then(|m| m.get("Name"));
    name.and_then(|n| n.as_string().ok())
        .is_some_and(|n| *n == SIGN)
}

/// The sign a goods placement names, if any.
pub(super) fn sign_of(actor: &Byml) -> Option<u64> {
    links(actor).iter().find(|l| is_sign(l)).and_then(dst)
}

/// Points `actor`'s sign link at `to` where it named `from`.
pub(super) fn relink(actor: &mut Byml, from: u64, to: u64) -> anyhow::Result<bool> {
    let Some(Byml::Array(links)) = actor.as_mut_map()?.get_mut("Links") else {
        return Ok(false);
    };
    let mut moved = false;
    for link in links
        .iter_mut()
        .filter(|l| is_sign(l) && dst(l) == Some(from))
    {
        link.as_mut_map()?.insert("Dst".into(), Byml::U64(to));
        moved = true;
    }
    Ok(moved)
}

/// The sign that says how many of `item` a sale gives, or nothing when no
/// board says its count.
pub(super) fn sign(tables: &Tables, item: &str) -> Option<String> {
    let count = row(&tables.pouch, item)
        .ok()
        .and_then(|r| r.get(field::BUNDLE_ACTOR_NUM.get())?.as_int::<i64>().ok())
        .unwrap_or(1);
    let counted = format!("{COUNTED}{count:02}");
    match count > 1 && row(&tables.actor, &counted).is_ok() {
        true => Some(counted),
        false => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{actor, item};

    #[test]
    fn a_sign_says_the_count_a_sale_gives_or_nothing() {
        let Some(romfs) = dev_dump() else { return };
        let tables = Tables::load(&romfs).unwrap();
        let sign = |i: &str| sign(&tables, i);
        assert_eq!(sign(item::DEKU_STICK_BUNDLE_SMALL), Some(format!("{COUNTED}03")));
        assert_eq!(sign(actor::T_BOX_RUPEE_RED), Some(format!("{COUNTED}20")));
        // One, and a count no sign says.
        assert_eq!(sign(item::ARROW_ICE), None);
        assert_eq!(sign(actor::T_BOX_RUPEE_GOLD), None);
    }
}
