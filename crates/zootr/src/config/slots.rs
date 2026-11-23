//! Several games on one machine (slots): which targets each plays, so a
//! shell set to one slot loads and saves only that slot's targets.

use super::{Config, Target};

impl Target {
    /// Whether this target is part of `game`, none meaning `here`.
    pub fn plays(&self, game: &str, here: &str) -> bool {
        self.game.as_deref().unwrap_or(here) == game
    }
}

impl Config {
    /// Only `game`'s targets, a target with none being `turn.here`'s.
    pub(super) fn of_game(mut self, game: &str) -> Self {
        let here = self.turn.here.clone();
        self.targets.retain(|t| t.plays(game, &here));
        self
    }

    /// This config with the targets `saved` keeps for other games than
    /// the one this shell plays, which [`Config::load`] left out.
    pub(super) fn with_others(&self, saved: Self, game: &str) -> Self {
        let mut all = self.clone();
        let here = saved.turn.here.clone();
        let others: Vec<Target> = (saved.targets.into_iter())
            .filter(|t| !t.plays(game, &here) && !all.targets.iter().any(|o| o.name == t.name))
            .collect();
        all.targets.extend(others);
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::target;

    /// A machine's own target and two slots' (`FakeHost`, `-2`, `-3`).
    fn slotted() -> Config {
        let mut config = Config::default();
        config.turn.here = "FakeHost".into();
        config.targets = vec![target("FakeOne", None), target("FakeTwo", None)];
        config.targets.push(Target {
            game: Some("FakeHost-2".into()),
            ..target("FakeSlotTwo", None)
        });
        config.targets.push(Target {
            game: Some("FakeHost-3".into()),
            ..target("FakeSlotThree", None)
        });
        config
    }

    fn names(config: &Config) -> Vec<&str> {
        config.targets.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn a_shell_sees_only_the_targets_of_the_game_it_plays() {
        assert_eq!(
            names(&slotted().of_game("FakeHost")),
            ["FakeOne", "FakeTwo"]
        );
        assert_eq!(names(&slotted().of_game("FakeHost-2")), ["FakeSlotTwo"]);
        assert_eq!(names(&slotted().of_game("FakeAway")), [""; 0]);
    }

    #[test]
    fn saving_one_slot_keeps_the_other_games_targets() {
        let mut two = slotted().of_game("FakeHost-2");
        two.targets[0].env = vec!["FAKE=1".into()];
        let saved = two.with_others(slotted(), "FakeHost-2");
        assert_eq!(
            names(&saved),
            ["FakeSlotTwo", "FakeOne", "FakeTwo", "FakeSlotThree"]
        );
        assert_eq!(saved.targets[0].env, ["FAKE=1"]);
        // A config holding every game already gains nothing twice.
        assert_eq!(
            slotted().with_others(slotted(), "FakeHost").targets.len(),
            4
        );
    }
}
