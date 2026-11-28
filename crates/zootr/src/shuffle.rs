//! The pool and the random deal. `fill.rs` deals with logic instead.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result, bail};

use crate::checks::ItemCheck;
use crate::keys;
use crate::known::item;
use crate::narrowing::{Let, allowed, asks_at, shuffleable};
use crate::progressive::Lines;
use crate::rng::Rng;
use crate::settings::{MET_ZELDA, NewGame};
use crate::starter_bags;
use crate::starting;
use crate::{bottles, cows, gifts, presets};

mod drops;
pub use drops::drops;

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

/// The pool's items, each with the check (by hash) its slot came from.
struct Pool<'a> {
    items: Vec<&'a str>,
    from: Vec<u64>,
}

impl<'a> Pool<'a> {
    /// One slot per check of `checks`, holding what `item` gives for it.
    fn of(checks: &[&'a ItemCheck], item: impl Fn(&'a ItemCheck) -> &'a str) -> Self {
        Self {
            items: checks.iter().map(|&c| item(c)).collect(),
            from: checks.iter().map(|c| c.hash).collect(),
        }
    }

    /// Remove one `item`, else one `fallback`, else any item, with its
    /// slot. Keeps the pool one item per free chest whatever was placed.
    /// A key copy needs a Red Rupee stand-in ([`keys::deal_copies`]), so
    /// a fallback that is one goes last: a pinned key chest whose item the
    /// pool has no more of leaves its stand-in to the keys.
    fn take(&mut self, item: &str, fallback: &str) {
        let stand_in = |i: &str| *keys::INSTEAD == *i;
        let first = |want: &str| self.items.iter().position(|&i| i == want);
        let at = (first(item))
            .or_else(|| first(fallback).filter(|_| !stand_in(fallback)))
            .or_else(|| self.items.iter().rposition(|&i| !stand_in(i)));
        if let Some(at) = at.or(self.items.len().checked_sub(1)) {
            self.items.swap_remove(at);
            self.from.swap_remove(at);
        }
    }
}

/// Each starting item that left the pool, with the check (by hash) whose
/// slot it left.
pub type Started<'a> = Vec<(&'a str, u64)>;

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
    pub started: Started<'a>,
    /// What settings add to the pool (`starter_bags::added`), at the
    /// level the pool holds each.
    pub added: Vec<&'a str>,
    /// Each of `added` with the rupee it took. One that found none is
    /// left out.
    pub traded: Vec<(&'a str, &'a str)>,
}

/// `fixed` placements, and the free checks with the items to deal them.
pub fn deal<'a>(checks: &'a [ItemCheck], fixed: &'a Fixed, new_game: &NewGame) -> Result<Deal<'a>> {
    let d = Dealer {
        checks,
        fixed,
        new_game,
        lines: Lines::of(new_game),
        open: Let::of(new_game),
    };
    let mut placed = d.plando()?;
    let pins = d.pin_story(&mut placed)?;
    let (kept, free, mut pool) = d.pool(&placed, &pins)?;
    let added: Vec<_> = (starter_bags::added(new_game).into_iter())
        .map(|i| d.level(i))
        .collect();
    let traded = starter_bags::swap_in(&mut pool.items, &added);
    let (started, items) = d.finish(kept, pool, &mut placed)?;
    Ok(Deal {
        placed,
        free,
        items,
        started,
        added,
        traded,
    })
}

/// `check`'s own item as the pool holds it: Weird Egg ✅ holds the cucco
/// (`gifts::egg`).
fn pooled(check: &ItemCheck) -> &str {
    gifts::egg::own(check).unwrap_or_else(|| keys::pooled(check))
}

/// Some checks, by reference.
type Checks<'a> = Vec<&'a ItemCheck>;

/// What a deal reads: the checks, plando's placements and the settings.
struct Dealer<'a, 'n> {
    checks: &'a [ItemCheck],
    fixed: &'a Fixed,
    new_game: &'n NewGame,
    lines: Lines,
    open: Let,
}

