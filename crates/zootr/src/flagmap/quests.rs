//! Quest steps in the flag map. A quest's current step is saved as the
//! enum `Step_<Challenge>`, and the flow actions and graph nodes that name a
//! quest (`ChallengeName`) move it or check it. Those become `writes` and
//! `reads` edges on that enum, labelled with the step, so `writers` lists
//! what moves a quest and `readers` what waits on it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ounce_formats::murmur3;
use serde::Serialize;

use super::{Graph, Kind, Link};
use crate::rsdb::flow_name;
use crate::steps::SAVED;

/// Words in the actions and classes that move a quest. Every other one
/// naming a quest checks its step.
const MOVERS: [&str; 4] = [
    "SetStep",
    "ProgressChallengeStep",
    "ProgressNextStep",
    "RollbackChallengeStep",
];

/// The saved step enum of `quest`, named bare (`DoorOfTime`, as flows do)
/// or by its file (as graphs do).
pub fn state(quest: &str) -> String {
    format!("{SAVED}{}", flow_name(quest))
}

/// Whether an action or class of this name moves the quest it names.
pub(super) fn moves(class: &str) -> bool {
    MOVERS.iter().any(|m| class.contains(m))
}

/// An edge's label: the class, then the step it moves to or checks. A
/// graph's check also says which of its answers (`IsAfter`, `IsEqual`) the
/// graph reads.
pub(super) fn label(class: &str, step: Option<&str>, read: &[String]) -> String {
    let mut label = class.to_owned();
    if moves(class) {
        label.push_str(" to");
    } else if !read.is_empty() {
        label.push(' ');
        label.push_str(&read.join("|"));
    }
    if let Some(step) = step {
        label.push(' ');
        label.push_str(step);
    }
    label
}

/// The answers a step check can read, as a graph's node gives them.
const ANSWERS: [(&str, &str); 3] = [
    ("IsBefore", "before"),
    ("IsEqual", "equal"),
    ("IsAfter", "after"),
];

/// Every quest: its steps in order, what moves it to each and what checks
/// it against each. `zootr flagmap quests` prints it.
#[derive(Serialize)]
#[serde(transparent)]
pub struct Catalogue(pub Vec<Quest>);

#[derive(Serialize)]
pub struct Quest {
    pub name: String,
    /// The flag that starts the quest, when the quest table names one.
    pub starts: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Serialize)]
pub struct Step {
    pub name: String,
    /// The events and graphs that move the quest to this step.
    pub moved_by: Vec<Mover>,
    /// The events and graphs that check the quest against this step.
    pub checks: Vec<Check>,
}

#[derive(Serialize)]
pub struct Mover {
    /// The flag map node, `event:FLOW/ENTRY` or `graph:NAME`.
    pub by: String,
    pub edge: usize,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Check {
    /// The flag map node that branches on the step.
    pub by: String,
    /// Which side of the step it reads: `before`, `equal`, `after`. Empty
    /// for a flow query, whose cases branch on the step itself.
    pub answers: Vec<&'static str>,
    /// The BoolExp it reads the step through, when it does.
    pub via: Option<String>,
    pub edge: usize,
}

impl Graph {
    /// The catalogue of quests in `starts` (the quest table, see
    /// [`crate::steps::starts`]), or of the one named `only`.
    pub fn quests(
        &self,
        starts: &BTreeMap<String, Option<String>>,
        only: Option<&str>,
    ) -> Catalogue {
        let gd = self
            .gamedata
            .as_ref()
            .expect("the flag map keeps its GameData");
        let mut quests = Vec::new();
        for (name, start) in starts {
            if only.is_some_and(|o| !o.eq_ignore_ascii_case(name)) {
                continue;
            }
            let saved = state(name);
            let Some(&node) = self.ids.get(&format!("state:{saved}")) else {
                continue;
            };
            let mut steps: Vec<Step> = (gd.enum_values(&saved).unwrap_or_default())
                .into_iter()
                .map(|name| Step {
                    name,
                    moved_by: Vec::new(),
                    checks: Vec::new(),
                })
                .collect();
            for &e in &self.ins[node] {
                let edge = &self.edges[e];
                if edge.link == Link::Writes
                    && let Some(step) = named(&mut steps, last(&edge.label))
                {
                    let by = self.nodes[edge.from].id.clone();
                    step.moved_by.push(Mover { by, edge: e });
                }
            }
            for (step, check) in self.checks(node, &saved) {
                if let Some(step) = named(&mut steps, &step) {
                    step.checks.push(check);
                }
            }
            quests.push(Quest {
                name: name.clone(),
                starts: start.clone(),
                steps,
            });
        }
        Catalogue(quests)
    }

