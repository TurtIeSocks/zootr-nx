//! Glitchless placement. Each important item goes where logic proves Link
//! can reach it without that item, then filler takes the rest. A check
//! logic can't prove a way into only ever gets filler.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};

use crate::checks::ItemCheck;
use crate::gifts;
use crate::logic::{Play, World};
use crate::narrowing::allowed;
use crate::progress::Run;
use crate::rng::Rng;
use crate::rules::Have;
use crate::settings::NewGame;
use crate::shuffle::{Deal, Placement};

/// Deals tried before a seed gives up.
pub const TRIES: u64 = 20;

/// What a new game starts with: its starting items, every dungeon key
/// copy with keys from the start, what presets hand over, and what their
/// edits own or set.
pub fn start(new_game: &NewGame) -> Have {
    let mut have = Have::default();
    new_game.start.owned().into_iter().for_each(|i| have.add(i));
    if new_game.keys == crate::settings::DungeonKeys::Start {
        crate::keys::SINGLES.iter().for_each(|k| have.add(k));
    }
    new_game.give.iter().for_each(|i| have.add(i));
    for edit in &new_game.defaults {
        let Some((name, "true")) = edit.split_once('=') else {
            continue;
        };
        match name.strip_prefix("IsGet.") {
            Some(actor) => have.add(actor),
            None => {
                have.flags.insert(name.to_owned());
            }
        }
    }
    have
}

/// `have` plus every item in `pool`. Sources outside the pool still have
/// to be reached.
pub fn everything<'a>(have: &Have, pool: impl IntoIterator<Item = &'a str>) -> Have {
    let mut all = have.clone();
    pool.into_iter().for_each(|i| all.add(i));
    all
}

/// What each check holds before the deal: its vanilla item, or plando's.
fn fixed<'a>(checks: &'a [ItemCheck], deal: &Deal<'a>) -> HashMap<u64, &'a str> {
    let free: HashSet<u64> = deal.free.iter().map(|c| c.hash).collect();
    let mut at: HashMap<u64, &str> = checks
        .iter()
        .filter(|c| !free.contains(&c.hash))
        .map(|c| (c.hash, crate::keys::held(c)))
        .collect();
    at.extend(deal.placed.iter().map(|p| (p.check.hash, p.item)));
    at
}

/// The free checks logic can show a way into with every item.
pub fn provable(world: &World, checks: &[ItemCheck], deal: &Deal, start: &Have) -> HashSet<u64> {
    let pool = deal.items.iter().copied();
    let reached = world
        .play(&everything(start, pool), &fixed(checks, deal))
        .reached;
    let free = deal.free.iter().map(|c| c.hash);
    free.filter(|h| reached.contains(h)).collect()
}

/// Whether logic reaches Ganondorf once Link holds every item in `deal`.
pub fn beatable(world: &World, checks: &[ItemCheck], deal: &Deal, start: &Have) -> bool {
    let all = everything(start, deal.items.iter().copied());
    world.beaten(&world.play(&all, &fixed(checks, deal)).have)
}

pub struct Filled<'a> {
    pub placements: Vec<Placement<'a>>,
    /// Free checks logic can't prove a way into. They hold filler.
    pub unprovable: Vec<&'a ItemCheck>,
    /// The seed played from a new game.
    pub play: Play,
    /// What every check holds, dealt or not.
    pub at: HashMap<u64, &'a str>,
}

/// Deal `deal` with logic, trying up to `TRIES` shuffles. A stop lands
/// between tries.
pub fn fill<'a>(
    world: &World,
    checks: &'a [ItemCheck],
    deal: Deal<'a>,
    seed: u64,
    new_game: &NewGame,
    run: &Run,
) -> Result<Filled<'a>> {
    let start = start(new_game);
    let base = fixed(checks, &deal);
    let provable = provable(world, checks, &deal, &start);
    let mentioned = world.items();
    let important = |i: &str| mentioned.contains(i);
    let mut last = None;
    for t in 0..TRIES {
        run.detail(format!("try {} of {TRIES}", t + 1));
        run.check()?;
        let mut rng = Rng::new(seed.wrapping_add(t));
        match once(
            world,
            &deal,
            &base,
            &provable,
            &start,
            &important,
            &mut rng,
            new_game.progressive,
        ) {
            Ok((dealt, at, play)) => {
                let mut placements = deal.placed.clone();
                placements.extend(dealt);
                let unprovable = deal.free.iter().filter(|c| !provable.contains(&c.hash));
                return Ok(Filled {
                    placements,
                    unprovable: unprovable.copied().collect(),
                    play,
                    at,
                });
            }
            Err(e) => last = Some(e),
        }
    }
    let why = last.map_or(String::new(), |e| format!("{e:#}"));
    bail!("no beatable seed in {TRIES} tries. The last: {why}")
}

