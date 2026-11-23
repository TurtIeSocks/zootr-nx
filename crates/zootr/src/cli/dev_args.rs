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
        #[arg(allow_hyphen_values = true)]
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
    /// Play control lines back to back (`LUp~1.2`, `LUp+A~0.2`, `wait~0.5`,
    /// `shot`) while reading where Link stands every frame, then say how
    /// far he went, how high and low, and how far he got before he fell
    /// below his start height. With `--out`, saves the frames and
    /// `trace.json` there.
    Trace {
        #[arg(required = true, value_parser = crate::nav::trace::Step::parse)]
        steps: Vec<crate::nav::trace::Step>,
        /// Seconds to keep reading after the last step.
        #[arg(long, default_value_t = crate::nav::trace::SETTLE)]
        settle: f32,
        /// The folder for the frames and `trace.json`.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// A running jump across a gap, from its near edge `from` to its far
    /// edge `to` (each `x,y,z`, as `logic derive` prints them): warp `run`
    /// m behind `from`, run at `to`, steering by where Link goes (no ZL,
    /// so nothing near gets locked onto), and X as he's due `past` m
    /// beyond `from`. Says whether he made it, where X went in, and the
    /// trace.
    Leap {
        #[arg(allow_hyphen_values = true)]
        from: String,
        #[arg(allow_hyphen_values = true)]
        to: String,
        /// How far behind the near edge the run starts, in m: Link reaches
        /// his full 4.5 m/s after about 3.
        #[arg(long, default_value_t = 3.0)]
        run: f32,
        /// How far past the near edge X goes in, in m: where the real lip
        /// sits past the navmesh's edge. Negative jumps early.
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        past: f32,
        /// Seconds to keep reading after the jump.
        #[arg(long, default_value_t = crate::nav::trace::SETTLE)]
        settle: f32,
        /// The folder for `trace.json`.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// How far from a placed holder of the current map its button prompt
    /// shows: warp Link to spots along lines out from its front, farthest
    /// first, face it, and ask the vision model whether `word` is up. Says
    /// each line's edge and every spot tried, by where Link stood.
    Reach {
        hash: u64,
        /// The prompt's word, like `Open` or `Lift`.
        #[arg(long, default_value = "Open")]
        word: String,
        /// Degrees off the holder's front, clockwise from above.
        #[arg(
            long,
            value_delimiter = ',',
            allow_hyphen_values = true,
            default_value = "0"
        )]
        angles: Vec<f32>,
        /// The farthest and nearest spots, and the step between, in m.
        #[arg(long, default_value_t = 2.4)]
        far: f32,
        #[arg(long, default_value_t = 0.6)]
        near: f32,
        #[arg(long, default_value_t = 0.1)]
        step: f32,
        /// Keep each frame here.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        #[command(flatten)]
        vision: super::args::VisionArgs,
        /// One JSON object instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Put an item on ZR, or gear (boots, a tunic) on or off, through the
    /// menu: the item is given once more, so the menu opens on it, then A.
    /// Names as `give` takes them.
    Equip { item: String },
    /// Turn Link to face a placed actor of the current map, by its
    /// placement hash, or a spot given as `x,y,z`, then ZL, for aiming: a
    /// slingshot shot, a bomb throw or a run then goes its way. Says
    /// whether he faces it and how far off it is.
    Face {
        #[arg(allow_hyphen_values = true)]
        target: String,
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
    /// Beat an enemy for real: its Life goes to 0, so its own death, drop
    /// and defeat event run. A dormant one gets its Life back after 10 s.
    Slay {
        /// A placement hash (its live actor nearest its spot) or an actor
        /// name (every one in reach).
        #[arg(required_unless_present = "list")]
        target: Option<String>,
        /// How far from Link to look, in metres.
        #[arg(long, default_value_t = 30.0)]
        within: f32,
        /// List the enemies in reach with their Life instead.
        #[arg(long)]
        list: bool,
        /// One JSON array instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// The live actors placements made, nearest Link first, with their
    /// placement hashes and whether each is hidden.
    Nearby {
        /// Only names holding this text.
        #[arg(long)]
        grep: Option<String>,
        /// How many to list.
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// One JSON array instead of lines.
        #[arg(long)]
        json: bool,
    },
    /// Hide the model of each placement's live actor, with no reload.
    /// Collision stays, and a map load shows it again.
    Hide {
        /// Placement hashes (`dev nearby` lists them).
        #[arg(required = true)]
        hashes: Vec<u64>,
        /// Show them again instead.
        #[arg(long)]
        show: bool,
    },
    /// Spring an ice trap: Link freezes once he's free to, as from a
    /// trap's get.
    Freeze,
    /// Show a received item's notice, as the Archipelago client does:
    /// "ITEM from SENDER" on the banner, with the popup for a pouch item.
    /// With `--count`, the summary "+COUNT more from SENDER" instead. The
    /// sender is a slot's stand-in, which an AP seed names.
    Notice {
        /// The item actor, for an item's notice.
        #[arg(required_unless_present = "count")]
        item: Option<String>,
        /// The sending slot, 0 for the server.
        #[arg(long, default_value_t = 0)]
        from: u32,
        /// A summary of this many.
        #[arg(long, conflicts_with = "item")]
        count: Option<usize>,
        /// With `--count`, from several senders: "+COUNT more".
        #[arg(long, requires = "count")]
        mixed: bool,
    },
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
    /// Whether a check is done in the running game, read from the flag the
    /// Checks tab reads: `true`, `false`, `no flag`, or `no flag in this seed`
    /// for a Bool only a seed that shuffles it adds.
    Checked { hash: u64 },
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
    /// Live actors whose name starts with any `prefix` (all with none):
    /// address, name, position, and for an enemy the address of the Life
    /// it has left, which a write of 00000000 beats it by.
    Actors { prefix: Vec<String> },
    /// The per-placement flags (Bool64bitKey) that are set, one placement
    /// hash a line.
    Keys,
    /// One placement's own flag (Bool64bitKey): where it sits and its value,
    /// or with `--set` a write of it, like a pot broken or whole.
    Key {
        hash: u64,
        #[arg(long)]
        set: Option<bool>,
    },
    /// Bytes at an address, as hex, sixteen to a line.
    Read {
        /// A 0x address, or `main+0x…` for an offset into `main`.
        at: String,
        #[arg(default_value_t = 64)]
        len: usize,
    },
    /// Ranges read together over and over, each pass that differs from the
    /// last printed with its time: catches states one frame long, which a
    /// single `read` misses.
    Watch {
        /// `ADDR:LEN` ranges, each a 0x address or `main+0x…`.
        #[arg(required = true)]
        ranges: Vec<String>,
        /// How long to watch.
        #[arg(long, default_value_t = 3.0)]
        secs: f64,
    },
    /// A cheat table search for the 4-byte values that follow a state of
    /// the game (a prompt shown or not): snap writable memory in one
    /// state, then the other, then again in either until few are left.
    Snap {
        /// The search's name, so several can run side by side.
        name: String,
        /// The state the game is in now, like `on` or `off`.
        state: String,
        /// Start the search over.
        #[arg(long)]
        new: bool,
        /// List the values once this many or fewer are left.
        #[arg(long, default_value_t = 40)]
        show: usize,
    },
    /// Write hex bytes at an address.
    Write {
        /// A 0x address, or `main+0x…` for an offset into `main`.
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
