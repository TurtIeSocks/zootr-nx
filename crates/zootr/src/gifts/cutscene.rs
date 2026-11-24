//! Gifts in a cutscene: the scene is cut down to the grant, its song and its
//! saves, and a cut cutscene leaves Link where he was.

use super::*;

/// Moves Link to where a cutscene left him.
const WARP: (&Name, &str) = (actor::PLAYER_EVENT, "EventPlayerWarpTo");

/// Start `entry` past a warp it opens with, since a cut cutscene leaves
/// Link where he was. Zelda's would put him back on the drawbridge.
pub(super) fn stay(flow: &mut Flowchart, entry: &str) -> Result<()> {
    let i = flow.entry_points.iter().position(|e| e.name == entry);
    let i = i.with_context(|| format!("no entry point {entry}"))?;
    let Some(first) = flow.entry_points[i].main else {
        return Ok(());
    };
    let EventKind::Action {
        actor,
        action,
        next,
        ..
    } = flow.events[first as usize].kind
    else {
        return Ok(());
    };
    let a = flow.actor_of(actor);
    match a.name == WARP.0 && a.actions[action as usize].starts_with(WARP.1) {
        true => flow.link(Link::Entry(i), next),
        false => Ok(()),
    }
}

/// Open `flow` straight onto its grant, then its "You got" window, past
/// the cutscene around them. Both stay single, so `in_place` still swaps
/// the item.
pub fn straight(flow: &mut Flowchart) -> Result<()> {
    let grant = only(flow, GRANT)?;
    let window = only_call(flow, WINDOW, None)?;
    flow.link(Link::Next(grant), Some(window))?;
    flow.link(Link::Next(window), None)?;
    let entry = flow.entry_points.iter().position(|e| e.name == ENTRY);
    flow.link(Link::Entry(entry.context("no entry point")?), Some(grant))
}

/// The events of `flow`'s cutscene to keep when it's cut: whatever changes
/// the save, in flow order, past the grants of `items`, any call that names
/// one, and the song lesson. Bolero's scene also sets its song's `IsGet`
/// flag by hand.
pub(crate) fn kept(flow: &Flowchart, flows: &Flows, items: &[&str]) -> Result<Vec<u16>> {
    let got: Vec<String> = items.iter().map(|i| format!("{IS_GET}{i}")).collect();
    let gives = |e: &EventKind| match e {
        EventKind::Action { params, .. } => {
            param(params, ACTOR).is_some_and(|a| items.contains(&a))
        }
        EventKind::SubFlow { params, .. } => names_item(params, items),
        _ => false,
    };
    let sets_got = |e: &EventKind| match e {
        EventKind::Action { params, .. } => {
            param(params, GAME_DATA).is_some_and(|n| got.iter().any(|g| g == n))
        }
        _ => false,
    };
    let grants = running(flow, GRANT);
    let mut kept = Vec::new();
    for e in flows::reachable(flow, ENTRY)? {
        let kind = &flow.events[e as usize].kind;
        let skip = match kind {
            EventKind::Action { .. } => (grants.contains(&e) && gives(kind)) || sets_got(kind),
            EventKind::SubFlow { flowchart, .. } | EventKind::SubFlowSwitch { flowchart, .. } => {
                is_local(flow, flowchart) || flowchart == LESSON || gives(kind)
            }
            _ => true,
        };
        if !skip && flows.changes_save(flow, e) {
            ensure!(
                matches!(kind, EventKind::Action { .. } | EventKind::SubFlow { .. }),
                "kept event {e} branches"
            );
            kept.push(e);
        }
    }
    Ok(kept)
}

