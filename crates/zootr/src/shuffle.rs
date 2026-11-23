//! The pool and the random deal. `fill.rs` deals with logic instead.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail};

use crate::checks::{Drop, ItemCheck};
use crate::keys;
use crate::known::item;
use crate::narrowing::{Let, allowed, asks_at, shuffleable};
use crate::progressive::Lines;
use crate::rng::Rng;
use crate::settings::{MET_ZELDA, NewGame};
use crate::starter_bags;
use crate::starting;
use crate::{ammo, cows, gifts, presets, progressive};

/// One shuffled chest, for the spoiler log.
#[derive(Clone, Copy)]
pub struct Placement<'a> {
    pub check: &'a ItemCheck,
    pub item: &'a str,
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

/// Remove one `item` from `pool`, else one `fallback`, else any item. Keeps
/// the pool one item per free chest whatever was placed. `from`, the
/// check each slot came from, loses the same slot.
fn take(pool: &mut Vec<&str>, from: &mut Vec<u64>, item: &str, fallback: &str) {
    let at = pool.iter().position(|&i| i == item);
    let at = at.or_else(|| pool.iter().position(|&i| i == fallback));
    if let Some(at) = at.or(pool.len().checked_sub(1)) {
        pool.swap_remove(at);
        from.swap_remove(at);
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
    /// Each starting item that left the pool, with the check (by hash)
    /// whose slot it left, for the spoiler's Start column.
    pub started: Vec<(&'a str, u64)>,
    /// What settings add to the pool (`starter_bags::added`), at the
    /// level the pool holds each.
    pub added: Vec<&'a str>,
    /// Each of `added` with the rupee it took. One that found none is
    /// left out.
    pub traded: Vec<(&'a str, &'a str)>,
}

/// `fixed` placements, and the free checks with the items to deal them.
pub fn deal<'a>(checks: &'a [ItemCheck], fixed: &'a Fixed, new_game: &NewGame) -> Result<Deal<'a>> {
    let lines = progressive::Lines::of(new_game);
    let level = |item: &'a str| match new_game.progressive {
        true => lines.first(item),
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
    let open = Let::of(new_game);
    // The story items a setting starts Link with stay at their checks, so
    // the pool holds no second.
    let mut pins = Vec::new();
    for (hash, item) in gifts::pinned(&new_game.on) {
        match fixed.get(&hash) {
            Some(own) if own == item => continue,
            Some(own) => bail!("check {hash} holds {item} with this game's settings, not {own}"),
            None => {}
        }
        let check = checks
            .iter()
            .find(|c| c.hash == hash && shuffleable(c, open));
        if let Some(check) = check {
            placed.push(Placement { check, item });
            pins.push(hash);
        }
    }
    let placed_at = |hash: &u64| fixed.contains_key(hash) || pins.contains(hash);
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
        new_game.keep_chests && (!gifts::is_gift(c) || !held) && !placed_at(&c.hash)
    };
    // Kept checks hold their own item, placed like plando's.
    let (kept, chests): (Vec<_>, Vec<_>) =
        (unique_chests(checks, open)?.into_iter()).partition(|c| keeps(c));
    let mut items: Vec<_> = chests.iter().map(|c| keys::pooled(c)).collect();
    let mut from: Vec<_> = chests.iter().map(|c| c.hash).collect();
    for p in placed.iter().filter(|p| shuffleable(p.check, open)) {
        take(&mut items, &mut from, p.item, keys::pooled(p.check));
    }
    let free: Vec<_> = chests.into_iter().filter(|c| !placed_at(&c.hash)).collect();
    if new_game.vanilla {
        items = free.iter().map(|c| own(c)).collect();
        from = free.iter().map(|c| c.hash).collect();
    } else {
        items.iter_mut().for_each(|i| *i = level(i));
    }
    let added: Vec<_> = starter_bags::added(new_game)
        .into_iter()
        .map(level)
        .collect();
    let traded = starter_bags::swap_in(&mut items, &added);
    // One swap over both, so a starting item leaves one spot either way.
    let mut all: Vec<_> = kept.iter().map(|c| own(c)).collect();
    all.append(&mut items);
    let from: Vec<_> = kept.iter().map(|c| c.hash).chain(from).collect();
    let mut owned = new_game.start.owned();
    if new_game.on.contains(MET_ZELDA) {
        // Met Zelda leaves nothing for the egg to wake.
        owned.push(item::WEIRD_EGG.get());
    }
    let slots = starting::swap_out(&mut all, &owned, level);
    let started = (owned.into_iter().zip(slots))
        .filter_map(|(item, at)| Some((item, from[at?])))
        .collect();
    if new_game.on.contains(presets::KING_ZORA_MOVED) {
        // Nothing reads the letter once King Zora has moved, so it's dealt
        // as the bottle it comes in, as Ship does.
        for i in all.iter_mut().filter(|i| **i == &**item::BOTTLE_LETTER) {
            *i = &**item::BOTTLE;
        }
    }
    let mut items = all.split_off(kept.len());
    let kept = kept.into_iter().zip(all);
    placed.extend(kept.map(|(check, item)| Placement { check, item }));
    cap_bottles(&mut items, placed.iter().map(|p| p.item));
    let fixed: Vec<_> = placed.iter().map(|p| p.item).collect();
    keys::deal_copies(&mut items, new_game.keys, new_game.fortress_keys, &fixed)?;
    Ok(Deal {
        placed,
        free,
        items,
        started,
        added,
        traded,
    })
}

/// `fixed` placements first, then the deal's items at random, or left
/// where they are in vanilla mode.
#[cfg(test)]
pub fn shuffle<'a>(
    checks: &'a [ItemCheck],
    seed: u64,
    fixed: &'a Fixed,
    new_game: &NewGame,
) -> Result<Vec<Placement<'a>>> {
    scatter(deal(checks, fixed, new_game)?, seed, new_game)
}

