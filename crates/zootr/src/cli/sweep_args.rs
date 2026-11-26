//! The arguments of the night harness's runs: `zootr sweep` and
//! `zootr gauntlet`.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Subcommand};

use super::args::JudgeArgs;
use crate::sweep::gauntlet::Gauntlet;
use crate::sweep::gauntlet::hang::Limits;

/// `zootr sweep`.
#[derive(Args)]
pub(super) struct SweepArgs {
    #[command(subcommand)]
    pub(super) report: Option<SweepReport>,
    /// The night's folder.
    #[arg(long, required = true)]
    pub(super) out: Option<PathBuf>,
    /// Rounds to run, 0 for no end.
    #[arg(long, default_value_t = 1)]
    pub(super) rounds: usize,
    /// Spots a round at most, for a short run.
    #[arg(long)]
    pub(super) limit: Option<usize>,
    /// Every round in the settings mix whose name starts with this,
    /// like `everything`, instead of each mix in turn.
    #[arg(long)]
    pub(super) mix: Option<String>,
    /// Only the spots whose name, holder or seed item holds this, like
    /// `Cow`, for a run that tries one fix. Commas join parts that
    /// must all match: `Pot,Melody` is a pot holding a song.
    #[arg(long)]
    pub(super) only: Option<String>,
    /// Every round on this seed, as a night's round named it
    /// (`round-011-412851`), to see a failure again.
    #[arg(long)]
    pub(super) seed: Option<u64>,
    /// Step aside between spots whenever someone waits for this game
    /// in the turn queue, keeping the round's place in
    /// `<out>/resume.json`. Run again, it waits for the game, then
    /// finishes that round.
    #[arg(long = "yield", value_name = "GAME")]
    pub(super) yield_to: Option<String>,
    /// A photo pass's `photos.jsonl`: each holder is used from the side
    /// it was framed from first, until a night learns a better one.
    #[arg(long, value_name = "FILE")]
    pub(super) sides: Option<PathBuf>,
    #[command(flatten)]
    pub(super) judge: JudgeArgs,
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

/// `zootr gauntlet`.
#[derive(Args)]
pub(super) struct GauntletArgs {
    /// What to run: `places` is the tour alone, `holders` and `items`
    /// one row or column of the grid, `quick` and `full` the tour and
    /// the grid.
    #[arg(value_enum)]
    pub(super) run: crate::sweep::gauntlet::grid::Run,
    /// Only this holder kind, like `pot`. Repeatable.
    #[arg(long = "holder")]
    pub(super) holders: Vec<String>,
    /// Only this item kind, like `song`. Repeatable.
    #[arg(long = "item")]
    pub(super) items: Vec<String>,
    /// Tour only the places whose `MAP/SPOT` holds this, like a map's
    /// name, to see a hung load again. Repeatable.
    #[arg(long = "place")]
    pub(super) places: Vec<String>,
    /// Stop at the first failure.
    #[arg(long)]
    pub(super) fail_fast: bool,
    /// Seconds of pure black, the triforce gone, that make a hang.
    #[arg(long, default_value_t = 2.0)]
    pub(super) black_limit: f32,
    /// Seconds a load may take before it's a hang.
    #[arg(long, default_value_t = 45.0)]
    pub(super) load_limit: f32,
    /// The first round's seed, to repeat a run.
    #[arg(long)]
    pub(super) seed: Option<u64>,
    /// The run's folder. Defaults to `gauntlet/<time>-<run>/` beside
    /// the app's live logs.
    #[arg(long)]
    pub(super) out: Option<PathBuf>,
    /// A save folder each start loads instead of a new file: on a
    /// target with no `{save}` line, a kept new file.
    #[arg(long)]
    pub(super) pristine: Option<PathBuf>,
    /// A photo pass's `photos.jsonl`, for the side each holder is used
    /// from first.
    #[arg(long, value_name = "FILE")]
    pub(super) sides: Option<PathBuf>,
}

impl GauntletArgs {
    /// The run these ask for, its folder, and its pristine save and sides
    /// file.
    pub(super) fn gauntlet(
        self,
    ) -> (
        Gauntlet,
        Option<PathBuf>,
        (Option<PathBuf>, Option<PathBuf>),
    ) {
        let g = Gauntlet {
            run: self.run,
            holders: self.holders,
            items: self.items,
            places: self.places,
            fail_fast: self.fail_fast,
            limits: Limits {
                black: Duration::from_secs_f32(self.black_limit),
                load: Duration::from_secs_f32(self.load_limit),
            },
            seed: self.seed,
            turn: None,
        };
        (g, self.out, (self.pristine, self.sides))
    }
}
