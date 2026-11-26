//! The gauntlet's report: each place and cell's outcome from the rows a
//! run appends, as `report.md`, and the exit code an agent reads.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

use super::grid::{self, HOLDERS, ITEMS};
use super::script::GAVE_UP;
use crate::ice_traps;
use crate::sweep::report::verdicts;

/// A row's `gauntlet` field: what it records.
pub mod row {
    pub const PLACE: &str = "place";
    pub const CELL: &str = "cell";
    /// A hang seen after a cell's sweep.
    pub const HANG: &str = "hang";
    pub const REFUSED: &str = "refused";
    /// A cell whose holder the round's seed didn't plan.
    pub const UNPLANNED: &str = "unplanned";
    pub const ERROR: &str = "error";
}

/// How a place or cell came out.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Pass,
    Fail(String),
    /// The shuffle refuses the cell: EIE debt, listed, never failing.
    Refused(String),
    /// The sweep has no trigger for the holder yet.
    NoTrigger(String),
    /// The harness broke, not the game.
    Harness(String),
}

impl Outcome {
    fn mark(&self) -> &'static str {
        match self {
            Outcome::Pass => "✅",
            Outcome::Fail(_) => "❌",
            Outcome::Refused(_) => "⛔",
            Outcome::NoTrigger(_) => "⏭",
            Outcome::Harness(_) => "⚠",
        }
    }
}

/// A cell's sweep row as an outcome.
fn swept(row: &Value) -> Outcome {
    outcome(&row["swept"]["verdict"], row["item_kind"] == grid::ICE_TRAP)
}

/// A verdict, as it serializes, as an outcome. A `trap` cell passes when
/// its get names the trap's disguise.
pub fn outcome(v: &Value, trap: bool) -> Outcome {
    let (verdict, why) = (v["verdict"].as_str().unwrap_or_default(), v["why"].as_str());
    let why = why.unwrap_or_default().to_owned();
    match verdict {
        v if verdicts::gave(v) => Outcome::Pass,
        verdicts::WRONG_ITEM if trap && why.contains(ice_traps::POOLED) => Outcome::Pass,
        "not_swept" => Outcome::NoTrigger(why),
        "setup_failed" | verdicts::NEIGHBOUR => Outcome::Harness(format!("{verdict}: {why}")),
        "bug" if why.starts_with(GAVE_UP) => Outcome::Fail(why),
        _ if why.is_empty() => Outcome::Fail(verdict.into()),
        _ => Outcome::Fail(format!("{verdict}: {why}")),
    }
}

/// A hang's words: its signal and what it carries.
fn hang_words(hang: &Value) -> String {
    let signal = hang["signal"].as_str().unwrap_or("hang");
    let rest: Vec<String> = (hang.as_object().into_iter().flatten())
        .filter(|(k, _)| *k != "signal")
        .map(|(k, v)| format!("{k} {}", v.as_str().map_or(v.to_string(), str::to_owned)))
        .collect();
    format!("hang: {signal} ({})", rest.join(", "))
}

/// What happened to one place or cell, and its evidence.
#[derive(Debug)]
struct Seen {
    outcome: Outcome,
    evidence: Vec<String>,
}

/// The rows of a run, read into places and cells by name.
#[derive(Default)]
struct Read {
    places: BTreeMap<String, Seen>,
    cells: BTreeMap<String, Seen>,
    errors: Vec<String>,
}

fn read(rows: &str) -> Read {
    let mut out = Read::default();
    for line in rows.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            out.errors.push(format!("an unreadable row: {line}"));
            continue;
        };
        let kind = row["gauntlet"].as_str().unwrap_or_default();
        let name = (row["name"].as_str().or(row["cell"].as_str()))
            .unwrap_or_default()
            .to_owned();
        let frame = row["frame"].as_str().map(str::to_owned);
        let why = row["why"].as_str().unwrap_or_default().to_owned();
        match kind {
            row::PLACE => {
                let outcome = match row.get("hang").filter(|h| !h.is_null()) {
                    Some(h) => Outcome::Fail(hang_words(h)),
                    None => Outcome::Pass,
                };
                let seen = Seen {
                    outcome,
                    evidence: frame.into_iter().collect(),
                };
                out.places.insert(name, seen);
            }
            row::CELL => {
                let cell = row["cell"].as_str().unwrap_or_default().to_owned();
                let shots = row["swept"]["shots"].as_array().into_iter().flatten();
                let evidence = shots
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect();
                let seen = Seen {
                    outcome: swept(&row),
                    evidence,
                };
                out.cells.insert(cell, seen);
            }
            row::HANG => {
                let seen = out.cells.entry(name).or_insert(Seen {
                    outcome: Outcome::Pass,
                    evidence: vec![],
                });
                seen.outcome = Outcome::Fail(hang_words(&row["hang"]));
                seen.evidence.extend(frame);
                seen.evidence
                    .extend(row["dump"].as_str().map(str::to_owned));
            }
            row::REFUSED => _ = out.cells.insert(name, seen_of(Outcome::Refused(why))),
            row::UNPLANNED => _ = out.cells.insert(name, seen_of(Outcome::Harness(why))),
            row::ERROR => out
                .errors
                .push(row["error"].as_str().unwrap_or_default().into()),
            // The sweep's own rows (its spot rows sit in the cell rows).
            "" => {}
            other => out.errors.push(format!("a row of no known kind {other:?}")),
        }
    }
    out
}

