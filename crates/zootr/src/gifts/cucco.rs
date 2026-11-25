//! Talon's cucco game. Its check is the first win's plain hand-over, a new
//! bottle of milk, which asks for no bottle. Every empty-bottle ask and
//! bottle fill in the flow belongs to the replays after a clear (milk into
//! an empty bottle) or the 30-rupee milk sale. Those stay the game's milk
//! whatever the check holds, so a dealt check renames the hand-over and
//! nothing else. Rewriting the sale's fill into a hand-over (#454) left
//! every talk with Talon dead.

use crate::known::{Name, flow, item};

use super::{Call, HAND_OVER};

pub(super) const FLOW: &Name = flow::MINIGAME_FIND_OUT_CUCCO;
/// The plain hand-over, and the first win's one call to it.
pub(super) const CALL: &[Call] = &[HAND_OVER];
/// What the first win gives, the gift's vanilla item.
pub(super) const MILK: &Name = item::BOTTLE_MILK;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ounce_formats::{EventKind, Flowchart};

    use super::super::{GIFTS, ITEM_PARAMS, calls_to, get, give_all};
    use super::*;
    use crate::flows::{self, param};
    use crate::known::actor;

    const HEART: &Name = actor::OBJ_HEART_PIECE;

    /// The item each call to `call` names, in event order.
    fn named(flow: &Flowchart, call: &[Call]) -> Vec<String> {
        let names = |i: u16| match &flow.events[i as usize].kind {
            EventKind::SubFlow { params, .. } => ITEM_PARAMS
                .iter()
                .filter_map(|k| param(params, k))
                .map(str::to_owned)
                .collect(),
            _ => vec![],
        };
        calls_to(flow, call).into_iter().flat_map(names).collect()
    }

    /// The flow as a seed writes it with the check holding `item`, if it
    /// writes it at all.
    fn dealt(romfs: &ounce_formats::Romfs, item: &str) -> Option<Flowchart> {
        let gift = GIFTS.iter().find(|g| g.flow == FLOW).unwrap();
        let flows = crate::events::Flows::load(romfs).unwrap();
        let drops = HashMap::from([(gift.hash(), crate::checks::Drop::from(item.to_owned()))]);
        let mut rewrites = flows::Rewrites::default();
        give_all(
            romfs,
            &flows,
            &mut rewrites,
            &drops,
            crate::progressive::Lines::default(),
            &Default::default(),
        )
        .unwrap();
        let bytes = rewrites.get(&flows::rel(FLOW))?;
        Some(Flowchart::parse(bytes).unwrap().unwrap())
    }

    #[test]
    fn a_dealt_check_renames_the_first_win_and_leaves_the_milk_sale_and_replays_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = flows::load(&romfs, FLOW).unwrap().unwrap();
        assert!(
            dealt(&romfs, MILK).is_none(),
            "a vanilla check writes nothing"
        );
        let flow = dealt(&romfs, HEART).unwrap();
        assert_eq!(flow.events.len(), vanilla.events.len());
        assert_eq!(named(&flow, CALL), [HEART.get()], "the first win");
        // The sale's fill and the replays' hand-over into a bottle.
        let fills = [
            get(flow::GET_IN_EVENT_IN_BOTTLE),
            super::super::call(
                flow::NPC_COMMON,
                flow::NPC_COMMON_HAND_OVER_NPC_TO_PLAYER_EMPTY_BOTTLE,
            ),
        ];
        assert_eq!(calls_to(&flow, &fills).len(), 2);
        // Every event but the first win's hand-over is the game's own.
        let changed: Vec<usize> = (0..flow.events.len())
            .filter(|&i| flow.events[i] != vanilla.events[i])
            .collect();
        assert_eq!(
            changed,
            calls_to(&flow, CALL)
                .iter()
                .map(|&i| i as usize)
                .collect::<Vec<_>>()
        );
    }
}
