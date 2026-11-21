//! Master Sword shuffle: the pedestal's first pull is a check, as Ship's.
//! The Door of Time's first pull grants the sword with no window. With the
//! door open that pull is the plain swap, which grants nothing, so the swap
//! gives it once. An adult start gives it in the opening, as Link's Pocket.

use super::*;
use crate::flows::Model;
use later::models;

/// The Door of Time's flow, whose first pull grants the sword.
pub(super) const FLOW: &Name = flow::DOOR_OF_TIME;
/// The plain swap a pull runs with the door open.
const SWAP: &Name = flow::MARKET_CHILD_FIELD_CHILD_TO_ADULT;
/// The equipment swap every pull runs.
const GEAR: &Name = flow::REPLACE_EQUIPMENT_FOR_ERA_CHANGE;
const SWORD: &Name = actor::WEAPON_SWORD_MASTER;
/// Set once the pedestal's gift is given.
pub(crate) const GOT: &str = "Randomizer_MasterSwordPedestal";
/// The pulls that change Link's age.
const PULLS: [&Name; 3] = [FLOW, SWAP, flow::MARKET_ADULT_FIELD_ADULT_TO_CHILD];
/// Re-dress the player from his gear and wait for it, as the boss rush
/// does after it changes his gear.
const DRESS: [(&Name, &Name); 2] = [
    (
        field::UI_SYSTEM_ACTOR,
        flow::EVENT_TRIGGER_SYNC_PLAYER_EQUIPMENT_AND_POUCH,
    ),
    (
        actor::PLAYER_EVENT,
        flow::EVENT_WAIT_CREATION_FINISHED_EQUIPMENT,
    ),
];
const SET_STRING: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_TRIGGER_SET_GAME_DATA_STRING64,
);

impl Gift {
    pub(super) fn is_pedestal(&self) -> bool {
        matches!(self.how, How::Pedestal)
    }
}

/// `GOT`, when a new game with `on` writes the pedestal's gift: dealt, or
/// moved to the swap by the open door.
pub(super) fn flag(on: &BTreeSet<String>) -> Option<String> {
    let gift = GIFTS.iter().find(|g| g.is_pedestal())?;
    (gift.dealt(on) || on.contains(OPEN_DOOR)).then(|| GOT.into())
}

/// What the pedestal gives in a seed dealt `drops`.
pub fn gives(drops: &HashMap<u64, Drop>) -> &str {
    let gift = GIFTS.iter().find(|g| g.is_pedestal());
    gift.map_or(SWORD.get(), |g| item_at(drops, g.hash(), g.item))
}

/// The flow that gives the pedestal's gift where its pull can't: the
/// swap.
pub(super) fn caller() -> &'static str {
    SWAP.get()
}

/// Grant `item` with its window, set `GOT`, then go on to `next`. Returns
/// the grant.
fn granting(
    flow: &mut Flowchart,
    flows: &Flows,
    setter: &Model,
    item: &str,
    next: Option<u16>,
) -> Result<u16> {
    let (actor, action) = flow.action(&setter.actor, setter.name);
    let mut params = setter.params.clone();
    flows::set(&mut params, GAME_DATA, string(GOT))?;
    flows::set(&mut params, flows::VALUE, Value::Bool(vec![true]))?;
    let set = flow.push(EventKind::Action {
        next,
        actor,
        action,
        params,
    });
    models(flows)?.push(flow, string(item), Some(set))
}

/// Send every link to `at` to `to` instead.
fn relink(flow: &mut Flowchart, at: u16, callers: Vec<Link>, to: u16) -> Result<()> {
    ensure!(
        !callers.is_empty(),
        "{} never reaches event {at}",
        flow.name
    );
    callers.into_iter().try_for_each(|l| flow.link(l, Some(to)))
}

