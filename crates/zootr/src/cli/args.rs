//! The command tree: the arguments each command takes and what they mean,
//! with the value parsers and the settings they resolve to.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use super::dev_args::{DevCommand, MemCommand};
use crate::config::Config;
use crate::progress::{About, Run};
use crate::settings::Settings;
use crate::sweep::drills::Skill;
use crate::vision::{Api, Effort};
use crate::{checks, generate, presets, savefile, settings, starting};

#[derive(Parser)]
#[command(
    version,
    about = "Randomizer generator for Ocarina of Time on Switch 2"
)]
pub(super) struct Cli {
    /// Your game dump: a `.nup` holding the 1.1.1 update, or a folder of
    /// its extracted romfs. Defaults to the app's, then `dump/oot.nup`.
    #[arg(long, global = true, alias = "romfs")]
    pub(super) game: Option<PathBuf>,
    #[command(subcommand)]
    pub(super) command: Command,
}

/// The vision model that judges a spot, and the save each start loads.
/// The vision model a command asks about frames.
#[derive(Args)]
pub(super) struct VisionArgs {
    /// The vision model. Defaults to `claude-opus-5-5` on the Anthropic
    /// API.
    #[arg(long, env = "ZOOTR_VISION_MODEL")]
    pub model: Option<String>,
    /// Which shape the vision model's API speaks.
    #[arg(long, env = "ZOOTR_VISION_API", value_enum, default_value_t = Api::Anthropic)]
    pub api: Api,
    /// The vision model's API, without `/v1/...`. Defaults to
    /// `https://api.anthropic.com` on the Anthropic API and is required on
    /// OpenAI's.
    #[arg(long, env = "ZOOTR_VISION_URL")]
    pub url: Option<String>,
    /// A file holding the API's key, kept out of the command line.
    #[arg(long, env = "ZOOTR_VISION_KEY_FILE")]
    pub key_file: Option<PathBuf>,
    /// How hard an Anthropic model thinks.
    #[arg(long, value_enum)]
    pub effort: Option<Effort>,
}

impl VisionArgs {
    pub(super) fn vision(self) -> anyhow::Result<crate::vision::Vision> {
        use anyhow::Context;
        let key_file =
            (self.key_file).context("--key-file or ZOOTR_VISION_KEY_FILE is required")?;
        crate::vision::Vision::new(self.api, self.url, self.model, key_file, self.effort)
    }
}

#[derive(Args)]
pub(super) struct JudgeArgs {
    /// Which shape the vision model's API speaks.
    #[arg(long, value_enum, default_value_t = Api::Anthropic)]
    pub(super) judge_api: Api,
    /// The vision model's API, without `/v1/...`. Defaults to
    /// `https://api.anthropic.com` on the Anthropic API and is
    /// required on OpenAI's.
    #[arg(long)]
    pub(super) judge_url: Option<String>,
    /// A file holding the API's key, kept out of the command line.
    #[arg(long, required = true)]
    pub(super) judge_key_file: Option<PathBuf>,
    /// Defaults to `claude-opus-5-5` on the Anthropic API.
    #[arg(long)]
    pub(super) judge_model: Option<String>,
    /// How hard an Anthropic judge thinks.
    #[arg(long, value_enum)]
    pub(super) judge_effort: Option<Effort>,
    /// The judge's playbook, a Markdown file sent with every look.
    #[arg(long, required = true)]
    pub(super) playbook: Option<PathBuf>,
    /// Where the judge's lessons grow. Defaults to `lessons.md` beside
    /// the playbook.
    #[arg(long)]
    pub(super) lessons: Option<PathBuf>,
    /// A save folder copied over the target's save folder before each
    /// start.
    #[arg(long)]
    pub(super) pristine: Option<PathBuf>,
}

/// `sweep report`.
#[derive(Subcommand)]
pub(super) enum SweepReport {
    /// Tally a sweep's rows by holder kind and verdict, and its failures by
    /// how the walk and use ended.
    Report {
        /// A night's folder, or a folder of nights.
        #[arg(long)]
        out: PathBuf,
        /// From this round on.
        #[arg(long)]
        from: Option<u64>,
        /// Up to this round.
        #[arg(long)]
        to: Option<u64>,
        /// A ladder's folder, once for each machine. Its proven skills turn
        /// a "nothing" that leans on an unproven one into "unproven".
        #[arg(long)]
        drills: Vec<PathBuf>,
    },
}

