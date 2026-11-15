//! Where a warp beside a placed actor puts Link: in front of it, by its
//! placement in the map's banc files.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Result, ensure};
use clap::ValueEnum;
use ounce_formats::{Byml, Romfs};
use serde::{Deserialize, Serialize};

use crate::checks::{GYAML, actors, banc_files, hash_of, load_banc, str_field, vec3};
use crate::config::TestSetup;
use crate::dev::{ADULT, CHILD, Dev, Place};
use crate::known::{Name, field, flow};
use crate::live::Memory;
use crate::world::{Presence, presence_of};

/// How far from the actor, and above where it stands, Link lands.
const AHEAD: f32 = 2.0;
const LIFT: f32 = 0.5;

/// Which side of the actor Link lands on, by its facing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Front,
    Back,
    Left,
    Right,
}

/// Where a placed actor stands, and its `Rotate` angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub at: [f32; 3],
    pub rotate: [f32; 3],
}

/// Each map that places `actor`, a name or a placement hash, with its first
/// placement there. Maps are named by banc file: `KokiriForest_Child_Static`
/// is `KokiriForest_Child`.
pub fn placements(romfs: &Romfs, actor: &str) -> Result<BTreeMap<String, Pose>> {
    let hash = actor.parse::<u64>().ok();
    let found = first_where(romfs, |a| {
        str_field(a, GYAML) == Some(actor) || hash.is_some() && hash_of(a) == hash
    })?;
    Ok(found
        .into_iter()
        .map(|(map, (_, pose))| (map, pose))
        .collect())
}

/// Each map with an `EventRequestActor` that starts `event`, with its
/// placement hash and pose.
pub fn requested(romfs: &Romfs, event: &str) -> Result<BTreeMap<String, (Option<u64>, Pose)>> {
    first_where(romfs, |a| {
        let dynamic = a.as_map().ok().and_then(|m| m.get("Dynamic"));
        str_field(a, GYAML) == Some(REQUEST)
            && dynamic.and_then(|d| str_field(d, field::REQUEST_EVENT_NAME)) == Some(event)
    })
}

/// The actor that starts an event its placement names.
const REQUEST: &Name = flow::EVENT_REQUEST_ACTOR;

/// Each map with a placement `pick` takes, with the first one's hash and
/// pose.
fn first_where(
    romfs: &Romfs,
    pick: impl Fn(&Byml) -> bool,
) -> Result<BTreeMap<String, (Option<u64>, Pose)>> {
    let mut found = BTreeMap::new();
    for rel in banc_files(romfs)? {
        let doc = load_banc(romfs, &rel)?;
        if let Some(a) = actors(&doc.root)?.iter().find(|a| pick(a)) {
            found
                .entry(map_of(&rel).to_owned())
                .or_insert((hash_of(a), pose_of(a)));
        }
    }
    Ok(found)
}

fn pose_of(actor: &Byml) -> Pose {
    Pose {
        at: vec3(actor, "Translate").unwrap_or_default(),
        rotate: vec3(actor, "Rotate").unwrap_or_default(),
    }
}

/// One placement: its actor's name, its instance hash, its pose and the
/// flag it exists by.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub name: String,
    pub hash: Option<u64>,
    /// The `Name` it carries apart from its actor's, like a sign's
    /// `GoronCity_000`.
    pub label: Option<String>,
    pub pose: Pose,
    /// `None` when it's always there.
    pub presence: Option<Presence>,
}

/// Every placement of the banc files `keep` picks, by map. A placement in
/// both of a map's files keeps the first.
fn placed_by_map(
    romfs: &Romfs,
    keep: impl Fn(&str) -> bool,
) -> Result<BTreeMap<String, Vec<Placement>>> {
    let mut found: BTreeMap<String, Vec<Placement>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for rel in banc_files(romfs)?.iter().filter(|rel| keep(rel)) {
        let map = map_of(rel);
        let doc = load_banc(romfs, rel)?;
        for a in actors(&doc.root)? {
            let hash = hash_of(a);
            if hash.is_none_or(|h| seen.insert((map.to_owned(), h))) {
                found.entry(map.to_owned()).or_default().push(Placement {
                    name: str_field(a, GYAML).unwrap_or_default().to_owned(),
                    hash,
                    label: str_field(a, LABEL).map(str::to_owned),
                    pose: pose_of(a),
                    presence: presence_of(a),
                });
            }
        }
    }
    Ok(found)
}

