//! What a run leaves behind: `report.md`, the screenshots and the raw
//! change log, in a folder of its own.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::known::flag;
use crate::live;
use crate::savefile::Change;

/// The raw change log's file name.
pub const CHANGES: &str = "changes.log";
/// The most changes `report.md` lists. The log has them all.
const SHOWN: usize = 300;

#[derive(Debug, Default)]
pub struct Report {
    pub name: String,
    pub script: PathBuf,
    /// The installed seed the run played.
    pub seed: String,
    /// Local time the run began.
    pub started: String,
    /// What the test start did.
    pub setup: Vec<String>,
    pub steps: Vec<Done>,
    /// Every GameData change, by time since the run began.
    pub changes: Vec<(Duration, Change)>,
    pub stall: Option<Stalled>,
    /// Screenshot file names, in the report's folder.
    pub shots: Vec<String>,
    /// Things that went wrong without failing the run, like a screenshot.
    pub notes: Vec<String>,
    /// Why the run stopped before its steps, outside any step.
    pub error: Option<String>,
}

/// A step as it ran.
#[derive(Debug)]
pub struct Done {
    pub line: usize,
    pub what: String,
    pub at: Duration,
    /// What it did, or why it failed.
    pub outcome: Result<String, String>,
}

/// An event that played while nothing changed.
#[derive(Debug, PartialEq)]
pub struct Stalled {
    pub at: Duration,
    pub map: String,
    pub quiet: Duration,
}

fn stamp(d: Duration) -> String {
    live::stamp(d.as_secs())
}

impl Report {
    pub fn passed(&self) -> bool {
        self.error.is_none() && self.stall.is_none() && self.steps.iter().all(|s| s.outcome.is_ok())
    }

    /// Why the run failed, in one line.
    pub fn verdict(&self) -> String {
        if let Some(e) = &self.error {
            return format!("FAIL: {e}");
        }
        if let Some(s) = &self.stall {
            return format!("FAIL: stalled on {} at {}", s.map, stamp(s.at));
        }
        match self.steps.iter().find(|s| s.outcome.is_err()) {
            Some(Done {
                line,
                outcome: Err(why),
                ..
            }) => format!("FAIL: the step at line {line}: {why}"),
            _ => "PASS".into(),
        }
    }

    pub fn markdown(&self) -> String {
        let mut md = String::new();
        let _ = writeln!(md, "# {}\n\n**{}**\n", self.name, self.verdict());
        let _ = writeln!(md, "- Script: `{}`", self.script.display());
        let _ = writeln!(md, "- Seed: {}", self.seed);
        let _ = writeln!(md, "- Started: {}", self.started);
        if !self.setup.is_empty() {
            let _ = writeln!(md, "\n## Setup\n");
            self.setup.iter().for_each(|l| _ = writeln!(md, "- {l}"));
        }
        let _ = writeln!(
            md,
            "\n## Steps\n\n| Line | Step | At | Result |\n|---|---|---|---|"
        );
        for s in &self.steps {
            let result = match &s.outcome {
                Ok(said) => format!("pass, {said}"),
                Err(why) => format!("**fail**, {why}"),
            };
            let cell = |t: &str| t.replace('|', "\\|").replace('\n', " ");
            let (what, at) = (cell(&s.what), stamp(s.at));
            let _ = writeln!(md, "| {} | {what} | {at} | {} |", s.line, cell(&result));
        }
        if let Some(s) = &self.stall {
            let _ = writeln!(
                md,
                "\n## Stall\n\nAt {} on {}, an event played and nothing changed for {} s.",
                stamp(s.at),
                s.map,
                s.quiet.as_secs()
            );
        }
        if !self.notes.is_empty() {
            let _ = writeln!(md, "\n## Notes\n");
            self.notes.iter().for_each(|n| _ = writeln!(md, "- {n}"));
        }
        if !self.shots.is_empty() {
            let _ = writeln!(md, "\n## Screenshots\n");
            self.shots
                .iter()
                .for_each(|s| _ = writeln!(md, "![{s}]({s})"));
        }
        let _ = writeln!(md, "\n## {} changes\n\n```", flag::GAME_DATA);
        for (at, c) in self.changes.iter().take(SHOWN) {
            let _ = writeln!(md, "{} {c}", stamp(*at));
        }
        if let Some(more) = self.changes.len().checked_sub(SHOWN).filter(|&n| n > 0) {
            let _ = writeln!(md, "... {more} more in {CHANGES}");
        }
        md.push_str("```\n");
        md
    }

    /// Write `report.md` and the change log to `dir`.
    pub fn write(&self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
        let log: String = (self.changes.iter())
            .map(|(at, c)| format!("{} {c}\n", stamp(*at)))
            .collect();
        std::fs::write(dir.join(CHANGES), log)?;
        let path = dir.join("report.md");
        std::fs::write(&path, self.markdown())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;
    use crate::known::{flag, flow};

    #[test]
    fn a_report_shows_the_failing_step_and_the_stall() {
        let gd = GameData::default();
        let change = Change::new(&gd, 0xa021, "Bool", "false -> true");
        let mut report = Report {
            name: "Zelda".into(),
            steps: vec![Done {
                line: 7,
                what: format!("play {}", flow::GOTO_HYRULE_CASTLE),
                at: Duration::from_secs(2),
                outcome: Ok("sent".into()),
            }],
            changes: vec![(Duration::from_secs(3), change)],
            shots: vec!["end.png".into()],
            ..Default::default()
        };
        assert!(report.passed());
        assert!(report.markdown().contains("**PASS**"));
        report.steps.push(Done {
            line: 10,
            what: "until A=true".into(),
            at: Duration::from_secs(4),
            outcome: Err("timed out | A is false".into()),
        });
        assert_eq!(
            report.verdict(),
            "FAIL: the step at line 10: timed out | A is false"
        );
        report.stall = Some(Stalled {
            at: Duration::from_secs(70),
            map: flag::HYRULE_CASTLE.to_string(),
            quiet: Duration::from_secs(15),
        });
        let md = report.markdown();
        assert!(
            md.contains(&format!("stalled on {} at [01:10]", flag::HYRULE_CASTLE)),
            "{md}"
        );
        assert!(md.contains("timed out \\| A is false"), "{md}");
        assert!(md.contains("[00:03] ") && md.contains("![end.png](end.png)"));
    }
}