/// `drill report`.
#[derive(Subcommand)]
pub(super) enum DrillReport {
    /// The ladder's scoreboard: passes per skill and per drill, and how each
    /// failed run ended, with its last frame.
    Report {
        /// The ladder's folder.
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
pub(super) enum Command {
    /// Whose turn it is on the game. Naming `claude`, `maintainer` or
    /// `free` hands it over. With a queue in the config's `turn`: `join`
    /// the line, `wait` for a game, `free` it, `reserve` one, `maintainer`
    /// to play this machine's, or `serve` the queue itself.
    #[command(hide = true)]
    Turn {
        to: Option<String>,
        /// Why, or what's being tested.
        #[arg(long, default_value = "")]
        note: String,
        /// The Claude session taking or handing over the turn, by a short
        /// name of its own. Needed to take a turn from Claude's shell.
        #[arg(long, default_value = "", env = crate::turn::SESSION)]
        session: String,
        /// Hand over the maintainer's or another session's turn from
        /// Claude's shell anyway.
        #[arg(long)]
        force: bool,
        /// `join` ahead of the waiters who aren't: for a merge blocker or a
        /// bug the maintainer reported.
        #[arg(long)]
        urgent: bool,
        /// `join` for every game at once: granted together once all are
        /// free, never one alone.
        #[arg(long)]
        both: bool,
        /// The game `reserve` keeps, or the one game `join` waits for.
        #[arg(long, default_value = "")]
        on: String,
    },
    /// Start the game on a kept save or a new file and press A through the
    /// menus, returning once Link is in play. On Claude's turn only, when
    /// run from Claude's shell. Muted, 30 fps and off the cloud saves when
    /// the target's harness env says so.
    #[command(hide = true)]
    Boot {
        #[command(flatten)]
        start: StartArgs,
        /// Seconds to give up after: the first start prepares shaders.
        #[arg(long, default_value_t = 600)]
        limit: u64,
    },
    /// Extract the game files zootr reads from the dump into the cache,
    /// and print where they are.
    Extract,
    /// List every placed actor that drops an item.
    Census,
    /// List every scene exit: from node, era, to node, arrival point.
    Exits,
    /// Every child `MAP/SPOT` that `shuffle --spawn` can start a new game at.
    Spawns,
    /// What each chest's GetEventName does to the save, beyond the drop.
    ChestEvents,
    /// What an event does to the save: items, equipment, flags, scene
    /// changes, following sub-flows and transitions.
    Effects {
        /// Event name, as the game spells it.
        event: String,
    },
    /// Every cutscene: area, tier, what it changes, and what starts it.
    Cutscenes,
    /// List every item Link can own (actors with a PouchContentParam),
    /// grouped by pouch category, with how many placements drop each.
    Items,
    /// Every flag a Presence gates on: how many actors it shows or hides,
    /// in which areas, and what the flag means.
    Flags,
    /// What one GameData flag is: its expression or the flows that set it.
    Flag {
        /// Flag name, as the game spells it.
        name: String,
    },
    /// What logic reaches with every check holding its vanilla item, and
    /// with every item, plus each place's provable checks and obstacles.
    /// Takes a new game's settings, like `shuffle`.
    #[command(args_conflicts_with_subcommands = true)]
    Logic {
        #[command(subcommand)]
        edit: Option<LogicCommand>,
        #[command(flatten)]
        new_game: NewGame,
    },
    /// Print an event flow, one line per entry point and event.
    Flow {
        /// Flowchart name, as the game spells it.
        name: String,
        /// A mod folder, like an installed seed's: print its rewrite of the
        /// flow when it has one.
        #[arg(long = "mod")]
        mod_dir: Option<PathBuf>,
    },
    /// Quick questions about the game's data, for research.
    #[command(subcommand)]
    Probe(crate::probe::Probe),
    /// Archipelago: the data the `znx` world loads.
    #[command(subcommand)]
    Ap(ApCommand),
    /// Round-trip every Banc file, GameData and the RESTBL through the
    /// format code and report anything that doesn't survive.
    Selftest,
    /// Make one placed actor drop a different item, written as a mod.
    SetDrop {
        /// Placement hash of the actor (from `census`).
        #[arg(long)]
        hash: u64,
        /// Actor to drop instead, by its full name.
        #[arg(long)]
        item: String,
        #[command(flatten)]
        new_game: NewGame,
        #[command(flatten)]
        out: ModDir,
    },
    /// Read, compare or edit a save slot by flag name.
    #[command(subcommand)]
    Save(SaveCommand),
    /// Print GameData values as the running game changes them: through the
    /// picked target's bridge, or its process on macOS.
    Live {
        /// Only names containing this.
        #[arg(long)]
        grep: Option<String>,
        /// Keep printing values that change all the time, like Link's
        /// position. Muted by default.
        #[arg(long)]
        all: bool,
        /// Only values with a name or a label, not a bare hash.
        #[arg(long)]
        named: bool,
    },
    /// Shuffle the items and write the seed as a mod.
    Shuffle(Shuffle),
    /// Give Link an item in the running game, by its actor name, with
    /// the game's own grant, run by our hook on Link's next update. Needs a
    /// seed installed with `dev_tools` and `dev_panel` on in the config.
    /// The menu shows it the next time it opens. Needs macOS or a target
    /// with a bridge.
    Give {
        actor: String,
        /// How many, for items with a count.
        #[arg(default_value_t = 1)]
        count: i32,
    },
    /// Write one GameData value into the running game. Needs macOS or a
    /// target with a bridge.
    Set {
        /// A GameData name or 0x hash.
        name: String,
        /// true/false, a number, or an Enum value's name. `?` only reads and
        /// lists the values it takes.
        value: String,
    },
    /// Read, search and write the running game's memory, for finding
    /// where the game keeps something. A game read directly only.
    #[command(subcommand)]
    Mem(MemCommand),
    /// Add a target from a shared preset, or print one to share.
    #[command(subcommand)]
    Target(TargetCommand),
    /// Dev panel requests for the running game, run by our hook on Link's
    /// next update. Needs a seed installed with `dev_tools` and `dev_panel`
    /// on in the config.
    #[command(subcommand)]
    Dev(DevCommand),
    /// Sweep every spot of new seeds, round after round,
    /// a vision model judging the spots that don't resolve by themselves.
    /// Results go to `<out>/results.jsonl`. Runs on the Mac too.
    #[command(
        hide = true,
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true
    )]
    Sweep {
        #[command(subcommand)]
        report: Option<SweepReport>,
        /// The night's folder.
        #[arg(long, required = true)]
        out: Option<PathBuf>,
        /// Rounds to run, 0 for no end.
        #[arg(long, default_value_t = 1)]
        rounds: usize,
        /// Spots a round at most, for a short run.
        #[arg(long)]
        limit: Option<usize>,
        /// Every round in the settings mix whose name starts with this,
        /// like `everything`, instead of each mix in turn.
        #[arg(long)]
        mix: Option<String>,
        /// Only the spots whose name, holder or seed item holds this, like
        /// `Cow`, for a run that tries one fix. Commas join parts that
        /// must all match: `Pot,Melody` is a pot holding a song.
        #[arg(long)]
        only: Option<String>,
        /// Every round on this seed, as a night's round named it
        /// (`round-011-412851`), to see a failure again.
        #[arg(long)]
        seed: Option<u64>,
        /// Step aside between spots whenever someone waits for this game
        /// in the turn queue, keeping the round's place in
        /// `<out>/resume.json`. Run again, it waits for the game, then
        /// finishes that round.
        #[arg(long = "yield", value_name = "GAME")]
        yield_to: Option<String>,
        /// A photo pass's `photos.jsonl`: each holder is used from the side
        /// it was framed from first, until a night learns a better one.
        #[arg(long, value_name = "FILE")]
        sides: Option<PathBuf>,
        #[command(flatten)]
        judge: JudgeArgs,
    },
    /// Run the harness ladder's drills: small play tasks with a known
    /// answer, through the sweep's own code, on a seed that shuffles
    /// nothing. Rows go to `<out>/drills.jsonl`.
    #[command(
        hide = true,
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true
    )]
    Drill {
        #[command(subcommand)]
        report: Option<DrillReport>,
        /// The ladder's folder.
        #[arg(long, required = true)]
        out: Option<PathBuf>,
        /// Only this skill's drills.
        #[arg(long, value_enum)]
        skill: Option<Skill>,
        /// Only the drills whose name holds this.
        #[arg(long)]
        name: Option<String>,
        /// Run each drill this many times, to catch a flaky one.
        #[arg(long, default_value_t = 1)]
        repeat: usize,
        #[command(flatten)]
        judge: JudgeArgs,
    },
    /// Ask a vision model questions about the game's picture, all at once,
    /// and print the answers as one JSON object. A question is yes or no,
    /// or asks for words when it ends in `?:`. The frame comes from the
    /// target's game, through its bridge when it has one.
    #[command(hide = true)]
    Look {
        #[arg(required = true)]
        questions: Vec<String>,
        #[command(flatten)]
        vision: VisionArgs,
        /// Keep the frame in this folder, so it can be opened when an
        /// answer looks wrong.
        #[arg(long, value_name = "DIR")]
        keep: Option<PathBuf>,
        /// Ask about this saved frame (a sweep's shot) instead of the
        /// game's.
        #[arg(long, value_name = "PNG", conflicts_with = "keep")]
        frame: Option<PathBuf>,
    },
    /// Photograph each check as it stands, untouched, for the ledger:
    /// Link warps beside its holder (a gift's or shop's giver) on each
    /// side in turn until the vision model sees it. Writes `HASH.png` and
    /// a line in `photos.jsonl` per check to `--out`, skipping checks
    /// already there. On the running game, on Claude's turn.
    #[command(hide = true)]
    Photo {
        /// Only this area's checks, by its Banc folder (`ForestTemple`).
        #[arg(long, conflicts_with = "hash")]
        area: Option<String>,
        /// Only these checks, by placement hash.
        #[arg(long, num_args = 1..)]
        hash: Vec<u64>,
        /// The run's folder.
        #[arg(long, required_unless_present_any = ["list", "go"])]
        out: Option<PathBuf>,
        /// Take Link to the one `--hash` as the pass would shoot it (age,
        /// hour, prep), standing beside its holder, with no frame.
        #[arg(long, conflicts_with_all = ["here", "list", "area"])]
        go: bool,
        /// With `--go`: stand on this side of the holder (where Hob's best
        /// frame was).
        #[arg(long, requires = "go")]
        side: Option<crate::warp::Side>,
        /// With `--side`: this many steps back.
        #[arg(long, requires = "side", default_value_t = 0)]
        step: u32,
        /// Take the game's current picture as the one `--hash`'s photo,
        /// shot by hand, into `--out`.
        #[arg(long, conflicts_with_all = ["list", "area"])]
        here: bool,
        /// Print each check's map, age and spot, a line each, without
        /// touching the game.
        #[arg(long)]
        list: bool,
        /// Swing the camera around Link before each shot, in ms of the
        /// right stick held left (negative: right), so he isn't in front
        /// of the holder.
        #[arg(long, default_value_t = 200, allow_hyphen_values = true)]
        orbit: i64,
        /// Raise the camera over Link, in ms of the right stick held down
        /// (negative: up).
        #[arg(long, default_value_t = 300, allow_hyphen_values = true)]
        raise: i64,
        /// The installed seed has the day-skulltulas setting: shoot every
        /// Gold Skulltula at noon.
        #[arg(long)]
        day_skulltulas: bool,
        #[command(flatten)]
        vision: VisionArgs,
    },
    /// Run a play script on the game, from `crates/zootr/play/`: its test
    /// start, its steps, a watch for stalls, and a report with screenshots
    /// beside the app's live logs. Exits nonzero when the run fails. Starts
    /// the game with the installed seed, never installing one.
    #[command(hide = true)]
    Play {
        script: PathBuf,
        /// Use the game already running. Without it a running game is
        /// refused, since it may be someone's session.
        #[arg(long, conflicts_with_all = ["save", "new"])]
        attach: bool,
        /// Seconds an event may play with nothing changing.
        #[arg(long, default_value_t = 15)]
        stall: u64,
        #[command(flatten)]
        start: StartArgs,
    },
}

