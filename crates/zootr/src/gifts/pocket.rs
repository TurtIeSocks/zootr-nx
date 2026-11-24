//! Checks the opening gives, for scenes a new game's settings never let
//! play. Link's Pocket: with the Door of Time open, Rauru's scene never
//! plays, so the opening gives his check, as Ship does. Met Zelda gives
//! Zelda's letter, Impa's and Malon's checks the same way, and Gerudo
//! Fortress Freed the membership card's.

use super::*;
use crate::settings::{FORTRESS_FREED, MET_ZELDA};
use later::models;

/// The checks a new game with `on` gives in its opening.
pub fn pocket(on: &BTreeSet<String>) -> BTreeSet<u64> {
    GIFTS
        .iter()
        .filter(|g| g.pocket(on))
        .map(Gift::hash)
        .collect()
}

/// The checks a new game with `on` pins to their own item, by hash: the
/// story items its settings start Link with, so the pool holds no second.
pub fn pinned(on: &BTreeSet<String>) -> Vec<(u64, &'static str)> {
    let pin = |flow: &Name, setting: &str| {
        let gift = GIFTS.iter().find(|g| g.flow == flow)?;
        on.contains(setting).then(|| (gift.hash(), gift.item.get()))
    };
    [(ZELDA, MET_ZELDA), (CARD, FORTRESS_FREED)]
        .into_iter()
        .filter_map(|(flow, setting)| pin(flow, setting))
        .collect()
}

/// The scenes of Zelda's letter, Impa's lullaby, Malon's Weird Egg and the
/// Gerudo membership card.
const ZELDA: &Name = scene::HRL_CASTLE_0050;
const IMPA: &Name = scene::MELODY_GET_ZELDA;
const MALON: &Name = flow::GOTO_HYRULE_CASTLE;
const CARD: &Name = scene::GERUDO_0025;

impl Gift {
    /// Whether the opening gives it in a new game with `on`. A pinned
    /// check's and Impa's are given dealt or not, as their scenes never
    /// play (Impa's holds the lullaby with Songs off). Malon's only when
    /// dealt, as Met Zelda leaves no need for the egg.
    pub(super) fn pocket(&self, on: &BTreeSet<String>) -> bool {
        let met = on.contains(MET_ZELDA);
        match self.flow {
            f if f == RAURU => on.contains(DOOR_OPEN),
            f if f == ZELDA || f == IMPA => met,
            f if f == MALON => met && self.dealt(on),
            f if f == CARD => on.contains(FORTRESS_FREED),
            _ => false,
        }
    }

    /// Where its grant moves when its scene can't play or comes later: the
    /// opening for Link's Pocket, else its caller.
    pub(super) fn moved_to(&self, placement: Placement) -> Option<&'static str> {
        match placement {
            Placement::Pocket => Some(starting::OPENING.get()),
            Placement::Lost => self.caller(),
            Placement::Scene => (self.caller()).filter(|_| matches!(self.how, How::Later { .. })),
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
/// window. Fails when nothing leads to the autosave, as the grant would
/// never be reached.
pub(super) fn grant_before_save(flow: &mut Flowchart, flows: &Flows, item: &str) -> Result<()> {
    let save = only(flow, starting::AUTOSAVE)?;
    let callers = flow.links_to(save);
    let first = models(flows)?.push(flow, string(item), Some(save))?;
    relink(flow, save, callers, first)
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
        assert_eq!(pocket(&on(&[DOOR_OPEN])), [rauru.hash()].into());
        assert_eq!(pocket(&on(&[DUNGEON_REWARDS])), BTreeSet::new());
    }

    fn hash_of(flow: &Name) -> u64 {
        GIFTS.iter().find(|g| g.flow == flow).unwrap().hash()
    }

    #[test]
    fn met_zelda_pockets_her_letter_impa_and_the_dealt_malon() {
        let met = pocket(&on(&[MET_ZELDA]));
        assert_eq!(met, [ZELDA, IMPA].map(hash_of).into());
        let dealt = pocket(&on(&[MET_ZELDA, NPC_GIFTS, SONGS]));
        let all = [ZELDA, IMPA, MALON].map(hash_of).into();
        assert_eq!(dealt, all);
        assert_eq!(pocket(&on(&[NPC_GIFTS, SONGS])), BTreeSet::new());
        let letter = item::ZELDA_LETTER.get();
        assert_eq!(pinned(&on(&[MET_ZELDA])), [(hash_of(ZELDA), letter)]);
    }

    #[test]
    fn gerudo_fortress_freed_pockets_and_pins_the_card() {
        let freed = on(&[FORTRESS_FREED, NPC_GIFTS]);
        assert_eq!(pocket(&freed), [hash_of(CARD)].into());
        let card = item::TOKEN_GERUDO.get();
        assert_eq!(pinned(&freed), [(hash_of(CARD), card)]);
        assert!(pinned(&on(&[crate::settings::FORTRESS_FAST])).is_empty());
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
    fn met_zelda_grants_the_items_at_impa_s_and_malon_s_checks_before_the_save() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let drops = [
            (hash_of(IMPA), Drop::from(item::HOOKSHOT.to_string())),
            (hash_of(MALON), Drop::from(item::RANGE_BOW.to_string())),
            (hash_of(ZELDA), Drop::from(item::ZELDA_LETTER.to_string())),
        ]
        .into();
        let mut rewrites = Rewrites::default();
        let on = on(&[MET_ZELDA, NPC_GIFTS, SONGS]);
        let lines = crate::progressive::Lines::default();
        give_all(&romfs, &all, &mut rewrites, &drops, lines, &on).unwrap();
        let opening = flows::render(&written(&rewrites, starting::OPENING));
        for got in [item::HOOKSHOT, item::RANGE_BOW, item::ZELDA_LETTER] {
            assert!(opening.contains(got.get()), "{got}");
        }
        assert!(!opening.contains(item::WEIRD_EGG.get()));
        // With Songs off, Impa's check grants its own lullaby.
        let mut rewrites = Rewrites::default();
        let drops = [(hash_of(ZELDA), Drop::from(item::ZELDA_LETTER.to_string()))].into();
        let met = [MET_ZELDA.to_owned()].into();
        give_all(&romfs, &all, &mut rewrites, &drops, lines, &met).unwrap();
        let opening = flows::render(&written(&rewrites, starting::OPENING));
        assert!(opening.contains(item::MELODY_ZELDA_LULLABY.get()));
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

    /// The opening's grant is refused, not left where nothing reaches it,
    /// when no event leads to the autosave.
    #[test]
    fn a_grant_nothing_leads_to_is_refused() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, starting::OPENING).unwrap().unwrap();
        let save = only(&flow, starting::AUTOSAVE).unwrap();
        assert!(relink(&mut flow, save, Vec::new(), save).is_err());
    }
}
