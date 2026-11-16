//! Upgrade lines. A chest holding any level of a line gives the next level
//! Link doesn't have, decided in the game by the hook (see `patch.rs`).
//! A gift asks the pouch in its own flow instead (`gifts/levels.rs`).
//!
//! Levels from gifts stay out: the Goron Bracelet comes from Darunia, the
//! first wallet is never an item, and the smallest stick and nut capacity
//! come with the first stick or nut. The quiver and bullet bag lines start
//! at the bow and slingshot, since the Small levels have no get text and a
//! chest giving one hangs the game.
use crate::known::{Name, actor, item};

/// Each line lowest level first: the actor a chest drops, then the pouch
/// name Link owns it as.
pub const LINES: [&[(&Name, &Name)]; 10] = [
    &[same(item::HOOKSHOT), same(item::LONGSHOT)],
    &[
        (item::GAUNTLET_SILVER, actor::ARMOR_ADULT_GAUNTLET_SILVER),
        (item::GAUNTLET_GOLD, actor::ARMOR_ADULT_GAUNTLET_GOLD),
    ],
    &[same(item::OCARINA_FAIRY), same(item::OCARINA_TIME)],
    &[same(item::SCALE_SILVER), same(item::SCALE_GOLD)],
    &[same(item::WALLET_MEDIUM), same(item::WALLET_LARGE)],
    &[
        same(item::BOMB_BAG_SMALL),
        same(item::BOMB_BAG_MEDIUM),
        same(item::BOMB_BAG_LARGE),
    ],
    &[
        (item::RANGE_BOW, actor::WEAPON_RANGE_BOW),
        same(item::QUIVER_MEDIUM),
        same(item::QUIVER_LARGE),
    ],
    &[
        (item::RANGE_SLING_SHOT, actor::WEAPON_RANGE_SLING_SHOT),
        same(item::BULLET_BAG_MEDIUM),
        same(item::BULLET_BAG_LARGE),
    ],
    &[same(item::STICK_BAG_MEDIUM), same(item::STICK_BAG_LARGE)],
    &[same(item::NUT_BAG_MEDIUM), same(item::NUT_BAG_LARGE)],
];

const fn same(actor: &Name) -> (&Name, &Name) {
    (actor, actor)
}

/// `actor`'s line, each level's drop then its pouch name, lowest first.
pub(crate) fn line(actor: &str) -> Option<&'static [(&'static Name, &'static Name)]> {
    LINES
        .into_iter()
        .find(|l| l.iter().any(|&(drop, _)| drop == actor))
}

/// The first level of `actor`'s line, or `actor` outside every line.
pub fn first(actor: &str) -> &str {
    line(actor).map_or(actor, |l| l[0].0.get())
}

/// Every name of `actor`'s line, each level's drop then its pouch name,
/// lowest first: none outside every line.
pub fn levels(actor: &str) -> Vec<&'static str> {
    let levels = line(actor).into_iter().flatten();
    levels
        .flat_map(|&(drop, pouch)| [drop.get(), pouch.get()])
        .collect()
}

/// Whether `actor` is a level of some line.
pub fn in_line(actor: &str) -> bool {
    line(actor).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_starts_its_line() {
        needs_names!();
        for line in LINES {
            for &(drop, _) in line {
                assert_eq!(first(drop), line[0].0);
                assert!(in_line(drop));
            }
        }
        assert_eq!(first("Item_A"), "Item_A");
        assert!(!in_line("Item_A"));
    }

    #[test]
    fn every_level_has_get_text() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let sarc = roead::sarc::Sarc::new(romfs.read(crate::names::TEXT).unwrap()).unwrap();
        let ids: Vec<String> = sarc
            .files()
            .filter(|f| f.name().is_some_and(|n| n.ends_with(".msbt")))
            .flat_map(|f| ounce_formats::messages(f.data()).unwrap().into_keys())
            .collect();
        for &(drop, pouch) in LINES.iter().flat_map(|l| l.iter()) {
            let has = |name: &str| ids.contains(&format!("{name}_GetMessage"));
            assert!(has(drop) || has(pouch), "{drop}");
        }
    }

    #[test]
    fn every_drop_is_an_item_the_dump_has() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for &(drop, _) in LINES.iter().flat_map(|l| l.iter()) {
            assert!(
                romfs.read(&format!("Pack/Actor/{drop}.pack.zs")).is_ok(),
                "{drop}"
            );
        }
    }
}