/// What a harness start plays. Without either, the game's own saves.
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
}

#[derive(clap::Args)]
pub(super) struct StartArgs {
    /// Continue a kept save (`zootr save kept`). The game plays a copy, so
    /// the kept one never changes.
    #[arg(long, value_name = "NAME", conflicts_with = "new")]
    pub(super) save: Option<String>,
    /// A new file on the installed seed.
    #[arg(long)]
    pub(super) new: bool,
    /// This Claude session's name, as it took the turn. Needed from
    /// Claude's shell.
    #[arg(long, default_value = "", env = crate::turn::SESSION)]
    pub(super) session: String,
    /// Keep the Market Potion Shop's bottle tutorial, for a test of its
    /// gift. Otherwise it's marked done once Link is in play: begun and not
    /// ended, it shuts Link in the shop until he catches a bug.
    #[arg(long)]
    pub(super) bottle_tutorial: bool,
}

#[derive(Subcommand)]
pub(super) enum TargetCommand {
    /// Add the target in a preset file (`-` reads stdin) and print its name.
    /// A name already taken gets a number.
    Import { file: PathBuf },
    /// Print a target's preset JSON, with the home folder as `~`.
    Export { name: String },
    /// Run a target's rebuild command, for a game that bakes the hook
    /// patch in when it's built, then say whether its build has the
    /// installed patch.
    Rebuild { name: String },
}

