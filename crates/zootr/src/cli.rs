//! The command line: parses arguments and prints what the library returns.
//!
//! `args` holds the command tree and the handlers sit beside it, one module
//! per family of commands.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;
use ounce_formats::Romfs;

use crate::config::Config;
use crate::progress::Run;
use crate::{events, flows, game, generate, spawn, world};
use args::{ApCommand, Cli, Command, DrillReport, SweepReport};
use play::{
    Shoot, Sweep, boot_command, drill, drill_report, look, photo, play, sweep, sweep_report,
};
use research::{census, chest_events, cutscenes, flags, flow, items, logic, logic_set};
use running::{ap_connect, dev, give, live, mem, set};
use save::save_command;
use seed::{print_written, shuffle};
use selftest::selftest;
use target::target_command;
use turn::turn;

mod ap;
pub(crate) mod args;
mod dev_args;
mod flagmap;
mod logic_args;
mod new_game;
mod play;
mod research;
mod running;
mod save;
mod seed;
mod selftest;
mod target;
mod turn;

pub use seed::shuffle_command;

pub fn run() -> Result<()> {
    crate::logging::to_stderr();
    let cli = Cli::parse();
    let path = || -> Result<PathBuf> {
        Ok(match cli.game.clone() {
            Some(p) => p,
            None => Config::load()?
                .game
                .unwrap_or_else(|| "dump/oot.nup".into()),
        })
    };
    // The dump's files, and the dump, for the commands that read them.
    let dump = || -> Result<(Romfs, PathBuf)> {
        let path = path()?;
        Ok((game::prepare(&path, None, &Run::silent())?, path))
    };
    let romfs = || Ok::<_, anyhow::Error>(dump()?.0);
    match cli.command {
        Command::Turn(args) => turn(args),
        Command::Target(cmd) => target_command(cmd),
        Command::Extract => {
            println!(
                "{}",
                game::folder(&path()?, None, &Run::silent())?.display()
            );
            Ok(())
        }
        // Its run reads the dump as its first step.
        Command::Shuffle(args) => shuffle(&path()?, args),
        Command::Ap(ApCommand::Open { file, install }) => seed::ap_open(&path()?, &file, install),
        Command::Ap(ApCommand::Room { cmd }) => ap::room_command(&path()?, cmd),
        Command::Ap(ApCommand::Bench(args)) => ap::bench_command(&path()?, args),
        Command::Census => census(&romfs()?),
        Command::Ap(ApCommand::Connect {
            server,
            slot,
            password,
            timings,
        }) => ap_connect(&romfs()?, server, slot, password, timings),
        Command::Ap(ApCommand::Export { out }) => {
            let dir = out.unwrap_or_else(crate::ap::data_dir);
            crate::ap::write(&romfs()?, &dir)?;
            println!("{}", dir.display());
            Ok(())
        }
        Command::Exits => {
            for e in world::exits(&romfs()?)? {
                println!(
                    "{:<28} {} {:<28} {:<44} {:<36} {}",
                    e.from,
                    e.era,
                    e.to,
                    e.to_start,
                    e.name.as_deref().unwrap_or("-"),
                    world::shown(e.presence.as_ref())
                );
            }
            Ok(())
        }
        Command::Spawns => {
            for d in spawn::destinations(&romfs()?)? {
                println!("{d}");
            }
            Ok(())
        }
        Command::ChestEvents => chest_events(&romfs()?),
        Command::Effects { event } => {
            let flows = events::Flows::load(&romfs()?)?;
            let Some(effects) = flows.effects(&event) else {
                bail!("no flow defines {event}")
            };
            effects.iter().for_each(|e| println!("{e}"));
            Ok(())
        }
        Command::Cutscenes => cutscenes(&romfs()?),
        Command::Items => items(&romfs()?),
        Command::Logic {
            edit: Some(edit), ..
        } => {
            let (romfs, path) = dump()?;
            logic_set(&romfs, &path, edit)
        }
        Command::Logic { new_game, .. } => logic(&romfs()?, &new_game),
        Command::Flags => {
            let survey = flags(&romfs()?)?;
            for (flag, u) in &survey.presence {
                println!(
                    "{flag} +{} -{} in {}",
                    u.when_set,
                    u.when_unset,
                    u.areas.iter().cloned().collect::<Vec<_>>().join(",")
                );
                println!("    {}", survey.describe(flag)?);
            }
            Ok(())
        }
        Command::Flag { name } => {
            println!("{}", flags(&romfs()?)?.describe(&name)?);
            Ok(())
        }
        Command::Flagmap { json, query } => flagmap::flagmap(&romfs()?, query, json),
        Command::Flow { name, mod_dir } => match flow(&romfs()?, &name, mod_dir.as_deref())? {
            Some(flow) => {
                print!("{}", flows::render(&flow));
                Ok(())
            }
            None => bail!("{name} only holds a timeline"),
        },
        Command::Probe(probe) => {
            let (romfs, path) = dump()?;
            crate::probe::run(&romfs, &path, probe)
        }
        Command::Selftest => selftest(&romfs()?),
        Command::SetDrop {
            hash,
            item,
            new_game,
            out,
        } => {
            let (drops, new_game) = (HashMap::from([(hash, item.into())]), new_game.resolve()?);
            let (romfs, run) = (romfs()?, Run::silent());
            let (written, _) = out.write(&run, |d| {
                generate::write_mod(&romfs, d, &drops, &new_game, &[], &run)
            })?;
            print_written(&written);
            Ok(())
        }
        Command::Save(cmd) => save_command(&romfs()?, cmd),
        // These read the game's names, which opening the dump registers.
        Command::Boot { start, limit } => {
            dump()?;
            boot_command(start, limit)
        }
        Command::Live { grep, all, named } => live(&romfs()?, grep, all, named),
        Command::Give { actor, count } => give(&romfs()?, &actor, count),
        Command::Set { name, value } => set(&romfs()?, &name, &value),
        Command::Dev(cmd) => {
            let (romfs, path) = dump()?;
            dev(&romfs, &path, cmd)
        }
        Command::Mem(cmd) => {
            dump()?;
            mem(cmd)
        }
        Command::Look {
            questions,
            vision,
            keep,
            frame,
        } => look(
            &vision.vision()?,
            &questions,
            keep.as_deref(),
            frame.as_deref(),
        ),
        Command::Photo {
            area,
            hash,
            out,
            list,
            go,
            side,
            step,
            here,
            orbit,
            raise,
            day_skulltulas,
            vision,
        } => {
            use crate::sweep::photo::{Mode, Swing};
            let mode = match (go, here) {
                (true, _) => Mode::Go(side.map(|s| (s, step))),
                (_, true) => Mode::Here,
                _ => Mode::Pass,
            };
            let shoot = Shoot {
                area: area.as_deref(),
                hashes: &hash,
                out: out.as_deref().filter(|_| !list),
                mode,
                swing: Swing { orbit, raise },
                day_skulltulas,
            };
            let (romfs, path) = dump()?;
            photo(&romfs, &path, shoot, || vision.vision())
        }
        Command::Play {
            script,
            attach,
            stall,
            start,
        } => play(&romfs()?, &script, attach, stall, start),
        Command::Sweep {
            report:
                Some(SweepReport::Report {
                    out,
                    from,
                    to,
                    drills,
                }),
            ..
        } => {
            dump()?;
            sweep_report(&out, from, to, &drills)
        }
        Command::Sweep {
            out: Some(out),
            rounds,
            limit,
            mix,
            only,
            seed,
            yield_to,
            sides,
            judge,
            ..
        } => {
            let (romfs, path) = dump()?;
            let night = Sweep {
                rounds,
                limit,
                mix,
                only,
                seed,
                yield_to,
                sides,
            };
            sweep(&romfs, &path, judge, out, night)
        }
        Command::Drill {
            report: Some(DrillReport::Report { out }),
            ..
        } => drill_report(&out),
        Command::Drill {
            out: Some(out),
            skill,
            name,
            repeat,
            judge,
            ..
        } => {
            let (romfs, path) = dump()?;
            drill(&romfs, &path, judge, out, skill, name.as_deref(), repeat)
        }
        Command::Sweep { .. } | Command::Drill { .. } => bail!("--out is required"),
    }
}
