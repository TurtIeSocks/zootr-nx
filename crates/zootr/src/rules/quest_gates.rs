//! The quest gate audit: every check and hand exit whose giver waits on a
//! quest step, beside its rule. A gate is met when the rule needs a word
//! whose [`IMPLIES`] row promises the step, or excused by the check's
//! `quest-gate-ok` reason. The rest are findings, and CI keeps them at none.

use std::collections::BTreeSet;
use std::fmt;

use anyhow::Result;
use ounce_formats::Romfs;

use super::vocab::{self, IMPLIES};
use super::{Book, Expr};
use crate::flagmap::{Gate, Graph};
use crate::known::{Name, field, flow};

/// The hand exits a flow opens, by the step that flow moves its quest to
/// as it opens them.
const EXIT_GIVERS: &[(&str, &str, (&Name, &Name), &str)] = &[(
    "market-child",
    "market-adult",
    (flow::DOOR_OF_TIME, field::DOOR_OF_TIME_OPEN_DOOR),
    "Step06",
)];

/// A word's promise: quest `quest` at step `step` or past it.
pub struct Implied {
    pub word: String,
    pub quest: String,
    pub step: String,
}

/// The [`IMPLIES`] rows, their words as rules hold them.
pub fn implies() -> Result<Vec<Implied>> {
    (IMPLIES.iter())
        .map(|&(word, quest, step)| {
            let (quest, step) = (quest.get().to_owned(), step.to_owned());
            Ok(Implied {
                word: vocab::id(word)?,
                quest,
                step,
            })
        })
        .collect()
}

/// A gate no word in the rule promises.
pub struct Finding {
    /// `check NAME HASH` or `exit FROM to TO`.
    pub what: String,
    pub gate: Gate,
    pub rule: String,
}

#[derive(Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// Checks and exits with a gate excused by `quest-gate-ok`.
    pub excused: usize,
    /// How many checks and exits the audit read a giver for.
    pub read: usize,
    /// Checks whose giver the flag map doesn't have, so they went unread.
    pub unread: Vec<String>,
}

/// The audit of every check and hand exit.
pub fn audit(romfs: &Romfs, graph: &Graph, book: &Book) -> Result<Report> {
    let implied = implies()?;
    let mut report = Report::default();
    let always = super::files::always();
    let checks = crate::checks::all(romfs)?;
    for check in checks.into_iter().filter(|c| !crate::ledger::kept_out(c)) {
        let entry = book.check(check.hash);
        let what = match entry {
            Some(e) => format!("check {} {}", e.name, check.hash),
            None => format!("check {} {} {}", check.holder, check.area, check.hash),
        };
        let gates = match crate::gifts::of(&check) {
            Some(gift) => graph.gates_of_grant(gift.flow.get(), gift.item.get()),
            None => Some(graph.gates_of_placement(check.hash)),
        };
        let rule = entry.map_or(&always, |e| &e.needs);
        let ok = entry.and_then(|e| e.quest_gate_ok.as_deref());
        report.add(what, gates, (book, rule, ok), (graph, &implied));
    }
    for &(from, to, (gflow, entry), step) in EXIT_GIVERS {
        let (from, to) = (vocab::node(from)?, vocab::node(to)?);
        let what = format!("exit {from} to {to}");
        let Some(exit) = book.exits.iter().find(|e| e.from == from && e.to == to) else {
            report.unread.push(what);
            continue;
        };
        let gates = graph.gates_of_write((gflow.get(), entry.get()), (gflow.get(), step));
        let ok = exit.quest_gate_ok.as_deref();
        report.add(what, gates, (book, &exit.expr, ok), (graph, &implied));
    }
    Ok(report)
}

impl Report {
    fn add(
        &mut self,
        what: String,
        gates: Option<Vec<Gate>>,
        (book, rule, ok): (&Book, &Expr, Option<&str>),
        (graph, implied): (&Graph, &[Implied]),
    ) {
        let Some(gates) = gates else {
            self.unread.push(what);
            return;
        };
        self.read += 1;
        let words = book.required(rule);
        let unmet: Vec<Gate> = (gates.into_iter())
            .filter(|g| !promised(g, &graph.steps_of(&g.quest), &words, implied))
            .collect();
        if unmet.is_empty() {
            return;
        }
        if ok.is_some() {
            self.excused += 1;
            return;
        }
        let rule = rule.to_string();
        (self.findings).extend(unmet.into_iter().map(|gate| Finding {
            what: what.clone(),
            gate,
            rule: rule.clone(),
        }));
    }
}