/// The first pull with the door closed: its sword grant gives `item`, with
/// a window. Returns the grant.
pub(super) fn in_draw(
    romfs: &Romfs,
    flow: &mut Flowchart,
    flows: &Flows,
    item: &str,
) -> Result<u16> {
    let at = (flows::events(flow, GRANT).into_iter())
        .find(|(_, p)| param(p, ACTOR) == Some(SWORD.get()))
        .with_context(|| format!("{} never grants {SWORD}", flow.name))?
        .0;
    let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
        unreachable!("a grant is an action");
    };
    let callers = flow.links_to(at);
    let first = granting(
        flow,
        flows,
        &crate::chest_levels::setter(romfs)?,
        item,
        next,
    )?;
    relink(flow, at, callers, first)?;
    Ok(first)
}

/// The plain swap: before the equipment swap, give `item` once.
pub(super) fn in_swap(
    romfs: &Romfs,
    flow: &mut Flowchart,
    flows: &Flows,
    item: &str,
) -> Result<()> {
    let gear = calls_to(flow, &[call(GEAR, GEAR)])?;
    let [gear] = gear[..] else {
        bail!("{} calls {GEAR} {} times", flow.name, gear.len());
    };
    let gear = gear as u16;
    let callers = flow.links_to(gear);
    let grant = granting(
        flow,
        flows,
        &crate::chest_levels::setter(romfs)?,
        item,
        Some(gear),
    )?;
    // Case 0 is given already.
    let ask = bool_switch(flow, GOT, vec![(0, gear), (1, grant)])?;
    let ask = flow.push(ask);
    relink(flow, gear, callers, ask)
}

/// The opening, for an adult start: give `item` just before the autosave.
pub(super) fn in_opening(
    romfs: &Romfs,
    flow: &mut Flowchart,
    flows: &Flows,
    item: &str,
) -> Result<()> {
    let save = only(flow, starting::AUTOSAVE)?;
    let callers = flow.links_to(save);
    let first = granting(
        flow,
        flows,
        &crate::chest_levels::setter(romfs)?,
        item,
        Some(save),
    )?;
    relink(flow, save, callers, first)
}

/// The equipment swap fills an empty adult sword slot with the Master Sword
/// only while Link holds it, as it does for the shields.
pub(super) fn swordless(flow: &mut Flowchart) -> Result<()> {
    let set = (flows::events(flow, SET_STRING).into_iter())
        .find(|(_, p)| param(p, flows::VALUE) == Some(SWORD.get()))
        .with_context(|| format!("{} never sets {SWORD}", flow.name))?
        .0;
    let EventKind::Action { next, .. } = flow.events[set as usize].kind else {
        unreachable!("a setter is an action");
    };
    let next = next.with_context(|| format!("{} ends at its sword", flow.name))?;
    let asks =
        flows::switch(flow, IN_POUCH).with_context(|| format!("{} asks no pouch", flow.name))?;
    let asks = flows::model(flow, IN_POUCH, asks)?;
    let mut params = asks.params;
    flows::set(&mut params, ACTOR, string(SWORD.get()))?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    let callers = flow.links_to(set);
    // Case 0 is held.
    let ask = flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, set), (1, next)],
    });
    relink(flow, set, callers, ask)
}

/// Once a pull flips Link's age, the player re-dresses from his gear. The
/// grab puts the Master Sword in his hand outside any flow, and neither an
/// empty sword slot nor re-dressing takes it away: a swordless adult kept
/// swinging it until a reload (spike). So with the slot empty and no Master
/// Sword in the pouch, it's taken from his hand as Ganon does.
fn redressed(flow: &mut Flowchart, dress: &Dress) -> Result<()> {
    let ages: Vec<u16> = (flows::events(flow, flows::SET_BOOL).into_iter())
        .filter(|(_, p)| param(p, GAME_DATA) == Some(flag::SEQUENCE_IS_CHILDHOOD.get()))
        .map(|(e, _)| e)
        .collect();
    ensure!(!ages.is_empty(), "{} never changes Link's age", flow.name);
    for at in ages {
        let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
            unreachable!("a setter is an action");
        };
        let next = next.with_context(|| format!("{} ends at its age", flow.name))?;
        let remove = push(flow, &dress.remove, Ok(next));
        // Case 0 is held.
        let held = push(flow, &dress.held, Err(vec![(0, next), (1, remove)]));
        // Case 0 is empty.
        let empty = push(flow, &dress.empty, Err(vec![(0, held), (1, next)]));
        let mut then = empty;
        for m in dress.sync.iter().rev() {
            then = push(flow, m, Ok(then));
        }
        if let EventKind::Action { next, .. } = &mut flow.events[at as usize].kind {
            *next = Some(then);
        }
    }
    Ok(())
}

