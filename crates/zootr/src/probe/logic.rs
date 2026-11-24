//! Randomizer subcommands: shops, groups, exits, gates, the pouch, tools,
//! checks and settings.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use ounce_formats::{Romfs, murmur3};

use super::filter::{Filter, hits};
use super::{actor_names, show};
use crate::checks::{banc_files, load_banc, str_field};
use crate::gamedata::GameData;
use crate::known::field;
use crate::tools::Tools;

pub(super) fn shops(romfs: &Romfs, seed: u64) -> Result<()> {
    for slot in crate::shops::slots(romfs)? {
        let price = crate::shops::price(seed, slot.hash());
        let rest: Vec<String> = slot.placements[1..].iter().map(u64::to_string).collect();
        println!(
            "{} {} {} {} {price} {}",
            slot.shop,
            slot.number,
            slot.hash(),
            slot.goods,
            rest.join(",")
        );
    }
    Ok(())
}

pub(super) fn groups(romfs: &Romfs, file: &str, logic: &str) -> Result<()> {
    for rel in banc_files(romfs)?.into_iter().filter(|r| r.contains(file)) {
        let doc = load_banc(romfs, &rel)?;
        let Some(groups) = doc.root.as_map()?.get(&**field::AI_GROUPS) else {
            continue;
        };
        for g in groups.as_array()? {
            if str_field(g, "Logic").is_some_and(|l| l.contains(logic)) {
                println!("{rel} {}", show(g));
            }
        }
    }
    Ok(())
}

/// Map exits touching `node`, each once, with its gate flag and rule.
pub(super) fn exits(romfs: &Romfs, node: &str) -> Result<()> {
    let node = node.to_lowercase();
    // A node answers to its name in the dump and its word in the rule files.
    let touches = |name: &str| {
        let word = crate::rules::vocab::place_word(name);
        name.to_lowercase().contains(&node) || word.is_some_and(|w| w.contains(&node))
    };
    let book = crate::rules::Book::load()?;
    let rule = |from: &str, to: &str| {
        let r = book.exits.iter().find(|r| r.from == from && r.to == to);
        r.map_or("always", |r| r.needs.as_str()).to_owned()
    };
    let row = |era: &str, from: &str, to: &str, gate: &str, need: &str| {
        format!("{era:<5} {from:<28} -> {to:<28} {gate:<24} {need}")
    };
    let mut rows = BTreeSet::new();
    for e in crate::world::exits(romfs)? {
        if touches(&e.from) || touches(&e.to) {
            let need = rule(&e.from, &e.to);
            let gate = crate::world::shown(e.presence.as_ref());
            rows.insert(row(&e.era.to_string(), &e.from, &e.to, &gate, &need));
        }
    }
    for r in book.exits.iter().filter(|r| r.added) {
        if touches(&r.from) || touches(&r.to) {
            rows.insert(row("added", &r.from, &r.to, "-", &r.needs));
        }
    }
    rows.iter().for_each(|r| println!("{r}"));
    Ok(())
}

pub(super) fn gates(
    romfs: &Romfs,
    trace: bool,
    node: Option<String>,
    creators: bool,
    checks: bool,
) -> Result<()> {
    if creators {
        let only: Option<std::collections::HashSet<u64>> = match checks {
            true => Some(
                crate::pickups::watched(romfs)?
                    .iter()
                    .map(|c| c.hash)
                    .collect(),
            ),
            false => None,
        };
        let creators = crate::gates::creators(romfs)?.into_iter();
        for c in creators.filter(|c| only.as_ref().is_none_or(|o| o.contains(&c.hash))) {
            let (hash, area, gyaml) = (c.hash, &c.area, &c.gyaml);
            let quiet = if c.quiet { "quiet" } else { "loud" };
            println!(
                "{hash} {area} {gyaml} {}#{} x{} {quiet}",
                c.graph, c.node, c.placed
            );
        }
        return Ok(());
    }
    if let Some(node) = node {
        for g in crate::gates::trace_node(romfs, &node)? {
            println!("{} {}\n    {}", g.hash, g.area, g.expr);
        }
        return Ok(());
    }
    let (mut wired, mut gated) = (0, 0);
    for g in crate::gates::trace(romfs)? {
        let rule = crate::gates::rule(&g.expr);
        wired += !g.expr.atoms().is_empty() as usize;
        gated += rule.is_some() as usize;
        match (trace, rule) {
            (true, rule) => {
                let rule = rule.unwrap_or_default();
                println!("{} {} {rule}\n    {}", g.hash, g.area, g.expr);
            }
            (false, Some(rule)) => println!("{} {} {rule}", g.hash, g.area),
            (false, None) => {}
        }
    }
    eprintln!("{wired} chests wired to a graph, {gated} of them gated");
    Ok(())
}