/// Whether one of `words` promises `gate`, quest `steps` in order.
pub fn promised(
    gate: &Gate,
    steps: &[String],
    words: &BTreeSet<String>,
    implied: &[Implied],
) -> bool {
    let Some(least) = gate.least(steps) else {
        return false;
    };
    (implied.iter())
        .filter(|i| i.quest == gate.quest && words.contains(&i.word))
        .any(|i| {
            steps
                .iter()
                .position(|s| *s == i.step)
                .is_some_and(|at| at >= least)
        })
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for x in &self.findings {
            let g = &x.gate;
            writeln!(f, "{}", x.what)?;
            writeln!(
                f,
                "  needs {} {} {}, by {}  #{}",
                g.quest, g.side, g.step, g.by, g.edge
            )?;
            writeln!(f, "  rule: {}", x.rule)?;
        }
        for what in &self.unread {
            writeln!(f, "unread: {what}")?;
        }
        let checks: BTreeSet<&str> = self.findings.iter().map(|x| x.what.as_str()).collect();
        writeln!(
            f,
            "{} findings on {} checks and exits, {} excused by quest-gate-ok, {} read, {} unread",
            self.findings.len(),
            checks.len(),
            self.excused,
            self.read,
            self.unread.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{EXIT_GIVERS, Implied, audit, implies, promised};
    use crate::flagmap::{Gate, Graph};
    use crate::known::scene;
    use crate::rules::lang::parse;
    use crate::rules::vocab::{self, EVENTS, IMPLIES};
    use crate::rules::{Book, Expr};

    /// The moat's two gifts and the Closed Door of Time: their gates, and
    /// what the rules named before #298.
    const BEFORE_298: &str = "beat:gohma and beat:king-dodongo and beat:barinade";

    fn met(graph: &Graph, book: &Book, gates: &[Gate], rule: &Expr) -> bool {
        let (words, implied) = (book.required(rule), implies().unwrap());
        (gates.iter()).all(|g| promised(g, &graph.steps_of(&g.quest), &words, &implied))
    }

    #[test]
    fn the_moat_gifts_and_the_closed_door_pass_on_the_moat_scene_and_failed_before_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (graph, book) = (Graph::build(&romfs).unwrap(), Book::load().unwrap());
        let mut old = parse(BEFORE_298).unwrap();
        vocab::words(&mut old).unwrap();
        for name in ["Zelda's gift", "Zelda's song"] {
            let (hash, entry) = book.checks().find(|(_, e)| e.name == name).unwrap();
            let check = (crate::gifts::checks().into_iter())
                .find(|c| c.hash == hash)
                .unwrap();
            let gift = crate::gifts::of(&check).unwrap();
            let gates = graph
                .gates_of_grant(gift.flow.get(), gift.item.get())
                .unwrap();
            assert!(!gates.is_empty(), "{name} waits on the quest");
            assert!(
                met(&graph, &book, &gates, &entry.needs),
                "{name}: {gates:?}"
            );
            assert!(!met(&graph, &book, &gates, &old), "{name} before #298");
        }
        let (from, to, (flow, entry), step) = EXIT_GIVERS[0];
        let (from, to) = (vocab::node(from).unwrap(), vocab::node(to).unwrap());
        let exit = book
            .exits
            .iter()
            .find(|e| e.from == from && e.to == to)
            .unwrap();
        let gates = graph
            .gates_of_write((flow.get(), entry.get()), (flow.get(), step))
            .unwrap();
        assert!(!gates.is_empty(), "the door waits on the quest");
        assert!(met(&graph, &book, &gates, &exit.expr), "{gates:?}");
        let old = format!("setting:door-of-time=open or ({BEFORE_298} and item:ocarina-of-time)");
        let mut old = parse(&old).unwrap();
        vocab::words(&mut old).unwrap();
        assert!(!met(&graph, &book, &gates, &old), "the door before #298");
    }

    /// Each `implies` row holds in the game: its event moves the quest to
    /// that step, or waits for it. A boss's row holds when Link's warp out
    /// of a boss room (the dungeon escape scene) moves the quest there.
    /// Which boss is the row's own claim: no data ties a boss to a quest.
    #[test]
    fn every_implies_row_is_what_its_event_does_to_the_quest() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let graph = Graph::build(&romfs).unwrap();
        for &(word, quest, step) in IMPLIES {
            let least = graph.steps_of(quest.get()).iter().position(|s| s == step);
            let least = least.unwrap_or_else(|| panic!("{quest} has no {step}"));
            let events = match word.split_once(':') {
                Some(("event", w)) => {
                    let (_, event) = EVENTS.iter().find(|e| e.0 == w).unwrap();
                    vec![event.get().to_owned()]
                }
                Some(("beat", _)) => graph.entries(scene::SY_BANC_CHANGE_ESCAPE_DUNGEON.get()),
                _ => panic!("{word}: only event and beat words can be checked yet"),
            };
            assert!(
                (events.iter()).any(|e| graph.promises(e, quest.get(), least)),
                "{word}: {quest} {step}"
            );
        }
    }

    /// CI's guard: every quest step gate is promised by its rule or
    /// excused by a reason.
    #[test]
    fn no_check_or_exit_waits_on_a_quest_step_its_rule_doesnt_promise() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (graph, book) = (Graph::build(&romfs).unwrap(), Book::load().unwrap());
        let report = audit(&romfs, &graph, &book).unwrap();
        assert!(
            report.findings.is_empty(),
            "zootr logic quest-gates:\n{report}"
        );
    }

    fn steps() -> Vec<String> {
        ["NotReady", "Ready", "Step01", "Step02", "Complete"]
            .map(String::from)
            .into()
    }

    fn sage(step: &str) -> Implied {
        let (word, quest) = ("Event_FakeSageMet".into(), "FakeSage".into());
        Implied {
            word,
            quest,
            step: step.into(),
        }
    }

    /// A made-up check waiting on a made-up quest's step is a finding with
    /// no word, and stops being one with a word that promises the step.
    #[test]
    fn a_step_gate_needs_a_word_that_promises_that_step_or_past_it() {
        let gate = Gate {
            quest: "FakeSage".into(),
            step: "Step01".into(),
            side: "after".into(),
            by: "event:FakeSage/Give".into(),
            edge: 0,
        };
        let words = BTreeSet::from(["Event_FakeSageMet".to_owned()]);
        assert!(!promised(
            &gate,
            &steps(),
            &BTreeSet::new(),
            &[sage("Step02")]
        ));
        assert!(promised(&gate, &steps(), &words, &[sage("Step02")]));
        assert!(
            !promised(&gate, &steps(), &words, &[sage("Step01")]),
            "after Step01 is Step02 on"
        );
        let before = Gate {
            side: "before|equal".into(),
            ..gate
        };
        assert!(!promised(&before, &steps(), &words, &[sage("Step02")]));
    }
}
