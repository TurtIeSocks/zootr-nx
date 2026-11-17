//! Checks a map places that its `_Alternative` copy doesn't. The game loads
//! the copy for good once a sage is done, so such a check would be lost.
//!
//! Burning Kakariko's East Purple Rupee 2 is chest 1's twin: the same chest
//! on the same spot under another placement hash, with its own opened flag.
//! There's no room beside it, so the copy's spot holds both in turn: chest
//! 1 until it's opened, then its twin, which stands closed in its place as
//! soon as chest 1's get window closes.

use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlMap, Romfs};

use crate::checks::{self, GYAML, TRANSLATE, actors, hash_of, str_field, vec3};
use crate::known::field;

/// How far apart two placements can be, in m, and still share a spot.
const SAME_SPOT: f32 = 0.05;

/// A check a map places but its alternative doesn't, and the twin standing
/// on its spot there.
pub struct Twin {
    /// The alternative's Banc file.
    pub file: String,
    /// The check's actor, as the map places it.
    pub actor: Byml,
    pub hash: u64,
    pub twin: u64,
    /// The check's opened flag, a Bool known only by hash.
    pub opened: u32,
}

impl Twin {
    /// The BoolExp both gates name: the check's opened flag.
    pub fn alias(&self) -> String {
        format!("ZootrOpened.{}", self.hash)
    }
}

/// Each check `dealt` says is in the shuffle that a map places and its
/// alternative doesn't, with its twin. Only a check with an `opened` flag
/// can take turns, so the rest are left as they are (Kakariko's beehives
/// keep theirs in a table a BoolExp can't read). One with a flag and no
/// twin fails: nothing would hand it out once the game swaps the map.
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
                    twin,
                    opened,
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
/// file's doc: the check stands until it's opened, its twin from then on.
/// Both gates are lazy, so the swap waits for the open to finish.
/// Returns how many checks it placed.
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
        let twin = list
            .iter_mut()
            .find(|a| hash_of(a) == Some(t.twin))
            .with_context(|| format!("no twin {} in {file}", t.twin))?;
        gate(twin, &t.alias(), false)?;
        let mut copy = t.actor.clone();
        gate(&mut copy, &t.alias(), true)?;
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
        let found: Vec<(u64, u64)> = twins.iter().map(|t| (t.hash, t.twin)).collect();
        assert_eq!(found, [(CHEST_1, CHEST_2)]);
        let t = &twins[0];
        assert!(
            t.file
                .contains(map::KAKARIKO_VILLAGE_ADULT_ALTERNATIVE.get())
        );

        let mut doc = checks::load_banc(&romfs, &t.file).unwrap();
        assert_eq!(place(&t.file, &mut doc.root, &twins).unwrap(), 1);
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
        assert_eq!(
            (first.flag.as_str(), first.negated),
            (t.alias().as_str(), true)
        );
        assert_eq!(
            (second.flag.as_str(), second.negated),
            (t.alias().as_str(), false)
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
    fn only_a_check_with_an_opened_flag_takes_turns() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = placed(&romfs);
        let none = find(&romfs, |h| all.contains(&h), |_| None).unwrap();
        assert!(none.is_empty());
        // Given a flag, a beehive standing a little off its twin's spot has
        // nothing to take turns with.
        let err = find(&romfs, |h| all.contains(&h), |_| Some(1))
            .err()
            .unwrap();
        assert!(err.to_string().contains("has no twin"), "{err}");
    }
}
