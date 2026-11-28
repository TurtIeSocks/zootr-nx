//! The gauntlet's loads: every holder a logic module makes, each dealt
//! item kinds in turn, and each map that holds one loaded. A module-made
//! holder's item can deadlock its map's load (the Deku Tree's boss heart
//! dealt ammo, #545), which the grid's one holder a kind can't see. Spec:
//! `docs/superpowers/specs/2026-11-27-gauntlet-v2-design.md`.

use std::collections::HashSet;

use anyhow::Result;
use ounce_formats::Romfs;

use super::grid::ITEMS;
use crate::settings::{NewGame, Settings};
use crate::{checks, gates};

/// The item kinds a load deals, one a risk class: lines, gear, quest,
/// dungeon, health and two counted bundles (#545's was ammo).
pub const KINDS: [&str; 7] = [
    "progressive",
    "major",
    "song",
    "small-key",
    "heart-piece",
    "ammo",
    "bombchus",
];

/// The kinds `--quick` keeps: #545's two.
pub const QUICK: [&str; 2] = ["progressive", "ammo"];

/// One item kind at one module-made holder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Load {
    pub holder: u64,
    /// Its place in [`ITEMS`].
    pub item_kind: usize,
}

/// The holders some logic module makes that are checks with `settings`,
/// in hash order.
pub fn holders(romfs: &Romfs, settings: &Settings) -> Result<Vec<u64>> {
    let wiring = gates::wiring(romfs)?;
    let made: HashSet<u64> = (wiring.placed.iter())
        .flat_map(|p| p.actors.iter().map(|&(_, hash, _)| hash))
        .collect();
    let new_game = NewGame::from_settings(settings, &[])?;
    let mut out: Vec<u64> = (checks::dealt(romfs, &new_game)?.iter())
        .map(|c| c.hash)
        .filter(|h| made.contains(h))
        .collect();
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// `names`' places in [`ITEMS`], those `only` names when it names any.
pub fn kinds(names: &[&str], only: &[String]) -> Vec<usize> {
    let kept = |n: &&&str| only.is_empty() || only.iter().any(|o| o == **n);
    let at = |n: &&str| ITEMS.iter().position(|k| k.name == *n);
    names.iter().filter(kept).filter_map(at).collect()
}

/// A round per kind: in round `r`, holder `i` holds kind `(i + r)` of
/// `kinds`, so every holder holds every kind once, and every round deals
/// every kind.
pub fn rounds(holders: &[u64], kinds: &[usize]) -> Vec<Vec<Load>> {
    (0..kinds.len())
        .map(|r| {
            (holders.iter().enumerate())
                .map(|(i, &holder)| Load {
                    holder,
                    item_kind: kinds[(i + r) % kinds.len()],
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_holder_holds_every_kind_once_over_the_rounds() {
        let kinds = kinds(&KINDS, &[]);
        assert_eq!(kinds.len(), KINDS.len(), "a kind isn't in ITEMS");
        let rounds = rounds(&[1, 2, 3], &kinds);
        assert_eq!(rounds.len(), KINDS.len());
        for holder in [1, 2, 3] {
            let mut held: Vec<_> = (rounds.iter().flatten())
                .filter(|l| l.holder == holder)
                .map(|l| l.item_kind)
                .collect();
            held.sort_unstable();
            let mut want = kinds.clone();
            want.sort_unstable();
            assert_eq!(held, want);
        }
    }

    #[test]
    fn quick_and_item_filters_narrow_the_kinds() {
        assert_eq!(kinds(&QUICK, &[]).len(), 2);
        assert_eq!(kinds(&KINDS, &["ammo".into()]), kinds(&["ammo"], &[]));
    }

    #[test]
    fn the_module_made_holders_take_in_every_boss_heart() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let settings = super::super::settings().unwrap();
        let holders = holders(&romfs, &settings).unwrap();
        let new_game = NewGame::from_settings(&settings, &[]).unwrap();
        let dealt = checks::dealt(&romfs, &new_game).unwrap();
        let heart = crate::known::actor::OBJ_HEART_CONTAINER.to_string();
        let hearts: Vec<u64> = (dealt.iter())
            .filter(|c| c.holder == heart)
            .map(|c| c.hash)
            .collect();
        assert!(!hearts.is_empty());
        for h in hearts {
            assert!(holders.contains(&h), "boss heart {h} isn't module-made");
        }
    }
}
