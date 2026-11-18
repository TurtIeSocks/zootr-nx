//! Using a holder under water: Link sinks to it in the Iron Boots from
//! the surface over it.

use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::act::Used;
use super::drive::{Drive, Sense};
use super::graph::Graph;
use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::gear;
use crate::live::Memory;
use crate::play::Input;
use crate::warp::Placement;

/// How long Link may take to sink to the floor.
const SINKING: Duration = Duration::from_secs(10);
/// How often his height is read as he sinks.
const POLL: Duration = Duration::from_millis(250);
/// He has landed once a poll finds him this little lower, in m.
const LANDED: f32 = 0.05;

/// Use `holder` from where Link floats over it: a stroke to find his
/// heading (the game's doesn't change in water), the Zora Tunic and the
/// Iron Boots on, sink, use it, then the boots off to float up.
pub fn touch_under<M: Memory, I: Input>(
    drive: &mut Drive<'_, Dev<M>, I>,
    gd: &GameData,
    graph: &Graph,
    holder: &Placement,
    shot: PathBuf,
) -> Result<Used> {
    drive.find()?;
    gear::wear(drive.sense, gd, &mut *drive.input, true)?;
    drive.sunk = true;
    land(drive.sense)?;
    let used = drive.touch(graph, holder, shot);
    drive.sunk = false;
    gear::wear(drive.sense, gd, &mut *drive.input, false)?;
    used
}

/// Wait for Link to stop sinking, at most [`SINKING`].
fn land(sense: &impl Sense) -> Result<()> {
    let start = Instant::now();
    let mut was = sense.link()?[1];
    while start.elapsed() < SINKING {
        sleep(POLL);
        let now = sense.link()?[1];
        if was - now < LANDED {
            return Ok(());
        }
        was = now;
    }
    Ok(())
}
