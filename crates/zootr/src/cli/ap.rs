//! `zootr ap room` and `zootr ap bench`: a local room for tests, and the
//! timings kept run after run.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::args::Install;
use crate::ap::bench::{self, Row, RunInfo};
use crate::ap::room::{self, Archipelago, Room, SLOTS};
use crate::ap::there::There;
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
        /// Host it on another machine of the queue (`machines.NAME` in the
        /// config), still opening Link's slot here: a LAN room.
        #[arg(long)]
        on: Option<String>,
        #[command(flatten)]
        at: At,
        #[command(flatten)]
        install: Install,
    },
    /// Clone Archipelago on another machine of the queue and make its
    /// venv, at the commit this machine's checkout is on. Once per machine,
    /// and again after updating Archipelago here.
    Setup {
        #[arg(long)]
        on: String,
        #[command(flatten)]
        at: At,
    },
    /// Send an item from the room's cheat console.
    Send {
        /// By its AP name, e.g. "Forest Medallion".
        item: String,
        #[arg(long, default_value = "Link")]
        to: String,
    },
    /// Whether the room is up, where, and its server log's tail.
    Status {
        /// The room's state as JSON instead.
        #[arg(long)]
        json: bool,
    },
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
    /// Run the whole bench on another machine of the queue
    /// (`machines.NAME` in the config), its game and a room there. Its rows
    /// come back here.
    #[arg(long)]
    on: Option<String>,
    /// Host the live rooms on another machine of the queue, the game
    /// staying here: the LAN rows.
    #[arg(long, conflicts_with = "on")]
    room_on: Option<String>,
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
            on,
            at,
            mut install,
        } => {
            if install.name.is_empty() {
                install.name = format!("AP room: {preset}");
                install.note = "Link's slot of the room `zootr ap room up` hosts".into();
            }
            let room = match on {
                Some(on) => Room::up_there(There::find(&on)?, &home, &preset, seed, port)?,
                None => {
                    let ap = Archipelago::find(at.archipelago, &home)?;
                    Room::up(&ap, &home, &world(), &preset, seed, port)?
                }
            };
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
        RoomCommand::Setup { on, at } => {
            let ap = Archipelago::find(at.archipelago, &home)?;
            let rev = crate::ap::bench::git(&ap.checkout, &["rev-parse", "HEAD"])?;
            There::find(&on)?.setup(&rev)?;
            println!("Archipelago {} ready on {on}", &rev[..9]);
            Ok(())
        }
        RoomCommand::Status { json: true } => {
            let room = up()?;
            println!("{}", serde_json::to_string(&room)?);
            Ok(())
        }
        RoomCommand::Status { json: false } => {
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

pub(super) fn bench_command(game: &Path, args: Bench) -> Result<()> {
    let world = world();
    let file = match &args.out {
        Some(f) => Some(f.clone()),
        None => bench::default_file(&world)?,
    };
    let history = (file.as_ref())
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| bench::rows(&t))
        .unwrap_or_default();
    let presets = match args.preset.is_empty() {
        true => vec!["defaults".to_owned(), "all-on".to_owned()],
        false => args.preset.clone(),
    };
    let (new, comparable) = match &args.on {
        Some(on) => (bench_there(&There::find(on)?, &presets, &args)?, true),
        None => (
            bench_here(game, &world, &presets, &args)?,
            !cfg!(debug_assertions),
        ),
    };
    print!("{}", bench::table(&history, &new));
    match file {
        _ if args.no_record => {}
        _ if !comparable => println!("not recorded: a debug build's timings aren't comparable"),
        None => println!("not recorded: the main checkout has no docs/"),
        Some(f) => {
            bench::append(&f, &new)?;
            println!("recorded in {}", f.display());
        }
    }
    Ok(())
}

/// The bench on this machine: the offline rows, then the live ones on a
/// room here or (`--room-on`) on another machine.
fn bench_here(game: &Path, world: &Path, presets: &[String], args: &Bench) -> Result<Vec<Row>> {
    let romfs = game::prepare(game, None, &Run::silent())?;
    let home = home()?;
    let ap = Archipelago::find(args.at.archipelago.clone(), &home)?;
    ap.prepare(world)?;
    let run = RunInfo::here(world, "none")?;
    let work = home.join("bench");
    let mut new = Vec::new();
    for preset in presets {
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
    if args.offline {
        return Ok(new);
    }
    let room = match args.room_on {
        Some(_) => "lan",
        None => "local",
    };
    let live_run = RunInfo {
        room: room.into(),
        ..run
    };
    for preset in presets {
        let locations = (new.iter().find(|r| r.preset == *preset)).map_or(0, |r| r.locations);
        let samples = live(&romfs, &home, preset, args)?;
        new.extend(
            samples
                .iter()
                .map(|s| Row::new(&live_run, preset, locations, s)),
        );
    }
    Ok(new)
}

/// The whole bench run on `there` (its release build, game and rooms),
/// its rows read back.
fn bench_there(there: &There, presets: &[String], args: &Bench) -> Result<Vec<Row>> {
    // A file of our own there: a fixed name in a shared /tmp could be
    // someone else's.
    let made = there.sh("mktemp -t zootr-ap-bench.XXXXXXXX").output()?;
    anyhow::ensure!(made.status.success(), "mktemp failed on {}", there.name);
    let out = String::from_utf8(made.stdout)?.trim().to_owned();
    let seed = args.seed.to_string();
    let mut run = vec!["ap", "bench", "--seed", &seed, "--out", &out];
    for p in presets {
        run.extend(["--preset", p]);
    }
    if args.offline {
        run.push("--offline");
    }
    let status = there.zootr(&run).status()?;
    let text = there.read(&out);
    there.sh(&format!("rm -f -- '{out}'")).status().ok();
    anyhow::ensure!(status.success(), "the bench failed on {}", there.name);
    Ok(bench::rows(&String::from_utf8_lossy(&text?)))
}

/// The live metrics on a room of `preset`: brings the room up (installing
/// Link's slot), boots a new file with infinite health, measures, then
/// quits the game and takes the room down, whatever happened.
fn live(
    romfs: &ounce_formats::Romfs,
    home: &Path,
    preset: &str,
    args: &Bench,
) -> Result<Vec<bench::Sample>> {
    crate::turn::hold(&crate::turn::session())?;
    if Room::load(home)?.is_some() {
        own(&["ap", "room", "down"])?;
    }
    let seed = args.seed.to_string();
    let mut up = vec!["ap", "room", "up", "--preset", preset, "--seed", &seed];
    up.extend(["--name", "AP bench", "--note", "zootr ap bench's live run"]);
    let ap = args
        .at
        .archipelago
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    if let Some(ap) = &ap {
        up.extend(["--archipelago", ap]);
    }
    if let Some(on) = &args.room_on {
        up.extend(["--on", on]);
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
    let down = own(&["ap", "room", "down"]);
    let samples = measured?;
    quit?;
    down?;
    Ok(samples)
}

fn input() -> Result<Box<dyn crate::play::Input>> {
    Ok(super::running::target_input()?.1)
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
