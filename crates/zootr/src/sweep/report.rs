//! The scoreboards: the drills' per skill and per drill, and the sweep's
//! per holder kind and per failure ending. Both read the JSON lines rows a
//! run appends.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde_json::Value;

use super::night::holder_kind;

/// Passes out of runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub passes: usize,
    pub runs: usize,
}

impl Tally {
    fn add(&mut self, passed: bool) {
        self.passes += usize::from(passed);
        self.runs += 1;
    }
}

impl fmt::Display for Tally {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.passes, self.runs)
    }
}

/// One drill run that didn't pass.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub drill: String,
    pub verdict: String,
    pub ending: String,
    /// The last frame the run saved.
    pub shot: String,
}

/// The drills' scoreboard.
#[derive(Debug, Default)]
pub struct Drills {
    pub skills: BTreeMap<String, Tally>,
    pub drills: BTreeMap<String, Tally>,
    pub failures: Vec<Failure>,
}

/// The sweep's scoreboard.
#[derive(Debug, Default)]
pub struct Sweep {
    /// Rows by holder kind, then by verdict.
    pub kinds: BTreeMap<String, BTreeMap<String, usize>>,
    /// Rows that didn't pass, by how they ended.
    pub endings: BTreeMap<String, usize>,
}

/// The rows of `text` that parse, skipping a night's error notes.
fn rows(text: &str) -> impl Iterator<Item = Value> + '_ {
    (text.lines())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|r| r.get("swept").is_some())
}

fn verdict(row: &Value) -> &str {
    row["swept"]["verdict"]["verdict"]
        .as_str()
        .unwrap_or("none")
}

/// How a row's walk and use ended, as `walk/reason/use`: `gave up/blocked/
/// nothing` is a walk stopped by something in the way. `-` stands for a
/// part the row lacks.
pub fn ending(row: &Value) -> String {
    let used = &row["swept"]["used"];
    let part = |v: &Value| v.as_str().unwrap_or("-").to_owned();
    let [walk, why, use_] = [
        &used["went"]["result"],
        &used["went"]["reason"],
        &used["result"],
    ];
    format!("{}/{}/{}", part(walk), part(why), part(use_))
}

/// Tally the drill rows of `text`.
pub fn drills(text: &str) -> Drills {
    let mut board = Drills::default();
    for row in rows(text) {
        let (Some(drill), Some(skill)) = (row["drill"].as_str(), row["skill"].as_str()) else {
            continue;
        };
        let passed = verdict(&row) == "pass";
        board
            .skills
            .entry(skill.to_owned())
            .or_default()
            .add(passed);
        board
            .drills
            .entry(drill.to_owned())
            .or_default()
            .add(passed);
        if !passed {
            let shots = row["swept"]["shots"].as_array();
            let shot = shots.and_then(|s| s.last()).and_then(Value::as_str);
            board.failures.push(Failure {
                drill: drill.to_owned(),
                verdict: verdict(&row).to_owned(),
                ending: ending(&row),
                shot: shot.unwrap_or("-").to_owned(),
            });
        }
    }
    board
}

/// Tally the sweep rows of `text` from round `from` through `to`.
pub fn sweep(text: &str, from: Option<u64>, to: Option<u64>) -> Sweep {
    let mut board = Sweep::default();
    let within = |r: &Value| {
        let round = r["round"].as_u64().unwrap_or(0);
        from.is_none_or(|f| round >= f) && to.is_none_or(|t| round <= t)
    };
    for row in rows(text).filter(within) {
        let kind = holder_kind(row["spot"]["holder"].as_str().unwrap_or("-"));
        let verdicts = board.kinds.entry(kind.to_owned()).or_default();
        *verdicts.entry(verdict(&row).to_owned()).or_default() += 1;
        if verdict(&row) != "pass" {
            *board.endings.entry(ending(&row)).or_default() += 1;
        }
    }
    board
}

/// The rows a sweep folder holds: its own `results.jsonl`, or each night
/// folder's inside it, oldest first.
pub fn sweep_rows(dir: &Path) -> std::io::Result<String> {
    let own = dir.join("results.jsonl");
    if own.exists() {
        return std::fs::read_to_string(own);
    }
    let mut nights: Vec<_> = (std::fs::read_dir(dir)?.flatten())
        .map(|n| n.path().join("results.jsonl"))
        .filter(|f| f.exists())
        .collect();
    nights.sort();
    nights.iter().map(std::fs::read_to_string).collect()
}

/// Lines of cells, each column padded to its widest.
fn table(lines: &[Vec<String>]) -> String {
    let widths = (0..lines.iter().map(Vec::len).max().unwrap_or(0))
        .map(|c| {
            lines
                .iter()
                .filter_map(|l| l.get(c))
                .map(|s| s.chars().count())
                .max()
        })
        .map(Option::unwrap_or_default)
        .collect::<Vec<_>>();
    let line = |l: &Vec<String>| {
        let cells = l.iter().zip(&widths).map(|(s, w)| format!("{s:w$}"));
        cells.collect::<Vec<_>>().join("  ").trim_end().to_owned() + "\n"
    };
    lines.iter().map(line).collect()
}

fn head<const N: usize>(names: [&str; N]) -> Vec<String> {
    names.map(String::from).to_vec()
}

fn cells<const N: usize>(cells: [&dyn fmt::Display; N]) -> Vec<String> {
    cells.iter().map(|c| c.to_string()).collect()
}