#[derive(Subcommand)]
pub(super) enum ApCommand {
    /// Write the world graph, pool, options, ids and golden answers the
    /// `znx` world loads, keeping the ids the last export gave out.
    Export {
        /// Defaults to the repo's `ap/znx/data`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Install the mod for one slot of a multiworld, from the `.apznx`
    /// Archipelago generated for it.
    Open {
        file: PathBuf,
        #[command(flatten)]
        install: Install,
    },
    /// Connect the running game to an Archipelago server as the installed
    /// seed's slot: send its checks, give Link what other worlds find for
    /// him, and report the goal. Runs until stopped.
    Connect {
        /// `host[:port]` (port 38281 by default), or a `ws://`/`wss://` url.
        server: String,
        /// Defaults to the seed's own slot.
        #[arg(long)]
        slot: Option<String>,
        #[arg(long)]
        password: Option<String>,
    },
}

#[derive(clap::Args)]
pub(super) struct Shuffle {
    /// Same seed, same shuffle. Random when omitted.
    #[arg(long)]
    pub(super) seed: Option<u64>,
    /// Plando: put an item in a chest, by its hash from `census` or a
    /// spoiler. The rest are shuffled around it. Repeatable.
    #[arg(long, value_name = "HASH=ITEM", value_parser = placement)]
    place: Vec<(u64, String)>,
    /// Set one placement field by hand for a test, e.g.
    /// `HASH.Gyaml=ACTOR` or
    /// `HASH.Dynamic.Drop__DropActor=ITEM`, or `+=` to raise a
    /// number: `HASH.Translate.1+=0.2` lifts it 0.2. Repeatable.
    #[arg(long, value_name = "HASH.PATH=VALUE", value_parser = field)]
    field: Vec<String>,
    /// Shuffle nothing: only `--place` changes chests.
    #[arg(long)]
    vanilla: bool,
    /// Start a new game here instead of Link's house, as `MAP/SPOT` from
    /// `spawns`. For setting up a test.
    #[arg(long, value_name = "MAP/SPOT")]
    spawn: Option<String>,
    /// Hand a new game this pouch item, by actor from `items`. Chests keep
    /// theirs. Repeatable.
    #[arg(long, value_name = "ITEM")]
    give: Vec<String>,
    /// Move a quest on in a new game's opening, as `CHALLENGE=STEP` like
    /// `SageOfSpirit=Step02`. Run in order. Repeatable.
    #[arg(long, value_name = "CHALLENGE=STEP")]
    progress: Vec<String>,
    #[command(flatten)]
    new_game: NewGame,
    #[command(flatten)]
    pub(super) install: Install,
}

/// Where a seed's mod goes, and how the history shows it.
#[derive(clap::Args)]
pub(super) struct Install {
    #[command(flatten)]
    pub(super) out: ModDir,
    /// What the seed is for, shown in the app's history.
    #[arg(long, default_value = "")]
    pub(super) name: String,
    /// A longer note under the name in the app's history.
    #[arg(long, default_value = "")]
    pub(super) note: String,
    /// With `--mod-dir`, still add the seed to the app's history, without
    /// installing it. An installed seed is always added, and so is one held
    /// back because the game is open.
    #[arg(long)]
    pub(super) history: bool,
    /// Build the seed and add it to the app's history, writing no mod
    /// anywhere. Install it from the history later, even while the game
    /// is open now.
    #[arg(long, conflicts_with_all = ["mod_dir", "history"])]
    pub(super) no_install: bool,
}

#[derive(Subcommand)]
pub(super) enum SaveCommand {
    /// Every value that differs from its GameData default.
    Show {
        /// Slot number (under the picked target's save folder) or a path to a progress.sav.
        slot: String,
        /// Only names containing this.
        #[arg(long)]
        grep: Option<String>,
        /// Every entry, including defaults and types shown by size.
        #[arg(long)]
        all: bool,
    },
    /// Every value that differs between two saves.
    Diff {
        a: String,
        b: String,
        #[arg(long)]
        grep: Option<String>,
    },
    /// Print what each write to a save changes, as the game autosaves.
    /// Every slot in the save folder unless one is given.
    Watch {
        slot: Option<String>,
        #[arg(long)]
        grep: Option<String>,
    },
    /// Keep a save folder in the library as NAME, for `boot --save NAME`.
    /// Replaces a kept save of the same name. Quit the game first when
    /// keeping the folder it plays in.
    Keep {
        name: String,
        /// Only this slot, kept as slot 0 so Continue loads it.
        #[arg(long)]
        slot: Option<u8>,
        /// The save folder: the picked target's when omitted, or a boot's
        /// run folder to keep what that run reached.
        #[arg(long, value_name = "DIR")]
        from: Option<PathBuf>,
        /// What the save is for.
        #[arg(long, default_value = "")]
        note: String,
    },
    /// The kept saves: name, map, age, when and why each was kept.
    Kept,
    /// Set scalar values, e.g. `IsGet.ITEM=true`. Close the game
    /// first. The old file is kept next to it as a timestamped backup.
    Set {
        slot: String,
        /// NAME=VALUE pairs. Enums take a value name.
        #[arg(required_unless_present = "presets")]
        edits: Vec<String>,
        /// Flag sets to apply too. Repeatable.
        #[arg(long = "preset", value_parser = presets::parser())]
        presets: Vec<String>,
    },
}

/// How a new save starts: GameData defaults, and the cutscenes skipped.
#[derive(clap::Args)]
pub(super) struct NewGame {
    /// Flag sets for a new game. Repeatable.
    #[arg(long = "preset", value_parser = presets::parser())]
    presets: Vec<String>,
    /// A new game's starting value for one flag, e.g.
    /// `IsGet.ITEM=true`. Repeatable.
    #[arg(long = "default", value_name = "NAME=VALUE")]
    edits: Vec<String>,
    /// Play the opening movie and Navi's wake-up cutscene. Skipped by
    /// default, so a new game starts with Link in control.
    #[arg(long)]
    keep_intro: bool,
    /// Cutscenes to skip: `all`, or a name or area. Repeatable.
    #[arg(long = "skip", value_name = "NAME", value_parser = settings::skip_parser())]
    skips: Vec<String>,
    /// How big each chest is.
    #[arg(long, value_enum)]
    chest_size: Option<settings::ChestSize>,
    /// What a new game starts with, as `WORD` or `WORD=N`: `kokiri-sword`,
    /// `longshot` (which brings the Hookshot), `bombs=20`, `heart-pieces=8`,
    /// `heart-containers=5`, `rupees=99`. Repeatable.
    #[arg(long = "start", value_name = "WORD[=N]", value_parser = starting::parse)]
    start: Vec<(String, i32)>,
    /// Any app setting by id, for those without a flag of their own, e.g.
    /// `progressive-items=false`. Repeatable.
    #[arg(long = "setting", value_name = "ID=VALUE", value_parser = setting)]
    raw: Vec<(String, serde_json::Value)>,
}

/// `ID=VALUE`, where `true` and `false` are toggles and anything else a
/// choice. The id must be one of the app's settings and the value one it
/// takes: an unknown choice would quietly read as the default.
pub(crate) fn setting(arg: &str) -> Result<(String, serde_json::Value)> {
    let (id, value) = savefile::split(arg)?;
    let schema = settings::schema();
    let def = schema.iter().find(|def| def.id == id);
    let def = def.with_context(|| format!("no setting is called {id}"))?;
    match &def.kind {
        settings::Kind::Toggle { .. } => anyhow::ensure!(
            value.parse::<bool>().is_ok(),
            "{id} is true or false, not {value}"
        ),
        settings::Kind::Choice { options, .. } => {
            let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
            anyhow::ensure!(
                ids.contains(&value),
                "{id} takes {}, not {value}",
                ids.join(", ")
            );
        }
        settings::Kind::Inventory => anyhow::bail!("{id} is set with --start"),
    }
    let value = value.parse::<bool>().map_or(value.into(), Into::into);
    Ok((id.to_owned(), value))
}

impl NewGame {
    /// The presets and the skips as the app's settings. `--default` edits
    /// have no setting, so they stay apart.
    fn settings(&self) -> Settings {
        let mut out: Settings = self
            .presets
            .iter()
            .map(|p| (p.clone(), true.into()))
            .collect();
        if self.keep_intro {
            out.insert(settings::SKIP_INTRO.into(), false.into());
        }
        if let Some(size) = self.chest_size {
            out.insert(
                settings::CHEST_SIZE.into(),
                settings::choice_id(size).into(),
            );
        }
        out.extend(
            self.skips
                .iter()
                .map(|s| (settings::skip_id(s), true.into())),
        );
        let mut start = starting::Start::default();
        for (word, n) in &self.start {
            start.add(word, *n).expect("--start checks its words");
        }
        if !start.0.is_empty() {
            out.insert(
                starting::SETTING.into(),
                serde_json::to_value(start).expect("a map"),
            );
        }
        out.extend(self.raw.iter().cloned());
        out
    }