fn seen_of(outcome: Outcome) -> Seen {
    Seen {
        outcome,
        evidence: vec![],
    }
}

/// `report.md` for the rows of a run named `run`, and its exit code: 1
/// when anything failed, else 2 when the harness broke or a cell has no
/// trigger yet, else 0.
pub fn render(rows: &str, run: &str) -> (String, i32) {
    let read = read(rows);
    let mut md = format!("# Gauntlet: {run}\n\n");
    let all = || read.places.values().chain(read.cells.values());
    let count = |f: fn(&Outcome) -> bool| all().filter(|s| f(&s.outcome)).count();
    let fails = count(|o| matches!(o, Outcome::Fail(_)));
    let harness = count(|o| matches!(o, Outcome::Harness(_))) + read.errors.len();
    let untriggered = count(|o| matches!(o, Outcome::NoTrigger(_)));
    let tally = |seen: &BTreeMap<String, Seen>, f: fn(&Outcome) -> bool| {
        seen.values().filter(|s| f(&s.outcome)).count()
    };
    md += "| | Pass | Fail | Refused | No trigger | Harness |\n|---|---|---|---|---|---|\n";
    for (label, seen) in [("Places", &read.places), ("Cells", &read.cells)] {
        let _ = writeln!(
            md,
            "| {label} | {} | {} | {} | {} | {} |",
            tally(seen, |o| *o == Outcome::Pass),
            tally(seen, |o| matches!(o, Outcome::Fail(_))),
            tally(seen, |o| matches!(o, Outcome::Refused(_))),
            tally(seen, |o| matches!(o, Outcome::NoTrigger(_))),
            tally(seen, |o| matches!(o, Outcome::Harness(_))),
        );
    }
    if !read.cells.is_empty() {
        grid(&mut md, &read.cells);
    }
    let section = |md: &mut String, title: &str, f: fn(&Outcome) -> Option<&String>| {
        let items: Vec<_> = (read.places.iter().chain(&read.cells))
            .filter_map(|(name, s)| Some((name, f(&s.outcome)?, &s.evidence)))
            .collect();
        if items.is_empty() {
            return;
        }
        let _ = writeln!(md, "\n## {title}\n");
        for (name, why, evidence) in items {
            let _ = writeln!(md, "- **{name}**: {why}");
            for e in evidence {
                let _ = writeln!(md, "  - `{e}`");
            }
        }
    };
    section(&mut md, "Failures", |o| match o {
        Outcome::Fail(w) => Some(w),
        _ => None,
    });
    section(&mut md, "Harness", |o| match o {
        Outcome::Harness(w) => Some(w),
        _ => None,
    });
    if !read.errors.is_empty() {
        let _ = writeln!(md, "\n## Errors\n");
        for e in &read.errors {
            let _ = writeln!(md, "- {e}");
        }
    }
    section(&mut md, "No trigger yet", |o| match o {
        Outcome::NoTrigger(w) => Some(w),
        _ => None,
    });
    section(&mut md, "Refused (EIE debt)", |o| match o {
        Outcome::Refused(w) => Some(w),
        _ => None,
    });
    let code = match (fails, harness + untriggered) {
        (0, 0) => 0,
        (0, _) => 2,
        _ => 1,
    };
    (md, code)
}

