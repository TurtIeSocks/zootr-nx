//! The pool and the random deal. `fill.rs` deals with logic instead.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail};

use crate::checks::{Drop, ItemCheck};
use crate::keys;
use crate::known::{Name, item};
use crate::rng::Rng;
use crate::settings::{self, NewGame};
use crate::starting;
use crate::{ammo, cows, gifts, pickups, progressive};

/// One shuffled chest, for the spoiler log.
#[derive(Clone, Copy)]
pub struct Placement<'a> {
    pub check: &'a ItemCheck,
    pub item: &'a str,
}

/// What a map or compass gives depends on the dungeon it's found in.
const OWN_DUNGEON: [&Name; 2] = [item::DUNGEON_MAP, item::DUNGEON_COMPASS];

/// Whether `new_game` moves maps and compasses around their own dungeon.
fn maps_move(new_game: &NewGame) -> bool {
    !new_game.vanilla && !new_game.keep_chests && new_game.on.contains(settings::MAPS_COMPASSES)
}

/// What `shuffleable` lets in beyond the plain chests.
#[derive(Clone, Copy)]
struct Let {
    /// Maps and compasses.
    maps: bool,
    /// The chests of keys Link starts with.
    keys: bool,
}

impl Let {
    fn of(new_game: &NewGame) -> Self {
        Self {
            maps: maps_move(new_game),
            keys: new_game.keys.moved(),
        }
    }
}

/// Chests M2 may touch. The rest stay vanilla until logic can place them.
fn shuffleable(c: &ItemCheck, open: Let) -> bool {
    // Not the Treasure Chest Game's shop chests, but its prize.
    (c.holder.starts_with("Obj_TBox_") || gifts::is_gift(c))
        // Keys moved around can soft-lock, since logic doesn't count them.
        // Keys Link starts with aren't moved: their chests bring a rupee.
        && (!c.item.starts_with("Item_Dungeon_")
            || open.maps && OWN_DUNGEON.iter().any(|d| **d == *c.item)
            || open.keys && keys::is_dungeon_key(&c.item))
        // Remix-only and not understood yet.
        && c.item != item::SCRUB_BUSINESS_SEED
        // Minigame events pick the outcome themselves, bar the prize chests.
        && (gifts::is_prize(c) || !c.get_event.as_deref().is_some_and(|e| e.starts_with("MiniGame_")))
}

/// One entry per chest. The child and adult copies of a map share a hash,
/// so a chest can show up in several files.
fn unique_chests(checks: &[ItemCheck], open: Let) -> Result<Vec<&ItemCheck>> {
    let mut by_hash = BTreeMap::<u64, &ItemCheck>::new();
    for c in checks.iter().filter(|c| shuffleable(c, open)) {
        let first = *by_hash.entry(c.hash).or_insert(c);
        if first.item != c.item {
            bail!(
                "chest {} holds {} in {} but {} in {}",
                c.hash,
                first.item,
                first.file,
                c.item,
                c.file
            );
        }
    }
    Ok(by_hash.into_values().collect())
}

/// Plando: chest hash to the item it must hold.
pub type Fixed = BTreeMap<u64, String>;

/// `fixed`, plus each map and compass in a chest of its own dungeon picked
/// by `seed`, when `new_game` moves them.
pub fn own_dungeon(
    checks: &[ItemCheck],
    seed: u64,
    fixed: &Fixed,
    new_game: &NewGame,
) -> Result<Fixed> {
    let mut fixed = fixed.clone();
    if !maps_move(new_game) {
        return Ok(fixed);
    }
    let chests = unique_chests(
        checks,
        Let {
            maps: true,
            ..Let::of(new_game)
        },
    )?;
    let moving: Vec<_> = chests
        .iter()
        .filter(|c| OWN_DUNGEON.iter().any(|d| **d == *c.item) && !fixed.contains_key(&c.hash))
        .map(|c| (c.area.as_str(), c.item.clone()))
        .collect();
    let mut rng = Rng::new(seed.rotate_left(17));
    for (area, item) in moving {
        let spots: Vec<_> = chests
            .iter()
            .filter(|c| c.area == area && !gifts::is_gift(c) && !fixed.contains_key(&c.hash))
            .collect();
        let spot = spots.get(rng.below(spots.len() as u64) as usize);
        let spot = spot.with_context(|| format!("no chest left in {area} for {item}"))?;
        fixed.insert(spot.hash, item);
    }
    Ok(fixed)
}

