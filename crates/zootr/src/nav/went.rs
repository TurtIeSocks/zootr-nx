//! What a walk reports: how it ended, why it gave up, and the `use` after it.

use std::fmt;

use serde::Serialize;

use super::act::Used;

/// Why a walk gave up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reason {
    /// No path in the graph.
    NoRoute,
    /// Two blocks at one waypoint: an enemy, a rock.
    Blocked,
    /// A loading zone took Link to another map.
    LeftMap,
    /// A text box or a scene started.
    Event,
    /// Out of time.
    Budget,
}

said!(Reason {
    NoRoute => "no route",
    Blocked => "blocked",
    LeftMap => "left map",
    Event => "event",
    Budget => "budget",
});

/// How a walk ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Done {
    Arrived,
    GaveUp,
    /// Link was moved with `warp` instead of walking.
    Warped,
}

said!(Done {
    Arrived => "arrived",
    GaveUp => "gave up",
    Warped => "warped",
});

/// A walk: how it ended, and the cost.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Went {
    pub result: Done,
    pub reason: Option<Reason>,
    pub secs: f32,
    pub legs: u32,
    /// Flat distance from Link to the goal, in m. Not said once he has left
    /// the goal's map.
    pub left: Option<f32>,
}

impl Went {
    /// Standing at the goal without a step taken.
    pub fn here() -> Self {
        Self {
            result: Done::Arrived,
            reason: None,
            secs: 0.0,
            legs: 0,
            left: Some(0.0),
        }
    }
}

impl fmt::Display for Went {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.result)?;
        if let Some(reason) = self.reason {
            write!(f, " ({reason})")?;
        }
        write!(f, ", {:.1} s, {} legs", self.secs, self.legs)?;
        match self.left {
            Some(left) => write!(f, ", {left:.1} m left"),
            None => Ok(()),
        }
    }
}

/// A walk, and when asked the `use` that followed.
#[derive(Debug, Serialize)]
pub struct Gone {
    #[serde(flatten)]
    pub went: Went,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<Used>,
}

impl fmt::Display for Gone {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.went)?;
        match &self.used {
            Some(used) => write!(f, "\n{used}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_says_what_it_did_in_words_and_json() {
        let went = Went {
            result: Done::GaveUp,
            reason: Some(Reason::NoRoute),
            secs: 3.25,
            legs: 2,
            left: Some(4.5),
        };
        assert_eq!(
            went.to_string(),
            "gave up (no route), 3.2 s, 2 legs, 4.5 m left"
        );
        assert_eq!(
            serde_json::to_string(&went).unwrap(),
            r#"{"result":"gave up","reason":"no route","secs":3.25,"legs":2,"left":4.5}"#
        );
        let went = Went {
            result: Done::Arrived,
            reason: None,
            ..went
        };
        assert!(
            serde_json::to_string(&went)
                .unwrap()
                .contains(r#""reason":null"#)
        );
        // Without a `use` after it, `go` says just the walk.
        let gone = Gone { went, used: None };
        assert_eq!(
            serde_json::to_string(&gone).unwrap(),
            serde_json::to_string(&went).unwrap()
        );
        let away = Went {
            result: Done::GaveUp,
            reason: Some(Reason::LeftMap),
            left: None,
            ..went
        };
        assert_eq!(away.to_string(), "gave up (left map), 3.2 s, 2 legs");
    }
}
