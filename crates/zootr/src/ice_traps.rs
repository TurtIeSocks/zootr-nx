//! Ice traps: junk swapped for copies that pass for an item logic needs,
//! say "FOOL!" at the get, and freeze Link. See
//! `docs/superpowers/specs/2026-11-18-ice-traps-design.md`. The freeze
//! itself is [`freeze`], its ice [`look`].

mod freeze;
mod get;
mod look;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;

pub(crate) use freeze::adult_effects;
pub use freeze::{event, flow, sensor, thaw};
pub use get::one_page;
pub use look::{FILES, look};

use crate::foreign::{Foreign, Owner};
use crate::key_tints::MADE;
use crate::known::{Name, item};
use crate::rng::Rng;
use crate::settings::{IceTraps, NewGame};
use crate::shuffle::Placement;

/// What a trap's get says, in every language.
pub const FOOL: &str = "FOOL!";

/// A trap before its disguise, as the pool deals it. Every disguise's
/// name starts with it too, which is how the hook freezes Link for each.
/// In an Archipelago seed it's also the copy the room's traps give
/// ([`received`]).
pub const POOLED: &str = zootr_hook::traps::ITEM;
/// What the copy the room's traps give passes for: what Link holds up
/// before "FOOL!".
const RECEIVED_SHOWS: &Name = item::HOOKSHOT;
/// The most disguises a seed's traps take, one actor each.
const MOST: usize = 999;
/// Each disguise's actor, in the order the seed draws them.
static DISGUISED: LazyLock<Vec<String>> =
    LazyLock::new(|| (1..=MOST).map(|n| format!("{POOLED}{n:03}")).collect());
/// Keeps the traps' rolls apart from the fill's, which starts from the
/// same seed.
const SALT: u64 = 0x1ce7_7a95_f001_0001;
/// The pouch categories that are junk: rupees, ammo and recovery.
const JUNK: [&str; 3] = ["rupee", "ammo", "recovery"];

/// Whether `item` is a trap, disguised or not.
pub fn is_trap(item: &str) -> bool {
    item.starts_with(POOLED)
}

/// Whether the seed holds any trap, so it needs the sensor, its event and
/// the hook's freeze.
pub fn springs(new_game: &NewGame) -> bool {
    actors(&new_game.foreign).next().is_some()
}

/// Each trap's actor in `foreign`.
pub fn actors(foreign: &[Foreign]) -> impl Iterator<Item = &str> {
    (foreign.iter())
        .filter(|f| matches!(f.owner, Owner::Trap { .. }))
        .map(|f| f.actor.as_str())
}

/// The items among `items` that are junk and logic doesn't name in
/// `needed`: what the pouch files under [`JUNK`], and what [`swap`] may
/// trap.
pub fn junk<'a>(
    romfs: &Romfs,
    items: impl Iterator<Item = &'a str>,
    needed: &BTreeSet<&str>,
) -> Result<BTreeSet<&'a str>> {
    let mut out = BTreeSet::new();
    for item in items.collect::<BTreeSet<_>>() {
        if needed.contains(item) || item.starts_with(MADE) {
            continue;
        }
        if crate::items::get(romfs, item)?.is_some_and(|i| JUNK.contains(&i.category())) {
            out.insert(item);
        }
    }
    Ok(out)
}

/// Swap `how` much of the junk among `items` for traps, picked by `seed`.
/// `needed` is what logic names, never junk.
pub fn swap(
    romfs: &Romfs,
    items: &mut [&str],
    needed: &BTreeSet<&str>,
    how: IceTraps,
    seed: u64,
) -> Result<()> {
    if how == IceTraps::Off {
        return Ok(());
    }
    let junk = junk(romfs, items.iter().copied(), needed)?;
    let mut at: Vec<usize> = (0..items.len())
        .filter(|&i| junk.contains(items[i]))
        .collect();
    Rng::new(seed ^ SALT).shuffle(&mut at);
    for &i in at.iter().take(how.of(at.len())) {
        items[i] = POOLED;
    }
    Ok(())
}

/// Dress each trap in `placements` as an item logic needs (`needed`), or
/// with no logic any item that isn't junk, drawn by `seed` from those the
/// seed places. Each disguise is one copy, returned to write.
pub fn disguise(
    romfs: &Romfs,
    placements: &mut [Placement],
    needed: Option<&BTreeSet<&str>>,
    seed: u64,
) -> Result<Vec<Foreign>> {
    let traps: Vec<usize> = (0..placements.len())
        .filter(|&i| placements[i].item == POOLED)
        .collect();
    if traps.is_empty() {
        return Ok(vec![]);
    }
    let placed = || placements.iter().map(|p| p.item);
    let none = BTreeSet::new();
    let junk = junk(romfs, placed(), needed.unwrap_or(&none))?;
    let wanted = |i: &str| needed.map_or(!junk.contains(i), |n| n.contains(i));
    let shown: Vec<&str> = (placed().filter(|i| !i.starts_with(MADE) && wanted(i)))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ensure!(!shown.is_empty(), "no item for a trap to pass for");
    let mut rng = Rng::new(seed ^ SALT ^ 1);
    let mut actors: BTreeMap<&str, &'static str> = BTreeMap::new();
    let mut copies = vec![];
    for i in traps {
        let shows = shown[rng.below(shown.len() as u64) as usize];
        let next = actors.len();
        let actor = match actors.get(shows) {
            Some(actor) => *actor,
            None => {
                let actor = DISGUISED.get(next).context("too many disguises")?.as_str();
                let label = crate::names::name_label(romfs, shows)?
                    .with_context(|| format!("{shows} has no name"))?;
                copies.push(Foreign {
                    actor: actor.to_owned(),
                    shows: shows.to_owned(),
                    model: None,
                    owner: Owner::Trap { label },
                });
                *actors.entry(shows).or_insert(actor)
            }
        };
        placements[i].item = actor;
    }
    Ok(copies)
}