impl<'a> Dealer<'a, '_> {
    /// `item` at the level the pool holds it: its line's first with
    /// progressive items on.
    fn level(&self, item: &'a str) -> &'a str {
        match self.new_game.progressive {
            true => self.lines.first(item),
            false => item,
        }
    }

    /// `c`'s own item as it's dealt. Gifts come in a fixed order, Saria's
    /// first, so they keep their own item rather than a line's first level
    /// they couldn't swap.
    fn own(&self, c: &'a ItemCheck) -> &'a str {
        match gifts::is_gift(c) {
            true => pooled(c),
            false => self.level(pooled(c)),
        }
    }

    /// Plando's placements, each at the level the pool holds it.
    fn plando(&self) -> Result<Vec<Placement<'a>>> {
        let mut placed = Vec::new();
        for (&hash, item) in self.fixed {
            let check = self.checks.iter().find(|c| c.hash == hash);
            let check = check.with_context(|| format!("no check with hash {hash}"))?;
            let item = self.level(item);
            let progressive = self.new_game.progressive;
            if let Some(gift) = gifts::of(check).filter(|_| asks_at(check, item, progressive)) {
                bail!(
                    "{} can't give {item}: what it gives depends on what Link has, and a gift can't check",
                    gift.giver
                );
            }
            placed.push(Placement { check, item });
        }
        Ok(placed)
    }

    /// Place the story items a setting starts Link with at their checks,
    /// so the pool holds no second. Returns the checks pinned.
    fn pin_story(&self, placed: &mut Vec<Placement<'a>>) -> Result<Vec<u64>> {
        let mut pins = Vec::new();
        for (hash, item) in gifts::pinned(&self.new_game.on) {
            match self.fixed.get(&hash) {
                Some(own) if own == item => continue,
                Some(own) => {
                    bail!("check {hash} holds {item} with this game's settings, not {own}")
                }
                None => {}
            }
            let check = (self.checks.iter()).find(|c| c.hash == hash && shuffleable(c, self.open));
            if let Some(check) = check {
                placed.push(Placement { check, item });
                pins.push(hash);
            }
        }
        Ok(pins)
    }

    /// The checks kept holding their own item, the free checks, and the
    /// pool to deal them: one item per free check, less what `placed` took.
    fn pool(
        &self,
        placed: &[Placement<'a>],
        pins: &[u64],
    ) -> Result<(Checks<'a>, Checks<'a>, Pool<'a>)> {
        let (new_game, open) = (self.new_game, self.open);
        let placed_at = |hash: &u64| self.fixed.contains_key(hash) || pins.contains(hash);
        // With chests kept, a gift that may not hold its own item keeps it
        // too: only chests could take it off its hands. The minigame prize
        // chests follow Chests, and the gifts no setting deals (Saria's and
        // Zelda's ocarinas) follow NPC Gifts, so with every kind off nothing
        // moves (#531).
        let keeps = |c: &'a ItemCheck| {
            let held = allowed(c, self.level(pooled(c)), new_game.progressive);
            let chest_kept = new_game.keep_chests && (!gifts::is_gift(c) || !held);
            let own = match gifts::of(c) {
                _ if gifts::is_prize(c) => new_game.keep_chests,
                Some(g) if g.setting.is_none() => new_game.keep_gifts || chest_kept,
                _ => chest_kept,
            };
            own && !placed_at(&c.hash)
        };
        // Kept checks hold their own item, placed like plando's.
        let (kept, chests): (Vec<_>, Vec<_>) =
            (unique_chests(self.checks, open)?.into_iter()).partition(|c| keeps(c));
        let mut pool = Pool::of(&chests, pooled);
        for p in placed.iter().filter(|p| shuffleable(p.check, open)) {
            pool.take(p.item, pooled(p.check));
        }
        let free: Vec<_> = chests.into_iter().filter(|c| !placed_at(&c.hash)).collect();
        if new_game.vanilla {
            pool = Pool::of(&free, |c| self.own(c));
        } else {
            pool.items.iter_mut().for_each(|i| *i = self.level(i));
        }
        Ok((kept, free, pool))
    }

    /// Take the starting items out of `pool` and the `kept` checks' own
    /// items, deal the letter as its bottle once King Zora has moved, and
    /// place the kept checks. Then deal the pool's bottles (`bottles`) and
    /// its key copies. Returns each starting item with the check whose slot it
    /// left, and the items to deal.
    fn finish(
        &self,
        kept: Checks<'a>,
        pool: Pool<'a>,
        placed: &mut Vec<Placement<'a>>,
    ) -> Result<(Started<'a>, Vec<&'a str>)> {
        let new_game = self.new_game;
        // One swap over both, so a starting item leaves one spot either way.
        let mut all = Pool::of(&kept, |c| self.own(c));
        all.items.extend(pool.items);
        all.from.extend(pool.from);
        let mut owned = new_game.start.owned();
        if new_game.on.contains(MET_ZELDA) {
            // Met Zelda leaves nothing for the cucco to wake.
            owned.push(item::ALARM_CUCCO.get());
        }
        let slots = starting::swap_out(&mut all.items, &owned, |i| self.level(i));
        let started = (owned.into_iter().zip(slots))
            .filter_map(|(item, at)| Some((item, all.from[at?])))
            .collect();
        if new_game.on.contains(presets::KING_ZORA_MOVED) {
            // Nothing reads the letter once King Zora has moved, so it's dealt
            // as the bottle it comes in, as Ship does.
            for i in all
                .items
                .iter_mut()
                .filter(|i| **i == &**item::BOTTLE_LETTER)
            {
                *i = &**item::BOTTLE;
            }
        }
        let mut items = all.items.split_off(kept.len());
        let from = all.from.split_off(kept.len());
        let kept = kept.into_iter().zip(all.items);
        placed.extend(kept.map(|(check, item)| Placement { check, item }));
        let cows: HashSet<u64> = (self.checks.iter().filter(|c| cows::is_cow(c)))
            .map(|c| c.hash)
            .collect();
        let bottles = bottles::Deal {
            started: new_game.start.bottles(),
            letter: !new_game.on.contains(presets::KING_ZORA_MOVED),
            roll: (!new_game.vanilla).then_some(new_game.seed),
        };
        let cow = |i: usize| cows.contains(&from[i]);
        bottles.apply(&mut items, cow, placed.iter().map(|p| p.item))?;
        let fixed: Vec<_> = placed.iter().map(|p| p.item).collect();
        keys::deal_copies(
            &mut items,
            &new_game.key_modes(),
            new_game.fortress_keys,
            &fixed,
        )?;
        Ok((started, items))
    }
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

#[cfg(test)]
mod tests {
    use crate::known::{Name, item};
    use crate::{progressive, settings};

    use super::*;
    use crate::ammo;
    use crate::narrowing::MAPS_COMPASSES;
    use crate::narrowing::tests::chest;

    /// The game's own magic, so the pool holds only the checks' items.
    pub(super) fn game(vanilla: bool, start: &[&str]) -> NewGame {
        NewGame {
            vanilla,
            start: starting::Start::of(start),
            magic_with_first_item: true,
            ..Default::default()
        }
    }

    pub(super) fn holding<'a>(placements: &'a [Placement], hash: u64) -> &'a str {
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
    pub(super) fn with_keys() -> Vec<ItemCheck> {
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
    fn a_pin_the_pool_lacks_leaves_the_key_stand_ins() {
        needs_names!();
        let red = keys::INSTEAD.get();
        let mut pool = Pool {
            items: vec!["Item_A", red, "Item_B", red],
            from: vec![1, 2, 3, 4],
        };
        pool.take("Item_Fake", red);
        assert_eq!(pool.items, ["Item_A", red, red]);
        pool.take("Item_A", red);
        assert_eq!(pool.items, [red, red]);
        // With only stand-ins left, one goes.
        pool.take("Item_Fake", red);
        assert_eq!(pool.items, [red]);
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
    pub(super) fn line(i: usize) -> [(&'static str, &'static str); 2] {
        let [(a, b), (c, d)] = <[_; 2]>::try_from(progressive::LINES[i]).unwrap();
        [(a.get(), b.get()), (c.get(), d.get())]
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
        let game = progressive(false, &[]);
        // A slot bottle's contents are rolled, so any bottle reads as one.
        let plain = |i: &str| match bottles::is_bottle(i) && i != item::BOTTLE_LETTER.get() {
            true => item::BOTTLE.to_string(),
            false => i.to_owned(),
        };
        // The pool as this seed holds it: each check's item at the seed's
        // line level, and each added item traded in for a rupee. With
        // progressive items the meter stays in the magic line, so the
        // Wisdom fountain's copy reads as one too.
        let lines = progressive::Lines::of(&game);
        let mut pool: Vec<_> = checks
            .iter()
            .map(|c| plain(lines.first(pooled(c))))
            .collect();
        for item in starter_bags::added(&game) {
            if let Some(at) = starter_bags::JUNK
                .iter()
                .find_map(|j| pool.iter().position(|i| *i == j.get()))
            {
                pool[at] = item.to_owned();
            }
        }
        pool.sort();
        for seed in 0..200 {
            let p = shuffle(&checks, seed, &none, &game).unwrap();
            let mut dealt: Vec<_> = p.iter().map(|p| plain(p.item)).collect();
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
        // Weird Egg ✅ holds the cucco, so only Malon's flow changes.
        let egg = |g: &gifts::Gift| g.item == item::WEIRD_EGG;
        for gift in &gifts::GIFTS {
            let own = if egg(gift) {
                item::ALARM_CUCCO
            } else {
                gift.item
            };
            assert_eq!(holding(&p, gift.hash()), own);
        }
        let d = drops(&checks, &p, Lines::default());
        assert!(
            gifts::GIFTS
                .iter()
                .all(|g| d.contains_key(&g.hash()) == egg(g))
        );
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

    /// A full shuffle's slot items (three bottles, the letter, Talon's
    /// milk), eight cows and some others.
    fn bottle_checks() -> Vec<ItemCheck> {
        let mut checks: Vec<_> = (0..3).map(|n| chest(n, item::BOTTLE)).collect();
        checks.push(chest(3, item::BOTTLE_LETTER));
        checks.push(chest(4, cows::VANILLA));
        checks.extend((5..13).map(|n| ItemCheck {
            holder: cows::HOLDER.into(),
            ..chest(n, cows::VANILLA)
        }));
        checks.extend((13..20).map(|n| chest(n, "Item_A")));
        checks
    }

    const STARTING_BOTTLES: [&str; 5] =
        ["bottle-1", "bottle-2", "bottle-3", "bottle-4", "bottle-5"];

    #[test]
    fn the_pool_deals_five_bottles_less_the_starting_ones() {
        needs_names!();
        let checks = bottle_checks();
        let none = Fixed::new();
        let moved = |start: &[&str]| NewGame {
            on: [presets::KING_ZORA_MOVED.to_owned()].into(),
            ..game(false, start)
        };
        let slots = |d: &Deal| d.items.iter().filter(|i| bottles::is_bottle(i)).count();
        let letters = |d: &Deal| {
            d.items
                .iter()
                .filter(|&&i| i == item::BOTTLE_LETTER.get())
                .count()
        };
        let rupees = |d: &Deal| d.items.iter().filter(|&&i| i == bottles::COW.get()).count();
        for n in 0..=5 {
            let start = &STARTING_BOTTLES[..n];
            let dealt = deal(&checks, &none, &moved(start)).unwrap();
            assert_eq!(dealt.items.len(), 20);
            assert_eq!(slots(&dealt), 5 - n, "{n} started, King Zora moved");
            assert_eq!(letters(&dealt), 0);
            assert_eq!(rupees(&dealt), 8, "each cow brings a rupee");
            if n < 5 {
                let dealt = deal(&checks, &none, &game(false, start)).unwrap();
                assert_eq!(slots(&dealt), 5 - n, "{n} started");
                assert_eq!(letters(&dealt), 1, "{n} started: the letter stays");
            }
        }
    }

    #[test]
    fn plando_bottles_past_the_room_are_refused() {
        needs_names!();
        let checks = bottle_checks();
        let fixed = Fixed::from([
            (13, item::BOTTLE.to_string()),
            (14, item::BOTTLE.to_string()),
        ]);
        let dealt = deal(&checks, &fixed, &game(false, &STARTING_BOTTLES[..2])).unwrap();
        assert_eq!(
            dealt.items.iter().filter(|i| bottles::is_bottle(i)).count(),
            1
        );
        let err = deal(&checks, &fixed, &game(false, &STARTING_BOTTLES[..4]))
            .err()
            .unwrap();
        assert!(err.to_string().contains("room for 1"), "{err}");
    }

    #[test]
    fn king_zora_moved_deals_the_letter_as_a_bottle() {
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
        let normal = |i: &str| bottles::CONTENTS.iter().any(|c| c.get() == i);
        assert_eq!(dealt.items.iter().filter(|i| normal(i)).count(), 1);
    }

    #[test]
    fn met_zelda_pins_the_letter_and_deals_no_cucco() {
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
            assert_eq!(count(&p, item::ALARM_CUCCO), 0, "seed {seed}");
            assert_eq!(count(&p, item::WEIRD_EGG), 0, "seed {seed}");
        }
        let p = shuffle(&checks, 0, &none, &game(false, &[])).unwrap();
        // Weird Egg ✅ holds the cucco, and nothing deals the egg.
        assert_eq!(count(&p, item::ALARM_CUCCO), 1);
        assert_eq!(count(&p, item::WEIRD_EGG), 0);
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
