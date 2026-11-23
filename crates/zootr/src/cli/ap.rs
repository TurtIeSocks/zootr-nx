//! `zootr ap room` and `zootr ap bench`: a local room for tests, and the
//! timings kept run after run.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::args::Install;
use crate::ap::bench::{self, Row, RunInfo};
use crate::ap::room::{self, Archipelago, Room, SLOTS};
use crate::game;
use crate::progress::Run;

#[derive(clap::Subcommand)]
pub(super) enum RoomCommand {
    /// Generate a two-slot multiworld (Link and Zelda) from a preset, host
    /// it, and open Link's slot. One room at a time.
    Up {
        /// A table of `ap/bench/presets.toml`.
        #[arg(long, default_value = "defaults")]
        preset: String,
        #[arg(long, default_value_t = room::SEED)]
        seed: u64,
        #[arg(long, default_value_t = room::PORT)]
        port: u16,
        #[command(flatten)]
        at: At,
        #[command(flatten)]
        install: Install,
    },
    /// Send an item from the room's cheat console.
    Send {
        /// By its AP name, e.g. "Forest Medallion".
        item: String,
        #[arg(long, default_value = "Link")]
        to: String,
    },
    /// Whether the room is up, where, and its server log's tail.
    Status,
    /// Stop the room's server.
    Down,
}

#[derive(clap::Args)]
pub(super) struct At {
    /// The Archipelago checkout, with its `.venv`. Defaults to
    /// `ZOOTR_ARCHIPELAGO`.
    #[arg(long)]
    archipelago: Option<PathBuf>,
}

#[derive(clap::Args)]
pub(super) struct Bench {
    /// A table of `ap/bench/presets.toml`. Repeatable. Defaults to
    /// defaults and all-on.
    #[arg(long)]
    preset: Vec<String>,
    #[arg(long, default_value_t = room::SEED)]
    seed: u64,
    /// Defaults to `docs/research/ap-bench.tsv` in the main checkout.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Print the table without keeping the rows.
    #[arg(long)]
    no_record: bool,
    /// Only the metrics that need no game. Without it, the live metrics
    /// run too: they need this session's turn on the Mac game, and they
    /// boot a new file on each preset's room and quit the game after.
    #[arg(long)]
    offline: bool,
    #[command(flatten)]
    at: At,
}

/// The `znx` world this checkout exports.
fn world() -> PathBuf {
    crate::ap::data_dir().join("..")
}

fn home() -> Result<PathBuf> {
    let home = room::home()?;
    std::fs::create_dir_all(&home)?;
    Ok(home)
}

pub(super) fn room_command(game: &Path, cmd: RoomCommand) -> Result<()> {
    let home = home()?;
    let up = || -> Result<Room> {
        Room::load(&home)?.ok_or_else(|| anyhow::anyhow!("no room is up: `ap room up` first"))
    };
    match cmd {
        RoomCommand::Up {
            preset,
            seed,
            port,
            at,
            mut install,
        } => {
            if install.name.is_empty() {
                install.name = format!("AP room: {preset}");
                install.note = "Link's slot of the room `zootr ap room up` hosts".into();
            }
            let ap = Archipelago::find(at.archipelago, &home)?;
            let room = Room::up(&ap, &home, &world(), &preset, seed, port)?;
            println!("room up at {} ({preset}, seed {seed})", room.address());
            for (slot, file) in SLOTS.iter().zip(&room.generated.slots) {
                println!("{slot}: {}", file.display());
            }
            super::seed::ap_open(game, &room.generated.slots[0], install)
        }
        RoomCommand::Send { item, to } => {
            up()?.send(&to, &item)?;
            println!("sent {item} to {to}");
            Ok(())
        }
        RoomCommand::Status => {
            match Room::load(&home)? {
                None => println!("no room is up"),
                Some(room) => {
                    let state = if room.alive() { "up" } else { "stopped" };
                    println!(
                        "{state} at {} ({}, seed {})",
                        room.address(),
                        room.preset,
                        room.seed
                    );
                    println!("{}", room.log_tail(8));
                }
            }
            Ok(())
        }
        RoomCommand::Down => {
            up()?.down(&home)?;
            println!("room down");
            Ok(())
        }
    }
}

