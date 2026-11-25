//! The options only the CLI sets, kept with a seed.

use serde::{Deserialize, Serialize};

use crate::ap::ApSeed;
use crate::game::Release;
use crate::shuffle::Fixed;

/// The Colors and Audio panes' drafts, as a seed was asked for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Drafts {
    #[serde(skip_serializing_if = "crate::colors::Draft::is_empty")]
    pub colors: crate::colors::Draft,
    #[serde(skip_serializing_if = "crate::audio::Draft::is_empty")]
    pub audio: crate::audio::Draft,
}

impl Drafts {
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty() && self.audio.is_empty()
    }
}

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
    /// The colors the seed paints, Random already drawn. Cosmetic: left
    /// out of the share code and the seed's icons ([`Self::cosmetic_free`]).
    #[serde(skip_serializing_if = "crate::colors::Colors::is_empty")]
    pub colors: crate::colors::Colors,
    /// The songs the seed plays, each slot dealt. Cosmetic, as colors are.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub audio: crate::audio::Audio,
    /// The colors and music as the panes asked for them, Random still
    /// Random: what drift and Reset to Last Seed compare with. Cosmetic,
    /// as colors are.
    #[serde(skip_serializing_if = "Drafts::is_empty")]
    pub drafts: Drafts,
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
            drafts: Default::default(),
            ..self.clone()
        }
    }

    /// These extras painted and scored as `drafts` asks for `seed`, Random
    /// drawn and the pools dealt, the drafts kept beside them.
    pub fn styled(self, drafts: Drafts, seed: u64) -> anyhow::Result<Self> {
        Ok(Self {
            colors: crate::colors::resolve(&drafts.colors, seed)?,
            audio: crate::audio::resolve(&drafts.audio, seed)?,
            drafts,
            ..self
        })
    }

    /// The drafts the seed was asked for. A seed made before they were
    /// kept answers with the colors and music it drew.
    pub fn asked(&self) -> Drafts {
        match self.drafts.is_empty() {
            false => self.drafts.clone(),
            true => Drafts {
                colors: crate::colors::draft(&self.colors),
                audio: crate::audio::draft(&self.audio),
            },
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

    #[test]
    fn a_seed_keeps_its_drafts_as_asked_and_an_old_one_answers_with_its_draw() {
        use crate::audio::{Pool, PoolMode};
        use crate::colors::{Mode, Pick};
        let drafts = Drafts {
            colors: [("hair".to_string(), Pick::Mode(Mode::Random))].into(),
            audio: crate::audio::Draft {
                pools: [(Pool::Area, PoolMode::Shuffle)].into(),
                ..Default::default()
            },
        };
        let styled = Extras::default().styled(drafts.clone(), 7).unwrap();
        assert!(styled.colors.contains_key("hair"), "Random is drawn");
        assert!(!styled.audio.is_empty(), "the pool is dealt");
        assert_eq!(styled.asked(), drafts, "Random stays Random");
        assert!(styled.cosmetic_free().is_empty(), "the drafts are cosmetic");
        let old = Extras {
            drafts: Drafts::default(),
            ..styled.clone()
        };
        let asked = old.asked();
        assert_eq!(asked.colors, crate::colors::draft(&styled.colors));
        assert_eq!(asked.audio, crate::audio::draft(&styled.audio));
    }
}
