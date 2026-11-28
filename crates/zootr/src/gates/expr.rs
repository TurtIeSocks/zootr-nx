//! What a gate reads, as the graphs wire it ([`Expr`]), and what of it a
//! GameData program or the hand can say: its needs, its foes, what opens
//! it, the age it opens for.

use std::fmt;

/// One thing a gate reads.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Atom {
    /// A placed actor's output, like a Stinger's `Dead`.
    Actor {
        name: String,
        output: String,
        /// The placement, when the graph names one.
        hash: Option<u64>,
    },
    Child,
    Adult,
    /// A flag no graph sets.
    Flag(String),
    /// A GameData enum holding a value, like the prize of Dampe's race,
    /// set to the Hookshot's appearance.
    Enum {
        name: String,
        value: String,
    },
    /// A GameData enum past a value: a quest past a step, like the Water
    /// Temple's heart container waiting for the Water Sage's quest past
    /// `Step07_2`. The walk doesn't know the steps' order, GameData does.
    After {
        name: String,
        value: String,
    },
    /// A BoolExp, by name, which only GameData renders.
    Exp(String),
    /// Something the walk doesn't read, described in one word.
    Todo(String),
}

impl Atom {
    /// The enemy placement this reads, like a Gibdo's `Dead`.
    pub fn foe(&self) -> Option<u64> {
        match self {
            Atom::Actor {
                name,
                hash: Some(h),
                ..
            } if name.starts_with(crate::enemies::ENEMY) => Some(*h),
            _ => None,
        }
    }
}

/// A gate as the graphs wire it.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Const(bool),
    Atom(Atom),
    /// A flag the graphs set, with what sets it.
    SetFlag(String, Vec<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
}

/// A value a GameData program reads on its own: a Bool, a BoolExp, or a
/// Bool read as false.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Readable {
    Bool(String),
    Exp(String),
    Not(String),
}

impl Expr {
    /// Every atom the gate reads, however its rule folds them.
    pub fn atoms(&self) -> Vec<&Atom> {
        match self {
            Expr::Const(_) => Vec::new(),
            Expr::Atom(a) => vec![a],
            Expr::Not(e) => e.atoms(),
            Expr::SetFlag(_, v) | Expr::And(v) | Expr::Or(v) => {
                v.iter().flat_map(Expr::atoms).collect()
            }
        }
    }

    /// What the gate needs that a GameData program can say on its own: the
    /// Bools and BoolExps every branch of its AND must hold. `None` where the
    /// gate branches (`Or`), where a term on one branch isn't needed at all,
    /// or where it denies a value a program can't name. A term a program
    /// can't name (an actor's own `Dead`, an enum, a sensor) is left out, so
    /// this is part of what the gate needs, never the whole of it: enough to
    /// keep a copy standing only while the gate could hold at all.
    pub fn needs(&self) -> Option<Vec<Readable>> {
        match self {
            Expr::Const(_) => Some(Vec::new()),
            Expr::Atom(Atom::Flag(flag)) => Some(vec![Readable::Bool(flag.clone())]),
            Expr::Atom(Atom::Exp(exp)) => Some(vec![Readable::Exp(exp.clone())]),
            Expr::Atom(_) => None,
            Expr::SetFlag(flag, _) => Some(vec![Readable::Bool(flag.clone())]),
            Expr::Not(e) => match e.lone() {
                Expr::Atom(Atom::Flag(flag)) | Expr::SetFlag(flag, _) => {
                    Some(vec![Readable::Not(flag.clone())])
                }
                _ => None,
            },
            Expr::Or(_) => None,
            Expr::And(terms) => {
                let mut out: Vec<Readable> = (terms.iter())
                    .flat_map(|t| t.needs().unwrap_or_default())
                    .collect();
                out.sort_unstable();
                out.dedup();
                Some(out)
            }
        }
    }

    /// The one term an AND or OR of one term holds, else itself: an
    /// `OperateAND` read on its `False` output is a NOT of its input.
    fn lone(&self) -> &Expr {
        match self {
            Expr::And(v) | Expr::Or(v) if v.len() == 1 => v[0].lone(),
            e => e,
        }
    }

