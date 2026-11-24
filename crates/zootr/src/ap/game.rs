//! The client's game side: what the core needs of the running game, and
//! [`Live`], the real one: the target's game, read directly or through its
//! bridge.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use ounce_formats::Romfs;

use super::RECEIVED;
use super::client::Book;
use super::death::Count;
use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::known::flow;
use crate::live::{self, Game as Target};
use crate::notice::Shown;
use crate::steps;
use crate::tracker::{Flag, Tracker};

/// What the core needs of the running game.
pub trait Game {
    /// Link is in play: no load and no event.
    fn ready(&mut self) -> Result<bool>;
    /// The spots Link has checked.
    fn checked(&mut self) -> Result<BTreeSet<u64>>;
    /// Link has reached the goal.
    fn goal(&mut self) -> Result<bool>;
    /// How many received items Link has been given, from the save.
    fn given(&mut self) -> Result<usize>;
    /// Give Link `actor`. Says what he got: a progressive item's level.
    /// An error marked [`crate::dev::Landed`] came after the item reached him.
    fn give(&mut self, actor: &str) -> Result<String>;
    /// Save that `given` received items have been given.
    fn count(&mut self, given: usize) -> Result<()>;
    /// Show a received item's notice.
    fn notice(&mut self, shown: &Shown) -> Result<()>;
    /// Link died since the last look.
    fn died(&mut self) -> Result<bool>;
    /// Drop Link's hearts to zero, as a last hit would.
    fn kill(&mut self) -> Result<()>;
}

/// The installed seed's client parts, on the game `target` runs.
pub struct Installed {
    pub book: Book,
    pub game: Live,
    /// The seed's own slot.
    pub slot: String,
}

/// What the client needs of the installed seed. `gd` is the installed
/// seed's, which names the Int it adds.
pub fn installed(romfs: &Romfs, gd: Arc<GameData>, target: Target) -> Result<Installed> {
    let history = crate::history::History::load()?;
    let entry = history.installed_entry()?;
    let seed =
        (entry.extras.ap.as_ref()).context("the installed seed isn't one from Archipelago")?;
    seed.check_version()?;
    Ok(Installed {
        book: Book::new(romfs, seed)?,
        game: Live::new(target, gd, &entry.report.spots)?,
        slot: seed.slot.clone(),
    })
}

/// The running game, through the mailbox.
pub struct Live {
    dev: Dev<Target>,
    gd: Arc<GameData>,
    tracker: Tracker,
    /// Checks the seed hands over at the start, as Met Zelda's: their
    /// scenes never play, so no flag says so.
    given: BTreeSet<u64>,
    goal: u32,
    received: u32,
    /// [`DEATHS`] at the last look.
    deaths: Count,
}

impl Live {
    pub fn new(game: Target, gd: Arc<GameData>, spots: &[Spot]) -> Result<Self> {
        let tracker = Tracker::new(&gd, spots)?;
        Ok(Self {
            dev: Dev::new(game)?,
            gd,
            tracker,
            given: spots.iter().filter(|s| s.given).map(|s| s.hash).collect(),
            goal: live::hash(&steps::saved(flow::DEFEAT_GANON))?,
            received: live::hash(RECEIVED)?,
            deaths: Count::default(),
        })
    }

    fn get(&self, hash: u32) -> Result<String> {
        self.dev.game().get(&self.gd, hash)
    }
}

/// Link's deaths in the save (GameData, unnamed). It counts the moment his
/// hearts run out, and a fairy's save doesn't count.
const DEATHS: u32 = 0xd30c_8e29;

/// The screen the game shows (GameData, an unnamed Enum): play, a menu,
/// the title, Game Over, or [`SCREEN_LOADING`].
const SCREEN: u32 = 0x578d_41e1;
/// [`SCREEN`] while a save loads, and on a reload after Game Over.
const SCREEN_LOADING: &str = "Loading";

/// The step Ganon's defeat reaches once his last fight is over.
const GOAL_DONE: &str = "Complete";

impl Game for Live {
    fn ready(&mut self) -> Result<bool> {
        Ok(!self.dev.busy()?)
    }

    /// One read for every Bool, and the per-placement flags only when a
    /// spot goes by one.
    fn checked(&mut self) -> Result<BTreeSet<u64>> {
        let game = self.dev.game();
        let (mut bools, mut keyed) = (Vec::new(), false);
        for flag in self.tracker.flags() {
            match flag {
                Flag::Bool(h) => bools.push(h),
                Flag::Key(_) => keyed = true,
            }
        }
        let read = live::bools(game, game.global(), &bools)?;
        let on: HashSet<u32> = (bools.iter().zip(read))
            .filter_map(|(&h, on)| on.then_some(h))
            .collect();
        let keys = match keyed {
            true => live::keys(game, game.global())?,
            false => BTreeSet::new(),
        };
        self.tracker.load(|f| match f {
            Flag::Bool(h) => on.contains(&h),
            Flag::Key(k) => keys.contains(&k),
        });
        Ok(self.tracker.checked().union(&self.given).copied().collect())
    }

    fn goal(&mut self) -> Result<bool> {
        Ok(self.get(self.goal)? == GOAL_DONE)
    }

    fn given(&mut self) -> Result<usize> {
        let n: i64 = self.get(self.received)?.parse()?;
        Ok(usize::try_from(n)?)
    }

    fn died(&mut self) -> Result<bool> {
        let loading = self.get(SCREEN)? == self.gd.name(live::hash(SCREEN_LOADING)?);
        Ok(self.deaths.died(self.get(DEATHS)?.parse()?, loading))
    }

    fn kill(&mut self) -> Result<()> {
        self.dev.kill()
    }

    fn give(&mut self, actor: &str) -> Result<String> {
        self.dev.give_level(actor)
    }

    fn count(&mut self, given: usize) -> Result<()> {
        let game = self.dev.game();
        live::set(game, game.global(), self.received, &given.to_string())
    }

    fn notice(&mut self, shown: &Shown) -> Result<()> {
        self.dev.notice(shown)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::dev::Landed;

    #[derive(Default)]
    pub(in crate::ap) struct Fake {
        pub busy: bool,
        pub checked: BTreeSet<u64>,
        pub goal: bool,
        /// What Link got, give by give.
        pub given: Vec<String>,
        /// The save's count of received items given.
        pub counted: usize,
        /// Each give lands, then fails.
        pub fail_landed: bool,
        /// Link died since the last look.
        pub died: bool,
        pub kills: usize,
        pub notices: Vec<Shown>,
    }

    impl Game for Fake {
        fn ready(&mut self) -> Result<bool> {
            Ok(!self.busy)
        }
        fn checked(&mut self) -> Result<BTreeSet<u64>> {
            Ok(self.checked.clone())
        }
        fn goal(&mut self) -> Result<bool> {
            Ok(self.goal)
        }
        fn given(&mut self) -> Result<usize> {
            Ok(self.counted)
        }
        fn give(&mut self, actor: &str) -> Result<String> {
            self.given.push(actor.into());
            match self.fail_landed {
                true => Err(anyhow::anyhow!("FakeWait timed out").context(Landed)),
                false => Ok(actor.into()),
            }
        }
        fn count(&mut self, given: usize) -> Result<()> {
            self.counted = given;
            Ok(())
        }
        fn notice(&mut self, shown: &Shown) -> Result<()> {
            self.notices.push(shown.clone());
            Ok(())
        }
        fn died(&mut self) -> Result<bool> {
            Ok(std::mem::take(&mut self.died))
        }
        fn kill(&mut self) -> Result<()> {
            self.kills += 1;
            Ok(())
        }
    }
}
