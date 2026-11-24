//! Jabu-Jabu in the adult fountain (the Adult Jabu-Jabu setting). The adult
//! map has no Jabu, no door to him and no arrival from his belly, so each
//! comes from the child map, read fresh each seed.

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, Romfs};

use crate::known::{field, flag, map};
use crate::settings::AdultJabu;
use crate::world::{self, Era};
use crate::{checks, foliage};

/// The child fountain's pieces adult Link needs. The logic groups naming
/// any of them come too, with everything those name: without the group the
/// mouth never opens and the door never fires.
const PIECES: [u64; 8] = [
    // Jabu, and the door's event request actor.
    9239523456607912768,
    4228704915231375030,
    // Where the door's camera and Link stand.
    3372489875717634129,
    14553022542402878271,
    // Link's arrival from the belly, and the belly's preload.
    578417708135220760,
    6879349138535148736,
    // The mouth's air wall and look target.
    12260267020908237353,
    7976482752136033709,
];

/// What stands inside Jabu at [`SPOT`]: the adult fountain's ruined arch
/// (two pillars and the arch, their rock down to the lakebed, the moss and
/// the bird on it). Gone in both modes: it's only in open water off the ice,
/// and the arch's pieces already carry a gate of their own.
const INSIDE: [u64; 20] = [
    // The pillars and the arch, and the rubble on top.
    3842539641451415813,
    6157457917480935380,
    14734606843902991473,
    16386602682060958005,
    3116079511899512267,
    // Their rock under the water.
    14906850302399543825,
    6514813006399998093,
    17986971035168378473,
    // Moss.
    1884835471922331659,
    2833940166543067386,
    5040705624368652861,
    6568009078182563173,
    10586363024981577611,
    11787932250316574784,
    13068366260814417931,
    13847096215753232196,
    14879023508391645629,
    15485030191422503411,
    16814576077885727546,
    // A bird perched on the arch.
    6333070843951403617,
];

/// The Octoroks Jabu would swallow at [`SPOT`], moved to open water the
/// maintainer picked, each kept at its own height. The fountain keeps all
/// four.
const MOVED: [(u64, [f32; 2]); 2] = [
    // The one by his head.
    (3305620433631243129, [36.93, -45.94]),
    // The one in his body.
    (10454006740117513252, [32.06, -8.83]),
];

/// Where the arch stood, low corner and high corner. The moss, turf, stones
/// and plants painted on it are foliage, not actors, so they stay floating
/// when the arch goes unless this box strips them too.
const ARCH: ([f32; 3], [f32; 3]) = ([44.0, 90.0, -14.0], [60.0, 135.0, 0.0]);

/// The folder holding the fountain's foliage, which [`crate::game::SUBTREES`]
/// extracts.
pub const FOLIAGE: &str = "Foliage/Placement/Scene/ZoraFountain";

/// Where Jabu floats in the adult fountain: his centre on the ground plane
/// and his turn from the child spot, in radians. In the child spot he would
/// stand on the ice path to the Ice Cavern and the heart piece on the ice.
/// Every copy turns and moves with him, so his door stays on his mouth.
/// The maintainer picked it live: his mouth by the ice, his fins just clear
/// of the cliff, and room to swim past his tail.
pub struct Spot {
    pub at: [f32; 2],
    pub turn: f32,
}

pub const SPOT: Spot = Spot {
    at: [47.53, -18.92],
    turn: (-34.71_f32).to_radians(),
};

/// The arrival lists every map's spots, in the base game and the DLC.
const START_POS: [&str; 2] = [
    "Banc/BancBasedResource/StartPos.byml.zs",
    "AoC1/Banc/BancBasedResource/StartPos.byml.zs",
];

