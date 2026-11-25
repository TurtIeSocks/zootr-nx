//! The options only the CLI sets, kept with a seed.

use serde::{Deserialize, Serialize};

use crate::ap::ApSeed;
use crate::game::Release;
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
    /// Dungeon doors pinned by hand, as `DOOR=DUNGEON`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub doors: Vec<String>,
    /// An Archipelago seed: its placements stand in for the shuffle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ap: Option<ApSeed>,
    /// The build the seed is made for, from `shuffle --release`; the
    /// dump's own build without it. Not a setting: within a build every
    /// seed's hook is the same.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<Release>,
    /// The colours the seed paints, Random already drawn. Cosmetic: left
    /// out of the share code and the seed's icons ([`Self::cosmetic_free`]).
    #[serde(skip_serializing_if = "crate::colors::Colors::is_empty")]
    pub colors: crate::colors::Colors,
    /// The songs the seed plays, each slot dealt. Cosmetic, as colours are.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub audio: crate::audio::Audio,
    /// From before Shuffle items was a setting. History moves it there on
    /// load, and it's never written.
    #[serde(skip_serializing)]
    pub(crate) vanilla: bool,
}

impl Extras {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// These extras without what only changes how the seed looks: what a
    /// share code carries and the seed's icons hash.
    pub fn cosmetic_free(&self) -> Self {
        Self {
            colors: Default::default(),
            audio: Default::default(),
            ..self.clone()
        }
    }

    /// An Archipelago seed's, with nothing else.
    pub fn ap(ap: ApSeed) -> Self {
        Self {
            ap: Some(ap),
            ..Self::default()
        }
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
            release: Some(Release::V100),
            ..Default::default()
        };
        let json = serde_json::to_string(&extras).unwrap();
        assert_eq!(serde_json::from_str::<Extras>(&json).unwrap(), extras);
    }
}
