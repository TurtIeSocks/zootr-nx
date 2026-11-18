//! Received items' notices in game ([`crate::notice`]): queued as items
//! are given, shown one at a time once Link is in play, and a burst past
//! its first few summed up in one. Apart from the connection, so it runs
//! against the fake game in tests.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::client::Game;
use crate::notice::Shown;

/// The least time between two notices. Each replaces the last on screen.
pub const PACE: Duration = Duration::from_millis(1500);
/// A burst's notices shown one by one before the rest are summed up.
const ONE_BY_ONE: usize = 3;

#[derive(Debug, Default)]
pub struct Notices {
    waiting: VecDeque<Shown>,
    /// Shown since nothing last waited: a burst's count so far.
    burst: usize,
    last: Option<Instant>,
}

impl Notices {
    /// Link got `item` from slot `sender`.
    pub fn got(&mut self, item: &str, sender: u32) {
        self.waiting.push_back(Shown::item(item, sender));
    }

    /// Show the next notice, if one waits, the last is [`PACE`] old and
    /// Link is in play. Once a burst has shown its first few, the rest
    /// show as one summary.
    pub fn look(&mut self, game: &mut impl Game, now: Instant) -> Result<Option<Shown>> {
        if self.waiting.is_empty() {
            self.burst = 0;
            return Ok(None);
        }
        if self.last.is_some_and(|last| now < last + PACE) || !game.ready()? {
            return Ok(None);
        }
        let shown = match self.burst >= ONE_BY_ONE && self.waiting.len() > 1 {
            true => summary(self.waiting.drain(..)),
            false => self.waiting.pop_front().expect("one waits"),
        };
        self.burst += 1;
        self.last = Some(now);
        game.notice(&shown)?;
        Ok(Some(shown))
    }
}

/// One notice for all of `items`: from their sender, or from several.
fn summary(items: impl Iterator<Item = Shown>) -> Shown {
    let (mut count, mut senders) = (0, Vec::new());
    for item in items {
        count += 1;
        if !senders.contains(&item.sender) {
            senders.push(item.sender);
        }
    }
    let one = match senders[..] {
        [sender] => Some(sender),
        _ => None,
    };
    Shown::summary(one, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::client::tests::Fake;
    use zootr_hook::Notice;

    const SARIA: u32 = 2;
    const DARUNIA: u32 = 3;

    /// Look every 0.5 s for `secs`, as the client does.
    fn run(notices: &mut Notices, game: &mut Fake, start: Instant, secs: u64) {
        for half in 0..secs * 2 {
            let now = start + Duration::from_millis(half * 500);
            notices.look(game, now).unwrap();
        }
    }

    #[test]
    fn notices_are_paced_and_wait_for_play() {
        let (mut notices, mut game, start) = (Notices::default(), Fake::default(), Instant::now());
        notices.got("Item_FakeHook", SARIA);
        notices.got("Item_FakeBow", SARIA);
        game.busy = true;
        run(&mut notices, &mut game, start, 2);
        assert!(game.notices.is_empty(), "Link isn't in play");
        game.busy = false;
        let now = start + Duration::from_secs(2);
        notices.look(&mut game, now).unwrap();
        notices
            .look(&mut game, now + Duration::from_millis(1000))
            .unwrap();
        assert_eq!(game.notices.len(), 1, "the second waits its 1.5 s");
        notices.look(&mut game, now + PACE).unwrap();
        assert_eq!(
            game.notices,
            [
                Shown::item("Item_FakeHook", SARIA),
                Shown::item("Item_FakeBow", SARIA)
            ]
        );
    }

    #[test]
    fn a_burst_shows_three_then_the_rest_in_one() {
        let (mut notices, mut game) = (Notices::default(), Fake::default());
        for _ in 0..82 {
            notices.got("Item_FakeBow", SARIA);
        }
        run(&mut notices, &mut game, Instant::now(), 10);
        let kinds: Vec<_> = game.notices.iter().map(|n| (n.kind, n.count)).collect();
        assert_eq!(
            kinds,
            [
                (Notice::Item, 1),
                (Notice::Item, 1),
                (Notice::Item, 1),
                (Notice::Summary, 79)
            ]
        );
        assert_eq!(game.notices[3].sender, SARIA);
    }

    #[test]
    fn a_burst_from_several_senders_sums_up_plainly() {
        let (mut notices, mut game) = (Notices::default(), Fake::default());
        for sender in [SARIA, SARIA, SARIA, SARIA, DARUNIA] {
            notices.got("Item_FakeBow", sender);
        }
        run(&mut notices, &mut game, Instant::now(), 10);
        assert_eq!(game.notices[3], Shown::summary(None, 2));
        assert_eq!(game.notices[3].kind, Notice::Mixed);
    }

    #[test]
    fn one_left_after_three_shows_as_itself() {
        let (mut notices, mut game) = (Notices::default(), Fake::default());
        for _ in 0..4 {
            notices.got("Item_FakeBow", SARIA);
        }
        run(&mut notices, &mut game, Instant::now(), 10);
        assert!(game.notices.iter().all(|n| n.kind == Notice::Item));
        assert_eq!(game.notices.len(), 4);
    }

    #[test]
    fn a_burst_ends_once_nothing_waits() {
        let (mut notices, mut game, start) = (Notices::default(), Fake::default(), Instant::now());
        for _ in 0..3 {
            notices.got("Item_FakeBow", SARIA);
        }
        run(&mut notices, &mut game, start, 10);
        for _ in 0..3 {
            notices.got("Item_FakeHook", DARUNIA);
        }
        run(&mut notices, &mut game, start + Duration::from_secs(10), 10);
        assert_eq!(game.notices.len(), 6);
        assert!(game.notices.iter().all(|n| n.kind == Notice::Item));
    }
}
