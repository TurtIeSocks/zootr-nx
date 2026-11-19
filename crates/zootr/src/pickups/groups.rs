//! A logic module making a holder appear or watching it runs only the
//! holder: a copy of it (a step's, or its picked twin) would spawn as a plain
//! placement, outside the module. A quiet-watched holder's copy gets a
//! duplicate of each `AiGroup` naming the holder, the copy's hash in the
//! holder's place, so the module runs the copy as it runs the holder
//! (research: upgrades off chests, watched holders).

use anyhow::Result;
use ounce_formats::Byml;

use crate::known::field;
use crate::shows::copy_hash;

const HASH: &str = "Hash";
const REFERENCES: &str = "References";
const REFERENCE: &str = "Reference";

/// Duplicate each of `root`'s `AiGroups` naming a holder, once per copy in
/// `copies` (copy by the holder it copies). Whether `root`, a map's doc,
/// gained any.
pub fn copy(root: &mut Byml, copies: &[(u64, u64)]) -> Result<bool> {
    let Some(Byml::Array(groups)) = root.as_mut_map()?.get_mut(field::AI_GROUPS.get()) else {
        return Ok(false);
    };
    let mut added = Vec::new();
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
                for r in refs.iter_mut().filter(|r| reference(r) == Some(*of)) {
                    r.as_mut_map()?.insert(REFERENCE.into(), Byml::U64(*copy));
                }
            }
            added.push(dup);
        }
    }
    let grew = !added.is_empty();
    groups.extend(added);
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

    /// The Skulltula's map.
    fn doc(romfs: &ounce_formats::Romfs) -> Byml {
        let checks = crate::pickups::SKULLTULAS.checks(romfs).unwrap();
        let check = checks.iter().find(|c| c.hash == SKULLTULA).unwrap();
        checks::load_banc(romfs, &check.file).unwrap().root
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
        let mut root = doc(&romfs);
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
        let refs = |g: &Byml| -> Vec<u64> {
            let refs = g.as_map().unwrap()[REFERENCES].as_array().unwrap();
            refs.iter().filter_map(reference).collect()
        };
        let swapped: Vec<u64> = refs(vanilla)
            .into_iter()
            .map(|r| if r == SKULLTULA { copy } else { r })
            .collect();
        assert_eq!(refs(dup), swapped);
        // The web stays named by both.
        assert_eq!(refs(dup).len(), 2);
    }

    #[test]
    fn a_holder_no_group_names_gets_none() {
        let Some(romfs) = dev_dump() else { return };
        assert!(!copy(&mut doc(&romfs), &[(1, 2)]).unwrap());
    }
}
