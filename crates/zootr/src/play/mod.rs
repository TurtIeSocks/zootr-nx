//! The play harness: runs a script against the running game and writes
//! a verdict with its evidence. It talks to the game only through
//! [`live::Memory`](crate::live::Memory) and [`Input`].

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Target;

mod input;
mod mac;
mod native;
mod report;
mod run;
mod script;
mod watch;

pub use input::{Boot, Input, PRESS};
pub use mac::Mac;
pub use native::Native;
pub use report::Report;
pub use run::{Run, run, session};
pub use script::{Kind, Script, Step};

/// The target's game as an [`Input`]: the native build when the target
/// has a bridge, else the Mac game. `game` is the dump `{game}` names.
pub fn input(target: &Target, game: Option<&Path>) -> Result<Box<dyn Input>> {
    let args = target.launch_args(game)?;
    Ok(match &target.bridge {
        Some(socket) => {
            let env = target.launch_env()?;
            Box::new(Native::new(target.app.clone(), args, env, socket.clone()))
        }
        None => Box::new(Mac::new(target.app.clone(), args, target.control.clone())),
    })
}

/// A run's own folder, `play/<time>-<name>/` beside the app's live logs.
pub fn folder(name: &str) -> Result<PathBuf> {
    let time = jiff::Zoned::now().strftime("%Y-%m-%d-%H%M%S");
    let slug: String = (name.chars())
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug
        .split('-')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    Ok(crate::recorder::logs()?
        .with_file_name("play")
        .join(format!("{time}-{slug}")))
}
