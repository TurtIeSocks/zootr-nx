//! How the harness drives the game besides its memory: buttons, the stick,
//! screenshots and starting it. The game on the Mac is one backend.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};

/// How long a press holds its button, in milliseconds.
pub const PRESS: u64 = 150;
/// The names a button press takes, joined with `+`.
const BUTTONS: [&str; 24] = [
    "A", "B", "X", "Y", "L", "R", "ZL", "ZR", "Plus", "Minus", "Left", "Up", "Right", "Down", "LS",
    "RS", "LLeft", "LUp", "LRight", "LDown", "RLeft", "RUp", "RRight", "RDown",
];
/// How long the game gets to quit.
const QUIT: Duration = Duration::from_secs(30);

/// Refuse `buttons` (names joined with `+`) unless each is a button.
pub fn check_buttons(buttons: &str) -> Result<()> {
    for b in buttons.split('+') {
        ensure!(
            BUTTONS.contains(&b),
            "{b} isn't a button. Buttons are {}",
            BUTTONS.join(" ")
        );
    }
    Ok(())
}

/// The left stick's buttons for a direction like `up` or `down-left`.
/// Other names, like `RUp`, pass through.
pub fn stick(direction: &str) -> String {
    let parts = direction.split(['-', '+']).map(|d| match d {
        "up" => "LUp",
        "down" => "LDown",
        "left" => "LLeft",
        "right" => "LRight",
        other => other,
    });
    parts.collect::<Vec<_>>().join("+")
}

/// Wait for `input`'s game to be gone, after asking it to quit.
pub(super) fn gone(input: &(impl Input + ?Sized)) -> Result<()> {
    let start = Instant::now();
    while input.running() {
        ensure!(start.elapsed() < QUIT, "the game didn't quit");
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(())
}

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

    /// Start the game with the installed seed, playing from the save
    /// folder `save` when given (the target's `{save}` harness line).
    fn launch(&mut self, save: Option<&Path>) -> Result<()>;

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

    /// Leave one control line (`LUp~0.5`, `shot PATH`) and return at once,
    /// without waiting out its hold.
    fn queue(&mut self, line: &str) -> Result<()> {
        bail!("this game takes no control lines, so can't take {line}")
    }

    /// Ask the game to log what it's stuck on, after a stall.
    fn dump(&mut self) -> Result<()> {
        Ok(())
    }

    /// What the game's own log says ended its last run (a crash line),
    /// when this input keeps one.
    fn crash(&self) -> Option<String> {
        None
    }

    /// Quit the game, if it runs, and return once it's gone.
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

    fn launch(&mut self, save: Option<&Path>) -> Result<()> {
        (**self).launch(save)
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

    fn queue(&mut self, line: &str) -> Result<()> {
        (**self).queue(line)
    }

    fn dump(&mut self) -> Result<()> {
        (**self).dump()
    }

    fn crash(&self) -> Option<String> {
        (**self).crash()
    }

    fn quit(&mut self) -> Result<()> {
        (**self).quit()
    }

    fn boot(&mut self) -> Result<Option<Boot>> {
        (**self).boot()
    }
}
