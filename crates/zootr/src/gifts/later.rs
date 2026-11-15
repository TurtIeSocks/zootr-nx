//! Gifts whose scene can't hold the grant: the scene's calls are silenced
//! and the flow that starts it grants the item right after.

use super::*;

/// An event that does nothing and ends a branch, which the game's cues
/// already run beside their real work.
const NOTHING: (&Name, &Name) = (
    field::DEVELOP_EVENT_SYSTEM_ACTOR,
    flow::EVENT_DEMO_PRODUCT_SUPPORT_COMMENT,
);

/// Empty every call to `call` naming `item`, at least one, so the scene
/// holding them grants nothing. Each becomes the flow's `NOTHING`, since a
/// fork branch can't be unlinked.
pub(super) fn silence_calls(flow: &mut Flowchart, call: &[Call], item: &str) -> Result<()> {
    let named = |kind: &EventKind| match kind {
        EventKind::SubFlow { params, .. } => {
            ITEM_PARAMS.iter().any(|k| param(params, k) == Some(item))
        }
        _ => false,
    };
    let silenced: Vec<usize> = calls_to(flow, call)?
        .into_iter()
        .filter(|&i| named(&flow.events[i].kind))
        .collect();
    ensure!(
        !silenced.is_empty(),
        "{} never calls {} with {item}",
        flow.name,
        said(call)
    );
    let nothing = (0u16..).zip(&flow.events).find(|(_, e)| match &e.kind {
        EventKind::Action {
            actor,
            action,
            next: None,
            ..
        } => {
            let a = flow.actor_of(*actor);
            a.name.as_str() == NOTHING.0 && a.actions[*action as usize].as_str() == NOTHING.1
        }
        _ => false,
    });
    let nothing =
        nothing.with_context(|| format!("{} has no {} to stand in", flow.name, NOTHING.1))?;
    let nothing = nothing.1.kind.clone();
    for i in silenced {
        flow.events[i].kind = nothing.clone();
    }
    Ok(())
}

/// The events in `flow` where the cutscene `scene` plays: its transitions,
/// and the sets of its played flag a cutscene skip swapped them for.
pub(crate) fn scene_marks(flow: &Flowchart, scene: &str) -> (Vec<u16>, Vec<u16>) {
    let flag = format!("{PLAYED}.{scene}");
    let plays = (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| transition(flow, &e.kind) == Some(scene))
        .map(|(i, _)| i)
        .collect();
    let swapped = running(flow, SET_BOOL)
        .into_iter()
        .filter(|&i| {
            matches!(&flow.events[i as usize].kind,
                EventKind::Action { params, .. } if param(params, GAME_DATA) == Some(&flag))
        })
        .collect();
    (plays, swapped)
}

