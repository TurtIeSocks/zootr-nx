//! The trouble the client keeps going past, said once while it lasts: a
//! dead or paused game fails the same way every pass, and its lines would
//! push everything else out of the log.

use std::collections::HashMap;

use anyhow::Result;

use super::client::Note;

/// What a trouble came from. Each keeps its own, so two that fail every
/// pass don't take turns saying theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A look at the game: while it fails, the game can't be reached.
    Game,
    Send,
    Goal,
    Give,
    /// A look for Link's death.
    Death,
    /// Telling the room he died.
    DeathSent,
}

/// The trouble each source last said, until it clears.
#[derive(Debug, Default)]
pub struct Troubles(HashMap<Source, String>);

impl Troubles {
    /// `outcome`'s value. Its trouble is noted when it's new, and a source
    /// that had one says so once it clears.
    pub fn take<T>(
        &mut self,
        from: Source,
        outcome: Result<T>,
        note: &mut impl FnMut(Note),
    ) -> Option<T> {
        let (value, why) = match outcome {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        if let Some(n) = self.news(from, why) {
            note(n);
        }
        value
    }

    /// What to say of `from`'s latest trouble, `None` when it had none.
    fn news(&mut self, from: Source, why: Option<String>) -> Option<Note> {
        let game = from == Source::Game;
        let was = match &why {
            Some(w) => self.0.insert(from, w.clone()),
            None => self.0.remove(&from),
        };
        match (why, was) {
            (Some(w), Some(was)) if w == was => None,
            (Some(w), _) if game => Some(Note::Unreachable(w)),
            (Some(w), _) => Some(Note::Trouble(w)),
            (None, Some(_)) if game => Some(Note::Reachable),
            (None, Some(was)) => Some(Note::Cleared(was)),
            (None, None) => None,
        }
    }

    /// The last look at the game worked.
    pub fn reachable(&self) -> bool {
        !self.0.contains_key(&Source::Game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The notes `outcomes` make, in order.
    fn said(troubles: &mut Troubles, outcomes: &[(Source, Option<&str>)]) -> Vec<Note> {
        let mut notes = vec![];
        for &(from, why) in outcomes {
            let outcome = why.map_or(Ok(()), |w| Err(anyhow::anyhow!(w.to_owned())));
            troubles.take(from, outcome, &mut |n| notes.push(n));
        }
        notes
    }

    #[test]
    fn a_trouble_is_said_once_while_it_lasts_and_once_when_it_clears() {
        let pause = Some("FakePause");
        let notes = said(
            &mut Troubles::default(),
            &[
                (Source::Give, pause),
                (Source::Give, pause),
                (Source::Give, pause),
                (Source::Give, None),
                (Source::Give, None),
            ],
        );
        assert_eq!(
            notes,
            [
                Note::Trouble("FakePause".into()),
                Note::Cleared("FakePause".into())
            ]
        );
    }

    #[test]
    fn a_changed_trouble_is_said_again() {
        let notes = said(
            &mut Troubles::default(),
            &[
                (Source::Send, Some("FakeA")),
                (Source::Send, Some("FakeB")),
                (Source::Send, Some("FakeB")),
            ],
        );
        assert_eq!(
            notes,
            [Note::Trouble("FakeA".into()), Note::Trouble("FakeB".into())]
        );
    }

    #[test]
    fn two_sources_failing_every_pass_each_say_theirs_once() {
        let pass = [
            (Source::Game, Some("FakePipe")),
            (Source::Death, Some("FakeRefused")),
        ];
        let notes = said(&mut Troubles::default(), &pass.repeat(4));
        assert_eq!(
            notes,
            [
                Note::Unreachable("FakePipe".into()),
                Note::Trouble("FakeRefused".into())
            ]
        );
    }

    #[test]
    fn the_game_is_unreachable_until_a_look_works() {
        let mut troubles = Troubles::default();
        assert!(troubles.reachable());
        let lost = said(&mut troubles, &[(Source::Game, Some("FakePipe"))]);
        assert_eq!(lost, [Note::Unreachable("FakePipe".into())]);
        assert!(!troubles.reachable());
        assert_eq!(
            said(&mut troubles, &[(Source::Game, None)]),
            [Note::Reachable]
        );
        assert!(troubles.reachable());
    }
}