/// The placements the placement `hash` in `map` links to.
pub fn linked(romfs: &Romfs, map: &str, hash: u64) -> Result<Vec<u64>> {
    for rel in banc_files(romfs)?.iter().filter(|rel| map_of(rel) == map) {
        let doc = load_banc(romfs, rel)?;
        let Some(actor) = actors(&doc.root)?.iter().find(|a| hash_of(a) == Some(hash)) else {
            continue;
        };
        let dst = |link: &Byml| link.as_map().ok()?.get("Dst")?.as_int::<u64>().ok();
        let links = actor
            .as_map()?
            .get("Links")
            .map(Byml::as_array)
            .transpose()?;
        return Ok(links.unwrap_or_default().iter().filter_map(dst).collect());
    }
    Ok(vec![])
}

/// Every placement of `map`.
pub fn placements_in(romfs: &Romfs, map: &str) -> Result<Vec<Placement>> {
    let mut found = placed_by_map(romfs, |rel| map_of(rel) == map)?;
    Ok(found.remove(map).unwrap_or_default())
}

/// Each map of the Banc folder `area` with the pose of every actor it
/// places, by instance hash.
pub fn poses_in(romfs: &Romfs, area: &str) -> Result<BTreeMap<String, HashMap<u64, Pose>>> {
    let found = placed_by_map(romfs, |rel| rel.split('/').nth(1) == Some(area))?;
    Ok(found
        .into_iter()
        .map(|(map, placed)| {
            let poses = placed.into_iter().filter_map(|p| Some((p.hash?, p.pose)));
            (map, poses.collect())
        })
        .collect())
}

/// A placement's own name field.
const LABEL: &str = "Name";

/// Every named placement of `map` with a hash, by actor, then label, then
/// hash: each one its own warp, since a warp by actor name takes the first
/// (the maintainer, 2026-11-15: Goron City's signs were one entry).
pub fn actors_in(romfs: &Romfs, map: &str) -> Result<Vec<Placement>> {
    let mut found: Vec<Placement> = placements_in(romfs, map)?
        .into_iter()
        .filter(|p| !p.name.is_empty() && p.hash.is_some())
        .collect();
    found.sort_by(|a, b| (&a.name, &a.label, a.hash).cmp(&(&b.name, &b.label, b.hash)));
    Ok(found)
}

/// Whether `map` is a child copy, an adult copy, or neither.
pub fn era(map: &str) -> Option<bool> {
    if map.ends_with(CHILD) {
        Some(true)
    } else {
        map.ends_with(ADULT).then_some(false)
    }
}

pub(crate) fn map_of(rel: &str) -> &str {
    let stem = rel.rsplit('/').next().unwrap_or(rel);
    let stem = stem.split('.').next().unwrap_or(stem);
    stem.rsplit_once('_').map_or(stem, |(map, _)| map)
}

/// `actor`'s placement in `map`, refusing one `map` doesn't place and
/// naming the maps that do.
pub fn placed<'a>(found: &'a BTreeMap<String, Pose>, map: &str, actor: &str) -> Result<&'a Pose> {
    ensure!(
        !found.is_empty(),
        "{actor} isn't placed in any map. `zootr probe actors` lists placed actors"
    );
    found.get(map).ok_or_else(|| {
        let maps: Vec<_> = found.keys().cloned().collect();
        anyhow::anyhow!(
            "{actor} isn't placed in {map}. Maps that place it: {}",
            maps.join(", ")
        )
    })
}

/// The spot `AHEAD` from `pose` on `side`, and `LIFT` above it. Its
/// forward is local `+Z` turned by its angles, X then Y then Z, flattened
/// to the ground. The game turns the other way from the textbook: a
/// quarter turn about `+Y` swings `+Z` to `-X` (chests turned that way
/// open only from there, on the native build). Left is a quarter turn from it, right handed with `+Y`
/// up. One facing straight up or down lands on the spot itself.
pub fn landing(pose: &Pose, side: Side) -> [f32; 3] {
    landing_at(pose, side, AHEAD)
}

/// [`landing`], `ahead` from `pose` instead of `AHEAD`.
pub fn landing_at(pose: &Pose, side: Side, ahead: f32) -> [f32; 3] {
    let [x, y, z] = pose.rotate;
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    let (fx, fz) = (sx * sz - cx * sy * cz, cx * cy);
    let flat = fx.hypot(fz);
    let (ux, uz) = if flat < 1e-3 {
        (0.0, 0.0)
    } else {
        (fx / flat * ahead, fz / flat * ahead)
    };
    let (dx, dz) = match side {
        Side::Front => (ux, uz),
        Side::Back => (-ux, -uz),
        Side::Left => (uz, -ux),
        Side::Right => (-uz, ux),
    };
    let [px, py, pz] = pose.at;
    [px + dx, py + LIFT, pz + dz]
}

