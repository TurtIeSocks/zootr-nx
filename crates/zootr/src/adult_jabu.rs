//! Jabu-Jabu in the adult fountain (the Adult Jabu-Jabu setting). The adult
//! map has no Jabu, no door to him and no arrival from his belly, so each
//! comes from the child map, read fresh each seed.

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, Romfs};

use crate::checks;
use crate::known::{field, flag, map};
use crate::settings::AdultJabu;
use crate::world::{self, Era};

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
        return Ok(false);
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
        add_arrival(&mut doc.root, spot).with_context(|| rel.to_owned())?;
        builder.write(rel, &doc.to_binary())?;
        written += 1;
    }
    ensure!(written > 0, "no {} in the dump", START_POS[0]);
    Ok(())
}

/// Copies the child fountain's `spot` to the adult fountain's.
fn add_arrival(root: &mut Byml, spot: &str) -> Result<()> {
    let maps = root.as_mut_map()?;
    let child = map::ZORA_FOUNTAIN_CHILD.get();
    let pos = (maps
        .get(child)
        .and_then(|m| m.as_map().ok()?.get(spot))
        .cloned())
    .with_context(|| format!("no {spot} under {child}"))?;
    let adult = map::ZORA_FOUNTAIN_ADULT.get();
    (maps.get_mut(adult).with_context(|| format!("no {adult}"))?)
        .as_mut_map()?
        .insert(spot.into(), pos);
    Ok(())
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
        add_arrival(&mut doc.root, spot).unwrap();
        let child = doc
            .root
            .as_map()
            .unwrap()
            .get(map::ZORA_FOUNTAIN_CHILD.get())
            .unwrap();
        assert_eq!(adult(&doc.root).as_ref(), child.as_map().unwrap().get(spot));
    }
}
