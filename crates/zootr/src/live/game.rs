//! The running game, attached for reads and writes, and the errors that say
//! why it can't be.

use std::path::Path;

use anyhow::{Result, anyhow, bail};

use super::{Bridge, Memory, Places, global, in_order, mach, read, set};
use crate::config::Target;
use crate::gamedata::GameData;
use crate::known::flag;
use crate::patch::OFFSETS;

/// The running game, attached for reads and writes.
pub struct Game {
    mem: Backend,
    pub(super) global: u64,
    places: Places,
}

/// How a [`Game`] reaches the game's memory.
enum Backend {
    /// The game's task, read directly.
    Mach(u32),
    /// The target's control socket.
    Bridge(Bridge),
}

/// Where a target with a bridge loads the game's `main`.
pub const NATIVE_MAIN: u64 = 0x8002_0000;

impl Memory for Game {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        match &self.mem {
            Backend::Mach(task) => mach::read(*task, at, len),
            Backend::Bridge(b) => b.read(at, len),
        }
    }

    fn write(&self, at: u64, data: &[u8]) -> Result<()> {
        match &self.mem {
            Backend::Mach(task) => mach::write(*task, at, data),
            Backend::Bridge(b) => b.write(at, data),
        }
    }

    fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        match &self.mem {
            Backend::Mach(task) => (ranges.iter())
                .map(|&(at, len)| mach::read(*task, at, len))
                .collect(),
            Backend::Bridge(b) => b.read_many(ranges),
        }
    }

    fn batch(&self, writes: &[(u64, &[u8])], reads: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        match &self.mem {
            Backend::Mach(_) => in_order(self, writes, reads),
            Backend::Bridge(b) => b.batch(writes, reads),
        }
    }

    fn places(&self) -> Option<&Places> {
        Some(&self.places)
    }
}

/// The OS refused to let this build read the game. The one attach error
/// that waiting for the game won't fix.
#[derive(Debug)]
pub struct NotSigned {
    pub pid: i32,
}

impl std::fmt::Display for NotSigned {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let exe = std::env::current_exe().unwrap_or_default();
        write!(
            f,
            "this build isn't signed for live reads of process {}\n\
             Build with `make zootr`, or sign this binary:\n  \
             codesign -f -s - --entitlements crates/zootr/live.entitlements {}",
            self.pid,
            exe.display()
        )
    }
}

impl std::error::Error for NotSigned {}

/// Live reads need macOS, or a target with a bridge. Like [`NotSigned`],
/// waiting won't fix it.
#[derive(Debug)]
pub struct Unsupported;

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("live reads need macOS, or a target with a bridge")
    }
}

impl std::error::Error for Unsupported {}

/// What Live says while the game is up but can't be read yet.
pub const STARTING: &str = "Waiting for the game to finish starting";
/// What Live says while the game doesn't answer in time.
pub const SLOW: &str = "The game isn't answering yet";

/// `raw`, under Live's word for a game still starting.
fn starting(raw: anyhow::Error) -> anyhow::Error {
    raw.context(crate::plain::Plain(STARTING.into()))
}

impl Game {
    /// The target's running game: through its bridge when it has one,
    /// else among the processes started from its app.
    pub fn attach(target: &Target) -> Result<Self> {
        match &target.bridge {
            Some(socket) => Self::over(Bridge::connect(socket).map_err(starting)?),
            None => Self::find(&target.app),
        }
    }

    /// The game behind `bridge`, whose GameData global sits at a
    /// fixed offset from `main`.
    pub fn over(bridge: Bridge) -> Result<Self> {
        let global = NATIVE_MAIN + u64::from(OFFSETS.got.gamedata);
        bridge.u64(global)?;
        Ok(Self {
            mem: Backend::Bridge(bridge),
            global,
            places: Places::default(),
        })
    }

    fn find(app: &Path) -> Result<Self> {
        let pids = mach::pids(app)?;
        if pids.is_empty() {
            return Err(starting(anyhow!(
                "the game isn't running from {}",
                app.display()
            )));
        }
        // The game runs as two processes, and the OS may refuse one of them
        // while the other, the one holding GameData, lets this build in.
        let mut refused = None;
        for pid in pids {
            let Some(task) = mach::task(pid) else {
                refused.get_or_insert(pid);
                continue;
            };
            for (at, size) in mach::code(task) {
                let Ok(code) = mach::read(task, at, size) else {
                    continue;
                };
                // The system's shared library cache, never the game.
                if code.starts_with(b"dyld") {
                    continue;
                }
                if let Some(global) = global(&code, at) {
                    return Ok(Self {
                        mem: Backend::Mach(task),
                        global,
                        places: Places::default(),
                    });
                }
            }
        }
        if let Some(pid) = refused {
            return Err(NotSigned { pid }.into());
        }
        Err(starting(anyhow!(
            "{} wasn't found in the game; is this the 1.1.1 update?",
            flag::GAME_DATA
        )))
    }
}

impl Game {
    pub fn global(&self) -> u64 {
        self.global
    }

    /// The current value, as the Live tab shows it. See [`read`].
    pub fn get(&self, gd: &GameData, hash: u32) -> Result<String> {
        read(self, gd, self.global, hash)
    }

    /// See [`set`].
    pub fn set(&self, hash: u32, value: &str) -> Result<()> {
        set(self, self.global, hash, value)
    }

    /// The game's mapped regions: start, size and protection (read 1,
    /// write 2, execute 4). Read directly only.
    pub fn regions(&self) -> Result<Vec<(u64, usize, i32)>> {
        match &self.mem {
            Backend::Mach(task) => Ok(mach::regions(*task)),
            Backend::Bridge(_) => bail!("the bridge can't list regions"),
        }
    }
}
