//! `zootr turn`: whose turn it is on the game.

use anyhow::Result;

use crate::turn::Turn;

/// Prints whose turn it is, after handing it to `to` when given, by Claude
/// session `session`.
pub(super) fn turn(to: Option<String>, note: String, session: String, force: bool) -> Result<()> {
    let mut now = Turn::load()?;
    if let Some(to) = to {
        let to = Turn::player(&to)?;
        let claude = std::env::var_os("CLAUDECODE").is_some();
        let secs = jiff::Timestamp::now().as_second();
        now = now.hand(to, note, session, secs, claude, force)?;
        now.save()?;
    }
    println!("{now}");
    Ok(())
}
