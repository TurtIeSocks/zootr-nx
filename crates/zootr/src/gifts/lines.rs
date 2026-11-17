//! A giver's line naming his vanilla item, said before he hands over a dealt
//! one: a copy of the line names the dealt item instead.

use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::{Result, ensure};
use ounce_formats::{Event, EventKind, Flowchart, Params, Romfs};

use super::{GIFTS, Gift, NAME, item_at, levels};
use crate::checks::Drop;
use crate::flows::{self, Rewrites, param, params_mut, string};
use crate::known::{Name, actor, field, flow, item};
use crate::text::Lines;

/// Each line, as its gift's flow, its message file, its label and the item
/// whose name it holds.
static NAMED: LazyLock<[(&Name, String, &Name, &Name); 2]> = LazyLock::new(|| {
    [
        (
            flow::SAGE_OF_FIRE,
            format!("{}/{}", field::EVENT_FLOW_MSG, flow::SAGE_OF_FIRE),
            field::TALK_0028_2,
            actor::OBJ_ARMOR_GORON,
        ),
        // The carpet merchant's thanks: "...were Bombchus!"
        (
            actor::NPC_HYLIA_044_ADULT,
            format!("{}/{}", field::EVENT_FLOW_MSG, actor::NPC_HYLIA_044_ADULT),
            field::TALK_03,
            item::BOMBCHU,
        ),
    ]
});

const MESSAGE: &Name = field::MESSAGE_ID;
/// The line a sale says once paid, a param of its call.
const PAID: &Name = flow::PAYMENT_MESSAGE_ID;

/// A dealt gift's line: the gift, the line's file, label and named item,
/// and the item the gift gives.
struct Dealt<'a> {
    gift: &'static Gift,
    file: &'static str,
    line: &'static Name,
    named: &'static Name,
    item: &'a str,
}

/// Each dealt gift with a line.
fn dealt(drops: &HashMap<u64, Drop>) -> impl Iterator<Item = Dealt<'_>> {
    NAMED.iter().filter_map(|&(flow, ref file, line, named)| {
        let gift = GIFTS.iter().find(|g| g.flow == flow)?;
        let item = item_at(drops, gift.hash(), gift.item);
        (item != gift.item).then_some(Dealt {
            gift,
            file: file.as_str(),
            line,
            named,
            item,
        })
    })
}

/// The label of `line`'s copy naming `item`.
fn label(line: &str, item: &str) -> String {
    format!("{line}_{item}")
}

/// Point a sale's paid line, a copy naming `from`, at the copy naming `to`.
/// A chain by level gives `to` instead.
pub(super) fn repoint(params: &mut Params, from: &str, to: &str) -> Result<()> {
    let line = param(params, PAID).and_then(|l| l.strip_suffix(&*label("", from)));
    match line.map(|l| label(l, to)) {
        Some(to) => flows::set(params, PAID, string(&to)),
        None => Ok(()),
    }
}

/// The copies the dealt gifts' lines need, one message file each: one
/// naming each item the gift can give, every level of a chain by level.
pub fn lines(drops: &HashMap<u64, Drop>, progressive: bool) -> Vec<Lines<'_>> {
    dealt(drops)
        .map(|d| {
            // Only a sale's line is said after the chain picks the level.
            let chained = d.gift.is_sale() && levels::needs(d.gift, d.item, progressive);
            let steps = chained.then(|| levels::steps(d.item)).into_iter().flatten();
            let mut items: Vec<&str> = steps.collect();
            if !items.contains(&d.item) {
                items.push(d.item);
            }
            Lines {
                file: d.file,
                like: (d.line, d.named),
                // A language that can't name the item keeps the vanilla line.
                fallback: d.line,
                new: items.into_iter().map(|i| (label(d.line, i), i)).collect(),
            }
        })
        .collect()
}

/// Point every dealt gift's talk at its line's copy.
pub(super) fn retalk(
    romfs: &Romfs,
    rewrites: &mut Rewrites,
    drops: &HashMap<u64, Drop>,
) -> Result<()> {
    for d in dealt(drops) {
        flows::edit(romfs, rewrites, d.gift.flow, |f| {
            point(
                f,
                &format!("{}:{}", d.file, d.line),
                &format!("{}:{}", d.file, label(d.line, d.item)),
                d.item,
            )
        })?;
    }
    Ok(())
}

