//! The commands that read the game's data and print what they find, for
//! research: placements, flows, flags, items, cutscenes and what logic reaches.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{Flowchart, Romfs};

use super::args::NewGame;
use super::logic_args::{LogicCommand, RoomEntry};
use crate::known::{field, flag};
use crate::rules::rooms::write;
use crate::{arrivals, checks, cut, cutscenes, events, fill, flags, flows, items, logic, rules};

pub(super) fn cutscenes(romfs: &Romfs) -> Result<()> {
    let catalogue = cutscenes::Catalogue::load(romfs)?;
    let all = events::Flows::load(romfs)?;
    for c in &catalogue.0 {
        let (tier, why) = match &c.tier {
            cutscenes::Tier::Clean => ("clean", String::new()),
            cutscenes::Tier::Timeline => ("timeline", String::new()),
            cutscenes::Tier::Effects(why) => {
                let cut = match cut::judge(&all, c) {
                    cut::Verdict::Cut(plan) => format!("cut, copying {}", plan.copies.len()),
                    cut::Verdict::Kept(why) => format!("kept: {why}"),
                };
                ("effects", format!("{why} [{cut}]"))
            }
        };
        let mut from: Vec<&str> = c.flows.iter().map(String::as_str).collect();
        if c.map {
            from.push("map");
        }
        println!(
            "{:<40} {:<16} {:<8} {:<5} {:<60} {}",
            c.name,
            c.area.unwrap_or("-"),
            tier,
            if c.flag { "flag" } else { "-" },
            from.join(","),
            why
        );
    }
    let skippable = catalogue.0.iter().filter(|c| c.tier.skippable()).count();
    eprintln!("{} cutscenes, {skippable} skippable", catalogue.0.len());
    for a in arrivals::all(romfs, &all)? {
        let how = match &a.skip {
            arrivals::Skip::Flag(flag) => format!("set {flag}"),
            arrivals::Skip::Swap { flow, .. } => format!("swap in {flow}"),
            arrivals::Skip::Plays(why) => format!("plays: {why}"),
        };
        println!("arrival {:<44} {:<40} {how}", a.event, a.flag);
    }
    Ok(())
}

pub(super) fn census(romfs: &Romfs) -> Result<()> {
    let checks = checks::all(romfs)?;
    let mut kinds = HashSet::<&str>::new();
    for c in &checks {
        kinds.insert(&c.item);
        println!(
            "{:<60} {:<32} {:>20} {:<32} {:<48} {}",
            c.file,
            c.holder,
            c.hash,
            c.item,
            c.get_event.as_deref().unwrap_or("-"),
            c.presence.as_ref().map_or("-".into(), |p| p.to_string())
        );
    }
    let unique: HashSet<u64> = checks.iter().map(|c| c.hash).collect();
    eprintln!(
        "{} placements, {} unique actors, {} item kinds",
        checks.len(),
        unique.len(),
        kinds.len()
    );
    Ok(())
}

/// `name` as `mod_dir` rewrote it, else the game's own.
pub(super) fn flow(romfs: &Romfs, name: &str, mod_dir: Option<&Path>) -> Result<Option<Flowchart>> {
    let rel = flows::rel(name);
    let Some(file) = mod_dir
        .map(|d| d.join("romfs").join(&rel))
        .filter(|f| f.exists())
    else {
        return flows::load(romfs, name);
    };
    let data = romfs.dicts().decompress(&rel, &std::fs::read(&file)?)?;
    Flowchart::parse(&data).with_context(|| format!("parsing {}", file.display()))
}

pub(super) fn chest_events(romfs: &Romfs) -> Result<()> {
    let flows = events::Flows::load(romfs)?;
    let mut seen = HashSet::new();
    for c in checks::scan(romfs)? {
        let Some(event) = c.get_event.as_deref() else {
            continue;
        };
        if !seen.insert(c.hash) {
            continue;
        }
        let area = c.file.split('/').nth(1).unwrap_or("?");
        println!("{area} {} {} {event}", c.hash, c.item);
        match flows.effects(event) {
            Some(effects) => effects.iter().for_each(|e| println!("    {e}")),
            None => println!("    (no flow defines this event)"),
        }
        if flows.shows_off(event) {
            println!("    (only shows off the item: dropped once randomized)");
        }
    }
    Ok(())
}

pub(super) fn items(romfs: &Romfs) -> Result<()> {
    let mut drops = HashMap::<String, usize>::new();
    for c in checks::scan(romfs)? {
        *drops.entry(c.item).or_default() += 1;
    }
    let mut items = items::scan(romfs)?;
    items.sort_by_cached_key(|i| (i.category(), i.actor.clone()));
    for i in &items {
        println!(
            "{:<8} {:<11} {:<11} {:<4} {:>3}  {}",
            i.category(),
            i.field(flag::GET_TYPE),
            i.field(field::TAB_CATEGORY),
            if i.assignable.is_some() { "btn" } else { "-" },
            drops.get(&i.actor).copied().unwrap_or(0),
            i.actor,
        );
    }
    Ok(())
}

