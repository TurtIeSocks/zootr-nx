//! Whose turn it is on the game: the maintainer's, Claude's, or nobody's.
//! Kept in a file beside the config, so it outlives any one session.
//! Claude plays only on its own turn, and never takes the maintainer's.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
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

    /// The turn handed to `to`. From Claude's shell, the maintainer's turn
    /// can only be handed back by her, unless `force`.
    pub fn hand(
        &self,
        to: Option<Player>,
        note: String,
        now: i64,
        claude: bool,
        force: bool,
    ) -> Result<Self> {
        if claude
            && !force
            && self.player == Some(Player::Maintainer)
            && to != Some(Player::Maintainer)
        {
            bail!(
                "it's the maintainer's turn: only she hands it over (from her own terminal, or with --force)"
            );
        }
        Ok(Self {
            player: to,
            since: now,
            note,
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
            Some(Player::Claude) => write!(f, "Claude's turn since {since}")?,
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
        }
    }

    #[test]
    fn claude_never_takes_the_maintainers_turn() {
        let hers = held(Some(Player::Maintainer));
        assert!(
            hers.hand(Some(Player::Claude), String::new(), 2, true, false)
                .is_err()
        );
        assert!(hers.hand(None, String::new(), 2, true, false).is_err());
        assert!(
            hers.hand(Some(Player::Claude), String::new(), 2, true, true)
                .is_ok()
        );
        assert!(
            hers.hand(Some(Player::Claude), String::new(), 2, false, false)
                .is_ok()
        );
    }

    #[test]
    fn claude_takes_a_free_turn_and_hands_its_own_back() {
        let free = held(None).hand(Some(Player::Claude), "notes".into(), 2, true, false);
        let mine = free.unwrap();
        assert_eq!((mine.player, mine.since), (Some(Player::Claude), 2));
        assert!(
            mine.hand(Some(Player::Maintainer), String::new(), 3, true, false)
                .is_ok()
        );
        assert!(mine.hand(None, String::new(), 3, true, false).is_ok());
    }

    #[test]
    fn a_missing_note_stays_out_of_the_file() {
        let json = serde_json::to_string(&held(Some(Player::Claude))).unwrap();
        assert_eq!(json, r#"{"player":"claude","since":1}"#);
        assert_eq!(Turn::default().to_string(), "free");
    }
}