    /// The enemy placements this waits on, when beating them is all it
    /// waits on: a kill alone sets its latch.
    pub fn foes(&self) -> Vec<u64> {
        let atoms = self.atoms();
        let mut foes: Vec<u64> = atoms.iter().filter_map(|a| a.foe()).collect();
        if foes.len() != atoms.len() {
            return Vec::new();
        }
        foes.sort_unstable();
        foes.dedup();
        foes
    }

    /// The GameData values that open the gate when set by hand, as a
    /// sweep's ready entries (`NAME` sets a Bool true, `NAME=VALUE` an
    /// enum): the flags a graph sets once its condition is met, like a
    /// mini-boss beaten, the flags no graph sets, and the enum values it
    /// asks. A value under a `Not` closes it, so it stays out. An enum
    /// takes the first value asked, the way the gate's first branch reads.
    pub fn opening(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for entry in self.openers() {
            let name = entry.split('=').next();
            if !found.iter().any(|f| f.split('=').next() == name) {
                found.push(entry);
            }
        }
        found
    }

    /// The age the gate opens for, `Some(true)` for a child, when only
    /// one can: what the hand sets opens, and only the age atoms hold.
    pub fn age(&self) -> Option<bool> {
        match (self.opens(true), self.opens(false)) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    }

    fn opens(&self, child: bool) -> bool {
        match self {
            Expr::Const(b) => *b,
            Expr::Atom(Atom::Child) => child,
            Expr::Atom(Atom::Adult) => !child,
            Expr::Not(e) => match **e {
                Expr::Atom(Atom::Child) => !child,
                Expr::Atom(Atom::Adult) => child,
                _ => true,
            },
            Expr::Atom(_) | Expr::SetFlag(..) => true,
            Expr::And(v) => v.iter().all(|e| e.opens(child)),
            Expr::Or(v) => v.iter().any(|e| e.opens(child)),
        }
    }

    fn openers(&self) -> Vec<String> {
        match self {
            Expr::SetFlag(name, _) | Expr::Atom(Atom::Flag(name)) => vec![name.clone()],
            Expr::Atom(Atom::Enum { name, value }) => vec![format!("{name}={value}")],
            Expr::And(v) | Expr::Or(v) => v.iter().flat_map(Expr::openers).collect(),
            Expr::Const(_) | Expr::Atom(_) | Expr::Not(_) => Vec::new(),
        }
    }