/// One try: important items first, each where Link reaches it without it,
/// then filler, gifts first since they can't hold everything.
#[allow(clippy::too_many_arguments)]
fn once<'a>(
    world: &World,
    deal: &Deal<'a>,
    base: &HashMap<u64, &'a str>,
    provable: &HashSet<u64>,
    start: &Have,
    important: &dyn Fn(&str) -> bool,
    rng: &mut Rng,
    progressive: bool,
) -> Result<(Vec<Placement<'a>>, HashMap<u64, &'a str>, Play)> {
    let (mut todo, mut filler): (Vec<&str>, Vec<&str>) =
        deal.items.iter().partition(|i| important(i));
    rng.shuffle(&mut todo);
    rng.shuffle(&mut filler);
    let mut empty = deal.free.clone();
    rng.shuffle(&mut empty);
    let mut at = base.clone();
    let mut dealt = Vec::new();
    while let Some(item) = todo.pop() {
        let mut have = start.clone();
        todo.iter().for_each(|i| have.add(i));
        let reached = world.play(&have, &at).reached;
        let Some(spot) = empty.iter().position(|c| {
            provable.contains(&c.hash) && reached.contains(&c.hash) && allowed(c, item, progressive)
        }) else {
            bail!("nowhere Link reaches without {item} can hold it");
        };
        let check = empty.swap_remove(spot);
        at.insert(check.hash, item);
        dealt.push(Placement { check, item });
    }
    // Stable, so gifts come first and the rest keep their shuffle.
    empty.sort_by_key(|c| !gifts::is_gift(c));
    for check in empty {
        let Some(i) = filler.iter().position(|f| allowed(check, f, progressive)) else {
            bail!("no filler left that {} can hold", check.holder);
        };
        let item = filler.swap_remove(i);
        at.insert(check.hash, item);
        dealt.push(Placement { check, item });
    }
    let play = world.play(start, &at);
    if let Some(missed) = provable.iter().find(|h| !play.reached.contains(h)) {
        let c = deal.free.iter().find(|c| c.hash == *missed);
        let name = c.map_or(String::new(), |c| format!("{} {} ", c.area, c.holder));
        bail!("{name}({missed}) is out of reach");
    }
    Ok((dealt, at, play))
}

