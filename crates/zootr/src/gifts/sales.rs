//! The sales that are gifts: the call that sells the vanilla item sells the
//! dealt one at the same price, and each sells once.

use super::*;

/// Make the call to `call` at `price` that sells `sold` sell `item`, named
/// by its `ActorName` and shown by its `DisplayItemActor` if it has one.
pub(super) fn sell(
    flow: &mut Flowchart,
    call: &[Call],
    sold: &str,
    price: i32,
    item: &str,
) -> Result<()> {
    let mut sells = 0;
    for i in calls_to(flow, call)? {
        let params = params_mut(&mut flow.events[i].kind);
        let asks = params
            .iter()
            .any(|(k, v)| k == "Price" && *v == Value::Int(vec![price]));
        if !asks || param(params, NAME) != Some(sold) {
            continue;
        }
        flows::set(params, NAME, string(item))?;
        for (key, value) in params.iter_mut() {
            if let Value::ActorId { name, .. } = value
                && key == DISPLAY
            {
                *name = item.into();
            }
        }
        sells += 1;
    }
    ensure!(
        sells > 0,
        "{} never sells {sold} at {price} by {}",
        flow.name,
        said(call)
    );
    Ok(())
}

/// The param a sale shows its item by.
const DISPLAY: &Name = flow::DISPLAY_ITEM_ACTOR;

/// Medigoron asks whether he has sold, by his own Bool, not whether Link
/// holds the knife, since what he sells may be anything.
pub(super) fn ask_medigorons_bool(flow: &mut Flowchart) -> Result<()> {
    let at = (flows::events(flow, IN_POUCH).into_iter())
        .find(|(_, p)| param(p, ACTOR) == Some(actor::WEAPON_SWORD_GIANT))
        .with_context(|| format!("{} never asks for the knife", flow.name))?
        .0;
    let asks =
        flows::switch(flow, IS_BOOL).with_context(|| format!("{} asks no Bool", flow.name))?;
    let asks = flows::model(flow, IS_BOOL, asks)?;
    let mut params = asks.params;
    flows::set(&mut params, GAME_DATA, string(MEDIGORON_SOLD))?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    let EventKind::Switch { cases, .. } = &flow.events[at as usize].kind else {
        unreachable!("the gate was found as a switch");
    };
    // Case 0 is the Bool set, as it was the knife held.
    let cases = cases.clone();
    flow.events[at as usize].kind = EventKind::Switch {
        actor,
        query,
        params,
        cases,
    };
    Ok(())
}

/// The sale Medigoron's call leads to hands over `ActorName` plainly and sets
/// his Bool right after.
pub(super) fn set_medigorons_bool(flow: &mut Flowchart) -> Result<()> {
    let sets = flows::running(flow, SET_BOOL);
    let sets = sets
        .first()
        .with_context(|| format!("{} sets no Bool", flow.name))?;
    let sets = flows::model(flow, SET_BOOL, *sets)?;
    let calls = calls_to(flow, &[get(flow::GET_IN_EVENT_ITEM_SWORD_MEDIGORON)])?;
    ensure!(!calls.is_empty(), "{} never gives the knife", flow.name);
    for i in calls {
        let EventKind::SubFlow {
            entry_point, next, ..
        } = &mut flow.events[i].kind
        else {
            unreachable!("calls_to finds sub-flows");
        };
        if DRESSED.iter().any(|d| **d == **entry_point) {
            *entry_point = PLAIN.to_string();
        }
        let next = *next;
        let mut params = sets.params.clone();
        flows::set(&mut params, GAME_DATA, string(MEDIGORON_SOLD))?;
        flows::set(&mut params, "Value", Value::Bool(vec![true]))?;
        let (actor, action) = flow.action(&sets.actor, sets.name);
        let mark = flow.push(EventKind::Action {
            next,
            actor,
            action,
            params,
        });
        flow.link(Link::Next(i as u16), Some(mark))?;
    }
    buys_into_a_full_pouch(flow)
}

/// Medigoron's sale asks whether the item fits the pouch, and his entry has
/// no message for a full one. A dealt rupee at his price always fills the
/// Adult's Wallet, so the talk ended with nothing said. Every answer buys.
fn buys_into_a_full_pouch(flow: &mut Flowchart) -> Result<()> {
    let fits = pouch_question(flow, MEDIGORON_SALE)?;
    let buy = answer(flow, fits, 0)?;
    cases_mut(flow, fits).iter_mut().for_each(|c| c.1 = buy);
    Ok(())
}

