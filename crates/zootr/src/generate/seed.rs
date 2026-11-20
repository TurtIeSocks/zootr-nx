//! Deals a seed's chests and builds its report.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Result, ensure};
use ounce_formats::Romfs;

use super::{Extras, Held, Report, RoundEvent, Spot, Start, write_mod};
use crate::logic::{Got, World};
use crate::names::Names;
use crate::progress::{Run, Step};
use crate::rules::{self, Book, Ctx, Have, Need};
use crate::settings::{self, Logic, NewGame, Settings};
use crate::shuffle::{self, Fixed};
use crate::{checks, fill, gamedata, hints, ice_traps, icons, places, steps};

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
    let new_game = &NewGame {
        seed,
        ..new_game.clone()
    };
    run.step(Step::Checks)?;
    let checks = checks::dealt(romfs, new_game)?;
    let logic = new_game.logic == Logic::Glitchless && !new_game.vanilla;
    let mut deal = shuffle::deal(&checks, fixed, new_game)?;
    let (mut placements, filled) = match logic {
        true => {
            run.step(Step::Logic)?;
            let ctx = Ctx::new(new_game);
            let world = World::build(romfs, &Book::load()?, &ctx, &checks)?;
            run.step(Step::Placing)?;
            let needed = world.items();
            ice_traps::swap(romfs, &mut deal.items, &needed, new_game.ice_traps, seed)?;
            let filled = fill::fill(&world, &checks, deal, seed, new_game, run)?;
            (filled.placements.clone(), Some((world, filled)))
        }
        false => {
            run.step(Step::Placing)?;
            let none = BTreeSet::new();
            ice_traps::swap(romfs, &mut deal.items, &none, new_game.ice_traps, seed)?;
            (shuffle::scatter(deal, seed, new_game)?, None)
        }
    };
    let needed = filled.as_ref().map(|(world, _)| world.items());
    let traps = ice_traps::disguise(romfs, &mut placements, needed.as_ref(), seed)?;
    let new_game = &NewGame {
        foreign: [new_game.foreign.clone(), traps].concat(),
        ..new_game.clone()
    };
    let drops = shuffle::drops(&checks, &placements);
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
    })
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
        settings: on.collect(),
    }
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
/// extras' fields and gifts join the presets' own, never replace them.
fn new_game(settings: &Settings, extras: &Extras) -> Result<NewGame> {
    let mut new_game = NewGame {
        spawn: extras.spawn.clone(),
        ..NewGame::from_settings(settings, &extras.defaults)?
    };
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
    let mut new_game = new_game(settings, extras)?;
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
        let game = new_game(&settings, &extras).unwrap();
        let presets = crate::presets::fields(&[crate::presets::JABU_OPEN.into()]).unwrap();
        assert_eq!(game.fields, [presets, extras.fields].concat());
    }
}