/// Point the entry point at the reward for `item`, then `then` if any,
/// then copies of the cutscene's save changes. Returns the flow and entry
/// point it hands back to, if it ends in a call to another flow, and the
/// reward giving `item`.
pub(super) fn cut(
    flow: &mut Flowchart,
    flows: &Flows,
    gift: &Name,
    item: &str,
    then: Option<&Name>,
) -> Result<(Option<(String, String)>, u16)> {
    let items: Vec<&str> = [Some(gift), then]
        .into_iter()
        .flatten()
        .map(|n| &**n)
        .collect();
    let kept = kept(flow, flows, &items)?;
    let back = match kept.last().map(|&e| &flow.events[e as usize].kind) {
        Some(EventKind::SubFlow {
            flowchart,
            entry_point,
            ..
        }) if !is_local(flow, flowchart) => Some((flowchart.clone(), entry_point.clone())),
        _ => None,
    };
    let reward = only_call(flow, REWARD, Some(&**then.unwrap_or(gift)))?;
    let mut given = flow.events[reward as usize].kind.clone();
    flows::set(params_mut(&mut given), NAME, string(item))?;
    let then = then.map(|_| flow.events[reward as usize].kind.clone());
    let chain: Vec<EventKind> = [given]
        .into_iter()
        .chain(then)
        .chain(kept.iter().map(|&e| flow.events[e as usize].kind.clone()))
        .collect();
    let mut first = None;
    for mut kind in chain.into_iter().rev() {
        if let EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } = &mut kind {
            *next = first;
        }
        first = Some(flow.push(kind));
    }
    let entry = flow.entry_points.iter().position(|e| e.name == ENTRY);
    flow.link(Link::Entry(entry.context("no entry point")?), first)?;
    // Pushed last, so the reward is the first event.
    Ok((back, first.context("nothing given")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::*;
    use crate::known::{field, flag, flow, item, scene};

    #[test]
    fn a_cut_gives_the_item_the_song_and_the_saves_in_order() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (all, zelda) = (Flows::load(&romfs).unwrap(), &GIFTS[1]);
        let How::Cut { then } = zelda.how else {
            panic!("Zelda's lesson is cut")
        };
        let mut flow = flows::load(&romfs, zelda.flow).unwrap().unwrap();
        let (back, _) = cut(&mut flow, &all, zelda.item, "Item_A", then).unwrap();
        assert_eq!(
            back,
            Some((
                flow::DOOR_OF_TIME.to_string(),
                scene::MELODY_GET_TIME_AFTER.to_string()
            ))
        );
        // The song's own gift renames the copied grant.
        let song = GIFTS
            .iter()
            .find(|g| g.flow == zelda.flow && g.item == then.unwrap());
        let How::Calls { call, .. } = song.unwrap().how else {
            panic!("the Song of Time is a call gift")
        };
        rename_calls(&mut flow, call, then.unwrap(), "Item_B").unwrap();
        let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        let ran = chain(&flow);
        let ran: Vec<_> = ran.iter().map(|l| strip(l)).collect();
        assert_eq!(
            ran,
            [
                &format!(
                    r#"call {}::{}({}="Item_A")"#,
                    flow::GET_POUCH_CONTENT,
                    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
                    flag::ACTOR_NAME
                ),
                &format!(
                    r#"call {}::{}({}="Item_B")"#,
                    flow::GET_POUCH_CONTENT,
                    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
                    flag::ACTOR_NAME
                ),
                &format!(
                    r#"{}.{}({}="{}" Index=-1 Value=true)"#,
                    field::GAME_DATA_ACTOR,
                    flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
                    field::GAME_DATA_NAME,
                    flag::MELODY_GET_TIME_END_CONVERSATION_WITH_ZELDA
                ),
                &format!(
                    "call {}::{}()",
                    flow::DOOR_OF_TIME,
                    scene::MELODY_GET_TIME_AFTER
                ),
            ]
        );
    }

    #[test]
    fn after_zeldas_cut_link_stays_in_the_moat() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, flow::DOOR_OF_TIME).unwrap().unwrap();
        let first = |f: &Flowchart| {
            let e = f
                .entry_points
                .iter()
                .find(|e| e.name == scene::MELODY_GET_TIME_AFTER);
            let at = e.unwrap().main.unwrap();
            let lines = flows::render(f);
            let line = lines.lines().find(|l| l.starts_with(&format!("#{at} ")));
            line.unwrap().to_owned()
        };
        assert!(first(&flow).contains(WARP.1));
        stay(&mut flow, scene::MELODY_GET_TIME_AFTER).unwrap();
        let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert!(!first(&flow).contains(WARP.1), "{}", first(&flow));
    }

    #[test]
    fn each_song_opens_on_its_new_item_with_no_lesson_or_song() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let cut_songs = GIFTS.iter().filter(|g| matches!(g.how, How::Cut { .. }));
        for gift in cut_songs.filter(|g| g.setting == Some(SONGS)) {
            let mut flow = flows::load(&romfs, gift.flow).unwrap().unwrap();
            cut(&mut flow, &all, gift.item, "Item_A", None).unwrap();
            let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
            let ran = chain(&flow);
            assert!(
                ran[0].contains(&format!(r#"{}="Item_A""#, flag::ACTOR_NAME)),
                "{}: {ran:?}",
                gift.flow
            );
            for line in &ran {
                assert!(!line.contains(&**gift.item), "{}: {line}", gift.flow);
                assert!(!line.contains(&**LESSON), "{}: {line}", gift.flow);
            }
        }
    }

    #[test]
    fn saria_opens_straight_onto_her_gift() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, scene::KOKIRI_0155).unwrap().unwrap();
        straight(&mut flow).unwrap();
        in_place(&mut flow, item::BOOMERANG).unwrap();
        let chain = chain(&flow);
        let chain: Vec<&str> = chain.iter().map(|l| strip(l)).collect();
        assert_eq!(chain.len(), 2, "{chain:?}");
        assert!(chain[0].starts_with(&format!(
            "{}.{}",
            field::POUCH_SYSTEM_ACTOR,
            flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT
        )));
        assert!(chain[0].contains(&format!(r#"Actor="{}""#, item::BOOMERANG)));
        assert_eq!(
            chain[1],
            format!(
                r#"call {}::{}({}="{}")"#,
                flow::GET_POUCH_CONTENT,
                flow::DISPLAY_GET_BY_ACTOR_NAME,
                flag::ACTOR_NAME,
                item::BOOMERANG
            )
        );
    }

    const STONE: &Name = flow::GET_STONE_OF_FIRE;

    const BRACELET: &Name = item::GAUNTLET_GORON;

    fn darunia_asks_for_the_tune_then_gives_the_bracelet_and_opens_dodongos(
        skips: &crate::cutscenes::Skips,
        item: &str,
        dealt: Option<(&Name, &Name)>,
    ) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, _) = seed(&romfs, skips, dealt);
        let flow = written(&rewrites, STONE);
        let ran = run(&flow, field::GET_STONE_OF_FIRE_PLAY_OCARINA);
        let at = |what: &dyn Fn(&str) -> bool| ran.iter().position(|l| what(l));
        // The prompt stays, and so does the wrong tune's scene.
        assert!(
            at(&|l| l.contains(&format!(
                "{}::{}",
                scene::SY_OCARINA,
                field::PLAY_OCARINA_FREE
            )))
            .is_some(),
            "{ran:?}"
        );
        assert!(
            at(&|l| l.contains(&format!(r#"{}="{}""#, flow::EVENT_NAME, scene::GORON_0028)))
                .is_some()
        );
        // The dance and his gift are flags, and no scene plays.
        for scene in [scene::GORON_0025, scene::GORON_0030] {
            assert!(
                !ran.iter()
                    .any(|l| l.contains(&format!(r#"{}="{scene}""#, flow::EVENT_NAME)))
            );
            assert!(at(&|l| is_set(l, scene)).is_some(), "{scene}");
        }
        // The bracelet, then the step, right after the gift scene's flag.
        let gift = at(&|l| is_set(l, scene::GORON_0030)).unwrap();
        assert_eq!(ran[gift + 1], FADE, "{ran:?}");
        assert_eq!(ran[gift + 2..gift + 5], granted(item), "{ran:?}");
        assert!(ran[gift + 5].contains(&format!(
            r#"{}="{}""#,
            field::CHALLENGE_NAME,
            flow::GET_STONE_OF_FIRE
        )));
        assert!(ran[gift + 5].contains(&format!(
            r#"{}="{}""#,
            field::STEP_NAME,
            flag::DODONGO_CAVERN_OPNE
        )));
        assert_eq!(grants(&flow, item), 1);
        if item != BRACELET {
            assert_eq!(grants(&flow, BRACELET), 0);
        }
        // The dance's flag comes first, after the tune.
        let dance = at(&|l| is_set(l, scene::GORON_0025)).unwrap();
        let tune = at(&|l| {
            l.contains(&format!(
                "{}::{}",
                scene::SY_OCARINA,
                field::PLAY_OCARINA_FREE
            ))
        })
        .unwrap();
        assert!(tune < dance && dance < gift);
    }

    #[test]
    fn darunias_dance_is_cut_with_every_area_skipped() {
        darunia_asks_for_the_tune_then_gives_the_bracelet_and_opens_dodongos(
            &skip_all(),
            BRACELET,
            None,
        );
    }

    #[test]
    fn darunias_dance_is_cut_with_only_goron_city_skipped_and_his_gift_dealt() {
        let mut skips = crate::cutscenes::Skips::default();
        skips.areas.insert("goron");
        darunia_asks_for_the_tune_then_gives_the_bracelet_and_opens_dodongos(
            &skips,
            item::BOTTLE,
            Some((scene::GORON_0030, item::BOTTLE)),
        );
    }

    #[test]
    fn without_the_goron_skip_darunias_dance_plays_and_the_gift_stays_inside_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut skips = skip_all();
        skips.areas.remove("goron");
        let (rewrites, _) = seed(&romfs, &skips, None);
        // Other areas' skips write the flow, and it still plays both scenes
        // and grants nothing itself.
        let flow = match rewrites.get(&flows::rel(STONE)) {
            Some(bytes) => Flowchart::parse(bytes).unwrap().unwrap(),
            None => flows::load(&romfs, STONE).unwrap().unwrap(),
        };
        for scene in [scene::GORON_0025, scene::GORON_0030] {
            assert_eq!(scene_marks(&flow, scene).0.len(), 1, "{scene}");
            assert!(scene_marks(&flow, scene).1.is_empty(), "{scene}");
        }
        assert_eq!(grants(&flow, BRACELET), 0);
        assert!(!rewrites.contains_key(&flows::rel(scene::GORON_0030)));
    }
}