/// The holder × item table: a mark a cell, `·` for one not run.
fn grid(md: &mut String, cells: &BTreeMap<String, Seen>) {
    let _ = writeln!(
        md,
        "\n## Grid\n\n✅ pass · ❌ fail · ⛔ refused · ⏭ no trigger · ⚠ harness\n"
    );
    let head: Vec<&str> = ITEMS.iter().map(|k| k.name).collect();
    let _ = writeln!(md, "| holder | {} |", head.join(" | "));
    let _ = writeln!(md, "|---|{}", "---|".repeat(head.len()));
    for h in &HOLDERS {
        let marks: Vec<&str> = (ITEMS.iter())
            .map(|i| {
                let name = format!("{} × {}", h.name, i.name);
                cells.get(&name).map_or("·", |s| s.outcome.mark())
            })
            .collect();
        if marks.iter().any(|m| *m != "·") {
            let _ = writeln!(md, "| {} | {} |", h.name, marks.join(" | "));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn lines(rows: &[Value]) -> String {
        rows.iter().map(|r| format!("{r}\n")).collect()
    }

    fn cell(name: &str, item_kind: &str, verdict: Value) -> Value {
        json!({"gauntlet": "cell", "cell": name, "item_kind": item_kind,
               "swept": {"verdict": verdict, "shots": ["end.png"]}})
    }

    #[test]
    fn all_passing_exits_zero() {
        let rows = lines(&[
            json!({"gauntlet": "place", "name": "FakeMap/FakeSpot", "hang": null}),
            cell("pot × song", "song", json!({"verdict": "pass"})),
        ]);
        let (md, code) = render(&rows, "fake");
        assert_eq!(code, 0, "{md}");
        assert!(md.contains("| pot |"), "{md}");
    }

    #[test]
    fn a_hung_place_or_a_wrong_item_fails() {
        let rows = lines(&[json!({"gauntlet": "place", "name": "FakeMap/FakeSpot",
            "hang": {"signal": "black", "secs": 2.4, "map": "FakeMap"}, "frame": "f.png"})]);
        let (md, code) = render(&rows, "fake");
        assert_eq!(code, 1);
        assert!(md.contains("hang: black") && md.contains("`f.png`"), "{md}");
        let rows = lines(&[cell(
            "cow × bottle",
            "bottle",
            json!({"verdict": "wrong_item", "why": "IsGet.Item_Fake"}),
        )]);
        assert_eq!(render(&rows, "fake").1, 1);
    }

    #[test]
    fn round_errors_unplanned_cells_and_bad_rows_break_the_harness() {
        for row in [
            json!({"gauntlet": "error", "round": 0, "error": "too few key chests"}).to_string(),
            json!({"gauntlet": "unplanned", "cell": "pot × song", "why": "no spot"}).to_string(),
            json!({"gauntlet": "fake kind"}).to_string(),
            "{not json".into(),
        ] {
            let (md, code) = render(&format!("{row}\n"), "fake");
            assert_eq!(code, 2, "{md}");
        }
        let rows =
            lines(&[json!({"gauntlet": "error", "round": 0, "error": "too few key chests"})]);
        let (md, _) = render(&rows, "fake");
        assert!(md.contains("## Errors\n\n- too few key chests"), "{md}");
    }

    #[test]
    fn a_hang_after_a_cell_overrides_its_verdict() {
        let rows = lines(&[
            cell("pot × song", "song", json!({"verdict": "nothing"})),
            json!({"gauntlet": "hang", "name": "pot × song", "hang": {"signal": "unread", "why": "x"}}),
        ]);
        let (md, code) = render(&rows, "fake");
        assert_eq!(code, 1);
        assert!(md.contains("hang: unread"), "{md}");
    }

    #[test]
    fn refused_cells_never_fail_and_untriggered_ones_exit_two() {
        let rows = lines(&[
            json!({"gauntlet": "refused", "cell": "pot × small-key", "why": "uncredited"}),
            cell("pot × song", "song", json!({"verdict": "pass"})),
        ]);
        assert_eq!(render(&rows, "fake").1, 0);
        let rows = lines(&[cell(
            "beehive × song",
            "song",
            json!({"verdict": "not_swept", "why": "out of scope"}),
        )]);
        let (md, code) = render(&rows, "fake");
        assert_eq!(code, 2);
        assert!(md.contains("No trigger yet"), "{md}");
    }

    #[test]
    fn a_trap_landing_as_its_disguise_passes_and_a_give_up_fails() {
        let trap = format!("{}001", ice_traps::POOLED);
        let rows = lines(&[cell(
            "pot × ice-trap",
            "ice-trap",
            json!({"verdict": "wrong_item", "why": trap}),
        )]);
        assert_eq!(render(&rows, "fake").1, 0);
        let why = format!("{GAVE_UP} 30 times");
        let rows = lines(&[cell(
            "cow × song",
            "song",
            json!({"verdict": "bug", "why": why}),
        )]);
        assert_eq!(render(&rows, "fake").1, 1);
    }
}