    pub(super) fn resolve(&self) -> Result<settings::NewGame> {
        settings::NewGame::from_settings(&self.settings(), &self.edits)
    }
}

fn field(arg: &str) -> Result<String> {
    checks::field(arg)?;
    Ok(arg.to_owned())
}

fn placement(arg: &str) -> Result<(u64, String)> {
    let (hash, item) = savefile::split(arg)?;
    Ok((hash.parse()?, item.to_owned()))
}

#[derive(clap::Args)]
pub(super) struct ModDir {
    /// Write only this mod folder, instead of installing to every target.
    #[arg(long)]
    mod_dir: Option<PathBuf>,
}

impl ModDir {
    /// The open game that stops an install, when there's no `--mod-dir`.
    pub(super) fn held(&self) -> Result<Option<String>> {
        if self.mod_dir.is_some() {
            return Ok(None);
        }
        let targets = Config::load()?.targets;
        Ok(generate::running(&targets).map(|t| t.name.clone()))
    }

    /// The folders written: `--mod-dir`, or every target's.
    pub(super) fn folders(&self) -> Result<Vec<(String, PathBuf)>> {
        Ok(match &self.mod_dir {
            Some(d) => vec![("--mod-dir".into(), d.clone())],
            None => About::folders(&Config::load()?.targets),
        })
    }