    /// Every check on quest step enum `node`, named `saved`, with the step
    /// it names: a node reading the enum, or reading a BoolExp built on it
    /// (through any BoolExps built on that).
    fn checks(&self, node: usize, saved: &str) -> BTreeSet<(String, Check)> {
        let gd = self
            .gamedata
            .as_ref()
            .expect("the flag map keeps its GameData");
        let mut out = BTreeSet::new();
        for &e in &self.outs[node] {
            let edge = &self.edges[e];
            let to = &self.nodes[edge.to];
            if edge.link != Link::Reads {
                continue;
            }
            if to.kind != Kind::State {
                let answers = (ANSWERS.iter())
                    .filter(|(word, _)| edge.label.contains(word))
                    .map(|&(_, a)| a)
                    .collect();
                let (by, via) = (to.id.clone(), None);
                let check = Check {
                    by,
                    answers,
                    via,
                    edge: e,
                };
                out.insert((last(&edge.label).to_owned(), check));
                continue;
            }
            let hash = match to.name.strip_prefix('#') {
                Some(hex) => u32::from_str_radix(hex, 16).unwrap_or_default(),
                None => murmur3(to.name.as_bytes()),
            };
            let Ok(Some(expr)) = gd.expr_of(hash) else {
                continue;
            };
            let compared = compares(&expr, saved);
            for reader in self.through_exps(edge.to) {
                for &(op, step) in &compared {
                    let check = Check {
                        by: self.nodes[reader].id.clone(),
                        answers: vec![op],
                        via: Some(to.name.clone()),
                        edge: e,
                    };
                    out.insert((step.to_owned(), check));
                }
            }
        }
        out
    }

    /// The nodes that read state `exp`, following reads through any
    /// BoolExps built on it.
    fn through_exps(&self, exp: usize) -> BTreeSet<usize> {
        let (mut seen, mut todo, mut out) = (BTreeSet::from([exp]), vec![exp], BTreeSet::new());
        while let Some(n) = todo.pop() {
            for &e in &self.outs[n] {
                let edge = &self.edges[e];
                if edge.link != Link::Reads {
                    continue;
                }
                match self.nodes[edge.to].kind {
                    Kind::State if seen.insert(edge.to) => todo.push(edge.to),
                    Kind::State => {}
                    _ => {
                        out.insert(edge.to);
                    }
                }
            }
        }
        out
    }
}

/// The comparisons of `saved` in a rendered BoolExp, as the answer each
/// reads and the step: `Step_X > Step04` is `after` Step04.
fn compares<'a>(expr: &'a str, saved: &str) -> Vec<(&'static str, &'a str)> {
    let words: Vec<&str> = expr.split_whitespace().collect();
    (words.windows(3))
        .filter(|w| w[0].trim_start_matches(['(', '!']) == saved)
        .filter_map(|w| {
            let op = match w[1] {
                ">" => "after",
                "<" => "before",
                "==" => "equal",
                _ => return None,
            };
            Some((op, w[2].trim_end_matches(')')))
        })
        .collect()
}

/// The step called `name`.
fn named<'a>(steps: &'a mut [Step], name: &str) -> Option<&'a mut Step> {
    steps.iter_mut().find(|s| s.name == name)
}