pub(super) fn pouch(romfs: &Romfs, slot: &str, all: bool) -> Result<()> {
    let gd = GameData::load(romfs)?;
    let save = crate::savefile::load(&crate::savefile::path(slot)?)?;
    let names = actor_names(romfs)?;
    let value = |hash: u32| {
        save.get(hash)
            .and_then(|e| save.scalar(e))
            .unwrap_or_default()
    };
    for (actor, stock) in crate::inventory::stock_keys(&gd)? {
        let name = names
            .get(&actor)
            .cloned()
            .unwrap_or_else(|| format!("#{actor:08x}"));
        let (num, got) = (
            value(stock) as i32,
            value(murmur3(format!("IsGet.{name}").as_bytes())),
        );
        if all || num != 0 || got != 0 {
            println!("{num:>5} {} {name}", if got != 0 { "got" } else { "   " });
        }
    }
    for e in save.entries() {
        let held: Vec<_> = save
            .strings(*e)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if !held.is_empty() {
            println!("{} {} {held:?}", e.kind, gd.name(e.hash));
        }
    }
    Ok(())
}

pub(super) fn tools(romfs: &Romfs, filter: &Filter) -> Result<()> {
    let mut tools = Tools::new(romfs);
    let mut counts = BTreeMap::<_, usize>::new();
    for hit in hits(romfs, filter)? {
        if let Some(rule) = tools.of(&hit.actor)? {
            let guess = tools.guessed(&hit.gyaml)?;
            *counts.entry((hit.gyaml, rule, guess)).or_default() += 1;
        }
    }
    for ((gyaml, rule, guess), n) in counts {
        let guess = if guess { "guess" } else { "" };
        println!("{n:>5} {gyaml:<56} {rule:<44} {guess}");
    }
    Ok(())
}

/// Rolls `rolls` dungeon door layouts and prints how many logic can beat,
/// and how often a seed's tries would all fail.
pub(super) fn doors(
    romfs: &Romfs,
    rolls: u32,
    settings: &[(String, serde_json::Value)],
) -> Result<()> {
    use crate::settings::{DungeonEntrances, NewGame, Settings};
    let mut settings: Settings = settings.iter().cloned().collect();
    (settings.entry("shuffle-dungeon-entrances".into())).or_insert("on".into());
    let new_game = NewGame::from_settings(&settings, &[])?;
    anyhow::ensure!(
        new_game.dungeon_entrances != DungeonEntrances::Off,
        "doors don't shuffle"
    );
    let checks = crate::checks::dealt(romfs, &new_game)?;
    let book = crate::rules::Book::load()?;
    let start = crate::fill::start(&new_game);
    let mut data = crate::logic::Data::read(romfs, &checks)?;
    let fixed = crate::shuffle::Fixed::new();
    let mut rng = crate::rng::Rng::new(1);
    let mut beaten = 0;
    for _ in 0..rolls {
        let doors = crate::entrances::Doors::roll(
            new_game.dungeon_entrances,
            &new_game.door_pins,
            &mut rng,
        )?;
        let new_game = NewGame {
            doors,
            ..new_game.clone()
        };
        let ctx = crate::rules::Ctx::new(&new_game);
        let world = crate::logic::World::from_data(&mut data, &book, &ctx, &checks)?;
        let deal = crate::shuffle::deal(&checks, &fixed, &new_game)?;
        beaten += u32::from(crate::fill::beatable(&world, &checks, &deal, &start));
    }
    let odds = f64::from(beaten) / f64::from(rolls);
    let tries = crate::generate::DOOR_TRIES;
    println!(
        "{beaten}/{rolls} layouts beatable ({:.1}%), so a seed's {tries} tries all fail for {:.1e} of seeds",
        odds * 100.0,
        (1.0 - odds).powi(tries as i32)
    );
    Ok(())
}

