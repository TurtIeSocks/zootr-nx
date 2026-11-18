//! Using a holder under water: Link sinks to it in the Iron Boots from
//! the surface over it.

use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::act::{self, Touch, Used};
use super::drive::{Drive, Sense};
use super::graph::Graph;
use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::gear;
use crate::live::Memory;
use crate::play::Input;
use crate::warp::{self, Placement};

/// How long Link may take to sink to the floor.
const SINKING: Duration = Duration::from_secs(10);
/// How often his height is read as he sinks.
const POLL: Duration = Duration::from_millis(250);
/// He has landed once a poll finds him this little lower, in m.
const LANDED: f32 = 0.05;
/// How long to watch for what a use under water started: a box's rupee
/// sinks slowly, and its get came after the 2 s a use on land watches
/// (Mac, 2026-11-18).
const DRIFT: Duration = Duration::from_secs(6);

/// Use `holder` from where Link floats over it: a stroke to find his
/// heading (the game's doesn't change in water), the Zora Tunic and the
/// Iron Boots on, then each side in turn, then the boots off to float up.
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
    let wait = std::mem::replace(&mut drive.wait, DRIFT);
    let used = each_side(drive, graph, holder, shot);
    drive.wait = wait;
    drive.sunk = false;
    gear::wear(drive.sense, gd, &mut *drive.input, false)?;
    used
}

/// Warp over each side's stand spot, sink to it and use `holder` from
/// there, until something comes of it. Under water no side is known to
/// have floor (Hyrule Field has no navmesh), and a box's front faced the
/// grotto pond's wall: the camera couldn't get behind Link, and his roll
/// went into the wall (Mac, 2026-11-18). The back broke it.
fn each_side<M: Memory, I: Input>(
    drive: &mut Drive<'_, Dev<M>, I>,
    graph: &Graph,
    holder: &Placement,
    shot: PathBuf,
) -> Result<Used> {
    let act = act::act_of(holder)?;
    let then = std::mem::take(&mut drive.then);
    drive.here = true;
    let mut used = None;
    for side in warp::ranked(drive.side, &then).collect::<Vec<_>>() {
        let (_, spot) = act::stand_from(graph, holder, act, drive.sense.link()?, side, &[]);
        drive.sense.warp(graph.above(spot).unwrap_or(spot))?;
        land(drive.sense)?;
        drive.side = side;
        let tried = drive.touch(graph, holder, shot.clone())?;
        let done = tried.result != Touch::Nothing;
        used = Some(tried);
        if done {
            break;
        }
    }
    Ok(used.expect("every holder has a side"))
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