/// Push `m` as an action going on to `Ok(next)`, or a switch on `Err(cases)`.
fn push(flow: &mut Flowchart, m: &Model, to: std::result::Result<u16, Vec<(u32, u16)>>) -> u16 {
    let params = m.params.clone();
    let kind = match to {
        Ok(next) => {
            let (actor, action) = flow.action(&m.actor, m.name);
            EventKind::Action {
                next: Some(next),
                actor,
                action,
                params,
            }
        }
        Err(cases) => {
            let (actor, query) = flow.query(&m.actor, m.name);
            EventKind::Switch {
                actor,
                query,
                params,
                cases,
            }
        }
    };
    flow.push(kind)
}

/// The events a pull re-dresses Link with: the boss rush's,
/// then the equipment swap's own questions and Ganon's taking the sword.
struct Dress {
    sync: Vec<Model>,
    empty: Model,
    held: Model,
    remove: Model,
}

const IS_EMPTY: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_QUERY_IS_EMPTY_GAME_DATA_STRING64,
);
const REMOVE: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_REMOVE_WEAPON_KEEP_SHEATH,
);

/// The first event in `name` that runs or asks `what`, as a model.
fn first(romfs: &Romfs, name: &Name, what: (&Name, &Name)) -> Result<Model> {
    let f = flows::load(romfs, name)?.with_context(|| format!("{name} is a timeline"))?;
    let at = (flows::running(&f, what).first().copied())
        .or_else(|| flows::switch(&f, what))
        .with_context(|| format!("{name} never runs {}", what.1))?;
    flows::model(&f, what, at)
}

fn dress(romfs: &Romfs) -> Result<Dress> {
    let sync = (DRESS.iter()).map(|&what| first(romfs, flow::NPC_DREAM, what));
    let mut empty = first(romfs, GEAR, IS_EMPTY)?;
    flows::set(
        &mut empty.params,
        GAME_DATA,
        string(flag::PLAYER_STATUS_EQUIP_WEAPON.get()),
    )?;
    let mut held = first(romfs, GEAR, IN_POUCH)?;
    flows::set(&mut held.params, ACTOR, string(SWORD.get()))?;
    let remove = first(romfs, flow::DEFEAT_GANON, REMOVE)?;
    ensure!(param(&remove.params, ACTOR) == Some(SWORD.get()));
    Ok(Dress {
        sync: sync.collect::<Result<_>>()?,
        empty,
        held,
        remove,
    })
}