/// Copies Jabu's pieces into `rel` when it's one of the adult fountain's
/// files. After Morpha, each copy without a gate of its own stands only
/// once the Water sage is done, the gate the fountain's rubble already has.
pub fn copy(romfs: &Romfs, rel: &str, root: &mut Byml, mode: AdultJabu) -> Result<bool> {
    let adult = map::ZORA_FOUNTAIN_ADULT.get();
    if mode == AdultJabu::Off || !rel.contains(&format!("{adult}_")) {
        return Ok(false);
    }
    let razed = make_room(root)?;
    let child = checks::load_banc(romfs, &rel.replace(adult, map::ZORA_FOUNTAIN_CHILD.get()))?;
    let mut wanted: HashSet<u64> = PIECES.into();
    let groups: Vec<Byml> = (child.root.as_map()?.get("AiGroups"))
        .map(|g| g.as_array())
        .transpose()?
        .unwrap_or_default()
        .iter()
        .filter(|g| references(g).iter().any(|h| wanted.contains(h)))
        .cloned()
        .collect();
    groups.iter().for_each(|g| wanted.extend(references(g)));
    let mut copies: Vec<Byml> = (checks::actors(&child.root)?.iter())
        .filter(|a| checks::hash_of(a).is_some_and(|h| wanted.contains(&h)))
        .cloned()
        .collect();
    if copies.is_empty() && groups.is_empty() {
        return Ok(razed);
    }
    let have: HashSet<u64> = checks::actors(root)?
        .iter()
        .filter_map(checks::hash_of)
        .collect();
    if let Some(clash) = copies
        .iter()
        .filter_map(checks::hash_of)
        .find(|h| have.contains(h))
    {
        bail!("{rel} already has {clash}, which Jabu's copy brings");
    }
    let centre = centre(romfs)?;
    for actor in &mut copies {
        place(actor, "Translate", "Rotate", centre, &SPOT)?;
    }
    if mode == AdultJabu::AfterMorpha {
        for actor in copies.iter_mut().filter(|a| !gated(a)) {
            checks::gate(actor, flag::SAGE_OF_WATER_IS_COMPLETED_EXP.get(), false)?;
        }
    }
    let map = root.as_mut_map()?;
    if !groups.is_empty() {
        let list = map
            .entry("AiGroups".into())
            .or_insert_with(|| Byml::Array(Vec::new()));
        list.as_mut_array()?.extend(groups);
    }
    let list = map
        .entry("Actors".into())
        .or_insert_with(|| Byml::Array(Vec::new()));
    list.as_mut_array()?.extend(copies);
    Ok(true)
}

/// The copied door, as logic reads the data's exits. It stands in every
/// seed's world: the word `adult-jabu-jabu` in its rule shuts it while the
/// setting is off.
pub fn door() -> world::Exit {
    world::Exit {
        from: map::ZORA_FOUNTAIN_ADULT.get().into(),
        era: Era::Adult,
        to: map::INSIDE_JABU_JABU_BELLY.get().into(),
        to_start: field::INSIDE_JABU_JABU_BELLY_FROM_ZORA_FOUNTAIN
            .get()
            .into(),
        name: String::new(),
        presence: "-".into(),
    }
}

/// Jabu's centre in the child fountain, which every copy turns about.
fn centre(romfs: &Romfs) -> Result<[f32; 3]> {
    let child = format!("{}_Static", map::ZORA_FOUNTAIN_CHILD.get());
    let rel = (checks::banc_files(romfs)?.into_iter())
        .find(|rel| rel.contains(&child))
        .with_context(|| format!("no {child}"))?;
    let doc = checks::load_banc(romfs, &rel)?;
    (checks::actors(&doc.root)?.iter())
        .find(|a| checks::hash_of(a) == Some(PIECES[0]))
        .and_then(|a| checks::vec3(a, checks::TRANSLATE))
        .with_context(|| format!("no Jabu in {rel}"))
}

/// Turns `node` about `centre` by the spot's turn, then moves it as far as
/// the centre moves.
fn place(node: &mut Byml, at: &str, rot: &str, centre: [f32; 3], spot: &Spot) -> Result<()> {
    let (sin, cos) = spot.turn.sin_cos();
    let map = node.as_mut_map()?;
    if let Some(pos) = map.get_mut(at) {
        let v = triple(pos, at)?;
        let (dx, dz) = (v[0].as_float()? - centre[0], v[2].as_float()? - centre[2]);
        v[0] = Byml::Float(spot.at[0] + dx * cos + dz * sin);
        v[2] = Byml::Float(spot.at[1] - dx * sin + dz * cos);
    }
    if spot.turn == 0.0 {
        return Ok(());
    }
    let Some(r) = map.get_mut(rot) else {
        return Ok(());
    };
    let r = triple(r, rot)?;
    // Rotate is X·Y·Z: a turn about the world's Y adds to Y while X is 0,
    // and comes off it while X is a half turn (X then flips Y).
    let rx = r[0].as_float()?;
    let turn = match rx.abs() {
        a if a < 0.05 => spot.turn,
        a if (a - std::f32::consts::PI).abs() < 0.05 => -spot.turn,
        _ => bail!("a copy tilts {rx} about X, so a turn about Y isn't one of its angles"),
    };
    r[1] = Byml::Float(r[1].as_float()? + turn);
    Ok(())
}

/// `v`, an X, Y, Z array kept under `key`.
fn triple<'a>(v: &'a mut Byml, key: &str) -> Result<&'a mut [Byml; 3]> {
    let v = v.as_mut_array()?;
    let len = v.len();
    <&mut [Byml; 3]>::try_from(v.as_mut_slice())
        .map_err(|_| anyhow::anyhow!("a {key} has {len} entries, not 3"))
}

