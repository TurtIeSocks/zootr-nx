//! Getting the game ready for a spot: waiting out an event, asking again
//! while one refuses a request, and teleporting to an arrival point that
//! starts no scene.

use std::collections::BTreeSet;
use std::thread::sleep;
use std::time::Instant;

use anyhow::{Result, bail};

use super::{BEAT, CALM};
use crate::dev::{Dev, EVENT_PLAYING, NOT_TAKEN, Place};
use crate::known::{Name, field, map};
use crate::play::Input;

/// How many times a request refused for an event is asked again.
const AGAIN: usize = 3;

/// `ask`, and again after `calm` while it's refused for an event: Navi or
/// an enemy's scene can start between one request and the next.
pub(super) fn calmly<T>(
    mut ask: impl FnMut() -> Result<T>,
    mut calm: impl FnMut() -> Result<()>,
) -> Result<T> {
    for _ in 0..AGAIN {
        match ask() {
            Err(e) if format!("{e:#}").contains(EVENT_PLAYING) => calm()?,
            done => return done,
        }
    }
    ask()
}

/// Whether `e` means the game is gone rather than a request refused.
pub fn gone(e: &anyhow::Error) -> bool {
    let text = format!("{e:#}");
    [
        "didn't answer",
        "closed the bridge",
        "no game is listening",
        "Broken pipe",
    ]
    .iter()
    .any(|t| text.contains(t))
}

/// Wait for no event and no load, pressing A through any text left.
pub(in crate::sweep) fn quiet<I: Input>(dev: &Dev, input: &mut I) -> Result<()> {
    let start = Instant::now();
    loop {
        let s = dev.state()?;
        if !s.event && !s.loading {
            return Ok(());
        }
        if start.elapsed() > CALM {
            bail!("an event from before the spot never ended");
        }
        if s.event {
            input.press("A")?;
        }
        sleep(BEAT);
    }
}

/// Teleport to `place` as `child`, asking again while the game ignores it
/// right after a load.
pub(in crate::sweep) fn teleport<I: Input>(
    dev: &Dev,
    input: &mut I,
    place: &Place,
    child: Option<bool>,
) -> Result<()> {
    let start = Instant::now();
    loop {
        match calmly(|| dev.go_as(place, child), || quiet(dev, &mut *input)) {
            Err(e) if format!("{e:#}").contains(NOT_TAKEN) && start.elapsed() < CALM => sleep(BEAT),
            done => return done.map(drop),
        }
    }
}

/// Arrival points inside the trigger of a scene that plays as Link lands
/// and waits on its get window, so a teleport there never settles and the
/// item is given before the sweep looks: the adult meadow's by the temple,
/// where Sheik teaches his song.
const SCENE_ON_ARRIVAL: [(&Name, &Name); 1] = [(
    map::SACRED_FOREST_MEADOW_ADULT,
    field::OUTSIDE_OF_FOREST_TEMPLE,
)];

/// Where a teleport to `map` arrives: its first arrival point that starts
/// no scene, or its first.
pub(in crate::sweep) fn arrival<'a>(places: &'a BTreeSet<Place>, map: &str) -> Option<&'a Place> {
    let starts_no_scene =
        |pl: &&Place| !(SCENE_ON_ARRIVAL.iter()).any(|(m, s)| **m == *pl.map && **s == *pl.spot);
    let mut of = places.iter().filter(|pl| pl.map == map);
    of.clone().find(starts_no_scene).or_else(|| of.next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_teleport_arrives_where_no_scene_starts() {
        needs_names!();
        let meadow = map::SACRED_FOREST_MEADOW_ADULT.to_string();
        let place = |map: &str, spot: &str| Place {
            map: map.to_string(),
            spot: spot.to_string(),
        };
        let temple = place(&meadow, field::OUTSIDE_OF_FOREST_TEMPLE.get());
        let woods = place(&meadow, "Zz_FakeEntrance");
        let both = BTreeSet::from([temple.clone(), woods.clone()]);
        assert_eq!(arrival(&both, &meadow), Some(&woods));
        let only = BTreeSet::from([temple.clone()]);
        assert_eq!(arrival(&only, &meadow), Some(&temple), "the only way in");
        assert_eq!(arrival(&both, "FakeMap"), None);
    }

    #[test]
    fn a_gone_game_is_told_from_a_refusal() {
        assert!(gone(&anyhow::anyhow!(
            "the game didn't answer mem_read in 30s"
        )));
        assert!(!gone(&anyhow::anyhow!("an event is playing")));
    }

    #[test]
    fn a_request_refused_for_an_event_waits_and_goes_again() {
        let (mut asks, mut calms) = (0, 0);
        let got = calmly(
            || {
                asks += 1;
                match asks {
                    1 | 2 => Err(anyhow::anyhow!("refused: {EVENT_PLAYING}")),
                    _ => Ok(asks),
                }
            },
            || {
                calms += 1;
                Ok(())
            },
        );
        assert_eq!(got.unwrap(), 3);
        assert_eq!(calms, 2);
        let other = calmly(
            || Err::<(), _>(anyhow::anyhow!("a map is loading")),
            || Ok(()),
        );
        assert!(other.is_err(), "only an event is waited out");
    }
}
