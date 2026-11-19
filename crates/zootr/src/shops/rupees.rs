//! Before a sale the shop asks whether the goods all fit in Link's pouch,
//! and warns him when they don't. No vanilla shop sells rupees, and for
//! rupees the pouch answers "only some" with the wallet far from full, so
//! the keeper said the bag was full on every rupee a check slot sold. A
//! rupee sale skips the question. Past the wallet's cap the excess is lost,
//! as with any rupee Link picks up.

use anyhow::{Context, Result, ensure};
use ounce_formats::{BymlDoc, EventKind, Flowchart, Romfs};

use crate::flows::{self, GAME_DATA, Rewrites, VALUE, events, param, params_mut, string};
use crate::known::{Name, field, flag, flow};
use crate::standins::{POUCH_ACTOR_INFO, row_id};

/// The query comparing a GameData String64 with its `Value`. Case 0 is equal.
const COMPARE: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_QUERY_IS_GAME_DATA_STRING64,
);
/// Whether the item a shop sale gives fits Link's pouch. Case 0 is yes.
const FITS: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_QUERY_CAN_ADD_TO_PORCH_BY_GAME_DATA,
);
/// The item a shop sale gives, by its pouch row.
const SELECTED: &Name = flag::SHOP_SELECT_ITEM_NAME;
/// The pouch category of every rupee.
const RUPEE: &str = "Rupee";

/// Every pouch row that's a rupee.
fn rupees(romfs: &Romfs) -> Result<Vec<String>> {
    let pouch = BymlDoc::parse(&romfs.read(POUCH_ACTOR_INFO)?)?;
    let is_rupee = |r: &&ounce_formats::Byml| {
        let category = r
            .as_map()
            .ok()
            .and_then(|m| m.get(field::POUCH_CATEGORY.get()));
        category
            .and_then(|c| c.as_string().ok())
            .is_some_and(|c| *c == RUPEE)
    };
    let rows = pouch.root.as_array()?.iter().filter(is_rupee);
    Ok(rows.map(|r| row_id(r).to_owned()).collect())
}

/// Has every pouch question in the shop flow pass a rupee as fitting.
pub fn write(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    let rupees = rupees(romfs)?;
    flows::edit(romfs, rewrites, flow::DISPLAY_SHOP, |flow| {
        skip(flow, &rupees)
    })
}

fn skip(flow: &mut Flowchart, rupees: &[String]) -> Result<()> {
    let template = (events(flow, COMPARE).into_iter())
        .find(|(_, p)| param(p, GAME_DATA) == Some(SELECTED.get()))
        .context("no compare of the selected item")?
        .0;
    let asks = flows::running(flow, FITS);
    ensure!(!asks.is_empty(), "the shop flow asks no pouch question");
    for ask in asks {
        let fits = match &flow.events[ask as usize].kind {
            EventKind::Switch { cases, .. } => cases.iter().find(|c| c.0 == 0).map(|c| c.1),
            _ => None,
        };
        let fits = fits.with_context(|| format!("pouch question {ask} has no yes"))?;
        let into = flow.links_to(ask);
        let mut head = ask;
        for rupee in rupees.iter().rev() {
            let mut compare = flow.events[template as usize].kind.clone();
            flows::set(params_mut(&mut compare), VALUE, string(rupee))?;
            if let EventKind::Switch { cases, .. } = &mut compare {
                *cases = vec![(0, fits), (1, head)];
            }
            head = flow.push(compare);
        }
        into.into_iter()
            .try_for_each(|l| flow.link(l, Some(head)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::actor;

    #[test]
    fn a_rupee_sale_passes_the_pouch_question_as_fitting() {
        let Some(romfs) = dev_dump() else { return };
        let rupees = rupees(&romfs).unwrap();
        assert!(rupees.iter().any(|r| r == actor::OBJ_RUPEE_RED.get()));
        let mut rewrites = Rewrites::default();
        write(&romfs, &mut rewrites).unwrap();
        let flow = flows::current(&romfs, &rewrites, flow::DISPLAY_SHOP).unwrap();
        let asks = flows::running(&flow, FITS);
        assert!(!asks.is_empty());
        for ask in asks {
            let EventKind::Switch { cases, .. } = &flow.events[ask as usize].kind else {
                panic!("{ask} isn't a switch");
            };
            let fits = cases.iter().find(|c| c.0 == 0).unwrap().1;
            // Whatever came to the question first asks each rupee by name,
            // each going on to where the question's yes goes.
            let mut seen = Vec::new();
            let mut at = flow.links_to(ask);
            while let [ounce_formats::Link::Branch(e, 1)] = at[..] {
                let EventKind::Switch { params, cases, .. } = &flow.events[e as usize].kind else {
                    break;
                };
                let Some(rupee) = param(params, VALUE) else {
                    break;
                };
                assert_eq!(cases[0].1, fits, "{rupee}");
                seen.push(rupee.to_owned());
                at = flow.links_to(e);
            }
            seen.reverse();
            assert_eq!(seen, rupees, "pouch question {ask}");
        }
    }
}
