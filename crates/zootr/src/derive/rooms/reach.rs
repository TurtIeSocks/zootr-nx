//! What hits a gold skulltula: the weapons whose range reaches it from
//! its room's floor.

use crate::nav::graph::{Graph, dist};
use crate::nav::limits::limits;

/// How far from a gold skulltula derive looks for floor: past the longest
/// range.
pub(super) const REACH_MAX: f32 = 30.0;

/// Below this height a sword or a bomb may hit a gold skulltula too,
/// which no playtest has settled, so its rule is a guess there.
const SKULLTULA_LOW: f32 = 2.5;

/// Din's Fire, by its `limits.toml` name.
const DINS: &str = "dins-fire";

/// What hits a gold skulltula from its room's floor.
pub(super) struct Reach {
    pub needs: String,
    pub why: String,
    pub guess: bool,
}

/// The weapons whose range reaches the gold skulltula at `at` from the
/// nearest floor `stands` keeps: its room's, where Link can stand. Its
/// token falls to the floor, so the hit is the whole check.
pub(super) fn skulltula_reach(
    graph: &Graph,
    stands: impl Fn(usize) -> bool,
    at: [f32; 3],
) -> Reach {
    let far = |why: String| Reach {
        needs: "gold-skulltula".into(),
        why,
        guess: true,
    };
    let Some(floor) = graph.nearest_floor(at, REACH_MAX, &stands) else {
        return far(format!("no floor of its room within {REACH_MAX} m"));
    };
    let d = dist(floor.at, at);
    // Din's Fire hits nothing from water: its hit test counts the water
    // and starts at Link's feet (the maintainer's playtest, 2026-11-22,
    // missed from the shallows 5.6 m under one).
    let dry = graph.nearest_floor(at, REACH_MAX, |f| stands(f) && !graph.wet(graph.centers[f]));
    let dins = dry.is_some_and(|f| limits().reaching(dist(f.at, at)).contains(&DINS));
    let mut words: Vec<_> = (limits().reaching(d).into_iter())
        .filter(|w| *w != DINS)
        .collect();
    words.extend(dins.then_some(DINS));
    if words.is_empty() {
        return far(format!("{d:.1} m from its room's floor, past every weapon"));
    }
    let needs = words.join(" or ");
    let mut why = format!("{d:.1} m from its room's floor, in each one's range");
    let low = d < SKULLTULA_LOW;
    if low {
        why.push_str("; a sword or a bomb may too, untested");
    }
    Reach {
        needs,
        why,
        guess: low,
    }
}
