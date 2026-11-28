//! Physical variants of the same overworld reward and holders whose reward
//! would be permanently lost by ordinary play.

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::checks::{Drop, ItemCheck};

/// Alternate actor, then the actor whose check ID and vanilla item we keep.
pub(crate) const LINKED: &[(u64, u64)] = &[
    (10656348214587939196, 17186151219195818834), // KF Rock Circle Adult Rock 1
    (5853846732169153879, 13002777795600434992),  // KF Rock Circle Adult Rock 3
    (8931631613049749371, 6429280567576879719),   // KF Rock Circle Adult Rock 4
    (13164882936498639212, 16857565298886773615), // KF Rock Circle Adult Rock 6
    (802407974539353270, 7243630621233515413),    // KF Rock Circle Adult Rock 8
    (15388836867476865988, 4182932136190668163),  // KF Rock Circle Adult Rock 10
    (2657789777104100407, 8795160036094865435),   // KF Rock Circle Adult Rock 11
    (5675403228854574159, 13321954940214063371),  // KF Rock Circle Adult Rock 13
    (6087670250388905572, 11138816988756551783),  // DMT Above Signposts Rock 1
    (11074416093858939225, 3302088556978088009),  // DMT Above Signposts Rock 4
    (8668774813841012997, 8438942300162055419),   // GC Middle Floor Adult Boulder
];

pub(crate) const EXCLUDED: &[u64] = &[
    // These four child rocks never spawn. Their adult counterparts
    // disappear after Phantom Ganon and have no late replacement.
    16780156705177292663, // KF Rock Circle Child Rock 2
    4164292104786619976,  // KF Rock Circle Child Rock 5
    12045642716728132811, // KF Rock Circle Child Rock 9
    13219695470225186387, // KF Rock Circle Child Rock 12
    13817169663978116369, // KF Rock Circle Adult Rock 2
    15732862253800965646, // KF Rock Circle Adult Rock 5
    4738397009648900238,  // KF Rock Circle Adult Rock 9
    5312950394209176374,  // KF Rock Circle Adult Rock 12
    // Killing the child Skullwalltula replaces the adult regular crate.
    14654205574115276454,
    // Escape pots are only reachable during the final sequence.
    13784984248963672668,
    7806434487081634021,
    15046736084248795685,
    2995876338500118652,
    7057323129962538362,
    5547511959614762973,
    10448158396041431760,
    1337254747199514230,
    16158747032522515754,
];

pub(crate) fn canonical(hash: u64) -> u64 {
    LINKED
        .iter()
        .find_map(|&(alternate, main)| (hash == alternate).then_some(main))
        .unwrap_or(hash)
}

pub(super) fn alternate_of(hash: u64) -> Option<u64> {
    LINKED
        .iter()
        .find_map(|&(alternate, main)| (hash == main).then_some(alternate))
}

pub(crate) fn excluded(hash: u64) -> bool {
    EXCLUDED.contains(&hash)
}

/// Keep the real files for logic, with the same ID and vanilla item in
/// each. The original goes first so its name and file identify the check.
pub(crate) fn checks(mut pieces: Vec<ItemCheck>) -> Result<Vec<ItemCheck>> {
    let mut alternates = Vec::new();
    for &(alternate, main) in LINKED {
        let Some(index) = pieces.iter().position(|c| c.hash == alternate) else {
            continue;
        };
        let mut check = pieces.remove(index);
        let original = pieces
            .iter()
            .find(|c| c.hash == main)
            .with_context(|| format!("variant {alternate} needs check {main}"))?;
        check.hash = main;
        check.item.clone_from(&original.item);
        alternates.push(check);
    }
    pieces.extend(alternates);
    Ok(pieces)
}

/// Both placements give the dealt item. An unchanged original may
/// have been omitted from the seed's drop map.
pub(crate) fn drops(pieces: &[ItemCheck], drops: &HashMap<u64, Drop>) -> HashMap<u64, Drop> {
    let mut out = drops.clone();
    for &(alternate, main) in LINKED {
        if let Some(original) = pieces.iter().find(|c| c.hash == main) {
            let item = drops
                .get(&main)
                .cloned()
                .unwrap_or_else(|| Drop::from(original.item.clone()));
            out.insert(alternate, item);
        }
    }
    out
}