/// The one question whether the item fits the pouch that the sale `call`
/// starts asks. Answer 0 buys, 2 is a full pouch.
fn pouch_question(flow: &Flowchart, Call(_, entry): Call) -> Result<u16> {
    let reach = flows::reachable(flow, entry)?;
    let fits = running(flow, FITS_POUCH);
    let fits: Vec<u16> = fits.into_iter().filter(|i| reach.contains(i)).collect();
    match fits[..] {
        [fits] => Ok(fits),
        _ => bail!("{entry} asks whether the item fits {} times", fits.len()),
    }
}

/// Where the switch at `at` goes on `case`.
fn answer(flow: &mut Flowchart, at: u16, case: u32) -> Result<u16> {
    let found = cases_mut(flow, at)
        .iter()
        .find(|c| c.0 == case)
        .map(|c| c.1);
    found.with_context(|| format!("{} event {at} has no case {case}", flow.name))
}

/// The cases of the switch at `at`.
fn cases_mut(flow: &mut Flowchart, at: u16) -> &mut Vec<(u32, u16)> {
    let EventKind::Switch { cases, .. } = &mut flow.events[at as usize].kind else {
        unreachable!("the question was found as a switch");
    };
    cases
}

/// The bean seller's one hand-over gives what the sale it's in names, which
/// is a bean in every call but the first sale's.
pub(super) fn hand_over_the_sold(flow: &mut Flowchart) -> Result<()> {
    let beans: Vec<usize> = calls_to(flow, &[HAND_OVER])?
        .into_iter()
        .filter(|&i| {
            let params = params_mut(&mut flow.events[i].kind);
            param(params, GIVEN) == Some(item::MAGIC_BEAN)
        })
        .collect();
    ensure!(
        beans.len() == 1,
        "{} hands over beans {} times, not once",
        flow.name,
        beans.len()
    );
    let params = params_mut(&mut flow.events[beans[0]].kind);
    flows::set(params, GIVEN, Value::Argument(NAME.to_string()))
}

/// The param a hand-over names its item by.
const GIVEN: &Name = flow::REVEIVE_ITEM;

/// A query on `flow`'s Bool `name`, modelled on the flow's first Bool
/// query: to `set` when it holds, else to `unset`.
fn ask_bool(flow: &mut Flowchart, name: &str, set: u16, unset: u16) -> Result<u16> {
    let asks =
        flows::switch(flow, IS_BOOL).with_context(|| format!("{} asks no Bool", flow.name))?;
    let asks = flows::model(flow, IS_BOOL, asks)?;
    let mut params = asks.params;
    flows::set(&mut params, GAME_DATA, string(name))?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    // Case 0 is true.
    Ok(flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, set), (1, unset)],
    }))
}

/// The carpet merchant's one sale, now of the dealt item, runs only until
/// he has sold. A copy of it sells his Bombchus after that.
pub(super) fn sell_bombchus_after(flow: &mut Flowchart) -> Result<()> {
    let [sale] = calls_to(flow, &[CARPET_SALE])?[..] else {
        bail!("{} has no one sale", flow.name);
    };
    let sale = sale as u16;
    let mut bombchus = flow.events[sale as usize].kind.clone();
    flows::set(params_mut(&mut bombchus), NAME, string(CARPET_BOMBCHUS))?;
    let links = flow.links_to(sale);
    ensure!(!links.is_empty(), "nothing runs {}'s sale", flow.name);
    let bombchus = flow.push(bombchus);
    let ask = ask_bool(flow, CARPET_SOLD, bombchus, sale)?;
    links.into_iter().try_for_each(|l| flow.link(l, Some(ask)))
}

