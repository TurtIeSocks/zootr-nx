//! Quest steps in the flag map. A quest's current step is saved as the
//! enum `Step_<Challenge>`, and the flow actions and graph nodes that name a
//! quest (`ChallengeName`) move it or check it. Those become `writes` and
//! `reads` edges on that enum, labelled with the step, so `writers` lists
//! what moves a quest and `readers` what waits on it.

use crate::rsdb::flow_name;
use crate::steps::SAVED;

/// Words in the actions and classes that move a quest. Every other one
/// naming a quest checks its step.
const MOVERS: [&str; 4] = [
    "SetStep",
    "ProgressChallengeStep",
    "ProgressNextStep",
    "RollbackChallengeStep",
];

/// The saved step enum of `quest`, named bare (`DoorOfTime`, as flows do)
/// or by its file (as graphs do).
pub(super) fn state(quest: &str) -> String {
    format!("{SAVED}{}", flow_name(quest))
}

/// Whether an action or class of this name moves the quest it names.
pub(super) fn moves(class: &str) -> bool {
    MOVERS.iter().any(|m| class.contains(m))
}

/// An edge's label: the class, then the step it moves to or checks. A
/// graph's check also says which of its answers (`IsAfter`, `IsEqual`) the
/// graph reads.
pub(super) fn label(class: &str, step: Option<&str>, read: &[String]) -> String {
    let mut label = class.to_owned();
    if moves(class) {
        label.push_str(" to");
    } else if !read.is_empty() {
        label.push(' ');
        label.push_str(&read.join("|"));
    }
    if let Some(step) = step {
        label.push(' ');
        label.push_str(step);
    }
    label
}

#[cfg(test)]
mod tests {
    use super::{label, moves, state};

    #[test]
    fn a_quest_named_bare_or_by_its_file_is_the_same_saved_step() {
        assert_eq!(state("FakeSage"), "Step_FakeSage");
        assert_eq!(
            state("Work/Challenge/FakeSage.game__challenge__Challenge.gyml"),
            "Step_FakeSage"
        );
    }

    #[test]
    fn setting_or_progressing_moves_a_quest_and_checking_doesnt() {
        assert!(moves("EventExecuteProgressChallengeStep"));
        assert!(moves("TriggerChallengeSetStepNoEventPause"));
        assert!(moves("OneShotChallengeProgressNextStep"));
        assert!(!moves("HoldChallengeCurrentStepCheckOrder"));
        assert_eq!(
            label("TriggerChallengeSetStep", Some("Step02"), &[]),
            "TriggerChallengeSetStep to Step02"
        );
        let read = ["IsAfter".to_owned()];
        assert_eq!(
            label("HoldChallengeCurrentStepCheckOrder", Some("Step04"), &read),
            "HoldChallengeCurrentStepCheckOrder IsAfter Step04"
        );
    }
}
