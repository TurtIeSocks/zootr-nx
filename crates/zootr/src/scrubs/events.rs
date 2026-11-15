//! Finding and copying events in the shared scrub flow.

use anyhow::{Context, Result, bail};
use ounce_formats::{EventKind, Flowchart};

use super::{FLOW, SELECTED, SET_STRING};
use crate::flows::{self, GAME_DATA, events, param, params_mut, string};

/// The events running `what` whose `key` param is `value`.
pub(super) fn with(
    flow: &Flowchart,
    what: (impl AsRef<str>, impl AsRef<str>),
    key: &str,
    value: &str,
) -> Vec<u16> {
    events(flow, what)
        .into_iter()
        .filter(|(_, p)| param(p, key) == Some(value))
        .map(|(i, _)| i)
        .collect()
}

pub(super) fn one(found: Vec<u16>, what: &str) -> Result<u16> {
    match found[..] {
        [at] => Ok(at),
        _ => bail!("{FLOW} has {} {what}, not one", found.len()),
    }
}

/// The flow's calls to its own entry point `entry`.
pub(super) fn calls(flow: &Flowchart, entry: &str) -> Vec<u16> {
    let calls = (0u16..).zip(&flow.events);
    calls
        .filter(|(_, e)| match &e.kind {
            EventKind::SubFlow {
                entry_point,
                flowchart,
                ..
            } => entry_point == entry && flows::is_local(flow, flowchart),
            _ => false,
        })
        .map(|(i, _)| i)
        .collect()
}

/// A copy of call `e` that ends the flow.
pub(super) fn last(flow: &Flowchart, e: u16) -> EventKind {
    let mut call = flow.events[e as usize].kind.clone();
    if let EventKind::SubFlow { next, .. } = &mut call {
        *next = None;
    }
    call
}

/// Where a switch's case `n` goes.
pub(super) fn case(flow: &Flowchart, e: u16, n: u32) -> Result<u16> {
    match &flow.events[e as usize].kind {
        EventKind::Switch { cases, .. } => cases
            .iter()
            .find(|c| c.0 == n)
            .map(|c| c.1)
            .with_context(|| format!("event {e} has no case {n}")),
        _ => bail!("event {e} isn't a switch"),
    }
}

/// A yes-or-no switch's cases: true, then false.
pub(super) fn set_cases(kind: &mut EventKind, [yes, no]: [u16; 2]) {
    if let EventKind::Switch { cases, .. } = kind {
        *cases = vec![(0, yes), (1, no)];
    }
}

/// A copy of the first event running `what` that sets `name` to `value`.
pub(super) fn set_string(
    flow: &Flowchart,
    name: &str,
    value: &str,
    next: Option<u16>,
) -> Result<EventKind> {
    let sets = with(flow, SET_STRING, GAME_DATA, SELECTED);
    let mut set = flow.events[*sets.first().context("no string set to copy")? as usize]
        .kind
        .clone();
    flows::set(params_mut(&mut set), GAME_DATA, string(name))?;
    flows::set(params_mut(&mut set), "Value", string(value))?;
    if let EventKind::Action { next: n, .. } = &mut set {
        *n = next;
    }
    Ok(set)
}
