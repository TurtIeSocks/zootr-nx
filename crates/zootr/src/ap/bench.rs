//! The AP bench: the same scenario timed every run, each metric kept as a
//! row in a TSV, and a metric flagged when it runs well past its last few.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;

use super::ApSeed;
use super::room::Archipelago;
use crate::generate::{self, Extras};
use crate::progress::Run;
use crate::tracker::{Flag, Tracker};

/// How many passes a pass metric times.
pub(super) const PASSES: usize = 100;

/// A metric more than this past the median of its last [`BASE`] rows is
/// flagged.
const SLOWER: f64 = 1.2;
const BASE: usize = 3;

pub const HEADER: &str =
    "date\tcommit\ttarget\troom\tpreset\tlocations\tmetric\tmedian_ms\tworst_ms";

/// One metric: its median and worst, in ms for a time, or in the unit its
/// name ends with (`%`).
#[derive(Debug, PartialEq)]
pub struct Sample {
    pub metric: &'static str,
    pub median: f64,
    pub worst: f64,
}

impl Sample {
    pub(super) fn once(metric: &'static str, took: Duration) -> Self {
        Self::of(metric, vec![took])
    }

    pub(super) fn of(metric: &'static str, mut runs: Vec<Duration>) -> Self {
        runs.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        Self {
            metric,
            median: ms(runs[runs.len() / 2]),
            worst: ms(*runs.last().expect("at least one run")),
        }
    }

    /// One reading, not a time.
    pub(super) fn value(metric: &'static str, v: f64) -> Self {
        Self {
            metric,
            median: v,
            worst: v,
        }
    }
}

fn timed<T>(f: impl FnOnce() -> Result<T>) -> Result<(T, Duration)> {
    let start = Instant::now();
    let out = f()?;
    Ok((out, start.elapsed()))
}

/// The export, which every preset shares: it writes every option's rows.
pub fn export(romfs: &Romfs, work: &Path) -> Result<Sample> {
    let dir = work.join("export");
    std::fs::create_dir_all(&dir)?;
    let ids = super::data_dir().join("ids.json");
    std::fs::copy(&ids, dir.join("ids.json")).with_context(|| format!("{}", ids.display()))?;
    let ((), took) = timed(|| super::write(romfs, &dir))?;
    Ok(Sample::once("export", took))
}

/// The metrics for `preset` that need no game: generate, open and the
/// client's pass over the flags of Link's slot. Also how many locations
/// that slot holds.
pub fn offline(
    romfs: &Romfs,
    ap: &Archipelago,
    work: &Path,
    preset: &str,
    seed: u64,
) -> Result<(usize, Vec<Sample>)> {
    let (generated, generate) = timed(|| ap.generate(preset, seed, &work.join("out")))?;

    let slot = ApSeed::read(&generated.slots[0])?;
    let locations = slot.placements.len();
    let (report, open) = timed(|| {
        let extras = Extras::ap(slot.clone());
        let settings = slot.full_settings();
        let run = Run::silent();
        generate::throwaway(|d| generate::generate(romfs, d, slot.seed, &settings, &extras, &run))
    })?;

    let gd = crate::recorder::gamedata(romfs)?;
    let mut tracker = Tracker::new(&gd, &report.spots)?;
    let pass = (0..PASSES)
        .map(|i| {
            let start = Instant::now();
            let flags: Vec<Flag> = tracker.flags().collect();
            tracker.load(|f| matches!(f, Flag::Bool(h) if (h as usize + i).is_multiple_of(2)));
            std::hint::black_box((flags.len(), tracker.checked().len()));
            start.elapsed()
        })
        .collect();

    Ok((
        locations,
        vec![
            Sample::once("generate", generate),
            Sample::once("open", open),
            Sample::of("pass-offline", pass),
        ],
    ))
}

/// One metric of one run, as the TSV keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub date: String,
    pub commit: String,
    pub target: String,
    pub room: String,
    pub preset: String,
    pub locations: usize,
    pub metric: String,
    pub median_ms: f64,
    pub worst_ms: f64,
}

impl Row {
    pub fn new(run: &RunInfo, preset: &str, locations: usize, s: &Sample) -> Self {
        Self {
            date: run.date.clone(),
            commit: run.commit.clone(),
            target: run.target.clone(),
            room: run.room.clone(),
            preset: preset.to_owned(),
            locations,
            metric: s.metric.to_owned(),
            median_ms: s.median,
            worst_ms: s.worst,
        }
    }

    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}",
            self.date,
            self.commit,
            self.target,
            self.room,
            self.preset,
            self.locations,
            self.metric,
            self.median_ms,
            self.worst_ms
        )
    }

    fn parse(line: &str) -> Option<Self> {
        let f: Vec<&str> = line.split('\t').collect();
        let [
            date,
            commit,
            target,
            room,
            preset,
            locations,
            metric,
            median,
            worst,
        ] = f[..]
        else {
            return None;
        };
        Some(Self {
            date: date.into(),
            commit: commit.into(),
            target: target.into(),
            room: room.into(),
            preset: preset.into(),
            locations: locations.parse().ok()?,
            metric: metric.into(),
            median_ms: median.parse().ok()?,
            worst_ms: worst.parse().ok()?,
        })
    }

    fn same(&self, other: &Self) -> bool {
        (self.metric == other.metric)
            & (self.target == other.target)
            & (self.room == other.room)
            & (self.preset == other.preset)
    }
}

/// What every row of one run shares.
pub struct RunInfo {
    pub date: String,
    pub commit: String,
    pub target: String,
    pub room: String,
}