/// A label's last word: the step a quest edge names.
fn last(label: &str) -> &str {
    label.rsplit(' ').next().unwrap_or(label)
}

impl fmt::Display for Catalogue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for quest in &self.0 {
            write!(f, "{}", quest.name)?;
            if let Some(start) = &quest.starts {
                write!(f, "  (starts with {start})")?;
            }
            writeln!(f)?;
            for step in &quest.steps {
                writeln!(f, "  {}", step.name)?;
                for m in &step.moved_by {
                    writeln!(f, "    moved by {}  #{}", m.by, m.edge)?;
                }
                for c in &step.checks {
                    write!(f, "    checked by {}", c.by)?;
                    if !c.answers.is_empty() {
                        write!(f, "  {}", c.answers.join("|"))?;
                    }
                    if let Some(via) = &c.via {
                        write!(f, "  through {via}")?;
                    }
                    writeln!(f, "  #{}", c.edge)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{compares, label, moves, state};
    use crate::known::{field, flow};

    #[test]
    fn a_quest_named_bare_or_by_its_file_is_the_same_saved_step() {
        assert_eq!(state("FakeSage"), "Step_FakeSage");
        assert_eq!(
            state("Work/Challenge/FakeSage.game__challenge__Challenge.gyml"),
            "Step_FakeSage"
        );
    }

    /// The Door of Time quest's steps come in order, Step04 moved by the
    /// escape and checked by the door through its BoolExp.
    #[test]
    fn the_door_quest_lists_its_steps_in_order_and_who_waits_on_step04() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let g = super::Graph::build(&romfs).unwrap();
        let starts = crate::steps::starts(&romfs).unwrap();
        let door = &*flow::DOOR_OF_TIME;
        let quests = g.quests(&starts, Some(door)).0;
        let [quest] = &quests[..] else {
            panic!("{} quests", quests.len())
        };
        let names: Vec<&str> = quest.steps.iter().map(|s| s.name.as_str()).collect();
        let (ready, done) = (names.iter().position(|&s| s == "Ready"), names.last());
        assert_eq!(ready, Some(1), "{names:?}");
        assert_eq!(done, Some(&"Complete"), "{names:?}");
        let step04 = quest.steps.iter().find(|s| s.name == "Step04").unwrap();
        let escape = format!("event:{door}/{}", &**field::DOOR_OF_TIME_ESCAPE);
        assert!(step04.moved_by.iter().any(|m| m.by == escape));
        let open = format!("event:{door}/{}", &**field::DOOR_OF_TIME_OPEN_DOOR);
        let check = step04.checks.iter().find(|c| c.by == open).unwrap();
        assert_eq!(check.answers, ["after"]);
        assert!(check.via.is_some(), "the door reads it through a BoolExp");
    }

    #[test]
    fn a_boolexp_on_a_step_reads_the_side_its_operator_names() {
        let expr = "FakeFlag && Step_FakeSage > Step04 || !(Step_FakeSage == Ready)";
        assert_eq!(
            compares(expr, "Step_FakeSage"),
            [("after", "Step04"), ("equal", "Ready")]
        );
        assert!(compares(expr, "Step_FakeOther").is_empty());
    }

    #[test]
    fn setting_or_progressing_moves_a_quest_and_checking_doesnt() {
        assert!(moves("EventExecuteProgressChallengeStep"));
        assert!(moves("TriggerChallengeSetStepNoEventPause"));
        assert!(moves("OneShotChallengeProgressNextStep"));
        assert!(!moves("HoldChallengeCurrentStepCheckOrder"));
        assert_eq!(
            label("TriggerChallengeSetStep", Some("Step02"), &[]),
            "TriggerChallengeSetStep to Step02"
        );
        let read = ["IsAfter".to_owned()];
        assert_eq!(
            label("HoldChallengeCurrentStepCheckOrder", Some("Step04"), &read),
            "HoldChallengeCurrentStepCheckOrder IsAfter Step04"
        );
    }
}
