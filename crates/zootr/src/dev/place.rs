//! Teleport destinations: the places a scene exit arrives at, which
//! ones are known, and where an age switch lands.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;
use serde::Serialize;

use super::distance;
use crate::known::flag;
use crate::warp::{ADULT, CHILD};
use crate::{warp, world};

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

/// Where each arrival point in `map` is, by its spot's name.
pub fn spots(romfs: &Romfs, map: &str) -> Result<HashMap<String, [f32; 3]>> {
    Ok(warp::placements_in(romfs, map)?
        .into_iter()
        .filter(|p| p.name == **flag::START_POS)
        .filter_map(|p| Some((p.label?, p.pose.at)))
        .collect())
}

/// The other age's version of `map`, when it has one.
pub fn twin(map: &str, child: bool) -> Option<String> {
    let (from, to) = if child {
        (CHILD, ADULT)
    } else {
        (ADULT, CHILD)
    };
    map.strip_suffix(from).map(|base| format!("{base}{to}"))
}

/// Where switching age from `map` lands: the arrival point of its [`twin`]
/// nearest `link`, by where [`spots`] puts each. `None` without a twin,
/// or when `places` lists no arrival point in it: Link swaps where he
/// stands then. Both ages' maps share their coordinates.
pub fn other_age(
    map: &str,
    child: bool,
    places: &BTreeSet<Place>,
    link: [f32; 3],
    spots: &HashMap<String, [f32; 3]>,
) -> Option<Place> {
    let away = |p: &Place| {
        spots
            .get(&p.spot)
            .map_or(f32::INFINITY, |&at| distance(at, link))
    };
    twin(map, child)
        .and_then(|twin| {
            (places.iter().filter(|p| p.map == twin)).min_by(|a, b| away(a).total_cmp(&away(b)))
        })
        .cloned()
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
        needs_names!();
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
        needs_names!();
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
    fn switching_age_lands_on_the_twin_nearest_spot_and_nowhere_without_one() {
        needs_names!();
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
        // The well side is the nearer spot, whatever order the set keeps.
        let spots = HashMap::from([
            (field::KAKARIKO_WELL_SIDE.to_string(), [1.0, 0.0, 0.0]),
            (
                field::KAKARIKO_VILLAGE_ENTRANCE.to_string(),
                [50.0, 0.0, 0.0],
            ),
        ]);
        assert_eq!(
            other_age(
                flag::KAKARIKO_VILLAGE_CHILD,
                true,
                &places,
                [0.0; 3],
                &spots
            ),
            Some(place(&format!(
                "{}/{}",
                flag::KAKARIKO_VILLAGE_ADULT,
                field::KAKARIKO_WELL_SIDE
            )))
        );
        let none = HashMap::new();
        // A twin with no arrival point listed, and a map with no twin.
        let lone = |map, child| other_age(map, child, &places, [0.0; 3], &none);
        assert_eq!(lone(flag::KAKARIKO_VILLAGE_ADULT, false), None);
        assert_eq!(lone(flag::WATER_TEMPLE, false), None);
    }

    #[test]
    fn switching_age_lands_at_the_arrival_point_nearest_link() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let places = places(&romfs).unwrap();
        let spots = spots(&romfs, map::KOKIRI_FOREST_ADULT).unwrap();
        let lost_woods = spots[&**field::KOKIRI_FOREST_FRONT_OF_LOST_WOOD];
        let near = [lost_woods[0] + 3.0, lost_woods[1], lost_woods[2] - 2.0];
        let to = other_age(map::KOKIRI_FOREST_CHILD, true, &places, near, &spots).unwrap();
        assert_eq!(to.map, **map::KOKIRI_FOREST_ADULT);
        assert_eq!(to.spot, **field::KOKIRI_FOREST_FRONT_OF_LOST_WOOD);
    }
}
