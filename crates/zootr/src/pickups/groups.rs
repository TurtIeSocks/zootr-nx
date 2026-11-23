//! A logic module making a holder appear or watching it runs only the
//! holder: a copy of it (a step's, or its picked twin) would spawn as a plain
//! placement, outside the module. A quiet-watched holder's copy gets a
//! duplicate of each `AiGroup` naming the holder, the copy's hash in the
//! holder's place, so the module runs the copy as it runs the holder
//! (research: upgrades off chests, watched holders). The copy names its own
//! clone of each sensor `Area` the group names: an `Area` two instances of
//! a module share can stop the map loading (research: Fire Temple crate
//! freeze).

use std::collections::BTreeSet;

use anyhow::Result;
use ounce_formats::Byml;

use crate::checks::{self, GYAML, hash_of, str_field};
use crate::known::{actor, field};
use crate::shows::{copy_hash, copy_in};

const HASH: &str = "Hash";
const REFERENCES: &str = "References";
const REFERENCE: &str = "Reference";

/// Duplicate each of `root`'s `AiGroups` naming a holder, once per copy in
/// `copies` (copy by the holder it copies), each with its own clones of the
/// group's `Area`s. Whether `root`, a map's doc, gained any.
pub fn copy(root: &mut Byml, copies: &[(u64, u64)]) -> Result<bool> {
    let areas: BTreeSet<u64> = (checks::actors(root)?.iter())
        .filter(|a| str_field(a, GYAML) == Some(actor::AREA.get()))
        .filter_map(hash_of)
        .collect();
    let Some(Byml::Array(groups)) = root.as_mut_map()?.get_mut(field::AI_GROUPS.get()) else {
        return Ok(false);
    };
    let mut added = Vec::new();
    // Each area cloned, by its clone.
    let mut clones = BTreeSet::new();
    for (of, copy) in copies {
        for group in groups.iter().filter(|g| names(g, *of)) {
            let mut dup = group.clone();
            let map = dup.as_mut_map()?;
            let hash = map.get(HASH).and_then(|h| h.as_int::<u64>().ok());
            map.insert(
                HASH.into(),
                Byml::U64(copy_hash(hash.unwrap_or(0) ^ copy, 1)),
            );
            if let Some(Byml::Array(refs)) = map.get_mut(REFERENCES) {
                for r in refs.iter_mut() {
                    let to = match reference(r) {
                        Some(h) if h == *of => *copy,
                        Some(h) if areas.contains(&h) => {
                            let clone = copy_hash(h ^ copy, 1);
                            clones.insert((h, clone));
                            clone
                        }
                        _ => continue,
                    };
                    r.as_mut_map()?.insert(REFERENCE.into(), Byml::U64(to));
                }
            }
            added.push(dup);
        }
    }
    let grew = !added.is_empty();
    groups.extend(added);
    copy_in(root, &clones.into_iter().collect::<Vec<_>>())?;
    Ok(grew)
}

/// Whether `group` names placement `hash` among its references.
fn names(group: &Byml, hash: u64) -> bool {
    let refs = group
        .as_map()
        .ok()
        .and_then(|g| g.get(REFERENCES)?.as_array().ok());
    refs.is_some_and(|refs| refs.iter().any(|r| reference(r) == Some(hash)))
}

