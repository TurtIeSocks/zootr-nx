//! Talking to `zootr turn serve`: one JSON line asked, one JSON line
//! answered, over TCP. A `wait` keeps its connection open until its grant.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use super::queue::Queue;

/// The `turn` part of the config: where the queue is, and the games it
/// hands out. Without a `url`, the turn stays the local `turn.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TurnConfig {
    /// The service, as `HOST:PORT`.
    pub url: String,
    /// A file holding the service's key.
    pub key_file: PathBuf,
    /// The games' names, in the order waiters are offered them.
    pub games: Vec<String>,
    /// The game on this machine: the one the maintainer's play takes.
    pub here: String,
    /// Set only where the service runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serve: Option<ServeConfig>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServeConfig {
    /// `ADDRESS:PORT` to listen on.
    pub listen: String,
    /// Seconds a hold lasts with no game command, and a sweep with no
    /// word to the queue (it asks at each row): past either, it's gone.
    pub lease: i64,
    /// Seconds a sweep gets to finish its row before it counts as gone:
    /// past the longest row (324 s on 2026-11-17).
    pub yield_cap: i64,
    /// Where the queue is kept between restarts.
    pub state: PathBuf,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            listen: String::new(),
            lease: 15 * 60,
            yield_cap: 7 * 60,
            state: PathBuf::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Ask {
    Status,
    Join {
        session: String,
        note: String,
        urgent: bool,
        #[serde(default)]
        both: bool,
        #[serde(default)]
        on: Option<String>,
    },
    Wait {
        session: String,
    },
    Free {
        session: String,
    },
    Renew {
        session: String,
        /// This machine's game, named back when the session holds it.
        #[serde(default)]
        here: String,
        /// Only a read of the game: check the hold, leave its lease alone.
        /// A server from before it renews anyway.
        #[serde(default)]
        look: bool,
    },
    Take {
        game: String,
    },
    Leave {
        game: String,
    },
    Reserve {
        game: String,
        session: Option<String>,
    },
    SweepStart {
        game: String,
        name: String,
    },
    SweepYields {
        game: String,
        name: String,
    },
    SweepDone {
        game: String,
        name: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Request {
    pub key: String,
    #[serde(flatten)]
    pub ask: Ask,
}

/// What the service answers: a game's name, a place in line, a yes or
/// no, or the whole queue.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Answer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Queue>,
}

/// How long a quick question gets before the service counts as down.
pub(super) const QUICK: Duration = Duration::from_secs(10);

impl TurnConfig {
    /// Nothing set: left out of the config file.
    pub fn is_unset(&self) -> bool {
        *self == Self::default()
    }

    /// Whether a queue service is configured.
    pub fn remote(&self) -> bool {
        !self.url.is_empty()
    }

    pub(super) fn key(&self) -> Result<String> {
        let key = std::fs::read_to_string(&self.key_file)
            .with_context(|| format!("reading the turn key {}", self.key_file.display()))?;
        Ok(key.trim().to_owned())
    }

    /// The service's first address that answers within [`QUICK`].
    fn connect(&self) -> std::io::Result<TcpStream> {
        let mut last = None;
        for addr in self.url.to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, QUICK) {
                Ok(stream) => return Ok(stream),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::ErrorKind::NotFound.into()))
    }

    /// Ask the service, failing with its error when it refuses. Only the
    /// service not answering fails with an `io::Error` in the chain, which
    /// is what counts as unreachable.
    pub fn ask(&self, ask: Ask) -> Result<Answer> {
        let quick = !matches!(ask, Ask::Wait { .. } | Ask::SweepStart { .. });
        // Flattened, so a missing key file isn't taken for the service down.
        let key = self.key().map_err(|e| anyhow!("{e:#}"))?;
        let stream = self
            .connect()
            .with_context(|| format!("reaching the turn queue at {}", self.url))?;
        stream.set_write_timeout(Some(QUICK))?;
        if quick {
            stream.set_read_timeout(Some(QUICK))?;
        }
        let mut line = serde_json::to_string(&Request { key, ask })?;
        line.push('\n');
        (&stream).write_all(line.as_bytes())?;
        let mut reply = String::new();
        if BufReader::new(&stream).read_line(&mut reply)? == 0 {
            let closed = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
            return Err(closed).with_context(|| format!("the turn queue at {} hung up", self.url));
        }
        let answer: Answer = serde_json::from_str(&reply)
            .with_context(|| format!("the turn queue answered {reply:?}"))?;
        match answer.error {
            Some(e) => Err(anyhow!(e)),
            None => Ok(answer),
        }
    }
}
