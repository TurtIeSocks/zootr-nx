//! How the harness drives the game besides its memory: buttons, the stick,
//! screenshots and starting it. The game on the Mac is one backend.

use std::path::Path;

use anyhow::Result;

/// How long a press holds its button, in milliseconds.
pub const PRESS: u64 = 150;

/// How far a start has got.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Boot {
    /// Not taking buttons yet.
    Starting,
    /// Menus or an event: A moves it on.
    Menus,
    /// In play, with no event and no load.
    Play,
}

pub trait Input {
    /// Whether the game is running already.
    fn running(&self) -> bool;

    /// Start the game with the installed seed.
    fn launch(&mut self) -> Result<()>;

    /// Whether this game takes buttons and the stick, or why not.
    fn keys(&self) -> Result<()>;

    /// Hold `button` (buttons joined with `+`) for `ms`.
    fn hold(&mut self, button: &str, ms: u64) -> Result<()>;

    fn press(&mut self, button: &str) -> Result<()> {
        self.hold(button, PRESS)
    }

    /// Push the stick toward `direction` (`up`, `down-left` and so on) for
    /// `ms`.
    fn stick(&mut self, direction: &str, ms: u64) -> Result<()>;

    /// A PNG of the game's picture, written to `to`.
    fn shot(&mut self, to: &Path) -> Result<()>;

    /// Ask the game to log what it's stuck on, after a stall.
    fn dump(&mut self) -> Result<()> {
        Ok(())
    }

    /// Quit the game, if it runs.
    fn quit(&mut self) -> Result<()> {
        Ok(())
    }

    /// How far a start has got, `None` when this input can't tell and the
    /// game's memory must.
    fn boot(&mut self) -> Result<Option<Boot>> {
        Ok(None)
    }
}

impl<I: Input + ?Sized> Input for Box<I> {
    fn running(&self) -> bool {
        (**self).running()
    }

    fn launch(&mut self) -> Result<()> {
        (**self).launch()
    }

    fn keys(&self) -> Result<()> {
        (**self).keys()
    }

    fn hold(&mut self, button: &str, ms: u64) -> Result<()> {
        (**self).hold(button, ms)
    }

    fn press(&mut self, button: &str) -> Result<()> {
        (**self).press(button)
    }

    fn stick(&mut self, direction: &str, ms: u64) -> Result<()> {
        (**self).stick(direction, ms)
    }

    fn shot(&mut self, to: &Path) -> Result<()> {
        (**self).shot(to)
    }

    fn dump(&mut self) -> Result<()> {
        (**self).dump()
    }

    fn quit(&mut self) -> Result<()> {
        (**self).quit()
    }

    fn boot(&mut self) -> Result<Option<Boot>> {
        (**self).boot()
    }
}
