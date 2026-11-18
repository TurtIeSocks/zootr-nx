//! A slot dealt a line's level or ammo shows what Link would get. Each step
//! gets a placement of its own on the slot's spot, with its own stand-in,
//! standing while a BoolExp over the pouch's `IsGet` flags says that step is
//! the one the slot gives: the level after the highest Link owns, the top
//! once he owns the one below it, and ammo's red rupee until he owns what
//! fires it, as the chains and the hook give. The gates are lazy, so a
//! shelf swaps once a purchase's get window closes.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::{Byml, murmur3};

use super::{Slot, dealt, sold};
use crate::checks::{self, Drop, hash_of};
use crate::gamedata::{GameData, IS_GET, Op};
use crate::settings::NewGame;
use crate::{ammo, progressive};

/// What a holder of `item` gives as Link's pouch grows, lowest first, each
/// with the program that's true while it's the one. `None` when it always
/// gives `item`.
pub(crate) fn steps(item: &str, progressive: bool) -> Option<Vec<(&'static str, Vec<Op>)>> {
    if let Some(&(bundle, fires)) = ammo::NEEDS.iter().find(|(a, _)| *a == item) {
        let owns = any(pouches(fires).map(owned));
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

/// The pouch names owning any of which means owning `fires`.
fn pouches(fires: &'static str) -> impl Iterator<Item = &'static str> {
    let line = progressive::line(fires).unwrap_or_default();
    let named = line.iter().map(|(_, pouch)| pouch.get());
    named.chain(line.is_empty().then_some(fires))
}

fn owned(pouch: &str) -> Vec<Op> {
    by_name(&format!("{IS_GET}.{pouch}"), Op::Bool)
}

/// Push the entry `name`, a Bool or a BoolExp as `op` says.
fn by_name(name: &str, op: fn(u32) -> Op) -> Vec<Op> {
    vec![op(murmur3(name.as_bytes()))]
}

fn not(mut a: Vec<Op>) -> Vec<Op> {
    a.push(Op::Not);
    a
}

fn join(mut a: Vec<Op>, b: Vec<Op>, op: Op) -> Vec<Op> {
    a.extend(b);
    a.push(op);
    a
}

fn and(a: Vec<Op>, b: Vec<Op>) -> Vec<Op> {
    join(a, b, Op::And)
}

fn or(a: Vec<Op>, b: Vec<Op>) -> Vec<Op> {
    join(a, b, Op::Or)
}

fn all(terms: impl Iterator<Item = Vec<Op>>) -> Vec<Op> {
    terms.reduce(and).unwrap_or_default()
}

fn any(terms: impl Iterator<Item = Vec<Op>>) -> Vec<Op> {
    terms.reduce(or).unwrap_or_default()
}

/// The placement of step `k` copied from slot placement `of`. Step 0 is the
/// placement itself.
pub(super) fn copy_hash(of: u64, k: usize) -> u64 {
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

/// One stand-in on a check slot's shelf.
pub struct Stand<'a> {
    pub slot: &'a Slot,
    pub actor: String,
    /// What it shows and sells.
    pub item: &'a str,
    pub placements: Vec<u64>,
}

/// One step of a slot: the BoolExp true while it's the one the slot gives,
/// and the Bool a sale sets while it is, so a sold slot keeps naming it.
pub struct Step {
    pub shows: String,
    pub sold: String,
}

impl Step {
    fn of(slot: &Slot, k: usize) -> Self {
        Self {
            shows: format!("ZootrStep.{}.{k}", slot.hash()),
            sold: format!("ZootrSold.{}.{k}", slot.hash()),
        }
    }
}

/// Each check slot dealt a line's level or ammo, with its steps lowest
/// first.
pub(super) fn stepped<'a>(
    slots: &'a [Slot],
    drops: &'a HashMap<u64, Drop>,
    new_game: &NewGame,
) -> Vec<(&'a Slot, Vec<Step>)> {
    let dealt = dealt(slots, new_game.shop_slots, drops);
    let counted =
        dealt.filter_map(|(slot, item)| Some((slot, steps(item, new_game.progressive)?.len())));
    counted
        .map(|(slot, n)| (slot, (0..n).map(|k| Step::of(slot, k)).collect()))
        .collect()
}

/// Every check slot's shelf: a stand-in per step, and what placing and
/// gating the steps takes.
pub struct Shelves<'a> {
    pub sold: Vec<&'a Slot>,
    pub stands: Vec<Stand<'a>>,
    /// Each step's BoolExps: its own, then one per placement.
    pub exps: Vec<(String, Vec<Op>)>,
    /// The Bools a sale sets, one per step.
    pub bools: Vec<String>,
    /// Each step's placement past a slot's own, by the placement it copies.
    copies: Vec<(u64, u64)>,
    /// Each step's placement's BoolExp, by its hash.
    gates: HashMap<u64, String>,
}

impl<'a> Shelves<'a> {
    pub fn new(
        slots: &'a [Slot],
        drops: &'a HashMap<u64, Drop>,
        new_game: &NewGame,
        gd: &GameData,
    ) -> Result<Self> {
        let mut shelves = Self {
            sold: Vec::new(),
            stands: Vec::new(),
            exps: Vec::new(),
            bools: Vec::new(),
            copies: Vec::new(),
            gates: HashMap::new(),
        };
        for (slot, item) in dealt(slots, new_game.shop_slots, drops) {
            shelves.sold.push(slot);
            let Some(steps) = steps(item, new_game.progressive) else {
                shelves.stands.push(Stand {
                    slot,
                    actor: slot.actor(),
                    item,
                    placements: slot.placements.clone(),
                });
                continue;
            };
            let slot_sold = by_name(&sold::flag(slot), Op::Bool);
            for (k, (shows, program)) in steps.into_iter().enumerate() {
                let step = Step::of(slot, k);
                shelves.exps.push((step.shows.clone(), program));
                // Unsold, the step Link's pouch points at. Sold, the one
                // that sold.
                let program = or(
                    and(by_name(&step.shows, Op::Exp), not(slot_sold.clone())),
                    by_name(&step.sold, Op::Bool),
                );
                shelves.bools.push(step.sold);
                let mut placements = Vec::new();
                for &of in &slot.placements {
                    let hash = copy_hash(of, k);
                    // The slot's own gate still holds: the Kokiri Shop's
                    // goods change once the Deku Tree is done.
                    let program = match slot.presence.get(&of) {
                        Some(p) => {
                            let own = vec![Op::read(gd, &p.flag)?];
                            and(if p.negated { not(own) } else { own }, program.clone())
                        }
                        None => program.clone(),
                    };
                    let name = format!("ZootrShelf.{hash}");
                    shelves.exps.push((name.clone(), program));
                    shelves.gates.insert(hash, name);
                    if k > 0 {
                        shelves.copies.push((of, hash));
                    }
                    placements.push(hash);
                }
                shelves.stands.push(Stand {
                    slot,
                    actor: match k {
                        0 => slot.actor(),
                        k => format!("{}_{k}", slot.actor()),
                    },
                    item: shows,
                    placements,
                });
            }
        }
        Ok(shelves)
    }

    /// Copy in each step's placement past a slot's own, link each copy
    /// from whoever links the slot (its keeper sells it), then gate every
    /// step's on its BoolExp. Whether `root`, a map's doc, changed.
    pub fn place(&self, root: &mut Byml) -> Result<bool> {
        let Some(list) = root.as_mut_map()?.get_mut("Actors") else {
            return Ok(false);
        };
        let list = list.as_mut_array()?;
        let mut new = Vec::new();
        for &(of, hash) in &self.copies {
            if let Some(actor) = list.iter().find(|a| hash_of(a) == Some(of)) {
                let mut copy = actor.clone();
                rehash(&mut copy, hash)?;
                new.push(copy);
            }
        }
        list.extend(new);
        let mut changed = false;
        for actor in list.iter_mut() {
            let mut more = Vec::new();
            for link in link_list(actor) {
                let dst = link_dst(link);
                for &(_, hash) in self.copies.iter().filter(|(of, _)| Some(*of) == dst) {
                    let mut copy = link.clone();
                    copy.as_mut_map()?.insert("Dst".into(), Byml::U64(hash));
                    more.push(copy);
                }
            }
            if !more.is_empty() {
                let links = actor.as_mut_map()?.get_mut("Links");
                links.expect("linked").as_mut_array()?.extend(more);
                changed = true;
            }
        }
        for actor in list.iter_mut() {
            if let Some(gate) = hash_of(actor).and_then(|h| self.gates.get(&h)) {
                actor.as_mut_map()?.remove("Presence");
                checks::gate(actor, gate, false)?;
                changed = true;
            }
        }
        Ok(changed)
    }
}

/// A placement's links to others.
fn link_list(actor: &Byml) -> &[Byml] {
    let links = actor.as_map().ok().and_then(|m| m.get("Links"));
    links.and_then(|l| l.as_array().ok()).unwrap_or_default()
}

/// The placement a link points at.
fn link_dst(link: &Byml) -> Option<u64> {
    link.as_map().ok()?.get("Dst")?.as_int::<u64>().ok()
}

/// Give a copied placement its own hash, in each field that holds it.
fn rehash(actor: &mut Byml, hash: u64) -> Result<()> {
    let map = actor.as_mut_map()?;
    map.insert("Hash".into(), Byml::U64(hash));
    let phive = map.get_mut("Phive").and_then(|p| p.as_mut_map().ok());
    let placement = phive.and_then(|p| p.get_mut("Placement")?.as_mut_map().ok());
    if let Some(p) = placement {
        p.insert("ID".into(), Byml::U64(hash));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let fired = ammo::NEEDS.iter().flat_map(|(_, f)| pouches(f));
        for pouch in lines.map(|(_, p)| p.get()).chain(fired) {
            let name = format!("{IS_GET}.{pouch}");
            assert_eq!(Op::read(&gd, &name).ok(), owned(pouch).pop(), "{name}");
        }
    }

    #[test]
    fn a_shelf_places_a_gated_copy_per_step_keeping_the_slots_own_gate() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let slots = super::super::slots(&romfs).unwrap();
        // A Kokiri Shop slot: its goods stand only before or after the
        // Deku Tree.
        let slot = (slots.iter())
            .find(|s| s.number == 1 && !s.presence.is_empty())
            .unwrap();
        let (of, own) = slot.presence.iter().next().unwrap();
        let drops = HashMap::from([(slot.hash(), Drop::from(item::HOOKSHOT.to_string()))]);
        let new_game = NewGame {
            shop_slots: 1,
            progressive: true,
            ..NewGame::default()
        };
        let shelves = Shelves::new(&slots, &drops, &new_game, &gd).unwrap();
        let mine: Vec<_> = (shelves.stands.iter())
            .filter(|s| std::ptr::eq(s.slot, slot))
            .map(|s| (s.actor.as_str(), s.item))
            .collect();
        let actor = slot.actor();
        assert_eq!(
            mine,
            [
                (actor.as_str(), item::HOOKSHOT.get()),
                (format!("{actor}_1").as_str(), item::LONGSHOT.get())
            ]
        );
        let copy = copy_hash(*of, 1);

        // The slot's own gate, then the step's.
        let mut doc =
            ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        crate::gamedata::add_exps(&mut doc.root, &shelves.exps).unwrap();
        let mut ours = GameData::parse(&doc.to_binary()).unwrap();
        ours.learn(&own.flag);
        let bang = if own.negated { "!" } else { "" };
        let exp = ours.expr(&format!("ZootrShelf.{copy}")).unwrap().unwrap();
        assert!(exp.starts_with(&format!("{bang}{} && ", own.flag)), "{exp}");

        let (_, banc) = (crate::checks::bancs(&romfs).unwrap())
            .map(|(f, d)| (f, d.unwrap()))
            .find(|(_, d)| {
                let actors = crate::checks::actors(&d.root).unwrap();
                actors.iter().any(|a| hash_of(a) == Some(*of))
            })
            .unwrap();
        let mut root = banc.root;
        assert!(shelves.place(&mut root).unwrap());
        let actors = crate::checks::actors(&root).unwrap();
        let placed = |h: u64| {
            let mut at = actors.iter().filter(|a| hash_of(a) == Some(h));
            let a = at.next().unwrap_or_else(|| panic!("{h} not placed"));
            assert!(at.next().is_none(), "{h} placed twice");
            a
        };
        for (hash, k) in [(*of, 0), (copy, 1)] {
            let a = placed(hash);
            let gate = crate::world::presence_of(a).unwrap();
            assert_eq!(gate.flag, format!("ZootrShelf.{hash}"), "step {k}");
            assert!(!gate.negated);
            let id = &a.as_map().unwrap()["Phive"].as_map().unwrap()["Placement"]
                .as_map()
                .unwrap()["ID"];
            assert_eq!(id.as_int::<u64>().unwrap(), hash);
        }
        // The keeper sells and references the copy as it does the slot, or
        // its menu can't price, name or reach it.
        let to = |dst: u64| {
            let mut by: Vec<(u64, String)> = (actors.iter())
                .flat_map(|a| link_list(a).iter().map(move |l| (a, l)))
                .filter(|(_, l)| link_dst(l) == Some(dst))
                .map(|(a, l)| {
                    (
                        hash_of(a).unwrap(),
                        checks::str_field(l, "Name").unwrap().into(),
                    )
                })
                .collect();
            by.sort();
            by
        };
        assert!(to(*of).iter().any(|(_, name)| name == "ForSale"));
        assert_eq!(to(copy), to(*of));
    }

    #[test]
    fn a_copy_never_takes_a_placed_hash() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let slots = super::super::slots(&romfs).unwrap();
        let mut placed = std::collections::HashSet::new();
        for (_, doc) in crate::checks::bancs(&romfs).unwrap() {
            let doc = doc.unwrap();
            let actors = crate::checks::actors(&doc.root).unwrap();
            placed.extend(actors.iter().filter_map(hash_of));
        }
        for of in slots.iter().flat_map(|s| s.placements.iter()) {
            for k in 1..4 {
                assert!(!placed.contains(&copy_hash(*of, k)), "{of} step {k}");
            }
        }
    }
}