/// Takes [`INSIDE`] out of `root` and moves [`MOVED`]. Whether any of
/// either was there.
fn make_room(root: &mut Byml) -> Result<bool> {
    let Some(list) = root.as_mut_map()?.get_mut("Actors") else {
        return Ok(false);
    };
    let list = list.as_mut_array()?;
    let before = list.len();
    list.retain(|a| !checks::hash_of(a).is_some_and(|h| INSIDE.contains(&h)));
    let mut changed = list.len() < before;
    for actor in list.iter_mut() {
        let Some(&(_, to)) = MOVED
            .iter()
            .find(|(h, _)| checks::hash_of(actor) == Some(*h))
        else {
            continue;
        };
        let at = (actor.as_mut_map()?.get_mut(checks::TRANSLATE))
            .context("a moved Octorok has no spot")?;
        let at = triple(at, checks::TRANSLATE)?;
        at[0] = Byml::Float(to[0]);
        at[2] = Byml::Float(to[1]);
        changed = true;
    }
    Ok(changed)
}

/// The actors a logic group names.
fn references(group: &Byml) -> Vec<u64> {
    (group.as_map().ok().and_then(|m| m.get("References")))
        .and_then(|r| r.as_array().ok())
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_map().ok()?.get("Reference")?.as_int::<u64>().ok())
        .collect()
}

fn gated(actor: &Byml) -> bool {
    actor.as_map().is_ok_and(|m| m.contains_key("Presence"))
}

/// Adds the arrival from Jabu's belly to the adult fountain, or walking out
/// lands at the arrival from Zora's Domain.
pub fn arrival(romfs: &Romfs, builder: &mut ModBuilder, mode: AdultJabu) -> Result<()> {
    if mode == AdultJabu::Off {
        return Ok(());
    }
    let spot = field::ZORA_FOUNTAIN_FROM_INSIDE_JABU_JABU_BELLY.get();
    let mut written = 0;
    for rel in START_POS {
        let Ok(bytes) = romfs.read(rel) else { continue };
        let mut doc = BymlDoc::parse(&bytes).with_context(|| format!("parsing {rel}"))?;
        add_arrival(&mut doc.root, spot, centre(romfs)?, &SPOT).with_context(|| rel.to_owned())?;
        builder.write(rel, &doc.to_binary())?;
        written += 1;
    }
    ensure!(written > 0, "no {} in the dump", START_POS[0]);
    Ok(())
}

/// Copies the child fountain's `spot` to the adult fountain's, where Jabu
/// floats there.
fn add_arrival(root: &mut Byml, spot: &str, centre: [f32; 3], place_at: &Spot) -> Result<()> {
    let maps = root.as_mut_map()?;
    let child = map::ZORA_FOUNTAIN_CHILD.get();
    let mut pos = (maps
        .get(child)
        .and_then(|m| m.as_map().ok()?.get(spot))
        .cloned())
    .with_context(|| format!("no {spot} under {child}"))?;
    place(&mut pos, "Trans", "Rot", centre, place_at)?;
    let adult = map::ZORA_FOUNTAIN_ADULT.get();
    (maps.get_mut(adult).with_context(|| format!("no {adult}"))?)
        .as_mut_map()?
        .insert(spot.into(), pos);
    Ok(())
}

/// Strips the foliage where the arch stood, in each of the adult
/// fountain's foliage files.
pub fn strip_foliage(romfs: &Romfs, builder: &mut ModBuilder, mode: AdultJabu) -> Result<()> {
    if mode == AdultJabu::Off {
        return Ok(());
    }
    for rel in foliage_files(romfs)? {
        let (bytes, gone) =
            foliage::strip(&romfs.read(&rel)?, in_arch).with_context(|| rel.clone())?;
        if gone > 0 {
            builder.write(&rel, &bytes)?;
        }
    }
    Ok(())
}

fn in_arch(p: [f32; 3]) -> bool {
    (0..3).all(|a| ARCH.0[a] <= p[a] && p[a] <= ARCH.1[a])
}

