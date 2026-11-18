//! What a holder of a line's level or of ammo shows as Link's pouch grows:
//! the level after the highest he owns, the top once he owns the one below
//! it, and ammo's red rupee until he owns what fires it, as the chains and
//! the hook give. A holder that can't ask the pouch itself places each step
//! on its spot, standing while that step's program holds: a shop slot
//! (`shops::steps`) or a pickup (`pickups::steps`).

use anyhow::Result;
use ounce_formats::{Byml, murmur3};

use crate::gamedata::{IS_GET, Op};
use crate::{ammo, progressive};

/// What a holder of `item` gives as Link's pouch grows, lowest first, each
/// with the program that's true while it's the one. `None` when it always
/// gives `item`.
pub(crate) fn steps(item: &str, progressive: bool) -> Option<Vec<(&'static str, Vec<Op>)>> {
    if let Some(&(bundle, fires)) = ammo::NEEDS.iter().find(|(a, _)| *a == item) {
        let owns = any(ammo::pouches(fires).map(owned));
        return Some(vec![
            (ammo::WITHOUT.get(), not(owns.clone())),
            (bundle.get(), owns),
        ]);
    }
    if !progressive {
        return None;
    }
    let line = progressive::line(item)?;
    let owns: Vec<_> = line.iter().map(|(_, pouch)| owned(pouch.get())).collect();
    let top = line.len() - 1;
    let program = |k: usize| match k {
        k if k == top => or(owns[top - 1].clone(), owns[top].clone()),
        0 => all(owns.iter().cloned().map(not)),
        k => and(owns[k - 1].clone(), all(owns[k..].iter().cloned().map(not))),
    };
    let steps = line.iter().enumerate();
    Some(
        steps
            .map(|(k, (level, _))| (level.get(), program(k)))
            .collect(),
    )
}

pub(crate) fn owned(pouch: &str) -> Vec<Op> {
    by_name(&format!("{IS_GET}.{pouch}"), Op::Bool)
}

/// Push the entry `name`, a Bool or a BoolExp as `op` says.
pub(crate) fn by_name(name: &str, op: fn(u32) -> Op) -> Vec<Op> {
    vec![op(murmur3(name.as_bytes()))]
}

pub(crate) fn not(mut a: Vec<Op>) -> Vec<Op> {
    a.push(Op::Not);
    a
}

pub(crate) fn join(mut a: Vec<Op>, b: Vec<Op>, op: Op) -> Vec<Op> {
    a.extend(b);
    a.push(op);
    a
}

pub(crate) fn and(a: Vec<Op>, b: Vec<Op>) -> Vec<Op> {
    join(a, b, Op::And)
}

pub(crate) fn or(a: Vec<Op>, b: Vec<Op>) -> Vec<Op> {
    join(a, b, Op::Or)
}

pub(crate) fn all(terms: impl Iterator<Item = Vec<Op>>) -> Vec<Op> {
    terms.reduce(and).unwrap_or_default()
}

pub(crate) fn any(terms: impl Iterator<Item = Vec<Op>>) -> Vec<Op> {
    terms.reduce(or).unwrap_or_default()
}

