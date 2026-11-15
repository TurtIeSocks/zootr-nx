//! Teleport destinations: the places a scene exit arrives at, which
//! ones are known, and where an age switch lands.

use std::collections::BTreeSet;
use std::fmt;

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;
use serde::Serialize;

use super::{ADULT, CHILD};
use crate::known::{Name, flag};
use crate::world;

/// Where an age switch lands when the map has no version for the other
/// age: the Temple of Time, where Prelude of Light arrives.
const FALLBACK: (&str, &Name) = ("Market", flag::LIGHT_PRELUDE);

/// A map and an arrival point in it, written `MAP/SPOT`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Place {
    pub map: String,
    pub spot: String,
}

impl Place {
    pub fn parse(text: &str) -> Result<Self> {
        let (map, spot) = text
            .split_once('/')
            .filter(|(m, s)| !m.is_empty() && !s.is_empty())
            .with_context(|| format!("{text} isn't MAP/SPOT"))?;
        Ok(Self {
            map: map.into(),
            spot: spot.into(),
        })
    }

    /// Whether the map is the child one, the adult one, or either.
    pub(super) fn child(&self) -> Option<bool> {
        crate::warp::era(&self.map)
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.map, self.spot)
    }
}

/// Every place a scene exit arrives at, both ages.
pub fn places(romfs: &Romfs) -> Result<BTreeSet<Place>> {
    Ok(world::exits(romfs)?
        .into_iter()
        .filter(|e| !e.to_start.is_empty())
        .map(|e| Place {
            map: e.to,
            spot: e.to_start,
        })
        .collect())
}

/// Refuses `place` unless a scene exit arrives there. The game crashes on a
/// map it doesn't have, and an unknown spot leaves a black screen.
pub fn known(place: &Place, places: &BTreeSet<Place>) -> Result<()> {
    if places.contains(place) {
        return Ok(());
    }
    let close = (places.iter())
        .filter(|p| p.map.starts_with(&place.map) || place.map.starts_with(&p.map))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    match close.is_empty() {
        true => bail!("{place} isn't an arrival point. `zootr probe places` lists them"),
        false => bail!(
            "{place} isn't an arrival point. Close: {}",
            close.join(", ")
        ),
    }
}

/// Where switching age from `map` lands: the other age's version of the
/// map at its first arrival point, else the Temple of Time.
pub fn other_age(map: &str, child: bool, places: &BTreeSet<Place>) -> Place {
    let (from, to) = if child {
        (CHILD, ADULT)
    } else {
        (ADULT, CHILD)
    };
    let other = map.strip_suffix(from).map(|base| format!("{base}{to}"));
    other
        .and_then(|other| places.iter().find(|p| p.map == other).cloned())
        .unwrap_or_else(|| Place {
            map: format!("{}{to}", FALLBACK.0),
            spot: FALLBACK.1.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, map};

    fn place(text: &str) -> Place {
        Place::parse(text).unwrap()
    }

    #[test]
    fn a_place_is_map_slash_spot() {
        let p = place(&format!(
            "{}/{}",
            flag::KAKARIKO_VILLAGE_CHILD,
            field::KAKARIKO_WELL_SIDE
        ));
        assert_eq!(
            p.to_string(),
            format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_CHILD,
                field::KAKARIKO_WELL_SIDE
            )
        );
        assert_eq!(p.child(), Some(true));
        assert_eq!(
            place(&format!("{}/Entrance", flag::WATER_TEMPLE)).child(),
            None
        );
        for bad in ["Kakariko", "/Spot", "Map/"] {
            assert!(Place::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_teleport_goes_only_where_an_exit_arrives() {
        let places = BTreeSet::from([
            place(&format!(
                "{}/{}",
                map::KOKIRI_FOREST_CHILD,
                field::FRONT_DEKU_TREE
            )),
            place(&format!(
                "{}/{}",
                map::KOKIRI_FOREST_CHILD,
                field::KOKIRI_FOREST_FRONT_OF_LOST_WOOD
            )),
            place(&format!("{}/Entrance", flag::WATER_TEMPLE)),
        ]);
        assert!(known(&place(&format!("{}/Entrance", flag::WATER_TEMPLE)), &places).is_ok());
        let err = known(
            &place(&format!(
                "{}/{}",
                flag::KOKIRI_FOREST,
                field::FRONT_DEKU_TREE
            )),
            &places,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "{}/{} isn't an arrival point. Close: \
             {}/{}, {}/{}",
                flag::KOKIRI_FOREST,
                field::FRONT_DEKU_TREE,
                map::KOKIRI_FOREST_CHILD,
                field::FRONT_DEKU_TREE,
                map::KOKIRI_FOREST_CHILD,
                field::KOKIRI_FOREST_FRONT_OF_LOST_WOOD
            )
        );
        let err = known(&place("Nowhere/Spot"), &places).unwrap_err();
        assert!(err.to_string().contains("probe places"), "{err}");
    }

    #[test]
    fn switching_age_keeps_the_map_or_goes_to_the_temple() {
        let places = BTreeSet::from([
            place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_WELL_SIDE
            )),
            place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_VILLAGE_ENTRANCE
            )),
            place(&format!("{}/Entrance", flag::WATER_TEMPLE)),
        ]);
        assert_eq!(
            other_age(flag::KAKARIKO_VILLAGE_CHILD, true, &places),
            place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_VILLAGE_ENTRANCE
            ))
        );
        assert_eq!(
            other_age(flag::KAKARIKO_VILLAGE_ADULT, false, &places),
            place(&format!("{}/{}", flag::MARKET_CHILD, flag::LIGHT_PRELUDE))
        );
        assert_eq!(
            other_age(flag::WATER_TEMPLE, false, &places),
            place(&format!("{}/{}", flag::MARKET_CHILD, flag::LIGHT_PRELUDE))
        );
    }
}
