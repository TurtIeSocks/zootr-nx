//! Upgrade lines. A chest holding any level of a line gives the next level
//! Link doesn't have, decided in the game by the hook (see `patch.rs`).
//! A gift asks the pouch in its own flow instead (`gifts/levels.rs`).
//!
//! Some levels stay out: the first wallet is never an item. The smallest
//! stick and nut bags come with the first stick or nut unless their
//! setting shuffles them, so a seed's own [`Lines`] drops them then. The
//! quiver and bullet bag lines start at the bow and slingshot, since the
//! Small levels have no get text and a chest giving one hangs the game.
//! The magic line starts at our Magic Meter, which seeds where magic
//! doesn't come with the first magic item have, and every seed with
//! progressive items, so a progressive seed never gives the double before
//! the meter.
use crate::bombchu_bags;
use crate::known::{Name, actor, item};
use crate::magic::meter;
use crate::settings::{Bombchus, NewGame};

/// Each line lowest level first: the actor a chest drops, then the pouch
/// name Link owns it as. Every level any seed has: [`Lines`] says which
/// one seed has.
pub const LINES: [&[(&Name, &Name)]; 12] = [
    &[same(item::HOOKSHOT), same(item::LONGSHOT)],
    &[
        (item::GAUNTLET_GORON, actor::ARMOR_CHILD_GAUNTLET_GORON),
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
    &[
        same(item::STICK_BAG_SMALL),
        same(item::STICK_BAG_MEDIUM),
        same(item::STICK_BAG_LARGE),
    ],
    &[
        same(item::NUT_BAG_SMALL),
        same(item::NUT_BAG_MEDIUM),
        same(item::NUT_BAG_LARGE),
    ],
    &[
        same(bombchu_bags::SMALL),
        same(bombchu_bags::MEDIUM),
        same(bombchu_bags::LARGE),
    ],
    &[same(meter::METER), same(item::FAIRY_BLESS_MP_DOUBLE)],
];

/// One level of a line: the actor a chest drops, then its pouch name.
pub type Level = (&'static Name, &'static Name);

/// The game actor a level of our own wears: the model and the pose it shows
/// are that one's, so a holder showing it measures against it. `None` for a
/// level the game ships.
pub fn worn(level: &str) -> Option<&'static Name> {
    (bombchu_bags::copies().into_iter())
        .find(|(bag, _)| *bag == level)
        .map(|(_, bag)| bag)
        .or(match level == meter::NAME {
            true => Some(meter::LIKE),
            false => None,
        })
}

/// The levels a seed's lines hold: [`LINES`] less the small nut or stick
/// bag the first nut or stick brings, less the Bombchu Bags in a seed
/// without them, and less the Magic Meter where magic comes with the first
/// magic item and progressive items are off. With `progressive` on, a
/// holder of any level gives the next one Link doesn't have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lines {
    pub nut_bag: bool,
    pub stick_bag: bool,
    pub bombchu_bags: bool,
    pub magic_meter: bool,
    pub progressive: bool,
}

impl Default for Lines {
    /// The game's own: no bag shuffled, and magic with the first magic
    /// item, so no Magic Meter.
    fn default() -> Self {
        Self::of(&NewGame {
            magic_with_first_item: true,
            ..Default::default()
        })
    }
}

impl Lines {
    pub fn of(new_game: &NewGame) -> Self {
        Self {
            nut_bag: new_game.shuffle_nut_bag,
            stick_bag: new_game.shuffle_stick_bag,
            bombchu_bags: new_game.bombchus == Bombchus::Bag,
            magic_meter: !new_game.magic_with_first_item || new_game.progressive,
            progressive: new_game.progressive,
        }
    }

    /// Every bag shuffled, and progressive items as `progressive` says:
    /// whatever any seed might ask.
    pub fn widest(progressive: bool) -> Self {
        Self {
            nut_bag: true,
            stick_bag: true,
            bombchu_bags: true,
            magic_meter: true,
            progressive,
        }
    }

    /// The game's lines climbed by progressive items, for tests.
    #[cfg(test)]
    pub fn climbing() -> Self {
        Self {
            progressive: true,
            ..Self::default()
        }
    }

