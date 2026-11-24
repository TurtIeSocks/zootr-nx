//! A grant made inline, for an event that never binds `GetPouchContent`:
//! calling it there hangs, and listing it in the event's `SubFlowList`
//! crashes the game. The grant is the starting items' own, then the get
//! window's two lines as plain talk. The window is text only, with no get
//! animation or jingle.

use std::sync::LazyLock;

use anyhow::{Context, Result};
use ounce_formats::{Actor, EventKind, Flowchart, Params, Romfs, Value};

use crate::flows::{self, GRANT, Model, param};
use crate::known::{Name, field, flow, scene};
use crate::starting;

/// The flow whose talks show the get window.
pub(crate) const POUCH: &Name = flow::GET_POUCH_CONTENT;
/// What the get window says, in order: `DefaultGetMessage`'s two talks.
/// Both name whatever the last grant did, so they fit any item.
pub(crate) static GET_LINES: LazyLock<[String; 2]> = LazyLock::new(|| {
    [
        format!(
            "{}/{}:{}",
            field::EVENT_FLOW_MSG,
            scene::SY_GET,
            field::GET_MESSAGE
        ),
        format!(
            "{}/{}:{}",
            field::EVENT_FLOW_MSG,
            scene::SY_GET,
            field::GET_CAPTION
        ),
    ]
});
const TALK: &Name = flow::EVENT_TALK;
const MESSAGE: &Name = field::MESSAGE_ID;
/// The wait [`POUCH`] runs, which [`frame`] copies.
pub(crate) const WAIT: (&Name, &Name) = (field::GAME_SYSTEM_ACTOR, flow::EVENT_WAIT);

/// `pouch`'s wait, cut to one frame.
pub(crate) fn frame(pouch: &Flowchart) -> Result<Model> {
    let at = *flows::running(pouch, WAIT)
        .first()
        .with_context(|| format!("{POUCH} never waits"))?;
    let mut wait = flows::model(pouch, WAIT, at)?;
    flows::set(&mut wait.params, "Frames", Value::Int(vec![1]))?;
    Ok(wait)
}

/// The two flows an inline grant copies its events from, as [`Models::load`]
/// reads them.
pub(crate) type ModelCharts = (Flowchart, Flowchart);

/// The grant's model flows: the starting items' grant and the get window.
pub(crate) struct Models<'a> {
    template: &'a Flowchart,
    pouch: &'a Flowchart,
}

impl<'a> Models<'a> {
    pub(crate) fn load(romfs: &Romfs) -> Result<ModelCharts> {
        Ok((
            flows::timeline(romfs, starting::TEMPLATE)?,
            flows::timeline(romfs, POUCH)?,
        ))
    }

    pub(crate) fn of(template: &'a Flowchart, pouch: &'a Flowchart) -> Self {
        Self { template, pouch }
    }

    /// Grant `item`, a name or an argument, then show the get window, then
    /// go on to `next`. Returns the grant.
    pub(crate) fn push(&self, flow: &mut Flowchart, item: Value, next: Option<u16>) -> Result<u16> {
        let (grant, params) = starting::grant_event(self.template)?;
        let params = starting::granting(params, item, Value::Bool(vec![false]))?;
        let mut first = next;
        for id in GET_LINES.iter().rev() {
            let (actor, params) = told(self.pouch, id)?;
            first = Some(action(flow, actor, TALK, params.clone(), first));
        }
        Ok(action(flow, grant, GRANT.1, params, first))
    }
}

/// `pouch`'s talk showing `id`: the actor it runs on and its params.
fn told<'a>(pouch: &'a Flowchart, id: &str) -> Result<(&'a Actor, &'a Params)> {
    pouch
        .events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::Action { actor, params, .. } if param(params, MESSAGE) == Some(id) => {
                Some((pouch.actor_of(*actor), params))
            }
            _ => None,
        })
        .with_context(|| format!("{POUCH} never shows {id}"))
}

/// Push `actor.action(params)` onto `flow`, going on to `next`.
fn action(
    flow: &mut Flowchart,
    actor: &Actor,
    action: &str,
    params: Params,
    next: Option<u16>,
) -> u16 {
    let (actor, action) = flow.action(actor, action);
    flow.push(EventKind::Action {
        next,
        actor,
        action,
        params,
    })
}
