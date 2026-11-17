//! `zootr turn`: whose turn it is on the game, or with a queue in the
//! config's `turn`, a place in line for one.

use anyhow::{Result, bail, ensure};

use crate::config::Config;
use crate::turn::{self, Ask, Turn, TurnConfig};

/// Prints whose turn it is, after handing it to `to` when given, by Claude
/// session `session`. With a queue configured, `to` is a step in line.
pub(super) fn turn(
    to: Option<String>,
    note: String,
    session: String,
    force: bool,
    urgent: bool,
    on: String,
) -> Result<()> {
    let config = Config::load()?.turn;
    if config.remote() {
        return queued(&config, to.as_deref(), note, session, urgent, on);
    }
    if let Some(step @ ("join" | "wait" | "reserve" | "serve")) = to.as_deref() {
        bail!("`{step}` needs a queue: set `turn.url` in the config");
    }
    let mut now = Turn::load()?;
    if let Some(to) = to {
        let to = Turn::player(&to)?;
        let claude = turn::from_claude();
        let secs = jiff::Timestamp::now().as_second();
        now = now.hand(to, note, session, secs, claude, force)?;
        now.save()?;
    }
    println!("{now}");
    Ok(())
}

/// One step in the queue's line, then the queue as it stands.
fn queued(
    config: &TurnConfig,
    to: Option<&str>,
    note: String,
    session: String,
    urgent: bool,
    on: String,
) -> Result<()> {
    let claude = turn::from_claude();
    let theirs = |what: &str| -> Result<()> {
        ensure!(!claude, "only the maintainer {what}");
        Ok(())
    };
    match to {
        None => {}
        Some("serve") => return turn::serve::serve(config),
        Some("join") => {
            let place = config.ask(Ask::Join {
                session,
                note,
                urgent,
            })?;
            println!("number {} in line", place.place.unwrap_or_default());
        }
        Some("wait") => {
            let granted = config.ask(Ask::Wait { session })?;
            println!("{}", granted.game.unwrap_or_default());
            return Ok(());
        }
        Some("free") if claude => {
            config.ask(Ask::Free { session })?;
        }
        Some("free") => {
            config.ask(Ask::Leave {
                game: config.here.clone(),
            })?;
        }
        Some("maintainer") => {
            theirs("takes a game for herself")?;
            config.ask(Ask::Take {
                game: config.here.clone(),
            })?;
        }
        Some("reserve") => {
            theirs("keeps a game for a session")?;
            ensure!(!on.is_empty(), "name the game with --on");
            let session = (!session.is_empty()).then_some(session);
            config.ask(Ask::Reserve { game: on, session })?;
        }
        Some("claude") => bail!("with a queue, `join` the line and `wait` for a game"),
        Some(other) => bail!("{other} isn't join, wait, free, reserve, maintainer or serve"),
    }
    let status = config.ask(Ask::Status)?;
    print!("{}", status.queue.unwrap_or_default());
    Ok(())
}