/// The edits Master Sword shuffle makes to every pull.
pub(super) fn shuffle(romfs: &Romfs, rewrites: &mut Rewrites, on: &BTreeSet<String>) -> Result<()> {
    if !on.contains(MASTER_SWORD) {
        return Ok(());
    }
    flows::edit(romfs, rewrites, GEAR, swordless)?;
    let dress = dress(romfs)?;
    (PULLS.iter()).try_for_each(|pull| flows::edit(romfs, rewrites, pull, |f| redressed(f, &dress)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::written;
    use crate::known::item;

    fn on(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|&i| i.to_owned()).collect()
    }

    fn pedestal() -> &'static Gift {
        GIFTS.iter().find(|g| g.is_pedestal()).unwrap()
    }

    /// The seed's flows, the pedestal dealt `dealt`.
    fn given(ids: &[&str], dealt: Option<&Name>, adult: bool) -> Option<Rewrites> {
        let romfs = crate::game::dev_dump()?;
        let all = Flows::load(&romfs).unwrap();
        let drops = (dealt.into_iter())
            .map(|i| (pedestal().hash(), Drop::from(i.to_string())))
            .collect();
        let mut rewrites = Rewrites::default();
        give_all_aged(&romfs, &all, &mut rewrites, &drops, false, &on(ids), adult).unwrap();
        Some(rewrites)
    }

    fn has(rewrites: &Rewrites, flow: &Name, text: &str) -> bool {
        rewrites.contains_key(&flows::rel(flow))
            && flows::render(&written(rewrites, flow)).contains(text)
    }

    #[test]
    fn the_pedestal_is_dealt_with_the_setting_and_marks_itself() {
        assert!(pedestal().dealt(&on(&[MASTER_SWORD])));
        assert!(!pedestal().dealt(&on(&[OPEN_DOOR])));
        assert_eq!(pedestal().found().as_deref(), Some(GOT));
        assert_eq!(flag(&on(&[])), None);
        assert_eq!(flag(&on(&[MASTER_SWORD])).as_deref(), Some(GOT));
        assert_eq!(flag(&on(&[OPEN_DOOR])).as_deref(), Some(GOT));
    }

    #[test]
    fn the_door_closed_draw_gives_the_dealt_item_with_a_window() {
        let Some(rewrites) = given(&[MASTER_SWORD], Some(item::HOOKSHOT), false) else {
            return;
        };
        let flow = written(&rewrites, FLOW);
        let draw = flows::render(&flow);
        assert!(draw.contains(item::HOOKSHOT.get()) && draw.contains(GOT));
        // The sword's grant is left with nothing leading to it.
        let (old, _) = (flows::events(&flow, GRANT).into_iter())
            .find(|(_, p)| param(p, ACTOR) == Some(SWORD.get()))
            .unwrap();
        assert!(flow.links_to(old).is_empty());
        assert!(!has(&rewrites, SWAP, GOT));
    }

    #[test]
    fn the_open_door_swap_gives_it_once_and_an_adult_start_in_the_opening() {
        let Some(child) = given(&[MASTER_SWORD, OPEN_DOOR], Some(item::HOOKSHOT), false) else {
            return;
        };
        let swap = flows::render(&written(&child, SWAP));
        assert!(swap.contains(item::HOOKSHOT.get()));
        let asks = |l: &str| l.contains(IS_BOOL.1.get()) && l.contains(GOT);
        assert!(swap.lines().any(asks));
        assert!(!has(&child, starting::OPENING, item::HOOKSHOT.get()));
        let adult = given(&[MASTER_SWORD, OPEN_DOOR], Some(item::HOOKSHOT), true).unwrap();
        assert!(has(&adult, starting::OPENING, item::HOOKSHOT.get()));
        assert!(has(&adult, starting::OPENING, GOT));
        // Undealt, the swap gives the sword itself.
        let off = given(&[OPEN_DOOR], None, false).unwrap();
        assert!(has(&off, SWAP, SWORD.get()));
        assert!(!has(&off, GEAR, IN_POUCH.1.get()));
    }

    #[test]
    fn only_the_shuffle_asks_the_pouch_before_equipping_the_sword() {
        let Some(on) = given(&[MASTER_SWORD, OPEN_DOOR], None, false) else {
            return;
        };
        let gear = flows::render(&written(&on, GEAR));
        let asks = |l: &str| l.contains(IN_POUCH.1.get()) && l.contains(SWORD.get());
        assert!(gear.lines().any(asks));
        // Every pull re-dresses Link once his age flips.
        for pull in PULLS {
            assert!(has(&on, pull, DRESS[0].1.get()), "{pull}");
        }
        let off = given(&[OPEN_DOOR], None, false).unwrap();
        assert!(PULLS.iter().all(|pull| !has(&off, pull, DRESS[0].1.get())));
        let swap = flows::render(&written(&on, SWAP));
        assert!(swap.contains(REMOVE.1.get()) && swap.contains(IS_EMPTY.1.get()));
        assert_eq!(gives(&HashMap::new()), SWORD.get());
    }
}
