//! A slot dealt a line's level or ammo shows what Link would get
//! ([`crate::shows`]). Each step gets a placement of its own on the slot's
//! spot, with its own stand-in, standing while that step is the one the
//! slot gives. The gates are lazy, so a shelf swaps once a purchase's get
//! window closes.

use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::Byml;

use super::signs::{BLANK, COUNTED, dst, links, relink, sign};
use super::{Slot, dealt, sold};
use crate::checks::{Drop, hash_of};
use crate::gamedata::{GameData, Op};
use crate::settings::NewGame;
use crate::shows::{and, by_name, copy_hash, copy_in, gate, not, or, steps, within};
use crate::standins::Tables;

/// One stand-in on a check slot's shelf.
pub struct Stand<'a> {
    pub slot: &'a Slot,
    pub actor: String,
    /// What it shows and sells.
    pub item: &'a str,
    pub placements: Vec<u64>,
    /// The sign under each placement that has one.
    pub signs: Vec<u64>,
    /// The sign naming the count this sale gives. Nothing to name, and
    /// [`Stand::signs`] stands gated on [`BLANK`].
    pub sign: Option<String>,
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
    let counted = dealt.filter_map(|(slot, item)| {
        Some((
            slot,
            steps(item, crate::progressive::Lines::of(new_game))?.len(),
        ))
    });
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
    /// The Bools a sale sets, one per step, and the blank sign's.
    pub bools: Vec<String>,
    /// Each step's placement past a slot's own, by the placement it copies.
    copies: Vec<(u64, u64)>,
    /// Each step's placement's BoolExp, by its hash, and its sign's.
    gates: HashMap<u64, String>,
    /// Each step's sign past a slot's own, by the sign it copies.
    sign_copies: Vec<(u64, u64)>,
    /// Each step's placement past a slot's own: the sign it copied, and
    /// its own copy of it.
    relinks: HashMap<u64, (u64, u64)>,
}