fn reference(r: &Byml) -> Option<u64> {
    r.as_map().ok()?.get(REFERENCE)?.as_int().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks;
    use crate::game::dev_dump;

    /// A Kokiri Forest Gold Skulltula the nighttime module makes appear,
    /// and the web it deletes on its death.
    const SKULLTULA: u64 = 17067594426901953848;

    /// A Fire Temple crate a room's module respawns, watched through 18
    /// sensor areas.
    const CRATE: u64 = 15851182856759542692;

    /// The map of `kind`'s check `hash`.
    fn doc(romfs: &ounce_formats::Romfs, kind: &crate::pickups::Kind, hash: u64) -> Byml {
        let checks = kind.checks(romfs).unwrap();
        let check = checks.iter().find(|c| c.hash == hash).unwrap();
        checks::load_banc(romfs, &check.file).unwrap().root
    }

    /// The hashes `group` refers to, in order.
    fn refs(group: &Byml) -> Vec<u64> {
        let refs = group.as_map().unwrap()[REFERENCES].as_array().unwrap();
        refs.iter().filter_map(reference).collect()
    }

    /// `root`'s placements by hash.
    fn placed(root: &Byml) -> std::collections::HashMap<u64, &Byml> {
        let actors = checks::actors(root).unwrap().iter();
        actors.filter_map(|a| Some((hash_of(a)?, a))).collect()
    }

    fn naming(root: &Byml, hash: u64) -> Vec<Byml> {
        let groups = root.as_map().unwrap()[field::AI_GROUPS.get()]
            .as_array()
            .unwrap();
        groups.iter().filter(|g| names(g, hash)).cloned().collect()
    }

    #[test]
    fn a_copy_of_a_watched_holder_gets_its_own_group() {
        let Some(romfs) = dev_dump() else { return };
        let mut root = doc(&romfs, &crate::pickups::SKULLTULAS, SKULLTULA);
        let [vanilla] = &naming(&root, SKULLTULA)[..] else {
            panic!("one group names the Skulltula");
        };
        let copy = copy_hash(SKULLTULA, 1);
        assert!(super::copy(&mut root, &[(SKULLTULA, copy)]).unwrap());

        // The holder's group stays, and the copy's is the same group with
        // the copy in the holder's place, under a hash of its own.
        assert_eq!(&naming(&root, SKULLTULA)[..], std::slice::from_ref(vanilla));
        let [dup] = &naming(&root, copy)[..] else {
            panic!("one group names the copy");
        };
        let field = |g: &Byml, k: &str| g.as_map().unwrap()[k].clone();
        assert_eq!(field(dup, "Logic"), field(vanilla, "Logic"));
        assert_ne!(field(dup, HASH), field(vanilla, HASH));
        let swapped: Vec<u64> = refs(vanilla)
            .into_iter()
            .map(|r| if r == SKULLTULA { copy } else { r })
            .collect();
        assert_eq!(refs(dup), swapped);
        // The web stays named by both.
        assert_eq!(refs(dup).len(), 2);
    }

    #[test]
    fn each_copy_watches_through_its_own_areas() {
        let Some(romfs) = dev_dump() else { return };
        let mut root = doc(&romfs, &crate::pickups::CRATES, CRATE);
        let [vanilla] = &naming(&root, CRATE)[..] else {
            panic!("one group names the crate");
        };
        let before = placed(&root).len();
        let copies = [1, 2].map(|k| (CRATE, copy_hash(CRATE, k)));
        assert!(super::copy(&mut root, &copies).unwrap());

        assert_eq!(&naming(&root, CRATE)[..], std::slice::from_ref(vanilla));
        let placed = placed(&root);
        let area =
            |h: &u64| placed.get(h).and_then(|a| str_field(a, GYAML)) == Some(actor::AREA.get());
        let areas: Vec<u64> = refs(vanilla).into_iter().filter(area).collect();
        assert_eq!(areas.len(), 18);
        let mut seen = BTreeSet::new();
        for (_, copy) in copies {
            let [dup] = &naming(&root, copy)[..] else {
                panic!("one group names the copy");
            };
            let theirs: Vec<u64> = refs(dup).into_iter().filter(area).collect();
            assert_eq!(theirs.len(), areas.len());
            for (mine, clone) in areas.iter().zip(&theirs) {
                assert!(seen.insert(*clone), "a clone shared between copies");
                assert!(!areas.contains(clone), "a copy names the holder's area");
                // The same sensor, in the same place.
                let at = |h: &u64| checks::vec3(placed[h], "Translate");
                assert_eq!(at(mine), at(clone));
            }
            // The other crates stay shared.
            let crates = |g: &Byml| refs(g).into_iter().filter(|h| !area(h)).count();
            assert_eq!(crates(dup), crates(vanilla));
        }
        assert_eq!(placed.len(), before + 2 * areas.len());
    }

    #[test]
    fn a_holder_no_group_names_gets_none() {
        let Some(romfs) = dev_dump() else { return };
        let mut root = doc(&romfs, &crate::pickups::SKULLTULAS, SKULLTULA);
        assert!(!copy(&mut root, &[(1, 2)]).unwrap());
    }
}