/// Actors whose model turns the other way round from the rest, the
/// textbook way: the big chest. On the other machine's sweep, 10 of the 11
/// big chests that opened stood Link there, at every turn, small chests
/// never.
const MIRRORED: [&str; 1] = ["TBox_A_02"];

/// `pose` as `actor` faces by it: turned the other way round for a
/// [`MIRRORED`] model.
pub fn facing(actor: &str, pose: &Pose) -> Pose {
    let [x, y, z] = pose.rotate;
    match MIRRORED.iter().any(|m| actor.contains(m)) {
        true => Pose {
            at: pose.at,
            rotate: [x, -y, z],
        },
        false => *pose,
    }
}

/// The sides in the order they're tried, front first, then left, right
/// and back.
const SIDES: [Side; 4] = [Side::Front, Side::Left, Side::Right, Side::Back];

/// The sides to try, `first` first, then the rest in [`SIDES`] order.
pub fn sides_from(first: Side) -> impl Iterator<Item = Side> {
    std::iter::once(first).chain(SIDES.into_iter().filter(move |&s| s != first))
}

/// Where a warp to `side` of `actor` in `map` puts Link.
pub fn spot(romfs: &Romfs, map: &str, actor: &str, side: Side) -> Result<[f32; 3]> {
    let found = placements(romfs, actor)?;
    Ok(landing(placed(&found, map, actor)?, side))
}

/// Warp Link to `side` of `actor` in the map he's in, or to `actor` read
/// as a spot `x,y,z`, saying what it did.
pub fn beside<M: Memory>(game: &Dev<M>, romfs: &Romfs, actor: &str, side: Side) -> Result<String> {
    let at = match point(actor) {
        Some(at) => at,
        None => spot(romfs, &game.state()?.map, actor, side)?,
    };
    Ok(format!("{} beside {actor}", game.warp(at)?))
}