/// Way of the Hero: each placed check whose item Link can't reach Ganondorf
/// without, by hash. Empty when logic shows no way to him at all.
pub fn way(world: &World, start: &Have, filled: &Filled) -> Vec<u64> {
    let beaten = |at: &HashMap<u64, &str>| world.beaten(&world.play(start, at).have);
    if !beaten(&filled.at) {
        return Vec::new();
    }
    let mentioned = world.items();
    let needed = |p: &&Placement| {
        let mut without = filled.at.clone();
        without.remove(&p.check.hash);
        !beaten(&without)
    };
    let placed = filled.placements.iter();
    let important = placed.filter(|p| mentioned.contains(p.item));
    important.filter(needed).map(|p| p.check.hash).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::item;
    use crate::progress::{self, StepEvent};
    use crate::rules::{Book, Ctx};
    use crate::settings::{
        CHESTS, DOOR_OF_TIME, DUNGEON_REWARDS, FORTRESS_KEYS, ICE_TRAPS, KEYS, NPC_GIFTS,
        PROGRESSIVE, SONGS, Settings,
    };
    use crate::shuffle::{Fixed, deal};
    use crate::{ice_traps, key_tints, keys};

    /// Seeds each settings case must fill.
    const SEEDS: u64 = 200;

    fn seeds_fill(settings: &Settings) {
        seeds_fill_and(settings, |_| {});
    }

    /// [`seeds_fill`], handing each filled seed to `each`. The seeds fill
    /// in parallel, since these tests set the suite's wall time.
    fn seeds_fill_and(settings: &Settings, each: impl Fn(&Filled) + Sync) {
        let Some(romfs) = dev_dump() else { return };
        let settings = crate::settings::favorites::randomizer_and(settings.clone());
        let new_game = NewGame::from_settings(&settings, &[]).unwrap();
        let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(
            &romfs,
            &Book::load().unwrap(),
            &Ctx::new(&new_game),
            &checks,
        )
        .unwrap();
        let fixed = Fixed::new();
        let needed = world.items();
        let one = |&seed: &u64| {
            let mut deal = deal(&checks, &fixed, &new_game).unwrap();
            let how = new_game.ice_traps;
            crate::ice_traps::swap(&romfs, &mut deal.items, &needed, how, seed).unwrap();
            match fill(
                &world,
                &checks,
                deal,
                seed * TRIES,
                &new_game,
                &Run::silent(),
            ) {
                Ok(filled) => {
                    each(&filled);
                    None
                }
                Err(e) => Some(format!("seed {seed}: {e:#}")),
            }
        };
        let seeds: Vec<u64> = (0..SEEDS).collect();
        let failed: Vec<String> = ounce_formats::par_map(&seeds, one)
            .into_iter()
            .flatten()
            .collect();
        assert!(
            failed.is_empty(),
            "{} of {SEEDS} failed:\n{}",
            failed.len(),
            failed.join("\n")
        );
    }

    #[test]
    fn the_way_is_every_check_ganondorf_needs_and_holds_the_bow() {
        let Some(romfs) = dev_dump() else { return };
        let settings = crate::settings::favorites::randomizer();
        let new_game = NewGame::from_settings(&settings, &[]).unwrap();
        let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        let book = Book::load().unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        let start = start(&new_game);
        assert!(!world.beaten(&start), "a new game can't reach Ganondorf");
        let fixed = Fixed::new();
        let deal = deal(&checks, &fixed, &new_game).unwrap();
        let filled = fill(&world, &checks, deal, 0, &new_game, &Run::silent()).unwrap();
        assert!(
            world.beaten(&filled.play.have),
            "the seed reaches Ganondorf"
        );
        let way = way(&world, &start, &filled);
        let mentioned = world.items();
        for p in &filled.placements {
            let mut without = filled.at.clone();
            without.remove(&p.check.hash);
            let needed = !world.beaten(&world.play(&start, &without).have);
            let on = way.contains(&p.check.hash);
            assert_eq!(on, needed && mentioned.contains(p.item), "{}", p.item);
        }
        // Gifts that give a quiver add bows to the pool, so no one bow's
        // check is on the way, but Ganondorf can't be reached without all.
        let bows: Vec<_> = (filled.placements.iter())
            .filter(|p| p.item == item::RANGE_BOW)
            .collect();
        assert!(!bows.is_empty(), "the bow is dealt");
        let mut without = filled.at.clone();
        for p in &bows {
            without.remove(&p.check.hash);
        }
        assert!(
            !world.beaten(&world.play(&start, &without).have),
            "Ganondorf needs a bow"
        );
    }

    #[test]
    fn a_stop_between_tries_ends_the_fill_with_stopped() {
        let Some(romfs) = dev_dump() else { return };
        let new_game = NewGame::from_settings(&Settings::new(), &[]).unwrap();
        let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        let book = Book::load().unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let run = Run::silent();
        let (stopper, heard) = (run.stopper(), seen.clone());
        let run = run.with(move |e| {
            if let StepEvent::Detail { text } = e {
                heard.lock().unwrap().push(text.clone());
                stopper.stop();
            }
        });
        let fixed = Fixed::new();
        let deal = deal(&checks, &fixed, &new_game).unwrap();
        let Err(e) = fill(&world, &checks, deal, 0, &new_game, &run) else {
            panic!("the fill went on")
        };
        assert!(progress::stopped(&e).is_some(), "{e:#}");
        assert_eq!(*seen.lock().unwrap(), [format!("try 1 of {TRIES}")]);
    }

    #[test]
    fn default_seeds_reach_every_provable_check() {
        seeds_fill(&Settings::new());
    }

    #[test]
    fn seeds_with_chests_kept_reach_every_provable_check() {
        seeds_fill(&Settings::from([(CHESTS.into(), false.into())]));
        seeds_fill(&Settings::from([
            (CHESTS.into(), false.into()),
            (PROGRESSIVE.into(), false.into()),
            (NPC_GIFTS.into(), true.into()),
            (DUNGEON_REWARDS.into(), true.into()),
            (SONGS.into(), true.into()),
        ]));
    }

    #[test]
    fn closed_door_seeds_reach_every_provable_check() {
        seeds_fill(&Settings::from([(DOOR_OF_TIME.into(), "closed".into())]));
    }

    #[test]
    fn seeds_with_every_gift_dealt_reach_every_provable_check() {
        seeds_fill(&Settings::from([
            (NPC_GIFTS.into(), true.into()),
            (DUNGEON_REWARDS.into(), true.into()),
            (SONGS.into(), true.into()),
        ]));
    }

    #[test]
    fn seeds_with_every_junk_a_trap_reach_every_provable_check() {
        let mayhem = || (ICE_TRAPS.into(), "mayhem".into());
        let traps = |filled: &Filled| {
            let traps = filled
                .placements
                .iter()
                .filter(|p| ice_traps::is_trap(p.item));
            assert!(traps.count() > 0, "traps dealt");
        };
        seeds_fill_and(&Settings::from([mayhem()]), traps);
        seeds_fill(&Settings::from([
            mayhem(),
            (NPC_GIFTS.into(), true.into()),
            (DUNGEON_REWARDS.into(), true.into()),
            (SONGS.into(), true.into()),
        ]));
    }

    #[test]
    fn fortress_keysanity_seeds_fill_with_no_copy_at_the_card_s_check() {
        let fortress = || (FORTRESS_KEYS.into(), "singles".into());
        let dealt = |filled: &Filled| {
            let copy = key_tints::fortress();
            let copies: Vec<_> = (filled.placements.iter())
                .filter(|p| p.item == copy)
                .collect();
            assert_eq!(copies.len(), 4);
            for p in copies {
                assert_ne!(
                    p.check.item,
                    crate::carpenters::guards::CARD.get(),
                    "{}",
                    p.check.hash
                );
            }
        };
        seeds_fill_and(&Settings::from([fortress()]), dealt);
        seeds_fill_and(
            &Settings::from([fortress(), (NPC_GIFTS.into(), true.into())]),
            dealt,
        );
    }

    #[test]
    fn key_ring_seeds_fill_and_keep_each_ring_out_of_the_rooms_that_need_it() {
        let rings = || (KEYS.into(), "rings".into());
        let at_home = std::sync::atomic::AtomicU32::new(0);
        let outside = |filled: &Filled| {
            for p in filled.placements.iter() {
                if !keys::is_shuffled(p.item) {
                    continue;
                }
                let chest = crate::checks::is_chest(&p.check.holder);
                assert!(chest || gifts::is_gift(p.check), "{}", p.item);
                let ring = (1..=keys::DUNGEONS.len()).find(|&n| key_tints::ring(n) == p.item);
                let Some((n, count)) = crate::key_rooms::count(p.check) else {
                    continue;
                };
                if ring == Some(n) {
                    assert_eq!(count, 0, "{} at {}", p.item, p.check.hash);
                    at_home.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        };
        seeds_fill_and(&Settings::from([rings()]), outside);
        // The well's keys can land in the well, in a room no lock closes.
        if dev_dump().is_some() {
            assert!(at_home.into_inner() > 0);
        }
        seeds_fill(&Settings::from([
            rings(),
            (NPC_GIFTS.into(), true.into()),
            (DUNGEON_REWARDS.into(), true.into()),
            (SONGS.into(), true.into()),
        ]));
    }
}
