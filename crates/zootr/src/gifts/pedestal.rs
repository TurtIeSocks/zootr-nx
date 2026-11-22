//! Master Sword shuffle: the pedestal's first pull is a check, as Ship's.
//! The Door of Time's first pull grants the sword with no window. With the
//! door open that pull is the plain swap, which grants nothing, so the swap
//! gives it once. An adult start waits for that swap too, as a child start
//! does: he starts with the Master Sword only through the start inventory.

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
/// The wake-up scene after a pull to adult.
const WAKE: &Name = scene::TIME_TEMPLE_0085;
const POSE: (&Name, &Name) = (
    actor::PLAYER_EVENT,
    flow::EVENT_TRIGGER_PLAYER_REQUEST_CREATE_DYNAMIC_EQUIPMENT_NO_RELATION_POUCH,
);
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
    (gift.dealt(on) || on.contains(DOOR_OPEN)).then(|| GOT.into())
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

/// Run event `at` only while Link holds the Master Sword, asking as `held`
/// asks the pouch. Otherwise go on past it.
fn while_held(flow: &mut Flowchart, at: u16, held: &Model) -> Result<()> {
    let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
        bail!("{} event {at} isn't an action", flow.name);
    };
    let next = next.with_context(|| format!("{} ends at event {at}", flow.name))?;
    let mut params = held.params.clone();
    flows::set(&mut params, ACTOR, string(SWORD.get()))?;
    let (actor, query) = flow.query(&held.actor, held.name);
    let callers = flow.links_to(at);
    // Case 0 is held.
    let ask = flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, at), (1, next)],
    });
    relink(flow, at, callers, ask)
}

/// The equipment swap fills an empty adult sword slot with the Master Sword
/// only while Link holds it, as it does for the shields.
fn swordless(flow: &mut Flowchart, held: &Model) -> Result<()> {
    let set = (flows::events(flow, SET_STRING).into_iter())
        .find(|(_, p)| param(p, flows::VALUE) == Some(SWORD.get()))
        .with_context(|| format!("{} never sets {SWORD}", flow.name))?
        .0;
    while_held(flow, set, held)
}

/// The wake-up scene puts a Master Sword in adult Link's hand for its pose,
/// pouch or not, and only the pose re-equips him from the pouch after. With
/// the pose skipped, a swordless adult kept swinging it until a reload
/// (spike). So the scene hands it over only while he holds it.
fn posed(flow: &mut Flowchart, held: &Model) -> Result<()> {
    let poses: Vec<u16> = (flows::events(flow, POSE).into_iter())
        .filter(|(_, p)| param(p, field::EQUIPMENT_ACTOR_NAME) == Some(SWORD.get()))
        .map(|(e, _)| e)
        .collect();
    ensure!(!poses.is_empty(), "{} never hands Link {SWORD}", flow.name);
    poses
        .into_iter()
        .try_for_each(|at| while_held(flow, at, held))
}

/// The equipment swap's own pouch question, as a model.
fn held(romfs: &Romfs) -> Result<Model> {
    let f = flows::load(romfs, GEAR)?.with_context(|| format!("{GEAR} is a timeline"))?;
    let at = flows::switch(&f, IN_POUCH).with_context(|| format!("{GEAR} asks no pouch"))?;
    flows::model(&f, IN_POUCH, at)
}

/// The edits Master Sword shuffle makes to every pull.
pub(super) fn shuffle(romfs: &Romfs, rewrites: &mut Rewrites, on: &BTreeSet<String>) -> Result<()> {
    if !on.contains(MASTER_SWORD) {
        return Ok(());
    }
    let held = held(romfs)?;
    flows::edit(romfs, rewrites, GEAR, |f| swordless(f, &held))?;
    flows::edit(romfs, rewrites, WAKE, |f| posed(f, &held))
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
    fn given(ids: &[&str], dealt: Option<&Name>) -> Option<Rewrites> {
        let romfs = crate::game::dev_dump()?;
        let all = Flows::load(&romfs).unwrap();
        let drops = (dealt.into_iter())
            .map(|i| (pedestal().hash(), Drop::from(i.to_string())))
            .collect();
        let mut rewrites = Rewrites::default();
        give_all(
            &romfs,
            &all,
            &mut rewrites,
            &drops,
            Default::default(),
            &on(ids),
        )
        .unwrap();
        Some(rewrites)
    }

    fn has(rewrites: &Rewrites, flow: &Name, text: &str) -> bool {
        rewrites.contains_key(&flows::rel(flow))
            && flows::render(&written(rewrites, flow)).contains(text)
    }

    #[test]
    fn the_pedestal_is_dealt_with_the_setting_and_marks_itself() {
        assert!(pedestal().dealt(&on(&[MASTER_SWORD])));
        assert!(!pedestal().dealt(&on(&[DOOR_OPEN])));
        assert_eq!(pedestal().found().as_deref(), Some(GOT));
        assert_eq!(flag(&on(&[])), None);
        assert_eq!(flag(&on(&[MASTER_SWORD])).as_deref(), Some(GOT));
        assert_eq!(flag(&on(&[DOOR_OPEN])).as_deref(), Some(GOT));
    }

    #[test]
    fn the_door_closed_draw_gives_the_dealt_item_with_a_window() {
        let Some(rewrites) = given(&[MASTER_SWORD], Some(item::HOOKSHOT)) else {
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
    fn the_open_door_swap_gives_it_once_and_never_the_opening() {
        let Some(child) = given(&[MASTER_SWORD, DOOR_OPEN], Some(item::HOOKSHOT)) else {
            return;
        };
        let swap = flows::render(&written(&child, SWAP));
        assert!(swap.contains(item::HOOKSHOT.get()));
        let asks = |l: &str| l.contains(IS_BOOL.1.get()) && l.contains(GOT);
        assert!(swap.lines().any(asks));
        // Nor for an adult start: he waits for the swap too.
        assert!(!has(&child, starting::OPENING, item::HOOKSHOT.get()));
        assert!(!has(&child, starting::OPENING, GOT));
        // Undealt, the swap gives the sword itself.
        let off = given(&[DOOR_OPEN], None).unwrap();
        assert!(has(&off, SWAP, SWORD.get()));
        assert!(!has(&off, GEAR, IN_POUCH.1.get()));
    }

    #[test]
    fn only_the_shuffle_asks_the_pouch_before_equipping_the_sword() {
        let Some(on) = given(&[MASTER_SWORD, DOOR_OPEN], None) else {
            return;
        };
        let gear = flows::render(&written(&on, GEAR));
        let asks = |l: &str| l.contains(IN_POUCH.1.get()) && l.contains(SWORD.get());
        assert!(gear.lines().any(asks));
        // The wake-up scene asks before handing him the sword.
        let wake = flows::render(&written(&on, WAKE));
        assert!(wake.lines().any(asks));
        let off = given(&[DOOR_OPEN], None).unwrap();
        assert!(!has(&off, WAKE, IN_POUCH.1.get()));
    }
}
