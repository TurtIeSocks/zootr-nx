//! Deals a seed's chests and builds its report.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Result, ensure};
use ounce_formats::Romfs;

use super::{Door, Extras, Held, Report, RoundEvent, Spot, Start, write_mod};
use crate::entrances::Doors;
use crate::logic::{Data, Got, World};
use crate::names::Names;
use crate::progress::{Run, Step};
use crate::rng::Rng;
use crate::rules::{self, Book, Ctx, Have, Need};
use crate::settings::{self, DungeonEntrances, Logic, NewGame, Settings};
use crate::shuffle::{self, Fixed};
use crate::world::Era;
use crate::{checks, fill, gamedata, hints, ice_traps, icons, places, steps};

/// How many door layouts a seed tries before it gives up. About one in
/// twelve rolled layouts can be beaten with Ganon's Castle in place
/// (Jabu-Jabu's Belly wants a child door, the child-only doors no adult
/// boss; `probe doors` counts them), so two hundred tries fail about once in
/// seventeen million seeds.
pub(crate) const DOOR_TRIES: u32 = 200;

/// Shuffle the chests around `fixed`, unless `new_game` keeps them vanilla,
/// and write the mod.
pub fn shuffle(
    romfs: &Romfs,
    mod_dir: &Path,
    seed: u64,
    fixed: &Fixed,
    new_game: &NewGame,
    run: &Run,
) -> Result<Report> {
    let mut doors = Rng::new(seed.rotate_left(29));
    let roll = |rng: &mut Rng| Doors::roll(new_game.dungeon_entrances, &new_game.door_pins, rng);
    let mut new_game = NewGame {
        seed,
        doors: roll(&mut doors)?,
        ..new_game.clone()
    };
    run.step(Step::Checks)?;
    let checks = checks::dealt(romfs, &new_game)?;
    let logic = new_game.logic == Logic::Glitchless && !new_game.vanilla;
    let (mut placements, filled) = match logic {
        true => {
            let book = Book::load()?;
            let shuffles = new_game.dungeon_entrances != DungeonEntrances::Off;
            let start = fill::start(&new_game);
            let mut data = Data::read(romfs, &checks)?;
            let mut build = |new_game: &NewGame| {
                let world = World::from_data(&mut data, &book, &Ctx::new(new_game), &checks)?;
                let deal = shuffle::deal(&checks, fixed, new_game)?;
                Ok::<_, anyhow::Error>((world, deal))
            };
            // Whether vanilla doors leave the seed beatable, asked once.
            let mut vanilla = None;
            let mut tries = 1;
            loop {
                run.step(Step::Logic)?;
                let (world, mut deal) = build(&new_game)?;
                let mut reroll = shuffles && !fill::beatable(&world, &checks, &deal, &start);
                if reroll && vanilla.is_none() {
                    let doors = NewGame {
                        doors: Doors::default(),
                        ..new_game.clone()
                    };
                    let (world, deal) = build(&doors)?;
                    vanilla = Some(fill::beatable(&world, &checks, &deal, &start));
                }
                reroll &= vanilla == Some(true);
                run.step(Step::Placing)?;
                let filled = match reroll {
                    true => None,
                    false => {
                        let needed = world.items();
                        ice_traps::swap(romfs, &mut deal.items, &needed, new_game.ice_traps, seed)?;
                        match fill::fill(&world, &checks, deal, seed, &new_game, run) {
                            Ok(filled) => Some(filled),
                            Err(e) if !shuffles || tries == DOOR_TRIES => return Err(e),
                            Err(_) => None,
                        }
                    }
                };
                if let Some(filled) = filled {
                    break (filled.placements.clone(), Some((world, filled)));
                }
                // A layout logic can't beat, like the Water Temple behind
                // Jabu-Jabu's child-only door, rolls new doors.
                ensure!(
                    shuffles && tries < DOOR_TRIES,
                    "no door layout logic can beat in {DOOR_TRIES} tries"
                );
                run.check()?;
                tries += 1;
                new_game.doors = roll(&mut doors)?;
            }
        }
        false => {
            run.step(Step::Placing)?;
            let mut deal = shuffle::deal(&checks, fixed, &new_game)?;
            let none = BTreeSet::new();
            ice_traps::swap(romfs, &mut deal.items, &none, new_game.ice_traps, seed)?;
            (shuffle::scatter(deal, seed, &new_game)?, None)
        }
    };
    let needed = filled.as_ref().map(|(world, _)| world.items());
    let traps = ice_traps::disguise(romfs, &mut placements, needed.as_ref(), seed)?;
    let foreign = [new_game.foreign.clone(), traps].concat();
    let new_game = &NewGame {
        foreign,
        ..new_game
    };
    let drops = shuffle::drops(
        &checks,
        &placements,
        crate::progressive::Lines::of(new_game),
    );
    let have = fill::start(new_game);
    let actors = placements
        .iter()
        .flat_map(|p| [p.check.item.as_str(), p.item])
        .chain(have.items.keys().map(String::as_str));
    let names = (Names::load(romfs, actors)?)
        .progressive(new_game.progressive)
        .keys(new_game.keys)
        .foreign(&new_game.foreign);
    let mut spots: Vec<Spot> = placements.iter().map(|p| Spot::new(p, &names)).collect();
    places::fill(romfs, &mut spots)?;
    spots.sort_by(|a, b| (&a.area_name, a.name(), a.hash).cmp(&(&b.area_name, b.name(), b.hash)));
    let hints = match (new_game.hints, &filled) {
        (true, Some((world, _))) => hints::choose(&spots, &world.items(), seed),
        _ => vec![],
    };
    let (mut events, mut unprovable) = (Vec::new(), Vec::new());
    if let Some((world, filled)) = &filled {
        events = rounds(world, filled, &mut spots);
        let needed = fill::way(world, &have, filled);
        for s in &mut spots {
            s.hero = needed.contains(&s.hash);
            (s.need, s.need_items) = need(world, s.hash);
            s.child = world.only_child(s.hash);
        }
        let spot = |h: &u64| spots.iter().find(|s| s.hash == *h);
        let names = filled.unprovable.iter().filter_map(|c| spot(&c.hash));
        unprovable = names
            .map(|s| format!("{}: {}", s.area_name, s.name()))
            .collect();
    }
    run.step(Step::Writing)?;
    let written = write_mod(romfs, mod_dir, &drops, new_game, &hints, run)?;
    Ok(Report {
        seed,
        spots,
        written,
        events,
        way: vec![],
        playthrough: vec![],
        unprovable,
        start: start(&have, new_game, &names),
        entrances: entrances(new_game),
    })
}