/// The copy an Archipelago seed with traps gives for each trap the room
/// sends, so its sensor and the hook's freeze are in whether or not any of
/// its own checks hold one.
pub fn received(romfs: &Romfs) -> Result<Foreign> {
    let shows = RECEIVED_SHOWS.get();
    let label =
        crate::names::name_label(romfs, shows)?.with_context(|| format!("{shows} has no name"))?;
    Ok(Foreign {
        actor: POOLED.to_owned(),
        shows: shows.to_owned(),
        model: None,
        owner: Owner::Trap { label },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{actor, item};
    use crate::narrowing::tests::chest;

    #[test]
    fn each_setting_swaps_its_share_of_the_junk() {
        let of = |how: IceTraps| [0, 1, 10, 11, 21].map(|n| how.of(n));
        assert_eq!(of(IceTraps::Off), [0; 5]);
        assert_eq!(of(IceTraps::Some), [0, 1, 1, 2, 3]);
        assert_eq!(of(IceTraps::Lots), [0, 1, 5, 6, 11]);
        assert_eq!(of(IceTraps::Mayhem), [0, 1, 10, 11, 21]);
    }

    #[test]
    fn a_trap_is_named_as_one_disguised_or_not() {
        assert!(is_trap(POOLED) && is_trap(&DISGUISED[0]));
        assert!(!is_trap(&crate::foreign::actor(1)) && !is_trap(&format!("{MADE}KeyRing01")));
    }

    /// Ten Red Rupees, a Recovery Heart and the Hookshot, swapped.
    fn swapped(how: IceTraps, needed: &BTreeSet<&str>, seed: u64) -> Option<Vec<&'static str>> {
        let romfs = dev_dump()?;
        let mut items = vec![actor::T_BOX_RUPEE_RED.get(); 10];
        items.extend([actor::OBJ_HEART.get(), item::HOOKSHOT.get()]);
        swap(&romfs, &mut items, needed, how, seed).unwrap();
        Some(items)
    }

    fn traps(items: &[&str]) -> usize {
        items.iter().filter(|i| **i == POOLED).count()
    }

    #[test]
    fn only_junk_logic_leaves_alone_becomes_traps() {
        let none = BTreeSet::new();
        let Some(off) = swapped(IceTraps::Off, &none, 7) else {
            return;
        };
        assert_eq!(traps(&off), 0);
        let some = swapped(IceTraps::Some, &none, 7).unwrap();
        assert_eq!(traps(&some), 2);
        let mayhem = swapped(IceTraps::Mayhem, &none, 7).unwrap();
        assert_eq!(traps(&mayhem), 11);
        assert_eq!(mayhem.last(), Some(&item::HOOKSHOT.get()));
        let heart = BTreeSet::from([actor::OBJ_HEART.get()]);
        let kept = swapped(IceTraps::Mayhem, &heart, 7).unwrap();
        assert_eq!(traps(&kept), 10);
        assert!(kept.contains(&actor::OBJ_HEART.get()));
    }

    #[test]
    fn the_same_seed_swaps_the_same_junk() {
        let none = BTreeSet::new();
        let Some(one) = swapped(IceTraps::Lots, &none, 7) else {
            return;
        };
        assert_eq!(swapped(IceTraps::Lots, &none, 7).unwrap(), one);
        let others = (8..20).map(|seed| swapped(IceTraps::Lots, &none, seed).unwrap());
        assert!(others.into_iter().any(|o| o != one), "the seed picks");
    }

    #[test]
    fn traps_pass_for_items_logic_needs_one_copy_each() {
        let Some(romfs) = dev_dump() else { return };
        let items = [POOLED, item::HOOKSHOT.get(), POOLED, item::BOOMERANG.get()];
        let checks: Vec<_> = (0..items.len() as u64).map(|h| chest(h, "x")).collect();
        let dealt = |needed: &BTreeSet<&str>, seed| {
            let mut placements: Vec<Placement> = (checks.iter().zip(items))
                .map(|(check, item)| Placement { check, item })
                .collect();
            let copies = disguise(&romfs, &mut placements, Some(needed), seed).unwrap();
            let items: Vec<&str> = placements.iter().map(|p| p.item).collect();
            (items, copies)
        };
        let hookshot = BTreeSet::from([item::HOOKSHOT.get()]);
        let (placed, copies) = dealt(&hookshot, 7);
        assert_eq!(placed[0], placed[2], "one disguise, one copy");
        assert!(is_trap(placed[0]) && placed[0] != POOLED);
        let [copy] = &copies[..] else {
            panic!("{copies:?}")
        };
        assert_eq!(
            (copy.actor.as_str(), copy.shows.as_str()),
            (placed[0], item::HOOKSHOT.get())
        );
        assert!(matches!(&copy.owner, Owner::Trap { label } if label.ends_with("_Name")));
        let both = BTreeSet::from([item::HOOKSHOT.get(), item::BOOMERANG.get()]);
        assert_eq!(dealt(&both, 7).0, dealt(&both, 7).0, "the seed picks");
    }
}