/// Grant `item` right after each of `flow`'s transitions to the cutscene
/// `scene`, or the sets of its played flag a skip swapped them for, before
/// whatever follows. The grant is inline: a caller's event never binds
/// `GetPouchContent`. A set gets a fade-in first: the scenes faded back in
/// as they ended, and the caller may have faded out before them.
pub(super) fn grant_after(
    flow: &mut Flowchart,
    flows: &Flows,
    scene: &str,
    item: &str,
) -> Result<()> {
    let (plays, swapped) = scene_marks(flow, scene);
    ensure!(
        !plays.is_empty() || !swapped.is_empty(),
        "{} never marks {scene}",
        flow.name
    );
    let chart = |name: &str| flows.0.get(name).with_context(|| format!("no {name} flow"));
    let inline = grant::Models::of(chart(starting::TEMPLATE)?, chart(grant::POUCH)?);
    for (at, fade) in plays
        .into_iter()
        .map(|at| (at, false))
        .chain(swapped.into_iter().map(|at| (at, true)))
    {
        let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
            unreachable!("transition found an action");
        };
        let mut first = inline.push(flow, string(item), next)?;
        if fade {
            first = flows::fade_in(flow, Some(first));
        }
        flow.link(Link::Next(at), Some(first))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gifts::tests::*;
    use crate::known::{actor, field, flag, flow, item, scene};

    #[test]
    fn zeldas_letter_is_granted_after_her_scene_and_never_inside_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let zelda = GIFTS
            .iter()
            .find(|g| g.flow == scene::HRL_CASTLE_0050)
            .unwrap();
        let How::Later { caller, .. } = zelda.how else {
            panic!("Zelda's letter is granted by its caller")
        };
        let mut rewrites = Rewrites::new();
        give(&romfs, &all, &mut rewrites, zelda, item::MEDAL_FIRE).unwrap();
        let parse = |name: &str| {
            let bytes = &rewrites[&flows::rel(name)];
            Flowchart::parse(bytes).unwrap().unwrap()
        };
        let scene = flows::render(&parse(zelda.flow));
        // The letter prop still acts in the cue, but nothing grants.
        for gone in [
            "Zeldaletter",
            &format!("{}=\"{}\"", flag::ACTOR_NAME, item::ZELDA_LETTER),
            item::MEDAL_FIRE,
        ] {
            assert!(!scene.contains(gone), "the scene still has {gone}");
        }
        // The scene's other cue events are untouched but for that one.
        let before = flows::render(&flows::load(&romfs, zelda.flow).unwrap().unwrap());
        let events = |r: &str| -> Vec<String> {
            r.lines()
                .filter(|l| l.starts_with('#'))
                .map(str::to_owned)
                .collect()
        };
        let (before, after) = (events(&before), events(&scene));
        let changed = before.iter().zip(&after).filter(|(a, b)| a != b);
        assert_eq!(changed.count(), 1);

        let flow = parse(caller);
        let rendered = flows::render(&flow);
        let grant = &granting(item::MEDAL_FIRE);
        assert_eq!(rendered.matches(grant).count(), 1, "granted once");
        let lines: Vec<&str> = rendered.lines().collect();
        let at = lines.iter().position(|l| l.contains(grant)).unwrap();
        let before_it = lines.iter().find(|l| {
            l.contains(&format!(
                r#"{}="{}""#,
                flow::EVENT_NAME,
                scene::HRL_CASTLE_0050
            ))
        });
        let (hop, to) = lines[at].split_once(" -> ").unwrap();
        assert!(hop.contains(&**item::MEDAL_FIRE));
        let hop = before_it.unwrap().rsplit_once(" -> ").unwrap().1;
        assert!(
            lines[at].starts_with(&format!("{} ", hop)),
            "{hop}: {}",
            lines[at]
        );
        let window = flows::render(&flow);
        let next = window.lines().find(|l| l.starts_with(&format!("{to} ")));
        assert!(
            next.unwrap().contains(grant::GET_LINES[0].as_str()),
            "the get window follows: {to}"
        );
    }

    #[test]
    fn only_the_castle_flow_starts_zeldas_scene_and_no_generic_skip_takes_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = crate::cutscenes::Catalogue::load(&romfs).unwrap();
        let scene = catalogue
            .0
            .iter()
            .find(|c| c.name == scene::HRL_CASTLE_0050);
        let scene = scene.unwrap();
        assert_eq!(
            scene.flows,
            BTreeSet::from([flow::GOTO_HYRULE_CASTLE.to_string()])
        );
        assert!(!scene.tier.skippable());
    }

    const MEET: &Name = flag::GOTO_HYRULE_CASTLE_MEET_ZELDA;

    const LETTER: &Name = item::ZELDA_LETTER;

    fn zeldas_chain_is_flags_and_one_grant(item: &str, dealt: Option<(&Name, &Name)>) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, flags) = seed(&romfs, &skip_all(), dealt);
        let flow = written(&rewrites, flow::GOTO_HYRULE_CASTLE);
        let ran = run(&flow, MEET);
        for z in crate::cutscenes::ZELDA {
            assert!(
                !ran.iter()
                    .any(|l| l.contains(&format!(r#"{}="{z}""#, flow::EVENT_NAME)))
            );
            assert!(
                !flags.contains(&format!("{PLAYED}.{z}=true")),
                "{z} at new game"
            );
        }
        // The five flags, in the order the scenes played, with the fade-in and
        // then the grant right after the fourth, and nothing else granting.
        let at: Vec<usize> = crate::cutscenes::ZELDA
            .iter()
            .map(|z| ran.iter().position(|l| is_set(l, z)).unwrap())
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{at:?}");
        let grant = granting(item);
        assert_eq!(ran.iter().filter(|l| **l == grant).count(), 1, "{item}");
        assert_eq!(ran[at[3] + 1], FADE);
        assert_eq!(ran[at[3] + 2..at[3] + 5], granted(item));
        assert_eq!(flows::render(&flow).matches(&grant).count(), 1);
        // The rest of the scene stays: her talk flag, her warp, both quest
        // steps and the fades.
        let has = |needles: &[&str]| ran.iter().any(|l| needles.iter().all(|n| l.contains(n)));
        assert!(has(&[&format!(
            "{}.{}",
            actor::NPC_HYLIA_ZELDA_CHILD,
            field::EVENT_TRIGGER_SET_HAVE_TALKED
        )]));
        assert!(has(&[&format!(
            "{}.{}",
            actor::NPC_HYLIA_ZELDA_CHILD,
            field::EVENT_AI_SCHEDULE_WARP_TO_ANCHOR
        )]));
        assert!(has(&[
            &format!(
                r#"{}="{}""#,
                field::CHALLENGE_NAME,
                flow::GOTO_HYRULE_CASTLE
            ),
            &format!(r#"{}="Complete""#, field::STEP_NAME)
        ]));
        assert!(has(&[
            &format!(r#"{}="{}""#, field::CHALLENGE_NAME, flow::DOOR_OF_TIME),
            &format!(r#"{}="Step01""#, field::STEP_NAME)
        ]));
        assert!(has(&[flow::EVENT_EVENT_FADE_OUT]) && has(&[flow::EVENT_EVENT_FADE_IN]));
        // 0050's own scene is never reached, so it is left alone.
        assert!(!rewrites.contains_key(&flows::rel(scene::HRL_CASTLE_0050)));
    }

    #[test]
    fn skipped_zelda_is_five_flags_and_the_shuffled_letter_once() {
        zeldas_chain_is_flags_and_one_grant(
            item::MEDAL_FIRE,
            Some((scene::HRL_CASTLE_0050, item::MEDAL_FIRE)),
        );
    }

    #[test]
    fn skipped_zelda_without_npc_gifts_still_grants_the_letter_once() {
        zeldas_chain_is_flags_and_one_grant(LETTER, None);
    }

    #[test]
    fn without_the_castle_skip_zeldas_chain_is_vanilla_but_for_the_gift() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut skips = skip_all();
        skips.areas.remove("castle");
        let dealt = Some((scene::HRL_CASTLE_0050, item::MEDAL_FIRE));
        let (rewrites, flags) = seed(&romfs, &skips, dealt);
        let flow = written(&rewrites, flow::GOTO_HYRULE_CASTLE);
        let ran = run(&flow, MEET);
        let vanilla = flows::load(&romfs, flow::GOTO_HYRULE_CASTLE)
            .unwrap()
            .unwrap();
        let before = run(&vanilla, MEET);
        for z in crate::cutscenes::ZELDA {
            let scene = format!(r#"{}="{z}""#, flow::EVENT_NAME);
            assert!(ran.iter().any(|l| l.contains(&scene)), "{z}");
            assert!(!ran.iter().any(|l| is_set(l, z)), "{z}");
        }
        // Only the one grant and its window, after 0050, differ.
        let at = ran
            .iter()
            .position(|l| {
                l.contains(&format!(
                    r#"{}="{}""#,
                    flow::EVENT_NAME,
                    scene::HRL_CASTLE_0050
                ))
            })
            .unwrap();
        assert_eq!(ran[at + 1..at + 4], granted(item::MEDAL_FIRE));
        let without = [&ran[..at + 1], &ran[at + 4..]].concat();
        assert_eq!(without, before);
        // Nothing dealt and no skip at all: the flow isn't written.
        let (plain, _) = seed(&romfs, &Default::default(), None);
        assert!(!plain.contains_key(&flows::rel(flow::GOTO_HYRULE_CASTLE)));
        drop(flags);
    }

    const IMPA: &Name = field::STEP01_NPC_HYLIA_IMPA_CHILD_TALK;

    const SEND_OFF: &Name = field::DOOR_OF_TIME_SEND_OFF;

    const LULLABY: &Name = item::MELODY_ZELDA_LULLABY;

    fn impa_is_a_flag_a_grant_and_the_banc_change(item: &str, dealt: Option<(&Name, &Name)>) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, flags) = seed(&romfs, &skip_all(), dealt);
        let flow = written(&rewrites, flow::DOOR_OF_TIME);
        let vanilla = flows::load(&romfs, flow::DOOR_OF_TIME).unwrap().unwrap();
        let banc = run(
            &vanilla,
            scene::MELODY_GET_ZELDA_AFTER_NPC_HYLIA_IMPA_CHILD_TALK,
        )
        .into_iter()
        .find(|l| l.contains(&**flow::EVENT_RESERVE_BANC_CHANGE))
        .unwrap();
        let ran = run(&flow, IMPA);
        assert!(
            !ran.iter().any(|l| l.contains(&**flow::EVENT_TRANSITION)),
            "{ran:?}"
        );
        let grant = granting(item);
        let tail = &ran[ran.len() - 6..];
        assert!(is_set(&tail[0], scene::MELODY_GET_ZELDA), "{tail:?}");
        assert_eq!(tail[1], FADE);
        assert_eq!(tail[2..5], granted(item));
        assert_eq!(tail[5], banc);
        assert_eq!(
            flows::render(&flow).matches(&grant).count(),
            1,
            "granted once"
        );
        for scene in crate::cutscenes::IMPA {
            assert!(
                !flags.contains(&format!("{PLAYED}.{scene}=true")),
                "{scene}"
            );
        }
        // The escort's transition is its flag, and the steps after it run.
        let escort = run(&flow, SEND_OFF);
        assert!(
            !escort.iter().any(|l| l.contains(&**flow::EVENT_TRANSITION)),
            "{escort:?}"
        );
        let at = escort
            .iter()
            .position(|l| is_set(l, scene::HRL_FIELD_0020))
            .unwrap();
        assert!(
            escort[at..]
                .iter()
                .any(|l| l.contains(&format!(r#"{}="Step02""#, field::STEP_NAME)))
        );
        assert!(escort.iter().any(|l| l.contains(&format!(
            r#"{}="{}""#,
            field::CHALLENGE_NAME,
            flow::GET_STONE_OF_FIRE
        ))));
        // The scene itself never plays, so nothing in it grants.
        if let Some(scene) = rewrites.get(&flows::rel(scene::MELODY_GET_ZELDA)) {
            let scene = flows::render(&Flowchart::parse(scene).unwrap().unwrap());
            assert!(!scene.contains(&**flow::GET_POUCH_CONTENT_BY_ACTOR_NAME));
        }
    }

    #[test]
    fn skipped_impa_sets_her_flag_grants_the_shuffled_song_once_and_leaves() {
        impa_is_a_flag_a_grant_and_the_banc_change(
            item::MEDAL_WATER,
            Some((scene::MELODY_GET_ZELDA, item::MEDAL_WATER)),
        );
    }

    #[test]
    fn skipped_impa_without_shuffled_songs_grants_the_lullaby_once() {
        impa_is_a_flag_a_grant_and_the_banc_change(LULLABY, None);
    }

    #[test]
    fn without_the_songs_skip_impa_still_plays_her_scene() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut skips = skip_all();
        skips.areas.remove("songs");
        let (rewrites, _) = seed(&romfs, &skips, None);
        let flow = written(&rewrites, flow::DOOR_OF_TIME);
        let ran = run(&flow, IMPA);
        assert!(ran.iter().any(|l| l.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::MELODY_GET_ZELDA
        ))));
        assert!(!ran.iter().any(|l| is_set(l, scene::MELODY_GET_ZELDA)));
        // Its escort is the hyrule-field setting's, which still plays it.
        let escort = run(&flow, SEND_OFF);
        assert!(escort.iter().any(|l| l.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::HRL_FIELD_0020
        ))));
        assert!(rewrites.contains_key(&flows::rel(scene::MELODY_GET_ZELDA)));
    }

    fn every_cut_scene_with_a_grant_grants_once_where_it_played(dealt: bool) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let swapped = SKIPPED_IN
            .iter()
            .filter(|(scene, _)| !UNBOUND.contains(scene));
        for &(scene, caller) in swapped {
            let gift = GIFTS.iter().find(|g| g.flow == scene).unwrap();
            let item = if dealt { item::BOTTLE } else { gift.item };
            let drop = dealt.then_some((scene, item));
            let (rewrites, _) = seed(&romfs, &skip_all(), drop);
            let vanilla = flows::load(&romfs, caller).unwrap().unwrap();
            let sites = scene_marks(&vanilla, scene).0.len();
            assert!(sites > 0, "{caller} never played {scene}");
            let flow = written(&rewrites, caller);
            // Once at each place the scene played, and not the vanilla item
            // as well when another was dealt.
            assert_eq!(grants(&flow, item), sites, "{scene} in {caller}");
            if dealt {
                assert_eq!(grants(&flow, gift.item), 0, "{scene} in {caller}");
            }
            let (plays, sets) = scene_marks(&flow, scene);
            assert!(plays.is_empty());
            assert_eq!(sets.len(), sites);
            // Each set is followed by a fade-in, its grant and the get
            // window, before anything the caller does next.
            let after = |at: u16| {
                let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
                    unreachable!();
                };
                next.unwrap()
            };
            for at in sets {
                let mut shown = Vec::new();
                let mut e = at;
                for _ in 0..4 {
                    e = after(e);
                    shown.push(event(&flow, e));
                }
                assert_eq!(shown[0], FADE, "{scene} in {caller}");
                assert_eq!(shown[1..], granted(item), "{scene} in {caller}");
            }
            // The scene's own flow is never written to grant it again.
            if let Some(bytes) = rewrites.get(&flows::rel(scene)) {
                let own = Flowchart::parse(bytes).unwrap().unwrap();
                assert_eq!(grants(&own, item), 0, "{scene}");
            }
        }
    }

    /// A grant a skip moves into a caller opens its window on a screen the
    /// caller may have faded out, so each is run straight after a fade-in.
    #[test]
    fn every_moved_grant_in_a_skip_all_seed_follows_a_fade_in() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, _) = seed(&romfs, &skip_all(), None);
        let mut moved = 0;
        for rel in rewrites.keys() {
            let Some(flow) = Flowchart::parse(&rewrites[rel]).unwrap() else {
                continue;
            };
            let Some(vanilla) = flows::load(&romfs, &flow.name).unwrap() else {
                continue;
            };
            // A moved grant is one added to the flow that opens the get
            // window: a cut scene's copied grants and starting items don't.
            let shown = shown(&flow);
            let opens = |e: usize| match flow.events[e].kind {
                EventKind::Action { next: Some(n), .. } => {
                    shown[n as usize].contains(grant::GET_LINES[0].as_str())
                }
                _ => false,
            };
            for e in vanilla.events.len()..flow.events.len() {
                if !shown[e].starts_with(GRANT_PREFIX) || !opens(e) {
                    continue;
                }
                moved += 1;
                for link in flow.links_to(e as u16) {
                    let Link::Next(p) = link else {
                        panic!("{} grants at #{e} off a {link:?}", flow.name);
                    };
                    assert_eq!(shown[p as usize], FADE, "{} grants at #{e}", flow.name);
                }
            }
        }
        // Every swapped scene with a gift, Zelda's and Impa's included.
        assert!(moved >= SKIPPED_IN.len() - UNBOUND.len(), "{moved}");
    }

    #[test]
    fn every_cut_scene_with_a_grant_grants_the_vanilla_item_once_without_its_setting() {
        every_cut_scene_with_a_grant_grants_once_where_it_played(false);
    }

    #[test]
    fn every_cut_scene_with_a_grant_grants_the_dealt_item_once_with_its_setting() {
        every_cut_scene_with_a_grant_grants_once_where_it_played(true);
    }
}
