//! The command line: parses arguments and prints what the library returns.
//!
//! `args` holds the command tree and the handlers sit beside it, one module
//! per family of commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::Parser;
use ounce_formats::Romfs;

use crate::config::Config;
use crate::progress::Run;
use crate::{events, flows, game, generate, spawn, world};
use args::{ApCommand, AudioCommand, Cli, ColorsCommand, Command, DrillReport};
use play::{
    Shoot, Sweep, boot_command, drill, drill_report, gauntlet_run, look, photo, play, sweep,
    sweep_report,
};
use research::{census, chest_events, cutscenes, flags, flow, items, logic, logic_set};
use running::{ap_connect, dev, give, live, mem, set};
use save::save_command;
use seed::{print_written, shuffle};
use selftest::selftest;
use sweep_args::{SweepArgs, SweepReport};
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
mod sweep_args;
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
        Command::Audio(AudioCommand::List { pool }) => {
            audio_list(pool);
            Ok(())
        }
        Command::Audio(AudioCommand::Play { song, out }) => {
            std::fs::write(&out, crate::audio::wav(&path()?, &song)?)?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Command::Listen {
            dump,
            songs,
            from,
            to,
            step,
            channels,
        } => listen(&path()?, &dump, channels, (from, to, step), &songs),
        Command::Colors(ColorsCommand::List) => {
            colors_list();
            Ok(())
        }
        Command::Colors(ColorsCommand::Preview {
            id,
            color,
            out,
            texture,
            changes,
        }) => colors_preview(&romfs()?, &id, &color, &out, texture, changes),
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
            let release = crate::game::Release::of(romfs.root());
            let (written, _) = out.write(release, &run, |d| {
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
        Command::Sweep(SweepArgs {
            report:
                Some(SweepReport::Report {
                    out,
                    from,
                    to,
                    drills,
                }),
            ..
        }) => {
            dump()?;
            sweep_report(&out, from, to, &drills)
        }
        Command::Sweep(SweepArgs {
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
        }) => {
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
        Command::Gauntlet(args) => {
            let (romfs, path) = dump()?;
            let (g, out, saves) = args.gauntlet();
            gauntlet_run(&romfs, &path, g, out, saves)
        }
        Command::Sweep(_) | Command::Drill { .. } => bail!("--out is required"),
    }
}

/// `colors list`: one palette a line.
fn colors_preview(
    romfs: &Romfs,
    id: &str,
    color: &str,
    out: &Path,
    texture: usize,
    changes: bool,
) -> Result<()> {
    use crate::colors::{Mode, Pick, parse_arg, preview};
    let picked = match parse_arg(&format!("{id}={color}"))?.1 {
        Pick::Color(picked) => Some(picked),
        Pick::Mode(Mode::Vanilla) => None,
        Pick::Mode(Mode::Random) => bail!("a preview takes a color, not random"),
    };
    let view = match changes {
        true => preview::View::Changes,
        false => preview::View::Paint,
    };
    let png = preview::Previews::default().png(romfs, id, texture, picked.as_ref(), view)?;
    std::fs::write(out, png)?;
    let sheets = preview::sheets(crate::colors::palette(id)?);
    println!("{} ({} of {})", out.display(), texture + 1, sheets.len());
    Ok(())
}

fn colors_list() {
    use crate::colors::{PALETTES, hex};
    let width = PALETTES.iter().map(|p| p.id.len()).max().unwrap_or(0);
    for p in PALETTES {
        let vanilla: Vec<_> = p.vanilla.iter().map(|&c| hex(c)).collect();
        let group = format!("{:?}", p.group).to_lowercase();
        println!(
            "{:<width$}  {group:<5}  {}  {}",
            p.id,
            vanilla.join(" "),
            p.label
        );
    }
}

/// `audio list`: one track a line.
fn audio_list(pool: Option<crate::audio::Pool>) {
    use crate::audio::TRACKS;
    let shown = TRACKS.iter().filter(|t| pool.is_none_or(|p| t.pool == p));
    let width = TRACKS.iter().map(|t| t.id.len()).max().unwrap_or(0);
    for t in shown {
        let song = match t.song {
            Some(_) => "",
            None => "  (sections: receives only)",
        };
        let lasts = if t.looping {
            "loop".to_owned()
        } else {
            format!("{:.1}s", t.secs)
        };
        println!(
            "{:<width$}  {:<7}  {:<6}  {lasts:>6}  {}{song}",
            t.id,
            t.pool.name(),
            t.band_label(),
            t.label
        );
    }
}

/// `listen`: each window of the dump and what plays in it.
fn listen(
    game: &std::path::Path,
    dump: &std::path::Path,
    channels: usize,
    (from, to, step): (f32, Option<f32>, f32),
    songs: &[String],
) -> Result<()> {
    let bytes_per_second = (48_000 * 2 * channels) as f32;
    let to = match to {
        Some(to) => to,
        None => std::fs::metadata(dump)?.len() as f32 / bytes_per_second,
    };
    let windows = crate::audio::listen::listen(game, dump, channels, (from, to, step), songs)?;
    for w in windows {
        let heard = match w.best {
            Some((name, score, at)) => format!("{name:<40} {score:.2} @ {at:6.1}"),
            None => "-".into(),
        };
        println!("{:8.1}  rms {:6.0}  {heard}", w.at, w.rms);
    }
    Ok(())
}