impl RunInfo {
    /// This machine, the checkout `world` is in, no room.
    pub fn here(world: &Path, room: &str) -> Result<Self> {
        let commit = git(world, &["rev-parse", "--short", "HEAD"])?;
        Ok(Self {
            date: jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string(),
            commit,
            target: if cfg!(target_os = "macos") {
                "mac"
            } else {
                "other"
            }
            .into(),
            room: room.into(),
        })
    }
}

/// The rows in `text`, skipping the header and anything malformed.
pub fn rows(text: &str) -> Vec<Row> {
    text.lines().filter_map(Row::parse).collect()
}

/// How many times slower `row` is than the median of the last [`BASE`]
/// rows like it in `history`, when that's past [`SLOWER`].
pub fn slower(history: &[Row], row: &Row) -> Option<f64> {
    let mut last: Vec<f64> = (history.iter().rev())
        .filter(|h| h.same(row))
        .take(BASE)
        .map(|h| h.median_ms)
        .collect();
    if last.len() < BASE {
        return None;
    }
    last.sort_by(f64::total_cmp);
    let base = last[BASE / 2];
    let ratio = row.median_ms / base;
    (base > 0.0 && ratio > SLOWER).then_some(ratio)
}

/// The bench file in the main checkout's private `docs/`, whatever
/// worktree `world` is in. `None` when that checkout has no `docs/`.
pub fn default_file(world: &Path) -> Result<Option<PathBuf>> {
    let common = git(
        world,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let docs = Path::new(&common)
        .parent()
        .context("a git dir at the root")?
        .join("docs");
    Ok(docs.is_dir().then(|| docs.join("research/ap-bench.tsv")))
}

/// Appends `new` to `file`, writing the header first into a new one.
pub fn append(file: &Path, new: &[Row]) -> Result<()> {
    use std::io::Write;
    let fresh = !file.exists();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    if fresh {
        writeln!(f, "{HEADER}")?;
    }
    for row in new {
        writeln!(f, "{}", row.line())?;
    }
    Ok(())
}

/// The run's table: each row, and how much slower it is where flagged.
pub fn table(history: &[Row], new: &[Row]) -> String {
    let mut out = format!(
        "{:<8} {:<14} {:>9} {:>11} {:>11}  \n",
        "preset", "metric", "locations", "median ms", "worst ms"
    );
    for row in new {
        let flag = match slower(history, row) {
            Some(r) => format!("  slower x{r:.2}"),
            None => String::new(),
        };
        out += &format!(
            "{:<8} {:<14} {:>9} {:>11.3} {:>11.3}{flag}\n",
            row.preset, row.metric, row.locations, row.median_ms, row.worst_ms
        );
    }
    out
}

pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    ensure!(
        out.status.success(),
        "git {args:?} failed in {}",
        dir.display()
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(metric: &str, median_ms: f64) -> Row {
        Row {
            date: "2026-11-23 10:00".into(),
            commit: "abc1234".into(),
            target: "mac".into(),
            room: "none".into(),
            preset: "all-on".into(),
            locations: 400,
            metric: metric.into(),
            median_ms,
            worst_ms: median_ms * 2.0,
        }
    }

    #[test]
    fn a_row_round_trips_through_its_line() {
        let r = row("export", 1234.5);
        assert_eq!(
            r.line(),
            "2026-11-23 10:00\tabc1234\tmac\tnone\tall-on\t400\texport\t1234.500\t2469.000"
        );
        assert_eq!(rows(&format!("{HEADER}\n{}\n", r.line())), [r]);
    }

    #[test]
    fn fewer_than_three_rows_flag_nothing() {
        let history = [row("open", 100.0), row("open", 100.0)];
        assert_eq!(slower(&history, &row("open", 500.0)), None);
    }

    #[test]
    fn past_a_fifth_slower_than_the_last_three_s_median_is_flagged() {
        let history = [
            row("open", 999.0),
            row("open", 90.0),
            row("open", 100.0),
            row("open", 110.0),
        ];
        assert_eq!(slower(&history, &row("open", 119.0)), None);
        let ratio = slower(&history, &row("open", 121.0)).unwrap();
        assert!((ratio - 1.21).abs() < 1e-9);
    }

    #[test]
    fn only_rows_of_the_same_metric_target_room_and_preset_count() {
        let mut other = row("open", 10.0);
        other.preset = "defaults".into();
        let history = [
            row("open", 100.0),
            other.clone(),
            row("open", 100.0),
            other,
            row("open", 100.0),
        ];
        assert_eq!(slower(&history, &row("open", 115.0)), None);
        assert!(slower(&history, &row("open", 130.0)).is_some());
    }

    #[test]
    fn a_sample_takes_the_median_and_the_worst() {
        let ms = Duration::from_millis;
        let s = Sample::of("pass", vec![ms(5), ms(1), ms(3), ms(9), ms(2)]);
        assert_eq!((s.median, s.worst), (3.0, 9.0));
    }

    #[test]
    fn the_table_marks_a_slower_row() {
        let history = [row("open", 100.0), row("open", 100.0), row("open", 100.0)];
        let t = table(&history, &[row("open", 150.0), row("export", 5.0)]);
        assert!(t.lines().nth(1).unwrap().ends_with("slower x1.50"));
        assert!(!t.lines().nth(2).unwrap().contains("slower"));
    }

    #[test]
    fn appending_writes_the_header_once() {
        let file = std::env::temp_dir().join(format!("zootr-bench-{}.tsv", std::process::id()));
        std::fs::remove_file(&file).ok();
        append(&file, &[row("open", 1.0)]).unwrap();
        append(&file, &[row("open", 2.0)]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.matches("date\t").count(), 1);
        assert_eq!(rows(&text).len(), 2);
        std::fs::remove_file(&file).unwrap();
    }
}