/// Swap `from` for `to` in every event of `flow` saying it, at least one:
/// a talk, or a sale of `item` once paid. A sale of anything else keeps
/// its line.
fn point(flow: &mut Flowchart, from: &str, to: &str, item: &str) -> Result<()> {
    let mut pointed = 0;
    for event in &mut flow.events {
        let says = |e: &Event| match &e.kind {
            EventKind::Action { params, .. } => {
                (param(params, MESSAGE) == Some(from)).then_some(MESSAGE)
            }
            EventKind::SubFlow { params, .. } => {
                let sells = param(params, NAME) == Some(item);
                (sells && param(params, PAID) == Some(from)).then_some(PAID)
            }
            _ => None,
        };
        if let Some(key) = says(event) {
            flows::set(params_mut(&mut event.kind), key, string(to))?;
            pointed += 1;
        }
    }
    ensure!(pointed > 0, "{} never says {from}", flow.name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Flows;
    use crate::game::dev_dump;
    use crate::known::actor;

    fn goron_gives(item: &str) -> HashMap<u64, Drop> {
        let gift = GIFTS.iter().find(|g| g.flow == NAMED[0].0).unwrap();
        HashMap::from([(gift.hash(), Drop::from(item.to_owned()))])
    }

    /// Every `MessageId` Link the Goron's flow says, after `drops`.
    fn says(drops: &HashMap<u64, Drop>) -> Vec<String> {
        let romfs = dev_dump().unwrap();
        let mut rewrites = Rewrites::default();
        super::super::give_all(
            &romfs,
            &Flows::load(&romfs).unwrap(),
            &mut rewrites,
            drops,
            false,
            &Default::default(),
        )
        .unwrap();
        let flow = match rewrites.get(&flows::rel(NAMED[0].0)) {
            Some(data) => Flowchart::parse(data).unwrap().unwrap(),
            None => flows::load(&romfs, NAMED[0].0).unwrap().unwrap(),
        };
        flow.events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Action { params, .. } => param(params, MESSAGE).map(str::to_owned),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn link_the_goron_says_the_dealt_item_s_line() {
        if dev_dump().is_none() {
            return;
        }
        let said = says(&goron_gives(actor::OBJ_HEART_PIECE));
        let (file, line) = (&NAMED[0].1, NAMED[0].2);
        assert!(
            said.contains(&format!("{file}:{line}_Obj_Heart_Piece")),
            "{said:?}"
        );
        assert!(!said.contains(&format!("{file}:{line}")), "{said:?}");
    }

    #[test]
    fn the_carpet_merchant_thanks_by_the_dealt_item_only_for_his_first_sale() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let (flow, file, line, _) = &NAMED[1];
        let (flow, line) = (*flow, *line);
        let gift = GIFTS.iter().find(|g| g.flow == flow).unwrap();
        let mut rewrites = Rewrites::default();
        let drops = HashMap::from([(gift.hash(), Drop::from("Item_A".to_owned()))]);
        super::super::give_all(
            &romfs,
            &Flows::load(&romfs).unwrap(),
            &mut rewrites,
            &drops,
            false,
            &Default::default(),
        )
        .unwrap();
        let flow = Flowchart::parse(&rewrites[&flows::rel(flow)])
            .unwrap()
            .unwrap();
        let paid: Vec<_> = (flow.events.iter())
            .filter_map(|e| match &e.kind {
                EventKind::SubFlow { params, .. } => {
                    Some((param(params, NAME)?, param(params, PAID)?))
                }
                _ => None,
            })
            .collect();
        let thanks = format!("{file}:{line}");
        assert!(
            paid.contains(&("Item_A", &format!("{thanks}_Item_A"))),
            "{paid:?}"
        );
        assert!(
            paid.contains(&(actor::SHOP_BOMBCHU_BUNDLE_01, &thanks)),
            "{paid:?}"
        );
    }

    #[test]
    fn link_the_goron_keeps_his_line_when_he_gives_the_tunic() {
        if dev_dump().is_none() {
            return;
        }
        let said = says(&HashMap::new());
        assert!(said.contains(&format!("{}:{}", NAMED[0].1, NAMED[0].2)));
        assert!(lines(&HashMap::new(), true).is_empty());
    }
}