impl<'a> Shelves<'a> {
    pub fn new(
        slots: &'a [Slot],
        drops: &'a HashMap<u64, Drop>,
        new_game: &NewGame,
        gd: &GameData,
        tables: &Tables,
    ) -> Result<Self> {
        let mut shelves = Self {
            sold: Vec::new(),
            stands: Vec::new(),
            exps: Vec::new(),
            bools: Vec::new(),
            copies: Vec::new(),
            gates: HashMap::new(),
            sign_copies: Vec::new(),
            relinks: HashMap::new(),
        };
        let mut blank = false;
        for (slot, item) in dealt(slots, new_game.shop_slots, drops) {
            shelves.sold.push(slot);
            let Some(steps) = steps(item, crate::progressive::Lines::of(new_game)) else {
                let (tag, signs) = (
                    sign(tables, item),
                    (slot.placements.iter())
                        .filter_map(|p| slot.signs.get(p).copied())
                        .collect::<Vec<_>>(),
                );
                if tag.is_none() {
                    blank |= !signs.is_empty();
                    for &named in &signs {
                        shelves.gates.insert(named, BLANK.to_string());
                    }
                }
                shelves.stands.push(Stand {
                    slot,
                    actor: slot.actor(),
                    item,
                    placements: slot.placements.clone(),
                    signs,
                    sign: tag,
                });
                continue;
            };
            let slot_sold = by_name(&sold::flag(slot), Op::Bool);
            for (k, (shows, program)) in steps.into_iter().enumerate() {
                let (step, tag) = (Step::of(slot, k), sign(tables, shows));
                shelves.exps.push((step.shows.clone(), program));
                // Unsold, the step Link's pouch points at. Sold, the one
                // that sold.
                let program = or(
                    and(by_name(&step.shows, Op::Exp), not(slot_sold.clone())),
                    by_name(&step.sold, Op::Bool),
                );
                shelves.bools.push(step.sold);
                let (mut placements, mut signs) = (Vec::new(), Vec::new());
                for &of in &slot.placements {
                    let hash = copy_hash(of, k);
                    // The slot's own gate still holds: the Kokiri Shop's
                    // goods change once the Deku Tree is done.
                    let program = within(slot.presence.get(&of), program.clone(), gd)?;
                    let name = format!("ZootrShelf.{hash}");
                    shelves.exps.push((name.clone(), program));
                    if let Some(&named) = slot.signs.get(&of) {
                        let copy = copy_hash(named, k);
                        // A step with no count to show keeps its sign hidden.
                        blank |= tag.is_none();
                        let gate = match &tag {
                            Some(_) => name.clone(),
                            None => BLANK.to_string(),
                        };
                        shelves.gates.insert(copy, gate);
                        if k > 0 {
                            shelves.sign_copies.push((named, copy));
                            shelves.relinks.insert(hash, (named, copy));
                        }
                        signs.push(copy);
                    }
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
                    signs,
                    sign: tag,
                });
            }
        }
        if blank {
            shelves.bools.push(BLANK.to_string());
        }
        Ok(shelves)
    }

    /// Copy in each step's placement past a slot's own, link each copy
    /// from whoever links the slot (its keeper sells it), give it a copy of
    /// the slot's sign, then gate every step's, and its sign, on its
    /// BoolExp. Whether `root`, a map's doc, changed.
    pub fn place(&self, root: &mut Byml) -> Result<bool> {
        let copied = copy_in(root, &self.copies)?;
        let signed = copy_in(root, &self.sign_copies)?;
        let Some(list) = root.as_mut_map()?.get_mut("Actors") else {
            return Ok(false);
        };
        let mut linked = false;
        for actor in list.as_mut_array()?.iter_mut() {
            let mut more = Vec::new();
            if let Some(&(from, to)) = hash_of(actor).and_then(|h| self.relinks.get(&h)) {
                linked |= relink(actor, from, to)?;
            }
            for link in links(actor) {
                let dst = dst(link);
                for &(_, hash) in self.copies.iter().filter(|(of, _)| Some(*of) == dst) {
                    let mut copy = link.clone();
                    copy.as_mut_map()?.insert("Dst".into(), Byml::U64(hash));
                    more.push(copy);
                }
            }
            if !more.is_empty() {
                let links = actor.as_mut_map()?.get_mut("Links");
                links.expect("linked").as_mut_array()?.extend(more);
                linked = true;
            }
        }
        let gated = gate(root, &self.gates)?;
        Ok(copied || signed || linked || gated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{self, hash_of};
    use crate::known::{actor, item};

    #[test]
    fn a_shelf_places_a_gated_copy_per_step_keeping_the_slots_own_gate() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let tables = Tables::load(&romfs).unwrap();
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
        let shelves = Shelves::new(&slots, &drops, &new_game, &gd, &tables).unwrap();
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
                .flat_map(|a| links(a).iter().map(move |l| (a, l)))
                .filter(|(_, l)| super::dst(l) == Some(dst))
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
    fn a_sale_of_one_gates_every_step_s_sign_off() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let tables = Tables::load(&romfs).unwrap();
        let slots = super::super::slots(&romfs).unwrap();
        let slot = slots.iter().find(|s| !s.signs.is_empty()).unwrap();
        let (&of, &sign) = slot.signs.iter().next().unwrap();
        let drops = HashMap::from([(slot.hash(), Drop::from(item::HOOKSHOT.to_string()))]);
        let new_game = NewGame {
            shop_slots: 8,
            progressive: true,
            ..NewGame::default()
        };
        let shelves = Shelves::new(&slots, &drops, &new_game, &gd, &tables).unwrap();
        let (copy, sign_copy) = (copy_hash(of, 1), copy_hash(sign, 1));
        let mine = |s: &&Stand| std::ptr::eq(s.slot, slot);
        let signs: Vec<_> = shelves
            .stands
            .iter()
            .filter(mine)
            .map(|s| &s.signs)
            .collect();
        assert!(signs[0].contains(&sign) && signs[1].contains(&sign_copy));

        let (_, banc) = (crate::checks::bancs(&romfs).unwrap())
            .map(|(f, d)| (f, d.unwrap()))
            .find(|(_, d)| {
                let actors = crate::checks::actors(&d.root).unwrap();
                actors.iter().any(|a| hash_of(a) == Some(of))
            })
            .unwrap();
        let mut root = banc.root;
        assert!(shelves.place(&mut root).unwrap());
        let actors = crate::checks::actors(&root).unwrap();
        let placed = |h: u64| actors.iter().find(|a| hash_of(a) == Some(h)).unwrap();
        for (goods, sign) in [(of, sign), (copy, sign_copy)] {
            assert_eq!(super::super::signs::sign_of(placed(goods)), Some(sign));
            let gate = crate::world::presence_of(placed(sign)).unwrap();
            assert_eq!(gate.flag, BLANK, "{goods}");
            assert!(!gate.negated);
        }
        let blanked: Vec<_> = (shelves.stands.iter())
            .filter(mine)
            .map(|s| (s.item, &s.sign))
            .filter(|(_, sign)| sign.is_some())
            .collect();
        assert!(blanked.is_empty(), "{blanked:?}");
        assert!(shelves.bools.contains(&BLANK.to_string()));
    }

    #[test]
    fn a_counted_sale_gates_its_sign_like_its_goods() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let tables = Tables::load(&romfs).unwrap();
        let slots = super::super::slots(&romfs).unwrap();
        let slot = slots.iter().find(|s| !s.signs.is_empty()).unwrap();
        let &sign = slot.signs.values().next().unwrap();
        let drops = HashMap::from([(slot.hash(), Drop::from(actor::T_BOX_RUPEE_RED.to_string()))]);
        let new_game = NewGame {
            shop_slots: 8,
            ..NewGame::default()
        };
        let shelves = Shelves::new(&slots, &drops, &new_game, &gd, &tables).unwrap();
        let mine = (shelves.stands.iter()).find(|s| std::ptr::eq(s.slot, slot)).unwrap();
        assert_eq!(mine.sign, Some(format!("{COUNTED}20")));
        assert!(!mine.signs.is_empty());

        let (_, banc) = (crate::checks::bancs(&romfs).unwrap())
            .map(|(f, d)| (f, d.unwrap()))
            .find(|(_, d)| {
                let actors = crate::checks::actors(&d.root).unwrap();
                actors.iter().any(|a| hash_of(a) == Some(sign))
            })
            .unwrap();
        let mut root = banc.root;
        shelves.place(&mut root).unwrap();
        let placed = (crate::checks::actors(&root).unwrap())
            .iter()
            .find(|a| hash_of(a) == Some(sign))
            .unwrap();
        let gate = crate::world::presence_of(placed);
        assert!(!gate.as_ref().is_some_and(|g| g.flag == BLANK), "{gate:?}");
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
