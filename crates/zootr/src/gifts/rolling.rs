//! The rolling Goron as a child picks his bag by the one Link holds: the
//! Large bag over the Medium, the Medium over the small, and nothing
//! without a bag. A moved gift has one call, so he goes straight to it
//! with `one_of`.

use super::{Call, local};
use crate::known::{Name, actor, flow, item};

pub(super) const FLOW: &Name = actor::NPC_GORON_006_CHILD;
pub(super) const CALL: &[Call] = &[local(
    flow::NPC_COMMON_HAND_OVER_NPC_TO_PLAYER_NO_DISPLAY_BOMB_BAG,
)];
/// What he gives Link holding the small bag, the gift's vanilla item.
pub(super) const BAG: &Name = item::BOMB_BAG_MEDIUM;

#[cfg(test)]
mod tests {
    use ounce_formats::EventKind;

    use super::super::{asked, calls_to, giving, one_of};
    use super::*;
    use crate::flows::{self, param};
    use crate::known::flag;

    #[test]
    fn a_moved_bag_skips_every_question_about_link_s_bag() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, FLOW).unwrap().unwrap();
        let asks = asked(
            &flow,
            "Item_BombBag_",
            giving(&flow, BAG, &std::collections::BTreeSet::new()).unwrap(),
        );
        assert!(asks.len() >= 2, "{asks:?}");
        one_of(
            &mut flow,
            "Item_BombBag_",
            BAG,
            &std::collections::BTreeSet::new(),
        )
        .unwrap();
        let reached = flows::reachable(&flow, flag::BOMB_HITS_TALK).unwrap();
        for q in &asks {
            assert!(!reached.contains(q), "#{q} still asked");
        }
        let calls = calls_to(&flow, CALL);
        let bags: Vec<&str> = (calls.iter())
            .filter(|&i| reached.contains(i))
            .filter_map(|&i| match &flow.events[i as usize].kind {
                EventKind::SubFlow { params, .. } => param(params, flow::REVEIVE_ITEM),
                _ => None,
            })
            .collect();
        assert_eq!(bags, [BAG]);
    }
}