    /// `write` into `--mod-dir`, or install to every target like the app.
    /// Also returns whether it went to the targets.
    pub(super) fn write<T>(
        self,
        run: &Run,
        mut write: impl FnMut(&Path) -> Result<T>,
    ) -> Result<(T, bool)> {
        let mut each = |d: &Path| {
            let out = write(d)?;
            println!("wrote {}", d.display());
            Ok(out)
        };
        let targets = Config::load()?.targets;
        let mods = targets.iter().map(|t| t.mod_dir.as_path());
        if self.mod_dir.as_deref().is_none_or(|d| is_mod_dir(d, mods)) {
            crate::turn::hold(&crate::turn::session())?;
        }
        match self.mod_dir {
            Some(d) => Ok((each(&d)?, false)),
            None => Ok((generate::install(&targets, run, each)?, true)),
        }
    }
}

/// Whether `dir` is one of the targets' `mods`: writing there installs on
/// that game as surely as installing does, so it needs the turn too.
fn is_mod_dir<'a>(dir: &Path, mods: impl IntoIterator<Item = &'a Path>) -> bool {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    mods.into_iter().any(|m| real(m) == real(dir))
}

impl Shuffle {
    pub(super) fn settings(&self) -> Settings {
        let mut settings = self.new_game.settings();
        if self.vanilla {
            settings.insert(settings::SHUFFLE_ITEMS.into(), false.into());
        }
        settings
    }

