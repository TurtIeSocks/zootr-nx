//! A trap's get window: "FOOL!" alone. The window shows its two lines as
//! two talks, so before each caption talk that follows the get line, the
//! seed asks whether the window names a trap and steps over the caption
//! if it does. That covers `GetPouchContent`'s default window and every
//! inline grant a setting copied into its own flow ([`crate::grant`]).

use anyhow::{Context, Result};
use ounce_formats::{EventKind, Flowchart, Link};

use crate::flows::{self, Model, Rewrites, param, set, string};
use crate::grant::{self, GET_LINES, POUCH};
use crate::known::{Name, field, flag, flow};

/// Whether a String64 entry holds a value. Case 0 is yes.
const IS_STRING: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_QUERY_IS_GAME_DATA_STRING64,
);
/// A vanilla flow that asks [`IS_STRING`], to copy.
const ASKS: &Name = flow::DISPLAY_SHOP;
/// The item the get window names.
const NAMED: &Name = flag::UI_MESSAGE_TARGET_ACTOR_NAME;

/// The two calls the check copies.
struct Models {
    is_string: Model,
    wait: Model,
}

impl Models {
    fn load(romfs: &ounce_formats::Romfs) -> Result<Self> {
        let model = |flow: &Flowchart, what| -> Result<Model> {
            let at = *flows::running(flow, what)
                .first()
                .with_context(|| format!("{} never runs {what:?}", flow.name))?;
            flows::model(flow, what, at)
        };
        Ok(Self {
            is_string: model(&flows::timeline(romfs, ASKS)?, IS_STRING)?,
            wait: grant::frame(&flows::timeline(romfs, POUCH)?)?,
        })
    }
}

/// The talk at `e`'s message, if it is one.
fn says(flow: &Flowchart, e: u16) -> Option<&str> {
    match &flow.events[e as usize].kind {
        EventKind::Action { params, .. } => param(params, field::MESSAGE_ID),
        _ => None,
    }
}

/// Each get line talk in `flow` whose next is the caption talk.
fn windows(flow: &Flowchart) -> Vec<(u16, u16)> {
    (0u16..)
        .zip(&flow.events)
        .filter_map(|(e, event)| match event.kind {
            EventKind::Action {
                next: Some(next), ..
            } if says(flow, e) == Some(&GET_LINES[0])
                && says(flow, next) == Some(&GET_LINES[1]) =>
            {
                Some((e, next))
            }
            _ => None,
        })
        .collect()
}

/// Between `line` and `caption`, one question per name in `traps`: a
/// trap's window waits a frame and goes on past the caption.
fn step_over(
    flow: &mut Flowchart,
    models: &Models,
    traps: &[&str],
    (line, caption): (u16, u16),
) -> Result<()> {
    let EventKind::Action { next: after, .. } = flow.events[caption as usize].kind else {
        unreachable!("windows finds actions");
    };
    let (actor, action) = flow.action(&models.wait.actor, models.wait.name);
    let past = flow.push(EventKind::Action {
        next: after,
        actor,
        action,
        params: models.wait.params.clone(),
    });
    let mut head = caption;
    for trap in traps.iter().rev() {
        let (actor, query) = flow.query(&models.is_string.actor, models.is_string.name);
        let mut params = models.is_string.params.clone();
        set(&mut params, flows::GAME_DATA, string(NAMED))?;
        set(&mut params, field::VALUE, string(trap))?;
        head = flow.push(EventKind::Switch {
            actor,
            query,
            params,
            cases: vec![(0, past), (1, head)],
        });
    }
    flow.link(Link::Next(line), Some(head))
}

/// Step over the caption for `traps` in every get window the seed has:
/// `GetPouchContent`'s and each rewritten flow's inline grants.
pub fn one_page(
    romfs: &ounce_formats::Romfs,
    rewrites: &mut Rewrites,
    traps: &[&str],
) -> Result<()> {
    if traps.is_empty() {
        return Ok(());
    }
    let models = Models::load(romfs)?;
    let pouch = flows::rel(POUCH);
    if !rewrites.contains_key(&pouch) {
        rewrites.insert(pouch.clone(), romfs.read(&pouch)?);
    }
    let caption = GET_LINES[1].as_bytes();
    for (rel, data) in rewrites.iter_mut() {
        if !data.windows(caption.len()).any(|w| w == caption) {
            continue;
        }
        let Some(mut flow) = Flowchart::parse(data).with_context(|| format!("parsing {rel}"))?
        else {
            continue;
        };
        let sites = windows(&flow);
        for site in &sites {
            step_over(&mut flow, &models, traps, *site)
                .with_context(|| format!("editing {rel}"))?;
        }
        if !sites.is_empty() {
            *data = flow.to_bytes();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    const FAKE_TRAPS: [&str; 2] = ["Item_FakeTrap1", "Item_FakeTrap2"];

    /// The messages `flow` shows from `start` when the window names `item`,
    /// answering each name question.
    fn shown(flow: &Flowchart, start: u16, item: &str) -> Vec<String> {
        let mut out = vec![];
        let mut at = Some(start);
        while let Some(e) = at {
            at = match &flow.events[e as usize].kind {
                EventKind::Action { next, .. } => {
                    out.extend(says(flow, e).map(str::to_owned));
                    *next
                }
                EventKind::Switch { params, cases, .. } => {
                    let case = u32::from(param(params, field::VALUE) != Some(item));
                    cases.iter().find(|(c, _)| *c == case).map(|(_, to)| *to)
                }
                other => panic!("unexpected event {other:?}"),
            };
        }
        out
    }

    fn default_window(flow: &Flowchart) -> u16 {
        let entry = flow
            .entry_points
            .iter()
            .find(|p| *p.name == **flow::DEFAULT_GET_MESSAGE);
        entry.and_then(|p| p.main).expect("the default window")
    }

    #[test]
    fn a_trap_window_shows_one_page_and_others_two() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        one_page(&romfs, &mut rewrites, &FAKE_TRAPS).unwrap();
        let flow = Flowchart::parse(&rewrites[&flows::rel(POUCH)])
            .unwrap()
            .unwrap();
        let start = default_window(&flow);
        for trap in FAKE_TRAPS {
            assert_eq!(shown(&flow, start, trap), [GET_LINES[0].clone()]);
        }
        let both = GET_LINES.to_vec();
        assert_eq!(shown(&flow, start, "Item_FakeSword"), both);
    }

    #[test]
    fn an_inline_grant_shows_one_page_for_a_trap() {
        let Some(romfs) = dev_dump() else { return };
        let (template, pouch) = crate::grant::Models::load(&romfs).unwrap();
        let mut flow = Flowchart::parse(&pouch.to_bytes()).unwrap().unwrap();
        let inline = crate::grant::Models::of(&template, &pouch);
        let grant = inline.push(&mut flow, string(FAKE_TRAPS[0]), None).unwrap();
        let mut rewrites = Rewrites::from([("Fake.bfevfl".to_owned(), flow.to_bytes())]);
        one_page(&romfs, &mut rewrites, &FAKE_TRAPS).unwrap();
        let flow = Flowchart::parse(&rewrites["Fake.bfevfl"]).unwrap().unwrap();
        assert_eq!(shown(&flow, grant, FAKE_TRAPS[0]), [GET_LINES[0].clone()]);
        assert_eq!(shown(&flow, grant, "Item_FakeSword"), GET_LINES.to_vec());
    }

    #[test]
    fn no_traps_rewrites_nothing() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        one_page(&romfs, &mut rewrites, &[]).unwrap();
        assert!(rewrites.is_empty());
    }
}