pub(super) fn flags(romfs: &Romfs) -> Result<flags::Survey> {
    flags::Survey::load(romfs, &events::Flows::load(romfs)?)
}

pub(super) fn logic(romfs: &Romfs, new_game: &NewGame) -> Result<()> {
    let new_game = new_game.resolve()?;
    let checks = checks::dealt(romfs, &new_game)?;
    let world = logic::World::build(
        romfs,
        &rules::Book::load()?,
        &rules::Ctx::new(&new_game),
        &checks,
    )?;
    let start = fill::start(&new_game);
    let vanilla: HashMap<u64, &str> = checks.iter().map(|c| (c.hash, c.item.as_str())).collect();
    let play = world.play(&start, &vanilla);
    let pool = checks.iter().map(|c| c.item.as_str());
    let all = world.play(&fill::everything(&start, pool), &vanilla);
    let (reach, reach_all) = (world.reach(&play.have), world.reach(&all.have));
    let total: HashSet<u64> = world.locations.iter().map(|l| l.hash).collect();
    let (node, age) = logic::start_of(new_game.age);
    println!("start {node} {age}");
    println!(
        "{:<8} {:>7} {:>10} {:>6}",
        "", "vanilla", "every item", "total"
    );
    let row = |what, a: usize, b: usize, n: usize| println!("{what:<8} {a:>7} {b:>10} {n:>6}");
    row("places", reach.len(), reach_all.len(), world.places.len());
    row("checks", play.reached.len(), all.reached.len(), total.len());
    let got: Vec<_> = play
        .rounds
        .iter()
        .flatten()
        .filter_map(|g| match g {
            logic::Got::Source(i) => Some(world.sources[*i].name.as_str()),
            logic::Got::Check(_) => None,
        })
        .collect();
    let missed: Vec<_> = world
        .sources
        .iter()
        .map(|s| s.name.as_str())
        .filter(|n| !got.contains(n))
        .collect();
    println!(
        "vanilla rounds {}, sources out of reach: {}",
        play.rounds.len(),
        missed.join(", ")
    );
    println!();
    for (i, place @ (node, era)) in world.places.iter().enumerate() {
        let mark = match (reach.contains(place), reach_all.contains(place)) {
            (true, _) => "vanilla",
            (_, true) => "all",
            _ => "-",
        };
        let here: HashSet<u64> = world
            .locations
            .iter()
            .filter(|l| l.place == i)
            .map(|l| l.hash)
            .collect();
        let proved = here.iter().filter(|h| all.reached.contains(h)).count();
        let obstacles = world.obstacles.get(node).map_or(String::new(), |o| {
            o.iter()
                .map(|(tool, n)| format!("{tool} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        });
        println!(
            "{node:<36} {era:<5} {mark:<7} {proved:>3}/{:<3} {obstacles}",
            here.len()
        );
    }
    Ok(())
}

pub(super) fn logic_set(romfs: &Romfs, game: &std::path::Path, cmd: LogicCommand) -> Result<()> {
    match cmd {
        LogicCommand::Explain {
            dungeon,
            at,
            within,
            picture,
            rooms,
            y,
        } => {
            anyhow::ensure!(
                at.is_some() || (picture.is_some() && !rooms.is_empty()),
                "name a spot (`--at X,Y,Z`), or rooms to draw (`--picture PNG --rooms 5,6`)"
            );
            let at = match at.as_deref() {
                None => None,
                Some(&[x, y, z]) => Some([x, y, z]),
                Some(_) => anyhow::bail!("`--at` takes three numbers, `X,Y,Z`"),
            };
            let ask = crate::derive::rooms::explain::Ask {
                at,
                within,
                picture,
                rooms,
                y,
            };
            print!(
                "{}",
                crate::derive::rooms::explain::explain(romfs, game, dungeon_of(&dungeon)?, &ask)?
            );
        }
        LogicCommand::Audit { dungeon } => {
            let only = match dungeon {
                Some(id) => Some(crate::rules::rooms::folder(dungeon_of(&id)?.node.get())),
                None => None,
            };
            let book = crate::rules::Book::load()?;
            let audits = crate::rules::audit::audit(&book, only.as_deref());
            anyhow::ensure!(!audits.is_empty(), "no room files for that dungeon yet");
            audits.iter().for_each(|a| print!("{a}"));
        }
        LogicCommand::QuestGates => {
            let (graph, book) = (
                crate::flagmap::Graph::build(romfs)?,
                crate::rules::Book::load()?,
            );
            print!(
                "{}",
                crate::rules::quest_gates::audit(romfs, &graph, &book)?
            );
        }
        LogicCommand::Derive {
            kinds,
            dungeon,
            dir,
        } => derive(romfs, game, kinds, dungeon, &dir)?,
        LogicCommand::SetExit {
            from,
            to,
            needs,
            dir,
        } => set_way(&dir, &rules::Way::Exit { from, to }, &needs)?,
        LogicCommand::SetInside { place, needs, dir } => {
            set_way(&dir, &rules::Way::Inside { place }, &needs)?;
        }
        LogicCommand::RoomSet {
            dungeon,
            room,
            hash,
            needs,
            why,
            evidence,
            dir,
        } => {
            let dungeon = dungeon_of(&dungeon)?;
            let path =
                write::set_check(&dir, dungeon, room, hash, &needs, &why, evidence.as_deref())?;
            println!("{}", path.display());
        }
        LogicCommand::RoomAdd {
            dungeon,
            room,
            entry,
            evidence,
            dir,
        } => {
            let entry = match entry {
                RoomEntry::Link {
                    from,
                    to,
                    via,
                    needs,
                    why,
                } => write::Add::Link {
                    from,
                    to,
                    via,
                    needs,
                    why,
                },
                RoomEntry::Flag {
                    hash,
                    name,
                    at,
                    needs,
                    why,
                } => write::Add::Flag {
                    hash,
                    name,
                    at,
                    needs,
                    why,
                },
                RoomEntry::Floor { at, why } => write::Add::Floor { at, why },
            };
            let path = write::add(
                &dir,
                dungeon_of(&dungeon)?,
                room,
                &entry,
                evidence.as_deref(),
            )?;
            println!("{}", path.display());
        }
        LogicCommand::Rooms { dungeon, json, dir } => {
            let rooms = write::rooms(&dir, dungeon_of(&dungeon)?)?;
            if json {
                let rows: Vec<_> = (rooms.iter())
                    .map(|(room, name)| serde_json::json!({ "room": room, "name": name }))
                    .collect();
                println!("{}", serde_json::to_string(&rows)?);
            } else {
                rooms.iter().for_each(|(r, n)| println!("{r:>2}  {n}"));
            }
        }
        LogicCommand::Set {
            hash,
            name,
            needs,
            place,
            era,
            dir,
        } => {
            let edit = rules::Edit {
                name,
                needs,
                place: place.zip(era),
            };
            let new = || {
                let ledger = crate::ledger::build(romfs)?;
                let row = (ledger.rows.into_iter())
                    .find(|r| r.hash == hash)
                    .with_context(|| format!("no check {hash} in the ledger"))?;
                Ok((row.area, row.name))
            };
            println!("{}", rules::set(&dir, hash, &edit, new)?.display());
        }
    }
    Ok(())
}

/// `zootr logic derive`: the object kinds, a dungeon's room files, or both.
fn derive(
    romfs: &Romfs,
    game: &Path,
    kinds: bool,
    dungeon: Option<String>,
    dir: &Path,
) -> Result<()> {
    anyhow::ensure!(kinds || dungeon.is_some(), "name a dungeon, or `--kinds`");
    if kinds {
        let (count, guesses) = crate::derive::kinds(romfs, dir)?;
        println!(
            "{count} kinds, {guesses} guesses: {}",
            dir.join("baseline/kinds.toml").display()
        );
    }
    let Some(id) = dungeon else {
        return Ok(());
    };
    let dungeon = dungeon_of(&id)?;
    let d = crate::derive::rooms::write(romfs, game, dungeon, dir)?;
    println!("{} room files for {}", d.files.len(), dungeon.label);
    println!("{} of {} faces in no room", d.unplaced, d.faces);
    if !d.exitless.is_empty() {
        println!("exits in no room: {:?}", d.exitless);
    }
    if !d.roomless.is_empty() {
        println!("checks in no room: {:?}", d.roomless);
    }
    if !d.floorless.is_empty() {
        println!("rooms with no floor: {:?}", d.floorless);
    }
    for w in &d.warnings {
        println!("warning: {w}");
    }
    let at = |p: [f32; 3]| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]);
    for j in &d.jumps {
        let (from, to) = (at(j.at.0), at(j.at.1));
        println!(
            "jump {} to {}: {:.1} m, {from} to {to}",
            j.from, j.to, j.width
        );
    }
    Ok(())
}

/// `zootr logic set-exit` and `set-inside`: what `way` needs, written.
fn set_way(dir: &Path, way: &rules::Way, needs: &str) -> Result<()> {
    println!("{}", rules::set_way(dir, way, needs)?.display());
    Ok(())
}

fn dungeon_of(id: &str) -> Result<&'static crate::entrances::Dungeon> {
    (crate::entrances::DUNGEONS.iter())
        .find(|d| d.id == id)
        .with_context(|| format!("no dungeon `{id}`"))
}