/// `deal`'s placements, then its items at random, or left where they are
/// in vanilla mode.
pub fn scatter<'a>(deal: Deal<'a>, seed: u64, new_game: &NewGame) -> Result<Vec<Placement<'a>>> {
    let Deal {
        mut placed,
        free,
        mut items,
        ..
    } = deal;
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
pub fn drop(item: &str, lines: Lines) -> Drop {
    match ammo::swap(item, lines) {
        Some(swap) => Drop {
            item: ammo::WITHOUT.to_string(),
            swap: Some(swap),
        },
        None => Drop::from(item.to_owned()),
    }
}

/// The drops a mod has to write, shuffled or not: chests whose item
/// changed, whose drop swaps, or whose vanilla swap needs clearing.
pub fn drops(checks: &[ItemCheck], placements: &[Placement], lines: Lines) -> HashMap<u64, Drop> {
    let write = |c: &ItemCheck, item: &str| {
        let d = match gifts::is_gift(c) {
            // A minigame's chest can't be a substitute chest.
            true if gifts::is_prize(c) => {
                Drop::from(ammo::copy(item, lines).unwrap_or(item.to_owned()))
            }
            // The deal keeps swaps off gifts.
            true => Drop::from(item.to_owned()),
            false => drop(item, lines),
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
    use crate::known::{Name, flag, item};
    use crate::settings;

    use super::*;
    use crate::checks::Swap;
    use crate::narrowing::MAPS_COMPASSES;
    use crate::narrowing::tests::chest;

    /// The game's own magic, so the pool holds only the checks' items.
    fn game(vanilla: bool, start: &[&str]) -> NewGame {
        NewGame {
            vanilla,
            start: starting::Start::of(start),
            magic_with_first_item: true,
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
    fn maps_and_compasses_move_as_their_dungeon_s_copies_to_any_chest() {
        needs_names!();
        let mut checks: Vec<_> = (1..=40).map(|h| chest(h, "Item_A")).collect();
        checks[0].item = MAPS_COMPASSES[0].to_string();
        checks[1].item = MAPS_COMPASSES[1].to_string();
        checks[..2]
            .iter_mut()
            .for_each(|c| c.area = crate::known::flag::FOREST_TEMPLE.to_string());
        let new_game = NewGame {
            on: [settings::MAPS_COMPASSES.to_owned()].into(),
            ..game(false, &[])
        };
        let copies = [keys::pooled(&checks[0]), keys::pooled(&checks[1])];
        assert!(
            copies.iter().all(|c| crate::map_copies::is_copy(c)),
            "{copies:?}"
        );
        let none = Fixed::new();
        let mut moved = false;
        for seed in 0..20 {
            let p = shuffle(&checks, seed, &none, &new_game).unwrap();
            assert_eq!(p.len(), 40);
            for copy in copies {
                let at: Vec<_> = p.iter().filter(|p| p.item == copy).collect();
                assert_eq!(at.len(), 1);
                moved |= at[0].check.hash > 2;
            }
            assert!(
                p.iter()
                    .all(|p| !MAPS_COMPASSES.iter().any(|m| **m == *p.item))
            );
        }
        assert!(moved);
        let p = shuffle(&checks, 7, &none, &game(false, &[])).unwrap();
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
        let d = drops(&checks, &p, Lines::default());
        for (hash, item) in [(3, keys::SMALL), (4, keys::BOSS)] {
            let now = holding(&p, hash);
            assert_eq!(d.contains_key(&hash), now != item);
        }
        let p = shuffle(&checks, 7, &none, &game(false, &[])).unwrap();
        assert!(drops(&checks, &p, Lines::default()).is_empty());
    }

    /// A minigame's chest can't turn into a substitute one, so it drops
    /// the ammo's Red Rupee copy, with no swap.
    #[test]
    fn a_prize_chest_drops_ammos_copy() {
        needs_names!();
        let prize = ItemCheck {
            hash: gifts::PRIZE_CHESTS[0],
            ..chest(1, "Item_A")
        };
        for (ammo, _) in ammo::NEEDS {
            let placed = [Placement {
                check: &prize,
                item: ammo,
            }];
            let lines = Lines::widest(false);
            let drops = drops(std::slice::from_ref(&prize), &placed, lines);
            let want = Drop::from(ammo::copy(ammo, lines).unwrap());
            assert_eq!(drops.get(&prize.hash), Some(&want), "{ammo}");
        }
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
        let sword = item::SWORD_KOKIRI.get();
        let checks = [chest(1, sword), chest(2, "Item_B")];
        let none = Fixed::new();
        for vanilla in [false, true] {
            let p = shuffle(&checks, 7, &none, &game(vanilla, &["kokiri-sword"])).unwrap();
            let mut all = [holding(&p, 1), holding(&p, 2)];
            all.sort();
            assert_eq!(all, ["Item_B", starting::FILLER]);
        }
        let p = shuffle(&checks, 7, &none, &game(true, &["kokiri-sword"])).unwrap();
        assert_eq!(holding(&p, 1), starting::FILLER);
    }

    #[test]
    fn a_starting_item_names_the_check_whose_slot_it_left() {
        needs_names!();
        let sword = item::SWORD_KOKIRI.get();
        let checks = [chest(1, "Item_A"), chest(2, sword), chest(3, "Item_C")];
        // Plando takes chest 1's slot, which moves the slots after it.
        let fixed = Fixed::from([(1, "Item_B".to_string())]);
        for vanilla in [false, true] {
            let deal = deal(&checks, &fixed, &game(vanilla, &["kokiri-sword"])).unwrap();
            assert_eq!(deal.started, [(sword, 2)], "vanilla {vanilla}");
        }
    }

    #[test]
    fn a_starting_item_outside_the_pool_changes_nothing() {
        needs_names!();
        let checks = [chest(1, "Item_A"), chest(2, "Item_B")];
        let none = Fixed::new();
        let p = shuffle(&checks, 7, &none, &game(true, &["deku-shield"])).unwrap();
        assert_eq!((holding(&p, 1), holding(&p, 2)), ("Item_A", "Item_B"));
        let deal = deal(&checks, &none, &game(true, &["deku-shield"])).unwrap();
        assert!(deal.started.is_empty());
    }

    #[test]
    fn plando_still_places_a_starting_item() {
        needs_names!();
        let sword = item::SWORD_KOKIRI.get();
        let checks = [chest(1, sword), chest(2, "Item_B")];
        let fixed = Fixed::from([(2, sword.to_string())]);
        let p = shuffle(&checks, 7, &fixed, &game(false, &["kokiri-sword"])).unwrap();
        assert_eq!(holding(&p, 2), sword);
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
    fn an_unmoved_line_chest_is_left_to_the_hook() {
        let ([_, (long, _)], none) = (line(0), Fixed::new());
        let race = ItemCheck {
            get_event: Some(flag::MINI_GAME_DAMPE_RACE_OPEN_GOAL_T_BOX.to_string()),
            ..chest(3, long)
        };
        let checks = [chest(1, long), chest(2, "Item_B"), race];
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        assert!(p.iter().all(|p| p.check.hash != 3));
        assert!(drops(&checks, &p, Lines::default()).is_empty());
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

    fn progressive(vanilla: bool, start: &[&str]) -> NewGame {
        NewGame {
            progressive: true,
            ..game(vanilla, start)
        }
    }

    #[test]
    fn gifts_hold_an_item_that_asks_only_when_they_give_by_level() {
        needs_names!();
        let (checks, none) = (with_scrubs(), Fixed::new());
        let mut pool: Vec<_> = checks
            .iter()
            .map(|c| progressive::Lines::default().first(&c.item))
            .collect();
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
    fn plando_puts_an_item_that_asks_on_any_npc_gift_a_sale_included() {
        needs_names!();
        let checks = with_gifts();
        let bombs = |g: &gifts::Gift| Fixed::from([(g.hash(), ammo::NEEDS[0].0.to_string())]);
        let hook = |g: &gifts::Gift| Fixed::from([(g.hash(), line(0)[0].0.to_string())]);
        // Saria and the sales give the bombs or a Red Rupee, and the
        // Hookshot or the Longshot, by what Link owns.
        let sales = gifts::GIFTS.iter().filter(|g| g.is_sale());
        for g in std::iter::once(&gifts::GIFTS[0]).chain(sales) {
            assert!(shuffle(&checks, 7, &bombs(g), &game(false, &[])).is_ok());
            assert!(shuffle(&checks, 7, &hook(g), &game(false, &[])).is_ok());
            assert!(shuffle(&checks, 7, &hook(g), &progressive(true, &[])).is_ok());
        }
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
        let d = drops(&checks, &p, Lines::default());
        assert!(gifts::GIFTS.iter().all(|g| !d.contains_key(&g.hash())));
    }

    #[test]
    fn kept_chests_hold_their_own_item_and_the_rest_still_deal() {
        needs_names!();
        let sword = item::SWORD_KOKIRI.get();
        let mut checks = with_gifts();
        checks.push(chest(20, sword));
        let new_game = NewGame {
            keep_chests: true,
            ..game(false, &["kokiri-sword"])
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
        let fairy = item::OCARINA_FAIRY.get();
        let [saria, zelda] = [0, 1].map(|i| gifts::GIFTS[i].hash());
        let p = shuffle(&checks, 7, &none, &progressive(true, &["fairy-ocarina"])).unwrap();
        assert_eq!(holding(&p, saria), starting::FILLER);
        assert_eq!(holding(&p, zelda), item::OCARINA_TIME);
        let p = shuffle(&checks, 7, &none, &progressive(false, &["fairy-ocarina"])).unwrap();
        let held: Vec<_> = p.iter().filter(|p| p.item == fairy).collect();
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
        let d = drops(&checks, &p, Lines::default());
        assert_eq!(d, HashMap::from([(1, Drop::from("Item_A".to_owned()))]));
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
            drop(bombs, Lines::default()),
            Drop {
                item: ammo::WITHOUT.to_string(),
                swap: Some(swap)
            }
        );
        for item in [hook, "Item_A"] {
            assert_eq!(drop(item, Lines::default()), Drop::from(item.to_owned()));
        }
    }

    #[test]
    fn an_unshuffled_bomb_chest_gives_a_rupee_until_link_has_the_bag() {
        needs_names!();
        let (bombs, none) = (ammo::NEEDS[0].0, Fixed::new());
        let checks = [chest(1, bombs), chest(2, "Item_B")];
        let p = shuffle(&checks, 7, &none, &game(true, &[])).unwrap();
        let d = drops(&checks, &p, Lines::default());
        assert_eq!(d, HashMap::from([(1, drop(bombs, Lines::default()))]));
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

    #[test]
    fn king_zora_moved_deals_the_letter_as_an_empty_bottle() {
        needs_names!();
        let checks = [chest(1, item::BOTTLE_LETTER), chest(2, "Item_A")];
        let none = Fixed::new();
        let count = |d: &Deal, i: &str| d.items.iter().filter(|&&x| x == i).count();
        let dealt = deal(&checks, &none, &game(false, &[])).unwrap();
        assert_eq!(count(&dealt, item::BOTTLE_LETTER), 1);
        let moved = NewGame {
            on: [presets::KING_ZORA_MOVED.to_owned()].into(),
            ..game(false, &[])
        };
        let dealt = deal(&checks, &none, &moved).unwrap();
        assert_eq!(count(&dealt, item::BOTTLE_LETTER), 0);
        assert_eq!(count(&dealt, item::BOTTLE), 1);
    }

    #[test]
    fn met_zelda_pins_the_letter_and_deals_no_egg() {
        needs_names!();
        let checks = with_gifts();
        let none = Fixed::new();
        let hash = |i: &Name| gifts::GIFTS.iter().find(|g| g.item == i).unwrap().hash();
        let count = |p: &[Placement], i: &Name| p.iter().filter(|p| p.item == i.get()).count();
        let met = NewGame {
            on: [MET_ZELDA.to_owned()].into(),
            ..game(false, &[])
        };
        for seed in 0..20 {
            let p = shuffle(&checks, seed, &none, &met).unwrap();
            assert_eq!(
                holding(&p, hash(item::ZELDA_LETTER)),
                item::ZELDA_LETTER.get()
            );
            assert_eq!(count(&p, item::ZELDA_LETTER), 1, "seed {seed}");
            assert_eq!(count(&p, item::WEIRD_EGG), 0, "seed {seed}");
        }
        let p = shuffle(&checks, 0, &none, &game(false, &[])).unwrap();
        assert_eq!(count(&p, item::WEIRD_EGG), 1);
        // A plando can't put anything else at the letter's check.
        let hook = Fixed::from([(hash(item::ZELDA_LETTER), line(0)[0].0.to_string())]);
        assert!(deal(&checks, &hook, &met).is_err());
    }

    #[test]
    fn gerudo_fortress_freed_pins_the_card_and_deals_no_second() {
        needs_names!();
        let checks = with_gifts();
        let none = Fixed::new();
        let card = gifts::GIFTS
            .iter()
            .find(|g| g.item == item::TOKEN_GERUDO)
            .unwrap();
        let freed = NewGame {
            on: [crate::settings::FORTRESS_FREED.to_owned()].into(),
            ..game(false, &[])
        };
        for seed in 0..20 {
            let p = shuffle(&checks, seed, &none, &freed).unwrap();
            assert_eq!(holding(&p, card.hash()), item::TOKEN_GERUDO.get());
            let cards = p.iter().filter(|p| p.item == item::TOKEN_GERUDO.get());
            assert_eq!(cards.count(), 1, "seed {seed}");
        }
    }
}