/// Fills seeds 1 to `seeds` and prints how many logic beats within its
/// tries, and the last failure's reason.
pub(super) fn seeds(
    romfs: &Romfs,
    seeds: u64,
    settings: &[(String, serde_json::Value)],
) -> Result<()> {
    use crate::settings::{NewGame, Settings};
    let new_game = NewGame::from_settings(&settings.iter().cloned().collect::<Settings>(), &[])?;
    let checks = crate::checks::dealt(romfs, &new_game)?;
    let book = crate::rules::Book::load()?;
    let ctx = crate::rules::Ctx::new(&new_game);
    let world = crate::logic::World::build(romfs, &book, &ctx, &checks)?;
    let fixed = crate::shuffle::Fixed::new();
    let run = crate::progress::Run::silent();
    let (mut beaten, mut last) = (0, None);
    for seed in 1..=seeds {
        let deal = crate::shuffle::deal(&checks, &fixed, &new_game)?;
        match crate::fill::fill(&world, &checks, deal, seed, &new_game, &run) {
            Ok(_) => beaten += 1,
            Err(e) => last = Some(format!("seed {seed}: {e:#}")),
        }
    }
    println!("{beaten}/{seeds} seeds beatable");
    if let Some(last) = last {
        println!("last failure: {last}");
    }
    Ok(())
}

/// Each dealt check with what logic says it needs, one line per era, or
/// with `never` only those no age can get.
pub(super) fn checks(
    romfs: &Romfs,
    area: Option<&str>,
    settings: &[(String, serde_json::Value)],
    never: bool,
) -> Result<()> {
    use crate::generate::Spot;
    use crate::settings::{NewGame, Settings};
    let new_game = NewGame::from_settings(&settings.iter().cloned().collect::<Settings>(), &[])?;
    let all = crate::checks::dealt(romfs, &new_game)?;
    let fixed = crate::shuffle::Fixed::new();
    let deal = crate::shuffle::deal(&all, &fixed, &new_game)?;
    let book = crate::rules::Book::load()?;
    let ctx = crate::rules::Ctx::new(&new_game);
    let world = crate::logic::World::build(romfs, &book, &ctx, &all)?;
    let free = deal
        .free
        .iter()
        .filter(|c| area.is_none_or(|a| c.area.contains(a)))
        .filter(|c| !never || world.never(c.hash));
    let names = crate::names::Names::load(romfs, deal.free.iter().map(|c| c.item.as_str()))?;
    let mut spots: Vec<Spot> = free
        .map(|&check| {
            Spot::new(
                &crate::shuffle::Placement {
                    check,
                    item: &check.item,
                },
                &names,
            )
        })
        .collect();
    crate::places::fill(romfs, &mut spots)?;
    spots.sort_by(|a, b| (&a.area, a.name()).cmp(&(&b.area, b.name())));
    for s in &spots {
        for l in world.locations.iter().filter(|l| l.hash == s.hash) {
            let (node, era) = &world.places[l.place];
            println!(
                "{:<20} {era:<5} {node:<28} {:<40} {:<24} {}",
                s.hash,
                s.name(),
                s.vanilla,
                l.need
            );
        }
    }
    Ok(())
}

/// Prints the settings schema a group at a time, for auditing its copy.
pub(super) fn settings() {
    use crate::settings::{GROUPS, Kind, schema};
    let defs = schema();
    for group in GROUPS {
        println!("## {group}");
        for d in defs.iter().filter(|d| d.group == group) {
            let default = match &d.kind {
                Kind::Toggle { default } => default.to_string(),
                Kind::Choice { default, .. } => default.clone(),
                Kind::Inventory { .. } => "{}".into(),
            };
            let nest = [
                d.parent.map(|p| format!("under {p}")),
                Some(format!("in {}", d.section)),
                d.requires.map(|r| format!("needs {r}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            println!(
                "{} = {default} {nest}\n  {}: {}",
                d.id, d.label, d.description
            );
            if let Kind::Choice { options, .. } = &d.kind {
                options
                    .iter()
                    .for_each(|o| println!("    {}: {}", o.id, o.label));
            }
        }
    }
}
