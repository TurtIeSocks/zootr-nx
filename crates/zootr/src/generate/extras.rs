//! The options only the CLI sets, kept with a seed.

use serde::{Deserialize, Serialize};

use crate::shuffle::Fixed;

/// Options only the CLI sets. Kept with a seed so it reinstalls the same.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Extras {
    /// New-game edits beyond the settings, as `NAME=VALUE`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub defaults: Vec<String>,
    /// Plando: chest hash to item.
    #[serde(skip_serializing_if = "Fixed::is_empty")]
    pub place: Fixed,
    /// Where a new game starts, as `MAP/SPOT`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawn: Option<String>,
    /// Pouch items a new game is handed, by actor.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub give: Vec<String>,
    /// Quest steps the opening moves on, as `CHALLENGE=STEP`, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub progress: Vec<String>,
    /// Placement fields set by hand, as `HASH.PATH=VALUE`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// Install the dev panel's hook. It follows the config at each install
    /// rather than the seed, so it's never kept.
    #[serde(skip)]
    pub dev_panel: bool,
    /// From before Shuffle items was a setting. History moves it there on
    /// load, and it's never written.
    #[serde(skip_serializing)]
    pub(crate) vanilla: bool,
}

impl Extras {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{flag, item, map};

    #[test]
    fn extras_round_trip_and_stay_out_of_plain_seeds() {
        assert_eq!(serde_json::to_string(&Extras::default()).unwrap(), "{}");
        let extras = Extras {
            defaults: vec![format!("IsGet.{}=true", item::BOOMERANG)],
            place: Fixed::from([(u64::MAX, item::HOOKSHOT.to_string())]),
            spawn: Some(format!(
                "{}/{}",
                map::GRAVEYARD_CHILD,
                flag::SHADOW_NOCTURNE
            )),
            give: vec![item::GLASS_TRUTH.to_string()],
            ..Default::default()
        };
        let json = serde_json::to_string(&extras).unwrap();
        assert_eq!(serde_json::from_str::<Extras>(&json).unwrap(), extras);
    }
}
