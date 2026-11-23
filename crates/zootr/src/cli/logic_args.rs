//! The arguments of `zootr logic`: rule files, room files and derive.

use std::path::PathBuf;

use clap::Subcommand;

/// `zootr logic`: what it does with the rule and room files.
#[derive(Subcommand)]
pub(super) enum LogicCommand {
    /// Set one check's name, need or place in the rule files, keeping
    /// comments and order. A check with no entry gets one in its area's
    /// file. Refused, with nothing written, unless every file still loads.
    Set {
        hash: u64,
        /// Shown in place of the generated name.
        #[arg(long)]
        name: Option<String>,
        /// What it needs beyond reaching its place, like `child and hookshot`.
        #[arg(long)]
        needs: Option<String>,
        /// The logic place Link gets it from, with `--era`.
        #[arg(long, requires = "era")]
        place: Option<String>,
        #[arg(long, requires = "place", value_parser = ["child", "adult"])]
        era: Option<String>,
        /// The rule files to edit: a worktree's `crates/zootr/logic`.
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/logic"))]
        dir: PathBuf,
    },
    /// Set a check's confirmed rule in a room file, made when the room has
    /// none. Refused, with nothing written, unless every file still loads.
    RoomSet {
        /// The dungeon, by its id (`deku-tree`).
        dungeon: String,
        room: u32,
        hash: u64,
        #[arg(long)]
        needs: String,
        #[arg(long)]
        why: String,
        /// Added to the entry's evidence. Without any, it's a guess.
        #[arg(long)]
        evidence: Option<String>,
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/logic"))]
        dir: PathBuf,
    },
    /// Add a confirmed entry that names a spot to a room file. Derive
    /// checks the spots: rerun it after.
    RoomAdd {
        dungeon: String,
        room: u32,
        #[command(subcommand)]
        entry: RoomEntry,
        #[arg(long, global = true)]
        evidence: Option<String>,
        #[arg(long, global = true, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/logic"))]
        dir: PathBuf,
    },
    /// A dungeon's rooms, by number, with their names.
    Rooms {
        dungeon: String,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/logic"))]
        dir: PathBuf,
    },
    /// The room files' playtest queue: hand rules that disagree with a room
    /// rule, guesses with their reasons, and routes that take knowledge.
    Audit {
        /// One dungeon, by its id (`deku-tree`).
        dungeon: Option<String>,
    },
    /// Every check and hand exit whose giver waits on a quest step that its
    /// rule names no word for, and has no `quest-gate-ok` reason.
    QuestGates,
    /// Write `logic/baseline/` from the game's data: never edited by hand,
    /// `logic/confirmed/` overrides it.
    Derive {
        /// The object kinds: what each needs and does, `baseline/kinds.toml`.
        #[arg(long)]
        kinds: bool,
        /// A dungeon's room files, by its id (`deku-tree`).
        dungeon: Option<String>,
        /// The logic folder to write in: a worktree's `crates/zootr/logic`.
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/logic"))]
        dir: PathBuf,
    },
    /// What derive sees in a dungeon: each navmesh face near a spot with
    /// its room, part and arcs out, the placements that number rooms there,
    /// and a top-down picture of the floor by room.
    Explain {
        /// The dungeon, by its id (`deku-tree`).
        dungeon: String,
        /// The spot to explain, as `X,Y,Z`.
        #[arg(long, value_delimiter = ',', allow_negative_numbers = true)]
        at: Option<Vec<f32>>,
        /// How far round the spot to look, in m.
        #[arg(long, default_value_t = 3.0)]
        within: f32,
        /// Write a top-down PNG: floors by room (swum water darker, room
        /// numbers drawn), walls cut at waist height (black solid, blue
        /// invisible), swims green, crawls orange, the spot a red cross.
        #[arg(long)]
        picture: Option<PathBuf>,
        /// The rooms the picture shows, like `5,6`. Without: round the spot.
        #[arg(long, value_delimiter = ',')]
        rooms: Vec<u32>,
        /// The height walls are cut at. Without: the spot's, or the floor's.
        #[arg(long, allow_negative_numbers = true)]
        y: Option<f32>,
    },
}

/// A spot entry `logic room-add` writes. Spots are `x,y,z`.
#[derive(Subcommand)]
pub(super) enum RoomEntry {
    /// A directed link from a spot in the room to another spot.
    Link {
        #[arg(long, allow_hyphen_values = true)]
        from: String,
        #[arg(long, allow_hyphen_values = true)]
        to: String,
        /// walk, door, drop, step, climb, crawl, jump, hover, hookshot,
        /// longshot, swim, dive, sink, fall or event.
        #[arg(long)]
        via: String,
        #[arg(long)]
        needs: String,
        #[arg(long)]
        why: String,
    },
    /// Where a flag is set, for one derive can't trace.
    Flag {
        #[arg(long, value_parser = flag_hash)]
        hash: u32,
        /// Defaults to the flag's name in the room's files.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        at: String,
        #[arg(long)]
        needs: String,
        #[arg(long)]
        why: String,
    },
    /// Floor no room had, claimed for this one.
    Floor {
        #[arg(long, allow_hyphen_values = true)]
        at: String,
        #[arg(long)]
        why: String,
    },
}

/// `0x1a2b3c4d`, `#1a2b3c4d` or decimal.
fn flag_hash(s: &str) -> Result<u32, std::num::ParseIntError> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix('#')) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => s.parse(),
    }
}
