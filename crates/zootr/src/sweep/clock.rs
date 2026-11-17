//! The hour a spot is swept at. A giver talked to is swept at noon, when
//! the game has them out: the other machine's saved file loads at night,
//! and at the ranch Malon's talk found no one (2026-11-17). Every other
//! spot keeps the hour the file loads at.

use anyhow::Result;

use super::plan::Trigger;
use crate::dev::{Dev, Request};

const NOON: u32 = 12;

fn hour(trigger: &Trigger) -> Option<u32> {
    matches!(trigger, Trigger::Talk { .. }).then_some(NOON)
}

/// Set the hour `trigger` is swept at, before the teleport that spawns
/// the people there.
pub(super) fn set(dev: &Dev, trigger: &Trigger) -> Result<()> {
    if let Some(h) = hour(trigger) {
        dev.ask(&Request::Time(h))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_talk_is_swept_at_noon_and_the_rest_at_the_files_hour() {
        let talk = Trigger::Talk {
            map: "FakeHill_Child".into(),
            at: [0.0; 3],
            face: [0.0, 0.0, 1.0],
            steps: vec![],
            ready: vec![],
            then: None,
            give: None,
            counter: false,
            halt: None,
        };
        assert_eq!(hour(&talk), Some(NOON));
        assert_eq!(hour(&Trigger::Skip("out of scope".into())), None);
    }
}
