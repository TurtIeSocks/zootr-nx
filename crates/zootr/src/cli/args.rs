//! The command tree: the arguments each command takes and what they mean,
//! with the value parsers and the settings they resolve to.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::config::Config;
use crate::progress::Run;
use crate::settings::Settings;
use crate::sweep::drills::Skill;
use crate::vision::{Api, Effort};
use crate::warp::Side;
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
    /// `free` hands it over.
    #[command(hide = true)]
    Turn {
        to: Option<String>,
        /// Why, or what's being tested.
        #[arg(long, default_value = "")]
        note: String,
        /// The Claude session taking or handing over the turn, by a short
        /// name of its own. Needed to take a turn from Claude's shell.
        #[arg(long, default_value = "")]
        session: String,
        /// Hand over the maintainer's or another session's turn from
        /// Claude's shell anyway.
        #[arg(long)]
        force: bool,
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
    Logic {
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
    /// seed installed with `dev_panel` on in the config. The menu shows it
    /// the next time it opens. Needs macOS or a target with a bridge.
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
    /// Add a target from a shared preset, or print one to share.
    #[command(subcommand)]
    Target(TargetCommand),
    /// Dev panel requests for the running game, run by our hook on Link's
    /// next update. Needs a seed installed with `dev_panel` on in the config.
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
    /// target's game, the native build when it has a bridge.
    #[command(hide = true)]
    Look {
        #[arg(required = true)]
        questions: Vec<String>,
        /// The vision model. Defaults to `claude-opus-5-5` on the
        /// Anthropic API.
        #[arg(long, env = "ZOOTR_VISION_MODEL")]
        model: Option<String>,
        /// Which shape the vision model's API speaks.
        #[arg(long, env = "ZOOTR_VISION_API", value_enum, default_value_t = Api::Anthropic)]
        api: Api,
        /// The vision model's API, without `/v1/...`. Defaults to
        /// `https://api.anthropic.com` on the Anthropic API and is
        /// required on OpenAI's.
        #[arg(long, env = "ZOOTR_VISION_URL")]
        url: Option<String>,
        /// A file holding the API's key, kept out of the command line.
        #[arg(long, env = "ZOOTR_VISION_KEY_FILE")]
        key_file: PathBuf,
        /// How hard an Anthropic model thinks.
        #[arg(long, value_enum)]
        effort: Option<Effort>,
        /// Keep the frame in this folder, so it can be opened when an
        /// answer looks wrong.
        #[arg(long, value_name = "DIR")]
        keep: Option<PathBuf>,
        /// Ask about this saved frame (a sweep's shot) instead of the
        /// game's.
        #[arg(long, value_name = "PNG", conflicts_with = "keep")]
        frame: Option<PathBuf>,
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
    #[arg(long, default_value = "")]
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
}

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
    /// Items a new game starts with. Repeatable.
    #[arg(long = "start", value_name = "ITEM", value_parser = starting::parser())]
    start: Vec<String>,
    /// Any app setting by id, for those without a flag of their own, e.g.
    /// `progressive-items=false`. Repeatable.
    #[arg(long = "setting", value_name = "ID=VALUE", value_parser = setting)]
    raw: Vec<(String, serde_json::Value)>,
}

/// `ID=VALUE`, where `true` and `false` are toggles and anything else a
/// choice. The id must be one of the app's settings.
pub(crate) fn setting(arg: &str) -> Result<(String, serde_json::Value)> {
    let (id, value) = savefile::split(arg)?;
    anyhow::ensure!(
        settings::schema().iter().any(|def| def.id == id),
        "no setting is called {id}"
    );
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
        out.extend(self.start.iter().map(|s| (starting::id(s), true.into())));
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

/// A number of seconds above zero.
fn positive(arg: &str) -> Result<f32> {
    let secs: f32 = arg.parse()?;
    anyhow::ensure!(secs > 0.0, "{arg} isn't above zero");
    Ok(secs)
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
        match self.mod_dir {
            Some(d) => Ok((each(&d)?, false)),
            None => Ok((
                generate::install(&Config::load()?.targets, run, each)?,
                true,
            )),
        }
    }
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
    fn shuffle_flags_become_app_settings_and_extras() {
        needs_names!();
        let args = [
            "zootr",
            "shuffle",
            "--preset",
            "warp-kit",
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
                ("warp-kit".into(), true.into()),
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
    }

    #[test]
    fn a_setting_must_be_one_the_app_knows() {
        assert!(setting("keys=start").is_ok());
        let error = setting("minigames=true").unwrap_err().to_string();
        assert!(error.contains("minigames"), "{error}");
    }
}

/// A dev switch `dev switches` names.
#[derive(Clone, Copy, Debug, PartialEq, clap::ValueEnum)]
pub enum Switch {
    Health,
    Magic,
    Resources,
}
