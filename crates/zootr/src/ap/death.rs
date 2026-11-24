//! DeathLink: Link's deaths go out to the room, and the room's come in.
//! Apart from the connection, so it runs against the fake game in tests.

use anyhow::Result;

use super::client::Game;

/// A death from the room.
#[derive(Clone, Debug, PartialEq)]
pub struct Death {
    pub source: String,
    pub cause: Option<String>,
}

/// What a look did.
#[derive(Debug, PartialEq)]
pub enum Done {
    /// Link died: tell the room.
    Died,
    /// Link was killed for a death from the room.
    Killed(Death),
}

/// Link's death counter at the last look. A death moves it up by one, and
/// a save loading by anything, so only a rise of one counts.
#[derive(Debug, Default)]
pub struct Count(Option<i64>);

impl Count {
    /// Whether the counter reading `n` is a death since the last look.
    pub fn died(&mut self, n: i64) -> bool {
        let died = self.0.is_some_and(|last| n == last + 1);
        self.0 = Some(n);
        died
    }
}

/// Looks a kill gets to take Link out of play before it counts as missed.
const GRACE: u32 = 20;

/// Link dying of a death from the room, which isn't sent back.
#[derive(Debug, Default)]
struct Dying {
    /// He has been out of play since the kill.
    out: bool,
    looks: u32,
}

#[derive(Debug, Default)]
pub struct Deaths {
    /// The latest death from the room, waiting for Link to be in play.
    held: Option<Death>,
    dying: Option<Dying>,
}

impl Deaths {
    /// A death from the room. Several before Link is in play kill him once.
    pub fn hold(&mut self, death: Death) {
        self.held = Some(death);
    }

    /// Send Link's death, or kill him for the room's once he's in play.
    pub fn look(&mut self, game: &mut impl Game) -> Result<Option<Done>> {
        let died = game.died()?;
        let ready = game.ready()?;
        if let Some(d) = &mut self.dying {
            d.out |= !ready;
            d.looks += 1;
            // He died of it, a fairy saved him, or the kill never landed.
            if died || (d.out && ready) || (!d.out && d.looks >= GRACE) {
                self.dying = None;
            }
            return Ok(None);
        }
        if died {
            return Ok(Some(Done::Died));
        }
        match self.held.clone() {
            Some(death) if ready => {
                game.kill()?;
                self.held = None;
                self.dying = Some(Dying::default());
                Ok(Some(Done::Killed(death)))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::client::tests::Fake;

    fn death(source: &str) -> Death {
        Death {
            source: source.into(),
            cause: None,
        }
    }

    #[test]
    fn a_rise_of_one_is_a_death_and_a_save_s_jump_isn_t() {
        let mut count = Count::default();
        let died: Vec<bool> = [0, 1, 1, 5, 2, 3].map(|n| count.died(n)).into();
        // The first read only sets the baseline.
        assert_eq!(died, [false, true, false, false, false, true]);
    }

    #[test]
    fn a_death_is_sent_once() {
        let (mut deaths, mut game) = (Deaths::default(), Fake::default());
        game.died = true;
        assert_eq!(deaths.look(&mut game).unwrap(), Some(Done::Died));
        assert_eq!(deaths.look(&mut game).unwrap(), None);
    }

    #[test]
    fn deaths_from_the_room_wait_for_play_then_kill_once() {
        let (mut deaths, mut game) = (Deaths::default(), Fake::default());
        game.busy = true;
        for n in 1..=3 {
            deaths.hold(death(&format!("FakeSlot{n}")));
            assert_eq!(deaths.look(&mut game).unwrap(), None);
        }
        game.busy = false;
        let done = deaths.look(&mut game).unwrap();
        assert_eq!(done, Some(Done::Killed(death("FakeSlot3"))));
        assert_eq!(deaths.look(&mut game).unwrap(), None);
        assert_eq!(game.kills, 1);
    }

    #[test]
    fn the_death_a_kill_causes_isnt_sent_back() {
        let (mut deaths, mut game) = (Deaths::default(), Fake::default());
        deaths.hold(death("FakeSlot"));
        deaths.look(&mut game).unwrap();
        game.busy = true;
        assert_eq!(deaths.look(&mut game).unwrap(), None);
        game.died = true;
        assert_eq!(deaths.look(&mut game).unwrap(), None);
        // His next death is his own.
        game.busy = false;
        game.died = true;
        assert_eq!(deaths.look(&mut game).unwrap(), Some(Done::Died));
    }

    #[test]
    fn a_fairy_save_ends_the_kill_once_link_is_back_in_play() {
        let (mut deaths, mut game) = (Deaths::default(), Fake::default());
        deaths.hold(death("FakeSlot"));
        deaths.look(&mut game).unwrap();
        game.busy = true;
        deaths.look(&mut game).unwrap();
        game.busy = false;
        deaths.look(&mut game).unwrap();
        game.died = true;
        assert_eq!(deaths.look(&mut game).unwrap(), Some(Done::Died));
    }

    #[test]
    fn a_kill_that_never_lands_is_let_go() {
        let (mut deaths, mut game) = (Deaths::default(), Fake::default());
        deaths.hold(death("FakeSlot"));
        deaths.look(&mut game).unwrap();
        for _ in 0..GRACE {
            deaths.look(&mut game).unwrap();
        }
        game.died = true;
        assert_eq!(deaths.look(&mut game).unwrap(), Some(Done::Died));
    }
}