/// The placement of step `k` copied from placement `of`. Step 0 is the
/// placement itself.
pub(crate) fn copy_hash(of: u64, k: usize) -> u64 {
    match k {
        0 => of,
        // splitmix64, so no copy lands on another placement's hash.
        k => {
            let mut z = of ^ (k as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    }
}
/// Give a copied placement its own hash, in each field that holds it.
pub(crate) fn rehash(actor: &mut Byml, hash: u64) -> Result<()> {
    let map = actor.as_mut_map()?;
    map.insert("Hash".into(), Byml::U64(hash));
    let phive = map.get_mut("Phive").and_then(|p| p.as_mut_map().ok());
    let placement = phive.and_then(|p| p.get_mut("Placement")?.as_mut_map().ok());
    if let Some(p) = placement.filter(|p| p.contains_key("ID")) {
        p.insert("ID".into(), Byml::U64(hash));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;
    use crate::known::{actor, item};

    fn read(program: &[Op]) -> String {
        let name = |h: u32| {
            let pouches = progressive::LINES.iter().flat_map(|l| l.iter());
            (pouches.map(|(_, p)| p.get()))
                .find(|p| murmur3(format!("{IS_GET}.{p}").as_bytes()) == h)
                .unwrap_or("?")
                .to_owned()
        };
        let mut stack: Vec<String> = Vec::new();
        for &ins in program {
            let term = match ins {
                Op::Bool(h) | Op::Exp(h) => name(h),
                Op::Not => format!("!{}", stack.pop().unwrap()),
                Op::And | Op::Or => {
                    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
                    let op = if ins == Op::And { "&" } else { "|" };
                    format!("({a} {op} {b})")
                }
            };
            stack.push(term);
        }
        stack.pop().unwrap()
    }

    fn shown(item: &str, progressive: bool) -> Vec<(&'static str, String)> {
        let steps = steps(item, progressive).unwrap();
        steps.into_iter().map(|(s, p)| (s, read(&p))).collect()
    }

    #[test]
    fn a_line_shows_the_level_after_the_highest_owned() {
        // Names resolve from the dump.
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        let (hook, long) = (item::HOOKSHOT.get(), item::LONGSHOT.get());
        assert_eq!(
            shown(long, true),
            [
                (hook, format!("(!{hook} & !{long})")),
                (long, format!("({hook} | {long})")),
            ]
        );
        let [bracelet, silver, gold] = [
            actor::ARMOR_CHILD_GAUNTLET_GORON,
            actor::ARMOR_ADULT_GAUNTLET_SILVER,
            actor::ARMOR_ADULT_GAUNTLET_GOLD,
        ];
        let strength = shown(item::GAUNTLET_SILVER.get(), true);
        let shows: Vec<_> = strength.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            shows,
            [
                item::GAUNTLET_GORON,
                item::GAUNTLET_SILVER,
                item::GAUNTLET_GOLD
            ]
            .map(|n| n.get())
        );
        assert_eq!(
            strength[0].1,
            format!("((!{bracelet} & !{silver}) & !{gold})")
        );
        assert_eq!(
            strength[1].1,
            format!("({bracelet} & (!{silver} & !{gold}))")
        );
        assert_eq!(strength[2].1, format!("({silver} | {gold})"));
    }

    #[test]
    fn ammo_shows_a_red_rupee_until_link_owns_what_fires_it() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        let bow = actor::WEAPON_RANGE_BOW.get();
        let (medium, large) = (item::QUIVER_MEDIUM.get(), item::QUIVER_LARGE.get());
        let owns = format!("(({bow} | {medium}) | {large})");
        for progressive in [false, true] {
            let arrows = actor::T_BOX_ARROW_BUNDLE_SMALL.get();
            assert_eq!(
                shown(arrows, progressive),
                [
                    (ammo::WITHOUT.get(), format!("!{owns}")),
                    (arrows, owns.clone())
                ]
            );
        }
    }

    #[test]
    fn a_plain_item_or_a_level_without_progressive_items_has_no_steps() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        assert!(steps(item::LONGSHOT.get(), false).is_none());
        assert!(steps(item::HOVER_BOOT.get(), true).is_none());
    }

    #[test]
    fn every_pouch_a_step_reads_has_an_is_get_bool() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let lines = progressive::LINES.iter().flat_map(|l| l.iter());
        let fired = ammo::NEEDS.iter().flat_map(|(_, f)| ammo::pouches(f));
        for pouch in lines.map(|(_, p)| p.get()).chain(fired) {
            let name = format!("{IS_GET}.{pouch}");
            assert_eq!(Op::read(&gd, &name).ok(), owned(pouch).pop(), "{name}");
        }
    }
}
