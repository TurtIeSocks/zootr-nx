//! Checks a map places that its `_Alternative` copy doesn't. The game loads
//! the copy for good once a sage is done, so such a check would be lost.
//!
//! Burning Kakariko's East Purple Rupee 2 is chest 1's twin: the same chest
//! on the same spot under another placement hash, with its own opened flag.
//! There's no room beside it, so the copy's spot holds both in turn: chest
//! 1 until it's opened, then its twin, which stands closed in its place as
//! soon as chest 1's get window closes.
//!
//! Its beehives are twins too, each a little off the other's spot. A hive
//! has no opened flag to take turns on, so each is copied in beside its
//! twin.

use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlMap, Romfs};

use crate::checks::{self, GYAML, TRANSLATE, actors, hash_of, str_field, vec3};
use crate::known::field;

/// How far apart two placements can be, in m, and still share a spot.
const SAME_SPOT: f32 = 0.05;

/// A check a map places but its alternative doesn't, and how the
/// alternative hands it out.
pub struct Twin {
    /// The alternative's Banc file.
    pub file: String,
    /// The check's actor, as the map places it.
    pub actor: Byml,
    pub hash: u64,
    pub share: Share,
}

/// How a [`Twin`] shares the alternative with the actor on its spot.
pub enum Share {
    /// It stands until opened, then `twin` in its place.
    Turns {
        twin: u64,
        /// The check's opened flag, a Bool known only by hash.
        opened: u32,
    },
    /// It stands beside its twin: with no opened flag, it can't take turns.
    Beside,
}

impl Twin {
    /// The BoolExp both gates of a [`Share::Turns`] name, and the opened
    /// flag it reads.
    pub fn alias(&self) -> Option<(String, u32)> {
        match self.share {
            Share::Turns { opened, .. } => Some((alias_of(self.hash), opened)),
            Share::Beside => None,
        }
    }
}

/// The BoolExp naming the opened flag of the check placed as `hash`.
fn alias_of(hash: u64) -> String {
    format!("ZootrOpened.{hash}")
}

/// Each check `dealt` says is in the shuffle that a map places and its
/// alternative doesn't. One with an `opened` flag takes turns with its
/// twin, and fails without one: nothing would hand it out once the game
/// swaps the map. The rest (Kakariko's beehives) stand beside theirs.
pub fn find(
    romfs: &Romfs,
    dealt: impl Fn(u64) -> bool,
    opened: impl Fn(u64) -> Option<u32>,
) -> Result<Vec<Twin>> {
    let files = checks::banc_files(romfs)?;
    let mut out = Vec::new();
    for map in crate::world::alternatives(romfs)?.keys() {
        let tag = format!("/{map}_Alternative_");
        for file in files.iter().filter(|f| f.contains(&tag)) {
            let base = file.replace(&tag, &format!("/{map}_"));
            let base_doc = checks::load_banc(romfs, &base)?;
            let alt_doc = checks::load_banc(romfs, file)?;
            let (placed, alt) = (actors(&base_doc.root)?, actors(&alt_doc.root)?);
            let in_base: HashSet<u64> = placed.iter().filter_map(hash_of).collect();
            let in_alt: HashSet<u64> = alt.iter().filter_map(hash_of).collect();
            for actor in placed {
                let Some(hash) = hash_of(actor).filter(|h| dealt(*h) && !in_alt.contains(h)) else {
                    continue;
                };
                let Some(opened) = opened(hash) else {
                    out.push(Twin {
                        file: file.clone(),
                        actor: actor.clone(),
                        hash,
                        share: Share::Beside,
                    });
                    continue;
                };
                let twin = alt
                    .iter()
                    .filter(|a| same_spot(a, actor))
                    .filter_map(hash_of)
                    .find(|h| !in_base.contains(h))
                    .with_context(|| {
                        format!("{hash} is in {base} but not {file}, and has no twin there")
                    })?;
                out.push(Twin {
                    file: file.clone(),
                    actor: actor.clone(),
                    hash,
                    share: Share::Turns { twin, opened },
                });
            }
        }
    }
    Ok(out)
}