/// Remove one `item` from `pool`, else one `fallback`, else any item. Keeps
/// the pool one item per free chest whatever was placed.
fn take(pool: &mut Vec<&str>, item: &str, fallback: &str) {
    let at = pool.iter().position(|&i| i == item);
    let at = at.or_else(|| pool.iter().position(|&i| i == fallback));
    if let Some(at) = at.or(pool.len().checked_sub(1)) {
        pool.swap_remove(at);
    }
}

/// The most bottles the pouch holds.
const BOTTLES: usize = 5;

/// Whether `item` is a bottle, empty or filled.
fn is_bottle(item: &str) -> bool {
    item.starts_with(&**item::BOTTLE)
}

/// Keep at most `BOTTLES` less the other bottles of `items` and `placed`
/// milks in `items`, a plando milk counting as one of the others. Each
/// cow whose milk goes gives its Red Rupee instead.
fn cap_bottles<'a>(items: &mut [&'a str], placed: impl Iterator<Item = &'a str>) {
    let loose = items.iter().copied().filter(|&i| i != cows::VANILLA);
    let others = placed.chain(loose).filter(|i| is_bottle(i)).count();
    let mut room = BOTTLES.saturating_sub(others);
    for item in items.iter_mut().filter(|i| **i == cows::VANILLA) {
        match room.checked_sub(1) {
            Some(left) => room = left,
            None => *item = ammo::WITHOUT,
        }
    }
}

/// What a seed deals out.
pub struct Deal<'a> {
    /// Plando placements.
    pub placed: Vec<Placement<'a>>,
    /// Shuffleable checks plando left free, one per hash.
    pub free: Vec<&'a ItemCheck>,
    /// One item per free check: each free check's own in vanilla mode, else
    /// the pool. Each starting item leaves it for a filler, and progressive
    /// lines hold their first level.
    pub items: Vec<&'a str>,
}

/// `fixed` placements, and the free checks with the items to deal them.
pub fn deal<'a>(checks: &'a [ItemCheck], fixed: &'a Fixed, new_game: &NewGame) -> Result<Deal<'a>> {
    let level = |item: &'a str| match new_game.progressive {
        true => progressive::first(item),
        false => item,
    };
    let mut placed = Vec::new();
    for (&hash, item) in fixed {
        let check = checks.iter().find(|c| c.hash == hash);
        let check = check.with_context(|| format!("no check with hash {hash}"))?;
        let item = level(item);
        if let Some(gift) = gifts::of(check).filter(|_| asks_at(check, item, new_game.progressive))
        {
            bail!(
                "{} can't give {item}: what it gives depends on what Link has, and a gift can't check",
                gift.giver
            );
        }
        placed.push(Placement { check, item });
    }
    // Gifts come in a fixed order, Saria's first, so they keep their own
    // item rather than a line's first level they couldn't swap.
    let own = |c: &'a ItemCheck| match gifts::is_gift(c) {
        true => keys::pooled(c),
        false => level(keys::pooled(c)),
    };
    // With chests kept, a gift that may not hold its own item keeps it
    // too: only chests could take it off its hands.
    let keeps = |c: &'a ItemCheck| {
        let held = allowed(c, level(keys::pooled(c)), new_game.progressive);
        new_game.keep_chests && (!gifts::is_gift(c) || !held) && !fixed.contains_key(&c.hash)
    };
    let open = Let::of(new_game);
    // Kept checks hold their own item, placed like plando's.
    let (kept, chests): (Vec<_>, Vec<_>) =
        (unique_chests(checks, open)?.into_iter()).partition(|c| keeps(c));
    let mut items: Vec<_> = chests.iter().map(|c| keys::pooled(c)).collect();
    for p in placed.iter().filter(|p| shuffleable(p.check, open)) {
        take(&mut items, p.item, keys::pooled(p.check));
    }
    let free: Vec<_> = chests
        .into_iter()
        .filter(|c| !fixed.contains_key(&c.hash))
        .collect();
    if new_game.vanilla {
        items = free.iter().map(|c| own(c)).collect();
    } else {
        items.iter_mut().for_each(|i| *i = level(i));
    }
    // One swap over both, so a starting item leaves one spot either way.
    let mut all: Vec<_> = kept.iter().map(|c| own(c)).collect();
    all.append(&mut items);
    starting::swap_out(&mut all, &new_game.start);
    let mut items = all.split_off(kept.len());
    let kept = kept.into_iter().zip(all);
    placed.extend(kept.map(|(check, item)| Placement { check, item }));
    cap_bottles(&mut items, placed.iter().map(|p| p.item));
    keys::deal_copies(&mut items, new_game.keys)?;
    Ok(Deal {
        placed,
        free,
        items,
    })
}

