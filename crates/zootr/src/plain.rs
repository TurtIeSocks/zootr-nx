//! [`Plain`]: what an error means, in a player's words, over the developer
//! chain under it.

use std::fmt;

/// An error's meaning for someone playing, with what to do about it,
/// attached as context over the raw chain. The CLI prints the whole chain;
/// the app shows this sentence and keeps the chain behind Details.
#[derive(Debug)]
pub struct Plain(pub String);

impl fmt::Display for Plain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Plain {}

/// `e`'s player wording, if any, and its chain without it.
pub fn split(e: &anyhow::Error) -> Option<(&str, String)> {
    let plain = e.downcast_ref::<Plain>()?.0.as_str();
    let raw: Vec<String> = (e.chain())
        .map(|c| c.to_string())
        .filter(|c| c != plain)
        .collect();
    Some((plain, raw.join(": ")))
}

/// Between an error's player wording and its chain, in [`shown`]'s text.
/// The app splits there and keeps the chain behind Details.
pub const DETAILS: &str = "\n\n";

/// `e` as the app shows it: the player wording, then [`DETAILS`] and the
/// chain, or just the chain when it has none.
pub fn shown(e: &anyhow::Error) -> String {
    match split(e) {
        Some((plain, raw)) => format!("{plain}{DETAILS}{raw}"),
        None => format!("{e:#}"),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Context, anyhow};

    use super::*;

    #[test]
    fn splits_the_plain_words_from_the_chain_under_them() {
        let e = anyhow!("bad magic")
            .context("opening a.nup")
            .context(Plain("Choose the game".into()))
            .context("saving");
        assert_eq!(
            split(&e),
            Some(("Choose the game", "saving: opening a.nup: bad magic".into()))
        );
        assert_eq!(split(&anyhow!("bad magic").context("opening")), None);
        assert_eq!(
            shown(&e),
            "Choose the game\n\nsaving: opening a.nup: bad magic"
        );
        assert_eq!(shown(&anyhow!("raw").context("doing")), "doing: raw");
    }

    #[test]
    fn a_result_takes_it_like_any_context() {
        let r: anyhow::Result<()> = Err(anyhow!("raw")).context(Plain("Say it".into()));
        assert_eq!(format!("{:#}", r.unwrap_err()), "Say it: raw");
    }
}