impl fmt::Display for Drills {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut skills = vec![head(["skill", "passes"])];
        skills.extend(self.skills.iter().map(|(s, t)| cells([s, t])));
        let mut drills = vec![head(["drill", "passes"])];
        drills.extend(self.drills.iter().map(|(d, t)| cells([d, t])));
        write!(f, "{}\n{}", table(&skills), table(&drills))?;
        if !self.failures.is_empty() {
            let mut failed = vec![head(["failed drill", "verdict", "ending", "frame"])];
            let row = |x: &Failure| cells([&x.drill, &x.verdict, &x.ending, &x.shot]);
            failed.extend(self.failures.iter().map(row));
            write!(f, "\n{}", table(&failed))?;
        }
        Ok(())
    }
}

impl fmt::Display for Sweep {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut kinds = vec![head(["holder kind", "rows", "verdicts"])];
        for (kind, verdicts) in &self.kinds {
            let rows: usize = verdicts.values().sum();
            let each = verdicts.iter().map(|(v, n)| format!("{v} {n}"));
            let each = each.collect::<Vec<_>>().join(", ");
            kinds.push(cells([kind, &rows, &each]));
        }
        let mut endings = vec![head(["failure ending", "rows"])];
        let mut sorted: Vec<_> = self.endings.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        endings.extend(sorted.into_iter().map(|(e, n)| cells([e, n])));
        write!(f, "{}\n{}", table(&kinds), table(&endings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row the way a run appends it: its walk's end and reason, and its
    /// use's result, `None` for a row without a walk.
    fn row(round: u64, holder: &str, verdict: &str, walk: Option<(&str, &str, &str)>) -> String {
        let used = walk.map(|(result, reason, used)| {
            let reason = (!reason.is_empty()).then_some(reason);
            serde_json::json!({"result": used, "went": {"result": result, "reason": reason}})
        });
        let swept = serde_json::json!({
            "verdict": {"verdict": verdict}, "shots": ["a.png", "b.png"], "used": used,
        });
        serde_json::json!({"round": round, "spot": {"holder": holder}, "swept": swept}).to_string()
    }

    fn drill_row(drill: &str, skill: &str, verdict: &str) -> String {
        let mut r: Value = serde_json::from_str(&row(0, "Fake", verdict, None)).unwrap();
        r["drill"] = drill.into();
        r["skill"] = skill.into();
        r.to_string()
    }

    #[test]
    fn a_drill_board_tallies_each_skill_and_drill_and_lists_each_failure() {
        let text = [
            drill_row("fake chest", "chest", "pass"),
            drill_row("fake chest", "chest", "nothing"),
            drill_row("fake box", "box", "pass"),
            row(0, "Fake", "pass", None),
            r#"{"round": 0, "error": "the game quit"}"#.to_owned(),
        ]
        .join("\n");
        let board = drills(&text);
        assert_eq!(board.skills["chest"], Tally { passes: 1, runs: 2 });
        assert_eq!(board.skills["box"], Tally { passes: 1, runs: 1 });
        assert_eq!(board.drills.len(), 2, "a sweep row isn't a drill");
        assert_eq!(
            board.failures,
            [Failure {
                drill: "fake chest".into(),
                verdict: "nothing".into(),
                ending: "-/-/-".into(),
                shot: "b.png".into(),
            }]
        );
        let shown = board.to_string();
        assert!(shown.contains("chest  1/2"), "{shown}");
        assert!(shown.contains("fake chest    nothing"), "{shown}");
    }

    #[test]
    fn a_sweep_board_tallies_kinds_and_endings_between_two_rounds() {
        let text = [
            row(1, "Fake_Box_A_01", "pass", Some(("arrived", "", "event"))),
            row(
                2,
                "Fake_Box_A_02",
                "nothing",
                Some(("gave up", "blocked", "nothing")),
            ),
            row(
                2,
                "Fake_Box_A_03",
                "nothing",
                Some(("gave up", "blocked", "nothing")),
            ),
            row(3, "Fake Shop 1", "nothing", Some(("arrived", "", "event"))),
            row(9, "Fake_Box_A_04", "nothing", None),
        ]
        .join("\n");
        let board = sweep(&text, Some(1), Some(3));
        assert_eq!(board.kinds["Fake_Box_A"]["pass"], 1);
        assert_eq!(board.kinds["Fake_Box_A"]["nothing"], 2);
        assert_eq!(board.kinds["Fake Shop"]["nothing"], 1);
        assert_eq!(board.endings["gave up/blocked/nothing"], 2);
        assert_eq!(board.endings["arrived/-/event"], 1);
        assert_eq!(board.endings.len(), 2, "round 9 is past the mark");
        let shown = board.to_string();
        let first = shown.lines().find(|l| l.contains("/")).unwrap();
        assert!(
            first.starts_with("gave up/blocked/nothing"),
            "most rows first: {shown}"
        );
    }

    #[test]
    fn a_sweep_folder_reads_its_own_rows_or_each_nights() {
        let dir = std::env::temp_dir().join(format!("zootr-report-{}", std::process::id()));
        for night in ["night-b", "night-a"] {
            std::fs::create_dir_all(dir.join(night)).unwrap();
            std::fs::write(dir.join(night).join("results.jsonl"), format!("{night}\n")).unwrap();
        }
        assert_eq!(sweep_rows(&dir).unwrap(), "night-a\nnight-b\n");
        std::fs::write(dir.join("results.jsonl"), "own\n").unwrap();
        assert_eq!(sweep_rows(&dir).unwrap(), "own\n");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
