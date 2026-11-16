//! Whose turn it is on the game: the maintainer's, Claude's, or nobody's.
//! Kept in a file beside the config, so it outlives any one session.
//! Claude plays only on its own turn, and never takes the maintainer's.
//! Each Claude session names itself, so one session's turn isn't taken
//! for another's.

use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Player {
    Maintainer,
    Claude,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    /// `None` when the game is free.
    pub player: Option<Player>,
    /// Unix seconds.
    pub since: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The Claude session holding a Claude turn, as it named itself.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session: String,
}

/// Whether zootr runs from Claude's shell, where turns are enforced.
pub fn from_claude() -> bool {
    std::env::var_os("CLAUDECODE").is_some()
}

impl Turn {
    pub fn path() -> Result<PathBuf> {
        Ok(Config::path()?.with_file_name("turn.json"))
    }

    /// The saved turn, or a free game when there is none.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        match std::fs::read(&path) {
            Ok(b) => {
                serde_json::from_slice(&b).with_context(|| format!("reading {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
    }

    /// `claude`, `maintainer` or `free`.
    pub fn player(name: &str) -> Result<Option<Player>> {
        Ok(match name {
            "claude" => Some(Player::Claude),
            "maintainer" => Some(Player::Maintainer),
            "free" => None,
            _ => bail!("{name} isn't claude, maintainer or free"),
        })
    }

    /// Refuses a Claude shell (`claude`) unless the turn is Claude
    /// session `session`'s.
    pub fn ensure_held(&self, session: &str, claude: bool) -> Result<()> {
        ensure!(
            !claude
                || (self.player == Some(Player::Claude)
                    && self.session == session
                    && !session.is_empty()),
            "the game isn't this session's to play ({self}): take a free turn with `zootr turn claude --session NAME`"
        );
        Ok(())
    }

    /// The turn handed to `to` by Claude session `session`. From Claude's
    /// shell, unless `force`: the maintainer's turn can only be handed back
    /// by her, a Claude turn only by the session holding it, and Claude
    /// takes a turn only naming its session.
    pub fn hand(
        &self,
        to: Option<Player>,
        note: String,
        session: String,
        now: i64,
        claude: bool,
        force: bool,
    ) -> Result<Self> {
        if claude && !force {
            if self.player == Some(Player::Maintainer) && to != Some(Player::Maintainer) {
                bail!(
                    "it's the maintainer's turn: only she hands it over (from her own terminal, or with --force)"
                );
            }
            if self.player == Some(Player::Claude) && self.session != session {
                bail!(
                    "it's another Claude session's turn ({self}): only that session hands it over (or --force)"
                );
            }
            if to == Some(Player::Claude) && session.is_empty() {
                bail!(
                    "name this Claude session with --session, so other sessions know the turn isn't theirs"
                );
            }
        }
        let session = match to {
            Some(Player::Claude) => session,
            _ => String::new(),
        };
        Ok(Self {
            player: to,
            since: now,
            note,
            session,
        })
    }
}

impl std::fmt::Display for Turn {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let since = jiff::Timestamp::from_second(self.since)
            .map(|t| {
                t.to_zoned(jiff::tz::TimeZone::system())
                    .strftime("%b %d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        match self.player {
            None if self.since == 0 => write!(f, "free")?,
            None => write!(f, "free since {since}")?,
            Some(Player::Maintainer) => write!(f, "the maintainer's turn since {since}")?,
            Some(Player::Claude) if self.session.is_empty() => {
                write!(f, "Claude's turn since {since}")?
            }
            Some(Player::Claude) => write!(f, "Claude's turn ({}) since {since}", self.session)?,
        }
        match self.note.is_empty() {
            true => Ok(()),
            false => write!(f, ": {}", self.note),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(player: Option<Player>) -> Turn {
        Turn {
            player,
            since: 1,
            note: String::new(),
            session: String::new(),
        }
    }

    fn claudes(session: &str) -> Turn {
        Turn {
            session: session.into(),
            ..held(Some(Player::Claude))
        }
    }

    #[test]
    fn only_the_session_holding_the_turn_plays_from_claudes_shell() {
        assert!(claudes("fake-mine").ensure_held("fake-mine", true).is_ok());
        assert!(
            claudes("fake-other")
                .ensure_held("fake-mine", true)
                .is_err()
        );
        assert!(claudes("").ensure_held("", true).is_err());
        assert!(held(None).ensure_held("fake-mine", true).is_err());
        assert!(
            held(Some(Player::Maintainer))
                .ensure_held("fake-mine", true)
                .is_err()
        );
        assert!(
            held(Some(Player::Maintainer))
                .ensure_held("", false)
                .is_ok()
        );
    }

    #[test]
    fn claude_never_takes_the_maintainers_turn() {
        let hers = held(Some(Player::Maintainer));
        assert!(
            hers.hand(
                Some(Player::Claude),
                String::new(),
                "fake-mine".into(),
                2,
                true,
                false
            )
            .is_err()
        );
        assert!(
            hers.hand(None, String::new(), "fake-mine".into(), 2, true, false)
                .is_err()
        );
        assert!(
            hers.hand(
                Some(Player::Claude),
                String::new(),
                "fake-mine".into(),
                2,
                true,
                true
            )
            .is_ok()
        );
        assert!(
            hers.hand(
                Some(Player::Claude),
                String::new(),
                String::new(),
                2,
                false,
                false
            )
            .is_ok()
        );
    }

    #[test]
    fn claude_takes_a_free_turn_and_hands_its_own_back() {
        let free = held(None).hand(
            Some(Player::Claude),
            "notes".into(),
            "fake-mine".into(),
            2,
            true,
            false,
        );
        let mine = free.unwrap();
        assert_eq!((mine.player, mine.since), (Some(Player::Claude), 2));
        assert!(
            mine.hand(
                Some(Player::Maintainer),
                String::new(),
                "fake-mine".into(),
                3,
                true,
                false
            )
            .is_ok()
        );
        assert!(
            mine.hand(None, String::new(), "fake-mine".into(), 3, true, false)
                .is_ok()
        );
    }

    #[test]
    fn a_session_hands_over_only_its_own_turn() {
        let theirs = claudes("fake-other");
        let to = |to, session: &str, force| {
            theirs.hand(to, String::new(), session.into(), 2, true, force)
        };
        assert!(to(None, "fake-mine", false).is_err());
        assert!(to(Some(Player::Claude), "fake-mine", false).is_err());
        assert!(to(None, "", false).is_err());
        assert!(to(None, "fake-other", false).is_ok());
        assert!(to(Some(Player::Claude), "fake-mine", true).is_ok());
        assert!(
            theirs
                .hand(None, String::new(), String::new(), 2, false, false)
                .is_ok()
        );
    }

    #[test]
    fn a_turn_without_a_session_is_handed_back_without_one() {
        let old = claudes("");
        assert!(
            old.hand(None, String::new(), String::new(), 2, true, false)
                .is_ok()
        );
        assert!(
            old.hand(None, String::new(), "fake-mine".into(), 2, true, false)
                .is_err()
        );
    }

    #[test]
    fn claude_takes_a_turn_only_naming_its_session() {
        let free = held(None);
        let take = |session: &str| {
            free.hand(
                Some(Player::Claude),
                String::new(),
                session.into(),
                2,
                true,
                false,
            )
        };
        assert!(take("").is_err());
        let mine = take("fake-mine").unwrap();
        assert_eq!(mine.session, "fake-mine");
        assert_eq!(
            mine.to_string().split(" since").next(),
            Some("Claude's turn (fake-mine)")
        );
    }

    #[test]
    fn a_turn_handed_away_from_claude_drops_the_session() {
        let back =
            claudes("fake-mine").hand(None, String::new(), "fake-mine".into(), 2, true, false);
        assert_eq!(back.unwrap().session, "");
    }

    #[test]
    fn an_old_file_without_a_session_loads() {
        let old: Turn = serde_json::from_str(r#"{"player":"claude","since":1}"#).unwrap();
        assert_eq!(old, claudes(""));
    }

    #[test]
    fn a_missing_note_stays_out_of_the_file() {
        let json = serde_json::to_string(&held(Some(Player::Claude))).unwrap();
        assert_eq!(json, r#"{"player":"claude","since":1}"#);
        assert_eq!(Turn::default().to_string(), "free");
    }
}