/// Whether `a` and `b` are the same actor on the same spot.
fn same_spot(a: &Byml, b: &Byml) -> bool {
    let at = |x: &Byml| vec3(x, TRANSLATE);
    match (at(a), at(b)) {
        (Some(p), Some(q)) if str_field(a, GYAML) == str_field(b, GYAML) => {
            let d = (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f32>();
            d.sqrt() < SAME_SPOT
        }
        _ => false,
    }
}

/// Places each of `twins` whose alternative is `file` in `root`, that
/// file's doc. One taking turns stands until it's opened, its twin from
/// then on, and both gates are lazy, so the swap waits for the open to
/// finish. Returns how many checks it placed.
pub fn place(file: &str, root: &mut Byml, twins: &[Twin]) -> Result<usize> {
    let mine: Vec<&Twin> = twins.iter().filter(|t| t.file == file).collect();
    if mine.is_empty() {
        return Ok(0);
    }
    let list = root
        .as_mut_map()?
        .get_mut("Actors")
        .with_context(|| format!("{file} has no actors"))?
        .as_mut_array()?;
    for t in &mine {
        let mut copy = t.actor.clone();
        if let Share::Turns { twin, .. } = t.share {
            let alias = alias_of(t.hash);
            let twin = list
                .iter_mut()
                .find(|a| hash_of(a) == Some(twin))
                .with_context(|| format!("no twin {twin} in {file}"))?;
            gate(twin, &alias, false)?;
            gate(&mut copy, &alias, true)?;
        }
        list.push(copy);
    }
    Ok(mine.len())
}

/// Gives `actor` a Presence on `flag`, read lazily, so never mid-open.
fn gate(actor: &mut Byml, flag: &str, negated: bool) -> Result<()> {
    let map = actor.as_mut_map()?;
    ensure!(!map.contains_key("Presence"), "{flag}: already gated");
    let mut presence = BymlMap::default();
    presence.insert(field::FLAG_NAME.get().into(), Byml::String(flag.into()));
    presence.insert(field::IS_CHECK_LAZY.get().into(), Byml::Bool(true));
    presence.insert(field::IS_NEGATION.get().into(), Byml::Bool(negated));
    map.insert("Presence".into(), Byml::Map(presence));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map;
    use crate::world::presence_of;

    /// Burning Kakariko's purple rupee chests, from the research notes.
    const CHEST_1: u64 = 11929552275106311978;
    const CHEST_2: u64 = 13080955777249750889;

    /// Every placed check's hash.
    fn placed(romfs: &Romfs) -> HashSet<u64> {
        checks::scan(romfs)
            .unwrap()
            .iter()
            .map(|c| c.hash)
            .collect()
    }

    #[test]
    fn burning_kakariko_hands_out_both_purple_rupees_in_turn() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = crate::gamedata::GameData::load(&romfs).unwrap();
        let opened = crate::tracker::chest_flags(&gd).unwrap();
        let all = placed(&romfs);
        let twins = find(&romfs, |h| all.contains(&h), opened).unwrap();
        let turns: Vec<_> = twins
            .iter()
            .filter_map(|t| match t.share {
                Share::Turns { twin, .. } => Some((t, twin)),
                Share::Beside => None,
            })
            .collect();
        let found: Vec<(u64, u64)> = turns.iter().map(|(t, twin)| (t.hash, *twin)).collect();
        assert_eq!(found, [(CHEST_1, CHEST_2)]);
        let t = turns[0].0;
        let alias = t.alias().unwrap().0;
        assert!(
            t.file
                .contains(map::KAKARIKO_VILLAGE_ADULT_ALTERNATIVE.get())
        );

        let mut doc = checks::load_banc(&romfs, &t.file).unwrap();
        let mine = |file: &str| twins.iter().filter(|t| t.file == file).count();
        assert_eq!(
            place(&t.file, &mut doc.root, &twins).unwrap(),
            mine(&t.file)
        );
        let gate_of = |hash| {
            let all = actors(&doc.root).unwrap();
            let mut at = all.iter().filter(|a| hash_of(a) == Some(hash));
            let a = at.next().unwrap();
            assert!(at.next().is_none(), "{hash} placed twice");
            let lazy = a.as_map().unwrap()["Presence"].as_map().unwrap()
                [field::IS_CHECK_LAZY.get()]
            .as_bool()
            .unwrap();
            assert!(lazy, "{hash} swaps mid-open");
            presence_of(a).unwrap()
        };
        let first = gate_of(CHEST_1);
        let second = gate_of(CHEST_2);
        assert_eq!((first.flag.as_str(), first.negated), (alias.as_str(), true));
        assert_eq!(
            (second.flag.as_str(), second.negated),
            (alias.as_str(), false)
        );
        // Placed again, it would gate an actor twice.
        assert!(place(&t.file, &mut doc.root, &twins).is_err());
        // Other files are left alone.
        assert_eq!(
            place("Banc/FakeMap.bcett.byml.zs", &mut doc.root, &twins).unwrap(),
            0
        );
    }

    #[test]
    fn a_check_with_no_opened_flag_stands_beside_its_twin() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = placed(&romfs);
        let beside = find(&romfs, |h| all.contains(&h), |_| None).unwrap();
        assert!(!beside.is_empty());
        assert!(beside.iter().all(|t| matches!(t.share, Share::Beside)));
        assert!(beside.iter().all(|t| t.alias().is_none()));
        // Copied in as they are: same hash, no gate, nothing else touched.
        let file = &beside[0].file;
        let mut doc = checks::load_banc(&romfs, file).unwrap();
        let before = actors(&doc.root).unwrap().len();
        let mine: Vec<_> = beside.iter().filter(|t| &t.file == file).collect();
        assert_eq!(place(file, &mut doc.root, &beside).unwrap(), mine.len());
        let after = actors(&doc.root).unwrap();
        assert_eq!(after.len(), before + mine.len());
        for t in mine {
            let copy = after.iter().find(|a| hash_of(a) == Some(t.hash)).unwrap();
            assert!(presence_of(copy).is_none(), "{} gated", t.hash);
        }
        // Given a flag, a beehive standing a little off its twin's spot has
        // nothing to take turns with.
        let err = find(&romfs, |h| all.contains(&h), |_| Some(1))
            .err()
            .unwrap();
        assert!(err.to_string().contains("has no twin"), "{err}");
    }
}
