//! King Zora's gift is granted in the melt. Once the Water Temple is beaten,
//! adult Zora's Domain loads its thawed map, where the King no longer
//! stands frozen. On the first talk after the temple he asks whether Link
//! holds the Zora Tunic, and without one sends him to the shop, whose
//! keeper hands over a tunic.
//!
//! A seed that deals the gift another item asks the gift's own Bool
//! instead: the melt sets it, the King sends Link to the shop only while
//! it is unset, and the shop hands over the dealt item and sets it. The
//! item comes from the melt or the shop, exactly once.

use super::*;

pub(super) const FLOW: &Name = flow::SAGE_OF_WATER;
/// The Bool the melt or the shop sets once the King's item is given.
pub(super) const GIVEN: &str = "Randomizer_KingZora";
/// The melt's grant.
const MELT: [Call; 1] = [get(flow::GET_IN_EVENT_NOT_TURN_POS_CAMERA)];
/// The King, thawed, who asks after the tunic.
const KING: &Name = actor::NPC_ZORA_KING_ZORA_ADULT;
/// The Zora shopkeeper, and her grant of the tunic.
const SHOP: &Name = actor::NPC_ZORA_011_SHOP_ADULT;
const SHOP_GET: [Call; 1] = [get(flow::GET_IN_EVENT_MASK)];

/// Deal `item` as the King's gift through the melt, the King and the shop.
pub(super) fn deal(romfs: &Romfs, rewrites: &mut Rewrites, item: &str) -> Result<()> {
    flows::edit(romfs, rewrites, FLOW, |f| set_own_bool(f, &MELT, GIVEN))?;
    flows::edit(romfs, rewrites, KING, |f| {
        ask_own_bool(f, actor::ARMOR_ADULT_ZORA, GIVEN)
    })?;
    flows::edit(romfs, rewrites, SHOP, |f| {
        // Setting the Bool makes the grant plain, so the rename finds it so.
        set_own_bool(f, &SHOP_GET, GIVEN)?;
        rename_calls(f, &[get(PLAIN)], actor::OBJ_ARMOR_ZORA, item)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Flows;

    /// `name` as vanilla has it and as `rewrites` writes it, rendered.
    fn rewritten(romfs: &Romfs, rewrites: &Rewrites, name: &str) -> (String, String) {
        let before = flows::load(romfs, name).unwrap().unwrap();
        let after = Flowchart::parse(&rewrites[&flows::rel(name)])
            .unwrap()
            .unwrap();
        (flows::render(&before), flows::render(&after))
    }

    /// The rewrites `give_all` makes with the King's gift dealt `item`.
    fn given(romfs: &Romfs, item: &str) -> Rewrites {
        let gift = GIFTS.iter().find(|g| g.flow == FLOW).unwrap();
        let drops = HashMap::from([(gift.hash(), Drop::from(item.to_owned()))]);
        let mut rewrites = Rewrites::default();
        give_all(romfs, &Flows::load(romfs).unwrap(), &mut rewrites, &drops).unwrap();
        rewrites
    }

    /// The one Bool set `after` adds, which must name `GIVEN` true.
    fn sets_given(after: &str) {
        let sets: Vec<_> = after.lines().filter(|l| l.contains(GIVEN)).collect();
        assert_eq!(sets.len(), 1, "{sets:#?}");
        assert!(sets[0].contains("SetGameDataBool") && sets[0].contains("Value=true"));
    }

    #[test]
    fn the_kings_gift_comes_from_the_melt_or_the_shop_once() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rewrites = given(&romfs, "Item_A");
        // The melt grants the dealt item, then sets the Bool.
        let (before, after) = rewritten(&romfs, &rewrites, FLOW);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(was.starts_with("#252 ") && was.contains("\"Obj_Armor_Zora\""));
        assert!(
            now.contains("\"Item_A\"") && !now.ends_with(" -> #133"),
            "{now}"
        );
        sets_given(&after);
        // The King asks the Bool where he asked for the tunic.
        let (before, after) = rewritten(&romfs, &rewrites, KING);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(was.contains("IsExistInPouch") && was.ends_with("? 0:#44 1:#138"));
        assert!(
            now.starts_with("#139 ") && now.contains("IsGameDataBool"),
            "{now}"
        );
        assert!(
            now.contains(GIVEN) && now.ends_with("? 0:#44 1:#138"),
            "{now}"
        );
        // The shop hands over the dealt item plainly, then sets the Bool.
        let (before, after) = rewritten(&romfs, &rewrites, SHOP);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(
            was.starts_with("#64 ") && was.contains(&format!("::{}(", flow::GET_IN_EVENT_MASK))
        );
        assert!(
            now.contains(&format!("::{PLAIN}(ActorName=\"Item_A\")")),
            "{now}"
        );
        assert!(!now.ends_with(" -> #58"), "{now}");
        sets_given(&after);
    }

    #[test]
    fn an_unshuffled_king_keeps_his_vanilla_flows() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rewrites = given(&romfs, actor::OBJ_ARMOR_ZORA);
        for name in [FLOW, KING, SHOP] {
            assert!(!rewrites.contains_key(&flows::rel(name)), "{name}");
        }
    }

    #[test]
    fn the_kings_bool_is_in_every_npc_gift_save() {
        let on = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(flags(&on(&[NPC_GIFTS])), [GIVEN]);
        assert!(!flags(&on(&[MERCHANTS])).contains(&GIVEN.to_string()));
    }
}
