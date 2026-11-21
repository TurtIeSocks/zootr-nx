//! The play harness: runs a script against the running game and writes
//! a verdict with its evidence. It talks to the game only through
//! [`live::Memory`](crate::live::Memory) and [`Input`].

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Target;

pub mod boot;
mod input;
mod mac;
mod native;
mod report;
mod run;
mod script;
mod watch;

pub use boot::Start;
pub use input::{Boot, Input, PRESS};
pub use mac::Mac;
pub use native::Native;
pub use report::Report;
pub use run::{Run, run, session};
pub use script::{Kind, Launch, Script, Step};

/// The target's game as an [`Input`]: through its bridge when the target
/// has one, else read directly. `game` is the dump `{game}` names.
pub fn input(target: &Target, game: Option<&Path>) -> Result<Box<dyn Input>> {
    let (app, args, env) = (
        target.app.clone(),
        target.launch_args(game)?,
        target.harness_env()?,
    );
    Ok(match &target.bridge {
        Some(socket) => Box::new(Native::new(app, args, env, socket.clone())),
        None => {
            let log = target.save_dir.as_deref().and_then(mac::log_beside);
            Box::new(Mac::new(app, args, env, target.control.clone()).logging(log))
        }
    })
}

/// A run's own folder, `<kind>/<time>-<name>/` beside the app's live
/// logs.
pub fn folder(kind: &str, name: &str) -> Result<PathBuf> {
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
        .with_file_name(kind)
        .join(format!("{time}-{slug}")))
}
