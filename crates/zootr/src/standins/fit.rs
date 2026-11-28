//! How a stand-in stands where it replaces something: on a shop's shelf,
//! tipped to fit under the one above, or where a placed actor stood.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlDoc};

use std::f32::consts::FRAC_PI_2;

use super::{ANIMATIONS, ARCHIVE, GOODS, SPIN, TOKEN_POSE, Tables, model, own_pose, row_id};
use crate::heights::{self, Hull, Role, Turn, UPRIGHT};
use crate::known::actor;

/// How a shelf item stands at a slot: the pose it plays, whether it
/// spins, how far it rises and the turn its placement takes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fit {
    pub pose: String,
    pub spin: bool,
    pub rise: f32,
    pub rotate: Option<[f32; 3]>,
}

/// How `item` stands in place of the shelf `goods`, at a slot turned by
/// `turn` with `gap` up to the shelf above. It plays its own shop pose if
/// a vanilla shop sells it in one, else the token's, and a model lying
/// flat there stands up in the Deku Shield's. When standing, its spin's
/// sweep passes the gap ([`heights::fits`]), it holds still instead,
/// tipped across the slot facing the customer till it fits
/// ([`diagonal`]): a spin about a tipped axis tumbles (the maintainer,
/// 2026-11-28). Its lowest point then sits on the slot's seat.
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
    let upright = standing(tables, goods, item, vanilla.clone())?;
    let standing = hull(&upright)?;
    let Some(gap) = gap.filter(|gap| !heights::fits(&standing, &place, *gap)) else {
        return Ok(Fit {
            pose: upright,
            spin: true,
            rise: anchor - standing.lo(&place),
            rotate: None,
        });
    };
    // A flat model stands up on its own long side, its face to the
    // customer, rather than in the stand pose's lean back.
    let (pose, base) = match upright == vanilla {
        true => (upright, UPRIGHT),
        false => (vanilla, heights::euler([-FRAC_PI_2, 0.0, 0.0])),
    };
    let shown = hull(&pose)?;
    let tipped = diagonal(&shown, &place, &base, gap);
    Ok(Fit {
        pose,
        spin: false,
        rise: anchor - shown.lo(&tipped),
        rotate: (tipped != place).then(|| heights::angles(&tipped)),
    })
}

/// `place` after `base`, tipped about the slot's facing axis (its +Z,
/// toward the customer) by the least whole degree that brings `item`,
/// held still, under `gap`. Lying along the shelf when nothing less does.
fn diagonal(item: &Hull, place: &Turn, base: &Turn, gap: f32) -> Turn {
    let tip = |deg: u16| {
        let roll = heights::euler([0.0, 0.0, f32::from(deg).to_radians()]);
        heights::times(place, &heights::times(&roll, base))
    };
    let height = |turn: &Turn| {
        let (lo, hi) = item.span(turn);
        hi - lo
    };
    (0..=90)
        .map(tip)
        .find(|turn| height(turn) <= gap)
        .unwrap_or_else(|| tip(90))
}

/// `pose`, or the Deku Shield's when `pose` is the token's and `item`
/// lies flat in it on the shelf of `goods`: the pose that stands it up.
fn standing(tables: &Tables, goods: &ActorPack, item: &ActorPack, pose: String) -> Result<String> {
    let shown = heights::shown(goods, item, Role::Shelf, Some(&pose))?;
    let flat = pose == **TOKEN_POSE && shown.hull(&tables.actor, &item.name)?.flat();
    Ok(if flat { stand_pose() } else { pose })
}

/// The pose a stand-in off the shelf plays beside the spin, built on
/// `template`: the first other archive `item` plays there, else the
/// token's. It lies as that pose leaves it, spinning about up: a tipped
/// spin would tumble (the maintainer, 2026-11-28).
pub(crate) fn rest_pose(template: &ActorPack, item: &ActorPack) -> Result<String> {
    let played = heights::shown(template, item, Role::Rest, None)?.archives;
    Ok((played.into_iter())
        .find(|a| *a != **SPIN)
        .unwrap_or_else(|| TOKEN_POSE.to_string()))
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

/// How far `item`, built on `template` and playing `pose` beside the spin
/// (its own archives when `None`), rises where `goods` stood, so its
/// lowest point meets theirs. A placement's turn here is a yaw, which
/// moves neither.
pub(crate) fn placed_rise(
    tables: &Tables,
    goods: &ActorPack,
    template: &ActorPack,
    item: &ActorPack,
    pose: Option<&str>,
) -> Result<f32> {
    let at_rest = |template: &ActorPack, item: &ActorPack, pose| -> Result<Hull> {
        heights::shown(template, item, Role::Rest, pose)?.hull(&tables.actor, &item.name)
    };
    Ok(heights::rise(
        &at_rest(goods, goods, None)?,
        &at_rest(template, item, pose)?,
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
    fn a_tall_item_tips_across_its_slot_by_the_least_that_fits() {
        let post = Hull::posed(
            heights::Kind::Mesh,
            &[[0.0; 3], [0.0, 1.0, 0.0]],
            &[heights::IDENTITY],
        );
        let height = |turn: &Turn| {
            let (lo, hi) = post.span(turn);
            hi - lo
        };
        // Short enough, it stays upright.
        assert_eq!(diagonal(&post, &UPRIGHT, &UPRIGHT, 1.0), UPRIGHT);
        // A 1 m post under 0.8 m tips about the facing axis alone, to
        // about 37°, a degree past which would already fit.
        let tipped = diagonal(&post, &UPRIGHT, &UPRIGHT, 0.8);
        assert!(height(&tipped) <= 0.8 && height(&tipped) > 0.78);
        assert_eq!(tipped[2], [0.0, 0.0, 1.0]);
        // Under nothing it lies along the shelf.
        assert!(height(&diagonal(&post, &UPRIGHT, &UPRIGHT, 0.0)) < 1e-6);
    }

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