/// `fixed` placements first, then the deal's items at random, or left
/// where they are in vanilla mode.
pub fn shuffle<'a>(
    checks: &'a [ItemCheck],
    seed: u64,
    fixed: &'a Fixed,
    new_game: &NewGame,
) -> Result<Vec<Placement<'a>>> {
    let Deal {
        mut placed,
        free,
        mut items,
    } = deal(checks, fixed, new_game)?;
    if !new_game.vanilla {
        Rng::new(seed).shuffle(&mut items);
    }
    let mut dealt: Vec<_> = free
        .into_iter()
        .zip(items)
        .map(|(check, item)| Placement { check, item })
        .collect();
    if !new_game.vanilla {
        trade(&mut dealt, new_game.progressive)?;
    }
    placed.extend(dealt);
    Ok(placed)
}

/// Whether what `item` gives depends on what Link has, which a gift can't
/// check: ammo without its bag, or a line the hook upgrades.
fn asks(item: &str, progressive: bool) -> bool {
    ammo::swap(item).is_some() || progressive && progressive::in_line(item)
}

/// Whether `item` asks on `check`. Nothing asks of a gift that gives
/// [`gifts::by_level`], and a line's level doesn't of a cow, whose song
/// asks the pouch first.
fn asks_at(check: &ItemCheck, item: &str, progressive: bool) -> bool {
    let cow = cows::is_cow(check) && ammo::swap(item).is_none();
    !gifts::by_level(check) && !cow && asks(item, progressive)
}

/// Whether `check` may hold `item`: a gift can't hold one that asks, or
/// one that [`gifts::clashes`]. A key copy goes in a plain chest, an NPC's
/// gift or a pickup, whose gets the hook credits as Link gets them (the
/// pouch add, or the pickup window). Shops, scrubs, cows and prize chests
/// wait on their own proof (stage 1 of the every-item-everywhere spec).
pub(crate) fn allowed(check: &ItemCheck, item: &str, progressive: bool) -> bool {
    let gift = gifts::is_gift(check);
    let fits = !gift || !asks_at(check, item, progressive) && !gifts::clashes(check, item);
    let chest = !gift && check.holder.starts_with("Obj_TBox_");
    let credited = chest || gifts::of(check).is_some() || pickups::is_pickup(check);
    fits && (credited || !keys::is_shuffled(item))
}

/// Trade each gift's item it may not hold for the next chest's (wrapping)
/// that it may.
fn trade(dealt: &mut [Placement], progressive: bool) -> Result<()> {
    let n = dealt.len();
    let stuck = |p: &Placement| gifts::is_gift(p.check);
    for g in 0..n {
        let gift = dealt[g].check;
        if !gifts::is_gift(gift) || allowed(gift, dealt[g].item, progressive) {
            continue;
        }
        let fits = |j: &usize| !stuck(&dealt[*j]) && allowed(gift, dealt[*j].item, progressive);
        let to = (1..n).map(|k| (g + k) % n).find(fits);
        let to = to.context("no chest left for a gift's item")?;
        let item = dealt[g].item;
        dealt[g].item = dealt[to].item;
        dealt[to].item = item;
    }
    Ok(())
}

/// What a chest holding `item` drops. The one place a drop is decided.
/// Lines need nothing here: the hook upgrades them in the game.
pub fn drop(item: &str) -> Drop {
    match ammo::swap(item) {
        Some(swap) => Drop {
            item: ammo::WITHOUT.to_string(),
            swap: Some(swap),
        },
        None => Drop::from(item.to_owned()),
    }
}

