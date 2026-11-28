//! How a stand-in stands where it replaces something: on a shop's shelf,
//! leaning to fit under the one above, or where a placed actor stood.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlDoc};

use super::{ANIMATIONS, ARCHIVE, GOODS, TOKEN_POSE, Tables, model, own_pose, row_id};
use crate::heights::{self, Hull, Role, UPRIGHT};
use crate::known::actor;

/// How a shelf item stands at a slot: the pose it plays, how far it rises
/// and the turn its placement takes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fit {
    pub pose: String,
    pub rise: f32,
    pub rotate: Option<[f32; 3]>,
}

/// How `item` stands in place of the shelf `goods`, at a slot turned by
/// `turn` with `gap` up to the shelf above. It plays its own shop pose if
/// a vanilla shop sells it in one, else the token's, and a model lying
/// flat there stands up in the Deku Shield's. While its spin's sweep
/// passes the gap it leans ([`heights::lean`]), and with no lean that
/// fits it lies flat again. Its lowest point then meets the goods'.
pub(crate) fn fit(
    tables: &Tables,
    goods: &ActorPack,
    item: &ActorPack,
    turn: [f32; 3],
    gap: Option<f32>,
    plank: Option<f32>,
) -> Result<Fit> {
    let place = heights::euler(turn);
    let anchor = seat(tables, goods, turn, plank)?;
    let hull = |pose: &str| -> Result<Hull> {
        heights::shown(goods, item, Role::Shelf, Some(pose))?.hull(&tables.actor, &item.name)
    };
    let vanilla = vanilla_pose(&tables.actor, &item.name)?;
    let flat = vanilla == **TOKEN_POSE && hull(&vanilla)?.flat();
    let upright = if flat { stand_pose() } else { vanilla.clone() };
    let standing = hull(&upright)?;
    let lean = match gap {
        Some(gap) => heights::lean(&standing, &place, gap),
        None => Some(0.0),
    };
    let (pose, shown, lean) = match lean {
        Some(lean) => (upright, standing, lean),
        None if flat => (vanilla.clone(), hull(&vanilla)?, 0.0),
        None => (upright, standing, 0.0),
    };
    let turned = heights::leaned(&place, lean);
    Ok(Fit {
        pose,
        rise: anchor - shown.lo(&turned),
        rotate: (lean != 0.0).then(|| heights::angles(&turned)),
    })
}

/// Where a shelf item's lowest point goes, over its slot: on the plank
/// under it, else, on a shelf with no collision, where the goods' lowest
/// point was.
pub(crate) fn seat(
    tables: &Tables,
    goods: &ActorPack,
    turn: [f32; 3],
    plank: Option<f32>,
) -> Result<f32> {
    match plank {
        Some(plank) => Ok(plank),
        None => Ok(heights::shown(goods, goods, Role::Shelf, None)?
            .hull(&tables.actor, &goods.name)?
            .lo(&heights::euler(turn))),
    }
}

/// How far `item`, built on `template`, rises where `goods` stood, so its
/// lowest point meets theirs. A placement's turn here is a yaw, which
/// moves neither.
pub(crate) fn placed_rise(
    tables: &Tables,
    goods: &ActorPack,
    template: &ActorPack,
    item: &ActorPack,
) -> Result<f32> {
    let at_rest = |template: &ActorPack, item: &ActorPack| -> Result<Hull> {
        heights::shown(template, item, Role::Rest, None)?.hull(&tables.actor, &item.name)
    };
    Ok(heights::rise(
        &at_rest(goods, goods)?,
        &at_rest(template, item)?,
        &UPRIGHT,
    ))
}

/// The Deku Shield's shelf pose, which stands a flat model up: what a
/// model lying flat in the token's pose plays instead.
pub(crate) fn stand_pose() -> String {
    own_pose(actor::WEAPON_SHIELD_DEKU)
}

/// The pose `item` plays before any flat model stands up: its own if a
/// vanilla shop sells it in one, else the token's.
pub(crate) fn vanilla_pose(actor: &BymlDoc, item: &str) -> Result<String> {
    let Some(model) = model(actor, item)? else {
        return Ok(TOKEN_POSE.to_string());
    };
    let own = own_pose(&model);
    for row in actor.root.as_array()? {
        if !row_id(row).starts_with(GOODS) {
            continue;
        }
        let Some(Byml::Array(archives)) = row.as_map()?.get(ANIMATIONS.get()) else {
            continue;
        };
        let named =
            |a: &Byml| (a.as_map().ok()?.get(ARCHIVE.get())?.as_string().ok()).map(|n| *n == own);
        if archives.iter().any(|a| named(a) == Some(true)) {
            return Ok(own);
        }
    }
    Ok(TOKEN_POSE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{Name, item};
    use crate::standins::OWN_POSE;

    #[test]
    fn a_shield_keeps_its_own_shelf_pose_and_a_medallion_takes_the_tokens() {
        let Some(romfs) = dev_dump() else { return };
        let tables = Tables::load(&romfs).unwrap();
        let pose = |i: &Name| vanilla_pose(&tables.actor, i).unwrap();
        let shield = format!("{}{OWN_POSE}", actor::WEAPON_SHIELD_DEKU);
        assert_eq!(pose(actor::WEAPON_SHIELD_DEKU), shield);
        assert_eq!(pose(actor::T_BOX_SHIELD_DEKU), shield);
        assert_eq!(pose(item::MEDAL_FIRE), TOKEN_POSE.get());
    }
}
