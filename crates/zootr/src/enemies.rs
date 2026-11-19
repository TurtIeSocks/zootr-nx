//! Quiet enemies: drill and sweep seeds keep enemies out of the way, so
//! nothing grabs Link or knocks him back while the harness walks to a
//! holder and uses it. Each enemy stays placed, since logic graphs and
//! groups name it by hash, but it shows only once a Bool nothing sets is
//! set. An enemy a check needs stays: one that holds, sells or gives a
//! check, and one whose death a chest's gate reads, so the sweep beats it
//! for real. Specs: quiet enemies (2026-11-18), real enemy kills
//! (2026-11-19).

use std::collections::HashSet;

use anyhow::Result;
use ounce_formats::{Byml, Romfs};

use crate::checks::{GYAML, actors_mut, gate, hash_of, str_field};
use crate::known::{Name, actor};

/// The Bool every quiet enemy waits on. Nothing sets it.
pub const FLAG: &str = "ZootrQuietEnemies";

/// How every enemy's actor name starts.
pub(crate) const ENEMY: &str = "Enemy_";

/// Enemy kinds kept whole, each with its variants (a scrub's leaf). Their
/// checks carry no placement of theirs: a gift's, a Business Scrub's leaf,
/// or the Gerudo thieves', whose keys the carpenters' flow moves to each
/// thief once beaten (`fortress`).
const KEPT: [&Name; 6] = [
    actor::ENEMY_MIDDLE_BOSS_GERUDO_THIEF,
    actor::ENEMY_POE_BIG,
    actor::ENEMY_SCRUB_BUSINESS,
    actor::ENEMY_SCRUB_DEKU,
    actor::ENEMY_SCRUB_FOREST_STAGE,
    actor::ENEMY_SKULL_KID,
];

/// Whether the actor named `gyaml` is an enemy of no kind kept whole.
fn quiet(gyaml: &str) -> bool {
    let kept = |k: &&Name| {
        (gyaml.strip_prefix(&***k)).is_some_and(|rest| rest.is_empty() || rest.starts_with('_'))
    };
    gyaml.starts_with(ENEMY) && !KEPT.iter().any(kept)
}

/// The enemy placements a seed keeps by hash: the `dealt` checks, every
/// check any setting deals (a Gold Skulltula, a thief's key, a flying
/// pot), and every enemy a chest's gate reads.
pub fn kept(romfs: &Romfs, dealt: &HashSet<u64>) -> Result<HashSet<u64>> {
    let mut out = dealt.clone();
    let every = crate::settings::NewGame::from_settings(&crate::ledger::every_shuffle(), &[])?;
    out.extend(crate::checks::dealt(romfs, &every)?.iter().map(|c| c.hash));
    out.extend(crate::gates::enemies(romfs)?);
    Ok(out)
}