/// `x,y,z` as a spot.
fn point(text: &str) -> Option<[f32; 3]> {
    let parts: Vec<f32> = text
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

/// Where a test setup's `near` and `side` put Link: in its spawn's map,
/// else the map `now` reads. `None` without a `near`.
pub fn near(
    romfs: &Romfs,
    setup: &TestSetup,
    now: impl FnOnce() -> Result<String>,
) -> Result<Option<[f32; 3]>> {
    let Some(actor) = &setup.near else {
        return Ok(None);
    };
    let map = match &setup.spawn {
        Some(spawn) => Place::parse(spawn)?.map,
        None => now()?,
    };
    spot(romfs, &map, actor, setup.side.unwrap_or_default()).map(Some)
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, field, flag, flow, map};
    use std::f32::consts::{FRAC_PI_2, PI};

    use super::*;
    use crate::game::dev_dump;

    fn near_enough(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn a_banc_file_names_its_map() {
        let at = |rel: &str| map_of(rel).to_owned();
        assert_eq!(
            at(&format!(
                "Banc/{}/KokiriForest_Child_Static.bcett.byml.zs",
                flag::KOKIRI_FOREST
            )),
            map::KOKIRI_FOREST_CHILD
        );
        assert_eq!(
            at(&format!(
                "Banc/{}/HyruleCastle_Dynamic.bcett.byml.zs",
                flag::HYRULE_CASTLE
            )),
            flag::HYRULE_CASTLE
        );
    }

    #[test]
    fn a_map_names_its_age_when_it_has_one() {
        assert_eq!(era(map::KOKIRI_FOREST_CHILD), Some(true));
        assert_eq!(era(flag::KAKARIKO_VILLAGE_ADULT), Some(false));
        assert_eq!(era(flag::HYRULE_CASTLE), None);
    }

    #[test]
    fn a_folders_actors_are_found_by_hash_in_each_map() {
        let Some(romfs) = dev_dump() else { return };
        let zelda = actor::NPC_HYLIA_ZELDA_CHILD;
        let maps = poses_in(&romfs, flag::HYRULE_CASTLE).unwrap();
        let poses = &maps[&**flag::HYRULE_CASTLE];
        let shown = [5.1, 127.8, -256.5];
        let at = |p: &Pose| p.at.iter().zip(shown).all(|(a, b)| (a - b).abs() < 0.1);
        assert!(poses.values().any(at));
        let found = placements(&romfs, zelda).unwrap();
        assert!(poses.values().any(|p| *p == found[&**flag::HYRULE_CASTLE]));
        assert!(poses_in(&romfs, "Nowhere").unwrap().is_empty());
    }

    #[test]
    fn a_warp_finds_an_actor_by_its_placement_hash() {
        let Some(romfs) = dev_dump() else { return };
        // The Bottom of the Well's Blue Rupee chest.
        let found = placements(&romfs, "631709185320708287").unwrap();
        assert_eq!(found.keys().collect::<Vec<_>>(), [flow::BOTTOM_OF_THE_WELL]);
        assert!(placements(&romfs, "1").unwrap().is_empty());
    }

    #[test]
    fn link_lands_on_the_side_asked_for() {
        let pose = Pose {
            at: [1.0, 2.0, 3.0],
            rotate: [0.0; 3],
        };
        let cases = [
            (Side::Front, [1.0, 2.5, 5.0]),
            (Side::Back, [1.0, 2.5, 1.0]),
            (Side::Left, [3.0, 2.5, 3.0]),
            (Side::Right, [-1.0, 2.5, 3.0]),
        ];
        for (side, want) in cases {
            let got = landing(&pose, side);
            assert!(near_enough(got, want), "{side:?}: {got:?}");
        }
    }

    #[test]
    fn link_lands_in_front_of_the_actor_by_its_facing() {
        let pose = |rotate| Pose {
            at: [1.0, 2.0, 3.0],
            rotate,
        };
        let cases = [
            ([0.0; 3], [1.0, 2.5, 5.0]),
            ([0.0, FRAC_PI_2, 0.0], [-1.0, 2.5, 3.0]),
            ([0.0, -FRAC_PI_2, 0.0], [3.0, 2.5, 3.0]),
            // Zelda's angles: a half turn however the order reads them.
            ([PI, 0.0, -PI], [1.0, 2.5, 1.0]),
            ([-FRAC_PI_2, 0.0, 0.0], [1.0, 2.5, 3.0]),
        ];
        for (rotate, want) in cases {
            let got = landing(&pose(rotate), Side::Front);
            assert!(near_enough(got, want), "{rotate:?}: {got:?}");
        }
    }

    #[test]
    fn a_big_chest_turns_the_other_way_round() {
        let pose = Pose {
            at: [1.0, 2.0, 3.0],
            rotate: [0.0, FRAC_PI_2, 0.0],
        };
        let big = landing(&facing(actor::OBJ_T_BOX_A_02_NO_BAKE, &pose), Side::Front);
        assert!(near_enough(big, [3.0, 2.5, 3.0]), "{big:?}");
        let small = landing(&facing(actor::OBJ_T_BOX_A_01, &pose), Side::Front);
        assert!(near_enough(small, [-1.0, 2.5, 3.0]), "{small:?}");
        // Unturned, both face +Z.
        let ahead = Pose {
            rotate: [0.0; 3],
            ..pose
        };
        assert_eq!(facing(actor::OBJ_T_BOX_A_02, &ahead), ahead);
    }

    #[test]
    fn a_map_lists_each_placement_of_an_actor_apart() {
        let Some(romfs) = dev_dump() else { return };
        let city = actors_in(&romfs, map::GORON_CITY_CHILD).unwrap();
        let signs: Vec<_> = (city.iter())
            .filter(|p| p.name == actor::OBJ_SIGN_BOARD_A_01)
            .collect();
        assert!(signs.len() > 1, "{signs:?}");
        let hole = signs
            .iter()
            .find(|p| p.label.as_deref() == Some(field::GORON_CITY_000));
        assert_eq!(hole.unwrap().hash, Some(5027760269036258092));
        assert!(actors_in(&romfs, "Nowhere").unwrap().is_empty());
    }

    #[test]
    fn a_warp_refuses_an_actor_the_map_doesnt_place() {
        let Some(romfs) = dev_dump() else { return };
        let zelda = actor::NPC_HYLIA_ZELDA_CHILD;
        let found = placements(&romfs, zelda).unwrap();
        let pose = placed(&found, flag::HYRULE_CASTLE, zelda).unwrap();
        let shown = [5.1, 127.8, -256.5];
        assert!(
            pose.at.iter().zip(shown).all(|(a, b)| (a - b).abs() < 0.1),
            "{pose:?}"
        );
        let err = placed(&found, map::KOKIRI_FOREST_CHILD, zelda).unwrap_err();
        assert!(
            err.to_string()
                .contains(&format!("Maps that place it: {}", flag::HYRULE_CASTLE)),
            "{err}"
        );
        let none = placements(&romfs, "Npc_Nobody").unwrap();
        let err = placed(&none, flag::HYRULE_CASTLE, "Npc_Nobody").unwrap_err();
        assert!(err.to_string().contains("isn't placed in any map"), "{err}");
    }
}