/// The drops a mod has to write, shuffled or not: chests whose item
/// changed, whose drop swaps, or whose vanilla swap needs clearing.
pub fn drops(checks: &[ItemCheck], placements: &[Placement]) -> HashMap<u64, Drop> {
    let write = |c: &ItemCheck, item: &str| {
        let d = match gifts::is_gift(c) {
            // The deal keeps swaps off gifts.
            true => Drop::from(item.to_owned()),
            false => drop(item),
        };
        let write = d.swap.is_some() || item != c.item || c.substitute.is_some();
        write.then_some((c.hash, d))
    };
    let unmoved = checks.iter().filter_map(|c| write(c, &c.item));
    let placed = placements.iter().filter_map(|p| write(p.check, p.item));
    // Placements come last so they win.
    unmoved.chain(placed).collect()
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, flag, item};
    use std::collections::BTreeSet;

    use super::*;
    use crate::checks::Swap;

    fn chest(hash: u64, item: &str) -> ItemCheck {
        ItemCheck {
            file: "Banc/Area/Map.bcett.byml.zs".into(),
            area: "Area".into(),
            hash,
            holder: actor::OBJ_T_BOX_A_02.to_string(),
            item: item.into(),
            get_event: None,
            substitute: None,
            presence: None,
        }
    }

    /// Nothing beyond plain chests.
    fn no() -> Let {
        Let {
            maps: false,
            keys: false,
        }
    }

    fn game(vanilla: bool, start: &[&'static starting::Item]) -> NewGame {
        NewGame {
            vanilla,
            start: start.to_vec(),
            ..Default::default()
        }
    }

    fn holding<'a>(placements: &'a [Placement], hash: u64) -> &'a str {
        placements
            .iter()
            .find(|p| p.check.hash == hash)
            .unwrap()
            .item
    }

    #[test]
    fn maps_and_compasses_stay_in_their_own_dungeon() {
        needs_names!();
        let mut checks: Vec<_> = (1..=40).map(|h| chest(h, "Item_A")).collect();
        checks[0].item = OWN_DUNGEON[0].to_string();
        checks[1].item = OWN_DUNGEON[1].to_string();
        checks
            .iter_mut()
            .skip(20)
            .for_each(|c| c.area = "Other".into());
        let new_game = NewGame {
            on: [settings::MAPS_COMPASSES.to_owned()].into(),
            ..game(false, &[])
        };
        let mut moved = false;
        for seed in 0..20 {
            let fixed = own_dungeon(&checks, seed, &Fixed::new(), &new_game).unwrap();
            assert_eq!(fixed.len(), 2);
            assert!(fixed.keys().all(|&h| h <= 20), "{fixed:?}");
            let p = shuffle(&checks, seed, &fixed, &new_game).unwrap();
            assert_eq!(p.len(), 40);
            for item in OWN_DUNGEON {
                assert_eq!(p.iter().filter(|p| p.item == item).count(), 1);
            }
            moved |= !fixed.contains_key(&1);
        }
        assert!(moved);
        let off = own_dungeon(&checks, 7, &Fixed::new(), &game(false, &[])).unwrap();
        assert!(off.is_empty());
        let p = shuffle(&checks, 7, &off, &game(false, &[])).unwrap();
        assert_eq!(p.len(), 38);
    }

    /// Two plain chests and a key chest of each kind.
    fn with_keys() -> Vec<ItemCheck> {
        vec![
            chest(1, "Item_A"),
            chest(2, "Item_B"),
            chest(3, keys::SMALL),
            chest(4, keys::BOSS),
        ]
    }

    #[test]
    fn starting_with_keys_lets_their_chests_in_and_keeps_the_keys_out() {
        needs_names!();
        let (checks, none) = (with_keys(), Fixed::new());
        let on = NewGame {
            keys: settings::DungeonKeys::Start,
            ..game(false, &[])
        };
        for vanilla in [false, true] {
            let new_game = NewGame {
                vanilla,
                ..on.clone()
            };
            for seed in 0..20 {
                let p = shuffle(&checks, seed, &none, &new_game).unwrap();
                assert_eq!(p.len(), 4);
                let mut dealt: Vec<_> = p.iter().map(|p| p.item).collect();
                dealt.sort();
                let red = keys::INSTEAD.get();
                assert_eq!(dealt, ["Item_A", "Item_B", red, red]);
                if vanilla {
                    assert_eq!((holding(&p, 3), holding(&p, 4)), (red, red));
                }
            }
        }
        let p = shuffle(&checks, 7, &none, &game(false, &[])).unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|p| p.check.hash < 3));
    }

    #[test]
    fn a_key_chest_that_changes_gets_a_drop_and_one_that_doesnt_is_left() {
        needs_names!();
        let (checks, none) = (with_keys(), Fixed::new());
        let on = NewGame {
            keys: settings::DungeonKeys::Start,
            ..game(false, &[])
        };
        let p = shuffle(&checks, 7, &none, &on).unwrap();
        let d = drops(&checks, &p);
        for (hash, item) in [(3, keys::SMALL), (4, keys::BOSS)] {
            let now = holding(&p, hash);
            assert_eq!(d.contains_key(&hash), now != item);
        }
        let p = shuffle(&checks, 7, &none, &game(false, &[])).unwrap();
        assert!(drops(&checks, &p).is_empty());
    }

    #[test]
    fn a_cow_holds_a_line_item_but_not_ammo() {
        needs_names!();
        let cow = ItemCheck {
            holder: cows::HOLDER.into(),
            ..chest(1, "Item_A")
        };
        assert!(allowed(&cow, item::HOOKSHOT, true));
        assert!(!allowed(&cow, item::BOMB_BUNDLE_SMALL, true));
    }

    #[test]
    fn key_copies_go_in_chests_npc_gifts_and_pickups() {
        needs_names!();
        let ring = crate::key_tints::ring(1);
        let pot = ItemCheck {
            holder: actor::OBJ_POT_A_01.to_string(),
            ..chest(1, "Item_A")
        };
        let other = ItemCheck {
            holder: "Obj_P".into(),
            ..chest(1, "Item_A")
        };
        let saria = ItemCheck {
            hash: gifts::GIFTS[0].hash(),
            ..chest(1, "Item_A")
        };
        assert!(allowed(&chest(1, "Item_A"), &ring, false));
        assert!(allowed(&saria, &ring, false));
        assert!(allowed(&pot, &ring, false));
        assert!(!allowed(&other, &ring, false));
        assert!(allowed(&other, "Item_A", false));
    }

    #[test]
    fn plando_takes_its_item_out_of_the_pool() {
        needs_names!();
        let checks = [chest(1, "Item_A"), chest(2, "Item_B"), chest(3, "Item_C")];
        let fixed = Fixed::from([(1, "Item_C".into())]);
        let p = shuffle(&checks, 7, &fixed, &game(false, &[])).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(holding(&p, 1), "Item_C");
        let mut rest = [holding(&p, 2), holding(&p, 3)];
        rest.sort();
        assert_eq!(rest, ["Item_A", "Item_B"]);
    }

    #[test]
    fn vanilla_leaves_unplaced_chests_alone() {
        needs_names!();
        let checks = [chest(1, "Item_A"), chest(2, "Item_B")];
        let fixed = Fixed::from([(1, item::HOOKSHOT.to_string())]);
        let p = shuffle(&checks, 7, &fixed, &game(true, &[])).unwrap();
        assert_eq!(
            (holding(&p, 1), holding(&p, 2)),
            (&**item::HOOKSHOT, "Item_B")
        );
        assert!(
            shuffle(
                &checks,
                7,
                &Fixed::from([(9, "X".into())]),
                &game(true, &[])
            )
            .is_err()
        );
    }

    #[test]
    fn a_starting_item_leaves_the_pool_for_a_filler() {
        needs_names!();
        let sword = &starting::ITEMS[0];
        let checks = [chest(1, sword.actor), chest(2, "Item_B")];
        let none = Fixed::new();
        for vanilla in [false, true] {
            let p = shuffle(&checks, 7, &none, &game(vanilla, &[sword])).unwrap();
            let mut all = [holding(&p, 1), holding(&p, 2)];
            all.sort();
            assert_eq!(all, ["Item_B", starting::FILLER]);
        }
        let p = shuffle(&checks, 7, &none, &game(true, &[sword])).unwrap();
        assert_eq!(holding(&p, 1), starting::FILLER);
    }

    #[test]
    fn a_starting_item_outside_the_pool_changes_nothing() {
        needs_names!();
        let checks = [chest(1, "Item_A"), chest(2, "Item_B")];
        let (shield, none) = (&starting::ITEMS[1], Fixed::new());
        let p = shuffle(&checks, 7, &none, &game(true, &[shield])).unwrap();
        assert_eq!((holding(&p, 1), holding(&p, 2)), ("Item_A", "Item_B"));
    }

    #[test]
    fn plando_still_places_a_starting_item() {
        needs_names!();
        let sword = &starting::ITEMS[0];
        let checks = [chest(1, sword.actor), chest(2, "Item_B")];
        let fixed = Fixed::from([(2, sword.actor.to_string())]);
        let p = shuffle(&checks, 7, &fixed, &game(false, &[sword])).unwrap();
        assert_eq!(holding(&p, 2), sword.actor);
        assert_eq!(holding(&p, 1), "Item_B");
    }

    #[test]
    fn progressive_lines_hold_their_first_level() {
        needs_names!();
        let ([(hook, _), (long, _)], none) = (line(0), Fixed::new());
        let checks = [chest(1, hook), chest(2, long), chest(3, "Item_B")];
        let [(silver, _), (gold, _)] = line(3);
        let fixed = Fixed::from([(3, gold.into())]);
        for vanilla in [false, true] {
            let new_game = NewGame {
                progressive: true,
                ..game(vanilla, &[])
            };
            let p = shuffle(&checks, 7, &none, &new_game).unwrap();
            let mut all = [1, 2, 3].map(|h| holding(&p, h));
            all.sort();
            assert_eq!(all, ["Item_B", hook, hook]);
            let p = shuffle(&checks, 7, &fixed, &new_game).unwrap();
            assert_eq!(holding(&p, 3), silver);
        }
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        assert_eq!((holding(&p, 1), holding(&p, 2)), (hook, long));
    }

    /// A two level line from `progressive::LINES`.
    fn line(i: usize) -> [(&'static str, &'static str); 2] {
        let [(a, b), (c, d)] = <[_; 2]>::try_from(progressive::LINES[i]).unwrap();
        [(a.get(), b.get()), (c.get(), d.get())]
    }

    #[test]
    fn only_the_three_prize_chests_of_the_minigames_are_dealt() {
        let minigame = |hash, holder: &str| ItemCheck {
            holder: holder.into(),
            get_event: Some(flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX.to_string()),
            ..chest(hash, "Item_A")
        };
        for hash in gifts::PRIZE_CHESTS {
            assert!(shuffleable(
                &minigame(hash, actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02),
                no()
            ));
        }
        assert!(!shuffleable(&minigame(3, actor::OBJ_T_BOX_A_02), no()));
        assert!(!shuffleable(
            &minigame(4, actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02),
            no()
        ));
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let games = |c: &&ItemCheck| {
            c.get_event
                .as_deref()
                .is_some_and(|e| e.starts_with("MiniGame_"))
        };
        let dealt: BTreeSet<u64> = (checks.iter().filter(games))
            .filter(|c| shuffleable(c, no()))
            .map(|c| c.hash)
            .collect();
        assert_eq!(dealt, BTreeSet::from(gifts::PRIZE_CHESTS));
    }

    #[test]
    fn an_unmoved_line_chest_is_left_to_the_hook() {
        let ([_, (long, _)], none) = (line(0), Fixed::new());
        let race = ItemCheck {
            get_event: Some(flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX.to_string()),
            ..chest(3, long)
        };
        let checks = [chest(1, long), chest(2, "Item_B"), race];
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        assert!(p.iter().all(|p| p.check.hash != 3));
        assert!(drops(&checks, &p).is_empty());
    }

    /// Two plain chests, a line, bombs, and both gifts.
    fn with_gifts() -> Vec<ItemCheck> {
        let [(hook, _), (long, _)] = line(0);
        let mut checks = vec![
            chest(1, "Item_A"),
            chest(2, "Item_B"),
            chest(3, hook),
            chest(4, long),
            chest(5, ammo::NEEDS[0].0),
        ];
        checks.extend((6..10).map(|n| chest(n, "Item_A")));
        // Room for the gifts that give a bag or a quiver's first level.
        checks.extend((1000..1010).map(|n| chest(n, "Item_A")));
        checks.extend(gifts::checks());
        checks
    }

    /// `with_gifts`, the scrubs, and chests enough to take what they can't
    /// hold.
    fn with_scrubs() -> Vec<ItemCheck> {
        let mut checks = with_gifts();
        checks.extend((10..40).map(|h| chest(h, &format!("Item_{h}"))));
        checks.extend(crate::scrubs::checks());
        checks
    }

    fn progressive(vanilla: bool, start: &[&'static starting::Item]) -> NewGame {
        NewGame {
            progressive: true,
            ..game(vanilla, start)
        }
    }

    #[test]
    fn gifts_hold_an_item_that_asks_only_when_they_give_by_level() {
        needs_names!();
        let (checks, none) = (with_scrubs(), Fixed::new());
        let mut pool: Vec<_> = checks.iter().map(|c| progressive::first(&c.item)).collect();
        pool.sort();
        for seed in 0..200 {
            let p = shuffle(&checks, seed, &none, &progressive(false, &[])).unwrap();
            let mut dealt: Vec<_> = p.iter().map(|p| p.item).collect();
            dealt.sort();
            assert_eq!(dealt, pool);
            for p in p.iter().filter(|p| gifts::is_gift(p.check)) {
                assert!(!asks_at(p.check, p.item, true), "seed {seed}: {}", p.item);
            }
        }
    }

    #[test]
    fn plando_cant_put_an_item_that_asks_on_a_sale() {
        needs_names!();
        let checks = with_gifts();
        let bombs = |g: &gifts::Gift| Fixed::from([(g.hash(), ammo::NEEDS[0].0.to_string())]);
        let sale = gifts::GIFTS.iter().find(|g| g.is_sale()).unwrap();
        let err = shuffle(&checks, 7, &bombs(sale), &game(false, &[]))
            .err()
            .unwrap();
        assert!(err.to_string().contains(sale.giver), "{err}");
        // Saria gives the bombs or a Red Rupee by what Link owns.
        let saria = gifts::GIFTS[0].hash();
        assert!(shuffle(&checks, 7, &bombs(&gifts::GIFTS[0]), &game(false, &[])).is_ok());
        let hook = Fixed::from([(saria, line(0)[0].0.into())]);
        assert!(shuffle(&checks, 7, &hook, &game(false, &[])).is_ok());
        // Saria gives the Hookshot or the Longshot by what Link owns.
        assert!(shuffle(&checks, 7, &hook, &progressive(true, &[])).is_ok());
    }

    #[test]
    fn a_scrub_can_hold_a_song_a_medallion_or_a_stone() {
        needs_names!();
        let mut checks = with_scrubs();
        let plain = [item::MELODY_SUN_SONG, item::MEDAL_WATER, item::STONE_FIRE];
        checks.extend(
            plain
                .iter()
                .enumerate()
                .map(|(n, i)| chest(100 + n as u64, i)),
        );
        let scrub = crate::scrubs::SCRUBS[2].hash();
        for item in plain {
            let at = checks.iter().find(|c| c.hash == scrub).unwrap();
            assert!(allowed(at, item, false), "{item}");
            let fixed = Fixed::from([(scrub, item.to_string())]);
            let dealt = shuffle(&checks, 7, &fixed, &game(false, &[])).unwrap();
            assert_eq!(
                dealt.iter().find(|p| p.check.hash == scrub).unwrap().item,
                item
            );
        }
    }

    #[test]
    fn vanilla_gifts_keep_their_own_item() {
        needs_names!();
        let (checks, none) = (with_gifts(), Fixed::new());
        let p = shuffle(&checks, 7, &none, &progressive(true, &[])).unwrap();
        for gift in &gifts::GIFTS {
            assert_eq!(holding(&p, gift.hash()), gift.item);
        }
        let d = drops(&checks, &p);
        assert!(gifts::GIFTS.iter().all(|g| !d.contains_key(&g.hash())));
    }

    #[test]
    fn kept_chests_hold_their_own_item_and_the_rest_still_deal() {
        needs_names!();
        let sword = &starting::ITEMS[0];
        let mut checks = with_gifts();
        checks.push(chest(20, sword.actor));
        let new_game = NewGame {
            keep_chests: true,
            ..game(false, &[sword])
        };
        let none = Fixed::new();
        let d = deal(&checks, &none, &new_game).unwrap();
        assert!(!d.free.is_empty() && d.free.iter().all(|c| gifts::is_gift(c)));
        assert_eq!(d.items.len(), d.free.len());
        let held = |h| d.placed.iter().find(|p| p.check.hash == h).unwrap().item;
        assert_eq!((held(2), held(20)), ("Item_B", &**starting::FILLER));
    }

    #[test]
    fn a_starting_fairy_ocarina_leaves_the_other_to_upgrade() {
        needs_names!();
        let (checks, none) = (with_gifts(), Fixed::new());
        let fairy = &starting::ITEMS[4];
        let [saria, zelda] = [0, 1].map(|i| gifts::GIFTS[i].hash());
        let p = shuffle(&checks, 7, &none, &progressive(true, &[fairy])).unwrap();
        assert_eq!(holding(&p, saria), starting::FILLER);
        assert_eq!(holding(&p, zelda), item::OCARINA_TIME);
        let p = shuffle(&checks, 7, &none, &progressive(false, &[fairy])).unwrap();
        let held: Vec<_> = p.iter().filter(|p| p.item == fairy.actor).collect();
        assert_eq!(held.len(), 1, "one Fairy Ocarina left");
        assert!(p.iter().any(|p| p.item == starting::FILLER));
    }

    #[test]
    fn a_vanilla_substitute_chest_keeps_its_item_and_loses_its_swap() {
        needs_names!();
        let owned = ItemCheck {
            substitute: Some("Item_B".into()),
            ..chest(1, "Item_A")
        };
        let (checks, none) = ([owned, chest(2, "Item_B")], Fixed::new());
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        assert_eq!(p.len(), 2);
        let d = drops(&checks, &p);
        assert_eq!(d, HashMap::from([(1, Drop::from("Item_A".to_owned()))]));
    }

    #[test]
    fn every_vanilla_substitute_chest_is_shuffleable() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let owned: std::collections::BTreeSet<_> = checks
            .iter()
            .filter(|c| c.substitute.is_some())
            .inspect(|c| assert!(shuffleable(c, no()), "{} {}", c.file, c.hash))
            .map(|c| c.hash)
            .collect();
        assert_eq!(owned.len(), 7);
    }

    #[test]
    fn only_ammo_swaps() {
        needs_names!();
        let [(hook, _), _] = line(0);
        let (bombs, bag) = ammo::NEEDS[0];
        let swap = Swap {
            check: bag,
            then: bombs,
        };
        assert_eq!(
            drop(bombs),
            Drop {
                item: ammo::WITHOUT.to_string(),
                swap: Some(swap)
            }
        );
        for item in [hook, "Item_A"] {
            assert_eq!(drop(item), Drop::from(item.to_owned()));
        }
    }

    #[test]
    fn an_unshuffled_bomb_chest_gives_a_rupee_until_link_has_the_bag() {
        needs_names!();
        let (bombs, none) = (ammo::NEEDS[0].0, Fixed::new());
        let checks = [chest(1, bombs), chest(2, "Item_B")];
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        let d = drops(&checks, &p);
        assert_eq!(d, HashMap::from([(1, drop(bombs))]));
        assert_eq!(d[&1].item, ammo::WITHOUT);
    }

    #[test]
    fn never_more_than_five_bottles_are_dealt() {
        needs_names!();
        let mut checks: Vec<_> = (0..8).map(|n| chest(n, cows::VANILLA)).collect();
        checks.extend((8..10).map(|n| chest(n, item::BOTTLE)));
        checks.push(chest(10, item::BOTTLE_LETTER));
        checks.extend((11..20).map(|n| chest(n, "Item_A")));
        let count = |d: &Deal, item: &str| d.items.iter().filter(|&&i| i == item).count();
        let none = Fixed::new();
        let dealt = deal(&checks, &none, &game(false, &[])).unwrap();
        assert_eq!(dealt.items.len(), 20);
        assert_eq!(count(&dealt, cows::VANILLA), 2);
        assert_eq!(count(&dealt, ammo::WITHOUT), 6);
        let fixed = Fixed::from([(0, cows::VANILLA.to_string())]);
        let dealt = deal(&checks, &fixed, &game(false, &[])).unwrap();
        assert_eq!(count(&dealt, cows::VANILLA), 1, "a plando milk counts too");
    }
}
