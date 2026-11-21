//! Link's Pocket: with the Door of Time open, Rauru's scene never plays, so
//! the opening gives his check, as Ship does.

use super::*;
use later::models;

/// The check a new game with `on` gives in its opening.
pub fn pocket(on: &BTreeSet<String>) -> Option<u64> {
    GIFTS.iter().find(|g| g.pocket(on)).map(Gift::hash)
}

impl Gift {
    /// Whether it's Link's Pocket in a new game with `on`.
    pub(super) fn pocket(&self, on: &BTreeSet<String>) -> bool {
        self.flow == RAURU && on.contains(DOOR_OPEN)
    }

    /// Where its grant moves when its scene can't play (`lost`) or comes
    /// later: the opening for Link's Pocket, else its caller.
    pub(super) fn moved_to(&self, lost: bool, pocket: bool) -> Option<&'static str> {
        match pocket {
            true => Some(starting::OPENING.get()),
            false => (self.caller()).filter(|_| lost || matches!(self.how, How::Later { .. })),
        }
    }
}

/// The level of `item` Link's Pocket gives: the opening can't ask the pouch
/// right after its own grants (it still reads them as missing), so the
/// level comes from what the opening grants before it.
pub(super) fn level<'a>(
    romfs: &Romfs,
    rewrites: &Rewrites,
    item: &'a str,
    lines: crate::progressive::Lines,
) -> Result<&'a str> {
    let opening = flows::current(romfs, rewrites, starting::OPENING)?;
    let owned = (flows::running(&opening, GRANT).into_iter())
        .filter_map(|at| match &opening.events[at as usize].kind {
            EventKind::Action { params, .. } => param(params, ACTOR).map(str::to_owned),
            _ => None,
        })
        .collect();
    Ok(levels::owned_level(item, &owned, lines))
}

/// Grant `item` in the opening just before its autosave, then show the get
/// window.
pub(super) fn grant_before_save(flow: &mut Flowchart, flows: &Flows, item: &str) -> Result<()> {
    let save = only(flow, starting::AUTOSAVE)?;
    let callers = flow.links_to(save);
    let first = models(flows)?.push(flow, string(item), Some(save))?;
    callers
        .into_iter()
        .try_for_each(|l| flow.link(l, Some(first)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::written;
    use crate::known::item;

    fn on(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|&i| i.to_owned()).collect()
    }

    fn rauru() -> &'static Gift {
        GIFTS.iter().find(|g| g.flow == RAURU).unwrap()
    }

    #[test]
    fn rauru_is_dealt_with_rewards_and_pocketed_with_the_door_open() {
        let rauru = rauru();
        assert!(rauru.dealt(&on(&[DUNGEON_REWARDS])));
        assert!(rauru.dealt(&on(&[DUNGEON_REWARDS, DOOR_OPEN])));
        assert!(!rauru.dealt(&on(&[])));
        assert_eq!(pocket(&on(&[DOOR_OPEN])), Some(rauru.hash()));
        assert_eq!(pocket(&on(&[DUNGEON_REWARDS])), None);
    }

    #[test]
    fn the_open_door_grants_rauru_s_item_before_the_opening_saves() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let opening = |ids: &[&str], dealt: Option<&Name>| {
            let drops = (dealt.into_iter())
                .map(|i| (rauru().hash(), Drop::from(i.to_string())))
                .collect();
            let mut rewrites = Rewrites::default();
            give_all(
                &romfs,
                &all,
                &mut rewrites,
                &drops,
                crate::progressive::Lines::default(),
                &on(ids),
            )
            .unwrap();
            let rel = flows::rel(starting::OPENING);
            rewrites
                .contains_key(&rel)
                .then(|| written(&rewrites, starting::OPENING))
        };
        // The grant's window, then the autosave.
        let granted_before_save = |flow: &Flowchart, item: &Name| {
            let save = only(flow, starting::AUTOSAVE).unwrap();
            let render = flows::render(flow);
            let window = &grant::GET_LINES[1];
            render.contains(item.get())
                && render.contains(window.as_str())
                && flow.links_to(save).iter().any(|l| match *l {
                    Link::Next(at) => match &flow.events[at as usize].kind {
                        EventKind::Action { params, .. } => {
                            param(params, field::MESSAGE_ID) == Some(window.as_str())
                        }
                        _ => false,
                    },
                    _ => false,
                })
        };
        let dealt = opening(&[DUNGEON_REWARDS, DOOR_OPEN], Some(item::HOOKSHOT)).unwrap();
        assert!(granted_before_save(&dealt, item::HOOKSHOT));
        assert!(!flows::render(&dealt).contains(item::MEDAL_LIGHT.get()));
        let undealt = opening(&[DOOR_OPEN], None).unwrap();
        assert!(granted_before_save(&undealt, item::MEDAL_LIGHT));
        let closed = opening(&[DUNGEON_REWARDS], Some(item::HOOKSHOT));
        assert!(closed.is_none_or(|f| !flows::render(&f).contains(item::HOOKSHOT.get())));
    }

    #[test]
    fn a_progressive_pocket_counts_the_start_grants() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let mut rewrites = Rewrites::default();
        let mut start = crate::starting::Start::default();
        start.add("hookshot", 1).unwrap();
        let pouch = crate::inventory::Pouch::load(&romfs).unwrap();
        starting::grant(&romfs, &mut rewrites, &start.grants(&pouch)).unwrap();
        let drops = [(rauru().hash(), Drop::from(item::HOOKSHOT.to_string()))].into();
        let on = on(&[DUNGEON_REWARDS, DOOR_OPEN]);
        give_all(
            &romfs,
            &all,
            &mut rewrites,
            &drops,
            crate::progressive::Lines::climbing(),
            &on,
        )
        .unwrap();
        let render = flows::render(&written(&rewrites, starting::OPENING));
        assert_eq!(render.matches(item::LONGSHOT.get()).count(), 1);
        assert!(!render.contains("EventQueryIsExistInPouch"));
    }
}