    pub(super) fn extras(&self) -> generate::Extras {
        generate::Extras {
            defaults: self.new_game.edits.clone(),
            place: self.place.iter().cloned().collect(),
            spawn: self.spawn.clone(),
            give: self.give.clone(),
            progress: self.progress.clone(),
            fields: self.field.clone(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flow, item};

    #[test]
    fn a_mod_dir_that_is_a_targets_own_counts_as_an_install() {
        let dir = std::env::temp_dir().join(format!("zootr-moddir-{}", std::process::id()));
        let (ours, scratch) = (dir.join("mods/zootr"), dir.join("scratch"));
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        let mods = [ours.as_path()];
        assert!(is_mod_dir(&ours, mods));
        assert!(is_mod_dir(&dir.join("mods/../mods/zootr"), mods));
        assert!(!is_mod_dir(&scratch, mods));
        assert!(!is_mod_dir(&dir.join("never-made"), mods));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shuffle_flags_become_app_settings_and_extras() {
        needs_names!();
        let args = [
            "zootr",
            "shuffle",
            "--preset",
            "kakariko-gate",
            "--start",
            "longshot",
            "--start",
            "bombs=20",
            "--keep-intro",
            "--skip",
            "owl",
            "--skip",
            "all",
            "--default",
            "A=1",
            "--chest-size",
            "small",
            "--spawn",
            &format!("{}/{}", flow::BOTTOM_OF_THE_WELL, field::WELL_KAKARIKO_SIDE),
            "--give",
            item::GLASS_TRUTH,
            "--give",
            item::FAIRY_BLESS_MP,
        ];
        let Command::Shuffle(s) = Cli::try_parse_from(args).unwrap().command else {
            panic!()
        };
        assert_eq!(
            s.new_game.settings(),
            Settings::from([
                ("kakariko-gate".into(), true.into()),
                (
                    starting::SETTING.into(),
                    serde_json::json!({ "hookshot": 1, "longshot": 1, "bombs": 20 }),
                ),
                (settings::SKIP_INTRO.into(), false.into()),
                ("skip-owl".into(), true.into()),
                (settings::SKIP_ALL.into(), true.into()),
                (settings::CHEST_SIZE.into(), "small".into()),
            ])
        );
        assert_eq!(
            s.new_game.resolve().unwrap().defaults.last().unwrap(),
            "A=1"
        );
        assert_eq!(
            s.spawn.as_deref(),
            Some(format!("{}/{}", flow::BOTTOM_OF_THE_WELL, field::WELL_KAKARIKO_SIDE).as_str())
        );
        assert_eq!(s.give, [item::GLASS_TRUTH, item::FAIRY_BLESS_MP]);
        let bad = Cli::try_parse_from(["zootr", "shuffle", "--start", "hookshoot"]);
        assert!(bad.is_err());
    }

    #[test]
    fn a_setting_must_be_one_the_app_knows() {
        assert!(setting("keys=start").is_ok());
        let error = setting("minigames=true").unwrap_err().to_string();
        assert!(error.contains("minigames"), "{error}");
    }

    #[test]
    fn a_value_must_be_one_the_setting_takes() {
        let error = setting("keys=keysanity").unwrap_err().to_string();
        assert!(error.contains("singles"), "{error}");
        let error = setting("chests=maybe").unwrap_err().to_string();
        assert!(error.contains("true or false"), "{error}");
        assert!(setting("keys=singles").is_ok());
        assert!(setting("chests=false").is_ok());
    }

    #[test]
    fn photo_goes_or_shoots_by_hand_but_not_both() {
        let parse = |line: &str| Cli::try_parse_from(line.split(' '));
        assert!(parse("zootr photo --hash 1 --go --side left --step 1").is_ok());
        assert!(parse("zootr photo --hash 1 --here --out x").is_ok());
        assert!(parse("zootr photo --hash 1 --go --here --out x").is_err());
        assert!(parse("zootr photo --hash 1 --side left --out x").is_err());
        assert!(parse("zootr photo --hash 1 --here").is_err());
    }
}
