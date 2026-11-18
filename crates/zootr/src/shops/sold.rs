//! A check slot sets a seed Bool when it sells, which the tracker ticks by.
//! The shop flow is one for every keeper, and nothing it keeps says which
//! slot sold. While Link shops, the keeper's own Bool is set (its flow
//! passes it to the shop's as `GameDataName`), and GameData holds the
//! goods-list entry of the slot he picked. So each sale first asks which
//! keeper is talking and which entry sold, and sets that slot's Bool.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Flowchart, Romfs, Value};

use super::Slot;
use crate::flows::{self, GAME_DATA, IS_BOOL, Model, Rewrites, SET_BOOL, VALUE, param, string};
use crate::known::{Name, actor, flag, flow};
use crate::picked;

/// The shop flow's event counting a sale.
const SALE: (&Name, &Name) = (
    actor::NPC_EVENT_STARTER,
    flow::EVENT_TRIGGER_ADD_SHOP_ITEM_SOLD_NUM,
);
/// The query comparing a GameData Int with its `Value`. Case 1 is equal.
const IS_INT: &Name = flow::EVENT_QUERY_IS_GAME_DATA_INT;
const EQUAL: usize = 1;

/// The Bool a check slot sets when it sells.
pub fn flag(slot: &Slot) -> String {
    picked::flag(slot.hash())
}

/// The first `count` slots of each shop: the check slots.
fn checks(slots: &[Slot], count: usize) -> impl Iterator<Item = &Slot> {
    slots.iter().filter(move |s| s.number <= count)
}

pub fn flags(slots: &[Slot], count: usize) -> Vec<String> {
    checks(slots, count).map(flag).collect()
}

/// The Bool set while Link shops with one of `keepers`.
fn talking(romfs: &Romfs, keepers: &[String]) -> Result<String> {
    let mut found = BTreeSet::new();
    // Not every actor near a shelf has a flow of its own.
    let own = keepers
        .iter()
        .filter(|k| romfs.root().join(flows::rel(k)).is_file());
    for keeper in own {
        let Some(flow) = flows::load(romfs, keeper)? else {
            continue;
        };
        for e in &flow.events {
            if let EventKind::SubFlow {
                flowchart,
                entry_point,
                params,
                ..
            } = &e.kind
                && flowchart == flow::DISPLAY_SHOP.get()
                && entry_point == flow::ROOT_BUY.get()
            {
                found.extend(param(params, GAME_DATA).map(str::to_owned));
            }
        }
    }
    ensure!(
        found.len() == 1,
        "keepers {keepers:?} shop by {found:?}, not one Bool"
    );
    Ok(found.pop_first().unwrap())
}

/// Make every sale in the shop flow set its check slot's Bool first.
pub fn write(romfs: &Romfs, rewrites: &mut Rewrites, slots: &[Slot], count: usize) -> Result<()> {
    let mut shops: Vec<(String, Vec<&Slot>)> = Vec::new();
    for slot in checks(slots, count) {
        match shops.iter_mut().find(|(_, s)| s[0].shop == slot.shop) {
            Some((_, s)) => s.push(slot),
            None => shops.push((talking(romfs, &slot.keepers)?, vec![slot])),
        }
    }
    flows::edit(romfs, rewrites, flow::DISPLAY_SHOP, |flow| {
        let sales = flows::running(flow, SALE);
        ensure!(!sales.is_empty(), "the shop flow counts no sale");
        let is_bool = flows::model(
            flow,
            IS_BOOL,
            flows::switch(flow, IS_BOOL).context("no Bool query")?,
        )?;
        let set = flows::model(
            flow,
            SET_BOOL,
            *flows::running(flow, SET_BOOL)
                .first()
                .context("no Bool setter")?,
        )?;
        for sale in sales {
            let links = flow.links_to(sale);
            let head = ask(flow, &is_bool, &set, &shops, sale)?;
            links
                .into_iter()
                .try_for_each(|l| flow.link(l, Some(head)))?;
        }
        Ok(())
    })
}

/// Push the questions run before `sale`: which keeper is talking, then
/// which of its slots sold. Each answer ends at `sale`. Returns the first.
fn ask(
    flow: &mut Flowchart,
    is_bool: &Model,
    set: &Model,
    shops: &[(String, Vec<&Slot>)],
    sale: u16,
) -> Result<u16> {
    let mut head = sale;
    for (talking, slots) in shops.iter().rev() {
        let mut which = sale;
        for slot in slots.iter().rev() {
            let mut params = set.params.clone();
            flows::set(&mut params, GAME_DATA, string(&flag(slot)))?;
            flows::set(&mut params, VALUE, Value::Bool(vec![true]))?;
            let (actor, action) = flow.action(&set.actor, set.name);
            let sold = flow.push(EventKind::Action {
                next: Some(sale),
                actor,
                action,
                params,
            });
            let mut params = is_bool.params.clone();
            flows::set(&mut params, GAME_DATA, string(flag::SHOP_SELECT_ITEM_INDEX))?;
            params.push((VALUE.into(), Value::Int(vec![slot.goods_index])));
            let (actor, query) = flow.query(&is_bool.actor, IS_INT);
            let mut cases = vec![(0, which), (1, which), (2, which)];
            cases[EQUAL].1 = sold;
            which = flow.push(EventKind::Switch {
                actor,
                query,
                params,
                cases,
            });
        }
        let mut params = is_bool.params.clone();
        flows::set(&mut params, GAME_DATA, string(talking))?;
        let (actor, query) = flow.query(&is_bool.actor, is_bool.name);
        // Case 0 is set.
        head = flow.push(EventKind::Switch {
            actor,
            query,
            params,
            cases: vec![(0, which), (1, head)],
        });
    }
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::shops::slots;
    use ounce_formats::Link;

    #[test]
    fn each_shop_is_known_by_one_keeper_bool() {
        let Some(romfs) = dev_dump() else { return };
        let slots = slots(&romfs).unwrap();
        let mut seen = BTreeSet::new();
        for shop in super::super::SHOPS {
            let slot = slots.iter().find(|s| s.shop == shop).unwrap();
            assert!(
                seen.insert(talking(&romfs, &slot.keepers).unwrap()),
                "{shop}"
            );
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn a_sale_sets_its_slot_bool_first() {
        let Some(romfs) = dev_dump() else { return };
        let slots = slots(&romfs).unwrap();
        let mut rewrites = Rewrites::new();
        write(&romfs, &mut rewrites, &slots, 4).unwrap();
        let flow = flows::current(&romfs, &rewrites, flow::DISPLAY_SHOP).unwrap();
        let text = flows::render(&flow);
        assert_eq!(flags(&slots, 4).len(), 32);
        for slot in slots.iter().filter(|s| s.number <= 4) {
            assert!(text.contains(&flag(slot)), "{} {}", slot.shop, slot.number);
        }
        assert!(
            !slots
                .iter()
                .filter(|s| s.number > 4)
                .any(|s| text.contains(&flag(s)))
        );
        // Every sale is reached only through the questions now.
        let vanilla = flows::load(&romfs, flow::DISPLAY_SHOP).unwrap().unwrap();
        for sale in flows::running(&flow, SALE) {
            for link in flow.links_to(sale) {
                let (Link::Next(e) | Link::Branch(e, _)) = link else {
                    panic!("{link:?}");
                };
                assert!(e as usize >= vanilla.events.len(), "{link:?}");
            }
        }
    }
}
