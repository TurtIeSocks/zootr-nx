//! How a spot went: its verdict and the evidence behind it.

use std::path::PathBuf;

use serde::Serialize;

use crate::nav::act::Used;

/// How a spot went.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "verdict", content = "why", rename_all = "snake_case")]
pub enum Verdict {
    /// The dealt item landed and the event ended.
    Pass,
    /// Another item landed: its value's name.
    WrongItem(String),
    /// The item of a neighbouring pot, crate or rock landed: the sword's
    /// swing broke it as well as the holder. The harness's fault, not the
    /// game's, and the dealt item's own holder may have given its item.
    Neighbour(String),
    /// The game gave its stand-in for an item Link can't use yet, as it
    /// does in an unrandomized game: what it gave.
    Substituted(String),
    /// The event ended and no item landed.
    Nothing,
    /// An event played and nothing came of it, after the judge's turns.
    Stall,
    /// The game quit or stopped answering.
    Crash(String),
    /// The judge named one.
    Bug(String),
    /// The harness couldn't set the spot up. Its fault, not the game's.
    SetupFailed(String),
    /// Out of scope, or no trigger known.
    NotSwept(String),
}

impl Verdict {
    /// Whether the game needs a restart before the next spot.
    pub fn broke(&self) -> bool {
        matches!(self, Verdict::Crash(_) | Verdict::Stall | Verdict::Bug(_))
    }
}

/// A spot's verdict and its evidence.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Swept {
    pub verdict: Option<Verdict>,
    /// The judge's answers, in order.
    pub judge: Vec<String>,
    pub shots: Vec<PathBuf>,
    /// What changed in GameData, one line each.
    pub changes: Vec<String>,
    /// How the walk to the holder and its use went, for a spot opened or
    /// talked to.
    pub used: Option<Used>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts_serialize_with_their_reason() {
        let v = serde_json::to_value(Verdict::WrongItem("IsGet.Item_A".into())).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"verdict": "wrong_item", "why": "IsGet.Item_A"})
        );
        let v = serde_json::to_value(Verdict::Neighbour("Item_A".into())).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"verdict": "neighbour", "why": "Item_A"})
        );
        let v = serde_json::to_value(Verdict::Pass).unwrap();
        assert_eq!(v, serde_json::json!({"verdict": "pass"}));
        assert!(Verdict::Stall.broke() && !Verdict::Nothing.broke());
    }
}