/// Each door the seed shuffles, with the dungeon behind it.
fn entrances(new_game: &NewGame) -> Vec<Door> {
    let doors = new_game.doors.shuffled(new_game.dungeon_entrances);
    doors
        .map(|(door, dungeon)| Door {
            id: door.id.into(),
            area: door.door.into(),
            vanilla: door.label.into(),
            dungeon: dungeon.id.into(),
            dungeon_name: dungeon.label.into(),
        })
        .collect()
}

/// What logic starts Link with, named: `have`'s items, and the settings
/// its rules read that are on.
fn start(have: &Have, new_game: &NewGame, names: &Names) -> Start {
    let mut items: Vec<Held> = (have.items.iter())
        .map(|(item, &count)| Held {
            item: item.clone(),
            name: names.dealt(item),
            count,
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let read = rules::settings_read();
    let on = (settings::schema().into_iter())
        .filter(|d| read.contains(&d.id) && new_game.on.contains(&d.id))
        .map(|d| d.label.to_owned());
    Start {
        items,
        settings: age(new_game).into_iter().chain(on).collect(),
    }
}

/// The starting age in the spoiler's words, when it isn't a plain child.
fn age(new_game: &NewGame) -> Option<String> {
    let label = match new_game.age {
        Era::Child => "Child",
        Era::Adult => "Adult",
    };
    let random = if new_game.random_age { " (random)" } else { "" };
    (new_game.age == Era::Adult || new_game.random_age)
        .then(|| format!("Starting age: {label}{random}"))
}

/// Number the playthrough's rounds that collect a source or a check whose
/// item a rule names, mark those checks' spots with theirs, and return the
/// sources by round.
fn rounds(world: &World, filled: &fill::Filled, spots: &mut [Spot]) -> Vec<RoundEvent> {
    let mentioned = world.items();
    let at: HashMap<u64, usize> = (spots.iter().enumerate())
        .filter(|(_, s)| mentioned.contains(s.item.as_str()))
        .map(|(i, s)| (s.hash, i))
        .collect();
    let (mut events, mut n) = (Vec::new(), 0);
    for round in &filled.play.rounds {
        let checks = round.iter().filter_map(|g| match g {
            Got::Check(h) => at.get(h).copied(),
            Got::Source(_) => None,
        });
        let checks: Vec<usize> = checks.collect();
        let sources = round.iter().filter_map(|g| match g {
            Got::Source(i) => Some(world.sources[*i].name.clone()),
            Got::Check(_) => None,
        });
        let sources: Vec<String> = sources.collect();
        if checks.is_empty() && sources.is_empty() {
            continue;
        }
        n += 1;
        checks.into_iter().for_each(|i| spots[i].round = Some(n));
        events.extend(
            sources
                .into_iter()
                .map(|name| RoundEvent { round: n, name }),
        );
    }
    events
}

/// The rule at a check's locations, eras joined with "or", and the items
/// it names.
fn need(world: &World, hash: u64) -> (String, Vec<String>) {
    let needs: Vec<&Need> = (world.locations.iter())
        .filter(|l| l.hash == hash)
        .map(|l| &l.need)
        .collect();
    let mut texts: Vec<String> = Vec::new();
    for text in needs.iter().map(|n| n.to_string()) {
        if !texts.contains(&text) {
            texts.push(text);
        }
    }
    let wrap = |t: &String| match texts.len() > 1 && t.contains(' ') {
        true => format!("({t})"),
        false => t.clone(),
    };
    let text = texts.iter().map(wrap).collect::<Vec<_>>().join(" or ");
    let mut items = BTreeSet::new();
    needs.iter().for_each(|n| n.items(&mut items));
    (text, items.into_iter().map(str::to_owned).collect())
}

/// The new game `settings` start, with what `extras` adds on top. The
/// extras' fields and gifts join the presets' own, never replace them. A
/// random age is rolled from `seed` first, so it forces what its pick does.
fn new_game(settings: &Settings, extras: &Extras, seed: u64) -> Result<NewGame> {
    let (settings, random_age) = settings::roll_age(settings, seed);
    let mut new_game = NewGame {
        spawn: extras.spawn.clone(),
        door_pins: extras.doors.clone(),
        random_age,
        ..NewGame::from_settings(&settings, &extras.defaults)?
    };
    ensure!(
        new_game.age == Era::Child || new_game.spawn.is_none(),
        "an adult start can't take --spawn: spawns are child maps"
    );
    new_game.fields.extend(extras.fields.iter().cloned());
    new_game.give.extend(extras.give.iter().cloned());
    Ok(new_game)
}

pub fn generate(
    romfs: &Romfs,
    mod_dir: &Path,
    seed: u64,
    settings: &Settings,
    extras: &Extras,
    run: &Run,
) -> Result<Report> {
    let ap = extras.ap.as_ref().map(|ap| ap.resolve(romfs)).transpose()?;
    ensure!(
        ap.is_none() || extras.place.is_empty(),
        "an Archipelago seed places every item itself"
    );
    let mut new_game = new_game(settings, extras, seed)?;
    ensure!(
        ap.is_none() || new_game.dungeon_entrances == DungeonEntrances::Off,
        "an Archipelago seed can't shuffle dungeon entrances yet"
    );
    ensure!(
        ap.is_none() || new_game.age == Era::Child,
        "an Archipelago seed starts as child for now"
    );
    if let Some(ap) = &ap {
        new_game.foreign = ap.foreign.clone();
        new_game.players = extras
            .ap
            .as_ref()
            .map(|s| s.players.clone())
            .unwrap_or_default();
        new_game.ints.push(crate::ap::RECEIVED.into());
        new_game.give.extend(ap.give.iter().cloned());
    }
    let place = ap.as_ref().map_or(&extras.place, |ap| &ap.fixed);
    if !extras.progress.is_empty() {
        let gd = gamedata::GameData::load(romfs)?;
        new_game
            .progress
            .extend(steps::parse_all(&gd, &extras.progress)?);
    }
    let report = shuffle(romfs, mod_dir, seed, place, &new_game, run)?;
    run.step(Step::Spoiler)?;
    let icons = icons::icons(seed, settings, extras);
    std::fs::write(mod_dir.join("spoiler.txt"), report.spoiler(&icons))?;
    Ok(report)
}

/// A seed from the clock, for when none is given.
pub fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_fields_join_the_presets_fields() {
        needs_names!();
        let settings = Settings::from([(crate::presets::JABU_OPEN.into(), true.into())]);
        let extras = Extras {
            fields: vec!["1.FakeField=1".into()],
            ..Extras::default()
        };
        let game = new_game(&settings, &extras, 0).unwrap();
        let presets = crate::presets::fields(&[crate::presets::JABU_OPEN.into()]).unwrap();
        assert_eq!(game.fields, [presets, extras.fields].concat());
    }

    #[test]
    fn an_adult_start_refuses_a_spawn() {
        let settings = Settings::from([(settings::STARTING_AGE.into(), "adult".into())]);
        let extras = Extras {
            spawn: Some("FakeMap_Child/FakeSpot".into()),
            ..Extras::default()
        };
        let err = new_game(&settings, &extras, 0).unwrap_err();
        assert!(err.to_string().contains("--spawn"), "{err}");
    }

    /// Seeds whose first ten layouts logic couldn't beat.
    #[test]
    fn a_seed_rolls_doors_until_logic_can_beat_them() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-doors-{}", std::process::id()));
        let settings = crate::settings::Settings::from([
            ("shuffle-dungeon-entrances".into(), "on".into()),
            ("open-forest".into(), true.into()),
            ("kakariko-gate".into(), true.into()),
        ]);
        let new_game = NewGame::from_settings(&settings, &[]).unwrap();
        assert_eq!(new_game.dungeon_entrances, DungeonEntrances::On);
        for seed in [2, 3] {
            let report = shuffle(
                &romfs,
                &dir,
                seed,
                &crate::shuffle::Fixed::new(),
                &new_game,
                &Run::silent(),
            );
            assert_eq!(report.unwrap().entrances.len(), 11, "seed {seed}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
