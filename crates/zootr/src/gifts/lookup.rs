//! Which checks are gifts, and the gift behind a check.

use super::*;

/// The gifts a scene hands over.
pub(crate) fn of_scene(scene: &str) -> Vec<&'static Gift> {
    GIFTS.iter().filter(|g| g.flow == scene).collect()
}

/// A check an event hands out, rather than a placed actor.
pub(crate) fn check(file: String, area: &str, hash: u64, holder: String, item: &str) -> ItemCheck {
    ItemCheck {
        file,
        area: area.into(),
        hash,
        holder,
        item: item.to_string(),
        get_event: None,
        substitute: None,
        presence: None,
        watched: crate::checks::Watched::No,
        room: None,
    }
}

/// The item the seed puts at `hash`, else `vanilla`.
pub(crate) fn item_at<'a>(drops: &'a HashMap<u64, Drop>, hash: u64, vanilla: &'a str) -> &'a str {
    drops.get(&hash).map_or(vanilla, |d| d.item.as_str())
}

/// One check per gift.
pub fn checks() -> Vec<ItemCheck> {
    GIFTS
        .iter()
        .map(|g| check(flows::rel(g.flow), g.area, g.hash(), g.giver.into(), g.item))
        .collect()
}

pub fn of(check: &ItemCheck) -> Option<&'static Gift> {
    by_hash(check.hash)
}

/// The gift whose [`Gift::hash`] is `hash`, the first in [`GIFTS`] when
/// two share it. Indexed once: a gift's hash spells out its flow, call and
/// item, and the fill asks for every check of every seed.
pub fn by_hash(hash: u64) -> Option<&'static Gift> {
    static INDEX: std::sync::LazyLock<HashMap<u64, &'static Gift>> =
        std::sync::LazyLock::new(|| GIFTS.iter().rev().map(|g| (g.hash(), g)).collect());
    INDEX.get(&hash).copied()
}

/// The minigame chests that are checks, by placement hash: the Treasure
/// Chest Game's heart piece, then Dampe's two. Every other chest whose event
/// starts `MiniGame_` stays vanilla. Gifts in all but where they're placed,
/// since a chest of the game can't turn into a substitute one.
pub(crate) const PRIZE_CHESTS: [u64; 3] = [
    16881481538337649070,
    dampe::HOOKSHOT_CHEST,
    dampe::HEART_CHEST,
];

pub(crate) fn is_prize(check: &ItemCheck) -> bool {
    PRIZE_CHESTS.contains(&check.hash)
}

/// Saria's and Zelda's, every business scrub's, shop slot's, cow's and
/// minigame prize chest's.
pub fn is_gift(check: &ItemCheck) -> bool {
    of(check).is_some()
        || is_prize(check)
        || scrubs::of(check).is_some()
        || shops::is_shop(check)
        || pickups::is_pickup(check)
        || cows::is_cow(check)
}

/// Whether `holder` is a gift's: a giver's name, a scrub's, a shop slot's,
/// a cow's or a thief's.
pub fn is_giver(holder: &str) -> bool {
    GIFTS.iter().any(|g| g.giver == holder)
        || holder.starts_with(scrubs::HOLDER)
        || shops::is_holder(holder)
        || cows::is_holder(holder)
        || holder == pickups::THIEF
}