    /// The quest steps and BoolExps the gate opens on, outside a `Not`:
    /// each [`Atom::After`] and [`Atom::Exp`], for GameData to resolve.
    pub fn quests(&self) -> Vec<&Atom> {
        match self {
            Expr::Atom(a @ (Atom::After { .. } | Atom::Exp(_))) => vec![a],
            Expr::SetFlag(_, v) | Expr::And(v) | Expr::Or(v) => {
                v.iter().flat_map(Expr::quests).collect()
            }
            Expr::Const(_) | Expr::Atom(_) | Expr::Not(_) => Vec::new(),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |f: &mut fmt::Formatter<'_>, v: &[Expr], sep: &str| {
            let shown: Vec<_> = v.iter().map(Expr::to_string).collect();
            write!(f, "({})", shown.join(sep))
        };
        match self {
            Expr::Const(b) => write!(f, "{b}"),
            Expr::Atom(Atom::Actor { name, output, .. }) => write!(f, "{name}.{output}"),
            Expr::Atom(Atom::Child) => f.write_str("Childhood"),
            Expr::Atom(Atom::Adult) => f.write_str("Adulthood"),
            Expr::Atom(Atom::Flag(flag)) => write!(f, "flag({flag})"),
            Expr::Atom(Atom::Enum { name, value }) => write!(f, "{name}=={value}"),
            Expr::Atom(Atom::After { name, value }) => write!(f, "{name}>{value}"),
            Expr::Atom(Atom::Exp(exp)) => write!(f, "exp({exp})"),
            Expr::Atom(Atom::Todo(t)) => write!(f, "?{t}"),
            Expr::SetFlag(flag, drives) => {
                write!(f, "{flag}<=")?;
                list(f, drives, " | ")
            }
            Expr::And(v) => list(f, v, " & "),
            Expr::Or(v) => list(f, v, " | "),
            Expr::Not(e) => write!(f, "!{e}"),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::gates::rule::rule;
    use crate::known::field;

    /// A flag the graph sets, read false through an AND of one term (an
    /// `OperateAND`'s `False`), is a program; an actor's `Dead` beside it
    /// isn't, so it's left out.
    #[test]
    fn a_set_flag_read_false_through_a_lone_and_is_readable() {
        let jar = Expr::Atom(Atom::Actor {
            name: "FakeJar".into(),
            output: "Dead".into(),
            hash: None,
        });
        let got = Expr::SetFlag("FakeRoom_GetKey".into(), vec![Expr::Const(false)]);
        let gate = Expr::And(vec![jar, Expr::Not(Box::new(Expr::And(vec![got])))]);
        assert_eq!(
            gate.needs(),
            Some(vec![Readable::Not("FakeRoom_GetKey".into())])
        );
    }

    #[test]
    fn a_gate_waits_on_foes_only_when_kills_are_all_it_reads() {
        let foe = |hash| {
            Expr::Atom(Atom::Actor {
                name: "Enemy_FakeBat".into(),
                output: "Dead".into(),
                hash: Some(hash),
            })
        };
        let both = Expr::And(vec![foe(2), foe(1), foe(2)]);
        assert_eq!(both.foes(), [1, 2]);
        let switched = Expr::And(vec![foe(1), actor("Obj_FakeSwitch", "On")]);
        assert!(switched.foes().is_empty());
        assert!(Expr::Const(true).foes().is_empty());
    }

    pub(in crate::gates) fn actor(name: &str, output: &str) -> Expr {
        Expr::Atom(Atom::Actor {
            name: name.into(),
            output: output.into(),
            hash: None,
        })
    }

    pub(in crate::gates) fn flag(name: &str) -> Expr {
        Expr::Atom(Atom::Flag(name.into()))
    }

    pub(in crate::gates) fn held(name: &str, value: &str) -> Expr {
        Expr::Atom(Atom::Enum {
            name: name.into(),
            value: value.into(),
        })
    }

    #[test]
    fn a_gate_opens_on_its_flags_and_first_enum_values_never_under_a_not() {
        let gate = Expr::And(vec![
            Expr::Or(vec![
                Expr::And(vec![held("Prize", "Appear"), flag("Shown")]),
                held("Prize", "Opened"),
                Expr::SetFlag("Beaten".into(), vec![Expr::Const(true)]),
                Expr::Not(Box::new(flag("Gone"))),
            ]),
            Expr::Not(Box::new(held("Other", "Off"))),
            Expr::Atom(Atom::Adult),
        ]);
        assert_eq!(gate.opening(), ["Prize=Appear", "Shown", "Beaten"]);
        assert_eq!(rule(&held("Prize", "Appear")).unwrap(), "enum:Prize=Appear");
    }

    #[test]
    fn a_gate_names_its_age_only_when_every_branch_needs_it() {
        let adult = Expr::Atom(Atom::Adult);
        let colossus = Expr::Or(vec![
            Expr::And(vec![actor("Switch", field::FIRST_ON), adult.clone()]),
            Expr::And(vec![adult.clone(), Expr::SetFlag("Latch".into(), vec![])]),
        ]);
        assert_eq!(colossus.age(), Some(false));
        let either = Expr::Or(vec![adult, flag("Shown")]);
        assert_eq!(either.age(), None);
        let grown = Expr::Not(Box::new(Expr::Atom(Atom::Child)));
        assert_eq!(Expr::And(vec![grown, flag("Shown")]).age(), Some(false));
        assert_eq!(Expr::Atom(Atom::Child).age(), Some(true));
    }
}
