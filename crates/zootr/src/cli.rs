//! The command line: parses arguments and prints what the library returns.
//!
//! `args` holds the command tree and the handlers sit beside it, one module
//! per family of commands.

use std::collections::HashMap;

use anyhow::{Result, bail};
use clap::Parser;

use crate::config::Config;
use crate::progress::Run;
use crate::vision::Vision;
use crate::{events, flows, game, generate, spawn, world};
use args::{ApCommand, Cli, Command, DrillReport, SweepReport};
use play::{boot_command, drill, drill_report, look, play, sweep, sweep_report};
use research::{census, chest_events, cutscenes, flags, flow, items, logic};
use running::{dev, give, live, mem, set};
use save::save_command;
use seed::{print_written, shuffle};
use selftest::selftest;
use target::target_command;
use turn::turn;

pub(crate) mod args;
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
    let cli = Cli::parse();
    if let Command::Turn {
        to,
        note,
        session,
        force,
        urgent,
        on,
    } = cli.command
    {
        return turn(to, note, session, force, urgent, on);
    }
    if let Command::Target(cmd) = cli.command {
        return target_command(cmd);
    }
    let path = match cli.game {
        Some(p) => p,
        None => Config::load()?
            .game
            .unwrap_or_else(|| "dump/oot.nup".into()),
    };
    if let Command::Extract = cli.command {
        println!("{}", game::folder(&path, &Run::silent())?.display());
        return Ok(());
    }
    // Its run reads the dump as its first step.
    match cli.command {
        Command::Shuffle(args) => return shuffle(&path, args),
        Command::Ap(ApCommand::Open { file, install }) => {
            return seed::ap_open(&path, &file, install);
        }
        _ => {}
    }
    let romfs = game::prepare(&path, &Run::silent())?;
    match cli.command {
        Command::Extract
        | Command::Target(_)
        | Command::Turn { .. }
        | Command::Shuffle(_)
        | Command::Ap(ApCommand::Open { .. }) => {
            unreachable!("handled above")
        }
        Command::Census => census(&romfs),
        Command::Ap(ApCommand::Export { out }) => {
            let dir = out.unwrap_or_else(crate::ap::data_dir);
            crate::ap::write(&romfs, &dir)?;
            println!("{}", dir.display());
            Ok(())
        }
        Command::Exits => {
            for e in world::exits(&romfs)? {
                println!(
                    "{:<28} {} {:<28} {:<44} {:<36} {}",
                    e.from, e.era, e.to, e.to_start, e.name, e.presence
                );
            }
            Ok(())
        }
        Command::Spawns => {
            for d in spawn::destinations(&romfs)? {
                println!("{d}");
            }
            Ok(())
        }
        Command::ChestEvents => chest_events(&romfs),
        Command::Effects { event } => {
            let flows = events::Flows::load(&romfs)?;
            let Some(effects) = flows.effects(&event) else {
                bail!("no flow defines {event}")
            };
            effects.iter().for_each(|e| println!("{e}"));
            Ok(())
        }
        Command::Cutscenes => cutscenes(&romfs),
        Command::Items => items(&romfs),
        Command::Logic { new_game } => logic(&romfs, &new_game),
        Command::Flags => {
            let survey = flags(&romfs)?;
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
            println!("{}", flags(&romfs)?.describe(&name)?);
            Ok(())
        }
        Command::Flow { name, mod_dir } => match flow(&romfs, &name, mod_dir.as_deref())? {
            Some(flow) => {
                print!("{}", flows::render(&flow));
                Ok(())
            }
            None => bail!("{name} only holds a timeline"),
        },
        Command::Probe(probe) => crate::probe::run(&romfs, &path, probe),
        Command::Selftest => selftest(&romfs),
        Command::SetDrop {
            hash,
            item,
            new_game,
            out,
        } => {
            let (drops, new_game) = (HashMap::from([(hash, item.into())]), new_game.resolve()?);
            let run = Run::silent();
            let (written, _) = out.write(&run, |d| {
                generate::write_mod(&romfs, d, &drops, &new_game, &[], &run)
            })?;
            print_written(&written);
            Ok(())
        }
        Command::Save(cmd) => save_command(&romfs, cmd),
        Command::Boot { start, limit } => boot_command(start, limit),
        Command::Live { grep, all, named } => live(&romfs, grep, all, named),
        Command::Give { actor, count } => give(&romfs, &actor, count),
        Command::Set { name, value } => set(&romfs, &name, &value),
        Command::Dev(cmd) => dev(&romfs, &path, cmd),
        Command::Mem(cmd) => mem(cmd),
        Command::Look {
            questions,
            model,
            api,
            url,
            key_file,
            effort,
            keep,
            frame,
        } => {
            let vision = Vision::new(api, url, model, key_file, effort)?;
            look(&vision, &questions, keep.as_deref(), frame.as_deref())
        }
        Command::Play {
            script,
            attach,
            stall,
            start,
        } => play(&romfs, &script, attach, stall, start),
        Command::Sweep {
            report:
                Some(SweepReport::Report {
                    out,
                    from,
                    to,
                    drills,
                }),
            ..
        } => sweep_report(&out, from, to, &drills),
        Command::Sweep {
            out: Some(out),
            rounds,
            limit,
            mix,
            only,
            seed,
            judge,
            ..
        } => sweep(&romfs, &path, judge, out, (rounds, limit, mix, only, seed)),
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
        } => drill(&romfs, &path, judge, out, (skill, name, repeat)),
        Command::Sweep { .. } | Command::Drill { .. } => bail!("--out is required"),
    }
}
