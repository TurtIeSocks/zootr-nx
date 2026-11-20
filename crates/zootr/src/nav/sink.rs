//! Using a holder under water from the surface over it: Link dives onto
//! one he takes by touch, and sinks in the Iron Boots to any other.

use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::act::{self, Touch, Used};
use super::drive::{Drive, Sense};
use super::graph::Graph;
use super::kinds::Act;
use crate::checks::flat;
use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::gear;
use crate::known::item;
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

/// How deep the Gold Scale dives, in m (its `DiveDepth`, the Silver
/// Scale's is 6). Link is given it before a dive.
const DIVE: f32 = 9.0;
/// How long A is held to dive: down [`DIVE`] at the 2.1 m/s Link sinks at
/// most, and a second to take what he lands on. From 4.9 m over Ruto's
/// bottle he dived and took it within the first read (Mac, 2026-11-18).
const DIVING: u64 = 5500;
/// Link floats over a spot he was warped to once he stands this much
/// higher, in m: the game puts a warp into water back on its surface
/// (Mac, 2026-11-18). Lake Hylia's water is no plane, so this is how a
/// holder under it is found.
const FLOATED: f32 = 1.0;
/// How far off a warp's spot Link may float and still be over it, in m.
const OVER: f32 = 2.0;

/// Whether Link, warped to `spot`, floats over it.
pub fn floated(link: [f32; 3], spot: [f32; 3]) -> bool {
    link[1] - spot[1] > FLOATED && flat(link, spot) <= OVER
}

/// Use `holder` from where Link floats over it. One he takes by touch
/// within a dive's reach, he dives onto. For any other: a stroke to find
/// his heading (the game's doesn't change in water), the Zora Tunic and
/// the Iron Boots on, then each side in turn, then the boots off to float
/// up.
pub fn touch_under<M: Memory, I: Input>(
    drive: &mut Drive<'_, Dev<M>, I>,
    gd: &GameData,
    graph: &Graph,
    holder: &Placement,
    shot: PathBuf,
) -> Result<Used> {
    let link = drive.sense.link()?;
    let at = holder.pose.at;
    let deep = link[1] + graph.float - at[1];
    if act::act_of(holder)? == Act::Over && deep <= DIVE {
        drive.sense.give(gd, item::SCALE_GOLD, 1)?;
        drive.sense.warp([at[0], link[1], at[2]])?;
        let wait = std::mem::replace(&mut drive.wait, DRIFT);
        let used = drive.hold_on(holder, "A", DIVING, shot);
        drive.wait = wait;
        return used;
    }
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
pub(crate) fn land(sense: &impl Sense) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warp_into_water_floats_link_up_over_its_spot() {
        let spot = [-151.6, 125.0, 461.0];
        // Ruto's bottle: the game put Link back on the lake's surface.
        assert!(floated([-151.6, 129.9, 461.0], spot));
        assert!(!floated([-151.6, 125.1, 461.0], spot), "he stands there");
        assert!(!floated([-151.6, 120.0, 461.0], spot), "he fell");
        assert!(!floated([-146.0, 129.9, 461.0], spot), "away from it");
    }
}