/// Keep out each enemy in `root` that isn't one of the `kept` placements.
/// Returns the hashes changed.
pub fn hide(root: &mut Byml, kept: &HashSet<u64>) -> Result<Vec<u64>> {
    let mut changed = Vec::new();
    for a in actors_mut(root)? {
        let Some(hash) = hash_of(a) else {
            continue;
        };
        if kept.contains(&hash) || !str_field(a, GYAML).is_some_and(quiet) {
            continue;
        }
        // Whatever it showed on before, it never shows now.
        a.as_mut_map()?.remove("Presence");
        gate(a, FLAG, false)?;
        changed.push(hash);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{actors, load_banc};
    use crate::known::{field, map};

    fn room(gyamls: &[&str]) -> Byml {
        let placed = gyamls
            .iter()
            .enumerate()
            .map(|(i, g)| Byml::from_iter([("Hash", Byml::from(i as u64)), (GYAML, (*g).into())]));
        Byml::from_iter([("Actors", Byml::from(placed.collect::<Vec<_>>()))])
    }

    #[test]
    fn plain_enemies_wait_on_the_flag_and_the_rest_stay() {
        needs_names!();
        let thief = format!("{}_03", &**actor::ENEMY_MIDDLE_BOSS_GERUDO_THIEF);
        let mut root = room(&[
            "Enemy_FakeOcto",
            &thief,
            &**actor::ENEMY_SCRUB_BUSINESS_LEAF,
            // A check, or one a gate reads, though an enemy.
            &**actor::ENEMY_FLYING_POT_SPIRIT_TEMPLE,
            &**actor::ENEMY_SKULLTULA,
            "Obj_FakeCrate",
        ]);
        let checks = HashSet::from([3]);
        assert_eq!(hide(&mut root, &checks).unwrap(), [0, 4]);
        let gate = |i: usize| -> Option<(String, bool)> {
            let p = actors(&root).unwrap()[i]
                .as_map()
                .unwrap()
                .get("Presence")?;
            let p = p.as_map().unwrap();
            Some((
                p[field::FLAG_NAME.get()].as_string().unwrap().to_string(),
                p[field::IS_NEGATION.get()].as_bool().unwrap(),
            ))
        };
        assert_eq!(gate(0), Some((FLAG.into(), false)));
        assert_eq!(gate(4), Some((FLAG.into(), false)));
        assert!([1, 2, 3, 5].into_iter().all(|i| gate(i).is_none()));
    }

    #[test]
    fn an_enemy_s_own_presence_gives_way() {
        needs_names!();
        let mut root = room(&["Enemy_FakeOcto"]);
        let actor = &mut actors_mut(&mut root).unwrap()[0];
        gate(actor, "FakeRoom_Cleared", true).unwrap();
        assert_eq!(hide(&mut root, &HashSet::new()).unwrap(), [0]);
    }

    #[test]
    fn a_kept_kind_s_prefix_is_a_whole_word() {
        needs_names!();
        let near = format!("{}Fake", &**actor::ENEMY_POE_BIG);
        assert!(quiet(&near));
        assert!(!quiet(&format!("{}_FakeLeaf", &**actor::ENEMY_POE_BIG)));
    }

    #[test]
    fn the_shadow_temple_s_like_like_goes_and_its_gold_skulltulas_stay() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let rel = format!(
            "Banc/{0}/{0}_Static.bcett.byml.zs",
            map::SHADOW_TEMPLE.get()
        );
        let mut root = load_banc(&romfs, &rel).unwrap().root;
        let of = |root: &Byml, kind: &Name| -> HashSet<u64> {
            let placed = actors(root).unwrap().iter();
            placed
                .filter(|a| str_field(a, GYAML) == Some(&**kind))
                .filter_map(hash_of)
                .collect()
        };
        let (like_likes, tokens) = (
            of(&root, actor::ENEMY_LIKE_LIKE),
            of(&root, actor::ENEMY_SKULLTULA_GOLD),
        );
        assert_eq!((like_likes.len(), tokens.len()), (1, 5));
        let kept = kept(&romfs, &HashSet::new()).unwrap();
        let hidden: HashSet<u64> = hide(&mut root, &kept).unwrap().into_iter().collect();
        assert!(like_likes.is_subset(&hidden));
        assert!(tokens.is_disjoint(&hidden));
        // The two Gibdos whose deaths raise a chest.
        let gibdos = of(&root, actor::ENEMY_GIBDO);
        assert!(gibdos.intersection(&kept).count() >= 2);
        assert!(gibdos.intersection(&kept).all(|h| !hidden.contains(h)));
    }

    #[test]
    fn the_fortress_keeps_its_four_thieves() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let kept = kept(&romfs, &HashSet::new()).unwrap();
        let mut thieves = 0;
        for (rel, doc) in
            crate::checks::load_bancs(&romfs, crate::checks::banc_files(&romfs).unwrap())
        {
            if !rel.contains(&**map::GERUDO_FORTRESS_ADULT) {
                continue;
            }
            let mut root = doc.unwrap().root;
            let hidden: HashSet<u64> = hide(&mut root, &kept).unwrap().into_iter().collect();
            for a in actors(&root).unwrap() {
                let thief = str_field(a, GYAML)
                    .is_some_and(|g| g.starts_with(&**actor::ENEMY_MIDDLE_BOSS_GERUDO_THIEF));
                if thief {
                    thieves += 1;
                    assert!(!hidden.contains(&hash_of(a).unwrap()));
                }
            }
        }
        assert_eq!(thieves, 4);
    }
}