    /// Every line as this seed has it, lowest level first.
    pub fn all(self) -> impl Iterator<Item = &'static [Level]> {
        LINES.into_iter().filter_map(move |l| self.trim(l))
    }

    /// `line` as this seed has it, or none.
    fn trim(self, line: &'static [Level]) -> Option<&'static [Level]> {
        let first = line[0].0;
        let unshuffled = (first == item::NUT_BAG_SMALL && !self.nut_bag)
            || (first == item::STICK_BAG_SMALL && !self.stick_bag)
            || (first == meter::METER && !self.magic_meter);
        let gone = first == bombchu_bags::SMALL && !self.bombchu_bags;
        (!gone).then_some(&line[usize::from(unshuffled)..])
    }

    /// `actor`'s line as this seed has it, each level's drop then its pouch
    /// name, lowest first.
    pub fn line(self, actor: &str) -> Option<&'static [Level]> {
        self.all()
            .find(|l| l.iter().any(|&(drop, _)| drop == actor))
    }

    /// The first level of `actor`'s line, or `actor` outside every line.
    pub fn first(self, actor: &str) -> &str {
        self.line(actor).map_or(actor, |l| l[0].0.get())
    }

    /// Whether `actor` is a level of some line.
    pub fn in_line(self, actor: &str) -> bool {
        self.line(actor).is_some()
    }

    /// `actor`'s line as [`line`](Self::line) gives it, if progressive
    /// items climb it. A line left with one level (Double Magic without the Magic
    /// Meter) isn't climbed: its holder always gives that level.
    pub fn climbs(self, actor: &str) -> Option<&'static [Level]> {
        let line = self.progressive.then(|| self.line(actor)).flatten()?;
        (line.len() > 1).then_some(line)
    }

    /// Whether this seed has `actor`: a level its lines keep, or an item
    /// outside every line.
    pub fn has(self, actor: &str) -> bool {
        self.in_line(actor) || line(actor).is_none()
    }
}

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

/// What a seed with progressive items calls any level of `actor`'s line,
/// where its first level's name would mislead: the Bracelet's line holds
/// both gauntlets.
pub fn label(actor: &str) -> Option<&'static str> {
    (first(actor) == item::GAUNTLET_GORON.get()).then_some("Strength")
}

/// Whether `actor` is a level of some line.
#[cfg(test)]
pub fn in_line(actor: &str) -> bool {
    line(actor).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_of_our_own_wears_the_actor_it_copies() {
        needs_names!();
        assert_eq!(worn(bombchu_bags::LARGE.get()), Some(item::BOMB_BAG_LARGE));
        assert_eq!(worn(meter::NAME), Some(meter::LIKE));
        // A level the game ships.
        assert_eq!(worn(item::OCARINA_TIME.get()), None);
    }

    #[test]
    fn a_line_left_with_one_level_is_not_climbed() {
        needs_names!();
        let double = item::FAIRY_BLESS_MP_DOUBLE.get();
        // With magic from the first magic item, the Magic Meter is no level,
        // so Double Magic stands alone and always gives itself.
        let unshuffled = Lines {
            magic_meter: false,
            ..Lines::climbing()
        };
        assert_eq!(unshuffled.climbs(double), None);
        let shuffled = Lines {
            magic_meter: true,
            ..Lines::climbing()
        };
        assert_eq!(shuffled.climbs(double).map(<[_]>::len), Some(2));
    }

    #[test]
    fn progressive_items_keep_the_magic_line_full_even_with_magic_from_the_first_item() {
        needs_names!();
        let double = item::FAIRY_BLESS_MP_DOUBLE.get();
        let game = NewGame {
            magic_with_first_item: true,
            progressive: true,
            ..Default::default()
        };
        // The meter stays in the line, so the hook gives it before the
        // double at any holder.
        assert!(Lines::of(&game).magic_meter);
        assert_eq!(Lines::of(&game).climbs(double).map(<[_]>::len), Some(2));
        // Without progressive items the line stays a lone Doubled Magic.
        let off = NewGame {
            progressive: false,
            ..game
        };
        assert!(!Lines::of(&off).magic_meter);
        assert_eq!(Lines::of(&off).climbs(double), None);
    }

    #[test]
    fn every_strength_level_is_strength_and_no_other_line_is() {
        needs_names!();
        for strength in [
            item::GAUNTLET_GORON,
            item::GAUNTLET_SILVER,
            item::GAUNTLET_GOLD,
        ] {
            assert_eq!(label(strength), Some("Strength"));
        }
        assert_eq!(label(item::HOOKSHOT), None);
        assert_eq!(label(item::BOMB_BAG_SMALL), None);
    }

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
        let levels = LINES.iter().flat_map(|l| l.iter());
        for &(drop, pouch) in levels.filter(|(d, _)| !crate::starter_bags::GET_TEXTS.contains(d)) {
            let has = |name: &str| ids.contains(&format!("{name}_GetMessage"));
            assert!(has(drop) || has(pouch), "{drop}");
        }
    }

    #[test]
    fn every_drop_is_an_item_the_dump_has() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let levels = LINES.iter().flat_map(|l| l.iter());
        for &(drop, _) in levels.filter(|(d, _)| !crate::known::is_ours(d.get())) {
            assert!(
                romfs.read(&format!("Pack/Actor/{drop}.pack.zs")).is_ok(),
                "{drop}"
            );
        }
    }
}
