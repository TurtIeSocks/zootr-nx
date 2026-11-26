//! The gauntlet's judge, with no model: A through every line and the
//! first choice of every box, until it gives up. A spot it can't see
//! through fails as unresolved (maintainer, 2026-11-25: no model in the
//! gauntlet).

use anyhow::Result;

use crate::sweep::judge::{Answer, Judge, Look};

/// How many answers a spot gets before the judge gives up.
const MOST: usize = 30;

/// What the judge's give-up says, so the report can tell it from a bug.
pub const GAVE_UP: &str = "unresolved: the scripted judge pressed A";

/// A presses, then a give-up.
#[derive(Default)]
pub struct Scripted;

impl Judge for Scripted {
    fn look(&mut self, look: &Look) -> Result<Answer> {
        Ok(match look.tried.len() >= MOST {
            true => Answer::Bug {
                why: format!("{GAVE_UP} {MOST} times"),
            },
            false => Answer::Press {
                button: "A".into(),
                why: "scripted".into(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn look(tried: &[String]) -> Answer {
        let look = Look {
            spot: "fake spot",
            expected: "Item_Fake",
            map: "FakeMap",
            event: true,
            tried,
            shot: Path::new("nothing.png"),
        };
        Scripted.look(&look).unwrap()
    }

    #[test]
    fn it_presses_a_then_gives_up() {
        assert!(matches!(look(&[]), Answer::Press { button, .. } if button == "A"));
        let many = vec!["A".to_string(); MOST];
        assert!(matches!(look(&many), Answer::Bug { why } if why.starts_with(GAVE_UP)));
    }
}