/// The plain sale's full-pouch answer buys while the carpet merchant hasn't
/// sold, so a dealt item that fills its pouch still sells once.
pub(super) fn buys_while_unsold(flow: &mut Flowchart) -> Result<()> {
    let fits = pouch_question(flow, CARPET_SALE)?;
    let (buy, full) = (answer(flow, fits, 0)?, answer(flow, fits, 2)?);
    let ask = ask_bool(flow, CARPET_SOLD, full, buy)?;
    (cases_mut(flow, fits).iter_mut())
        .filter(|c| c.0 == 2)
        .for_each(|c| c.1 = ask);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, flag, flow, item};

    /// A flow before and after `f`, rendered, as the game would read it.
    fn edited(
        romfs: &Romfs,
        name: &str,
        f: impl FnOnce(&mut Flowchart) -> Result<()>,
    ) -> (String, String) {
        let mut flow = flows::load(romfs, name).unwrap().unwrap();
        let before = flows::render(&flow);
        f(&mut flow).unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        (before, flows::render(&after))
    }

    /// The lines of `after` that differ from `before` at the same place.
    fn changed<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .collect()
    }

    fn sale(flow: &str) -> (&'static Gift, &'static [Call], &'static str, i32) {
        let gift = GIFTS.iter().find(|g| g.flow == flow).unwrap();
        let How::Sale {
            call, sold, price, ..
        } = gift.how
        else {
            panic!("{flow} isn't a sale");
        };
        (gift, call, sold, price)
    }

    #[test]
    fn each_poe_collector_gift_renames_only_the_call_naming_its_item() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gifts: Vec<_> = GIFTS
            .iter()
            .filter(|g| g.flow == actor::NPC_HYLIA_048_BOTTLE_SHOP)
            .collect();
        assert_eq!(gifts.len(), 2);
        assert_ne!(gifts[0].hash(), gifts[1].hash());
        for gift in gifts {
            let How::Calls { call, found } = gift.how else {
                panic!("{} is a call gift", gift.giver);
            };
            assert!(found.is_none());
            let (before, after) = edited(&romfs, gift.flow, |f| {
                rename_calls(f, call, gift.item, "Item_A")
            });
            let lines = changed(&before, &after);
            assert_eq!(lines.len(), 1, "{} {lines:#?}", gift.giver);
            let (was, now) = lines[0];
            assert!(was.contains(&format!("=\"{}\"", gift.item)), "{was}");
            assert!(now.contains("=\"Item_A\"") && !now.contains(&**gift.item));
            assert!(now.contains(&format!("::{PLAIN}(")), "{now}");
        }
    }

    #[test]
    fn the_sales_are_dealt_by_merchants_alone() {
        let sales: Vec<_> = GIFTS.iter().filter(|g| g.is_sale()).collect();
        assert_eq!(sales.len(), 3);
        assert!(sales.iter().all(|g| g.setting() == Some(MERCHANTS)));
        let on = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert!(sales.iter().all(|g| g.dealt(&on(&[MERCHANTS]))));
        assert!(sales.iter().all(|g| !g.dealt(&on(&[NPC_GIFTS]))));
        assert_eq!(flags(&on(&[MERCHANTS])), [MEDIGORON_SOLD]);
        assert!(flags(&on(&[NPC_GIFTS])).is_empty());
    }

    #[test]
    fn medigoron_sells_the_dealt_item_once_by_his_own_bool() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (gift, call, sold, price) = sale(MEDIGORON);
        let How::Sale { gate, shop, .. } = gift.how else {
            unreachable!()
        };
        let (before, after) = edited(&romfs, MEDIGORON, |f| {
            sell(f, call, sold, price, "Item_A")?;
            gate.unwrap()(f)
        });
        // The sale's call, and the gate in front of the talk.
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 2, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(was.starts_with("#2 ") && now.starts_with("#2 "), "{now}");
        assert!(was.contains("IsExistInPouch") && was.ends_with("? 0:#35 1:#3"));
        assert!(
            now.contains(&format!("{}(", flow::EVENT_QUERY_IS_GAME_DATA_BOOL))
                && now.contains(MEDIGORON_SOLD)
        );
        assert!(now.ends_with("? 0:#35 1:#3"), "{now}");
        let (_, now) = lines[1];
        assert!(
            now.starts_with("#6 ") && now.contains(&format!("{}=\"Item_A\"", flag::ACTOR_NAME))
        );
        assert!(
            now.contains(&format!("{}=<Item_A >", flow::DISPLAY_ITEM_ACTOR)) && !now.contains(sold)
        );
        // The shop buys whether or not the item fits the pouch, hands it
        // over plainly, then sets the Bool.
        let (before, after) = edited(&romfs, SIMPLE_SHOP, shop);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 3, "{lines:#?}");
        // Its full-pouch cancel is out of reach.
        let (was, now) = lines[0];
        assert!(was.contains(" #13") && !now.contains(" #13"), "{now}");
        let (was, now) = lines[1];
        assert!(
            was.contains(&format!("{}(", flow::EVENT_QUERY_CAN_ADD_TO_PORCH))
                && was.ends_with("? 0:#18 1:#16 2:#13")
        );
        assert!(now.ends_with("? 0:#18 1:#18 2:#18"), "{now}");
        let (was, now) = lines[2];
        assert!(
            was.contains(&format!("::{}(", flow::GET_IN_EVENT_ITEM_SWORD_MEDIGORON))
                && was.starts_with("#112 ")
        );
        assert!(now.contains(&format!("::{PLAIN}(")) && now.starts_with("#112 "));
        let set = after.lines().find(|l| l.contains(MEDIGORON_SOLD)).unwrap();
        let at = set.split(' ').next().unwrap();
        assert!(now.ends_with(&format!("-> {at}")), "{now}");
        assert!(
            set.contains(&format!("{}(", flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL))
                && set.contains("Value=true")
        );
        assert!(set.ends_with(&was[was.rfind("-> ").unwrap()..]), "{set}");
    }

    #[test]
    fn the_carpet_merchant_sells_the_dealt_item_until_he_has_sold() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (gift, call, sold, price) = sale(CARPET_MERCHANT);
        let How::Sale { gate, shop, .. } = gift.how else {
            unreachable!()
        };
        let (before, after) = edited(&romfs, CARPET_MERCHANT, |f| {
            sell(f, call, sold, price, "Item_A")?;
            gate.unwrap()(f)
        });
        let line = |at: &str| {
            after
                .lines()
                .find(|l| l.starts_with(&format!("{at} ")))
                .unwrap()
        };
        let sold_bool = format!("{}=\"{CARPET_SOLD}\"", field::GAME_DATA_NAME);
        let ask = (after.lines().rev())
            .find(|l| l.contains(&sold_bool))
            .unwrap();
        let ask_at = ask.split(' ').next().unwrap();
        assert!(!before.contains(&format!("{ask_at} ")), "{ask}");
        // Both talks that led to the sale now ask first.
        for talk in ["#12", "#13"] {
            assert!(
                line(talk).ends_with(&format!("-> {ask_at}")),
                "{}",
                line(talk)
            );
        }
        // Sold goes to a Bombchu copy, unsold to the dealt sale.
        let (set, unset) = ask
            .rsplit_once("? 0:")
            .unwrap()
            .1
            .split_once(" 1:")
            .unwrap();
        assert_eq!(unset, "#0");
        assert!(
            line("#0").contains(&format!("{}=\"Item_A\"", flag::ACTOR_NAME))
                && line("#0").contains("Price=90 ")
        );
        assert!(
            line(set).contains(&format!("{}=\"{sold}\"", flag::ACTOR_NAME)),
            "{}",
            line(set)
        );
        assert!(
            line(set).contains("Price=90 ")
                && line(set).contains(&format!("::{}(", flow::SIMPLE_SHOP_ROOT))
        );
        // The plain sale's full pouch buys until he has sold.
        let (before, after) = edited(&romfs, SIMPLE_SHOP, shop);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(
            was.starts_with("#7 ") && was.ends_with("? 0:#25 1:#8 2:#5"),
            "{was}"
        );
        let ask_at = now.rsplit_once(" 2:").unwrap().1;
        let ask = after
            .lines()
            .find(|l| l.starts_with(&format!("{ask_at} ")))
            .unwrap();
        assert!(
            ask.contains(&sold_bool) && ask.ends_with("? 0:#5 1:#25"),
            "{ask}"
        );
    }

    #[test]
    fn the_bean_seller_gives_the_dealt_item_for_his_first_sale_only() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (gift, call, sold, price) = sale(BEAN_SELLER);
        let How::Sale { gate, shop, .. } = gift.how else {
            unreachable!()
        };
        assert!(gate.is_none());
        let (before, after) = edited(&romfs, BEAN_SELLER, |f| {
            sell(f, call, sold, price, "Item_A")
        });
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(was.starts_with("#0 ") && now.starts_with("#0 "));
        assert!(
            now.contains("Price=10 ") && now.contains(&format!("{}=\"Item_A\"", flag::ACTOR_NAME))
        );
        // The seven others, at 20 and up, still sell beans.
        let beans = |t: &str| {
            t.matches(&format!("{}=\"{}\"", flag::ACTOR_NAME, item::MAGIC_BEAN))
                .count()
        };
        assert_eq!(beans(&before), beans(&after) + 1);
        assert!(beans(&after) >= 5);
        // The one hand-over gives what its sale names.
        let (before, after) = edited(&romfs, SIMPLE_SHOP, shop);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(
            was.contains(&format!("{}=\"{}\"", flow::REVEIVE_ITEM, item::MAGIC_BEAN)),
            "{was}"
        );
        assert_eq!(
            was.replace(
                &format!("\"{}\"", item::MAGIC_BEAN),
                &format!("${}", flag::ACTOR_NAME)
            ),
            now
        );
    }
}