/// The adult fountain's foliage files, one per kind (moss, turf, ...).
fn foliage_files(romfs: &Romfs) -> Result<Vec<String>> {
    let lead = format!("{}.", map::ZORA_FOUNTAIN_ADULT.get());
    let dir = romfs.root().join(FOLIAGE);
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with(&lead) && name.ends_with(".flnt.zs") {
            files.push(format!("{FOLIAGE}/{name}"));
        }
    }
    files.sort();
    ensure!(!files.is_empty(), "no foliage in {FOLIAGE}");
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adult fountain's map file holding `part` (`Static`, `Dynamic`).
    fn adult_file(romfs: &Romfs, part: &str) -> String {
        let name = format!("{}_{part}", map::ZORA_FOUNTAIN_ADULT.get());
        (checks::banc_files(romfs).unwrap().into_iter())
            .find(|rel| rel.contains(&name))
            .unwrap()
    }

    fn copied(romfs: &Romfs, rel: &str, mode: AdultJabu) -> (bool, Byml) {
        let mut root = checks::load_banc(romfs, rel).unwrap().root;
        (copy(romfs, rel, &mut root, mode).unwrap(), root)
    }

    fn hashes(root: &Byml) -> HashSet<u64> {
        checks::actors(root)
            .unwrap()
            .iter()
            .filter_map(checks::hash_of)
            .collect()
    }

    #[test]
    fn each_mode_copies_jabu_into_the_adult_fountain_or_not() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (fixed, dynamic) = (adult_file(&romfs, "Static"), adult_file(&romfs, "Dynamic"));
        assert!(!copied(&romfs, &fixed, AdultJabu::Off).0);
        let (changed, root) = copied(&romfs, &fixed, AdultJabu::Always);
        assert!(changed);
        let (_, dynamic_root) = copied(&romfs, &dynamic, AdultJabu::Always);
        let have: HashSet<u64> = &hashes(&root) | &hashes(&dynamic_root);
        assert!(
            PIECES.iter().all(|p| have.contains(p)),
            "every piece copied"
        );
        let groups = root.as_map().unwrap().get("AiGroups").unwrap();
        assert!(
            groups
                .as_array()
                .unwrap()
                .iter()
                .any(|g| references(g).contains(&PIECES[1]))
        );
        // Copying twice brings the same hashes again.
        let mut twice = root.clone();
        assert!(copy(&romfs, &fixed, &mut twice, AdultJabu::Always).is_err());
    }

    #[test]
    fn after_morpha_gates_each_copy_on_the_water_sage() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let sage = flag::SAGE_OF_WATER_IS_COMPLETED_EXP.get();
        for part in ["Static", "Dynamic"] {
            let rel = adult_file(&romfs, part);
            let vanilla = hashes(&checks::load_banc(&romfs, &rel).unwrap().root);
            let (_, root) = copied(&romfs, &rel, AdultJabu::AfterMorpha);
            for actor in checks::actors(&root).unwrap() {
                if vanilla.contains(&checks::hash_of(actor).unwrap()) {
                    continue;
                }
                let presence = actor.as_map().unwrap().get("Presence").unwrap();
                let flag = checks::str_field(presence, field::FLAG_NAME).unwrap();
                // The mouth's air wall keeps its own gate.
                let mouth = checks::hash_of(actor) == Some(PIECES[6]);
                assert_eq!(flag == sage, !mouth, "{rel}: {actor:?}");
            }
        }
    }

    #[test]
    fn the_adult_fountain_gets_the_arrival_from_jabus_belly() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(START_POS[0]).unwrap()).unwrap();
        let spot = field::ZORA_FOUNTAIN_FROM_INSIDE_JABU_JABU_BELLY.get();
        let adult = |root: &Byml| {
            let maps = root.as_map().unwrap();
            let adult = maps.get(map::ZORA_FOUNTAIN_ADULT.get()).unwrap();
            adult.as_map().unwrap().get(spot).cloned()
        };
        assert!(adult(&doc.root).is_none());
        // Jabu left where he is: the arrival is the child's.
        let centre = [34.8, 104.1, -24.4];
        let stay = Spot {
            at: [centre[0], centre[2]],
            turn: 0.0,
        };
        add_arrival(&mut doc.root, spot, centre, &stay).unwrap();
        let child = doc
            .root
            .as_map()
            .unwrap()
            .get(map::ZORA_FOUNTAIN_CHILD.get())
            .unwrap();
        assert_eq!(adult(&doc.root).as_ref(), child.as_map().unwrap().get(spot));
    }

    #[test]
    fn what_stands_inside_jabu_goes_with_the_setting_on() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (mut kept, mut razed) = (HashSet::new(), HashSet::new());
        for part in ["Dynamic", "Static"] {
            let rel = adult_file(&romfs, part);
            kept.extend(hashes(&copied(&romfs, &rel, AdultJabu::Off).1));
            razed.extend(hashes(&copied(&romfs, &rel, AdultJabu::Always).1));
        }
        assert!(
            INSIDE.iter().all(|h| kept.contains(h)),
            "a hash in INSIDE isn't in the map"
        );
        assert!(!INSIDE.iter().any(|h| razed.contains(h)));
    }

    #[test]
    fn the_octoroks_jabu_would_swallow_move_to_open_water() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rel = adult_file(&romfs, "Dynamic");
        let spots = |root: &Byml| -> Vec<[f32; 3]> {
            let actors = checks::actors(root).unwrap();
            (MOVED.iter())
                .map(|(h, _)| {
                    let a = actors
                        .iter()
                        .find(|a| checks::hash_of(a) == Some(*h))
                        .unwrap();
                    checks::vec3(a, checks::TRANSLATE).unwrap()
                })
                .collect()
        };
        let before = spots(&copied(&romfs, &rel, AdultJabu::Off).1);
        let after = spots(&copied(&romfs, &rel, AdultJabu::Always).1);
        for ((was, now), (_, to)) in before.iter().zip(&after).zip(MOVED) {
            assert_eq!([now[0], now[2]], to);
            assert_eq!(now[1], was[1], "each keeps its height");
        }
    }

    #[test]
    fn the_foliage_where_the_arch_stood_goes() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut gone = 0;
        for rel in foliage_files(&romfs).unwrap() {
            let (bytes, n) = foliage::strip(&romfs.read(&rel).unwrap(), in_arch).unwrap();
            gone += n;
            // A second pass finds nothing left standing in the box.
            assert_eq!(foliage::strip(&bytes, in_arch).unwrap().1, 0, "{rel}");
        }
        assert!(gone > 0, "the arch had foliage");
    }

    #[test]
    fn each_copy_turns_and_moves_with_jabu() {
        let centre = [10.0, 0.0, 0.0];
        // A quarter turn, Jabu moved to the origin.
        let spot = Spot {
            at: [0.0, 0.0],
            turn: std::f32::consts::FRAC_PI_2,
        };
        let node = |pos: [f32; 3], rot: [f32; 3]| {
            let v = |a: [f32; 3]| Byml::Array(a.iter().map(|f| Byml::Float(*f)).collect());
            Byml::Map(
                [("Translate".into(), v(pos)), ("Rotate".into(), v(rot))]
                    .into_iter()
                    .collect(),
            )
        };
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4);
        // His mouth 5 in front of him, facing -x (yaw -pi/2).
        let half = std::f32::consts::FRAC_PI_2;
        let mut mouth = node([5.0, 1.0, 0.0], [0.0, -half, 0.0]);
        place(&mut mouth, "Translate", "Rotate", centre, &spot).unwrap();
        // Facing -x turned a quarter faces +z (yaw 0): 5 along +z.
        assert!(close(
            checks::vec3(&mouth, "Translate").unwrap(),
            [0.0, 1.0, 5.0]
        ));
        assert!(close(
            checks::vec3(&mouth, "Rotate").unwrap(),
            [0.0, 0.0, 0.0]
        ));
        // An upside-down Euler turns its yaw the other way.
        let pi = std::f32::consts::PI;
        let mut flipped = node([10.0, 0.0, 0.0], [pi, 0.0, pi]);
        place(&mut flipped, "Translate", "Rotate", centre, &spot).unwrap();
        assert!(close(
            checks::vec3(&flipped, "Rotate").unwrap(),
            [pi, -half, pi]
        ));
        // A roll about Z rides along.
        let mut rolled = node([10.0, 0.0, 0.0], [0.0, -half, half]);
        place(&mut rolled, "Translate", "Rotate", centre, &spot).unwrap();
        assert!(close(
            checks::vec3(&rolled, "Rotate").unwrap(),
            [0.0, 0.0, half]
        ));
        // A tilt about X can't turn about Y.
        let mut leaning = node([0.0; 3], [0.3, 0.0, 0.0]);
        assert!(place(&mut leaning, "Translate", "Rotate", centre, &spot).is_err());
    }

    #[test]
    fn a_short_spot_is_an_error() {
        let spot = Spot {
            at: [0.0, 0.0],
            turn: std::f32::consts::FRAC_PI_2,
        };
        let short = |key: &str| {
            let v = |n: usize| Byml::Array(vec![Byml::Float(0.0); n]);
            let (pos, rot) = if key == "Translate" { (1, 3) } else { (3, 1) };
            Byml::Map(
                [("Translate".into(), v(pos)), ("Rotate".into(), v(rot))]
                    .into_iter()
                    .collect(),
            )
        };
        for key in ["Translate", "Rotate"] {
            let err = place(&mut short(key), "Translate", "Rotate", [0.0; 3], &spot);
            assert!(err.is_err(), "{key}");
        }
    }
}