/// The live metrics on a room of `preset`: brings the room up (installing
/// Link's slot), boots a new file with infinite health, measures, then
/// quits the game and takes the room down, whatever happened.
fn live(
    romfs: &ounce_formats::Romfs,
    home: &Path,
    preset: &str,
    seed: u64,
    ap: Option<&Path>,
) -> Result<Vec<bench::Sample>> {
    crate::turn::hold(&crate::turn::session())?;
    if Room::load(home)?.is_some() {
        own(&["ap", "room", "down"])?;
    }
    let seed = seed.to_string();
    let mut up = vec!["ap", "room", "up", "--preset", preset, "--seed", &seed];
    up.extend(["--name", "AP bench", "--note", "zootr ap bench's live run"]);
    let ap = ap.map(|p| p.to_string_lossy().into_owned());
    if let Some(ap) = &ap {
        up.extend(["--archipelago", ap]);
    }
    own(&up)?;
    let session = crate::turn::session();
    let measured = (|| {
        own(&["boot", "--session", &session, "--new"])?;
        own(&["dev", "switches", "health"])?;
        let room = Room::load(home)?.context("the room didn't come up")?;
        let attach = || super::running::attach(true);
        crate::ap::bench_live::live(romfs, &room, attach()?, attach()?, input()?.as_mut())
    })();
    let quit = input().and_then(|mut i| i.quit());
    own(&["ap", "room", "down"])?;
    let samples = measured?;
    quit?;
    Ok(samples)
}

fn input() -> Result<Box<dyn crate::play::Input>> {
    let config = crate::config::Config::load()?;
    crate::play::input(&config.target()?, config.game.as_deref())
}

/// Runs this same binary with `args`, its output shown only on failure.
fn own(args: &[&str]) -> Result<()> {
    let out = std::process::Command::new(std::env::current_exe()?)
        .args(args)
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "zootr {} failed:\n{}{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

pub(super) fn bench_command(game: &Path, args: Bench) -> Result<()> {
    let romfs = game::prepare(game, &Run::silent())?;
    let home = home()?;
    let ap = Archipelago::find(args.at.archipelago.clone(), &home)?;
    let world = world();
    ap.prepare(&world)?;
    let presets = match args.preset.is_empty() {
        true => vec!["defaults".to_owned(), "all-on".to_owned()],
        false => args.preset,
    };
    let run = RunInfo::here(&world, "none")?;
    let file = match args.out {
        Some(f) => Some(f),
        None => bench::default_file(&world)?,
    };
    let history = (file.as_ref())
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| bench::rows(&t))
        .unwrap_or_default();
    let work = home.join("bench");
    let mut new = Vec::new();
    for preset in &presets {
        let (locations, samples) = bench::offline(&romfs, &ap, &work, preset, args.seed)?;
        new.extend(samples.iter().map(|s| Row::new(&run, preset, locations, s)));
    }
    // Every option's rows, whatever the presets: its pool is the widest.
    let widest = new.iter().map(|r| r.locations).max().unwrap_or(0);
    new.push(Row::new(
        &run,
        "every",
        widest,
        &bench::export(&romfs, &work)?,
    ));
    if !args.offline {
        let local = RunInfo {
            room: "local".into(),
            ..run
        };
        for preset in &presets {
            let locations = (new.iter().find(|r| r.preset == *preset)).map_or(0, |r| r.locations);
            let samples = live(
                &romfs,
                &home,
                preset,
                args.seed,
                args.at.archipelago.as_deref(),
            )?;
            new.extend(
                samples
                    .iter()
                    .map(|s| Row::new(&local, preset, locations, s)),
            );
        }
    }
    print!("{}", bench::table(&history, &new));
    match file {
        _ if args.no_record => {}
        _ if cfg!(debug_assertions) => {
            println!("not recorded: a debug build's timings aren't comparable")
        }
        None => println!("not recorded: the main checkout has no docs/"),
        Some(f) => {
            bench::append(&f, &new)?;
            println!("recorded in {}", f.display());
        }
    }
    Ok(())
}
