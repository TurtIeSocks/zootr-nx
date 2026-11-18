//! The arguments of the commands that drive the running game: `dev` and
//! `mem`.

use clap::Subcommand;

use crate::warp::Side;
use anyhow::Result;

#[derive(Subcommand)]
pub(super) enum DevCommand {
    /// The map, the age, and whether a teleport would be refused.
    State,
    /// Send Link to `MAP/SPOT`, switching age first when it's the other
    /// age's map.
    Teleport {
        place: String,
        /// Then warp beside this placed actor of the map, e.g.
        /// the one `probe actors` lists.
        #[arg(long, value_name = "ACTOR")]
        near: Option<String>,
        /// Which side of the actor to land on.
        #[arg(long, value_enum, default_value_t)]
        side: Side,
    },
    /// Warp Link beside a placed actor of the current map, by its actor
    /// name or its placement hash from `probe actors`, or onto a spot
    /// given as `x,y,z`.
    Warp {
        actor: String,
        /// Which side of the actor to land on.
        #[arg(long, value_enum, default_value_t)]
        side: Side,
    },
    /// What's placed around Link in the current map, nearest first, each
    /// with a clock bearing from his facing (12 ahead, 3 to his right).
    /// Placements don't move: a rolling Goron isn't where this says.
    Around {
        /// How far to look, measured flat.
        #[arg(long, default_value_t = 15.0)]
        radius: f32,
        /// Also terrain, areas, anchors and locators.
        #[arg(long)]
        all: bool,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// What lies around Link by clock hour from his facing: walls, drops,
    /// steps and the things `around` lists, nearest first, each named.
    Scene {
        /// How far to look, measured flat.
        #[arg(long, default_value_t = 8.0)]
        radius: f32,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Walk Link to a placed actor of the current map, by its placement
    /// hash from `probe actors`. Gives up with a reason (`no route`,
    /// `blocked`, `left map`, `event` or `budget`), never silently.
    Go {
        hash: u64,
        /// Print the route (waypoints and the links, door, drop or step,
        /// between them) without moving Link.
        #[arg(long, conflicts_with_all = ["use_it", "warp", "budget"])]
        plan: bool,
        /// Then `use` it, once there.
        #[arg(long = "use")]
        use_it: bool,
        /// With no route, warp beside it instead and say `warped`.
        #[arg(long)]
        warp: bool,
        /// Seconds the walk may take.
        #[arg(long, default_value_t = 60)]
        budget: u64,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Push the stick toward a clock hour from Link's facing for SECS
    /// seconds, ZL first, then say what came of it: free, slowed, blocked
    /// (and what is there), dropped, climbed or cornered.
    Move {
        /// From his facing, as `scene` reads them: 12 ahead, 3 to his
        /// right. The stick has eight directions, so some hours share one.
        #[arg(value_parser = clap::value_parser!(u8).range(1..=12))]
        hour: u8,
        /// How long to push. Link covers about 5 m a second.
        #[arg(value_parser = positive)]
        secs: f32,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Walk to where a placed actor of the current map is used from, face
    /// it, then press A, or for a door walk through. Says `event`,
    /// `entered` or `nothing`, and where a frame of the game was saved.
    Use {
        hash: u64,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Hold the named dev switches on and the rest off: health, magic, or
    /// resources (every ammo kind and rupees), each kept at its cap on
    /// Link's update. `--off` turns all off. With neither, says which are on.
    Switches {
        #[arg(value_enum)]
        on: Vec<Switch>,
        #[arg(long, conflicts_with = "on")]
        off: bool,
    },
    /// Drop Link's hearts to zero, as a last hit would: a bottled fairy
    /// still saves him.
    Kill,
    /// Switch age: the same map for the other age, else the Temple of Time.
    Age,
    /// Set the time of day.
    Time { hour: u32 },
    /// Move a quest to one of its steps, as the game's own flows do, e.g.
    /// `SageOfSpirit Step02`.
    Step { challenge: String, step: String },
    /// Play an event with Link as its starter, by
    /// its full name.
    Play { event: String },
    /// Apply a saved test setup: its values, items, quest steps and hour,
    /// then the teleport.
    Apply { name: String },
    /// Read a saved test setup's goals: ✓ or ✗ and the value each holds.
    Goals { name: String },
}

/// A dev switch `dev switches` names.
#[derive(Clone, Copy, Debug, PartialEq, clap::ValueEnum)]
pub enum Switch {
    Health,
    Magic,
    Resources,
}

#[derive(Subcommand)]
pub enum MemCommand {
    /// The mapped regions: start, end, size and protection, the larger
    /// than `--min` MB only.
    Regions {
        #[arg(long, default_value_t = 0)]
        min: usize,
    },
    /// Where these bytes are, in readable memory.
    Find {
        /// Hex bytes (`0f 1e`, `0f1e`), or text in quotes after `s:`
        /// (`s:Rupee`).
        needle: String,
        /// Only writable memory, where the game's loaded data lives.
        #[arg(long)]
        writable: bool,
        /// Stop after this many hits.
        #[arg(long, default_value_t = 64)]
        limit: usize,
        /// Only from this 0x address.
        #[arg(long)]
        from: Option<String>,
        /// Only below this 0x address.
        #[arg(long)]
        to: Option<String>,
    },
    /// The per-placement flags (Bool64bitKey) that are set, one placement
    /// hash a line.
    Keys,
    /// Bytes at an address, as hex, sixteen to a line.
    Read {
        /// A 0x address.
        at: String,
        #[arg(default_value_t = 64)]
        len: usize,
    },
    /// Write hex bytes at an address.
    Write {
        /// A 0x address.
        at: String,
        /// Hex bytes.
        bytes: String,
    },
}

/// A number of seconds above zero.
fn positive(arg: &str) -> Result<f32> {
    let secs: f32 = arg.parse()?;
    anyhow::ensure!(secs > 0.0, "{arg} isn't above zero");
    Ok(secs)
}
